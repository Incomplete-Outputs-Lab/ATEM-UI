use atem_core::{
    connect_udp, AtemClientHandle, AtemConnection, AtemSnapshot, ConnectionStatus, TransitionType,
    VideoSource,
};
use gpui::prelude::*;
use gpui::{
    div, px, size, App, Application, Bounds, Context, Entity, IntoElement, Window, WindowBounds,
    WindowOptions,
};
use gpui_component::button::Button;
use gpui_component::input::{Input, InputState};
use gpui_component::label::Label;
use gpui_component::Disableable;

struct AtemUiView {
    ip_input: Entity<InputState>,

    status: ConnectionStatus,
    snapshot: Option<AtemSnapshot>,

    connection: Option<AtemConnection>,
    client: Option<AtemClientHandle>,

    selected_me: u8,
    selected_source_index: usize,
    next_transition: TransitionType,

    // Phase 2 controls
    selected_aux_bus: u8,
    selected_dsk_key_index: usize,
    dsk_rate: u8,
}

impl AtemUiView {
    fn transition_next(t: TransitionType) -> TransitionType {
        match t {
            TransitionType::Mix => TransitionType::Dip,
            TransitionType::Dip => TransitionType::Wipe,
            TransitionType::Wipe => TransitionType::DVE,
            TransitionType::DVE => TransitionType::Sting,
            TransitionType::Sting => TransitionType::Mix,
        }
    }

    fn video_source_label(src: VideoSource) -> String {
        // VideoSource doesn't implement Display, so we provide a compact label.
        match src {
            VideoSource::Black => "Black".to_string(),
            VideoSource::ColourBars => "ColourBars".to_string(),
            VideoSource::Colour1 => "Colour1".to_string(),
            VideoSource::Colour2 => "Colour2".to_string(),
            VideoSource::MediaPlayer1 => "MediaPlayer1".to_string(),
            VideoSource::MediaPlayer2 => "MediaPlayer2".to_string(),
            VideoSource::MediaPlayer3 => "MediaPlayer3".to_string(),
            VideoSource::MediaPlayer4 => "MediaPlayer4".to_string(),
            VideoSource::Auxilary1 => "Aux1".to_string(),
            VideoSource::Auxilary2 => "Aux2".to_string(),
            VideoSource::Auxilary3 => "Aux3".to_string(),
            VideoSource::Auxilary4 => "Aux4".to_string(),
            VideoSource::Auxilary5 => "Aux5".to_string(),
            VideoSource::Auxilary6 => "Aux6".to_string(),
            VideoSource::Unknown => "Unknown".to_string(),
            // Fall back to the numeric representation via Debug.
            other => format!("{other:?}"),
        }
    }
}

