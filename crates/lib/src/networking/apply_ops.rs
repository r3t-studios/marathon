//! Apply remote operations to local ECS state
//!
//! This module handles incoming EntityDelta messages and applies them to the
//! local Bevy world using CRDT merge semantics.

use bevy::{
    prelude::*,
    reflect::TypeRegistry,
};

use crate::{
    networking::{
        blob_support::{
            get_component_data,
            BlobStore,
        },
        delta_generation::NodeVectorClock,
        entity_map::NetworkEntityMap,
        messages::{
            ComponentData,
            EntityDelta,
            SyncMessage,
        },
        operations::ComponentOp,
        NetworkedEntity,
    },
    persistence::reflection::deserialize_component,
};

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
/// - `commands`: Bevy Commands for spawning/modifying entities
/// - `entity_map`: Map from network_id to Entity
/// - `type_registry`: Bevy's type registry for deserialization
/// - `node_clock`: Our node's vector clock (for causality tracking)
/// - `blob_store`: Optional blob store for resolving large component references
/// - `tombstone_registry`: Optional tombstone registry for deletion tracking
pub fn apply_entity_delta(
    delta: &EntityDelta,
    commands: &mut Commands,
    entity_map: &mut NetworkEntityMap,
    type_registry: &TypeRegistry,
    node_clock: &mut NodeVectorClock,
    blob_store: Option<&BlobStore>,
    mut tombstone_registry: Option<&mut crate::networking::TombstoneRegistry>,
) {
    // Validate and merge the remote vector clock
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

    // Check if any operations are Delete operations
    for op in &delta.operations {
        if let crate::networking::ComponentOp::Delete { vector_clock } = op {
            // Record tombstone
            if let Some(ref mut registry) = tombstone_registry {
                registry.record_deletion(
                    delta.entity_id,
                    delta.node_id,
                    vector_clock.clone(),
                );

                // Despawn the entity if it exists locally
                if let Some(entity) = entity_map.get_entity(delta.entity_id) {
                    commands.entity(entity).despawn();
                    entity_map.remove_by_network_id(delta.entity_id);
                    info!("Despawned entity {:?} due to Delete operation", delta.entity_id);
                }

                // Don't process other operations - entity is deleted
                return;
            }
        }
    }

    // Check if we should ignore this delta due to deletion
    if let Some(ref registry) = tombstone_registry {
        if registry.should_ignore_operation(delta.entity_id, &delta.vector_clock) {
            debug!(
                "Ignoring delta for deleted entity {:?}",
                delta.entity_id
            );
            return;
        }
    }

    // Look up or create the entity
    let entity = match entity_map.get_entity(delta.entity_id) {
        Some(entity) => entity,
        None => {
            // Spawn new entity with NetworkedEntity component
            let entity = commands
                .spawn(NetworkedEntity::with_id(delta.entity_id, delta.node_id))
                .id();

            entity_map.insert(delta.entity_id, entity);
            info!(
                "Spawned new networked entity {:?} from node {}",
                delta.entity_id, delta.node_id
            );

            entity
        }
    };

    // Apply each operation (skip Delete operations - handled above)
    for op in &delta.operations {
        if !op.is_delete() {
            apply_component_op(entity, op, commands, type_registry, blob_store);
        }
    }
}

/// Apply a single ComponentOp to an entity
///
/// This dispatches to the appropriate CRDT merge logic based on the operation
/// type.
fn apply_component_op(
    entity: Entity,
    op: &ComponentOp,
    commands: &mut Commands,
    type_registry: &TypeRegistry,
    blob_store: Option<&BlobStore>,
) {
    match op {
        | ComponentOp::Set {
            component_type,
            data,
            vector_clock: _,
        } => {
            apply_set_operation(entity, component_type, data, commands, type_registry, blob_store);
        }
        | ComponentOp::SetAdd { component_type, .. } => {
            // OR-Set add - Phase 10 provides OrSet<T> type
            // Application code should use OrSet in components and handle SetAdd/SetRemove
            // Full integration will be in Phase 12 plugin
            debug!("SetAdd operation for {} (use OrSet<T> in components)", component_type);
        }
        | ComponentOp::SetRemove { component_type, .. } => {
            // OR-Set remove - Phase 10 provides OrSet<T> type
            // Application code should use OrSet in components and handle SetAdd/SetRemove
            // Full integration will be in Phase 12 plugin
            debug!("SetRemove operation for {} (use OrSet<T> in components)", component_type);
        }
        | ComponentOp::SequenceInsert { .. } => {
            // RGA insert - will be implemented in Phase 11
            debug!("SequenceInsert operation not yet implemented");
        }
        | ComponentOp::SequenceDelete { .. } => {
            // RGA delete - will be implemented in Phase 11
            debug!("SequenceDelete operation not yet implemented");
        }
        | ComponentOp::Delete { .. } => {
            // Entity deletion - will be implemented in Phase 9
            debug!("Delete operation not yet implemented");
        }
    }
}

