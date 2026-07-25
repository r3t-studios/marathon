//! Commands sent from Bevy to the Core Engine

use bevy::prelude::*;
use uuid::Uuid;

use crate::networking::SessionId;

#[derive(Debug, Clone)]
pub enum EngineCommand {
    // Networking lifecycle
    StartNetworking {
        session_id: SessionId,
    },
    StopNetworking,
    JoinSession {
        session_id: SessionId,
    },
    LeaveSession,

    // CRDT operations
    SpawnEntity {
        entity_id: Uuid,
        position: Vec3,
        rotation: Quat,
    },
    UpdateTransform {
        entity_id: Uuid,
        position: Vec3,
        rotation: Quat,
    },
    DeleteEntity {
        entity_id: Uuid,
    },

    // Lock operations
    AcquireLock {
        entity_id: Uuid,
    },
    ReleaseLock {
        entity_id: Uuid,
    },
    BroadcastHeartbeat {
        entity_id: Uuid,
    },

    // Persistence
    SaveSession,
    LoadSession {
        session_id: SessionId,
    },

    // Clock
    TickClock,

    // Lifecycle
    Shutdown,
}
