//! Input handling using engine GameActions
//!
//! Processes GameActions (from InputController) and applies them to game
//! entities.

use bevy::prelude::*;
use libmarathon::{
    engine::GameAction,
    networking::{
        EntityLockRegistry,
        LocalSelection,
        NetworkedEntity,
        NodeVectorClock,
    },
    platform::input::InputController,
};

use super::event_buffer::InputEventBuffer;
use crate::cube::CubeMarker;

pub struct InputHandlerPlugin;

impl Plugin for InputHandlerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InputControllerResource>()
            // handle_game_actions updates selection - must run before
            // release_locks_on_deselection_system
            .add_systems(
                Update,
                handle_game_actions
                    .before(libmarathon::networking::release_locks_on_deselection_system),
            )
            .add_systems(PostUpdate, update_lock_visuals);
    }
}

/// Resource wrapping the InputController
#[derive(Resource)]
struct InputControllerResource {
    controller: InputController,
}

impl Default for InputControllerResource {
    fn default() -> Self {
        Self {
            controller: InputController::new(),
        }
    }
}

/// Convert glam::Vec2 to Bevy's Vec2
///
/// They're the same type, just construct a new one.
#[inline]
fn to_bevy_vec2(v: glam::Vec2) -> bevy::math::Vec2 {
    bevy::math::Vec2::new(v.x, v.y)
}

/// Process GameActions and apply to entities
fn handle_game_actions(
    input_buffer: Res<InputEventBuffer>,
    mut controller_res: ResMut<InputControllerResource>,
    lock_registry: Res<EntityLockRegistry>,
    node_clock: Res<NodeVectorClock>,
    mut selection: ResMut<LocalSelection>,
    mut cube_query: Query<(&NetworkedEntity, &mut Transform), With<crate::cube::CubeMarker>>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    window_query: Query<&Window>,
) {
    let node_id = node_clock.node_id;

    // Process all input events through the controller to get game actions
    let mut all_actions = Vec::new();
    for event in input_buffer.events.iter() {
        let actions = controller_res.controller.process_event(event);
        all_actions.extend(actions);
    }

    // Apply game actions to entities
    for action in all_actions {
        match action {
            | GameAction::SelectEntity { position } => {
                // Do raycasting to find which entity (if any) was clicked
                let entity_id = raycast_entity(position, &cube_query, &camera_query, &window_query);

                // Update selection
                // The release_locks_on_deselection_system will automatically handle lock
                // changes
                selection.clear();
                if let Some(id) = entity_id {
                    selection.insert(id);
                    info!("Selected entity {}", id);
                } else {
                    info!("Deselected all entities");
                }
            },

            | GameAction::MoveEntity { delta } => {
                apply_move_entity(delta, &lock_registry, node_id, &mut cube_query);
            },

            | GameAction::RotateEntity { delta } => {
                apply_rotate_entity(delta, &lock_registry, node_id, &mut cube_query);
            },

            | GameAction::MoveEntityDepth { delta } => {
                apply_move_depth(delta, &lock_registry, node_id, &mut cube_query);
            },

            | GameAction::ResetEntity => {
                apply_reset_entity(&lock_registry, node_id, &mut cube_query);
            },

            | _ => {
                // Other actions not yet implemented
            },
        }
    }
}

/// Raycast to find which entity was clicked
///
/// Returns the network ID of the closest entity hit by the ray, or None if
/// nothing was hit.
fn raycast_entity(
    position: glam::Vec2,
    cube_query: &Query<(&NetworkedEntity, &mut Transform), With<crate::cube::CubeMarker>>,
    camera_query: &Query<(&Camera, &GlobalTransform)>,
    window_query: &Query<&Window>,
) -> Option<uuid::Uuid> {
    // Get the camera and window
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return None;
    };
    let Ok(window) = window_query.single() else {
        return None;
    };

    // Convert screen position to world ray
    let Some(ray) = screen_to_world_ray(position, camera, camera_transform, window) else {
        return None;
    };

    // Find the closest cube hit by the ray
    let mut closest_hit: Option<(uuid::Uuid, f32)> = None;

    for (networked, transform) in cube_query.iter() {
        // Test ray against cube AABB (1x1x1 cube)
        if let Some(distance) = ray_aabb_intersection(
            ray.origin,
            ray.direction,
            transform.translation,
            Vec3::splat(0.5), // Half extents for 1x1x1 cube
        ) {
            if closest_hit.map_or(true, |(_, d)| distance < d) {
                closest_hit = Some((networked.network_id, distance));
            }
        }
    }

    closest_hit.map(|(entity_id, _)| entity_id)
}

/// Apply MoveEntity action to locked cubes
fn apply_move_entity(
    delta: glam::Vec2,
    lock_registry: &EntityLockRegistry,
    node_id: uuid::Uuid,
    cube_query: &mut Query<(&NetworkedEntity, &mut Transform), With<crate::cube::CubeMarker>>,
) {
    let bevy_delta = to_bevy_vec2(delta);
    let sensitivity = 0.01; // Scale factor

    for (networked, mut transform) in cube_query.iter_mut() {
        if lock_registry.is_locked_by(networked.network_id, node_id, node_id) {
            transform.translation.x += bevy_delta.x * sensitivity;
            transform.translation.y -= bevy_delta.y * sensitivity; // Invert Y for screen coords
        }
    }
}

