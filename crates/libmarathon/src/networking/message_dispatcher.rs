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
    GossipBridge,
    JoinType,
    NetworkedEntity,
    TombstoneRegistry,
    VersionedMessage,
    apply_entity_delta,
    apply_full_state,
    blob_support::BlobStore,
    build_missing_deltas,
    delta_generation::NodeVectorClock,
    entity_map::NetworkEntityMap,
    messages::SyncMessage,
    operation_log::OperationLog,
    plugin::SessionSecret,
    validate_session_secret,
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
/// use libmarathon::networking::message_dispatcher_system;
///
/// App::new().add_systems(Update, message_dispatcher_system);
/// ```
pub fn message_dispatcher_system(world: &mut World) {
    // This is an exclusive system to avoid parameter conflicts with world access
    // Check if bridge exists
    if world.get_resource::<GossipBridge>().is_none() {
        return;
    }

    // Atomically drain all pending messages from the incoming queue
    // This prevents race conditions where messages could arrive between individual
    // try_recv() calls
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
fn dispatch_message(world: &mut World, message: crate::networking::VersionedMessage) {
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
        },

        // JoinRequest - new peer joining (or rejoining)
        | SyncMessage::JoinRequest {
            node_id,
            session_id,
            session_secret,
            last_known_clock,
            join_type,
        } => {
            info!(
                "Received JoinRequest from node {} for session {} (type: {:?})",
                node_id, session_id, join_type
            );

            // Validate session secret if configured
            if let Some(expected) = world.get_resource::<SessionSecret>() {
                match &session_secret {
                    | Some(provided_secret) => {
                        if let Err(e) =
                            validate_session_secret(provided_secret, expected.as_bytes())
                        {
                            error!("JoinRequest from {} rejected: {}", node_id, e);
                            return; // Stop processing this message
                        }
                        info!("Session secret validated for node {}", node_id);
                    },
                    | None => {
                        warn!(
                            "JoinRequest from {} missing required session secret, rejecting",
                            node_id
                        );
                        return; // Reject requests without secret when one is configured
                    },
                }
            } else if session_secret.is_some() {
                // No session secret configured but peer provided one
                debug!("Session secret provided but none configured, accepting");
            }

            // Hybrid join protocol: decide between FullState and MissingDeltas
            // Fresh joins always get FullState
            // Rejoins get deltas if <1000 operations, otherwise FullState
            let response = match (&join_type, &last_known_clock) {
                // Fresh join or no clock provided → send FullState
                | (JoinType::Fresh, _) | (_, None) => {
                    info!("Fresh join from node {} - sending FullState", node_id);

                    // Collect networked entities
                    let networked_entities = {
                        let mut query = world.query::<(Entity, &NetworkedEntity)>();
                        query.iter(world).collect::<Vec<_>>()
                    };

                    // Build full state
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
                },

                // Rejoin with known clock → check delta count
                | (JoinType::Rejoin { .. }, Some(their_clock)) => {
                    info!(
                        "Rejoin from node {} - checking delta count since last known clock",
                        node_id
                    );

                    // Get operation log and check missing deltas
                    let operation_log = world.resource::<crate::networking::OperationLog>();
                    let missing_deltas =
                        operation_log.get_all_operations_newer_than(their_clock);

                    // If delta count is small (<= 1000 ops), send deltas
                    // Otherwise fall back to full state
                    if missing_deltas.len() <= 1000 {
                        info!(
                            "Rejoin from node {} - sending {} MissingDeltas (efficient rejoin)",
                            node_id,
                            missing_deltas.len()
                        );

                        VersionedMessage::new(SyncMessage::MissingDeltas {
                            deltas: missing_deltas,
                        })
                    } else {
                        info!(
                            "Rejoin from node {} - delta count {} exceeds threshold, sending FullState",
                            node_id,
                            missing_deltas.len()
                        );

                        // Collect networked entities
                        let networked_entities = {
                            let mut query = world.query::<(Entity, &NetworkedEntity)>();
                            query.iter(world).collect::<Vec<_>>()
                        };

                        // Build full state
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
                    }
                },
            };

            // Send response
            if let Some(bridge) = world.get_resource::<GossipBridge>() {
                if let Err(e) = bridge.send(response) {
                    error!("Failed to send join response: {}", e);
                } else {
                    info!("Sent join response to node {}", node_id);
                }
            }
        },

        // FullState - receiving world state after join
        | SyncMessage::FullState {
            entities,
            vector_clock,
        } => {
            info!("Received FullState with {} entities", entities.len());

            let type_registry = {
                let registry_resource = world.resource::<crate::persistence::ComponentTypeRegistryResource>();
                registry_resource.0
            };

            apply_full_state(
                entities,
                vector_clock,
                world,
                type_registry,
            );
        },

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
        },

        // MissingDeltas - receiving operations we requested
        | SyncMessage::MissingDeltas { deltas } => {
            info!("Received MissingDeltas with {} operations", deltas.len());

            // Apply each delta
            for delta in deltas {
                debug!("Applying missing delta for entity {:?}", delta.entity_id);

                apply_entity_delta(&delta, world);
            }
        },

        // Lock - entity lock protocol messages
        | SyncMessage::Lock(lock_msg) => {
            use crate::networking::LockMessage;

            if let Some(mut registry) = world.get_resource_mut::<crate::networking::EntityLockRegistry>() {
                match lock_msg {
                    | LockMessage::LockRequest { entity_id, node_id } => {
                        debug!("Received LockRequest for entity {} from node {}", entity_id, node_id);

                        match registry.try_acquire(entity_id, node_id) {
                            | Ok(()) => {
                                // Acquired successfully - broadcast confirmation
                                if let Some(bridge) = world.get_resource::<GossipBridge>() {
                                    let msg = VersionedMessage::new(SyncMessage::Lock(
                                        LockMessage::LockAcquired {
                                            entity_id,
                                            holder: node_id,
                                        },
                                    ));
                                    if let Err(e) = bridge.send(msg) {
                                        error!("Failed to broadcast LockAcquired: {}", e);
                                    } else {
                                        info!("Lock acquired: entity {} by node {}", entity_id, node_id);
                                    }
                                }
                            },
                            | Err(current_holder) => {
                                // Already locked - send rejection
                                if let Some(bridge) = world.get_resource::<GossipBridge>() {
                                    let msg = VersionedMessage::new(SyncMessage::Lock(
                                        LockMessage::LockRejected {
                                            entity_id,
                                            requester: node_id,
                                            current_holder,
                                        },
                                    ));
                                    if let Err(e) = bridge.send(msg) {
                                        error!("Failed to send LockRejected: {}", e);
                                    } else {
                                        debug!("Lock rejected: entity {} requested by {} (held by {})",
                                            entity_id, node_id, current_holder);
                                    }
                                }
                            },
                        }
                    },

                    | LockMessage::LockAcquired { entity_id, holder } => {
                        debug!("Received LockAcquired for entity {} by node {}", entity_id, holder);
                        // Lock already applied optimistically, just log confirmation
                    },

                    | LockMessage::LockRejected {
                        entity_id,
                        requester,
                        current_holder,
                    } => {
                        warn!(
                            "Lock rejected: entity {} requested by {} (held by {})",
                            entity_id, requester, current_holder
                        );
                        // Could trigger UI notification here
                    },

                    | LockMessage::LockHeartbeat { entity_id, holder } => {
                        trace!("Received LockHeartbeat for entity {} from node {}", entity_id, holder);

                        // Renew the lock's heartbeat timestamp
                        if registry.renew_heartbeat(entity_id, holder) {
                            trace!("Lock heartbeat renewed: entity {} by node {}", entity_id, holder);
                        } else {
                            debug!(
                                "Received heartbeat for entity {} from {}, but lock not found or holder mismatch",
                                entity_id, holder
                            );
                        }
                    },

                    | LockMessage::LockRelease { entity_id, node_id } => {
                        debug!("Received LockRelease for entity {} from node {}", entity_id, node_id);

                        if registry.release(entity_id, node_id) {
                            // Broadcast confirmation
                            if let Some(bridge) = world.get_resource::<GossipBridge>() {
                                let msg = VersionedMessage::new(SyncMessage::Lock(
                                    LockMessage::LockReleased { entity_id },
                                ));
                                if let Err(e) = bridge.send(msg) {
                                    error!("Failed to broadcast LockReleased: {}", e);
                                } else {
                                    info!("Lock released: entity {}", entity_id);
                                }
                            }
                        }
                    },

                    | LockMessage::LockReleased { entity_id } => {
                        debug!("Received LockReleased for entity {}", entity_id);
                        // Lock already released locally, just log confirmation
                    },
                }
            } else {
                warn!("Received lock message but EntityLockRegistry not available");
            }
        },
    }
}

/// Helper to build full state from collected data
fn build_full_state_from_data(
    world: &World,
    networked_entities: &[(Entity, &NetworkedEntity)],
    _type_registry: &bevy::reflect::TypeRegistry,
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
        let mut components = Vec::new();

        // Get component type registry
        let type_registry_res = world.resource::<crate::persistence::ComponentTypeRegistryResource>();
        let component_registry = type_registry_res.0;

        // Serialize all registered components on this entity
        let serialized_components = component_registry.serialize_entity_components(world, *entity);

        for (discriminant, type_path, serialized) in serialized_components {
            // Skip networked wrapper components
            if type_path.ends_with("::NetworkedEntity") ||
                type_path.ends_with("::NetworkedTransform") ||
                type_path.ends_with("::NetworkedSelection") ||
                type_path.ends_with("::NetworkedDrawingPath")
            {
                continue;
            }

            // Create component data (inline or blob)
            let data = if let Some(store) = blob_store {
                match create_component_data(serialized, store) {
                    | Ok(d) => d,
                    | Err(_) => continue,
                }
            } else {
                crate::networking::ComponentData::Inline(serialized)
            };

            components.push(ComponentState {
                discriminant,
                data,
            });
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
