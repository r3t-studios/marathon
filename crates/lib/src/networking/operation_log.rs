//! Operation log for anti-entropy and partition recovery
//!
//! This module maintains a bounded log of recent operations for each entity,
//! enabling peers to request missing deltas after network partitions or when
//! they join late.
//!
//! The operation log:
//! - Stores EntityDelta messages for recent operations
//! - Bounded by time (keep operations from last N minutes) or size (max M ops)
//! - Allows peers to request operations newer than their vector clock
//! - Supports periodic anti-entropy sync to repair partitions

use std::collections::{
    HashMap,
    VecDeque,
};

use bevy::prelude::*;

use crate::networking::{
    messages::{
        EntityDelta,
        SyncMessage,
        VersionedMessage,
    },
    vector_clock::{
        NodeId,
        VectorClock,
    },
    GossipBridge,
    NodeVectorClock,
};

/// Maximum operations to keep per entity (prevents unbounded growth)
const MAX_OPS_PER_ENTITY: usize = 100;

/// Maximum age for operations (in seconds)
const MAX_OP_AGE_SECS: u64 = 300; // 5 minutes

/// Maximum number of entities to track (prevents unbounded growth)
const MAX_ENTITIES: usize = 10_000;

/// Operation log entry with timestamp
#[derive(Debug, Clone)]
struct LogEntry {
    /// The entity delta operation
    delta: EntityDelta,

    /// When this operation was created (for pruning old ops)
    timestamp: std::time::Instant,
}

/// Resource storing the operation log for all entities
///
/// This is used for anti-entropy - peers can request operations they're missing
/// by comparing vector clocks.
///
/// # Bounded Growth
///
/// The operation log is bounded in three ways:
/// - Max operations per entity: `MAX_OPS_PER_ENTITY` (100)
/// - Max operation age: `MAX_OP_AGE_SECS` (300 seconds / 5 minutes)
/// - Max entities: `MAX_ENTITIES` (10,000)
///
/// When limits are exceeded, oldest operations/entities are pruned automatically.
#[derive(Resource)]
pub struct OperationLog {
    /// Map from entity ID to list of recent operations
    logs: HashMap<uuid::Uuid, VecDeque<LogEntry>>,

    /// Total number of operations across all entities (for monitoring)
    total_ops: usize,
}

impl OperationLog {
    /// Create a new operation log
    pub fn new() -> Self {
        Self {
            logs: HashMap::new(),
            total_ops: 0,
        }
    }

    /// Record an operation in the log
    ///
    /// This should be called whenever we generate or apply an EntityDelta.
    ///
    /// # Example
    ///
    /// ```
    /// use lib::networking::{OperationLog, EntityDelta, VectorClock};
    /// use uuid::Uuid;
    ///
    /// let mut log = OperationLog::new();
    /// let entity_id = Uuid::new_v4();
    /// let node_id = Uuid::new_v4();
    /// let clock = VectorClock::new();
    ///
    /// let delta = EntityDelta::new(entity_id, node_id, clock, vec![]);
    /// log.record_operation(delta);
    /// ```
    pub fn record_operation(&mut self, delta: EntityDelta) {
        // Check if we're at the entity limit
        if self.logs.len() >= MAX_ENTITIES && !self.logs.contains_key(&delta.entity_id) {
            // Prune oldest entity (by finding entity with oldest operation)
            if let Some(oldest_entity_id) = self.find_oldest_entity() {
                warn!(
                    "Operation log at entity limit ({}), pruning oldest entity {:?}",
                    MAX_ENTITIES, oldest_entity_id
                );
                if let Some(removed_log) = self.logs.remove(&oldest_entity_id) {
                    self.total_ops = self.total_ops.saturating_sub(removed_log.len());
                }
            }
        }

        let entry = LogEntry {
            delta: delta.clone(),
            timestamp: std::time::Instant::now(),
        };

        let log = self.logs.entry(delta.entity_id).or_insert_with(VecDeque::new);
        log.push_back(entry);
        self.total_ops += 1;

        // Prune if we exceed max ops per entity
        while log.len() > MAX_OPS_PER_ENTITY {
            log.pop_front();
            self.total_ops = self.total_ops.saturating_sub(1);
        }
    }