/// Apply RotateEntity action to locked cubes
fn apply_rotate_entity(
    delta: glam::Vec2,
    lock_registry: &EntityLockRegistry,
    node_id: uuid::Uuid,
    cube_query: &mut Query<(&NetworkedEntity, &mut Transform), With<crate::cube::CubeMarker>>,
) {
    let bevy_delta = to_bevy_vec2(delta);
    let sensitivity = 0.01;

    for (networked, mut transform) in cube_query.iter_mut() {
        if lock_registry.is_locked_by(networked.network_id, node_id, node_id) {
            let rotation_x = Quat::from_rotation_y(bevy_delta.x * sensitivity);
            let rotation_y = Quat::from_rotation_x(-bevy_delta.y * sensitivity);
            transform.rotation = rotation_x * transform.rotation * rotation_y;
        }
    }
}

/// Apply MoveEntityDepth action to locked cubes
fn apply_move_depth(
    delta: f32,
    lock_registry: &EntityLockRegistry,
    node_id: uuid::Uuid,
    cube_query: &mut Query<(&NetworkedEntity, &mut Transform), With<crate::cube::CubeMarker>>,
) {
    let sensitivity = 0.1;

    for (networked, mut transform) in cube_query.iter_mut() {
        if lock_registry.is_locked_by(networked.network_id, node_id, node_id) {
            transform.translation.z += delta * sensitivity;
        }
    }
}

/// Apply ResetEntity action to locked cubes
fn apply_reset_entity(
    lock_registry: &EntityLockRegistry,
    node_id: uuid::Uuid,
    cube_query: &mut Query<(&NetworkedEntity, &mut Transform), With<crate::cube::CubeMarker>>,
) {
    for (networked, mut transform) in cube_query.iter_mut() {
        if lock_registry.is_locked_by(networked.network_id, node_id, node_id) {
            transform.translation = Vec3::ZERO;
            transform.rotation = Quat::IDENTITY;
        }
    }
}

/// A 3D ray for raycasting
struct Ray {
    origin: Vec3,
    direction: Vec3,
}

/// Convert screen coordinates to a world-space ray from the camera
fn screen_to_world_ray(
    screen_pos: glam::Vec2,
    camera: &Camera,
    camera_transform: &GlobalTransform,
    _window: &Window,
) -> Option<Ray> {
    // Convert screen position to viewport position (0..1 range)
    let viewport_pos = Vec2::new(screen_pos.x, screen_pos.y);

    // Use Bevy's viewport_to_world method
    let ray_bevy = camera
        .viewport_to_world(camera_transform, viewport_pos)
        .ok()?;

    Some(Ray {
        origin: ray_bevy.origin,
        direction: *ray_bevy.direction,
    })
}

/// Test ray-AABB (axis-aligned bounding box) intersection
///
/// Returns the distance along the ray if there's an intersection, None
/// otherwise.
fn ray_aabb_intersection(
    ray_origin: Vec3,
    ray_direction: Vec3,
    aabb_center: Vec3,
    aabb_half_extents: Vec3,
) -> Option<f32> {
    // Calculate AABB min and max
    let aabb_min = aabb_center - aabb_half_extents;
    let aabb_max = aabb_center + aabb_half_extents;

    // Slab method for ray-AABB intersection
    let mut tmin = f32::NEG_INFINITY;
    let mut tmax = f32::INFINITY;

    for i in 0..3 {
        let origin_component = ray_origin[i];
        let dir_component = ray_direction[i];
        let min_component = aabb_min[i];
        let max_component = aabb_max[i];

        if dir_component.abs() < f32::EPSILON {
            // Ray is parallel to slab, check if origin is within slab
            if origin_component < min_component || origin_component > max_component {
                return None;
            }
        } else {
            // Compute intersection t values for near and far plane
            let inv_dir = 1.0 / dir_component;
            let mut t1 = (min_component - origin_component) * inv_dir;
            let mut t2 = (max_component - origin_component) * inv_dir;

            // Ensure t1 is the near intersection
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }

            // Update tmin and tmax
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);

            // Check for intersection failure
            if tmin > tmax {
                return None;
            }
        }
    }

    // If tmin is negative, the ray origin is inside the AABB
    // Return tmax in that case, otherwise return tmin
    if tmin < 0.0 {
        if tmax < 0.0 {
            return None; // AABB is behind the ray
        }
        Some(tmax)
    } else {
        Some(tmin)
    }
}

/// System to update visual appearance based on lock state
///
/// Color scheme:
/// - Green: Locked by us (we can edit)
/// - Blue: Locked by someone else (they can edit, we can't)
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
            // Locked by someone else - blue
            Color::srgb(0.3, 0.5, 0.9)
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
