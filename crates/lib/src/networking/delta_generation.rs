//! Delta generation system for broadcasting entity changes
//!
//! This module implements the core delta generation logic that detects changed
//! entities and broadcasts EntityDelta messages.

use bevy::prelude::*;

use crate::networking::{
    change_detection::LastSyncVersions,
    entity_map::NetworkEntityMap,
    gossip_bridge::GossipBridge,
    messages::{
        EntityDelta,
        SyncMessage,
        VersionedMessage,
    },
    operation_builder::build_entity_operations,
    vector_clock::{
        NodeId,
        VectorClock,
    },
    NetworkedEntity,
};

/// Resource wrapping our node's vector clock
///
/// This tracks the logical time for our local operations.
#[derive(Resource)]
pub struct NodeVectorClock {
    pub node_id: NodeId,
    pub clock: VectorClock,
}

impl NodeVectorClock {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            clock: VectorClock::new(),
        }
    }

    /// Increment our clock for a new operation
    pub fn tick(&mut self) -> u64 {
        self.clock.increment(self.node_id)
    }

    /// Get current sequence number for our node
    pub fn sequence(&self) -> u64 {
        self.clock.get(self.node_id)
    }
}

/// System to generate and broadcast EntityDelta messages
///
/// This system:
/// 1. Queries for Changed<NetworkedEntity>
/// 2. Serializes all components on those entities
/// 3. Builds EntityDelta messages
/// 4. Broadcasts via GossipBridge
///
/// Add this to your app to enable delta broadcasting:
///
/// ```no_run
/// use bevy::prelude::*;
/// use lib::networking::generate_delta_system;
///
/// App::new()
///     .add_systems(Update, generate_delta_system);
/// ```
pub fn generate_delta_system(
    query: Query<(Entity, &NetworkedEntity), Changed<NetworkedEntity>>,
    world: &World,
    type_registry: Res<AppTypeRegistry>,
    mut node_clock: ResMut<NodeVectorClock>,
    mut last_versions: ResMut<LastSyncVersions>,
    bridge: Option<Res<GossipBridge>>,
    _entity_map: Res<NetworkEntityMap>,
    mut operation_log: Option<ResMut<crate::networking::OperationLog>>,
) {
    // Early return if no gossip bridge
    let Some(bridge) = bridge else {
        return;
    };

    let registry = type_registry.read();

    for (entity, networked) in query.iter() {
        // Check if we should sync this entity
        let current_seq = node_clock.sequence();
        if !last_versions.should_sync(networked.network_id, current_seq) {
            continue;
        }

        // Increment our vector clock
        node_clock.tick();

        // Build operations for all components
        // TODO: Add BlobStore support in future phases
        let operations = build_entity_operations(
            entity,
            world,
            node_clock.node_id,
            node_clock.clock.clone(),
            &registry,
            None, // blob_store - will be added in later phases
        );

        if operations.is_empty() {
            continue;
        }

        // Create EntityDelta
        let delta = EntityDelta::new(
            networked.network_id,
            node_clock.node_id,
            node_clock.clock.clone(),
            operations,
        );

        // Record in operation log for anti-entropy
        if let Some(ref mut log) = operation_log {
            log.record_operation(delta.clone());
        }

        // Wrap in VersionedMessage
        let message = VersionedMessage::new(SyncMessage::EntityDelta {
            entity_id: delta.entity_id,
            node_id: delta.node_id,
            vector_clock: delta.vector_clock.clone(),
            operations: delta.operations.clone(),
        });

        // Broadcast
        if let Err(e) = bridge.send(message) {
            error!("Failed to broadcast EntityDelta: {}", e);
        } else {
            debug!(
                "Broadcast EntityDelta for entity {:?} with {} operations",
                networked.network_id,
                delta.operations.len()
            );

            // Update last sync version
            last_versions.update(networked.network_id, current_seq);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_vector_clock_creation() {
        let node_id = uuid::Uuid::new_v4();
        let clock = NodeVectorClock::new(node_id);

        assert_eq!(clock.node_id, node_id);
        assert_eq!(clock.sequence(), 0);
    }

    #[test]
    fn test_node_vector_clock_tick() {
        let node_id = uuid::Uuid::new_v4();
        let mut clock = NodeVectorClock::new(node_id);

        assert_eq!(clock.tick(), 1);
        assert_eq!(clock.sequence(), 1);

        assert_eq!(clock.tick(), 2);
        assert_eq!(clock.sequence(), 2);
    }

    #[test]
    fn test_node_vector_clock_multiple_nodes() {
        let node1 = uuid::Uuid::new_v4();
        let node2 = uuid::Uuid::new_v4();

        let mut clock1 = NodeVectorClock::new(node1);
        let mut clock2 = NodeVectorClock::new(node2);

        clock1.tick();
        clock2.tick();

        assert_eq!(clock1.sequence(), 1);
        assert_eq!(clock2.sequence(), 1);

        // Merge clocks
        clock1.clock.merge(&clock2.clock);
        assert_eq!(clock1.clock.get(node1), 1);
        assert_eq!(clock1.clock.get(node2), 1);
    }
}
