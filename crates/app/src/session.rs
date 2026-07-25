//! App-level offline resource management
//!
//! Sets up vector clock and networking resources. Sessions are created later
//! when the user starts networking.

use bevy::prelude::*;
use libmarathon::networking::{
    CurrentSession,
    EntityLockRegistry,
    NetworkEntityMap,
    NodeVectorClock,
    Session,
    VectorClock,
};
use uuid::Uuid;

/// Initialize offline resources on app startup
///
/// This sets up the vector clock and networking-related resources.
/// Creates an offline CurrentSession that will be updated when networking
/// starts.
pub fn initialize_offline_resources(world: &mut World) {
    info!("Initializing offline resources...");

    // Create node ID (persists for this app instance)
    let node_id = Uuid::new_v4();
    info!("Node ID: {}", node_id);

    // Insert vector clock resource (always available, offline or online)
    world.insert_resource(NodeVectorClock {
        node_id,
        clock: VectorClock::new(),
    });

    // Insert networking resources (available from startup, even before networking
    // starts)
    world.insert_resource(NetworkEntityMap::default());
    world.insert_resource(EntityLockRegistry::default());

    // Create offline session (will be updated when networking starts)
    // This ensures CurrentSession resource always exists for UI binding
    let offline_session_id = libmarathon::networking::SessionId::new();
    let offline_session = Session::new(offline_session_id);
    world.insert_resource(CurrentSession::new(offline_session, VectorClock::new()));

    info!("Offline resources initialized (vector clock ready, session created in offline state)");
}
