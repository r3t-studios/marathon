//! Apply remote operations to local ECS state
//!
//! This module handles incoming EntityDelta messages and applies them to the
//! local Bevy world using CRDT merge semantics.

use std::collections::HashMap;

use bevy::prelude::*;
use uuid::Uuid;

use crate::networking::{
    VectorClock,
    blob_support::{
        BlobStore,
        get_component_data,
    },
    delta_generation::NodeVectorClock,
    entity_map::NetworkEntityMap,
    merge::compare_operations_lww,
    messages::{
        ComponentData,
        EntityDelta,
        SyncMessage,
    },
    operations::ComponentOp,
};

/// Resource to track the last vector clock and originating node for each
/// component on each entity
///
/// This enables Last-Write-Wins conflict resolution by comparing incoming
/// operations' vector clocks with the current component's vector clock.
/// The node_id is used as a deterministic tiebreaker for concurrent operations.
#[derive(Resource, Default)]
pub struct ComponentVectorClocks {
    /// Maps (entity_network_id, component_type) -> (vector_clock,
    /// originating_node_id)
    clocks: HashMap<(Uuid, String), (VectorClock, Uuid)>,
}

impl ComponentVectorClocks {
    pub fn new() -> Self {
        Self {
            clocks: HashMap::new(),
        }
    }

    /// Get the current vector clock and node_id for a component
    pub fn get(&self, entity_id: Uuid, component_type: &str) -> Option<&(VectorClock, Uuid)> {
        self.clocks.get(&(entity_id, component_type.to_string()))
    }

    /// Update the vector clock and node_id for a component
    pub fn set(
        &mut self,
        entity_id: Uuid,
        component_type: String,
        clock: VectorClock,
        node_id: Uuid,
    ) {
        self.clocks
            .insert((entity_id, component_type), (clock, node_id));
    }

    /// Remove all clocks for an entity (when entity is deleted)
    pub fn remove_entity(&mut self, entity_id: Uuid) {
        self.clocks.retain(|(eid, _), _| *eid != entity_id);
    }
}

/// Apply an EntityDelta message to the local world
///
/// This function:
/// 1. Checks tombstone registry to prevent resurrection
/// 2. Looks up the entity by network_id
/// 3. Spawns a new entity if it doesn't exist
/// 4. Applies each ComponentOp using CRDT merge semantics
///
/// # Parameters
///
/// - `delta`: The EntityDelta to apply
/// - `world`: The Bevy world to apply changes to
pub fn apply_entity_delta(delta: &EntityDelta, world: &mut World) {
    // Validate and merge the remote vector clock
    {
        let mut node_clock = world.resource_mut::<NodeVectorClock>();

        // Check for clock regression (shouldn't happen in correct implementations)
        if delta.vector_clock.happened_before(&node_clock.clock) {
            warn!(
                "Received operation with clock from the past for entity {:?}. \
                 Remote clock happened before our clock. This may indicate clock issues.",
                delta.entity_id
            );
        }

        // Merge the remote vector clock into ours
        node_clock.clock.merge(&delta.vector_clock);
    }

    // Check if any operations are Delete operations
    for op in &delta.operations {
        if let crate::networking::ComponentOp::Delete { vector_clock } = op {
            // Record tombstone
            if let Some(mut registry) =
                world.get_resource_mut::<crate::networking::TombstoneRegistry>()
            {
                registry.record_deletion(delta.entity_id, delta.node_id, vector_clock.clone());

                // Despawn the entity if it exists locally
                let entity_to_despawn = {
                    let entity_map = world.resource::<NetworkEntityMap>();
                    entity_map.get_entity(delta.entity_id)
                };
                if let Some(entity) = entity_to_despawn {
                    world.despawn(entity);
                    let mut entity_map = world.resource_mut::<NetworkEntityMap>();
                    entity_map.remove_by_network_id(delta.entity_id);
                    info!(
                        "Despawned entity {:?} due to Delete operation",
                        delta.entity_id
                    );
                }

                // Don't process other operations - entity is deleted
                return;
            }
        }
    }

    // Check if we should ignore this delta due to deletion
    if let Some(registry) = world.get_resource::<crate::networking::TombstoneRegistry>() {
        if registry.should_ignore_operation(delta.entity_id, &delta.vector_clock) {
            debug!("Ignoring delta for deleted entity {:?}", delta.entity_id);
            return;
        }
    }

    let entity = {
        let entity_map = world.resource::<NetworkEntityMap>();
        if let Some(entity) = entity_map.get_entity(delta.entity_id) {
            entity
        } else {
            // Use shared helper to spawn networked entity with persistence
            crate::networking::spawn_networked_entity(world, delta.entity_id, delta.node_id)
        }
    };

    // Apply each operation (skip Delete operations - handled above)
    for op in &delta.operations {
        if !op.is_delete() {
            apply_component_op(entity, op, delta.node_id, world);
        }
    }

    // Trigger persistence by marking Persisted as changed
    // This ensures remote entities are persisted after sync
    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
        if let Some(mut persisted) = entity_mut.get_mut::<crate::persistence::Persisted>() {
            // Accessing &mut triggers Bevy's change detection
            let _ = &mut *persisted;
            debug!(
                "Triggered persistence for synced entity {:?}",
                delta.entity_id
            );
        }
    }
}

