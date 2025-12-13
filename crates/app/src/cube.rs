//! Cube entity management

use bevy::prelude::*;
use libmarathon::{
    networking::{
        NetworkEntityMap,
        NetworkedEntity,
        NetworkedSelection,
        NetworkedTransform,
        NodeVectorClock,
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

/// Message to spawn a new cube at a specific position
#[derive(Message)]
pub struct SpawnCubeEvent {
    pub position: Vec3,
}

/// Message to delete a cube by its network ID
#[derive(Message)]
pub struct DeleteCubeEvent {
    pub entity_id: Uuid,
}

pub struct CubePlugin;

impl Plugin for CubePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<CubeMarker>()
            .add_message::<SpawnCubeEvent>()
            .add_message::<DeleteCubeEvent>()
            .add_systems(Update, (handle_spawn_cube, handle_delete_cube));
    }
}

/// Handle cube spawn messages
fn handle_spawn_cube(
    mut commands: Commands,
    mut messages: MessageReader<SpawnCubeEvent>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    node_clock: Res<NodeVectorClock>,
) {
    for event in messages.read() {
        let entity_id = Uuid::new_v4();
        let node_id = node_clock.node_id;

        info!("Spawning cube {} at {:?}", entity_id, event.position);

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
            Transform::from_translation(event.position),
            GlobalTransform::default(),
            // Networking
            NetworkedEntity::with_id(entity_id, node_id),
            NetworkedTransform,
            NetworkedSelection::default(),
            // Persistence
            Persisted::with_id(entity_id),
            // Sync marker
            Synced,
        ));
    }
}

/// Handle cube delete messages
fn handle_delete_cube(
    mut commands: Commands,
    mut messages: MessageReader<DeleteCubeEvent>,
    entity_map: Res<NetworkEntityMap>,
) {
    for event in messages.read() {
        if let Some(bevy_entity) = entity_map.get_entity(event.entity_id) {
            info!("Deleting cube {}", event.entity_id);
            commands.entity(bevy_entity).despawn();
        } else {
            warn!("Attempted to delete unknown cube {}", event.entity_id);
        }
    }
}