/// Apply a Set operation (Last-Write-Wins)
///
/// Deserializes the component and inserts/updates it on the entity.
/// Handles both inline data and blob references.
fn apply_set_operation(
    entity: Entity,
    component_type: &str,
    data: &ComponentData,
    commands: &mut Commands,
    type_registry: &TypeRegistry,
    blob_store: Option<&BlobStore>,
) {
    // Get the actual data (resolve blob if needed)
    let data_bytes = match data {
        | ComponentData::Inline(bytes) => bytes.clone(),
        | ComponentData::BlobRef { hash: _, size: _ } => {
            if let Some(store) = blob_store {
                match get_component_data(data, store) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        error!(
                            "Failed to retrieve blob for component {}: {}",
                            component_type, e
                        );
                        return;
                    }
                }
            } else {
                error!(
                    "Blob reference for {} but no blob store available",
                    component_type
                );
                return;
            }
        }
    };

    // Deserialize the component
    let reflected = match deserialize_component(&data_bytes, type_registry) {
        Ok(reflected) => reflected,
        Err(e) => {
            error!(
                "Failed to deserialize component {}: {}",
                component_type, e
            );
            return;
        }
    };

    // Get the type registration
    let registration = match type_registry.get_with_type_path(component_type) {
        Some(reg) => reg,
        None => {
            error!("Component type {} not registered", component_type);
            return;
        }
    };

    // Get ReflectComponent data
    let reflect_component = match registration.data::<ReflectComponent>() {
        Some(rc) => rc.clone(),
        None => {
            error!(
                "Component type {} does not have ReflectComponent data",
                component_type
            );
            return;
        }
    };

    // Clone what we need to avoid lifetime issues
    let component_type_owned = component_type.to_string();

    // Insert or update the component
    commands.queue(move |world: &mut World| {
        // Get the type registry from the world and clone it
        let type_registry_arc = {
            let Some(type_registry_res) = world.get_resource::<AppTypeRegistry>() else {
                error!("AppTypeRegistry not found in world");
                return;
            };
            type_registry_res.clone()
        };

        // Now we can safely get mutable access to the world
        let type_registry = type_registry_arc.read();

        if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
            reflect_component.insert(&mut entity_mut, &*reflected, &type_registry);
            debug!("Applied Set operation for {}", component_type_owned);
        }
    });
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
/// use lib::networking::receive_and_apply_deltas_system;
///
/// App::new()
///     .add_systems(Update, receive_and_apply_deltas_system);
/// ```
pub fn receive_and_apply_deltas_system(
    mut commands: Commands,
    bridge: Option<Res<crate::networking::GossipBridge>>,
    mut entity_map: ResMut<NetworkEntityMap>,
    type_registry: Res<AppTypeRegistry>,
    mut node_clock: ResMut<NodeVectorClock>,
    blob_store: Option<Res<BlobStore>>,
    mut tombstone_registry: Option<ResMut<crate::networking::TombstoneRegistry>>,
) {
    let Some(bridge) = bridge else {
        return;
    };

    let registry = type_registry.read();
    let blob_store_ref = blob_store.as_deref();

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

                apply_entity_delta(
                    &delta,
                    &mut commands,
                    &mut entity_map,
                    &registry,
                    &mut node_clock,
                    blob_store_ref,
                    tombstone_registry.as_deref_mut(),
                );
            }
            | SyncMessage::JoinRequest { .. } => {
                // Handled by handle_join_requests_system
                debug!("JoinRequest handled by dedicated system");
            }
            | SyncMessage::FullState { .. } => {
                // Handled by handle_full_state_system
                debug!("FullState handled by dedicated system");
            }
            | SyncMessage::SyncRequest { .. } => {
                // Handled by handle_sync_requests_system
                debug!("SyncRequest handled by dedicated system");
            }
            | SyncMessage::MissingDeltas { .. } => {
                // Handled by handle_missing_deltas_system
                debug!("MissingDeltas handled by dedicated system");
            }
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