/// Apply a single ComponentOp to an entity
///
/// This dispatches to the appropriate CRDT merge logic based on the operation
/// type.
fn apply_component_op(entity: Entity, op: &ComponentOp, incoming_node_id: Uuid, world: &mut World) {
    match op {
        | ComponentOp::Set {
            discriminant,
            data,
            vector_clock,
        } => {
            apply_set_operation_with_lww(
                entity,
                *discriminant,
                data,
                vector_clock,
                incoming_node_id,
                world,
            );
        },
        | ComponentOp::SetAdd { discriminant, .. } => {
            // OR-Set add - Phase 10 provides OrSet<T> type
            // Application code should use OrSet in components and handle SetAdd/SetRemove
            // Full integration will be in Phase 12 plugin
            debug!(
                "SetAdd operation for discriminant {} (use OrSet<T> in components)",
                discriminant
            );
        },
        | ComponentOp::SetRemove { discriminant, .. } => {
            // OR-Set remove - Phase 10 provides OrSet<T> type
            // Application code should use OrSet in components and handle SetAdd/SetRemove
            // Full integration will be in Phase 12 plugin
            debug!(
                "SetRemove operation for discriminant {} (use OrSet<T> in components)",
                discriminant
            );
        },
        | ComponentOp::SequenceInsert { .. } => {
            // RGA insert - will be implemented in Phase 11
            debug!("SequenceInsert operation not yet implemented");
        },
        | ComponentOp::SequenceDelete { .. } => {
            // RGA delete - will be implemented in Phase 11
            debug!("SequenceDelete operation not yet implemented");
        },
        | ComponentOp::Delete { .. } => {
            // Entity deletion - will be implemented in Phase 9
            debug!("Delete operation not yet implemented");
        },
    }
}

