//! Cube entity management

use bevy::prelude::*;
use libmarathon::networking::{NetworkEntityMap, Synced};
use uuid::Uuid;

/// Marker component for the replicated cube
///
/// This component contains all the data needed for rendering a cube.
/// The `#[synced]` attribute automatically handles network synchronization.
#[libmarathon_macros::synced]
pub struct CubeMarker {
    /// RGB color values (0.0 to 1.0)
    pub color_r: f32,
    pub color_g: f32,
    pub color_b: f32,
    pub size: f32,
}

impl CubeMarker {
    pub fn with_color(color: Color, size: f32) -> Self {
        let [r, g, b, _] = color.to_linear().to_f32_array();
        Self {
            color_r: r,
            color_g: g,
            color_b: b,
            size,
        }
    }

    pub fn color(&self) -> Color {
        Color::srgb(self.color_r, self.color_g, self.color_b)
    }
}

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
        app.add_message::<SpawnCubeEvent>()
            .add_message::<DeleteCubeEvent>()
            .add_systems(Update, (
                handle_spawn_cube,
                handle_delete_cube,
                add_cube_rendering_system,  // Custom rendering!
            ));
    }
}

/// Custom rendering system - detects Added<CubeMarker> and adds mesh/material
fn add_cube_rendering_system(
    mut commands: Commands,
    query: Query<(Entity, &CubeMarker), Added<CubeMarker>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, cube) in &query {
        commands.entity(entity).insert((
            Mesh3d(meshes.add(Cuboid::new(cube.size, cube.size, cube.size))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: cube.color(),  // Use the color() helper method
                perceptual_roughness: 0.7,
                metallic: 0.3,
                ..default()
            })),
        ));
    }
}

/// Handle cube spawn messages
fn handle_spawn_cube(
    mut commands: Commands,
    mut messages: MessageReader<SpawnCubeEvent>,
) {
    for event in messages.read() {
        info!("Spawning cube at {:?}", event.position);

        commands.spawn((
            CubeMarker::with_color(Color::srgb(0.8, 0.3, 0.6), 1.0),
            Transform::from_translation(event.position),
            GlobalTransform::default(),
            Synced,  // Auto-adds NetworkedEntity, Persisted, NetworkedTransform
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
            info!("Marking cube {} for deletion", event.entity_id);
            // Add ToDelete marker - the handle_local_deletions_system will:
            // 1. Increment vector clock
            // 2. Create Delete operation
            // 3. Record tombstone
            // 4. Broadcast deletion to peers
            // 5. Despawn entity locally
            commands.entity(bevy_entity).insert(libmarathon::networking::ToDelete);
        } else {
            warn!("Attempted to delete unknown cube {}", event.entity_id);
        }
    }
}
