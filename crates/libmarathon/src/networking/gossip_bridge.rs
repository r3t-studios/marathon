//! Async-to-sync bridge for iroh-gossip integration with Bevy
//!
//! This module provides the bridge between Bevy's synchronous ECS world and
//! iroh-gossip's async runtime. It uses channels to pass messages between the
//! async tokio tasks and Bevy systems.
//!
//! **NOTE:** This is a simplified implementation for Phase 3. Full gossip
//! integration will be completed in later phases.

use std::{
    collections::VecDeque,
    sync::{
        Arc,
        Mutex,
    },
};

use bevy::prelude::*;

use crate::networking::{
    error::{
        NetworkingError,
        Result,
    },
    messages::VersionedMessage,
    vector_clock::NodeId,
};

/// Bevy resource wrapping the gossip bridge
///
/// This resource provides the interface between Bevy systems and the async
/// gossip network. Systems can send messages via `send()` and poll for
/// incoming messages via `try_recv()`.
#[derive(Resource, Clone)]
pub struct GossipBridge {
    /// Queue for outgoing messages
    outgoing: Arc<Mutex<VecDeque<VersionedMessage>>>,

    /// Queue for incoming messages
    incoming: Arc<Mutex<VecDeque<VersionedMessage>>>,

    /// Our node ID
    pub node_id: NodeId,
}

impl std::fmt::Debug for GossipBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GossipBridge")
            .field("node_id", &self.node_id)
            .field("outgoing_len", &self.outgoing.lock().ok().map(|q| q.len()))
            .field("incoming_len", &self.incoming.lock().ok().map(|q| q.len()))
            .finish()
    }
}

impl GossipBridge {
    /// Create a new gossip bridge
    pub fn new(node_id: NodeId) -> Self {
        Self {
            outgoing: Arc::new(Mutex::new(VecDeque::new())),
            incoming: Arc::new(Mutex::new(VecDeque::new())),
            node_id,
        }
    }

    /// Send a message to the gossip network
    pub fn send(&self, message: VersionedMessage) -> Result<()> {
        // Diagnostic logging: track message type and nonce
        let msg_type = match &message.message {
            | crate::networking::SyncMessage::EntityDelta { entity_id, .. } => {
                format!("EntityDelta({})", entity_id)
            },
            | crate::networking::SyncMessage::JoinRequest { node_id, .. } => {
                format!("JoinRequest({})", node_id)
            },
            | crate::networking::SyncMessage::FullState { entities, .. } => {
                format!("FullState({} entities)", entities.len())
            },
            | crate::networking::SyncMessage::SyncRequest { node_id, .. } => {
                format!("SyncRequest({})", node_id)
            },
            | crate::networking::SyncMessage::MissingDeltas { deltas } => {
                format!("MissingDeltas({} ops)", deltas.len())
            },
            | crate::networking::SyncMessage::Lock(lock_msg) => {
                format!("Lock({:?})", lock_msg)
            },
        };

        debug!(
            "[GossipBridge::send] Node {} queuing message: {} (nonce: {})",
            self.node_id, msg_type, message.nonce
        );

        self.outgoing
            .lock()
            .map_err(|e| NetworkingError::Gossip(format!("Failed to lock outgoing queue: {}", e)))?
            .push_back(message);
        Ok(())
    }

    /// Try to receive a message from the gossip network (from incoming queue)
    pub fn try_recv(&self) -> Option<VersionedMessage> {
        self.incoming.lock().ok()?.pop_front()
    }

    /// Drain all pending messages from the incoming queue atomically
    ///
    /// This acquires the lock once and drains all messages, preventing race
    /// conditions where messages could arrive between individual try_recv()
    /// calls.
    pub fn drain_incoming(&self) -> Vec<VersionedMessage> {
        self.incoming
            .lock()
            .ok()
            .map(|mut queue| queue.drain(..).collect())
            .unwrap_or_default()
    }

    /// Try to get a message from the outgoing queue to send to gossip
    pub fn try_recv_outgoing(&self) -> Option<VersionedMessage> {
        self.outgoing.lock().ok()?.pop_front()
    }

    /// Push a message to the incoming queue (for testing/integration)
    pub fn push_incoming(&self, message: VersionedMessage) -> Result<()> {
        // Diagnostic logging: track incoming message type
        let msg_type = match &message.message {
            | crate::networking::SyncMessage::EntityDelta { entity_id, .. } => {
                format!("EntityDelta({})", entity_id)
            },
            | crate::networking::SyncMessage::JoinRequest { node_id, .. } => {
                format!("JoinRequest({})", node_id)
            },
            | crate::networking::SyncMessage::FullState { entities, .. } => {
                format!("FullState({} entities)", entities.len())
            },
            | crate::networking::SyncMessage::SyncRequest { node_id, .. } => {
                format!("SyncRequest({})", node_id)
            },
            | crate::networking::SyncMessage::MissingDeltas { deltas } => {
                format!("MissingDeltas({} ops)", deltas.len())
            },
            | crate::networking::SyncMessage::Lock(lock_msg) => {
                format!("Lock({:?})", lock_msg)
            },
        };

        debug!(
            "[GossipBridge::push_incoming] Node {} received from network: {} (nonce: {})",
            self.node_id, msg_type, message.nonce
        );

        self.incoming
            .lock()
            .map_err(|e| NetworkingError::Gossip(format!("Failed to lock incoming queue: {}", e)))?
            .push_back(message);
        Ok(())
    }

    /// Get our node ID
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }
}

/// Initialize the gossip bridge
pub fn init_gossip_bridge(node_id: NodeId) -> GossipBridge {
    info!("Initializing gossip bridge for node: {}", node_id);
    GossipBridge::new(node_id)
}

/// Bevy system to broadcast outgoing messages
pub fn broadcast_messages_system(/* will be implemented in later phases */) {
    // This will be populated when we have delta generation
}

/// Bevy system to receive incoming messages
///
/// **Note:** This is deprecated in favor of `receive_and_apply_deltas_system`
/// which provides full CRDT merge semantics. This stub remains for backward
/// compatibility.
pub fn receive_messages_system(bridge: Option<Res<GossipBridge>>) {
    let Some(bridge) = bridge else {
        return;
    };

    // Poll for incoming messages
    while let Some(message) = bridge.try_recv() {
        // For now, just log the message
        debug!("Received message: {:?}", message.message);

        // Use receive_and_apply_deltas_system for full functionality
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gossip_bridge_creation() {
        let node_id = uuid::Uuid::new_v4();
        let bridge = GossipBridge::new(node_id);

        assert_eq!(bridge.node_id(), node_id);
    }

    #[test]
    fn test_send_message() {
        use crate::networking::{
            JoinType,
            SessionId,
            SyncMessage,
        };

        let node_id = uuid::Uuid::new_v4();
        let bridge = GossipBridge::new(node_id);
        let session_id = SessionId::new();

        let message = SyncMessage::JoinRequest {
            node_id,
            session_id,
            session_secret: None,
            last_known_clock: None,
            join_type: JoinType::Fresh,
        };
        let versioned = VersionedMessage::new(message);

        let result = bridge.send(versioned);
        assert!(result.is_ok());
    }

    #[test]
    fn test_try_recv_empty() {
        let node_id = uuid::Uuid::new_v4();
        let bridge = GossipBridge::new(node_id);

        assert!(bridge.try_recv().is_none());
    }
}
