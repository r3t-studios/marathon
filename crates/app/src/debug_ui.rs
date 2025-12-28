//! Debug UI overlay using egui

use bevy::prelude::*;
use bevy::ecs::message::MessageWriter;
use libmarathon::debug_ui::{EguiContexts, EguiPrimaryContextPass};
use libmarathon::networking::{
    EntityLockRegistry,
    GossipBridge,
    NetworkedEntity,
    NodeVectorClock,
};

use crate::cube::{CubeMarker, DeleteCubeEvent, SpawnCubeEvent};

pub struct DebugUiPlugin;

impl Plugin for DebugUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(EguiPrimaryContextPass, render_debug_ui);
    }
}

/// Render the debug UI panel
fn render_debug_ui(
    mut egui_ctx: EguiContexts,
    node_clock: Option<Res<NodeVectorClock>>,
    gossip_bridge: Option<Res<GossipBridge>>,
    lock_registry: Option<Res<EntityLockRegistry>>,
    cube_query: Query<(&Transform, &NetworkedEntity), With<CubeMarker>>,
    mut spawn_events: MessageWriter<SpawnCubeEvent>,
    mut delete_events: MessageWriter<DeleteCubeEvent>,
) -> Result {
    let ctx: &egui::Context = egui_ctx.ctx_mut()?;

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
                // Show the sum of all timestamps (total operations across all nodes)
                let total_ops: u64 = clock.clock.timestamps.values().sum();
                ui.label(format!("Clock: {} (total ops)", total_ops));
                ui.label(format!("Known nodes: {}", clock.clock.node_count()));
            } else {
                ui.label("Node: Not initialized");
            }

            ui.add_space(5.0);

            // Gossip bridge status
            if let Some(bridge) = &gossip_bridge {
                ui.label(format!(
                    "Bridge Node: {}",
                    &bridge.node_id().to_string()[..8]
                ));
                ui.label("Status: Connected");
            } else {
                ui.label("Gossip: Not ready");
            }

            ui.add_space(10.0);
            ui.heading("Cube State");
            ui.separator();

            // Cube information
            match cube_query.iter().next() {
                | Some((transform, networked)) => {
                    let pos = transform.translation;
                    ui.label(format!(
                        "Position: ({:.2}, {:.2}, {:.2})",
                        pos.x, pos.y, pos.z
                    ));

                    let (axis, angle) = transform.rotation.to_axis_angle();
                    let angle_deg: f32 = angle.to_degrees();
                    ui.label(format!(
                        "Rotation: {:.2}° around ({:.2}, {:.2}, {:.2})",
                        angle_deg, axis.x, axis.y, axis.z
                    ));

                    ui.label(format!(
                        "Scale: ({:.2}, {:.2}, {:.2})",
                        transform.scale.x, transform.scale.y, transform.scale.z
                    ));

                    ui.add_space(5.0);
                    ui.label(format!(
                        "Network ID: {}",
                        &networked.network_id.to_string()[..8]
                    ));
                    ui.label(format!(
                        "Owner: {}",
                        &networked.owner_node_id.to_string()[..8]
                    ));
                },
                | None => {
                    ui.label("Cube: Not spawned yet");
                },
            }

            ui.add_space(10.0);
            ui.heading("Entity Controls");
            ui.separator();

            if ui.button("➕ Spawn Cube").clicked() {
                spawn_events.write(SpawnCubeEvent {
                    position: Vec3::new(
                        rand::random::<f32>() * 4.0 - 2.0,
                        0.5,
                        rand::random::<f32>() * 4.0 - 2.0,
                    ),
                });
            }

            ui.label(format!("Total cubes: {}", cube_query.iter().count()));

            // List all cubes with delete buttons
            ui.add_space(5.0);
            egui::ScrollArea::vertical()
                .id_salt("cube_list")
                .max_height(150.0)
                .show(ui, |ui| {
                    for (_transform, networked) in cube_query.iter() {
                        ui.horizontal(|ui| {
                            ui.label(format!("Cube {:.8}...", networked.network_id));
                            if ui.small_button("🗑").clicked() {
                                delete_events.write(DeleteCubeEvent {
                                    entity_id: networked.network_id,
                                });
                            }
                        });
                    }
                });

            ui.add_space(10.0);
            ui.heading("Lock Status");
            ui.separator();

            if let (Some(lock_registry), Some(clock)) = (&lock_registry, &node_clock) {
                let node_id = clock.node_id;
                let locked_cubes = cube_query
                    .iter()
                    .filter(|(_, networked)| lock_registry.is_locked(networked.network_id, node_id))
                    .count();

                ui.label(format!("Locked entities: {}", locked_cubes));

                ui.add_space(5.0);
                egui::ScrollArea::vertical()
                    .id_salt("lock_list")
                    .max_height(100.0)
                    .show(ui, |ui| {
                        for (_, networked) in cube_query.iter() {
                            let entity_id = networked.network_id;
                            if let Some(holder) = lock_registry.get_holder(entity_id, node_id) {
                                let is_ours = holder == node_id;
                                ui.horizontal(|ui| {
                                    ui.label(format!("🔒 {:.8}...", entity_id));
                                    ui.label(if is_ours { "(you)" } else { "(peer)" });
                                });
                            }
                        }
                    });
            } else {
                ui.label("Lock registry: Not ready");
            }

            ui.add_space(10.0);
            ui.heading("Controls");
            ui.separator();
            ui.label("Left click: Select cube");
            ui.label("Left drag: Move cube (XY)");
            ui.label("Right drag: Rotate cube");
            ui.label("Scroll: Move cube (Z)");
            ui.label("ESC: Deselect");
        });

    Ok(())
}
