//! Events emitted from the Core Engine to Bevy

use crate::networking::{NodeId, SessionId, VectorClock};
use bevy::prelude::*;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum NetworkingInitStatus {
    CreatingEndpoint,
    EndpointReady,
    DiscoveringPeers {
        session_code: String,
        attempt: u8,
    },
    PeersFound {
        count: usize,
    },
    NoPeersFound,
    PublishingToDHT,
    InitializingGossip,
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    // Networking status
    NetworkingInitializing {
        session_id: SessionId,
        status: NetworkingInitStatus,
    },
    NetworkingStarted {
        session_id: SessionId,
        node_id: NodeId,
        bridge: crate::networking::GossipBridge,
    },
    NetworkingFailed {
        error: String,
    },
    NetworkingStopped,
    SessionJoined {
        session_id: SessionId,
    },
    SessionLeft,

    // Peer events
    PeerJoined {
        node_id: NodeId,
    },
    PeerLeft {
        node_id: NodeId,
    },

    // CRDT sync events
    EntitySpawned {
        entity_id: Uuid,
        position: Vec3,
        rotation: Quat,
        version: VectorClock,
    },
    EntityUpdated {
        entity_id: Uuid,
        position: Vec3,
        rotation: Quat,
        version: VectorClock,
    },
    EntityDeleted {
        entity_id: Uuid,
        version: VectorClock,
    },

    // Lock events
    LockAcquired {
        entity_id: Uuid,
        holder: NodeId,
    },
    LockReleased {
        entity_id: Uuid,
    },
    LockDenied {
        entity_id: Uuid,
        current_holder: NodeId,
    },
    LockExpired {
        entity_id: Uuid,
    },

    // Clock events
    ClockTicked {
        sequence: u64,
        clock: VectorClock,
    },
}
