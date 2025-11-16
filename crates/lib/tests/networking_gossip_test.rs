//! Integration test for gossip bridge
//!
//! Tests the gossip bridge channel infrastructure. Full iroh-gossip integration
//! will be tested in Phase 3.5.

use lib::networking::*;

#[test]
fn test_gossip_bridge_creation() {
    let node_id = uuid::Uuid::new_v4();
    let bridge = init_gossip_bridge(node_id);

    assert_eq!(bridge.node_id(), node_id);
}

#[test]
fn test_gossip_bridge_send() {
    let node_id = uuid::Uuid::new_v4();
    let bridge = init_gossip_bridge(node_id);

    let message = SyncMessage::JoinRequest {
        node_id,
        session_secret: None,
    };
    let versioned = VersionedMessage::new(message);

    // Should not error when sending
    let result = bridge.send(versioned);
    assert!(result.is_ok());
}

#[test]
fn test_gossip_bridge_try_recv_empty() {
    let node_id = uuid::Uuid::new_v4();
    let bridge = init_gossip_bridge(node_id);

    // Should return None when no messages available
    assert!(bridge.try_recv().is_none());
}
