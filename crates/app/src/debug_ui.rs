//! Debug UI overlay using egui

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};
use lib::networking::{GossipBridge, NodeVectorClock};

pub struct DebugUiPlugin;

impl Plugin for DebugUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(EguiPrimaryContextPass, render_debug_ui);
    }
}

/// Render the debug UI panel
fn render_debug_ui(
    mut contexts: EguiContexts,
    node_clock: Option<Res<NodeVectorClock>>,
    gossip_bridge: Option<Res<GossipBridge>>,
    cube_query: Query<(&Transform, &lib::networking::NetworkedEntity), With<crate::cube::CubeMarker>>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::Window::new("Debug Info")
        .default_pos([10.0, 10.0])
        .default_width(300.0)
        .resizable(true)
        .show(ctx, |ui| {
            ui.heading("Network Status");
            ui.separator();

            // Node information
            if let Some(clock) = &node_clock {
                ui.label(format!("Node ID: {}", &clock.node_id.to_string()[..8]));
                // Show the current node's clock value (timestamp)
                let current_timestamp = clock.clock.clocks.get(&clock.node_id).copied().unwrap_or(0);
                ui.label(format!("Clock: {}", current_timestamp));
                ui.label(format!("Known nodes: {}", clock.clock.clocks.len()));
            } else {
                ui.label("Node: Not initialized");
            }

            ui.add_space(5.0);

            // Gossip bridge status
            if let Some(bridge) = &gossip_bridge {
                ui.label(format!("Bridge Node: {}", &bridge.node_id().to_string()[..8]));
                ui.label("Status: Connected");
            } else {
                ui.label("Gossip: Not ready");
            }

            ui.add_space(10.0);
            ui.heading("Cube State");
            ui.separator();

            // Cube information
            match cube_query.iter().next() {
                Some((transform, networked)) => {
                    let pos = transform.translation;
                    ui.label(format!("Position: ({:.2}, {:.2}, {:.2})", pos.x, pos.y, pos.z));

                    let (axis, angle) = transform.rotation.to_axis_angle();
                    let angle_deg: f32 = angle.to_degrees();
                    ui.label(format!("Rotation: {:.2}° around ({:.2}, {:.2}, {:.2})",
                        angle_deg, axis.x, axis.y, axis.z));

                    ui.label(format!("Scale: ({:.2}, {:.2}, {:.2})",
                        transform.scale.x, transform.scale.y, transform.scale.z));

                    ui.add_space(5.0);
                    ui.label(format!("Network ID: {}", &networked.network_id.to_string()[..8]));
                    ui.label(format!("Owner: {}", &networked.owner_node_id.to_string()[..8]));
                }
                None => {
                    ui.label("Cube: Not spawned yet");
                }
            }

            ui.add_space(10.0);
            ui.heading("Controls");
            ui.separator();
            ui.label("Left drag: Move cube (XY)");
            ui.label("Right drag: Rotate cube");
            ui.label("Scroll: Move cube (Z)");
        });
}
