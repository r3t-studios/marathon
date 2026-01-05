//! Entity tombstone tracking for deletion semantics
//!
//! This module manages tombstones for deleted entities, preventing resurrection
//! and supporting eventual garbage collection.
//!
//! ## Deletion Semantics
//!
//! When an entity is deleted:
//! 1. A Delete operation is generated with current vector clock
//! 2. The entity is marked as deleted (tombstone) in TombstoneRegistry
//! 3. The tombstone is propagated to all peers
//! 4. Operations older than the deletion are ignored
//! 5. After a grace period, tombstones can be garbage collected
//!
//! ## Resurrection Prevention
//!
//! If a peer creates an entity (Set operation) while another peer deletes it:
//! - Use vector clock comparison: if delete happened-after create, deletion
//!   wins
//! - If concurrent, deletion wins (delete bias for safety)
//! - This prevents "zombie" entities from reappearing
//!
//! ## Garbage Collection
//!
//! Tombstones are kept for a configurable period (default: 1 hour) to handle
//! late-arriving operations. After this period, they can be safely removed.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::networking::{
    GossipBridge,
    NodeVectorClock,
    vector_clock::{
        NodeId,
        VectorClock,
    },
};

/// How long to keep tombstones before garbage collection (in seconds)
const TOMBSTONE_TTL_SECS: u64 = 3600; // 1 hour

/// A tombstone record for a deleted entity
#[derive(Debug, Clone)]
pub struct Tombstone {
    /// The entity that was deleted
    pub entity_id: uuid::Uuid,

    /// Node that initiated the deletion
    pub deleting_node: NodeId,

    /// Vector clock when deletion occurred
    pub deletion_clock: VectorClock,

    /// When this tombstone was created (for garbage collection)
    pub timestamp: std::time::Instant,
}

/// Resource tracking tombstones for deleted entities
///
/// This prevents deleted entities from being resurrected by late-arriving
/// create operations.
#[derive(Resource, Default)]
pub struct TombstoneRegistry {
    /// Map from entity ID to tombstone
    tombstones: HashMap<uuid::Uuid, Tombstone>,
}

impl TombstoneRegistry {
    /// Create a new tombstone registry
    pub fn new() -> Self {
        Self {
            tombstones: HashMap::new(),
        }
    }

    /// Check if an entity is deleted
    pub fn is_deleted(&self, entity_id: uuid::Uuid) -> bool {
        self.tombstones.contains_key(&entity_id)
    }

    /// Get the tombstone for an entity, if it exists
    pub fn get_tombstone(&self, entity_id: uuid::Uuid) -> Option<&Tombstone> {
        self.tombstones.get(&entity_id)
    }

    /// Record a deletion
    ///
    /// This creates a tombstone for the entity. If a tombstone already exists
    /// and the new deletion has a later clock, it replaces the old one.
    pub fn record_deletion(
        &mut self,
        entity_id: uuid::Uuid,
        deleting_node: NodeId,
        deletion_clock: VectorClock,
    ) {
        // Check if we already have a tombstone
        if let Some(existing) = self.tombstones.get(&entity_id) {
            // Only update if the new deletion is later
            // (new deletion happened-after existing = existing happened-before new)
            if existing.deletion_clock.happened_before(&deletion_clock) {
                self.tombstones.insert(
                    entity_id,
                    Tombstone {
                        entity_id,
                        deleting_node,
                        deletion_clock,
                        timestamp: std::time::Instant::now(),
                    },
                );
                debug!("Updated tombstone for entity {:?}", entity_id);
            } else {
                debug!(
                    "Ignoring older or concurrent deletion for entity {:?}",
                    entity_id
                );
            }
        } else {
            // New tombstone
            self.tombstones.insert(
                entity_id,
                Tombstone {
                    entity_id,
                    deleting_node,
                    deletion_clock,
                    timestamp: std::time::Instant::now(),
                },
            );
            info!("Created tombstone for entity {:?}", entity_id);
        }
    }

    /// Check if an operation should be ignored because the entity is deleted
    ///
    /// Returns true if:
    /// - The entity has a tombstone AND
    /// - The operation's clock happened-before or is concurrent with the
    ///   deletion
    ///
    /// This prevents operations on deleted entities from being applied.
    pub fn should_ignore_operation(
        &self,
        entity_id: uuid::Uuid,
        operation_clock: &VectorClock,
    ) -> bool {
        if let Some(tombstone) = self.tombstones.get(&entity_id) {
            // If operation happened-before deletion, ignore it
            // operation_clock.happened_before(deletion_clock) => ignore

            // If deletion happened-before operation, don't ignore (resurrection)
            // deletion_clock.happened_before(operation_clock) => don't ignore

            // If concurrent, deletion wins (delete bias) => ignore
            // !operation_clock.happened_before(deletion_clock) &&
            // !deletion_clock.happened_before(operation_clock) => ignore

            // So we DON'T ignore only if deletion happened-before operation
            !tombstone.deletion_clock.happened_before(operation_clock)
        } else {
            false
        }
    }

