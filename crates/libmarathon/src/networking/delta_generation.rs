//! Delta generation system for broadcasting entity changes
//!
//! This module implements the core delta generation logic that detects changed
//! entities and broadcasts EntityDelta messages.

use bevy::prelude::*;

use crate::networking::{
    NetworkedEntity,
    change_detection::LastSyncVersions,
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
/// 1. Queries for Added<NetworkedEntity> or Changed<NetworkedEntity>
/// 2. Serializes all components on those entities
/// 3. Builds EntityDelta messages
/// 4. Broadcasts via GossipBridge
///
/// Add this to your app to enable delta broadcasting:
///
/// ```no_run
/// use bevy::prelude::*;
/// use libmarathon::networking::generate_delta_system;
///
/// App::new().add_systems(Update, generate_delta_system);
/// ```
pub fn generate_delta_system(world: &mut World) {
    // Works both online and offline - clock increments and operations are recorded
    // Broadcast only happens when online

    let changed_entities: Vec<(Entity, uuid::Uuid, uuid::Uuid)> = {
        let mut query = world.query_filtered::<
            (Entity, &NetworkedEntity),
            (
                Or<(Added<NetworkedEntity>, Changed<NetworkedEntity>)>,
                Without<crate::networking::SkipNextDeltaGeneration>,
            ),
        >();
        query
            .iter(world)
            .map(|(entity, networked)| (entity, networked.network_id, networked.owner_node_id))
            .collect()
    };

    if changed_entities.is_empty() {
        return;
    }

    debug!(
        "generate_delta_system: Processing {} changed entities",
        changed_entities.len()
    );

    // Process each entity separately to avoid borrow conflicts
    for (entity, network_id, _owner_node_id) in changed_entities {
        // Phase 1: Check and update clocks, collect data
        let mut system_state: bevy::ecs::system::SystemState<(
            Option<Res<GossipBridge>>,
            Res<crate::persistence::ComponentTypeRegistryResource>,
            ResMut<NodeVectorClock>,
            ResMut<LastSyncVersions>,
            Option<ResMut<crate::networking::OperationLog>>,
        )> = bevy::ecs::system::SystemState::new(world);

        let (node_id, vector_clock, new_seq) = {
            let (_, _, mut node_clock, last_versions, _) = system_state.get_mut(world);

            // Check if we should sync this entity with the NEXT sequence (after tick)
            // This prevents duplicate sends when system runs multiple times per frame
            let current_seq = node_clock.sequence();
            let next_seq = current_seq + 1; // What the sequence will be after tick
            if !last_versions.should_sync(network_id, next_seq) {
                drop(last_versions);
                drop(node_clock);
                system_state.apply(world);
                continue;
            }

            // Increment our vector clock and get the NEW sequence
            let new_seq = node_clock.tick();
            debug_assert_eq!(new_seq, next_seq, "tick() should return next_seq");

            (node_clock.node_id, node_clock.clock.clone(), new_seq)
        };

        // Phase 2: Build operations (needs world access without holding other borrows)
        let operations = {
            let type_registry_res = world.resource::<crate::persistence::ComponentTypeRegistryResource>();
            let type_registry = type_registry_res.0;
            build_entity_operations(
                entity,
                world,
                node_id,
                vector_clock.clone(),
                type_registry,
                None, // blob_store - will be added in later phases
            )
        };

        if operations.is_empty() {
            system_state.apply(world);
            continue;
        }

        // Phase 3: Record, broadcast, and update
        let delta = {
            let (bridge, _, _, mut last_versions, mut operation_log) = system_state.get_mut(world);

            // Create EntityDelta
            let delta = EntityDelta::new(network_id, node_id, vector_clock.clone(), operations);

            // Record in operation log for anti-entropy (works offline!)
            if let Some(ref mut log) = operation_log {
                log.record_operation(delta.clone());
            }

            // Broadcast if online
            if let Some(ref bridge) = bridge {
                // Wrap in VersionedMessage
                let message = VersionedMessage::new(SyncMessage::EntityDelta {
                    entity_id: delta.entity_id,
                    node_id: delta.node_id,
                    vector_clock: delta.vector_clock.clone(),
                    operations: delta.operations.clone(),
                });

                // Broadcast to peers
                if let Err(e) = bridge.send(message) {
                    error!("Failed to broadcast EntityDelta: {}", e);
                } else {
                    debug!(
                        "Broadcast EntityDelta for entity {:?} with {} operations",
                        network_id,
                        delta.operations.len()
                    );
                }
            } else {
                debug!(
                    "Generated EntityDelta for entity {:?} offline (will sync when online)",
                    network_id
                );
            }

            // Update last sync version with NEW sequence (after tick) to prevent duplicates
            // CRITICAL: Must use new_seq (after tick), not current_seq (before tick)
            // This prevents sending duplicate deltas if system runs multiple times per frame
            last_versions.update(network_id, new_seq);

            delta
        };

        // Phase 4: Update component vector clocks for local modifications
        {
            // Get type registry first before mutable borrow
            let type_registry = {
                let type_registry_res = world.resource::<crate::persistence::ComponentTypeRegistryResource>();
                type_registry_res.0
            };
            
            if let Some(mut component_clocks) =
                world.get_resource_mut::<crate::networking::ComponentVectorClocks>()
            {
                for op in &delta.operations {
                    if let crate::networking::ComponentOp::Set {
                        discriminant,
                        vector_clock: op_clock,
                        ..
                    } = op
                    {
                        let component_type_name = type_registry.get_type_name(*discriminant)
                            .unwrap_or("unknown");
                            
                        component_clocks.set(
                            network_id,
                            component_type_name.to_string(),
                            op_clock.clone(),
                            node_id,
                        );
                        debug!(
                            "Updated local vector clock for {} on entity {:?} (node_id: {:?})",
                            component_type_name, network_id, node_id
                        );
                    }
                }
            }
        }

        system_state.apply(world);
    }
}

/// Remove SkipNextDeltaGeneration markers after delta generation has run
///
/// This system must run AFTER `generate_delta_system` to allow entities to be
/// synced again on the next actual local change. The marker prevents feedback
/// loops by skipping entities that just received remote updates, but we need
/// to remove it so future local changes get broadcast.
///
/// Add this to your app after generate_delta_system:
///
/// ```no_run
/// use bevy::prelude::*;
/// use libmarathon::networking::{generate_delta_system, cleanup_skip_delta_markers_system};
///
/// App::new().add_systems(PostUpdate, (
///     generate_delta_system,
///     cleanup_skip_delta_markers_system,
/// ).chain());
/// ```
pub fn cleanup_skip_delta_markers_system(world: &mut World) {
    // Use immediate removal (not deferred commands) to ensure markers are removed
    // synchronously after generate_delta_system runs, not at the start of next frame
    let entities_to_clean: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, With<crate::networking::SkipNextDeltaGeneration>>();
        query.iter(world).collect()
    };

    for entity in &entities_to_clean {
        if let Ok(mut entity_mut) = world.get_entity_mut(*entity) {
            entity_mut.remove::<crate::networking::SkipNextDeltaGeneration>();
        }
    }

    if !entities_to_clean.is_empty() {
        debug!(
            "cleanup_skip_delta_markers_system: Removed markers from {} entities",
            entities_to_clean.len()
        );
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