/// Apply a Set operation with Last-Write-Wins conflict resolution
///
/// Compares the incoming vector clock with the stored clock for this component.
/// Only applies the operation if the incoming clock wins the LWW comparison.
/// Uses node_id as a deterministic tiebreaker for concurrent operations.
fn apply_set_operation_with_lww(
    entity: Entity,
    discriminant: u16,
    data: &ComponentData,
    incoming_clock: &VectorClock,
    incoming_node_id: Uuid,
    world: &mut World,
) {
    // Get component type name for logging and clock tracking
    let type_registry = {
        let registry_resource = world.resource::<crate::persistence::ComponentTypeRegistryResource>();
        registry_resource.0
    };
    
    let component_type_name = match type_registry.get_type_name(discriminant) {
        | Some(name) => name,
        | None => {
            error!("Unknown discriminant {} - component not registered", discriminant);
            return;
        },
    };

    // Get the network ID for this entity
    let entity_network_id = {
        if let Ok(entity_ref) = world.get_entity(entity) {
            if let Some(networked) = entity_ref.get::<crate::networking::NetworkedEntity>() {
                networked.network_id
            } else {
                warn!("Entity {:?} has no NetworkedEntity component", entity);
                return;
            }
        } else {
            warn!("Entity {:?} not found", entity);
            return;
        }
    };

    // Check if we should apply this operation based on LWW
    let should_apply = {
        if let Some(component_clocks) = world.get_resource::<ComponentVectorClocks>() {
            if let Some((current_clock, current_node_id)) =
                component_clocks.get(entity_network_id, component_type_name)
            {
                // We have a current clock - do LWW comparison with real node IDs
                let decision = compare_operations_lww(
                    current_clock,
                    *current_node_id,
                    incoming_clock,
                    incoming_node_id,
                );

                match decision {
                    | crate::networking::merge::MergeDecision::ApplyRemote => {
                        debug!(
                            "Applying remote Set for {} (remote is newer)",
                            component_type_name
                        );
                        true
                    },
                    | crate::networking::merge::MergeDecision::KeepLocal => {
                        debug!(
                            "Ignoring remote Set for {} (local is newer)",
                            component_type_name
                        );
                        false
                    },
                    | crate::networking::merge::MergeDecision::Concurrent => {
                        // For concurrent operations, use node_id comparison as deterministic
                        // tiebreaker This ensures all nodes make the same
                        // decision for concurrent updates
                        if incoming_node_id > *current_node_id {
                            debug!(
                                "Applying remote Set for {} (concurrent, remote node_id {:?} > local {:?})",
                                component_type_name, incoming_node_id, current_node_id
                            );
                            true
                        } else {
                            debug!(
                                "Ignoring remote Set for {} (concurrent, local node_id {:?} >= remote {:?})",
                                component_type_name, current_node_id, incoming_node_id
                            );
                            false
                        }
                    },
                    | crate::networking::merge::MergeDecision::Equal => {
                        debug!("Ignoring remote Set for {} (clocks equal)", component_type_name);
                        false
                    },
                }
            } else {
                // No current clock - this is the first time we're setting this component
                debug!(
                    "Applying remote Set for {} (no current clock)",
                    component_type_name
                );
                true
            }
        } else {
            // No ComponentVectorClocks resource - apply unconditionally
            warn!("ComponentVectorClocks resource not found - applying Set without LWW check");
            true
        }
    };

    if !should_apply {
        return;
    }

    // Apply the operation
    apply_set_operation(entity, discriminant, data, world);

    // Update the stored vector clock with node_id
    if let Some(mut component_clocks) = world.get_resource_mut::<ComponentVectorClocks>() {
        component_clocks.set(
            entity_network_id,
            component_type_name.to_string(),
            incoming_clock.clone(),
            incoming_node_id,
        );
        debug!(
            "Updated vector clock for {} on entity {:?} (node_id: {:?})",
            component_type_name, entity_network_id, incoming_node_id
        );
    }
}

/// Apply a Set operation (Last-Write-Wins)
///
/// Deserializes the component and inserts/updates it on the entity.
/// Handles both inline data and blob references.
fn apply_set_operation(
    entity: Entity,
    discriminant: u16,
    data: &ComponentData,
    world: &mut World,
) {
    let blob_store = world.get_resource::<BlobStore>();
    
    // Get the actual data (resolve blob if needed)
    let data_bytes = match data {
        | ComponentData::Inline(bytes) => bytes.clone(),
        | ComponentData::BlobRef { hash: _, size: _ } => {
            if let Some(store) = blob_store {
                match get_component_data(data, store) {
                    | Ok(bytes) => bytes,
                    | Err(e) => {
                        error!(
                            "Failed to retrieve blob for discriminant {}: {}",
                            discriminant, e
                        );
                        return;
                    },
                }
            } else {
                error!(
                    "Blob reference for discriminant {} but no blob store available",
                    discriminant
                );
                return;
            }
        },
    };

    // Get component type registry
    let type_registry = {
        let registry_resource = world.resource::<crate::persistence::ComponentTypeRegistryResource>();
        registry_resource.0
    };

    // Look up deserialize and insert functions by discriminant
    let deserialize_fn = type_registry.get_deserialize_fn(discriminant);
    let insert_fn = type_registry.get_insert_fn(discriminant);

    let (deserialize_fn, insert_fn) = match (deserialize_fn, insert_fn) {
        | (Some(d), Some(i)) => (d, i),
        | _ => {
            error!("Discriminant {} not registered in ComponentTypeRegistry", discriminant);
            return;
        },
    };

    // Deserialize the component
    let boxed_component = match deserialize_fn(&data_bytes) {
        | Ok(component) => component,
        | Err(e) => {
            error!("Failed to deserialize discriminant {}: {}", discriminant, e);
            return;
        },
    };

    // Insert the component into the entity
    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
        insert_fn(&mut entity_mut, boxed_component);
        debug!("Applied Set operation for discriminant {}", discriminant);

        // If we just inserted a Transform component, also add NetworkedTransform
        // This ensures remote entities can have their Transform changes detected
        let type_path = type_registry.get_type_path(discriminant);
        if type_path == Some("bevy_transform::components::transform::Transform") {
            if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
                if entity_mut
                    .get::<crate::networking::NetworkedTransform>()
                    .is_none()
                {
                    entity_mut.insert(crate::networking::NetworkedTransform::default());
                    debug!("Added NetworkedTransform to entity with Transform");
                }
            }
        }
    } else {
        error!(
            "Entity {:?} not found when applying discriminant {}",
            entity, discriminant
        );
    }
}