    /// Find the entity with the oldest operation (for LRU eviction)
    fn find_oldest_entity(&self) -> Option<uuid::Uuid> {
        self.logs
            .iter()
            .filter_map(|(entity_id, log)| {
                log.front().map(|entry| (*entity_id, entry.timestamp))
            })
            .min_by_key(|(_, timestamp)| *timestamp)
            .map(|(entity_id, _)| entity_id)
    }

    /// Get operations for an entity that are newer than a given vector clock
    ///
    /// This is used to respond to SyncRequest messages.
    pub fn get_operations_newer_than(
        &self,
        entity_id: uuid::Uuid,
        their_clock: &VectorClock,
    ) -> Vec<EntityDelta> {
        let Some(log) = self.logs.get(&entity_id) else {
            return vec![];
        };

        log.iter()
            .filter(|entry| {
                // Include operation if they haven't seen it yet
                // (their clock happened before the operation's clock)
                their_clock.happened_before(&entry.delta.vector_clock)
            })
            .map(|entry| entry.delta.clone())
            .collect()
    }

    /// Get all operations newer than a vector clock across all entities
    ///
    /// This is used to respond to SyncRequest for the entire world state.
    pub fn get_all_operations_newer_than(&self, their_clock: &VectorClock) -> Vec<EntityDelta> {
        let mut deltas = Vec::new();

        for (entity_id, _log) in &self.logs {
            let entity_deltas = self.get_operations_newer_than(*entity_id, their_clock);
            deltas.extend(entity_deltas);
        }

        deltas
    }

    /// Prune old operations from the log
    ///
    /// This should be called periodically to prevent unbounded growth.
    /// Removes operations older than MAX_OP_AGE_SECS.
    pub fn prune_old_operations(&mut self) {
        let max_age = std::time::Duration::from_secs(MAX_OP_AGE_SECS);
        let now = std::time::Instant::now();

        let mut pruned_count = 0;

        for log in self.logs.values_mut() {
            let before_len = log.len();
            log.retain(|entry| {
                now.duration_since(entry.timestamp) < max_age
            });
            pruned_count += before_len - log.len();
        }

        // Update total_ops counter
        self.total_ops = self.total_ops.saturating_sub(pruned_count);

        // Remove empty logs
        self.logs.retain(|_, log| !log.is_empty());
    }

    /// Get the number of operations in the log
    pub fn total_operations(&self) -> usize {
        self.total_ops
    }

    /// Get the number of entities with logged operations
    pub fn num_entities(&self) -> usize {
        self.logs.len()
    }
}

