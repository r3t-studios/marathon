//! Join protocol for new peer onboarding
//!
//! This module handles the protocol for new peers to join an existing session
//! and receive the full world state. The join flow:
//!
//! 1. New peer sends JoinRequest with node ID and optional session secret
//! 2. Existing peer validates request and responds with FullState
//! 3. New peer applies FullState to initialize local world
//! 4. New peer begins participating in delta synchronization
//!
//! **NOTE:** This is a simplified implementation for Phase 7. Full security
//! and session management will be enhanced in Phase 13.

use bevy::prelude::*;

use crate::networking::{
    GossipBridge,
    NetworkedEntity,
    SessionId,
    VectorClock,
    blob_support::BlobStore,
    delta_generation::NodeVectorClock,
    entity_map::NetworkEntityMap,
    messages::{
        EntityState,
        JoinType,
        SyncMessage,
        VersionedMessage,
    },
};

/// Build a JoinRequest message
///
/// # Arguments
/// * `node_id` - The UUID of the node requesting to join
/// * `session_id` - The session to join
/// * `session_secret` - Optional pre-shared secret for authentication
/// * `last_known_clock` - Optional vector clock from previous session (for rejoin)
/// * `join_type` - Whether this is a fresh join or rejoin
///
/// # Example
///
/// ```
/// use libmarathon::networking::{build_join_request, SessionId, JoinType};
/// use uuid::Uuid;
///
/// let node_id = Uuid::new_v4();
/// let session_id = SessionId::new();
/// let request = build_join_request(node_id, session_id, None, None, JoinType::Fresh);
/// ```
pub fn build_join_request(
    node_id: uuid::Uuid,
    session_id: SessionId,
    session_secret: Option<bytes::Bytes>,
    last_known_clock: Option<VectorClock>,
    join_type: JoinType,
) -> VersionedMessage {
    VersionedMessage::new(SyncMessage::JoinRequest {
        node_id,
        session_id,
        session_secret,
        last_known_clock,
        join_type,
    })
}

