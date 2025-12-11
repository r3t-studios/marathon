//! Mouse input handling for macOS

use bevy::prelude::*;
use bevy::input::mouse::{MouseMotion, MouseWheel};

pub struct MouseInputPlugin;

impl Plugin for MouseInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, handle_mouse_input);
    }
}

/// Mouse interaction state
#[derive(Resource, Default)]
struct MouseState {
    /// Whether the left mouse button is currently pressed
    left_pressed: bool,
    /// Whether the right mouse button is currently pressed
    right_pressed: bool,
}

/// Handle mouse input to move and rotate the cube
fn handle_mouse_input(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: EventReader<MouseMotion>,
    mut mouse_wheel: EventReader<MouseWheel>,
    mut mouse_state: Local<Option<MouseState>>,
    mut cube_query: Query<&mut Transform, With<crate::cube::CubeMarker>>,
) {
    // Initialize mouse state if needed
    if mouse_state.is_none() {
        *mouse_state = Some(MouseState::default());
    }
    let state = mouse_state.as_mut().unwrap();

    // Update button states
    state.left_pressed = mouse_buttons.pressed(MouseButton::Left);
    state.right_pressed = mouse_buttons.pressed(MouseButton::Right);

    // Get total mouse delta this frame
    let mut total_delta = Vec2::ZERO;
    for motion in mouse_motion.read() {
        total_delta += motion.delta;
    }

    // Process mouse motion
    if total_delta != Vec2::ZERO {
        for mut transform in cube_query.iter_mut() {
            if state.left_pressed {
                // Left drag: Move cube in XY plane
                // Scale factor for sensitivity
                let sensitivity = 0.01;
                transform.translation.x += total_delta.x * sensitivity;
                transform.translation.y -= total_delta.y * sensitivity; // Invert Y
            } else if state.right_pressed {
                // Right drag: Rotate cube
                let sensitivity = 0.01;
                let rotation_x = Quat::from_rotation_y(total_delta.x * sensitivity);
                let rotation_y = Quat::from_rotation_x(-total_delta.y * sensitivity);
                transform.rotation = rotation_x * transform.rotation * rotation_y;
            }
        }
    }

    // Process mouse wheel for Z-axis movement
    let mut total_scroll = 0.0;
    for wheel in mouse_wheel.read() {
        total_scroll += wheel.y;
    }

    if total_scroll != 0.0 {
        for mut transform in cube_query.iter_mut() {
            // Scroll: Move in Z axis
            let sensitivity = 0.1;
            transform.translation.z += total_scroll * sensitivity;
        }
    }
}
