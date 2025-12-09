//! Bevy plugin for CRDT networking
//!
//! This module provides a complete Bevy plugin that integrates all networking
//! components: delta generation, operation log, anti-entropy, join protocol,
//! tombstones, and CRDT types.
//!
//! # Quick Start
//!
//! ```no_run
//! use bevy::prelude::*;
//! use lib::networking::{NetworkingPlugin, NetworkingConfig};
//! use uuid::Uuid;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins(DefaultPlugins)
//!         .add_plugins(NetworkingPlugin::new(NetworkingConfig {
//!             node_id: Uuid::new_v4(),
//!             sync_interval_secs: 10.0,
//!             prune_interval_secs: 60.0,
//!             tombstone_gc_interval_secs: 300.0,
//!         }))
//!         .run();
//! }
//! ```

use bevy::prelude::*;

use crate::networking::{
    change_detection::{
        auto_detect_transform_changes_system,
        LastSyncVersions,
    },
    delta_generation::{
        generate_delta_system,
        NodeVectorClock,
    },
    entity_map::{
        cleanup_despawned_entities_system,
        register_networked_entities_system,
        NetworkEntityMap,
    },
    message_dispatcher::message_dispatcher_system,
    operation_log::{
        periodic_sync_system,
        prune_operation_log_system,
        OperationLog,
    },
    tombstones::{
        garbage_collect_tombstones_system,
        handle_local_deletions_system,
        TombstoneRegistry,
    },
    vector_clock::NodeId,
};

/// Configuration for the networking plugin
#[derive(Debug, Clone)]
pub struct NetworkingConfig {
    /// Unique ID for this node
    pub node_id: NodeId,

    /// How often to send SyncRequest for anti-entropy (in seconds)
    /// Default: 10.0 seconds
    pub sync_interval_secs: f32,

    /// How often to prune old operations from the log (in seconds)
    /// Default: 60.0 seconds (1 minute)
    pub prune_interval_secs: f32,

    /// How often to garbage collect tombstones (in seconds)
    /// Default: 300.0 seconds (5 minutes)
    pub tombstone_gc_interval_secs: f32,
}

impl Default for NetworkingConfig {
    fn default() -> Self {
        Self {
            node_id: uuid::Uuid::new_v4(),
            sync_interval_secs: 10.0,
            prune_interval_secs: 60.0,
            tombstone_gc_interval_secs: 300.0,
        }
    }
}

