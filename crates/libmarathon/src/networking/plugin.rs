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
//! use libmarathon::networking::{
//!     NetworkingConfig,
//!     NetworkingPlugin,
//! };
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
        LastSyncVersions,
        auto_detect_transform_changes_system,
    },
    components::{NetworkedEntity, NetworkedTransform},
    delta_generation::{
        NodeVectorClock,
        generate_delta_system,
    },
    entity_map::{
        NetworkEntityMap,
        cleanup_despawned_entities_system,
        register_networked_entities_system,
    },
    gossip_bridge::GossipBridge,
    locks::{
        EntityLockRegistry,
        acquire_locks_on_selection_system,
        broadcast_lock_heartbeats_system,
        cleanup_expired_locks_system,
        release_locks_on_deselection_system,
    },
    message_dispatcher::message_dispatcher_system,
    operation_log::{
        OperationLog,
        periodic_sync_system,
        prune_operation_log_system,
    },
    session_lifecycle::{
        initialize_session_system,
        save_session_on_shutdown_system,
    },
    sync_component::Synced,
    tombstones::{
        TombstoneRegistry,
        garbage_collect_tombstones_system,
        handle_local_deletions_system,
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

/// Optional session secret for authentication
///
/// This is a pre-shared secret that controls access to the gossip network.
/// If configured, all joining nodes must provide the correct session secret
/// to receive the full state.
///
/// # Security Model
///
/// The session secret provides network-level access control by:
/// - Preventing unauthorized nodes from joining the gossip
/// - Hash-based comparison prevents timing attacks
/// - Works alongside iroh-gossip's built-in QUIC transport encryption
///
/// # Usage
///
/// Insert this as a Bevy resource to enable session secret validation:
///
/// ```no_run
/// use bevy::prelude::*;
/// use libmarathon::networking::{
///     NetworkingPlugin,
///     SessionSecret,
/// };
/// use uuid::Uuid;
///
/// App::new()
///     .add_plugins(NetworkingPlugin::default_with_node_id(Uuid::new_v4()))
///     .insert_resource(SessionSecret::new(b"my_secret_key"))
///     .run();
/// ```
#[derive(Resource, Clone)]
pub struct SessionSecret(bytes::Bytes);

impl SessionSecret {
    /// Create a new session secret from bytes
    pub fn new(secret: impl Into<Vec<u8>>) -> Self {
        Self(bytes::Bytes::from(secret.into()))
    }

    /// Get the secret as a byte slice
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// System that auto-inserts required sync components when `Synced` marker is detected.
///
/// This system runs in PreUpdate and automatically adds:
/// - `NetworkedEntity` with a new UUID and node ID
/// - `Persisted` with the same UUID
/// - `NetworkedTransform` if the entity has a `Transform` component
///
/// Note: Selection is now a global `LocalSelection` resource, not a per-entity component.
///
/// This eliminates the need for users to manually add these components when spawning synced entities.
fn auto_insert_sync_components(
    mut commands: Commands,
    query: Query<Entity, (Added<Synced>, Without<NetworkedEntity>)>,
    node_clock: Res<NodeVectorClock>,
    // We need access to check if entity has Transform
    transforms: Query<&Transform>,
) {
    for entity in &query {
        let entity_id = uuid::Uuid::new_v4();
        let node_id = node_clock.node_id;

        // Always add NetworkedEntity and Persisted
        let mut entity_commands = commands.entity(entity);
        entity_commands.insert((
            NetworkedEntity::with_id(entity_id, node_id),
            crate::persistence::Persisted::with_id(entity_id),
        ));

        // Auto-add NetworkedTransform if entity has Transform
        if transforms.contains(entity) {
            entity_commands.insert(NetworkedTransform);
        }

        debug!("Auto-inserted sync components for entity {:?} (UUID: {})", entity, entity_id);
    }
}

/// System that adds NetworkedTransform to networked entities when Transform is added.
///
/// This handles entities received from the network that already have NetworkedEntity,
/// Persisted, and Synced, but need NetworkedTransform when Transform is added.
fn auto_insert_networked_transform(
    mut commands: Commands,
    query: Query<
        Entity,
        (
            With<NetworkedEntity>,
            With<Synced>,
            Added<Transform>,
            Without<NetworkedTransform>,
        ),
    >,
) {
    for entity in &query {
        commands.entity(entity).insert(NetworkedTransform);
        debug!("Auto-inserted NetworkedTransform for networked entity {:?}", entity);
    }
}

/// System that triggers anti-entropy sync when going online (GossipBridge added).
///
/// This handles the offline-to-online transition: when GossipBridge is inserted,
/// we immediately send a SyncRequest to trigger anti-entropy and broadcast all
/// operations from the operation log.
///
/// Uses a Local resource to track if we've already sent the sync request, so this only runs once.
fn trigger_sync_on_connect(
    mut has_synced: Local<bool>,
    bridge: Res<GossipBridge>,
    node_clock: Res<NodeVectorClock>,
    operation_log: Res<OperationLog>,
) {
    if *has_synced {
        return; // Already did this
    }

    let op_count = operation_log.total_operations();
    debug!(
        "Going online: triggering anti-entropy sync to broadcast {} offline operations",
        op_count
    );

    // Send a SyncRequest to trigger anti-entropy
    // This will cause the message_dispatcher to respond with all operations from our log
    let request = crate::networking::operation_log::build_sync_request(
        node_clock.node_id,
        node_clock.clock.clone(),
    );

    if let Err(e) = bridge.send(request) {
        error!("Failed to send SyncRequest on connect: {}", e);
    } else {
        debug!("Sent SyncRequest to trigger anti-entropy sync");
    }

    *has_synced = true;
}

/// Bevy plugin for CRDT networking
///
/// This plugin sets up all systems and resources needed for distributed
/// synchronization using CRDTs.
///
/// # Systems Added
///
/// ## Startup
/// - Initialize or restore session from persistence (auto-rejoin)
///
/// ## PreUpdate
/// - Register newly spawned networked entities
/// - **Central message dispatcher** (handles all incoming messages efficiently)
///   - EntityDelta messages
///   - JoinRequest messages
///   - FullState messages
///   - SyncRequest messages
///   - MissingDeltas messages
///   - Lock messages (LockRequest, LockAcquired, LockRejected, LockHeartbeat, LockRelease, LockReleased)
///
/// ## Update
/// - Auto-detect Transform changes
/// - Handle local entity deletions
/// - Release locks when entities are deselected
///
/// ## PostUpdate
/// - Generate and broadcast EntityDelta for changed entities
/// - Periodic SyncRequest for anti-entropy
/// - Broadcast lock heartbeats to maintain active locks
/// - Prune old operations from operation log
/// - Garbage collect tombstones
/// - Cleanup expired locks (5-second timeout)
/// - Cleanup despawned entities from entity map
///
/// ## Last
/// - Save session state and vector clock to persistence
///
/// # Resources Added
///
/// - `NodeVectorClock` - This node's vector clock
/// - `NetworkEntityMap` - Bidirectional entity ID mapping
/// - `LastSyncVersions` - Change detection for entities
/// - `OperationLog` - Operation log for anti-entropy
/// - `TombstoneRegistry` - Tombstone tracking for deletions
/// - `EntityLockRegistry` - Entity lock registry with heartbeat tracking
///
/// # Example
///
/// ```no_run
/// use bevy::prelude::*;
/// use libmarathon::networking::{
///     NetworkingConfig,
///     NetworkingPlugin,
/// };
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
            .insert_resource(EntityLockRegistry::new())
            .insert_resource(crate::networking::ComponentVectorClocks::new())
            .insert_resource(crate::networking::LocalSelection::new());

        // Startup systems - initialize session from persistence
        app.add_systems(Startup, initialize_session_system);

        // PreUpdate systems - handle incoming messages first
        app.add_systems(
            PreUpdate,
            (
                // Auto-insert sync components when Synced marker is added (must run first)
                auto_insert_sync_components,
                // Register new networked entities
                register_networked_entities_system,
                // Central message dispatcher - handles all incoming messages
                // This replaces the individual message handling systems and
                // eliminates O(n²) behavior from multiple systems polling the same queue
                message_dispatcher_system,
                // Auto-insert NetworkedTransform for networked entities when Transform is added
                auto_insert_networked_transform,
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
                // Acquire locks when entities are selected
                acquire_locks_on_selection_system,
                // Release locks when entities are deselected
                release_locks_on_deselection_system,
            ),
        );

        // Trigger anti-entropy sync when going online (separate from chain to allow conditional execution)
        app.add_systems(
            PostUpdate,
            trigger_sync_on_connect
                .run_if(bevy::ecs::schedule::common_conditions::resource_exists::<GossipBridge>),
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
                cleanup_expired_locks_system,
                // Cleanup despawned entities
                cleanup_despawned_entities_system,
            ),
        );

        // Broadcast lock heartbeats every 1 second to maintain active locks
        app.add_systems(
            PostUpdate,
            broadcast_lock_heartbeats_system.run_if(bevy::time::common_conditions::on_timer(
                std::time::Duration::from_secs(1),
            )),
        );

        // Auto-save session state every 5 seconds
        app.add_systems(
            Last,
            save_session_on_shutdown_system.run_if(bevy::time::common_conditions::on_timer(
                std::time::Duration::from_secs(5),
            )),
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
/// use libmarathon::networking::NetworkingAppExt;
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
        assert!(app.world().get_resource::<EntityLockRegistry>().is_some());
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
