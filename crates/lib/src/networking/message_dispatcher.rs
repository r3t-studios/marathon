//! Message dispatcher for efficient message routing
//!
//! This module eliminates the DRY violation and O(n²) behavior from having
//! multiple systems each polling the same message queue. Instead, a single
//! dispatcher system polls once and routes messages to appropriate handlers.

use bevy::{
    ecs::system::SystemState,
    prelude::*,
};

use crate::networking::{
    apply_entity_delta,
    apply_full_state,
    blob_support::BlobStore,
    build_missing_deltas,
    delta_generation::NodeVectorClock,
    entity_map::NetworkEntityMap,
    messages::SyncMessage,
    operation_log::OperationLog,
    GossipBridge,
    NetworkedEntity,
    TombstoneRegistry,
};

/// Central message dispatcher system
///
/// This system replaces the individual message polling loops in:
/// - `receive_and_apply_deltas_system`
/// - `handle_join_requests_system`
/// - `handle_full_state_system`
/// - `handle_sync_requests_system`
/// - `handle_missing_deltas_system`
///
/// By polling the message queue once and routing to handlers, we eliminate
/// O(n²) behavior and code duplication.
///
/// # Performance
///
/// - **Before**: O(n × m) where n = messages, m = systems (~5)
/// - **After**: O(n) - each message processed exactly once
///
/// # Example
///
/// ```no_run
/// use bevy::prelude::*;
/// use lib::networking::message_dispatcher_system;
///
/// App::new()
///     .add_systems(Update, message_dispatcher_system);
/// ```
pub fn message_dispatcher_system(world: &mut World) {
    // This is an exclusive system to avoid parameter conflicts with world access
    // Check if bridge exists
    if world.get_resource::<GossipBridge>().is_none() {
        return;
    }

    // Atomically drain all pending messages from the incoming queue
    // This prevents race conditions where messages could arrive between individual try_recv() calls
    let messages: Vec<crate::networking::VersionedMessage> = {
        let bridge = world.resource::<GossipBridge>();
        bridge.drain_incoming()
    };

    // Dispatch each message (bridge is no longer borrowed)
    for message in messages {
        dispatch_message(world, message);
    }

    // Flush all queued commands to ensure components are inserted immediately
    world.flush();
}

