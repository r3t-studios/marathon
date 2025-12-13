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

use bevy::{
    prelude::*,
    reflect::TypeRegistry,
};

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
    session_secret: Option<Vec<u8>>,
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
/// - `type_registry`: Type registry for serialization
/// - `node_clock`: Current node vector clock
/// - `blob_store`: Optional blob store for large components
///
/// # Returns
///
/// A FullState message ready to send to the joining peer
pub fn build_full_state(
    world: &World,
    networked_entities: &Query<(Entity, &NetworkedEntity)>,
    type_registry: &TypeRegistry,
    node_clock: &NodeVectorClock,
    blob_store: Option<&BlobStore>,
) -> VersionedMessage {
    use crate::{
        networking::{
            blob_support::create_component_data,
            messages::ComponentState,
        },
        persistence::reflection::serialize_component,
    };

    let mut entities = Vec::new();

    for (entity, networked) in networked_entities.iter() {
        let entity_ref = world.entity(entity);
        let mut components = Vec::new();

        // Iterate over all type registrations to find components
        for registration in type_registry.iter() {
            // Skip if no ReflectComponent data
            let Some(reflect_component) = registration.data::<ReflectComponent>() else {
                continue;
            };

            let type_path = registration.type_info().type_path();

            // Skip networked wrapper components
            if type_path.ends_with("::NetworkedEntity") ||
                type_path.ends_with("::NetworkedTransform") ||
                type_path.ends_with("::NetworkedSelection") ||
                type_path.ends_with("::NetworkedDrawingPath")
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
                            | Ok(d) => d,
                            | Err(_) => continue,
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
/// - `type_registry`: Type registry for deserialization
/// - `node_clock`: Our node's vector clock to update
/// - `blob_store`: Optional blob store for resolving blob references
/// - `tombstone_registry`: Optional tombstone registry for deletion tracking
pub fn apply_full_state(
    entities: Vec<EntityState>,
    remote_clock: crate::networking::VectorClock,
    commands: &mut Commands,
    entity_map: &mut NetworkEntityMap,
    type_registry: &TypeRegistry,
    node_clock: &mut NodeVectorClock,
    blob_store: Option<&BlobStore>,
    mut tombstone_registry: Option<&mut crate::networking::TombstoneRegistry>,
) {
    use crate::{
        networking::blob_support::get_component_data,
        persistence::reflection::deserialize_component,
    };

    info!("Applying FullState with {} entities", entities.len());

    // Merge the remote vector clock
    node_clock.clock.merge(&remote_clock);

    // Spawn all entities and apply their state
    for entity_state in entities {
        // Handle deleted entities (tombstones)
        if entity_state.is_deleted {
            // Record tombstone
            if let Some(ref mut registry) = tombstone_registry {
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
        let entity = commands
            .spawn((
                NetworkedEntity::with_id(entity_state.entity_id, entity_state.owner_node_id),
                crate::persistence::Persisted::with_id(entity_state.entity_id),
            ))
            .id();

        // Register in entity map
        entity_map.insert(entity_state.entity_id, entity);

        let num_components = entity_state.components.len();

        // Apply all components
        for component_state in &entity_state.components {
            // Get the actual data (resolve blob if needed)
            let data_bytes = match &component_state.data {
                | crate::networking::ComponentData::Inline(bytes) => bytes.clone(),
                | blob_ref @ crate::networking::ComponentData::BlobRef { .. } => {
                    if let Some(store) = blob_store {
                        match get_component_data(blob_ref, store) {
                            | Ok(bytes) => bytes,
                            | Err(e) => {
                                error!(
                                    "Failed to retrieve blob for {}: {}",
                                    component_state.component_type, e
                                );
                                continue;
                            },
                        }
                    } else {
                        error!(
                            "Blob reference for {} but no blob store available",
                            component_state.component_type
                        );
                        continue;
                    }
                },
            };

            // Deserialize the component
            let reflected = match deserialize_component(&data_bytes, type_registry) {
                | Ok(r) => r,
                | Err(e) => {
                    error!(
                        "Failed to deserialize {}: {}",
                        component_state.component_type, e
                    );
                    continue;
                },
            };

            // Get the type registration
            let registration =
                match type_registry.get_with_type_path(&component_state.component_type) {
                    | Some(reg) => reg,
                    | None => {
                        error!(
                            "Component type {} not registered",
                            component_state.component_type
                        );
                        continue;
                    },
                };

            // Get ReflectComponent data
            let reflect_component = match registration.data::<ReflectComponent>() {
                | Some(rc) => rc.clone(),
                | None => {
                    error!(
                        "Component type {} does not have ReflectComponent data",
                        component_state.component_type
                    );
                    continue;
                },
            };

            // Insert the component
            let component_type_owned = component_state.component_type.clone();
            commands.queue(move |world: &mut World| {
                let type_registry_arc = {
                    let Some(type_registry_res) = world.get_resource::<AppTypeRegistry>() else {
                        error!("AppTypeRegistry not found in world");
                        return;
                    };
                    type_registry_res.clone()
                };

                let type_registry = type_registry_arc.read();

                if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
                    reflect_component.insert(&mut entity_mut, &*reflected, &type_registry);
                    debug!("Applied component {} from FullState", component_type_owned);
                }
            });
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
    type_registry: Res<AppTypeRegistry>,
    node_clock: Res<NodeVectorClock>,
    blob_store: Option<Res<BlobStore>>,
) {
    let Some(bridge) = bridge else {
        return;
    };

    let registry = type_registry.read();
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
pub fn handle_full_state_system(
    mut commands: Commands,
    bridge: Option<Res<GossipBridge>>,
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
                    &mut commands,
                    &mut entity_map,
                    &registry,
                    &mut node_clock,
                    blob_store_ref,
                    tombstone_registry.as_deref_mut(),
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
            Some(secret.clone()),
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
                assert_eq!(session_secret, Some(secret));
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
        let mut node_clock = NodeVectorClock::new(node_id);
        let remote_clock = VectorClock::new();

        // Create minimal setup for testing
        let mut entity_map = NetworkEntityMap::new();
        let type_registry = TypeRegistry::new();

        // Need a minimal Bevy app for Commands
        let mut app = App::new();
        let mut commands = app.world_mut().commands();

        apply_full_state(
            vec![],
            remote_clock.clone(),
            &mut commands,
            &mut entity_map,
            &type_registry,
            &mut node_clock,
            None,
            None, // tombstone_registry
        );

        // Should have merged clocks
        assert_eq!(node_clock.clock, remote_clock);
    }
}