/// Bevy plugin for CRDT networking
///
/// This plugin sets up all systems and resources needed for distributed
/// synchronization using CRDTs.
///
/// # Systems Added
///
/// ## PreUpdate
/// - Register newly spawned networked entities
/// - **Central message dispatcher** (handles all incoming messages efficiently)
///   - EntityDelta messages
///   - JoinRequest messages
///   - FullState messages
///   - SyncRequest messages
///   - MissingDeltas messages
///
/// ## Update
/// - Handle local entity deletions
///
/// ## PostUpdate
/// - Generate and broadcast EntityDelta for changed entities
/// - Periodic SyncRequest for anti-entropy
/// - Prune old operations from operation log
/// - Garbage collect tombstones
/// - Cleanup despawned entities from entity map
///
/// # Resources Added
///
/// - `NodeVectorClock` - This node's vector clock
/// - `NetworkEntityMap` - Bidirectional entity ID mapping
/// - `LastSyncVersions` - Change detection for entities
/// - `OperationLog` - Operation log for anti-entropy
/// - `TombstoneRegistry` - Tombstone tracking for deletions
///
/// # Example
///
/// ```no_run
/// use bevy::prelude::*;
/// use lib::networking::{NetworkingPlugin, NetworkingConfig};
/// use uuid::Uuid;
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(NetworkingPlugin::new(NetworkingConfig {
///         node_id: Uuid::new_v4(),
///         ..Default::default()
///     }))
///     .run();
/// ```
pub struct NetworkingPlugin {
    config: NetworkingConfig,
}

impl NetworkingPlugin {
    /// Create a new networking plugin with custom configuration
    pub fn new(config: NetworkingConfig) -> Self {
        Self { config }
    }

    /// Create a new networking plugin with default configuration
    pub fn default_with_node_id(node_id: NodeId) -> Self {
        Self {
            config: NetworkingConfig {
                node_id,
                ..Default::default()
            },
        }
    }
}

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        // Add resources
        app.insert_resource(NodeVectorClock::new(self.config.node_id))
            .insert_resource(NetworkEntityMap::new())
            .insert_resource(LastSyncVersions::default())
            .insert_resource(OperationLog::new())
            .insert_resource(TombstoneRegistry::new())
            .insert_resource(crate::networking::ComponentVectorClocks::new());

        // PreUpdate systems - handle incoming messages first
        app.add_systems(
            PreUpdate,
            (
                // Register new networked entities
                register_networked_entities_system,
                // Central message dispatcher - handles all incoming messages
                // This replaces the individual message handling systems and
                // eliminates O(n²) behavior from multiple systems polling the same queue
                message_dispatcher_system,
            )
                .chain(),
        );

        // Update systems - handle local operations
        app.add_systems(
            Update,
            (
                // Track Transform changes and mark NetworkedTransform as changed
                auto_detect_transform_changes_system,
                // Handle local entity deletions
                handle_local_deletions_system,
            ),
        );

        // PostUpdate systems - generate and send deltas
        app.add_systems(
            PostUpdate,
            (
                // Generate deltas for changed entities
                generate_delta_system,
                // Periodic anti-entropy sync
                periodic_sync_system,
                // Maintenance tasks
                prune_operation_log_system,
                garbage_collect_tombstones_system,
                // Cleanup despawned entities
                cleanup_despawned_entities_system,
            ),
        );

        info!(
            "NetworkingPlugin initialized for node {}",
            self.config.node_id
        );
        info!(
            "Sync interval: {}s, Prune interval: {}s, GC interval: {}s",
            self.config.sync_interval_secs,
            self.config.prune_interval_secs,
            self.config.tombstone_gc_interval_secs
        );
    }
}

/// Extension trait for App to add networking more ergonomically
///
/// # Example
///
/// ```no_run
/// use bevy::prelude::*;
/// use lib::networking::NetworkingAppExt;
/// use uuid::Uuid;
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_networking(Uuid::new_v4())
///     .run();
/// ```
pub trait NetworkingAppExt {
    /// Add networking with default configuration and specified node ID
    fn add_networking(&mut self, node_id: NodeId) -> &mut Self;

    /// Add networking with custom configuration
    fn add_networking_with_config(&mut self, config: NetworkingConfig) -> &mut Self;
}

impl NetworkingAppExt for App {
    fn add_networking(&mut self, node_id: NodeId) -> &mut Self {
        self.add_plugins(NetworkingPlugin::default_with_node_id(node_id))
    }

    fn add_networking_with_config(&mut self, config: NetworkingConfig) -> &mut Self {
        self.add_plugins(NetworkingPlugin::new(config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_networking_config_default() {
        let config = NetworkingConfig::default();
        assert_eq!(config.sync_interval_secs, 10.0);
        assert_eq!(config.prune_interval_secs, 60.0);
        assert_eq!(config.tombstone_gc_interval_secs, 300.0);
    }

    #[test]
    fn test_networking_plugin_creation() {
        let node_id = uuid::Uuid::new_v4();
        let plugin = NetworkingPlugin::default_with_node_id(node_id);
        assert_eq!(plugin.config.node_id, node_id);
    }

    #[test]
    fn test_networking_plugin_build() {
        let mut app = App::new();
        let node_id = uuid::Uuid::new_v4();

        app.add_plugins(NetworkingPlugin::default_with_node_id(node_id));

        // Verify resources were added
        assert!(app.world().get_resource::<NodeVectorClock>().is_some());
        assert!(app.world().get_resource::<NetworkEntityMap>().is_some());
        assert!(app.world().get_resource::<LastSyncVersions>().is_some());
        assert!(app.world().get_resource::<OperationLog>().is_some());
        assert!(app.world().get_resource::<TombstoneRegistry>().is_some());
    }

    #[test]
    fn test_app_extension_trait() {
        let mut app = App::new();
        let node_id = uuid::Uuid::new_v4();

        app.add_networking(node_id);

        // Verify resources were added
        assert!(app.world().get_resource::<NodeVectorClock>().is_some());
        assert!(app.world().get_resource::<NetworkEntityMap>().is_some());
    }
}
