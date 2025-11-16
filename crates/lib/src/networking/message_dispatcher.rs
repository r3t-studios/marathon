//! Message dispatcher for efficient message routing
//!
//! This module eliminates the DRY violation and O(n²) behavior from having
//! multiple systems each polling the same message queue. Instead, a single
//! dispatcher system polls once and routes messages to appropriate handlers.

use bevy::prelude::*;

use crate::networking::{
    apply_entity_delta,
    apply_full_state,
    blob_support::BlobStore,
    build_full_state,
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
pub fn message_dispatcher_system(
    world: &World,
    mut commands: Commands,
    bridge: Option<Res<GossipBridge>>,
    mut entity_map: ResMut<NetworkEntityMap>,
    type_registry: Res<AppTypeRegistry>,
    mut node_clock: ResMut<NodeVectorClock>,
    blob_store: Option<Res<BlobStore>>,
    mut tombstone_registry: Option<ResMut<TombstoneRegistry>>,
    operation_log: Option<Res<OperationLog>>,
    networked_entities: Query<(Entity, &NetworkedEntity)>,
) {
    let Some(bridge) = bridge else {
        return;
    };

    let registry = type_registry.read();
    let blob_store_ref = blob_store.as_deref();

    // Poll messages once and route to appropriate handlers
    while let Some(message) = bridge.try_recv() {
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
                let full_state = build_full_state(
                    world,
                    &networked_entities,
                    &registry,
                    &node_clock,
                    blob_store_ref,
                );

                if let Err(e) = bridge.send(full_state) {
                    error!("Failed to send FullState: {}", e);
                } else {
                    info!("Sent FullState to node {}", node_id);
                }
            }

            // FullState - receiving world state after join
            | SyncMessage::FullState {
                entities,
                vector_clock,
            } => {
                info!("Received FullState with {} entities", entities.len());

                apply_full_state(
                    entities,
                    vector_clock,
                    &mut commands,
                    &mut entity_map,
                    &registry,
                    &mut node_clock,
                    blob_store_ref,
                    tombstone_registry.as_deref_mut(),
                );
            }

            // SyncRequest - peer requesting missing operations
            | SyncMessage::SyncRequest {
                node_id: requesting_node,
                vector_clock: their_clock,
            } => {
                debug!("Received SyncRequest from node {}", requesting_node);

                if let Some(ref op_log) = operation_log {
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
                        if let Err(e) = bridge.send(response) {
                            error!("Failed to send MissingDeltas: {}", e);
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
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_message_dispatcher_compiles() {
        // This test just ensures the dispatcher system compiles
        // Integration tests would require a full Bevy app setup
    }
}