/// Build a FullState message containing all networked entities
///
/// This serializes the entire world state for a new peer. Large worlds may
/// take significant bandwidth - Phase 14 will add compression.
///
/// # Parameters
///
/// - `world`: Bevy world containing entities
/// - `query`: Query for all NetworkedEntity components
/// - `type_registry`: Component type registry for serialization
/// - `node_clock`: Current node vector clock
/// - `blob_store`: Optional blob store for large components
///
/// # Returns
///
/// A FullState message ready to send to the joining peer
pub fn build_full_state(
    world: &World,
    networked_entities: &Query<(Entity, &NetworkedEntity)>,
    type_registry: &crate::persistence::ComponentTypeRegistry,
    node_clock: &NodeVectorClock,
    blob_store: Option<&BlobStore>,
) -> VersionedMessage {
    use crate::{
        networking::{
            blob_support::create_component_data,
            messages::ComponentState,
        },
    };

    let mut entities = Vec::new();

    for (entity, networked) in networked_entities.iter() {
        let mut components = Vec::new();

        // Serialize all registered Synced components on this entity
        let serialized_components = type_registry.serialize_entity_components(world, entity);

        for (discriminant, _type_path, serialized) in serialized_components {
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

    VersionedMessage::new(SyncMessage::FullState {
        entities,
        vector_clock: node_clock.clock.clone(),
    })
}

/// Apply a FullState message to the local world
///
/// This initializes the world for a newly joined peer by spawning all entities
/// and applying their component state.
///
/// # Parameters
///
/// - `entities`: List of entity states from FullState message
/// - `vector_clock`: Vector clock from FullState
/// - `commands`: Bevy commands for spawning entities
/// - `entity_map`: Entity map to populate
/// - `type_registry`: Component type registry for deserialization
/// - `node_clock`: Our node's vector clock to update
/// - `blob_store`: Optional blob store for resolving blob references
/// - `tombstone_registry`: Optional tombstone registry for deletion tracking
pub fn apply_full_state(
    entities: Vec<EntityState>,
    remote_clock: crate::networking::VectorClock,
    world: &mut World,
    type_registry: &crate::persistence::ComponentTypeRegistry,
) {
    use crate::networking::blob_support::get_component_data;

    info!("Applying FullState with {} entities", entities.len());

    // Merge the remote vector clock
    {
        let mut node_clock = world.resource_mut::<NodeVectorClock>();
        node_clock.clock.merge(&remote_clock);
    }

    // Spawn all entities and apply their state
    for entity_state in entities {
        // Handle deleted entities (tombstones)
        if entity_state.is_deleted {
            // Record tombstone
            if let Some(mut registry) = world.get_resource_mut::<crate::networking::TombstoneRegistry>() {
                registry.record_deletion(
                    entity_state.entity_id,
                    entity_state.owner_node_id,
                    entity_state.vector_clock.clone(),
                );
            }
            continue;
        }

        // Spawn entity with NetworkedEntity and Persisted components
        // This ensures entities received via FullState are persisted locally
        let entity = world
            .spawn((
                NetworkedEntity::with_id(entity_state.entity_id, entity_state.owner_node_id),
                crate::persistence::Persisted::with_id(entity_state.entity_id),
            ))
            .id();

        // Register in entity map
        {
            let mut entity_map = world.resource_mut::<NetworkEntityMap>();
            entity_map.insert(entity_state.entity_id, entity);
        }

        let num_components = entity_state.components.len();

        // Apply all components
        for component_state in &entity_state.components {
            // Get the actual data (resolve blob if needed)
            let data_bytes = match &component_state.data {
                | crate::networking::ComponentData::Inline(bytes) => bytes.clone(),
                | blob_ref @ crate::networking::ComponentData::BlobRef { .. } => {
                    let blob_store = world.get_resource::<BlobStore>();
                    if let Some(store) = blob_store.as_deref() {
                        match get_component_data(blob_ref, store) {
                            | Ok(bytes) => bytes,
                            | Err(e) => {
                                error!(
                                    "Failed to retrieve blob for discriminant {}: {}",
                                    component_state.discriminant, e
                                );
                                continue;
                            },
                        }
                    } else {
                        error!(
                            "Blob reference for discriminant {} but no blob store available",
                            component_state.discriminant
                        );
                        continue;
                    }
                },
            };

            // Use the discriminant directly from ComponentState
            let discriminant = component_state.discriminant;

            // Deserialize the component
            let boxed_component = match type_registry.deserialize(discriminant, &data_bytes) {
                | Ok(component) => component,
                | Err(e) => {
                    error!(
                        "Failed to deserialize discriminant {}: {}",
                        discriminant, e
                    );
                    continue;
                },
            };

            // Get the insert function for this discriminant
            let Some(insert_fn) = type_registry.get_insert_fn(discriminant) else {
                error!("No insert function for discriminant {}", discriminant);
                continue;
            };

            // Insert the component directly
            let type_name_for_log = type_registry.get_type_name(discriminant)
                .unwrap_or("unknown");
            if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
                insert_fn(&mut entity_mut, boxed_component);
                debug!("Applied component {} from FullState", type_name_for_log);
            }
        }

        debug!(
            "Spawned entity {:?} from FullState with {} components",
            entity_state.entity_id, num_components
        );
    }

    info!("FullState applied successfully");
}

/// System to handle JoinRequest messages
///
/// When we receive a JoinRequest, build and send a FullState response.
///
/// Add this to your app:
///
/// ```no_run
/// use bevy::prelude::*;
/// use libmarathon::networking::handle_join_requests_system;
///
/// App::new().add_systems(Update, handle_join_requests_system);
/// ```
pub fn handle_join_requests_system(
    world: &World,
    bridge: Option<Res<GossipBridge>>,
    networked_entities: Query<(Entity, &NetworkedEntity)>,
    type_registry: Res<crate::persistence::ComponentTypeRegistryResource>,
    node_clock: Res<NodeVectorClock>,
    blob_store: Option<Res<BlobStore>>,
) {
    let Some(bridge) = bridge else {
        return;
    };

    let registry = type_registry.0;
    let blob_store_ref = blob_store.as_deref();

    // Poll for incoming JoinRequest messages
    while let Some(message) = bridge.try_recv() {
        match message.message {
            | SyncMessage::JoinRequest {
                node_id,
                session_id,
                session_secret,
                last_known_clock: _,
                join_type,
            } => {
                info!(
                    "Received JoinRequest from node {} for session {} (type: {:?})",
                    node_id, session_id, join_type
                );

                // Validate session secret if configured
                if let Some(expected) =
                    world.get_resource::<crate::networking::plugin::SessionSecret>()
                {
                    match &session_secret {
                        | Some(provided_secret) => {
                            if let Err(e) = crate::networking::validate_session_secret(
                                provided_secret,
                                expected.as_bytes(),
                            ) {
                                error!("JoinRequest from {} rejected: {}", node_id, e);
                                continue; // Skip this request, don't send FullState
                            }
                            info!("Session secret validated for node {}", node_id);
                        },
                        | None => {
                            warn!(
                                "JoinRequest from {} missing required session secret, rejecting",
                                node_id
                            );
                            continue; // Reject requests without secret when one is configured
                        },
                    }
                } else if session_secret.is_some() {
                    // No session secret configured but peer provided one
                    debug!("Session secret provided but none configured, accepting");
                }

                // Build full state
                let full_state = build_full_state(
                    world,
                    &networked_entities,
                    &registry,
                    &node_clock,
                    blob_store_ref,
                );

                // Send full state to joining peer
                if let Err(e) = bridge.send(full_state) {
                    error!("Failed to send FullState: {}", e);
                } else {
                    info!("Sent FullState to node {}", node_id);
                }
            },
            | _ => {
                // Not a JoinRequest, ignore (other systems handle other
                // messages)
            },
        }
    }
}

/// System to handle FullState messages
///
/// When we receive a FullState (after sending JoinRequest), apply it to our
/// world.
///
/// This system should run BEFORE receive_and_apply_deltas_system to ensure
/// we're fully initialized before processing deltas.
pub fn handle_full_state_system(world: &mut World) {
    // Check if bridge exists
    if world.get_resource::<GossipBridge>().is_none() {
        return;
    }

    let bridge = world.resource::<GossipBridge>().clone();
    let type_registry = {
        let registry_resource = world.resource::<crate::persistence::ComponentTypeRegistryResource>();
        registry_resource.0
    };

    // Poll for FullState messages
    while let Some(message) = bridge.try_recv() {
        match message.message {
            | SyncMessage::FullState {
                entities,
                vector_clock,
            } => {
                info!("Received FullState with {} entities", entities.len());

                apply_full_state(
                    entities,
                    vector_clock,
                    world,
                    type_registry,
                );
            },
            | _ => {
                // Not a FullState, ignore
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::networking::VectorClock;

    #[test]
    fn test_build_join_request() {
        let node_id = uuid::Uuid::new_v4();
        let session_id = SessionId::new();
        let request = build_join_request(node_id, session_id.clone(), None, None, JoinType::Fresh);

        match request.message {
            | SyncMessage::JoinRequest {
                node_id: req_node_id,
                session_id: req_session_id,
                session_secret,
                last_known_clock,
                join_type,
            } => {
                assert_eq!(req_node_id, node_id);
                assert_eq!(req_session_id, session_id);
                assert!(session_secret.is_none());
                assert!(last_known_clock.is_none());
                assert!(matches!(join_type, JoinType::Fresh));
            },
            | _ => panic!("Expected JoinRequest"),
        }
    }

    #[test]
    fn test_build_join_request_with_secret() {
        let node_id = uuid::Uuid::new_v4();
        let session_id = SessionId::new();
        let secret = vec![1, 2, 3, 4];
        let request = build_join_request(
            node_id,
            session_id.clone(),
            Some(bytes::Bytes::from(secret.clone())),
            None,
            JoinType::Fresh,
        );

        match request.message {
            | SyncMessage::JoinRequest {
                node_id: _,
                session_id: req_session_id,
                session_secret,
                last_known_clock,
                join_type,
            } => {
                assert_eq!(req_session_id, session_id);
                assert_eq!(session_secret, Some(bytes::Bytes::from(secret)));
                assert!(last_known_clock.is_none());
                assert!(matches!(join_type, JoinType::Fresh));
            },
            | _ => panic!("Expected JoinRequest"),
        }
    }

    #[test]
    fn test_build_join_request_rejoin() {
        let node_id = uuid::Uuid::new_v4();
        let session_id = SessionId::new();
        let clock = VectorClock::new();
        let join_type = JoinType::Rejoin {
            last_active: 1234567890,
            entity_count: 42,
        };

        let request = build_join_request(
            node_id,
            session_id.clone(),
            None,
            Some(clock.clone()),
            join_type.clone(),
        );

        match request.message {
            | SyncMessage::JoinRequest {
                node_id: req_node_id,
                session_id: req_session_id,
                session_secret,
                last_known_clock,
                join_type: req_join_type,
            } => {
                assert_eq!(req_node_id, node_id);
                assert_eq!(req_session_id, session_id);
                assert!(session_secret.is_none());
                assert_eq!(last_known_clock, Some(clock));
                assert!(matches!(req_join_type, JoinType::Rejoin { .. }));
            },
            | _ => panic!("Expected JoinRequest"),
        }
    }

    #[test]
    fn test_entity_state_structure() {
        let entity_id = uuid::Uuid::new_v4();
        let owner_node_id = uuid::Uuid::new_v4();

        let state = EntityState {
            entity_id,
            owner_node_id,
            vector_clock: VectorClock::new(),
            components: vec![],
            is_deleted: false,
        };

        assert_eq!(state.entity_id, entity_id);
        assert_eq!(state.owner_node_id, owner_node_id);
        assert_eq!(state.components.len(), 0);
        assert!(!state.is_deleted);
    }

    #[test]
    fn test_apply_full_state_empty() {
        let node_id = uuid::Uuid::new_v4();
        let remote_clock = VectorClock::new();
        let type_registry = crate::persistence::component_registry();

        // Need a minimal Bevy app for testing
        let mut app = App::new();
        
        // Insert required resources
        app.insert_resource(NetworkEntityMap::new());
        app.insert_resource(NodeVectorClock::new(node_id));

        apply_full_state(
            vec![],
            remote_clock.clone(),
            app.world_mut(),
            type_registry,
        );

        // Should have merged clocks
        let node_clock = app.world().resource::<NodeVectorClock>();
        assert_eq!(node_clock.clock, remote_clock);
    }
}
