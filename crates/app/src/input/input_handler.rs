//! Input handling using engine GameActions
//!
//! Processes GameActions (from InputController) and applies them to game entities.

use bevy::prelude::*;
use libmarathon::{
    engine::{GameAction, InputController},
    networking::{EntityLockRegistry, NetworkedEntity, NodeVectorClock},
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
    lock_registry: Res<EntityLockRegistry>,
    node_clock: Res<NodeVectorClock>,
    mut cube_query: Query<(&NetworkedEntity, &mut Transform), With<crate::cube::CubeMarker>>,
) {
    let node_id = node_clock.node_id;

    // Process all input events through the controller to get game actions
    let mut all_actions = Vec::new();
    for event in input_buffer.events() {
        let actions = controller_res.controller.process_event(event);
        all_actions.extend(actions);
    }

    // Apply game actions to entities
    for action in all_actions {
        match action {
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