impl Render for AtemUiView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak_self = cx.entity().downgrade();

        let status_label = match &self.status {
            ConnectionStatus::Disconnected => "Disconnected".to_string(),
            ConnectionStatus::Connected => "Connected".to_string(),
            ConnectionStatus::Error(e) => format!("Error: {e}"),
        };

        let snapshot = self.snapshot.as_ref();
        let mes_count = snapshot.map(|s| s.mes_count).unwrap_or(0);
        let aux_count = snapshot.map(|s| s.aux_count).unwrap_or(0);
        let dsk_keys = snapshot.map(|s| s.dsk_keys.clone()).unwrap_or_default();
        let available_sources = snapshot
            .map(|s| s.available_sources.clone())
            .unwrap_or_default();

        let selected_source = available_sources.get(self.selected_source_index).copied();
        let selected_dsk_key = dsk_keys.get(self.selected_dsk_key_index).copied();

        // ---- Input + Connect/Disconnect ----
        let connect_button = {
            let ip_input = self.ip_input.clone();
            let weak_self = weak_self.clone();
            let status_blocked = matches!(self.status, ConnectionStatus::Connected);

            Button::new("connect")
                .label("Connect")
                .disabled(status_blocked)
                .on_click(move |_, _, app| {
                    if status_blocked {
                        return;
                    }
                    let ip = ip_input.read(app).value().to_string();
                    let weak_self_task = weak_self.clone();
                    app.spawn(async move |cx| {
                        // Best effort status update.
                        weak_self_task
                            .update(cx, |state, cx| {
                                state.status = ConnectionStatus::Error("Connecting...".to_string());
                                cx.notify();
                            })
                            .ok();

                        let connect_res = connect_udp(&ip, 9910, true).await;
                        match connect_res {
                            Ok(conn) => {
                                let initial_snapshot = conn.snapshot_rx.borrow().clone();
                                let client = conn.client.clone();
                                let mut snapshot_rx = conn.snapshot_rx.clone();

                                weak_self_task
                                    .update(cx, |state, cx| {
                                        state.connection = Some(conn);
                                        state.client = Some(client);
                                        state.snapshot = Some(initial_snapshot);
                                        state.status = ConnectionStatus::Connected;
                                        state.selected_me = 0;
                                        state.selected_source_index = 0;
                                        state.next_transition = TransitionType::Mix;
                                        state.selected_aux_bus = 0;
                                        state.selected_dsk_key_index = 0;
                                        state.dsk_rate = 20;
                                        cx.notify();
                                    })
                                    .ok();

                                // Subscribe to state changes and update the view.
                                while snapshot_rx.changed().await.is_ok() {
                                    let snap = snapshot_rx.borrow().clone();
                                    weak_self_task
                                        .update(cx, |state, cx| {
                                            state.snapshot = Some(snap);
                                            cx.notify();
                                        })
                                        .ok();
                                }
                            }
                            Err(err) => {
                                weak_self_task
                                    .update(cx, |state, cx| {
                                        state.connection = None;
                                        state.client = None;
                                        state.snapshot = None;
                                        state.status = ConnectionStatus::Error(err.to_string());
                                        cx.notify();
                                    })
                                    .ok();
                            }
                        }
                    })
                    .detach();
                })
        };

        let disconnect_button = {
            let weak_self = weak_self.clone();
            let enabled = self.connection.is_some();
            Button::new("disconnect")
                .label("Disconnect")
                .disabled(!enabled)
                .on_click(move |_, _, app| {
                    weak_self
                        .update(app, |state, cx| {
                            if let Some(conn) = state.connection.take() {
                                conn.disconnect();
                            }
                            state.client = None;
                            state.snapshot = None;
                            state.status = ConnectionStatus::Disconnected;
                            state.selected_aux_bus = 0;
                            state.selected_dsk_key_index = 0;
                            state.dsk_rate = 20;
                            cx.notify();
                        })
                        .ok();
                })
        };

        // ---- ME selection ----
        let me_minus = {
            let weak_self = weak_self.clone();
            Button::new("me_minus")
                .label("ME -")
                .disabled(mes_count <= 1)
                .on_click(move |_, _, app| {
                    weak_self
                        .update(app, |state, cx| {
                            if state.selected_me > 0 {
                                state.selected_me -= 1;
                            }
                            cx.notify();
                        })
                        .ok();
                })
        };

        let me_plus = {
            let weak_self = weak_self.clone();
            Button::new("me_plus")
                .label("ME +")
                .disabled(mes_count == 0)
                .on_click(move |_, _, app| {
                    weak_self
                        .update(app, |state, cx| {
                            let max_me = state.snapshot.as_ref().map(|s| s.mes_count).unwrap_or(0);
                            if max_me > 0 && state.selected_me + 1 < max_me {
                                state.selected_me += 1;
                            }
                            cx.notify();
                        })
                        .ok();
                })
        };

        // ---- Source selection ----
        let source_minus = {
            let weak_self = weak_self.clone();
            Button::new("source_minus")
                .label("Source -")
                .disabled(available_sources.is_empty())
                .on_click(move |_, _, app| {
                    weak_self
                        .update(app, |state, cx| {
                            let len = state
                                .snapshot
                                .as_ref()
                                .map(|s| s.available_sources.len())
                                .unwrap_or(0);
                            if len > 0 && state.selected_source_index > 0 {
                                state.selected_source_index -= 1;
                            }
                            cx.notify();
                        })
                        .ok();
                })
        };

        let source_plus = {
            let weak_self = weak_self.clone();
            Button::new("source_plus")
                .label("Source +")
                .disabled(available_sources.is_empty())
                .on_click(move |_, _, app| {
                    weak_self
                        .update(app, |state, cx| {
                            let len = state
                                .snapshot
                                .as_ref()
                                .map(|s| s.available_sources.len())
                                .unwrap_or(0);
                            if len > 0 && state.selected_source_index + 1 < len {
                                state.selected_source_index += 1;
                            }
                            cx.notify();
                        })
                        .ok();
                })
        };

        // ---- Next transition selection ----
        let transition_button = {
            let weak_self = weak_self.clone();
            Button::new("transition_next")
                .label("Next Transition")
                .disabled(self.client.is_none())
                .on_click(move |_, _, app| {
                    weak_self
                        .update(app, |state, cx| {
                            state.next_transition =
                                AtemUiView::transition_next(state.next_transition);
                            cx.notify();
                        })
                        .ok();
                })
        };

        // ---- Command buttons (MVP) ----
        let client = self.client.clone();
        let me = self.selected_me;
        let source = selected_source;
        let next_transition = self.next_transition;

        let cut_button = {
            let client = client.clone();
            Button::new("cut")
                .label("Cut")
                .disabled(client.is_none() || mes_count == 0)
                .on_click(move |_, _, app| {
                    if let Some(client) = client.clone() {
                        app.spawn(async move |cx| {
                            let _res = client.cut(me).await;
                            let _ = cx;
                        })
                        .detach();
                    }
                })
        };

        let auto_button = {
            let client = client.clone();
            Button::new("auto")
                .label("Auto")
                .disabled(client.is_none() || mes_count == 0)
                .on_click(move |_, _, app| {
                    if let Some(client) = client.clone() {
                        app.spawn(async move |cx| {
                            let _res = client.auto(me).await;
                            let _ = cx;
                        })
                        .detach();
                    }
                })
        };

        let set_next_transition_button = {
            let client = client.clone();
            Button::new("set_next_transition")
                .label("Set Next Transition")
                .disabled(client.is_none() || mes_count == 0)
                .on_click(move |_, _, app| {
                    if let Some(client) = client.clone() {
                        app.spawn(async move |cx| {
                            let _res = client.set_next_transition(me, next_transition).await;
                            let _ = cx;
                        })
                        .detach();
                    }
                })
        };

        let set_program_button = {
            let client = client.clone();
            Button::new("set_program")
                .label("Set Program")
                .disabled(client.is_none() || source.is_none())
                .on_click(move |_, _, app| {
                    if let (Some(client), Some(source)) = (client.clone(), source) {
                        app.spawn(async move |cx| {
                            let _res = client.set_program_input(me, source).await;
                            let _ = cx;
                        })
                        .detach();
                    }
                })
        };

        let set_preview_button = {
            let client = client.clone();
            Button::new("set_preview")
                .label("Set Preview")
                .disabled(client.is_none() || source.is_none())
                .on_click(move |_, _, app| {
                    if let (Some(client), Some(source)) = (client.clone(), source) {
                        app.spawn(async move |cx| {
                            let _res = client.set_preview_input(me, source).await;
                            let _ = cx;
                        })
                        .detach();
                    }
                })
        };

        // ---- Phase 2: AUX ----
        let aux_minus = {
            let weak_self = weak_self.clone();
            Button::new("aux_minus")
                .label("AUX -")
                .disabled(self.client.is_none() || aux_count <= 1)
                .on_click(move |_, _, app| {
                    weak_self
                        .update(app, |state, cx| {
                            if state.selected_aux_bus > 0 {
                                state.selected_aux_bus -= 1;
                            }
                            cx.notify();
                        })
                        .ok();
                })
        };

        let aux_plus = {
            let weak_self = weak_self.clone();
            Button::new("aux_plus")
                .label("AUX +")
                .disabled(self.client.is_none() || aux_count == 0)
                .on_click(move |_, _, app| {
                    weak_self
                        .update(app, |state, cx| {
                            let max_aux = state.snapshot.as_ref().map(|s| s.aux_count).unwrap_or(0);
                            if max_aux > 0 && state.selected_aux_bus + 1 < max_aux {
                                state.selected_aux_bus += 1;
                            }
                            cx.notify();
                        })
                        .ok();
                })
        };

        let set_aux_source_button = {
            let client = client.clone();
            let aux_bus = self.selected_aux_bus;
            let source = selected_source;
            Button::new("set_aux_source")
                .label("Set AUX Source")
                .disabled(client.is_none() || source.is_none() || aux_count == 0)
                .on_click(move |_, _, app| {
                    if let (Some(client), Some(source)) = (client.clone(), source) {
                        app.spawn(async move |cx| {
                            let _res = client.set_aux_source(aux_bus, source).await;
                            let _ = cx;
                        })
                        .detach();
                    }
                })
        };

        // ---- Phase 2: DSK ----
        let dsk_minus = {
            let weak_self = weak_self.clone();
            Button::new("dsk_minus")
                .label("DSK -")
                .disabled(self.client.is_none() || dsk_keys.is_empty())
                .on_click(move |_, _, app| {
                    weak_self
                        .update(app, |state, cx| {
                            if state.selected_dsk_key_index > 0 {
                                state.selected_dsk_key_index -= 1;
                            }
                            cx.notify();
                        })
                        .ok();
                })
        };

        let dsk_plus = {
            let weak_self = weak_self.clone();
            Button::new("dsk_plus")
                .label("DSK +")
                .disabled(self.client.is_none() || dsk_keys.is_empty())
                .on_click(move |_, _, app| {
                    weak_self
                        .update(app, |state, cx| {
                            let len = state
                                .snapshot
                                .as_ref()
                                .map(|s| s.dsk_keys.len())
                                .unwrap_or(0);
                            if len > 0 && state.selected_dsk_key_index + 1 < len {
                                state.selected_dsk_key_index += 1;
                            }
                            cx.notify();
                        })
                        .ok();
                })
        };

        let dsk_rate_minus = {
            let weak_self = weak_self.clone();
            Button::new("dsk_rate_minus")
                .label("Rate -")
                .disabled(self.client.is_none() || selected_dsk_key.is_none())
                .on_click(move |_, _, app| {
                    weak_self
                        .update(app, |state, cx| {
                            state.dsk_rate = state.dsk_rate.saturating_sub(1);
                            cx.notify();
                        })
                        .ok();
                })
        };

        let dsk_rate_plus = {
            let weak_self = weak_self.clone();
            Button::new("dsk_rate_plus")
                .label("Rate +")
                .disabled(self.client.is_none() || selected_dsk_key.is_none())
                .on_click(move |_, _, app| {
                    weak_self
                        .update(app, |state, cx| {
                            state.dsk_rate = state.dsk_rate.saturating_add(1);
                            cx.notify();
                        })
                        .ok();
                })
        };

        let dsk_auto_button = {
            let client = client.clone();
            let key = selected_dsk_key;
            Button::new("dsk_auto")
                .label("DSK Auto")
                .disabled(client.is_none() || key.is_none())
                .on_click(move |_, _, app| {
                    if let (Some(client), Some(key)) = (client.clone(), key) {
                        app.spawn(async move |cx| {
                            let _res = client.dsk_auto(key).await;
                            let _ = cx;
                        })
                        .detach();
                    }
                })
        };

        let dsk_on_button = {
            let client = client.clone();
            let key = selected_dsk_key;
            Button::new("dsk_on")
                .label("DSK On")
                .disabled(client.is_none() || key.is_none())
                .on_click(move |_, _, app| {
                    if let (Some(client), Some(key)) = (client.clone(), key) {
                        app.spawn(async move |cx| {
                            let _res = client.set_dsk_on_air(key, true).await;
                            let _ = cx;
                        })
                        .detach();
                    }
                })
        };

        let dsk_off_button = {
            let client = client.clone();
            let key = selected_dsk_key;
            Button::new("dsk_off")
                .label("DSK Off")
                .disabled(client.is_none() || key.is_none())
                .on_click(move |_, _, app| {
                    if let (Some(client), Some(key)) = (client.clone(), key) {
                        app.spawn(async move |cx| {
                            let _res = client.set_dsk_on_air(key, false).await;
                            let _ = cx;
                        })
                        .detach();
                    }
                })
        };

        let dsk_tie_on_button = {
            let client = client.clone();
            let key = selected_dsk_key;
            Button::new("dsk_tie_on")
                .label("DSK Tie On")
                .disabled(client.is_none() || key.is_none())
                .on_click(move |_, _, app| {
                    if let (Some(client), Some(key)) = (client.clone(), key) {
                        app.spawn(async move |cx| {
                            let _res = client.set_dsk_tie(key, true).await;
                            let _ = cx;
                        })
                        .detach();
                    }
                })
        };

        let dsk_tie_off_button = {
            let client = client.clone();
            let key = selected_dsk_key;
            Button::new("dsk_tie_off")
                .label("DSK Tie Off")
                .disabled(client.is_none() || key.is_none())
                .on_click(move |_, _, app| {
                    if let (Some(client), Some(key)) = (client.clone(), key) {
                        app.spawn(async move |cx| {
                            let _res = client.set_dsk_tie(key, false).await;
                            let _ = cx;
                        })
                        .detach();
                    }
                })
        };

        let dsk_set_cut_button = {
            let client = client.clone();
            let key = selected_dsk_key;
            let source = selected_source;
            Button::new("dsk_set_cut")
                .label("DSK Cut -> Sel")
                .disabled(client.is_none() || key.is_none() || source.is_none())
                .on_click(move |_, _, app| {
                    if let (Some(client), Some(key), Some(source)) = (client.clone(), key, source) {
                        app.spawn(async move |cx| {
                            let _res = client.set_dsk_cut_source(key, source).await;
                            let _ = cx;
                        })
                        .detach();
                    }
                })
        };

        let dsk_set_fill_button = {
            let client = client.clone();
            let key = selected_dsk_key;
            let source = selected_source;
            Button::new("dsk_set_fill")
                .label("DSK Fill -> Sel")
                .disabled(client.is_none() || key.is_none() || source.is_none())
                .on_click(move |_, _, app| {
                    if let (Some(client), Some(key), Some(source)) = (client.clone(), key, source) {
                        app.spawn(async move |cx| {
                            let _res = client.set_dsk_fill_source(key, source).await;
                            let _ = cx;
                        })
                        .detach();
                    }
                })
        };

        let dsk_set_rate_button = {
            let client = client.clone();
            let key = selected_dsk_key;
            let rate = self.dsk_rate;
            Button::new("dsk_set_rate")
                .label("DSK Rate Set")
                .disabled(client.is_none() || key.is_none())
                .on_click(move |_, _, app| {
                    if let (Some(client), Some(key)) = (client.clone(), key) {
                        app.spawn(async move |cx| {
                            let _res = client.set_dsk_rate(key, rate).await;
                            let _ = cx;
                        })
                        .detach();
                    }
                })
        };

        // ---- Render snapshot info ----
        let program_label = snapshot
            .and_then(|s| s.program_sources.get(self.selected_me as usize).copied())
            .map(AtemUiView::video_source_label)
            .unwrap_or_else(|| "-".to_string());
        let preview_label = snapshot
            .and_then(|s| s.preview_sources.get(self.selected_me as usize).copied())
            .map(AtemUiView::video_source_label)
            .unwrap_or_else(|| "-".to_string());

        let available_list_preview = {
            let show = 20usize;
            if available_sources.is_empty() {
                "-".to_string()
            } else {
                let mut labels = available_sources
                    .iter()
                    .take(show)
                    .map(|s| AtemUiView::video_source_label(*s))
                    .collect::<Vec<_>>();
                if available_sources.len() > show {
                    labels.push(format!("+{} more", available_sources.len() - show));
                }
                labels.join(", ")
            }
        };

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(Label::new(format!("Status: {status_label}")))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(Label::new("ATEM IP (UDP 9910)"))
                    .child(Input::new(&self.ip_input)),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(connect_button)
                    .child(disconnect_button),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(Label::new(format!("MEs: {mes_count}, AUX: {aux_count}")))
                    .child(div().flex().gap_2().child(me_minus).child(me_plus))
                    .child(Label::new(format!(
                        "Selected ME: {} (0-based)",
                        self.selected_me
                    )))
                    .child(Label::new(format!(
                        "Program: {program_label} | Preview: {preview_label}"
                    )))
                    .child(div().flex().gap_2().child(cut_button).child(auto_button))
                    .child(set_next_transition_button)
                    .child(transition_button)
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(set_program_button)
                            .child(set_preview_button),
                    )
                    .child(Label::new("Phase 2: AUX / DSK"))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(Label::new(format!(
                                "Selected AUX: {} (0-based)",
                                self.selected_aux_bus
                            )))
                            .child(div().flex().gap_2().child(aux_minus).child(aux_plus))
                            .child(set_aux_source_button)
                            .child(Label::new(format!(
                                "Selected DSK Key: {:?}",
                                selected_dsk_key
                            )))
                            .child(div().flex().gap_2().child(dsk_minus).child(dsk_plus))
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(dsk_auto_button)
                                    .child(dsk_on_button),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(dsk_off_button)
                                    .child(dsk_tie_on_button),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(dsk_tie_off_button)
                                    .child(dsk_set_cut_button),
                            )
                            .child(dsk_set_fill_button)
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(dsk_rate_minus)
                                    .child(dsk_rate_plus),
                            )
                            .child(dsk_set_rate_button),
                    )
                    .child(div().flex().gap_2().child(source_minus).child(source_plus))
                    .child(Label::new(format!(
                        "Selected Source: {}",
                        selected_source
                            .map(AtemUiView::video_source_label)
                            .unwrap_or_else(|| "-".to_string())
                    )))
                    .child(Label::new(format!(
                        "Sources (preview): {available_list_preview}"
                    ))),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        // Required for gpui-component widgets (e.g. Theme global) to be available.
        gpui_component::init(cx);

        let bounds = Bounds::centered(None, size(px(720.), px(520.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let ip_input =
                    cx.new(|cx| InputState::new(window, cx).default_value("192.168.1.50"));

                let view = cx.new(|_| AtemUiView {
                    ip_input,
                    status: ConnectionStatus::Disconnected,
                    snapshot: None,
                    connection: None,
                    client: None,
                    selected_me: 0,
                    selected_source_index: 0,
                    next_transition: TransitionType::Mix,
                    selected_aux_bus: 0,
                    selected_dsk_key_index: 0,
                    dsk_rate: 20,
                });

                // gpui-component requires a Root view as the first window layer.
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            },
        )
        .unwrap();
    });
}