    /// Remove old tombstones that are past their TTL
    ///
    /// This should be called periodically to prevent unbounded growth.
    pub fn garbage_collect(&mut self) {
        let ttl = std::time::Duration::from_secs(TOMBSTONE_TTL_SECS);
        let now = std::time::Instant::now();

        let before_count = self.tombstones.len();

        self.tombstones
            .retain(|_, tombstone| now.duration_since(tombstone.timestamp) < ttl);

        let after_count = self.tombstones.len();

        if before_count != after_count {
            info!(
                "Garbage collected {} tombstones ({} -> {})",
                before_count - after_count,
                before_count,
                after_count
            );
        }
    }

    /// Get the number of tombstones
    pub fn num_tombstones(&self) -> usize {
        self.tombstones.len()
    }
}

/// System to handle entity deletions initiated locally
///
/// This system watches for entities with the `ToDelete` marker component
/// and generates Delete operations for them.
///
/// # Usage
///
/// To delete an entity, add the `ToDelete` component:
///
/// ```no_run
/// use bevy::prelude::*;
/// use libmarathon::networking::ToDelete;
///
/// fn delete_entity_system(mut commands: Commands, entity: Entity) {
///     commands.entity(entity).insert(ToDelete);
/// }
/// ```
#[derive(Component)]
pub struct ToDelete;

pub fn handle_local_deletions_system(
    mut commands: Commands,
    query: Query<(Entity, &crate::networking::NetworkedEntity), With<ToDelete>>,
    mut node_clock: ResMut<NodeVectorClock>,
    mut tombstone_registry: ResMut<TombstoneRegistry>,
    mut operation_log: Option<ResMut<crate::networking::OperationLog>>,
    bridge: Option<Res<GossipBridge>>,
    mut write_buffer: Option<ResMut<crate::persistence::WriteBufferResource>>,
) {
    for (entity, networked) in query.iter() {
        // Increment clock for deletion
        node_clock.tick();

        // Create Delete operation
        let delete_op = crate::networking::ComponentOpBuilder::new(
            node_clock.node_id,
            node_clock.clock.clone(),
        )
        .delete();

        // Record tombstone in memory
        tombstone_registry.record_deletion(
            networked.network_id,
            node_clock.node_id,
            node_clock.clock.clone(),
        );

        // Persist tombstone to database
        if let Some(ref mut buffer) = write_buffer {
            // Serialize the vector clock using rkyv
            match rkyv::to_bytes::<rkyv::rancor::Failure>(&node_clock.clock).map(|b| b.to_vec()) {
                Ok(clock_bytes) => {
                    if let Err(e) = buffer.add(crate::persistence::PersistenceOp::RecordTombstone {
                        entity_id: networked.network_id,
                        deleting_node: node_clock.node_id,
                        deletion_clock: bytes::Bytes::from(clock_bytes),
                    }) {
                        error!("Failed to persist tombstone for entity {:?}: {}", networked.network_id, e);
                    } else {
                        debug!("Persisted tombstone for entity {:?} to database", networked.network_id);
                    }
                },
                Err(e) => {
                    error!("Failed to serialize vector clock for tombstone persistence: {:?}", e);
                }
            }
        }

        // Create EntityDelta with Delete operation
        let delta = crate::networking::EntityDelta::new(
            networked.network_id,
            node_clock.node_id,
            node_clock.clock.clone(),
            vec![delete_op],
        );

        // Record in operation log (for when we go online later)
        if let Some(ref mut log) = operation_log {
            log.record_operation(delta.clone());
        }

        // Broadcast deletion if online
        if let Some(ref bridge) = bridge {
            let message =
                crate::networking::VersionedMessage::new(crate::networking::SyncMessage::EntityDelta {
                    entity_id: delta.entity_id,
                    node_id: delta.node_id,
                    vector_clock: delta.vector_clock.clone(),
                    operations: delta.operations.clone(),
                });

            if let Err(e) = bridge.send(message) {
                error!("Failed to broadcast Delete operation: {}", e);
            } else {
                info!(
                    "Broadcast Delete operation for entity {:?}",
                    networked.network_id
                );
            }
        } else {
            info!(
                "Deleted entity {:?} locally (offline mode - will sync when online)",
                networked.network_id
            );
        }

        // Despawn the entity locally
        commands.entity(entity).despawn();
    }
}

