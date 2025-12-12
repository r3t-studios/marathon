//! Cube entity management

use bevy::prelude::*;
use lib::{
    networking::{
        NetworkedEntity,
        NetworkedTransform,
        Synced,
    },
    persistence::Persisted,
};
use serde::{
    Deserialize,
    Serialize,
};
use uuid::Uuid;

/// Marker component for the replicated cube
#[derive(Component, Reflect, Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[reflect(Component)]
pub struct CubeMarker;

pub struct CubePlugin;

impl Plugin for CubePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<CubeMarker>()
            .add_systems(Startup, spawn_cube);
    }
}

/// Spawn the synced cube on startup
fn spawn_cube(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    node_clock: Option<Res<lib::networking::NodeVectorClock>>,
) {
    // Wait until NodeVectorClock is available (after networking plugin initializes)
    let Some(clock) = node_clock else {
        warn!("NodeVectorClock not ready, deferring cube spawn");
        return;
    };

    let entity_id = Uuid::new_v4();
    let node_id = clock.node_id;

    info!("Spawning cube with network ID: {}", entity_id);

    commands.spawn((
        CubeMarker,
        // Bevy 3D components
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.8, 0.3, 0.6),
            perceptual_roughness: 0.7,
            metallic: 0.3,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.5, 0.0),
        GlobalTransform::default(),
        // Networking
        NetworkedEntity::with_id(entity_id, node_id),
        NetworkedTransform,
        // Persistence
        Persisted::with_id(entity_id),
        // Sync marker
        Synced,
    ));

    info!("Cube spawned successfully");
}
