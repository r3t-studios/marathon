//! Bevy plugin for the persistence layer
//!
//! This module provides a Bevy plugin that sets up all the necessary resources
//! and systems for the persistence layer.

use std::{
    ops::{
        Deref,
        DerefMut,
    },
    path::PathBuf,
};

use bevy::prelude::*;

use crate::persistence::*;

/// Bevy plugin for persistence
///
/// # Example
///
/// ```no_run
/// use bevy::prelude::*;
/// use libmarathon::persistence::PersistencePlugin;
///
/// App::new()
///     .add_plugins(PersistencePlugin::new("app.db"))
///     .run();
/// ```
pub struct PersistencePlugin {
    /// Path to the SQLite database file
    pub db_path: PathBuf,

    /// Persistence configuration
    pub config: PersistenceConfig,
}

impl PersistencePlugin {
    /// Create a new persistence plugin with default configuration
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            config: PersistenceConfig::default(),
        }
    }

    /// Create a new persistence plugin with custom configuration
    pub fn with_config(db_path: impl Into<PathBuf>, config: PersistenceConfig) -> Self {
        Self {
            db_path: db_path.into(),
            config,
        }
    }

    /// Load configuration from a TOML file
    pub fn with_config_file(
        db_path: impl Into<PathBuf>,
        config_path: impl AsRef<std::path::Path>,
    ) -> crate::persistence::error::Result<Self> {
        let config = load_config_from_file(config_path)?;
        Ok(Self {
            db_path: db_path.into(),
            config,
        })
    }
}

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        // Initialize database
        let db = PersistenceDb::from_path(&self.db_path)
            .expect("Failed to initialize persistence database");

        // Register types for reflection
        app.register_type::<Persisted>();

        // Add messages/events
        app.add_message::<PersistenceFailureEvent>()
            .add_message::<PersistenceRecoveryEvent>()
            .add_message::<AppLifecycleEvent>();

        // Insert resources
        app.insert_resource(db)
            .insert_resource(DirtyEntitiesResource::default())
            .insert_resource(WriteBufferResource::new(self.config.max_buffer_operations))
            .insert_resource(self.config.clone())
            .insert_resource(BatteryStatus::default())
            .insert_resource(PersistenceMetrics::default())
            .insert_resource(CheckpointTimer::default())
            .insert_resource(PersistenceHealth::default())
            .insert_resource(PendingFlushTasks::default())
            .init_resource::<ComponentTypeRegistryResource>();

        // Add startup systems
        // First initialize the database, then rehydrate entities
        app.add_systems(Startup, (
            persistence_startup_system,
            rehydrate_entities_system,
        ).chain());

        // Add systems in the appropriate schedule
        app.add_systems(
            Update,
            (
                lifecycle_event_system,
                collect_dirty_entities_bevy_system,
                flush_system,
                checkpoint_bevy_system,
            )
                .chain(),
        );
    }
}

/// Resource wrapper for DirtyEntities
#[derive(Resource, Default)]
pub struct DirtyEntitiesResource(pub DirtyEntities);

