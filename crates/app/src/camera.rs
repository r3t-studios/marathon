//! Camera configuration
//!
//! This module handles the 3D camera setup for the cube demo.

use bevy::prelude::*;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_camera);
    }
}

/// Set up the 3D camera
///
/// Camera is positioned at (4, 3, 6) looking at the cube's initial position (0,
/// 0.5, 0). This provides a good viewing angle to see the cube, ground plane,
/// and any movements.
fn setup_camera(mut commands: Commands) {
    info!("Setting up camera");

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(4.0, 3.0, 6.0).looking_at(Vec3::new(0.0, 0.5, 0.0), Vec3::Y),
    ));
}
