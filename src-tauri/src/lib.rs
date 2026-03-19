use atem_core::{
    connect_udp, AtemConnection, AtemSnapshot, ConnectionStatus, TransitionType, VideoSource,
};
use serde::Serialize;
use std::collections::HashMap;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

#[derive(Default)]
struct AppState {
    inner: Mutex<InnerState>,
}

#[derive(Default)]
struct InnerState {
    connection: Option<ManagedConnection>,
}

struct ManagedConnection {
    connection: AtemConnection,
    event_task: tauri::async_runtime::JoinHandle<()>,
}

#[derive(Debug, Clone, Serialize)]
struct DskPropertiesDto {
    tie: bool,
    rate: u8,
}

#[derive(Debug, Clone, Serialize)]
struct DskRuntimeDto {
    on_air: bool,
    in_transition: bool,
    remaining_frames: u8,
}

#[derive(Debug, Clone, Serialize)]
struct SourceItemDto {
    source_index: usize,
    source_id: u16,
    source_name: String,
}

#[derive(Debug, Clone, Serialize)]
struct AtemSnapshotDto {
    initialisation_complete: bool,
    mes_count: u8,
    aux_count: u8,
    program_sources: Vec<String>,
    preview_sources: Vec<String>,
    available_sources: Vec<String>,
    program_source_ids: Vec<u16>,
    preview_source_ids: Vec<u16>,
    available_source_items: Vec<SourceItemDto>,
    tally_by_source: HashMap<String, Vec<String>>,
    dsk_keys: Vec<u8>,
    dsk_sources: HashMap<String, (String, String)>,
    dsk_state: HashMap<u8, DskRuntimeDto>,
    dsk_properties: HashMap<u8, DskPropertiesDto>,
    transition_position: Vec<u16>,
    transition_in_progress: Vec<bool>,
    ftb_fully_black: Vec<bool>,
    ftb_in_transition: Vec<bool>,
    ftb_frames_remaining: Vec<u8>,
    ftb_rate: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
struct SnapshotEventDto {
    snapshot: AtemSnapshotDto,
    status: ConnectionStatusDto,
}

#[derive(Debug, Clone, Serialize)]
enum ConnectionStatusDto {
    Disconnected,
    Connected,
    Error(String),
}

fn stringify_source(source: &VideoSource) -> String {
    format!("{source:?}")
}

fn source_id(source: &VideoSource) -> u16 {
    *source as u16
}

fn snapshot_to_dto(snapshot: &AtemSnapshot) -> AtemSnapshotDto {
    let tally_by_source = snapshot
        .tally_by_source
        .iter()
        .map(|(source, flags)| {
            let mut names = Vec::new();
            if flags.program() {
                names.push("Program".to_string());
            }
            if flags.preview() {
                names.push("Preview".to_string());
            }
            (stringify_source(source), names)
        })
        .collect::<HashMap<_, _>>();

    let dsk_sources = snapshot
        .dsk_sources
        .iter()
        .map(|(k, (fill, cut))| (k.to_string(), (stringify_source(fill), stringify_source(cut))))
        .collect::<HashMap<_, _>>();

    let dsk_state = snapshot
        .dsk_state
        .iter()
        .map(|(k, v)| {
            (
                *k,
                DskRuntimeDto {
                    on_air: v.on_air,
                    in_transition: v.in_transition,
                    remaining_frames: v.remaining_frames,
                },
            )
        })
        .collect::<HashMap<_, _>>();

    let dsk_properties = snapshot
        .dsk_properties
        .iter()
        .map(|(k, v)| {
            (
                *k,
                DskPropertiesDto {
                    tie: v.tie,
                    rate: v.rate,
                },
            )
        })
        .collect::<HashMap<_, _>>();

    AtemSnapshotDto {
        initialisation_complete: snapshot.initialisation_complete,
        mes_count: snapshot.mes_count,
        aux_count: snapshot.aux_count,
        program_sources: snapshot.program_sources.iter().map(stringify_source).collect(),
        preview_sources: snapshot.preview_sources.iter().map(stringify_source).collect(),
        available_sources: snapshot
            .available_sources
            .iter()
            .map(stringify_source)
            .collect(),
        program_source_ids: snapshot.program_sources.iter().map(source_id).collect(),
        preview_source_ids: snapshot.preview_sources.iter().map(source_id).collect(),
        available_source_items: snapshot
            .available_sources
            .iter()
            .enumerate()
            .map(|(source_index, source)| SourceItemDto {
                source_index,
                source_id: source_id(source),
                source_name: stringify_source(source),
            })
            .collect(),
        tally_by_source,
        dsk_keys: snapshot.dsk_keys.clone(),
        dsk_sources,
        dsk_state,
        dsk_properties,
        transition_position: snapshot.transition_position.clone(),
        transition_in_progress: snapshot.transition_in_progress.clone(),
        ftb_fully_black: snapshot.ftb_fully_black.clone(),
        ftb_in_transition: snapshot.ftb_in_transition.clone(),
        ftb_frames_remaining: snapshot.ftb_frames_remaining.clone(),
        ftb_rate: snapshot.ftb_rate.clone(),
    }
}

fn status_to_dto(status: &ConnectionStatus) -> ConnectionStatusDto {
    match status {
        ConnectionStatus::Disconnected => ConnectionStatusDto::Disconnected,
        ConnectionStatus::Connected => ConnectionStatusDto::Connected,
        ConnectionStatus::Error(e) => ConnectionStatusDto::Error(e.clone()),
    }
}

fn empty_snapshot_dto() -> AtemSnapshotDto {
    AtemSnapshotDto {
        initialisation_complete: false,
        mes_count: 1,
        aux_count: 0,
        program_sources: Vec::new(),
        preview_sources: Vec::new(),
        available_sources: Vec::new(),
        program_source_ids: Vec::new(),
        preview_source_ids: Vec::new(),
        available_source_items: Vec::new(),
        tally_by_source: HashMap::new(),
        dsk_keys: Vec::new(),
        dsk_sources: HashMap::new(),
        dsk_state: HashMap::new(),
        dsk_properties: HashMap::new(),
        transition_position: Vec::new(),
        transition_in_progress: Vec::new(),
        ftb_fully_black: Vec::new(),
        ftb_in_transition: Vec::new(),
        ftb_frames_remaining: Vec::new(),
        ftb_rate: Vec::new(),
    }
}

async fn emit_snapshot_event(app: &AppHandle, connection: &AtemConnection) {
    let snapshot = connection.snapshot_rx.borrow().clone();
    let status = connection.status_rx.borrow().clone();
    let payload = SnapshotEventDto {
        snapshot: snapshot_to_dto(&snapshot),
        status: status_to_dto(&status),
    };
    let _ = app.emit("atem://snapshot", payload);
}

fn spawn_event_pump(app: AppHandle, connection: &AtemConnection) -> tauri::async_runtime::JoinHandle<()> {
    let mut snapshot_rx = connection.snapshot_rx.clone();
    let mut status_rx = connection.status_rx.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::select! {
                snapshot_result = snapshot_rx.changed() => {
                    if snapshot_result.is_err() {
                        break;
                    }
                }
                status_result = status_rx.changed() => {
                    if status_result.is_err() {
                        break;
                    }
                }
            }

            let payload = SnapshotEventDto {
                snapshot: snapshot_to_dto(&snapshot_rx.borrow().clone()),
                status: status_to_dto(&status_rx.borrow().clone()),
            };
            let _ = app.emit("atem://snapshot", payload);
        }
    })
}