/// Helper function to dispatch a single message
/// This is separate to allow proper borrowing of world resources
fn dispatch_message(
    world: &mut World,
    message: crate::networking::VersionedMessage,
) {
    match message.message {
        // EntityDelta - apply remote operations
        | SyncMessage::EntityDelta {
            entity_id,
            node_id,
            vector_clock,
            operations,
        } => {
            let delta = crate::networking::EntityDelta {
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
        }

        // JoinRequest - new peer joining
        | SyncMessage::JoinRequest {
            node_id,
            session_secret,
        } => {
            info!("Received JoinRequest from node {}", node_id);

            // TODO: Validate session_secret in Phase 13
            if let Some(_secret) = session_secret {
                debug!("Session secret validation not yet implemented");
            }

            // Build and send full state
            // We need to collect data in separate steps to avoid borrow conflicts
            let networked_entities = {
                let mut query = world.query::<(Entity, &NetworkedEntity)>();
                query.iter(world).collect::<Vec<_>>()
            };

            let full_state = {
                let type_registry = world.resource::<AppTypeRegistry>().read();
                let node_clock = world.resource::<NodeVectorClock>();
                let blob_store = world.get_resource::<BlobStore>();

                build_full_state_from_data(
                    world,
                    &networked_entities,
                    &type_registry,
                    &node_clock,
                    blob_store.map(|b| b as &BlobStore),
                )
            };

            // Get bridge to send response
            if let Some(bridge) = world.get_resource::<GossipBridge>() {
                if let Err(e) = bridge.send(full_state) {
                    error!("Failed to send FullState: {}", e);
                } else {
                    info!("Sent FullState to node {}", node_id);
                }
            }
        }

        // FullState - receiving world state after join
        | SyncMessage::FullState {
            entities,
            vector_clock,
        } => {
            info!("Received FullState with {} entities", entities.len());

            // Use SystemState to properly borrow multiple resources
            let mut system_state: SystemState<(
                Commands,
                ResMut<NetworkEntityMap>,
                Res<AppTypeRegistry>,
                ResMut<NodeVectorClock>,
                Option<Res<BlobStore>>,
                Option<ResMut<TombstoneRegistry>>,
            )> = SystemState::new(world);

            {
                let (mut commands, mut entity_map, type_registry, mut node_clock, blob_store, mut tombstone_registry) = system_state.get_mut(world);
                let registry = type_registry.read();

                apply_full_state(
                    entities,
                    vector_clock,
                    &mut commands,
                    &mut entity_map,
                    &registry,
                    &mut node_clock,
                    blob_store.as_deref(),
                    tombstone_registry.as_deref_mut(),
                );
                // registry is dropped here
            }

            system_state.apply(world);
        }

        // SyncRequest - peer requesting missing operations
        | SyncMessage::SyncRequest {
            node_id: requesting_node,
            vector_clock: their_clock,
        } => {
            debug!("Received SyncRequest from node {}", requesting_node);

            if let Some(op_log) = world.get_resource::<OperationLog>() {
                // Find operations they're missing
                let missing_deltas = op_log.get_all_operations_newer_than(&their_clock);

                if !missing_deltas.is_empty() {
                    info!(
                        "Sending {} missing deltas to node {}",
                        missing_deltas.len(),
                        requesting_node
                    );

                    // Send MissingDeltas response
                    let response = build_missing_deltas(missing_deltas);
                    if let Some(bridge) = world.get_resource::<GossipBridge>() {
                        if let Err(e) = bridge.send(response) {
                            error!("Failed to send MissingDeltas: {}", e);
                        }
                    }
                } else {
                    debug!("No missing deltas for node {}", requesting_node);
                }
            } else {
                warn!("Received SyncRequest but OperationLog resource not available");
            }
        }

        // MissingDeltas - receiving operations we requested
        | SyncMessage::MissingDeltas { deltas } => {
            info!("Received MissingDeltas with {} operations", deltas.len());

            // Apply each delta
            for delta in deltas {
                debug!(
                    "Applying missing delta for entity {:?}",
                    delta.entity_id
                );

                apply_entity_delta(&delta, world);
            }
        }
    }
}

/// Helper to build full state from collected data
fn build_full_state_from_data(
    world: &World,
    networked_entities: &[(Entity, &NetworkedEntity)],
    type_registry: &bevy::reflect::TypeRegistry,
    node_clock: &NodeVectorClock,
    blob_store: Option<&BlobStore>,
) -> crate::networking::VersionedMessage {
    use crate::{
        networking::{
            blob_support::create_component_data,
            messages::{
                ComponentState,
                EntityState,
            },
        },
        persistence::reflection::serialize_component,
    };

    // Get tombstone registry to filter out deleted entities
    let tombstone_registry = world.get_resource::<crate::networking::TombstoneRegistry>();

    let mut entities = Vec::new();

    for (entity, networked) in networked_entities {
        // Skip tombstoned entities to prevent resurrection on joining nodes
        if let Some(registry) = &tombstone_registry {
            if registry.is_deleted(networked.network_id) {
                debug!(
                    "Skipping tombstoned entity {:?} in full state build",
                    networked.network_id
                );
                continue;
            }
        }
        let entity_ref = world.entity(*entity);
        let mut components = Vec::new();

        // Iterate over all type registrations to find components
        for registration in type_registry.iter() {
            // Skip if no ReflectComponent data
            let Some(reflect_component) = registration.data::<ReflectComponent>() else {
                continue;
            };

            let type_path = registration.type_info().type_path();

            // Skip networked wrapper components
            if type_path.ends_with("::NetworkedEntity")
                || type_path.ends_with("::NetworkedTransform")
                || type_path.ends_with("::NetworkedSelection")
                || type_path.ends_with("::NetworkedDrawingPath")
            {
                continue;
            }

            // Try to reflect this component from the entity
            if let Some(reflected) = reflect_component.reflect(entity_ref) {
                // Serialize the component
                if let Ok(serialized) = serialize_component(reflected, type_registry) {
                    // Create component data (inline or blob)
                    let data = if let Some(store) = blob_store {
                        match create_component_data(serialized, store) {
                            Ok(d) => d,
                            Err(_) => continue,
                        }
                    } else {
                        crate::networking::ComponentData::Inline(serialized)
                    };

                    components.push(ComponentState {
                        component_type: type_path.to_string(),
                        data,
                    });
                }
            }
        }

        entities.push(EntityState {
            entity_id: networked.network_id,
            owner_node_id: networked.owner_node_id,
            vector_clock: node_clock.clock.clone(),
            components,
            is_deleted: false,
        });
    }

    info!(
        "Built FullState with {} entities for new peer",
        entities.len()
    );

    crate::networking::VersionedMessage::new(SyncMessage::FullState {
        entities,
        vector_clock: node_clock.clock.clone(),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_message_dispatcher_compiles() {
        // This test just ensures the dispatcher system compiles
        // Integration tests would require a full Bevy app setup
    }
}
