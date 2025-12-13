//! App-level offline resource management
//!
//! Sets up vector clock and networking resources. Sessions are created later
//! when the user starts networking.

use bevy::prelude::*;
use libmarathon::{
    networking::{
        EntityLockRegistry, NetworkEntityMap, NodeVectorClock, VectorClock,
    },
};
use uuid::Uuid;

/// Initialize offline resources on app startup
///
/// This sets up the vector clock and networking-related resources, but does NOT
/// create a session. Sessions only exist when networking is active.
pub fn initialize_offline_resources(world: &mut World) {
    info!("Initializing offline resources (no session yet)...");

    // Create node ID (persists for this app instance)
    let node_id = Uuid::new_v4();
    info!("Node ID: {}", node_id);

    // Insert vector clock resource (always available, offline or online)
    world.insert_resource(NodeVectorClock {
        node_id,
        clock: VectorClock::new(),
    });

    // Insert networking resources (available from startup, even before networking starts)
    world.insert_resource(NetworkEntityMap::default());
    world.insert_resource(EntityLockRegistry::default());

    info!("Offline resources initialized (vector clock ready)");
}
