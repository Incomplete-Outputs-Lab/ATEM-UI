use necromancer::{
    protocol::structs::{TallyFlags, TransitionType, VideoSource},
    AtemController, AtemState,
};
use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddrV4},
    sync::{Arc, OnceLock},
};
use thiserror::Error;
use tokio::sync::watch;
use tokio::{
    runtime::{Builder, Runtime},
    sync::broadcast,
    task::JoinHandle,
};
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
    /// DSK source assignments: key -> (fill, cut)
    pub dsk_sources: HashMap<u8, (VideoSource, VideoSource)>,
    /// DSK runtime state flags by key.
    pub dsk_state: HashMap<u8, DskRuntimeSnapshot>,
    /// DSK configuration properties by key.
    pub dsk_properties: HashMap<u8, DskPropertiesSnapshot>,
    /// Transition position percent-like value (0..=10000) by ME.
    pub transition_position: Vec<u16>,
    /// Whether transition is currently moving by ME.
    pub transition_in_progress: Vec<bool>,
    /// Fade-to-black fully black status by ME.
    pub ftb_fully_black: Vec<bool>,
    /// Fade-to-black in-transition status by ME.
    pub ftb_in_transition: Vec<bool>,
    /// Fade-to-black frames remaining by ME.
    pub ftb_frames_remaining: Vec<u8>,
    /// Fade-to-black rate by ME.
    pub ftb_rate: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub struct DskPropertiesSnapshot {
    pub tie: bool,
    pub rate: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct DskRuntimeSnapshot {
    pub on_air: bool,
    pub in_transition: bool,
    pub remaining_frames: u8,
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
        let dsk_sources = state.dsk_sources.clone();
        let dsk_state = state
            .dsk_state
            .iter()
            .map(|(k, v)| {
                (
                    *k,
                    DskRuntimeSnapshot {
                        on_air: v.on_air,
                        in_transition: v.in_transition,
                        remaining_frames: v.remaining_frames,
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let dsk_properties = state
            .dsk_properties
            .iter()
            .map(|(k, v)| {
                (
                    *k,
                    DskPropertiesSnapshot {
                        tie: v.tie,
                        rate: v.rate,
                    },
                )
            })
            .collect::<HashMap<_, _>>();

        let transition_position = (0..mes_count)
            .map(|me| {
                state
                    .transition_position
                    .get(&me)
                    .map(|p| p.position)
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        let transition_in_progress = (0..mes_count)
            .map(|me| {
                state
                    .transition_position
                    .get(&me)
                    .map(|p| p.in_progress)
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();

        let mut ftb_fully_black = Vec::with_capacity(mes_count as usize);
        let mut ftb_in_transition = Vec::with_capacity(mes_count as usize);
        let mut ftb_frames_remaining = Vec::with_capacity(mes_count as usize);
        let mut ftb_rate = Vec::with_capacity(mes_count as usize);
        for me in 0..mes_count {
            let status = state.get_fade_to_black_status(me).unwrap_or_default();
            ftb_fully_black.push(status.fully_black);
            ftb_in_transition.push(status.in_transition);
            ftb_frames_remaining.push(status.frames_remaining);
            ftb_rate.push(state.get_fade_to_black_rate(me).unwrap_or_default());
        }

        Self {
            initialisation_complete: state.initialisation_complete,
            mes_count,
            aux_count,
            program_sources,
            preview_sources,
            available_sources,
            tally_by_source,
            dsk_keys,
            dsk_sources,
            dsk_state,
            dsk_properties,
            transition_position,
            transition_in_progress,
            ftb_fully_black,
            ftb_in_transition,
            ftb_frames_remaining,
            ftb_rate,
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
    pub fn set_program_input(&self, me: u8, video_source: VideoSource) -> Result<(), ClientError> {
        tokio_runtime().block_on(self.controller.set_program_input(me, video_source))?;
        Ok(())
    }

    pub fn set_preview_input(&self, me: u8, video_source: VideoSource) -> Result<(), ClientError> {
        tokio_runtime().block_on(self.controller.set_preview_input(me, video_source))?;
        Ok(())
    }

    pub fn cut(&self, me: u8) -> Result<(), ClientError> {
        tokio_runtime().block_on(self.controller.cut(me))?;
        Ok(())
    }

    pub fn auto(&self, me: u8) -> Result<(), ClientError> {
        tokio_runtime().block_on(self.controller.auto(me))?;
        Ok(())
    }

    pub fn set_next_transition(
        &self,
        me: u8,
        next_transition: TransitionType,
    ) -> Result<(), ClientError> {
        tokio_runtime().block_on(self.controller.set_next_transition(me, next_transition))?;
        Ok(())
    }

    // ---- Phase 2 additions (AUX / DSK) ----
    pub fn set_aux_source(
        &self,
        aux_bus: u8,
        video_source: VideoSource,
    ) -> Result<(), ClientError> {
        tokio_runtime().block_on(self.controller.set_aux_source(aux_bus, video_source))?;
        Ok(())
    }

    pub fn dsk_auto(&self, key: u8) -> Result<(), ClientError> {
        tokio_runtime().block_on(self.controller.dsk_auto(key))?;
        Ok(())
    }

    pub fn set_dsk_on_air(&self, key: u8, on_air: bool) -> Result<(), ClientError> {
        tokio_runtime().block_on(self.controller.set_dsk_on_air(key, on_air))?;
        Ok(())
    }

    pub fn set_dsk_tie(&self, key: u8, tie: bool) -> Result<(), ClientError> {
        tokio_runtime().block_on(self.controller.set_dsk_tie(key, tie))?;
        Ok(())
    }

    pub fn set_dsk_cut_source(&self, key: u8, source: VideoSource) -> Result<(), ClientError> {
        tokio_runtime().block_on(self.controller.set_dsk_cut_source(key, source))?;
        Ok(())
    }

    pub fn set_dsk_fill_source(&self, key: u8, source: VideoSource) -> Result<(), ClientError> {
        tokio_runtime().block_on(self.controller.set_dsk_fill_source(key, source))?;
        Ok(())
    }

    pub fn set_dsk_rate(&self, key: u8, rate: u8) -> Result<(), ClientError> {
        tokio_runtime().block_on(self.controller.set_dsk_rate(key, rate))?;
        Ok(())
    }

    pub fn cut_black(&self, me: u8, black: bool) -> Result<(), ClientError> {
        tokio_runtime().block_on(self.controller.cut_black(me, black))?;
        Ok(())
    }

    pub fn toggle_auto_black(&self, me: u8) -> Result<(), ClientError> {
        tokio_runtime().block_on(self.controller.toggle_auto_black(me))?;
        Ok(())
    }
}

pub fn connect_udp(ip: &str, port: u16, reconnect: bool) -> Result<AtemConnection, ClientError> {
    let ip: Ipv4Addr = ip
        .parse()
        .map_err(|_| ClientError::InvalidIp(ip.to_string()))?;
    let addr = SocketAddrV4::new(ip, port);
    tokio_runtime().block_on(connect_socketaddr(addr, reconnect))
}

fn tokio_runtime() -> &'static Runtime {
    static TOKIO_RUNTIME: OnceLock<Runtime> = OnceLock::new();
    TOKIO_RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("failed to create internal tokio runtime for atem-core")
    })
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
    let bg_task = tokio_runtime().spawn(async move {
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