impl std::ops::Deref for DirtyEntitiesResource {
    type Target = DirtyEntities;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for DirtyEntitiesResource {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Resource wrapper for WriteBuffer
#[derive(Resource)]
pub struct WriteBufferResource(pub WriteBuffer);

impl WriteBufferResource {
    pub fn new(max_operations: usize) -> Self {
        Self(WriteBuffer::new(max_operations))
    }
}

impl std::ops::Deref for WriteBufferResource {
    type Target = WriteBuffer;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for WriteBufferResource {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Startup system to initialize persistence
fn persistence_startup_system(db: Res<PersistenceDb>, mut metrics: ResMut<PersistenceMetrics>) {
    if let Err(e) = startup_system(db.deref(), metrics.deref_mut()) {
        error!("Failed to initialize persistence: {}", e);
    } else {
        info!("Persistence system initialized");
    }
}

/// Exclusive startup system to rehydrate entities from database
///
/// This system runs after `persistence_startup_system` and loads all entities
/// from SQLite, deserializing and spawning them into the Bevy world with all
/// their components.
fn rehydrate_entities_system(world: &mut World) {
    if let Err(e) = crate::persistence::database::rehydrate_all_entities(world) {
        error!("Failed to rehydrate entities from database: {}", e);
    } else {
        info!("Successfully rehydrated entities from database");
    }
}

/// System to collect dirty entities using Bevy's change detection
///
/// This system tracks changes to the `Persisted` component. When `Persisted` is
/// marked as changed (via `mark_dirty()` or direct mutation), ALL components on
/// that entity are serialized and added to the write buffer.
///
/// For automatic tracking without manual `mark_dirty()` calls, use the
/// `auto_track_component_changes_system` which automatically detects changes
/// to common components like Transform, GlobalTransform, etc.
fn collect_dirty_entities_bevy_system(world: &mut World) {
    // Collect changed entities first
    let changed_entities: Vec<(Entity, uuid::Uuid)> = {
        let mut query = world.query_filtered::<(Entity, &Persisted), Changed<Persisted>>();
        query
            .iter(world)
            .map(|(entity, persisted)| (entity, persisted.network_id))
            .collect()
    };

    if changed_entities.is_empty() {
        return;
    }

    // Serialize components for each entity
    for (entity, network_id) in changed_entities {
        // First, ensure the entity exists in the database
        {
            let now = chrono::Utc::now();
            let mut write_buffer = world.resource_mut::<WriteBufferResource>();
            if let Err(e) = write_buffer.add(PersistenceOp::UpsertEntity {
                id: network_id,
                data: EntityData {
                    id: network_id,
                    created_at: now,
                    updated_at: now,
                    entity_type: "NetworkedEntity".to_string(),
                },
            }) {
                error!(
                    "Failed to add UpsertEntity operation for {}: {}",
                    network_id, e
                );
                return; // Skip this entity if we can't even add the entity op
            }
        }

        // Serialize all components on this entity (generic tracking)
        let components = {
            let type_registry_res = world.resource::<crate::persistence::ComponentTypeRegistryResource>();
            let type_registry = type_registry_res.0;
            type_registry.serialize_entity_components(world, entity)
        };

        // Add operations for each component
        for (_discriminant, type_path, data) in components {
            // Get mutable access to dirty and mark it
            {
                let mut dirty = world.resource_mut::<DirtyEntitiesResource>();
                dirty.mark_dirty(network_id, type_path);
            }

            // Get mutable access to write_buffer and add the operation
            {
                let mut write_buffer = world.resource_mut::<WriteBufferResource>();
                if let Err(e) = write_buffer.add(PersistenceOp::UpsertComponent {
                    entity_id: network_id,
                    component_type: type_path.to_string(),
                    data,
                }) {
                    error!(
                        "Failed to add UpsertComponent operation for entity {} component {}: {}",
                        network_id, type_path, e
                    );
                    // Continue with other components even if one fails
                }
            }
        }
    }
}

/// System to automatically track changes to common Bevy components
///
/// This system detects changes to Transform, automatically triggering
/// persistence by accessing `Persisted` mutably (which marks it as changed via
/// Bevy's change detection).
///
/// Add this system to your app if you want automatic persistence of Transform
/// changes:
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use libmarathon::persistence::*;
/// App::new()
///     .add_plugins(PersistencePlugin::new("app.db"))
///     .add_systems(Update, auto_track_transform_changes_system)
///     .run();
/// ```
pub fn auto_track_transform_changes_system(
    mut query: Query<&mut Persisted, (With<Transform>, Changed<Transform>)>,
) {
    // Simply accessing &mut Persisted triggers Bevy's change detection
    for _persisted in query.iter_mut() {
        // No-op - the mutable access itself marks Persisted as changed
    }
}

/// System to checkpoint the WAL
fn checkpoint_bevy_system(
    db: Res<PersistenceDb>,
    config: Res<PersistenceConfig>,
    mut timer: ResMut<CheckpointTimer>,
    mut metrics: ResMut<PersistenceMetrics>,
    mut health: ResMut<PersistenceHealth>,
) {
    match checkpoint_system(
        db.deref(),
        config.deref(),
        timer.deref_mut(),
        metrics.deref_mut(),
    ) {
        | Ok(_) => {
            health.record_checkpoint_success();
        },
        | Err(e) => {
            health.record_checkpoint_failure();
            error!(
                "Failed to checkpoint WAL (attempt {}): {}",
                health.consecutive_checkpoint_failures, e
            );
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_creation() {
        let plugin = PersistencePlugin::new("test.db");
        assert_eq!(plugin.db_path, PathBuf::from("test.db"));
    }

    #[test]
    fn test_plugin_with_config() {
        let mut config = PersistenceConfig::default();
        config.flush_interval_secs = 5;

        let plugin = PersistencePlugin::with_config("test.db", config);
        assert_eq!(plugin.config.flush_interval_secs, 5);
    }
}