/// System to garbage collect old tombstones
///
/// This runs periodically to remove tombstones that are past their TTL.
pub fn garbage_collect_tombstones_system(
    mut tombstone_registry: ResMut<TombstoneRegistry>,
    time: Res<Time>,
    mut last_gc: Local<f32>,
) {
    // Garbage collect every 5 minutes
    const GC_INTERVAL: f32 = 300.0;

    *last_gc += time.delta_secs();

    if *last_gc >= GC_INTERVAL {
        *last_gc = 0.0;

        debug!("Running tombstone garbage collection");
        tombstone_registry.garbage_collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tombstone_registry_creation() {
        let registry = TombstoneRegistry::new();
        assert_eq!(registry.num_tombstones(), 0);
    }

    #[test]
    fn test_record_deletion() {
        let mut registry = TombstoneRegistry::new();
        let entity_id = uuid::Uuid::new_v4();
        let node_id = uuid::Uuid::new_v4();
        let clock = VectorClock::new();

        registry.record_deletion(entity_id, node_id, clock);

        assert!(registry.is_deleted(entity_id));
        assert_eq!(registry.num_tombstones(), 1);
    }

    #[test]
    fn test_should_ignore_older_operation() {
        let mut registry = TombstoneRegistry::new();
        let entity_id = uuid::Uuid::new_v4();
        let node_id = uuid::Uuid::new_v4();

        // Create deletion at clock = 2
        let mut deletion_clock = VectorClock::new();
        deletion_clock.increment(node_id);
        deletion_clock.increment(node_id);

        registry.record_deletion(entity_id, node_id, deletion_clock);

        // Operation at clock = 1 should be ignored
        let mut old_operation_clock = VectorClock::new();
        old_operation_clock.increment(node_id);

        assert!(registry.should_ignore_operation(entity_id, &old_operation_clock));
    }

    #[test]
    fn test_should_not_ignore_newer_operation() {
        let mut registry = TombstoneRegistry::new();
        let entity_id = uuid::Uuid::new_v4();
        let node_id = uuid::Uuid::new_v4();

        // Create deletion at clock = 1
        let mut deletion_clock = VectorClock::new();
        deletion_clock.increment(node_id);

        registry.record_deletion(entity_id, node_id, deletion_clock);

        // Operation at clock = 2 should NOT be ignored (resurrection)
        let mut new_operation_clock = VectorClock::new();
        new_operation_clock.increment(node_id);
        new_operation_clock.increment(node_id);

        assert!(!registry.should_ignore_operation(entity_id, &new_operation_clock));
    }

    #[test]
    fn test_concurrent_delete_wins() {
        let mut registry = TombstoneRegistry::new();
        let entity_id = uuid::Uuid::new_v4();
        let node1 = uuid::Uuid::new_v4();
        let node2 = uuid::Uuid::new_v4();

        // Node 1 deletes
        let mut delete_clock = VectorClock::new();
        delete_clock.increment(node1);

        registry.record_deletion(entity_id, node1, delete_clock);

        // Node 2 has concurrent operation
        let mut concurrent_clock = VectorClock::new();
        concurrent_clock.increment(node2);

        // Concurrent operation should be ignored (delete bias)
        assert!(registry.should_ignore_operation(entity_id, &concurrent_clock));
    }

    #[test]
    fn test_update_tombstone_with_later_deletion() {
        let mut registry = TombstoneRegistry::new();
        let entity_id = uuid::Uuid::new_v4();
        let node_id = uuid::Uuid::new_v4();

        // First deletion at clock = 1
        let mut clock1 = VectorClock::new();
        clock1.increment(node_id);
        registry.record_deletion(entity_id, node_id, clock1.clone());

        let tombstone1 = registry.get_tombstone(entity_id).unwrap();
        assert_eq!(tombstone1.deletion_clock, clock1);

        // Second deletion at clock = 2 (later)
        let mut clock2 = VectorClock::new();
        clock2.increment(node_id);
        clock2.increment(node_id);
        registry.record_deletion(entity_id, node_id, clock2.clone());

        let tombstone2 = registry.get_tombstone(entity_id).unwrap();
        assert_eq!(tombstone2.deletion_clock, clock2);
    }

    #[test]
    fn test_ignore_older_tombstone_update() {
        let mut registry = TombstoneRegistry::new();
        let entity_id = uuid::Uuid::new_v4();
        let node_id = uuid::Uuid::new_v4();

        // First deletion at clock = 2
        let mut clock2 = VectorClock::new();
        clock2.increment(node_id);
        clock2.increment(node_id);
        registry.record_deletion(entity_id, node_id, clock2.clone());

        // Try to record older deletion at clock = 1
        let mut clock1 = VectorClock::new();
        clock1.increment(node_id);
        registry.record_deletion(entity_id, node_id, clock1);

        // Should still have the newer tombstone
        let tombstone = registry.get_tombstone(entity_id).unwrap();
        assert_eq!(tombstone.deletion_clock, clock2);
    }
}