fn source_by_index(connection: &AtemConnection, source_index: usize) -> Result<VideoSource, String> {
    connection
        .snapshot_rx
        .borrow()
        .available_sources
        .get(source_index)
        .copied()
        .ok_or_else(|| format!("invalid source index: {source_index}"))
}

#[tauri::command]
async fn connect(
    app: AppHandle,
    state: State<'_, AppState>,
    ip: String,
    port: u16,
    reconnect: bool,
) -> Result<(), String> {
    let connection = connect_udp(&ip, port, reconnect)
        .await
        .map_err(|e| e.to_string())?;
    let event_task = spawn_event_pump(app.clone(), &connection);
    emit_snapshot_event(&app, &connection).await;

    let mut inner = state.inner.lock().await;
    if let Some(existing) = inner.connection.take() {
        existing.event_task.abort();
        existing.connection.disconnect();
    }
    inner.connection = Some(ManagedConnection {
        connection,
        event_task,
    });
    Ok(())
}

#[tauri::command]
async fn disconnect(state: State<'_, AppState>) -> Result<(), String> {
    let mut inner = state.inner.lock().await;
    if let Some(existing) = inner.connection.take() {
        existing.event_task.abort();
        existing.connection.disconnect();
    }
    Ok(())
}

#[tauri::command]
async fn get_snapshot(state: State<'_, AppState>) -> Result<AtemSnapshotDto, String> {
    let inner = state.inner.lock().await;
    Ok(inner
        .connection
        .as_ref()
        .map(|conn| snapshot_to_dto(&conn.connection.snapshot_rx.borrow().clone()))
        .unwrap_or_else(empty_snapshot_dto))
}

