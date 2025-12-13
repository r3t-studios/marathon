//! Entity selection and lock acquisition
//!
//! Handles clicking/tapping on entities to select them, acquiring locks,
//! and providing visual feedback based on lock state.

use bevy::prelude::*;
use libmarathon::networking::{
    EntityLockRegistry,
    GossipBridge,
    LockMessage,
    NetworkedEntity,
    NetworkedSelection,
    NodeVectorClock,
    SyncMessage,
    VersionedMessage,
};
use uuid::Uuid;

use crate::cube::CubeMarker;

pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                handle_entity_click,
                handle_deselect_key,
                update_lock_visuals,
            )
                .chain(),
        );
    }
}

/// System to handle clicking/tapping on entities to select and acquire locks
fn handle_entity_click(
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    cubes: Query<(Entity, &Transform, &NetworkedEntity), With<CubeMarker>>,
    mut selections: Query<&mut NetworkedSelection>,
    mut lock_registry: ResMut<EntityLockRegistry>,
    node_clock: Res<NodeVectorClock>,
    bridge: Option<Res<GossipBridge>>,
) {
    // Only on left click press
    if !mouse_button.just_pressed(MouseButton::Left) {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok((camera, cam_transform)) = cameras.single() else {
        return;
    };

    // Cast ray from cursor
    let Ok(ray) = camera.viewport_to_world(cam_transform, cursor_pos) else {
        return;
    };

    // Find closest cube that intersects ray
    let mut closest: Option<(f32, Entity, Uuid)> = None;

    for (entity, transform, networked) in cubes.iter() {
        // Simple sphere collision (approximate cube as sphere with radius ~0.7)
        let to_cube = transform.translation - ray.origin;
        let t = to_cube.dot(*ray.direction);
        if t < 0.0 {
            continue;
        }

        let closest_point = ray.origin + *ray.direction * t;
        let distance = (closest_point - transform.translation).length();

        if distance < 0.7 {
            // Hit!
            if let Some((best_dist, _, _)) = closest {
                if t < best_dist {
                    closest = Some((t, entity, networked.network_id));
                }
            } else {
                closest = Some((t, entity, networked.network_id));
            }
        }
    }

    // Process result
    if let Some((_, bevy_entity, entity_id)) = closest {
        // Clicked on a cube - try to acquire lock
        match lock_registry.try_acquire(entity_id, node_clock.node_id) {
            Ok(()) => {
                info!("Lock acquired for {}", entity_id);

                // Update selection component
                if let Ok(mut selection) = selections.get_mut(bevy_entity) {
                    selection.selected_ids.clear(); // Clear previous selections
                    selection.selected_ids.insert(entity_id);
                }

                // Broadcast LockRequest to other nodes
                if let Some(bridge) = bridge.as_ref() {
                    let msg = VersionedMessage::new(SyncMessage::Lock(LockMessage::LockRequest {
                        entity_id,
                        node_id: node_clock.node_id,
                    }));
                    if let Err(e) = bridge.send(msg) {
                        error!("Failed to broadcast lock request: {}", e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to acquire lock for {}: {}", entity_id, e);
            }
        }
    } else {
        // Clicked on empty space - deselect all and release locks
        for mut selection in selections.iter_mut() {
            // Release all locks we're holding
            for entity_id in selection.selected_ids.iter() {
                lock_registry.release(*entity_id, node_clock.node_id);
                info!("Released lock for {} (clicked away)", entity_id);

                // Broadcast LockRelease to other nodes
                if let Some(bridge) = bridge.as_ref() {
                    let msg = VersionedMessage::new(SyncMessage::Lock(LockMessage::LockRelease {
                        entity_id: *entity_id,
                        node_id: node_clock.node_id,
                    }));
                    if let Err(e) = bridge.send(msg) {
                        error!("Failed to broadcast lock release: {}", e);
                    }
                }
            }

            selection.selected_ids.clear();
        }
    }
}

/// System to handle ESC key for deselection
fn handle_deselect_key(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut selections: Query<&mut NetworkedSelection>,
    mut lock_registry: ResMut<EntityLockRegistry>,
    node_clock: Res<NodeVectorClock>,
    bridge: Option<Res<GossipBridge>>,
) {
    if keyboard.just_pressed(KeyCode::Escape) {
        for mut selection in selections.iter_mut() {
            if !selection.selected_ids.is_empty() {
                info!("Deselecting {} entities via ESC key", selection.selected_ids.len());

                // Release all locks we're holding
                for entity_id in selection.selected_ids.iter() {
                    lock_registry.release(*entity_id, node_clock.node_id);
                    info!("Released lock for {} (ESC key)", entity_id);

                    // Broadcast LockRelease to other nodes
                    if let Some(bridge) = bridge.as_ref() {
                        let msg = VersionedMessage::new(SyncMessage::Lock(LockMessage::LockRelease {
                            entity_id: *entity_id,
                            node_id: node_clock.node_id,
                        }));
                        if let Err(e) = bridge.send(msg) {
                            error!("Failed to broadcast lock release: {}", e);
                        }
                    }
                }

                selection.selected_ids.clear();
            }
        }
    }
}

/// System to update visual appearance based on lock state
///
/// Color scheme:
/// - Green: Locked by us (we can edit)
/// - Red: Locked by someone else (they can edit, we can't)
/// - Pink: Not locked (nobody is editing)
fn update_lock_visuals(
    lock_registry: Res<EntityLockRegistry>,
    node_clock: Res<NodeVectorClock>,
    mut cubes: Query<(&NetworkedEntity, &mut MeshMaterial3d<StandardMaterial>), With<CubeMarker>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {

    for (networked, material_handle) in cubes.iter_mut() {
        let entity_id = networked.network_id;

        // Determine color based on lock state
        let node_id = node_clock.node_id;
        let color = if lock_registry.is_locked_by(entity_id, node_id, node_id) {
            // Locked by us - green
            Color::srgb(0.3, 0.8, 0.3)
        } else if lock_registry.is_locked(entity_id, node_id) {
            // Locked by someone else - red
            Color::srgb(0.8, 0.3, 0.3)
        } else {
            // Not locked - default pink
            Color::srgb(0.8, 0.3, 0.6)
        };

        // Update material color
        if let Some(mat) = materials.get_mut(&material_handle.0) {
            mat.base_color = color;
        }
    }
}
