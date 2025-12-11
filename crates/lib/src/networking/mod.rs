//! CRDT-based networking layer for distributed synchronization
//!
//! This module implements the networking strategy defined in RFC 0001.
//! It provides CRDT-based synchronization over iroh-gossip with support for:
//!
//! - **Entity Synchronization** - Automatic sync of NetworkedEntity components
//! - **CRDT Merge Semantics** - LWW, OR-Set, and Sequence CRDTs
//! - **Large Blob Support** - Files >64KB via iroh-blobs
//! - **Join Protocol** - New peers receive full world state
//! - **Anti-Entropy** - Periodic sync to repair network partitions
//! - **Vector Clock** - Causality tracking for distributed operations
//!
//! # Example
//!
//! ```
//! use lib::networking::*;
//! use uuid::Uuid;
//!
//! // Create a vector clock and track operations
//! let node_id = Uuid::new_v4();
//! let mut clock = VectorClock::new();
//!
//! // Increment the clock for local operations
//! clock.increment(node_id);
//!
//! // Build a component operation
//! let builder = ComponentOpBuilder::new(node_id, clock.clone());
//! let op = builder.set(
//!     "Transform".to_string(),
//!     ComponentData::Inline(vec![1, 2, 3]),
//! );
//! ```

mod apply_ops;
mod auth;
mod blob_support;
mod change_detection;
mod components;
mod delta_generation;
mod entity_map;
mod error;
mod gossip_bridge;
mod join_protocol;
mod merge;
mod message_dispatcher;
mod messages;
mod operation_builder;
mod operation_log;
mod operations;
mod orset;
mod plugin;
mod rga;
mod sync_component;
mod tombstones;
mod vector_clock;

pub use apply_ops::*;
pub use auth::*;
pub use blob_support::*;
pub use change_detection::*;
pub use components::*;
pub use delta_generation::*;
pub use entity_map::*;
pub use error::*;
pub use gossip_bridge::*;
pub use join_protocol::*;
pub use merge::*;
pub use message_dispatcher::*;
pub use messages::*;
pub use operation_builder::*;
pub use operation_log::*;
pub use operations::*;
pub use orset::*;
pub use plugin::*;
pub use rga::*;
pub use sync_component::*;
pub use tombstones::*;
pub use vector_clock::*;

/// Spawn a networked entity with persistence enabled
///
/// Creates an entity with both NetworkedEntity and Persisted components,
/// registers it in the NetworkEntityMap, and returns the entity ID.
/// This is the single source of truth for creating networked entities
/// that need to be synchronized and persisted across the network.
///
/// # Parameters
/// - `world`: Bevy world to spawn entity in
/// - `entity_id`: Network ID for the entity (UUID)
/// - `node_id`: ID of the node that owns this entity
///
/// # Returns
/// The spawned Bevy entity's ID
///
/// # Example
/// ```no_run
/// use bevy::prelude::*;
/// use lib::networking::spawn_networked_entity;
/// use uuid::Uuid;
///
/// fn my_system(world: &mut World) {
///     let entity_id = Uuid::new_v4();
///     let node_id = Uuid::new_v4();
///     let entity = spawn_networked_entity(world, entity_id, node_id);
///     // Entity is now registered and ready for sync/persistence
/// }
/// ```
pub fn spawn_networked_entity(
    world: &mut bevy::prelude::World,
    entity_id: uuid::Uuid,
    node_id: uuid::Uuid,
) -> bevy::prelude::Entity {
    use bevy::prelude::*;

    // Spawn with both NetworkedEntity and Persisted components
    let entity = world
        .spawn((
            NetworkedEntity::with_id(entity_id, node_id),
            crate::persistence::Persisted::with_id(entity_id),
        ))
        .id();

    // Register in entity map
    let mut entity_map = world.resource_mut::<NetworkEntityMap>();
    entity_map.insert(entity_id, entity);

    info!(
        "Spawned new networked entity {:?} from node {}",
        entity_id, node_id
    );

    entity
}
