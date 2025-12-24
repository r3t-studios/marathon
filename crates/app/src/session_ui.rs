//! Session UI panel
//!
//! Displays current session code, allows joining different sessions,
//! and shows connected peer information.

use bevy::prelude::*;
use libmarathon::{
    debug_ui::{egui, EguiContexts, EguiPrimaryContextPass},
    engine::{EngineBridge, EngineCommand},
    networking::{CurrentSession, NodeVectorClock, SessionId, SessionState},
};

pub struct SessionUiPlugin;

impl Plugin for SessionUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SessionUiState>()
            .add_systems(EguiPrimaryContextPass, session_ui_panel);
    }
}

#[derive(Resource, Default)]
struct SessionUiState {
    join_code_input: String,
    show_join_dialog: bool,
}

fn session_ui_panel(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<SessionUiState>,
    current_session: Res<CurrentSession>,
    node_clock: Option<Res<NodeVectorClock>>,
    bridge: Res<EngineBridge>,
) {
    // Log session state for debugging
    debug!("Session UI: state={:?}, id={}", 
        current_session.session.state, 
        current_session.session.id.to_code());

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::Window::new("Session")
        .default_pos([320.0, 10.0])
        .default_width(280.0)
        .show(ctx, |ui| {
            // Check if networking is active based on session state
            if current_session.session.state == SessionState::Active {
                // ONLINE MODE: Networking is active
                ui.heading("Session (Online)");
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Code:");
                    ui.code(current_session.session.id.to_code());
                    if ui.small_button("📋").clicked() {
                        // TODO: Copy to clipboard (requires clipboard API)
                        info!("Session code: {}", current_session.session.id.to_code());
                    }
                });

                ui.label(format!("State: {:?}", current_session.session.state));

                if let Some(clock) = node_clock.as_ref() {
                    ui.label(format!("Connected nodes: {}", clock.clock.clocks.len()));
                }

                ui.add_space(10.0);

                // Stop networking button
                if ui.button("🔌 Stop Networking").clicked() {
                    info!("Stopping networking");
                    bridge.send_command(EngineCommand::StopNetworking);
                }
            } else {
                // OFFLINE MODE: Networking not started or disconnected
                ui.heading("Offline Mode");
                ui.separator();

                ui.label("World is running offline");
                ui.label("Vector clock is tracking changes");

                if let Some(clock) = node_clock.as_ref() {
                    let current_seq = clock.clock.clocks.get(&clock.node_id).copied().unwrap_or(0);
                    ui.label(format!("Local sequence: {}", current_seq));
                }

                ui.add_space(10.0);

                // Start networking button
                if ui.button("🌐 Start Networking").clicked() {
                    info!("Starting networking (will create new session)");
                    // Generate a new session ID on the fly
                    let new_session_id = libmarathon::networking::SessionId::new();
                    info!("New session code: {}", new_session_id.to_code());
                    bridge.send_command(EngineCommand::StartNetworking {
                        session_id: new_session_id,
                    });
                }

                ui.add_space(5.0);

                // Join existing session button
                if ui.button("➕ Join Session").clicked() {
                    ui_state.show_join_dialog = true;
                }
            }
        });

    // Join dialog (using same context)
    if ui_state.show_join_dialog {
        egui::Window::new("Join Session")
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label("Enter session code (abc-def-123):");
                ui.text_edit_singleline(&mut ui_state.join_code_input);

                ui.add_space(5.0);
                ui.label("Note: Joining requires app restart");

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Join").clicked() {
                        match SessionId::from_code(&ui_state.join_code_input) {
                            Ok(session_id) => {
                                info!("Joining session: {} → {}", ui_state.join_code_input, session_id);
                                bridge.send_command(EngineCommand::JoinSession {
                                    session_id,
                                });
                                ui_state.show_join_dialog = false;
                                ui_state.join_code_input.clear();
                            }
                            Err(e) => {
                                error!("Invalid session code '{}': {:?}", ui_state.join_code_input, e);
                            }
                        }
                    }

                    if ui.button("Cancel").clicked() {
                        ui_state.show_join_dialog = false;
                        ui_state.join_code_input.clear();
                    }
                });
            });
    }
}