/// System to receive and apply incoming EntityDelta messages
///
/// This system polls the GossipBridge for incoming messages and applies them
/// to the local world.
///
/// Add this to your app:
///
/// ```no_run
/// use bevy::prelude::*;
/// use libmarathon::networking::receive_and_apply_deltas_system;
///
/// App::new().add_systems(Update, receive_and_apply_deltas_system);
/// ```
pub fn receive_and_apply_deltas_system(world: &mut World) {
    // Check if bridge exists
    if world
        .get_resource::<crate::networking::GossipBridge>()
        .is_none()
    {
        return;
    }

    // Clone the bridge to avoid borrowing issues
    let bridge = world.resource::<crate::networking::GossipBridge>().clone();

    // Poll for incoming messages
    while let Some(message) = bridge.try_recv() {
        match message.message {
            | SyncMessage::EntityDelta {
                entity_id,
                node_id,
                vector_clock,
                operations,
            } => {
                let delta = EntityDelta {
                    entity_id,
                    node_id,
                    vector_clock,
                    operations,
                };

                debug!(
                    "Received EntityDelta for entity {:?} with {} operations",
                    delta.entity_id,
                    delta.operations.len()
                );

                apply_entity_delta(&delta, world);
            },
            | SyncMessage::JoinRequest { .. } => {
                // Handled by handle_join_requests_system
                debug!("JoinRequest handled by dedicated system");
            },
            | SyncMessage::FullState { .. } => {
                // Handled by handle_full_state_system
                debug!("FullState handled by dedicated system");
            },
            | SyncMessage::SyncRequest { .. } => {
                // Handled by handle_sync_requests_system
                debug!("SyncRequest handled by dedicated system");
            },
            | SyncMessage::MissingDeltas { .. } => {
                // Handled by handle_missing_deltas_system
                debug!("MissingDeltas handled by dedicated system");
            },
            | SyncMessage::Lock { .. } => {
                // Handled by lock message dispatcher
                debug!("Lock message handled by dedicated system");
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_clock_merge() {
        let node_id = uuid::Uuid::new_v4();
        let mut node_clock = NodeVectorClock::new(node_id);

        let remote_node = uuid::Uuid::new_v4();
        let mut remote_clock = crate::networking::VectorClock::new();
        remote_clock.increment(remote_node);
        remote_clock.increment(remote_node);

        // Merge remote clock
        node_clock.clock.merge(&remote_clock);

        // Our clock should have the remote node's sequence
        assert_eq!(node_clock.clock.get(remote_node), 2);
    }

    #[test]
    fn test_entity_delta_structure() {
        let entity_id = uuid::Uuid::new_v4();
        let node_id = uuid::Uuid::new_v4();
        let clock = crate::networking::VectorClock::new();

        let delta = EntityDelta::new(entity_id, node_id, clock, vec![]);

        assert_eq!(delta.entity_id, entity_id);
        assert_eq!(delta.node_id, node_id);
        assert_eq!(delta.operations.len(), 0);
    }
}