#[tauri::command]
async fn get_connection_status(state: State<'_, AppState>) -> Result<ConnectionStatusDto, String> {
    let inner = state.inner.lock().await;
    Ok(inner
        .connection
        .as_ref()
        .map(|conn| status_to_dto(&conn.connection.status_rx.borrow().clone()))
        .unwrap_or(ConnectionStatusDto::Disconnected))
}

#[tauri::command]
async fn set_program_input_by_index(
    state: State<'_, AppState>,
    me: u8,
    source_index: usize,
) -> Result<(), String> {
    let inner = state.inner.lock().await;
    let managed = inner
        .connection
        .as_ref()
        .ok_or_else(|| "not connected".to_string())?;
    let source = source_by_index(&managed.connection, source_index)?;
    managed
        .connection
        .client
        .set_program_input(me, source)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_preview_input_by_index(
    state: State<'_, AppState>,
    me: u8,
    source_index: usize,
) -> Result<(), String> {
    let inner = state.inner.lock().await;
    let managed = inner
        .connection
        .as_ref()
        .ok_or_else(|| "not connected".to_string())?;
    let source = source_by_index(&managed.connection, source_index)?;
    managed
        .connection
        .client
        .set_preview_input(me, source)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn cut(state: State<'_, AppState>, me: u8) -> Result<(), String> {
    let inner = state.inner.lock().await;
    let managed = inner
        .connection
        .as_ref()
        .ok_or_else(|| "not connected".to_string())?;
    managed
        .connection
        .client
        .cut(me)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn auto_transition(state: State<'_, AppState>, me: u8) -> Result<(), String> {
    let inner = state.inner.lock().await;
    let managed = inner
        .connection
        .as_ref()
        .ok_or_else(|| "not connected".to_string())?;
    managed
        .connection
        .client
        .auto(me)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_next_transition(
    state: State<'_, AppState>,
    me: u8,
    transition: String,
) -> Result<(), String> {
    let transition = match transition.to_ascii_lowercase().as_str() {
        "mix" => TransitionType::Mix,
        "dip" => TransitionType::Dip,
        "wipe" => TransitionType::Wipe,
        "sting" => TransitionType::Sting,
        "dve" => TransitionType::DVE,
        other => return Err(format!("unsupported transition: {other}")),
    };

    let inner = state.inner.lock().await;
    let managed = inner
        .connection
        .as_ref()
        .ok_or_else(|| "not connected".to_string())?;
    managed
        .connection
        .client
        .set_next_transition(me, transition)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn dsk_auto(state: State<'_, AppState>, key: u8) -> Result<(), String> {
    let inner = state.inner.lock().await;
    let managed = inner
        .connection
        .as_ref()
        .ok_or_else(|| "not connected".to_string())?;
    managed
        .connection
        .client
        .dsk_auto(key)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_dsk_on_air(state: State<'_, AppState>, key: u8, on_air: bool) -> Result<(), String> {
    let inner = state.inner.lock().await;
    let managed = inner
        .connection
        .as_ref()
        .ok_or_else(|| "not connected".to_string())?;
    managed
        .connection
        .client
        .set_dsk_on_air(key, on_air)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_dsk_tie(state: State<'_, AppState>, key: u8, tie: bool) -> Result<(), String> {
    let inner = state.inner.lock().await;
    let managed = inner
        .connection
        .as_ref()
        .ok_or_else(|| "not connected".to_string())?;
    managed
        .connection
        .client
        .set_dsk_tie(key, tie)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_dsk_rate(state: State<'_, AppState>, key: u8, rate: u8) -> Result<(), String> {
    let inner = state.inner.lock().await;
    let managed = inner
        .connection
        .as_ref()
        .ok_or_else(|| "not connected".to_string())?;
    managed
        .connection
        .client
        .set_dsk_rate(key, rate)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn toggle_auto_black(state: State<'_, AppState>, me: u8) -> Result<(), String> {
    let inner = state.inner.lock().await;
    let managed = inner
        .connection
        .as_ref()
        .ok_or_else(|| "not connected".to_string())?;
    managed
        .connection
        .client
        .toggle_auto_black(me)
        .await
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            inner: Mutex::new(InnerState::default()),
        })
        .invoke_handler(tauri::generate_handler![
            connect,
            disconnect,
            get_snapshot,
            get_connection_status,
            set_program_input_by_index,
            set_preview_input_by_index,
            cut,
            auto_transition,
            set_next_transition,
            dsk_auto,
            set_dsk_on_air,
            set_dsk_tie,
            set_dsk_rate,
            toggle_auto_black
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
