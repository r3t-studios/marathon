//! Input handling using engine GameActions
//!
//! Processes GameActions (from InputController) and applies them to game entities.

use bevy::prelude::*;
use libmarathon::{
    engine::GameAction,
    platform::input::InputController,
    networking::{EntityLockRegistry, NetworkedEntity, NetworkedSelection, NodeVectorClock},
};

use super::event_buffer::InputEventBuffer;

pub struct InputHandlerPlugin;

impl Plugin for InputHandlerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InputControllerResource>()
            .add_systems(Update, handle_game_actions);
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
    mut lock_registry: ResMut<EntityLockRegistry>,
    node_clock: Res<NodeVectorClock>,
    mut cube_query: Query<(&NetworkedEntity, &mut Transform, &mut NetworkedSelection), With<crate::cube::CubeMarker>>,
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
            GameAction::SelectEntity { position } => {
                apply_select_entity(
                    position,
                    &mut lock_registry,
                    node_id,
                    &mut cube_query,
                    &camera_query,
                    &window_query,
                );
            }

            GameAction::MoveEntity { delta } => {
                apply_move_entity(delta, &lock_registry, node_id, &mut cube_query);
            }

            GameAction::RotateEntity { delta } => {
                apply_rotate_entity(delta, &lock_registry, node_id, &mut cube_query);
            }

            GameAction::MoveEntityDepth { delta } => {
                apply_move_depth(delta, &lock_registry, node_id, &mut cube_query);
            }

            GameAction::ResetEntity => {
                apply_reset_entity(&lock_registry, node_id, &mut cube_query);
            }

            _ => {
                // Other actions not yet implemented
            }
        }
    }
}

/// Apply SelectEntity action - raycast to find clicked cube and select it
fn apply_select_entity(
    position: glam::Vec2,
    lock_registry: &mut EntityLockRegistry,
    node_id: uuid::Uuid,
    cube_query: &mut Query<(&NetworkedEntity, &mut Transform, &mut NetworkedSelection), With<crate::cube::CubeMarker>>,
    camera_query: &Query<(&Camera, &GlobalTransform)>,
    window_query: &Query<&Window>,
) {
    // Get the camera and window
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };
    let Ok(window) = window_query.single() else {
        return;
    };

    // Convert screen position to world ray
    let Some(ray) = screen_to_world_ray(position, camera, camera_transform, window) else {
        return;
    };

    // Find the closest cube hit by the ray
    let mut closest_hit: Option<(uuid::Uuid, f32)> = None;

    for (networked, transform, _) in cube_query.iter() {
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

    // If we hit a cube, clear all selections and select this one
    if let Some((hit_entity_id, _)) = closest_hit {
        // Clear all previous selections and locks
        for (networked, _, mut selection) in cube_query.iter_mut() {
            selection.clear();
            lock_registry.release(networked.network_id, node_id);
        }

        // Select and lock the clicked cube
        for (networked, _, mut selection) in cube_query.iter_mut() {
            if networked.network_id == hit_entity_id {
                selection.add(hit_entity_id);
                let _ = lock_registry.try_acquire(hit_entity_id, node_id);
                info!("Selected cube {}", hit_entity_id);
                break;
            }
        }
    } else {
        // Clicked on empty space - deselect all
        for (networked, _, mut selection) in cube_query.iter_mut() {
            selection.clear();
            lock_registry.release(networked.network_id, node_id);
        }
        info!("Deselected all cubes");
    }
}

/// Apply MoveEntity action to locked cubes
fn apply_move_entity(
    delta: glam::Vec2,
    lock_registry: &EntityLockRegistry,
    node_id: uuid::Uuid,
    cube_query: &mut Query<(&NetworkedEntity, &mut Transform, &mut NetworkedSelection), With<crate::cube::CubeMarker>>,
) {
    let bevy_delta = to_bevy_vec2(delta);
    let sensitivity = 0.01; // Scale factor

    for (networked, mut transform, _) in cube_query.iter_mut() {
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
    cube_query: &mut Query<(&NetworkedEntity, &mut Transform, &mut NetworkedSelection), With<crate::cube::CubeMarker>>,
) {
    let bevy_delta = to_bevy_vec2(delta);
    let sensitivity = 0.01;

    for (networked, mut transform, _) in cube_query.iter_mut() {
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
    cube_query: &mut Query<(&NetworkedEntity, &mut Transform, &mut NetworkedSelection), With<crate::cube::CubeMarker>>,
) {
    let sensitivity = 0.1;

    for (networked, mut transform, _) in cube_query.iter_mut() {
        if lock_registry.is_locked_by(networked.network_id, node_id, node_id) {
            transform.translation.z += delta * sensitivity;
        }
    }
}

/// Apply ResetEntity action to locked cubes
fn apply_reset_entity(
    lock_registry: &EntityLockRegistry,
    node_id: uuid::Uuid,
    cube_query: &mut Query<(&NetworkedEntity, &mut Transform, &mut NetworkedSelection), With<crate::cube::CubeMarker>>,
) {
    for (networked, mut transform, _) in cube_query.iter_mut() {
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
    let ray_bevy = camera.viewport_to_world(camera_transform, viewport_pos).ok()?;

    Some(Ray {
        origin: ray_bevy.origin,
        direction: *ray_bevy.direction,
    })
}

/// Test ray-AABB (axis-aligned bounding box) intersection
///
/// Returns the distance along the ray if there's an intersection, None otherwise.
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
