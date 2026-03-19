use necromancer::{
    protocol::structs::{TallyFlags, TransitionType, VideoSource},
    AtemController, AtemState,
};
use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddrV4},
    sync::Arc,
};
use thiserror::Error;
use tokio::sync::watch;
use tokio::{sync::broadcast, task::JoinHandle};
use tracing::{debug, info};

#[derive(Debug, Error)]
pub enum ClientError {
    #[error(transparent)]
    Necromancer(#[from] necromancer::Error),

    #[error("invalid ip address: {0}")]
    InvalidIp(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Connected,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct AtemSnapshot {
    pub initialisation_complete: bool,
    pub mes_count: u8,
    pub aux_count: u8,

    pub program_sources: Vec<VideoSource>,
    pub preview_sources: Vec<VideoSource>,

    /// Inputs / sources that we can present in the UI for selection.
    pub available_sources: Vec<VideoSource>,

    /// Source -> tally info.
    pub tally_by_source: HashMap<VideoSource, TallyFlags>,

    /// Available DSK keys for downstream keyers.
    ///
    /// This allows the UI to offer DSK controls only when supported.
    pub dsk_keys: Vec<u8>,
}

impl AtemSnapshot {
    fn from_state(state: &AtemState) -> Self {
        let mes_count = state.topology.mes;
        let aux_count = state.topology.auxs;

        let program_sources = (0..mes_count)
            .map(|me| state.get_program_source(me).unwrap_or_default())
            .collect();
        let preview_sources = (0..mes_count)
            .map(|me| state.get_preview_source(me).unwrap_or_default())
            .collect();

        let available_sources = state.input_properties.keys().copied().collect::<Vec<_>>();

        let tally_by_source = state.tally_by_source.clone();
        let dsk_keys = state.dsk_sources.keys().copied().collect::<Vec<_>>();

        Self {
            initialisation_complete: state.initialisation_complete,
            mes_count,
            aux_count,
            program_sources,
            preview_sources,
            available_sources,
            tally_by_source,
            dsk_keys,
        }
    }
}

#[derive(Clone)]
pub struct AtemClientHandle {
    controller: Arc<AtemController>,
}

pub struct AtemConnection {
    pub client: AtemClientHandle,
    pub snapshot_rx: watch::Receiver<AtemSnapshot>,
    pub status_rx: watch::Receiver<ConnectionStatus>,
    bg_task: JoinHandle<()>,
}

impl AtemConnection {
    pub fn disconnect(self) {
        self.bg_task.abort();
        // Dropping `self` will also drop the underlying controller once all clones are gone.
    }
}

impl AtemClientHandle {
    pub async fn set_program_input(
        &self,
        me: u8,
        video_source: VideoSource,
    ) -> Result<(), ClientError> {
        self.controller.set_program_input(me, video_source).await?;
        Ok(())
    }

    pub async fn set_preview_input(
        &self,
        me: u8,
        video_source: VideoSource,
    ) -> Result<(), ClientError> {
        self.controller.set_preview_input(me, video_source).await?;
        Ok(())
    }

    pub async fn cut(&self, me: u8) -> Result<(), ClientError> {
        self.controller.cut(me).await?;
        Ok(())
    }

    pub async fn auto(&self, me: u8) -> Result<(), ClientError> {
        self.controller.auto(me).await?;
        Ok(())
    }

    pub async fn set_next_transition(
        &self,
        me: u8,
        next_transition: TransitionType,
    ) -> Result<(), ClientError> {
        self.controller
            .set_next_transition(me, next_transition)
            .await?;
        Ok(())
    }

    // ---- Phase 2 additions (AUX / DSK) ----
    pub async fn set_aux_source(
        &self,
        aux_bus: u8,
        video_source: VideoSource,
    ) -> Result<(), ClientError> {
        self.controller
            .set_aux_source(aux_bus, video_source)
            .await?;
        Ok(())
    }

    pub async fn dsk_auto(&self, key: u8) -> Result<(), ClientError> {
        self.controller.dsk_auto(key).await?;
        Ok(())
    }

    pub async fn set_dsk_on_air(&self, key: u8, on_air: bool) -> Result<(), ClientError> {
        self.controller.set_dsk_on_air(key, on_air).await?;
        Ok(())
    }

    pub async fn set_dsk_tie(&self, key: u8, tie: bool) -> Result<(), ClientError> {
        self.controller.set_dsk_tie(key, tie).await?;
        Ok(())
    }

    pub async fn set_dsk_cut_source(
        &self,
        key: u8,
        source: VideoSource,
    ) -> Result<(), ClientError> {
        self.controller.set_dsk_cut_source(key, source).await?;
        Ok(())
    }

    pub async fn set_dsk_fill_source(
        &self,
        key: u8,
        source: VideoSource,
    ) -> Result<(), ClientError> {
        self.controller.set_dsk_fill_source(key, source).await?;
        Ok(())
    }

    pub async fn set_dsk_rate(&self, key: u8, rate: u8) -> Result<(), ClientError> {
        self.controller.set_dsk_rate(key, rate).await?;
        Ok(())
    }
}

pub async fn connect_udp(
    ip: &str,
    port: u16,
    reconnect: bool,
) -> Result<AtemConnection, ClientError> {
    let ip: Ipv4Addr = ip
        .parse()
        .map_err(|_| ClientError::InvalidIp(ip.to_string()))?;
    let addr = SocketAddrV4::new(ip, port);
    connect_socketaddr(addr, reconnect).await
}

async fn connect_socketaddr(
    addr: SocketAddrV4,
    reconnect: bool,
) -> Result<AtemConnection, ClientError> {
    info!("Connecting to ATEM over UDP: {addr}");
    let controller = AtemController::connect_udp(addr, reconnect).await?;
    let controller = Arc::new(controller);

    let initial_state = controller.get_state().await;
    let initial_snapshot = AtemSnapshot::from_state(&initial_state);

    let (snapshot_tx, snapshot_rx) = watch::channel(initial_snapshot);
    let (status_tx, status_rx) = watch::channel(ConnectionStatus::Connected);

    let mut state_updates = controller.state_update_events();
    let controller_bg = controller.clone();
    let bg_task = tokio::spawn(async move {
        loop {
            match state_updates.recv().await {
                Ok((_txn, _update)) => {
                    let state = controller_bg.get_state().await;
                    let snapshot = AtemSnapshot::from_state(&state);
                    let _ = snapshot_tx.send(snapshot);
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    debug!("ATEM state update lagged; continuing");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }

        let _ = status_tx.send(ConnectionStatus::Disconnected);
    });

    Ok(AtemConnection {
        client: AtemClientHandle { controller },
        snapshot_rx,
        status_rx,
        bg_task,
    })
}

// Note: `connect_udp` is re-exported from `lib.rs`.