impl Default for OperationLog {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a SyncRequest message
///
/// This asks peers to send us any operations we're missing.
///
/// # Example
///
/// ```
/// use lib::networking::{build_sync_request, VectorClock};
/// use uuid::Uuid;
///
/// let node_id = Uuid::new_v4();
/// let clock = VectorClock::new();
/// let request = build_sync_request(node_id, clock);
/// ```
pub fn build_sync_request(node_id: NodeId, vector_clock: VectorClock) -> VersionedMessage {
    VersionedMessage::new(SyncMessage::SyncRequest {
        node_id,
        vector_clock,
    })
}

/// Build a MissingDeltas response
///
/// This contains operations that the requesting peer is missing.
pub fn build_missing_deltas(deltas: Vec<EntityDelta>) -> VersionedMessage {
    VersionedMessage::new(SyncMessage::MissingDeltas { deltas })
}

/// System to handle SyncRequest messages
///
/// When we receive a SyncRequest, compare vector clocks and send any
/// operations the peer is missing.
///
/// Add this to your app:
///
/// ```no_run
/// use bevy::prelude::*;
/// use lib::networking::handle_sync_requests_system;
///
/// App::new()
///     .add_systems(Update, handle_sync_requests_system);
/// ```
pub fn handle_sync_requests_system(
    bridge: Option<Res<GossipBridge>>,
    operation_log: Res<OperationLog>,
) {
    let Some(bridge) = bridge else {
        return;
    };

    // Poll for SyncRequest messages
    while let Some(message) = bridge.try_recv() {
        match message.message {
            | SyncMessage::SyncRequest {
                node_id: requesting_node,
                vector_clock: their_clock,
            } => {
                debug!("Received SyncRequest from node {}", requesting_node);

                // Find operations they're missing
                let missing_deltas = operation_log.get_all_operations_newer_than(&their_clock);

                if !missing_deltas.is_empty() {
                    info!(
                        "Sending {} missing deltas to node {}",
                        missing_deltas.len(),
                        requesting_node
                    );

                    // Send MissingDeltas response
                    let response = build_missing_deltas(missing_deltas);
                    if let Err(e) = bridge.send(response) {
                        error!("Failed to send MissingDeltas: {}", e);
                    }
                } else {
                    debug!("No missing deltas for node {}", requesting_node);
                }
            }
            | _ => {
                // Not a SyncRequest, ignore
            }
        }
    }
}

/// System to handle MissingDeltas messages
///
/// When we receive MissingDeltas (in response to our SyncRequest), apply them.
pub fn handle_missing_deltas_system(
    mut commands: Commands,
    bridge: Option<Res<GossipBridge>>,
    mut entity_map: ResMut<crate::networking::NetworkEntityMap>,
    type_registry: Res<AppTypeRegistry>,
    mut node_clock: ResMut<NodeVectorClock>,
    blob_store: Option<Res<crate::networking::BlobStore>>,
    mut tombstone_registry: Option<ResMut<crate::networking::TombstoneRegistry>>,
) {
    let Some(bridge) = bridge else {
        return;
    };

    let registry = type_registry.read();
    let blob_store_ref = blob_store.as_deref();

    // Poll for MissingDeltas messages
    while let Some(message) = bridge.try_recv() {
        match message.message {
            | SyncMessage::MissingDeltas { deltas } => {
                info!("Received MissingDeltas with {} operations", deltas.len());

                // Apply each delta
                for delta in deltas {
                    debug!(
                        "Applying missing delta for entity {:?}",
                        delta.entity_id
                    );

                    crate::networking::apply_entity_delta(
                        &delta,
                        &mut commands,
                        &mut entity_map,
                        &registry,
                        &mut node_clock,
                        blob_store_ref,
                        tombstone_registry.as_deref_mut(),
                    );
                }
            }
            | _ => {
                // Not MissingDeltas, ignore
            }
        }
    }
}

/// System to periodically send SyncRequest for anti-entropy
///
/// This runs every N seconds to request any operations we might be missing,
/// helping to repair network partitions.
///
/// **NOTE:** This is a simple timer-based implementation. Phase 14 will add
/// adaptive sync intervals based on network conditions.
pub fn periodic_sync_system(
    bridge: Option<Res<GossipBridge>>,
    node_clock: Res<NodeVectorClock>,
    time: Res<Time>,
    mut last_sync: Local<f32>,
) {
    let Some(bridge) = bridge else {
        return;
    };

    // Sync every 10 seconds
    const SYNC_INTERVAL: f32 = 10.0;

    *last_sync += time.delta_secs();

    if *last_sync >= SYNC_INTERVAL {
        *last_sync = 0.0;

        debug!("Sending periodic SyncRequest for anti-entropy");

        let request = build_sync_request(node_clock.node_id, node_clock.clock.clone());
        if let Err(e) = bridge.send(request) {
            error!("Failed to send SyncRequest: {}", e);
        }
    }
}

/// System to prune old operations from the log
///
/// This runs periodically to remove operations older than MAX_OP_AGE_SECS.
pub fn prune_operation_log_system(
    mut operation_log: ResMut<OperationLog>,
    time: Res<Time>,
    mut last_prune: Local<f32>,
) {
    // Prune every 60 seconds
    const PRUNE_INTERVAL: f32 = 60.0;

    *last_prune += time.delta_secs();

    if *last_prune >= PRUNE_INTERVAL {
        *last_prune = 0.0;

        let before = operation_log.total_operations();
        operation_log.prune_old_operations();
        let after = operation_log.total_operations();

        if before != after {
            debug!(
                "Pruned operation log: {} ops -> {} ops",
                before, after
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_log_creation() {
        let log = OperationLog::new();
        assert_eq!(log.num_entities(), 0);
        assert_eq!(log.total_operations(), 0);
    }

    #[test]
    fn test_record_operation() {
        let mut log = OperationLog::new();
        let entity_id = uuid::Uuid::new_v4();
        let node_id = uuid::Uuid::new_v4();
        let clock = VectorClock::new();

        let delta = EntityDelta::new(entity_id, node_id, clock, vec![]);
        log.record_operation(delta);

        assert_eq!(log.num_entities(), 1);
        assert_eq!(log.total_operations(), 1);
    }

    #[test]
    fn test_get_operations_newer_than() {
        let mut log = OperationLog::new();
        let entity_id = uuid::Uuid::new_v4();
        let node_id = uuid::Uuid::new_v4();

        // Create two operations with different clocks
        let mut clock1 = VectorClock::new();
        clock1.increment(node_id);

        let mut clock2 = VectorClock::new();
        clock2.increment(node_id);
        clock2.increment(node_id);

        let delta1 = EntityDelta::new(entity_id, node_id, clock1.clone(), vec![]);
        let delta2 = EntityDelta::new(entity_id, node_id, clock2.clone(), vec![]);

        log.record_operation(delta1);
        log.record_operation(delta2);

        // Request with clock1 should get delta2
        let newer = log.get_operations_newer_than(entity_id, &clock1);
        assert_eq!(newer.len(), 1);
        assert_eq!(newer[0].vector_clock, clock2);

        // Request with clock2 should get nothing
        let newer = log.get_operations_newer_than(entity_id, &clock2);
        assert_eq!(newer.len(), 0);
    }

    #[test]
    fn test_max_ops_per_entity() {
        let mut log = OperationLog::new();
        let entity_id = uuid::Uuid::new_v4();
        let node_id = uuid::Uuid::new_v4();

        // Add more than MAX_OPS_PER_ENTITY operations
        for _ in 0..(MAX_OPS_PER_ENTITY + 10) {
            let mut clock = VectorClock::new();
            clock.increment(node_id);
            let delta = EntityDelta::new(entity_id, node_id, clock, vec![]);
            log.record_operation(delta);
        }

        // Should be capped at MAX_OPS_PER_ENTITY
        assert_eq!(log.total_operations(), MAX_OPS_PER_ENTITY);
    }

    #[test]
    fn test_build_sync_request() {
        let node_id = uuid::Uuid::new_v4();
        let clock = VectorClock::new();

        let request = build_sync_request(node_id, clock.clone());

        match request.message {
            | SyncMessage::SyncRequest {
                node_id: req_node_id,
                vector_clock,
            } => {
                assert_eq!(req_node_id, node_id);
                assert_eq!(vector_clock, clock);
            }
            | _ => panic!("Expected SyncRequest"),
        }
    }

    #[test]
    fn test_build_missing_deltas() {
        let entity_id = uuid::Uuid::new_v4();
        let node_id = uuid::Uuid::new_v4();
        let clock = VectorClock::new();

        let delta = EntityDelta::new(entity_id, node_id, clock, vec![]);
        let response = build_missing_deltas(vec![delta.clone()]);

        match response.message {
            | SyncMessage::MissingDeltas { deltas } => {
                assert_eq!(deltas.len(), 1);
                assert_eq!(deltas[0].entity_id, entity_id);
            }
            | _ => panic!("Expected MissingDeltas"),
        }
    }
}
