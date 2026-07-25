//! Control socket protocol for remote engine control
//!
//! This module defines the message protocol for controlling the engine via
//! Unix domain sockets without exposing network ports. Used for testing,
//! validation, and programmatic control of sessions.
//!
//! # Security
//!
//! Currently debug-only. See issue #135 for production security requirements.

use uuid::Uuid;

use crate::networking::{
    SessionId,
    SessionState,
    SyncMessage,
    VersionedMessage,
};

/// Control command sent to the engine
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ControlCommand {
    /// Get current session status
    GetStatus,

    /// Send a test message through gossip
    SendTestMessage { content: String },

    /// Inject a message directly into the incoming queue (for testing)
    InjectMessage { message: VersionedMessage },

    /// Broadcast a full sync message through gossip
    BroadcastMessage { message: SyncMessage },

    /// Request graceful shutdown
    Shutdown,

    // Session lifecycle commands
    /// Join a specific session by code
    JoinSession { session_code: String },

    /// Leave the current session gracefully
    LeaveSession,

    /// Get detailed current session information
    GetSessionInfo,

    /// List all sessions in the database
    ListSessions,

    /// Delete a session from the database
    DeleteSession { session_code: String },

    /// Get list of connected peers in current session
    ListPeers,

    // Entity commands
    /// Spawn an entity with a given type and position
    SpawnEntity {
        entity_type: String,
        position: [f32; 3],
    },

    /// Delete an entity by its UUID
    DeleteEntity { entity_id: Uuid },
}

/// Detailed session information
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct SessionInfo {
    pub session_id: SessionId,
    pub session_name: Option<String>,
    pub state: SessionState,
    pub created_at: i64,
    pub last_active: i64,
    pub entity_count: usize,
}

/// Peer information
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct PeerInfo {
    pub node_id: Uuid,
    pub connected_since: Option<i64>,
}

/// Response from the engine to a control command
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ControlResponse {
    /// Session status information
    Status {
        node_id: Uuid,
        session_id: SessionId,
        outgoing_queue_size: usize,
        incoming_queue_size: usize,
        /// Number of connected peers (if available from gossip)
        connected_peers: Option<usize>,
    },

    /// Detailed session information
    SessionInfo(SessionInfo),

    /// List of sessions
    Sessions(Vec<SessionInfo>),

    /// List of connected peers
    Peers(Vec<PeerInfo>),

    /// Acknowledgment of command execution
    Ok { message: String },

    /// Error occurred during command execution
    Error { error: String },
}

impl ControlCommand {
    /// Serialize a command to bytes using rkyv
    pub fn to_bytes(&self) -> Result<Vec<u8>, rkyv::rancor::Error> {
        rkyv::to_bytes::<rkyv::rancor::Error>(self).map(|b| b.to_vec())
    }

    /// Deserialize a command from bytes using rkyv
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, rkyv::rancor::Error> {
        rkyv::from_bytes::<Self, rkyv::rancor::Error>(bytes)
    }
}

impl ControlResponse {
    /// Serialize a response to bytes using rkyv
    pub fn to_bytes(&self) -> Result<Vec<u8>, rkyv::rancor::Error> {
        rkyv::to_bytes::<rkyv::rancor::Error>(self).map(|b| b.to_vec())
    }

    /// Deserialize a response from bytes using rkyv
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, rkyv::rancor::Error> {
        rkyv::from_bytes::<Self, rkyv::rancor::Error>(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_roundtrip() {
        let cmd = ControlCommand::GetStatus;
        let bytes = cmd.to_bytes().unwrap();
        let decoded = ControlCommand::from_bytes(&bytes).unwrap();

        match decoded {
            | ControlCommand::GetStatus => {},
            | _ => panic!("Failed to decode GetStatus"),
        }
    }

    #[test]
    fn test_response_roundtrip() {
        let resp = ControlResponse::Ok {
            message: "Test".to_string(),
        };
        let bytes = resp.to_bytes().unwrap();
        let decoded = ControlResponse::from_bytes(&bytes).unwrap();

        match decoded {
            | ControlResponse::Ok { message } => assert_eq!(message, "Test"),
            | _ => panic!("Failed to decode Ok response"),
        }
    }
}
