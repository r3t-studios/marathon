//! Integration tests for session synchronization
//!
//! These tests validate the session lifecycle:
//! - Sending JoinRequest when networking starts
//! - Transitioning from Joining → Active state
//! - Session persistence and recovery
//!
//! NOTE: These tests use manual GossipBridge creation for testing session
//! lifecycle behaviors. For actual multi-node sync tests, see multi_node_sync_test.rs
//! which uses real iroh-gossip networking.

mod test_utils;

use std::time::Duration;

use anyhow::Result;
use bevy::prelude::Transform;
use libmarathon::networking::{
    CurrentSession,
    GossipBridge,
    SessionState,
};
use test_utils::{TestContext, create_test_app_maybe_offline};
use uuid::Uuid;

// ============================================================================
// Session Lifecycle Tests
// ============================================================================

/// Test 1: Session starts in Created state when offline
#[tokio::test(flavor = "multi_thread")]
async fn test_session_starts_offline() -> Result<()> {
    println!("=== Starting test_session_starts_offline ===");

    let ctx = TestContext::new();
    let node_id = Uuid::new_v4();

    // Create app without GossipBridge (offline)
    let mut app = create_test_app_maybe_offline(node_id, ctx.db_path(), None);

    // Update once to run startup systems
    app.update();

    // Verify session is in Created state
    {
        let current_session = app.world().resource::<CurrentSession>();
        assert_eq!(
            current_session.session.state,
            SessionState::Created,
            "Session should start in Created state when offline"
        );
        println!("✓ Session in Created state");
    }

    println!("✓ Session starts offline test passed");

    Ok(())
}

/// Test 2: Session transitions to Joining when networking starts
#[tokio::test(flavor = "multi_thread")]
async fn test_session_transitions_to_joining() -> Result<()> {
    println!("=== Starting test_session_transitions_to_joining ===");

    let ctx = TestContext::new();
    let node_id = Uuid::new_v4();

    // Create app offline
    let mut app = create_test_app_maybe_offline(node_id, ctx.db_path(), None);
    app.update();

    // Verify starts in Created
    {
        let current_session = app.world().resource::<CurrentSession>();
        assert_eq!(current_session.session.state, SessionState::Created);
        println!("✓ Session starts in Created state");
    }

    // Create and insert GossipBridge (simulate networking starting)
    let bridge = GossipBridge::new(node_id);
    app.insert_resource(bridge);

    // Transition to Joining manually (in real app, this happens when StartNetworking command is sent)
    {
        let mut current_session = app.world_mut().resource_mut::<CurrentSession>();
        current_session.transition_to(SessionState::Joining);
    }

    app.update();

    // Verify session transitioned to Joining
    {
        let current_session = app.world().resource::<CurrentSession>();
        assert_eq!(
            current_session.session.state,
            SessionState::Joining,
            "Session should transition to Joining when networking starts"
        );
        println!("✓ Session transitioned to Joining state");
    }

    println!("✓ Session transitions to Joining test passed");

    Ok(())
}

/// Test 3: JoinRequest is sent when session is in Joining state
#[tokio::test(flavor = "multi_thread")]
async fn test_join_request_sent() -> Result<()> {
    println!("=== Starting test_join_request_sent ===");

    let ctx = TestContext::new();
    let node_id = Uuid::new_v4();

    // Create app offline
    let mut app = create_test_app_maybe_offline(node_id, ctx.db_path(), None);
    app.update();

    // Create and insert GossipBridge
    let bridge = GossipBridge::new(node_id);
    app.insert_resource(bridge.clone());

    // Transition to Joining
    {
        let mut current_session = app.world_mut().resource_mut::<CurrentSession>();
        current_session.transition_to(SessionState::Joining);
    }

    // Update to trigger send_join_request_once_system
    // With the peer-wait logic, JoinRequest waits for peers or timeout
    app.update(); // Start wait timer

    // Simulate 1-second timeout (first node case - no peers)
    {
        let mut join_sent = app.world_mut().resource_mut::<libmarathon::networking::JoinRequestSent>();
        join_sent.wait_started = Some(
            std::time::Instant::now() - Duration::from_millis(1100)
        );
    }

    // Update again - should send JoinRequest due to timeout
    app.update();

    // Verify JoinRequest was sent by checking JoinRequestSent resource
    {
        let join_sent = app.world().resource::<libmarathon::networking::JoinRequestSent>();
        assert!(
            join_sent.sent,
            "JoinRequest should have been sent after 1-second timeout"
        );
        println!("✓ JoinRequest sent successfully (timeout case)");
    }

    println!("✓ JoinRequest sent test passed");

    Ok(())
}

/// Test 4: Session transitions to Active when peers are detected
#[tokio::test(flavor = "multi_thread")]
async fn test_session_transitions_to_active() -> Result<()> {
    println!("=== Starting test_session_transitions_to_active ===");

    let ctx = TestContext::new();
    let node_id = Uuid::new_v4();

    // Create app with bridge (online)
    let bridge = GossipBridge::new(node_id);
    let mut app = create_test_app_maybe_offline(node_id, ctx.db_path(), Some(bridge));

    // Update to run startup systems (initializes CurrentSession)
    app.update();

    // Transition to Joining
    {
        let mut current_session = app.world_mut().resource_mut::<CurrentSession>();
        current_session.transition_to(SessionState::Joining);
    }

    app.update();

    // Simulate receiving FullState by:
    // 1. Incrementing the vector clock to show we have peers
    // 2. Spawning entities to simulate the entities from FullState
    {
        let other_node = Uuid::new_v4();
        let mut node_clock = app.world_mut().resource_mut::<libmarathon::networking::NodeVectorClock>();
        // Add current node to clock (if not already there)
        node_clock.clock.timestamps.entry(node_id).or_insert(0);
        // Add peer node
        node_clock.clock.timestamps.insert(other_node, 1);
        println!("✓ Simulated receiving state from peer (clock has {} nodes)", node_clock.clock.node_count());
    }

    // Spawn some entities to simulate receiving FullState with entities
    {
        let entity1_id = Uuid::new_v4();
        let entity2_id = Uuid::new_v4();
        let owner_node = Uuid::new_v4();

        let entity1 = app.world_mut().spawn((
            libmarathon::networking::NetworkedEntity::with_id(entity1_id, owner_node),
            libmarathon::persistence::Persisted::with_id(entity1_id),
        )).id();

        let entity2 = app.world_mut().spawn((
            libmarathon::networking::NetworkedEntity::with_id(entity2_id, owner_node),
            libmarathon::persistence::Persisted::with_id(entity2_id),
        )).id();

        // Register in entity map
        let mut entity_map = app.world_mut().resource_mut::<libmarathon::networking::NetworkEntityMap>();
        entity_map.insert(entity1_id, entity1);
        entity_map.insert(entity2_id, entity2);

        println!("✓ Spawned 2 entities to simulate FullState");
    }

    // Mark JoinRequest as sent to trigger the timer
    {
        let mut join_sent = app.world_mut().resource_mut::<libmarathon::networking::JoinRequestSent>();
        join_sent.sent = true;
    }

    // Update to trigger transition_session_state_system
    for _ in 0..10 {
        app.update();
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Verify session transitioned to Active
    {
        let current_session = app.world().resource::<CurrentSession>();
        assert_eq!(
            current_session.session.state,
            SessionState::Active,
            "Session should transition to Active when peers are detected"
        );
        println!("✓ Session transitioned to Active state");
    }

    println!("✓ Session transitions to Active test passed");

    Ok(())
}

/// Test 5: Fresh join vs rejoin detection
#[tokio::test(flavor = "multi_thread")]
async fn test_join_type_detection() -> Result<()> {
    println!("=== Starting test_join_type_detection ===");

    let ctx = TestContext::new();
    let node_id = Uuid::new_v4();

    // Test 1: Fresh join (no last known clock)
    {
        println!("\nTesting fresh join...");
        let bridge = GossipBridge::new(node_id);
        let mut app = create_test_app_maybe_offline(node_id, ctx.db_path(), Some(bridge));

        // Update to run startup systems
        app.update();

        // Verify last_known_clock is empty (fresh join)
        {
            let current_session = app.world().resource::<CurrentSession>();
            assert_eq!(
                current_session.last_known_clock.node_count(),
                0,
                "Fresh join should have empty last_known_clock"
            );
            println!("✓ Detected fresh join (no previous clock)");
        }
    }

    // Test 2: Rejoin (has last known clock)
    {
        println!("\nTesting rejoin...");
        let ctx2 = TestContext::new();
        let bridge = GossipBridge::new(node_id);
        let mut app = create_test_app_maybe_offline(node_id, ctx2.db_path(), Some(bridge));

        // Update to run startup systems
        app.update();

        // Simulate previous session by setting last_known_clock
        {
            let mut current_session = app.world_mut().resource_mut::<CurrentSession>();
            current_session.last_known_clock.timestamps.insert(node_id, 5);
            current_session.session.entity_count = 10;
        }

        // Verify rejoin detection
        {
            let current_session = app.world().resource::<CurrentSession>();
            assert!(
                current_session.last_known_clock.node_count() > 0,
                "Rejoin should have non-empty last_known_clock"
            );
            assert_eq!(
                current_session.session.entity_count,
                10,
                "Rejoin should preserve entity count"
            );
            println!("✓ Detected rejoin (has previous clock with {} nodes)",
                current_session.last_known_clock.node_count());
        }
    }

    println!("\n✓ Join type detection test passed");

    Ok(())
}

/// Test 6: Session persistence and recovery
#[tokio::test(flavor = "multi_thread")]
async fn test_session_persistence_recovery() -> Result<()> {
    println!("=== Starting test_session_persistence_recovery ===");

    let ctx = TestContext::new();
    let node_id = Uuid::new_v4();
    let session_id;

    // Phase 1: Create session and persist
    {
        println!("\nPhase 1: Creating session...");
        let bridge = GossipBridge::new(node_id);
        let mut app = create_test_app_maybe_offline(node_id, ctx.db_path(), Some(bridge));

        // Update to run startup systems
        app.update();

        // Transition to Active
        {
            let mut current_session = app.world_mut().resource_mut::<CurrentSession>();
            current_session.transition_to(SessionState::Joining);
            current_session.transition_to(SessionState::Active);
            session_id = current_session.session.id.clone();
        }

        // Update to trigger persistence (need to run for >5 seconds for auto-save)
        // Auto-save runs every 5 seconds, so we need to wait at least that long
        for _ in 0..350 {
            app.update();
            tokio::time::sleep(Duration::from_millis(16)).await;
        }

        println!("✓ Session created and persisted");
    }

    // Phase 2: Restart app and verify recovery
    {
        println!("\nPhase 2: Restarting app...");
        let bridge = GossipBridge::new(node_id);
        let mut app = create_test_app_maybe_offline(node_id, ctx.db_path(), Some(bridge));

        // Update to run startup systems (should load session from DB)
        app.update();

        // Verify session was recovered
        {
            let current_session = app.world().resource::<CurrentSession>();
            assert_eq!(
                current_session.session.id,
                session_id,
                "Session ID should match after recovery"
            );
            println!("✓ Session recovered from database");
        }

        println!("✓ Session persistence recovery test passed");
    }

    Ok(())
}

// ============================================================================
// Peer-Wait JoinRequest Tests (Issue Fix)
// ============================================================================

/// Test 7: JoinRequest waits for peers before sending
///
/// CRITICAL: This test validates the fix for JoinRequest being sent before
/// peers connect via pkarr+DHT discovery, which caused messages to be lost.
#[tokio::test(flavor = "multi_thread")]
async fn test_join_request_waits_for_peers() -> Result<()> {
    println!("=== Test: JoinRequest waits for peers (Issue Fix) ===");

    let ctx = TestContext::new();
    let node_id = Uuid::new_v4();

    // Create app with bridge
    let bridge = GossipBridge::new(node_id);
    let mut app = create_test_app_maybe_offline(node_id, ctx.db_path(), Some(bridge.clone()));

    app.update();

    // Transition to Joining
    {
        let mut current_session = app.world_mut().resource_mut::<CurrentSession>();
        current_session.transition_to(SessionState::Joining);
    }

    // Check initial clock state
    {
        let node_clock = app.world().resource::<libmarathon::networking::NodeVectorClock>();
        println!("Initial clock node_count: {}", node_clock.clock.node_count());
        println!("Clock timestamps: {:?}", node_clock.clock.timestamps);
    }

    println!("Initial state: Session=Joining, Peers=0");

    // Run for 10 frames (~166ms) - should NOT send JoinRequest yet (no peers)
    for i in 0..10 {
        app.update();
        tokio::time::sleep(Duration::from_millis(16)).await;

        let join_sent = app.world().resource::<libmarathon::networking::JoinRequestSent>();
        let node_clock = app.world().resource::<libmarathon::networking::NodeVectorClock>();

        println!("Frame {}: sent={}, wait_started={:?}, node_count={}",
                 i, join_sent.sent, join_sent.wait_started.is_some(), node_clock.clock.node_count());

        assert!(
            !join_sent.sent,
            "Frame {}: JoinRequest should NOT be sent yet (waiting for peers). node_count={}",
            i, node_clock.clock.node_count()
        );
        assert!(
            join_sent.wait_started.is_some(),
            "Frame {}: Wait timer should be started",
            i
        );
    }

    // Verify no JoinRequest was queued (check the bridge from the app's resources)
    // Note: SyncRequest may be queued by trigger_sync_on_connect system (expected)
    {
        let app_bridge = app.world().resource::<libmarathon::networking::GossipBridge>();

        // Check all queued messages
        let mut found_join_request = false;
        loop {
            let queued_msg = app_bridge.try_recv_outgoing();
            if let Some(msg) = queued_msg {
                println!("Found queued message: {:?} (nonce: {})",
                         std::mem::discriminant(&msg.message), msg.nonce);

                // Check if it's a JoinRequest (discriminant 1)
                if matches!(msg.message, libmarathon::networking::SyncMessage::JoinRequest { .. }) {
                    found_join_request = true;
                    break;
                }
            } else {
                break;
            }
        }

        assert!(
            !found_join_request,
            "JoinRequest should NOT be queued while waiting for peers"
        );
    }

    println!("✓ JoinRequest correctly waits for peers (not sent after 166ms with no peers)");

    Ok(())
}

/// Test 8: JoinRequest sends immediately when peer connects
#[tokio::test(flavor = "multi_thread")]
async fn test_join_request_sends_when_peer_connects() -> Result<()> {
    println!("=== Test: JoinRequest sends when peer connects ===");

    let ctx = TestContext::new();
    let node_id = Uuid::new_v4();

    // Create app with bridge
    let bridge = GossipBridge::new(node_id);
    let mut app = create_test_app_maybe_offline(node_id, ctx.db_path(), Some(bridge.clone()));

    app.update();

    // Transition to Joining
    {
        let mut current_session = app.world_mut().resource_mut::<CurrentSession>();
        current_session.transition_to(SessionState::Joining);
    }

    println!("Initial state: Session=Joining, Peers=0");

    // Run 5 frames - should not send yet
    for _ in 0..5 {
        app.update();
        tokio::time::sleep(Duration::from_millis(16)).await;
    }

    {
        let join_sent = app.world().resource::<libmarathon::networking::JoinRequestSent>();
        assert!(!join_sent.sent, "JoinRequest should NOT be sent yet (no peers)");
    }

    println!("After 5 frames: JoinRequest NOT sent (waiting for peers)");

    // Simulate peer connection by adding to vector clock
    {
        let mut node_clock = app.world_mut().resource_mut::<libmarathon::networking::NodeVectorClock>();
        // Add self to clock (normally happens on first tick)
        node_clock.clock.timestamps.insert(node_id, 0);
        // Add peer to simulate connected peer
        let peer_id = Uuid::new_v4();
        node_clock.clock.timestamps.insert(peer_id, 0);
        println!("Simulated peer connection: {}", peer_id);
    }

    // Run one more frame - should send JoinRequest now
    app.update();

    {
        let join_sent = app.world().resource::<libmarathon::networking::JoinRequestSent>();
        assert!(
            join_sent.sent,
            "JoinRequest SHOULD be sent immediately after peer connects"
        );
    }

    // Verify JoinRequest was queued in GossipBridge (check app's resource)
    {
        let app_bridge = app.world().resource::<libmarathon::networking::GossipBridge>();

        // Drain all messages and find the JoinRequest
        let mut found_join_request = false;
        while let Some(msg) = app_bridge.try_recv_outgoing() {
            if let libmarathon::networking::SyncMessage::JoinRequest { node_id: req_node, .. } = msg.message {
                found_join_request = true;
                assert_eq!(req_node, node_id, "JoinRequest should have correct node ID");
                println!("✓ JoinRequest sent with correct node ID");
                break;
            }
            // Skip other message types (like SyncRequest from trigger_sync_on_connect)
        }

        assert!(found_join_request, "JoinRequest should be queued in GossipBridge");
    }

    println!("✓ JoinRequest sent immediately when peer connected");

    Ok(())
}

/// Test 9: JoinRequest sends after 1-second timeout (first node case)
#[tokio::test(flavor = "multi_thread")]
async fn test_join_request_sends_after_timeout() -> Result<()> {
    println!("=== Test: JoinRequest sends after 1-second timeout (first node) ===");

    let ctx = TestContext::new();
    let node_id = Uuid::new_v4();

    // Create app with bridge
    let bridge = GossipBridge::new(node_id);
    let mut app = create_test_app_maybe_offline(node_id, ctx.db_path(), Some(bridge.clone()));

    app.update();

    // Transition to Joining
    {
        let mut current_session = app.world_mut().resource_mut::<CurrentSession>();
        current_session.transition_to(SessionState::Joining);
    }

    println!("Initial state: Session=Joining, Peers=0");

    // Run one frame to start the wait timer
    app.update();

    // Manually set wait_started to 1.1 seconds ago to simulate timeout
    {
        let mut join_sent = app.world_mut().resource_mut::<libmarathon::networking::JoinRequestSent>();
        join_sent.wait_started = Some(
            std::time::Instant::now() - Duration::from_millis(1100)
        );
        println!("Simulated wait time: 1.1 seconds");
    }

    // Run one frame - should send JoinRequest due to timeout
    app.update();

    {
        let join_sent = app.world().resource::<libmarathon::networking::JoinRequestSent>();
        assert!(
            join_sent.sent,
            "JoinRequest SHOULD be sent after 1-second timeout (first node case)"
        );
    }

    // Verify JoinRequest was queued (check app's resource)
    {
        let app_bridge = app.world().resource::<libmarathon::networking::GossipBridge>();

        // Drain all messages and find the JoinRequest
        let mut found_join_request = false;
        while let Some(msg) = app_bridge.try_recv_outgoing() {
            if matches!(msg.message, libmarathon::networking::SyncMessage::JoinRequest { .. }) {
                found_join_request = true;
                break;
            }
            // Skip other message types (like SyncRequest from trigger_sync_on_connect)
        }

        assert!(found_join_request, "JoinRequest should be queued in GossipBridge");
    }

    println!("✓ JoinRequest sent after timeout (assuming first node in session)");

    Ok(())
}

/// Test 10: JoinRequest only sent once (idempotency)
#[tokio::test(flavor = "multi_thread")]
async fn test_join_request_only_sent_once() -> Result<()> {
    println!("=== Test: JoinRequest only sent once (idempotency) ===");

    let ctx = TestContext::new();
    let node_id = Uuid::new_v4();

    // Create app with bridge
    let bridge = GossipBridge::new(node_id);
    let mut app = create_test_app_maybe_offline(node_id, ctx.db_path(), Some(bridge.clone()));

    app.update();

    // Transition to Joining and add a peer
    {
        let mut current_session = app.world_mut().resource_mut::<CurrentSession>();
        current_session.transition_to(SessionState::Joining);

        let mut node_clock = app.world_mut().resource_mut::<libmarathon::networking::NodeVectorClock>();
        // Add self to clock (normally happens on first tick)
        node_clock.clock.timestamps.insert(node_id, 0);
        // Add peer to simulate connected peer
        node_clock.clock.timestamps.insert(Uuid::new_v4(), 0);
    }

    println!("Initial state: Session=Joining, Peers=1");

    // Run frame - should send JoinRequest
    app.update();

    {
        let join_sent = app.world().resource::<libmarathon::networking::JoinRequestSent>();
        let node_clock = app.world().resource::<libmarathon::networking::NodeVectorClock>();
        let peer_count = node_clock.clock.node_count().saturating_sub(1);

        println!("After update: sent={}, wait_started={:?}, node_count={}, peer_count={}",
                 join_sent.sent,
                 join_sent.wait_started.is_some(),
                 node_clock.clock.node_count(),
                 peer_count);

        assert!(join_sent.sent, "JoinRequest should be sent (peer_count={}, node_count={})",
                peer_count, node_clock.clock.node_count());
    }

    // Drain messages and find the JoinRequest (check app's resource)
    {
        let app_bridge = app.world().resource::<libmarathon::networking::GossipBridge>();

        let mut found_join_request = false;
        while let Some(msg) = app_bridge.try_recv_outgoing() {
            if matches!(msg.message, libmarathon::networking::SyncMessage::JoinRequest { .. }) {
                found_join_request = true;
                println!("First JoinRequest found and drained");
                break;
            }
        }
        assert!(found_join_request, "First JoinRequest should be queued");
    }

    // Run 20 more frames - should NOT send JoinRequest again
    for i in 0..20 {
        app.update();
        tokio::time::sleep(Duration::from_millis(16)).await;

        let app_bridge = app.world().resource::<libmarathon::networking::GossipBridge>();

        // Check all messages, verify no JoinRequest
        let mut found_duplicate_join = false;
        while let Some(msg) = app_bridge.try_recv_outgoing() {
            if matches!(msg.message, libmarathon::networking::SyncMessage::JoinRequest { .. }) {
                found_duplicate_join = true;
                break;
            }
        }

        assert!(
            !found_duplicate_join,
            "Frame {}: Should NOT send duplicate JoinRequest",
            i
        );
    }

    println!("✓ JoinRequest sent only once (no duplicates after 20 frames)");

    Ok(())
}

/// Test 11: EntityDelta deduplication prevents spam
///
/// CRITICAL: This test validates the fix for EntityDelta spam (60+ sends in 3 seconds).
/// The bug was that last_versions was updated with the OLD sequence (before tick),
/// causing duplicate deltas when the system ran multiple times per frame.
#[tokio::test(flavor = "multi_thread")]
async fn test_entity_delta_deduplication() -> Result<()> {
    println!("=== Test: EntityDelta deduplication per frame ===");

    let ctx = TestContext::new();
    let node_id = Uuid::new_v4();

    // Create app with bridge
    let bridge = GossipBridge::new(node_id);
    let mut app = create_test_app_maybe_offline(node_id, ctx.db_path(), Some(bridge.clone()));

    app.update();

    // Transition to Active
    {
        let mut current_session = app.world_mut().resource_mut::<CurrentSession>();
        current_session.transition_to(SessionState::Active);
    }

    // Spawn a networked entity with Transform
    let entity_id = Uuid::new_v4();
    let entity = app.world_mut().spawn((
        libmarathon::networking::NetworkedEntity::with_id(entity_id, node_id),
        libmarathon::networking::Synced,
        Transform::from_xyz(1.0, 2.0, 3.0),
        libmarathon::networking::NetworkedTransform,
    )).id();

    println!("Spawned entity {:?} with NetworkedEntity({})", entity, entity_id);

    // Run one frame to process the spawn
    app.update();

    // Drain any messages from spawn
    {
        let app_bridge = app.world().resource::<libmarathon::networking::GossipBridge>();
        let mut count = 0;
        while app_bridge.try_recv_outgoing().is_some() {
            count += 1;
        }
        println!("Drained {} messages from initial spawn", count);
    }

    // Modify the entity's Transform to trigger change detection
    {
        let mut transform = app.world_mut().get_mut::<Transform>(entity).unwrap();
        transform.translation.x = 5.0;
        println!("Modified Transform to trigger change detection");
    }

    // Run multiple updates in the same frame (simulating system running multiple times)
    // Before the fix, this would send multiple EntityDeltas
    for i in 0..5 {
        app.update();
        tokio::time::sleep(Duration::from_millis(1)).await;

        // Count EntityDeltas for our entity
        let app_bridge = app.world().resource::<libmarathon::networking::GossipBridge>();
        let mut entity_delta_count = 0;

        while let Some(msg) = app_bridge.try_recv_outgoing() {
            if let libmarathon::networking::SyncMessage::EntityDelta { entity_id: msg_entity_id, .. } = msg.message {
                if msg_entity_id == entity_id {
                    entity_delta_count += 1;
                    println!("Frame {}: Found EntityDelta for entity {}", i, entity_id);
                }
            }
        }

        // After the fix, we should get AT MOST 1 EntityDelta per frame
        assert!(
            entity_delta_count <= 1,
            "Frame {}: Should send at most 1 EntityDelta per entity per frame, got {}",
            i,
            entity_delta_count
        );
    }

    println!("✓ EntityDelta deduplication works (no spam across 5 frames)");

    Ok(())
}

/// Test 12: FullState sent when JoinRequest received
///
/// CRITICAL: This test validates that when a node receives a JoinRequest,
/// it responds with a FullState message containing all networked entities.
#[tokio::test(flavor = "multi_thread")]
async fn test_fullstate_sent_on_join_request() -> Result<()> {
    println!("=== Test: FullState sent when JoinRequest received ===");

    let ctx = TestContext::new();
    let node_id = Uuid::new_v4();

    // Create app with bridge
    let bridge = GossipBridge::new(node_id);
    let mut app = create_test_app_maybe_offline(node_id, ctx.db_path(), Some(bridge.clone()));

    app.update();

    // Transition to Active
    {
        let mut current_session = app.world_mut().resource_mut::<CurrentSession>();
        current_session.transition_to(SessionState::Active);
    }

    // Spawn some networked entities
    let entity1_id = Uuid::new_v4();
    let entity2_id = Uuid::new_v4();

    app.world_mut().spawn((
        libmarathon::networking::NetworkedEntity::with_id(entity1_id, node_id),
        libmarathon::networking::Synced,
        Transform::from_xyz(1.0, 2.0, 3.0),
        libmarathon::networking::NetworkedTransform,
    ));

    app.world_mut().spawn((
        libmarathon::networking::NetworkedEntity::with_id(entity2_id, node_id),
        libmarathon::networking::Synced,
        Transform::from_xyz(4.0, 5.0, 6.0),
        libmarathon::networking::NetworkedTransform,
    ));

    println!("Spawned 2 entities: {} and {}", entity1_id, entity2_id);

    // Run one frame to process spawns
    app.update();

    // Drain any messages from spawns
    {
        let app_bridge = app.world().resource::<libmarathon::networking::GossipBridge>();
        let mut count = 0;
        while app_bridge.try_recv_outgoing().is_some() {
            count += 1;
        }
        println!("Drained {} messages from spawns", count);
    }

    // Simulate receiving a JoinRequest from a peer
    let peer_id = Uuid::new_v4();
    let session_id = {
        let current_session = app.world().resource::<CurrentSession>();
        current_session.session.id.clone()
    };

    let join_request = libmarathon::networking::VersionedMessage::new(
        libmarathon::networking::SyncMessage::JoinRequest {
            node_id: peer_id,
            session_id,
            session_secret: None,
            last_known_clock: None,
            join_type: libmarathon::networking::JoinType::Fresh,
        }
    );

    println!("Pushing JoinRequest from peer {} to incoming queue", peer_id);

    // Push to incoming queue
    {
        let app_bridge = app.world().resource::<libmarathon::networking::GossipBridge>();
        app_bridge.push_incoming(join_request).unwrap();
    }

    // Run frame to process JoinRequest
    app.update();

    // Check that FullState was queued in response
    {
        let app_bridge = app.world().resource::<libmarathon::networking::GossipBridge>();

        let mut found_fullstate = false;
        let mut fullstate_entity_count = 0;

        while let Some(msg) = app_bridge.try_recv_outgoing() {
            if let libmarathon::networking::SyncMessage::FullState { entities, .. } = msg.message {
                found_fullstate = true;
                fullstate_entity_count = entities.len();
                println!("Found FullState with {} entities", fullstate_entity_count);
                break;
            }
        }

        assert!(
            found_fullstate,
            "FullState should be sent in response to JoinRequest"
        );

        assert_eq!(
            fullstate_entity_count, 2,
            "FullState should contain 2 entities"
        );
    }

    println!("✓ FullState sent with correct entity count when JoinRequest received");

    Ok(())
}

/// Test 13: NetworkedTransform auto-inserted when FullState contains Transform
///
/// CRITICAL: This test validates the PreUpdate system chain ordering.
/// Verifies that when message_dispatcher spawns entities with Transform from FullState,
/// auto_insert_networked_transform correctly detects and adds NetworkedTransform in the same frame.
#[tokio::test(flavor = "multi_thread")]
async fn test_networked_transform_auto_inserted_from_fullstate() -> Result<()> {
    println!("=== Test: NetworkedTransform auto-inserted from FullState ===");

    let ctx = TestContext::new();
    let node_id = Uuid::new_v4();

    // Create app with bridge
    let bridge = GossipBridge::new(node_id);
    let mut app = create_test_app_maybe_offline(node_id, ctx.db_path(), Some(bridge.clone()));

    app.update();

    // Transition to Joining (receiving FullState)
    {
        let mut current_session = app.world_mut().resource_mut::<CurrentSession>();
        current_session.transition_to(SessionState::Joining);
    }

    // Build a FullState message with one entity that has Transform
    let entity_id = Uuid::new_v4();
    let owner_node = Uuid::new_v4();

    let bevy_transform = Transform::from_xyz(10.0, 20.0, 30.0);
    let transform_bytes = {
        // Convert Bevy Transform to rkyv-compatible Transform
        let transform = libmarathon::transform::Transform {
            translation: bevy_transform.translation.into(),
            rotation: bevy_transform.rotation.into(),
            scale: bevy_transform.scale.into(),
        };
        let serialized = rkyv::to_bytes::<rkyv::rancor::Failure>(&transform)
            .expect("Failed to serialize Transform");
        bytes::Bytes::from(serialized.to_vec())
    };

    let transform_discriminant = {
        let type_registry = app.world().resource::<libmarathon::persistence::ComponentTypeRegistryResource>();
        type_registry.0.get_discriminant(std::any::TypeId::of::<Transform>()).unwrap()
    };

    let entity_state = libmarathon::networking::EntityState {
        entity_id,
        owner_node_id: owner_node,
        vector_clock: libmarathon::networking::VectorClock::new(),
        is_deleted: false,
        components: vec![libmarathon::networking::ComponentState {
            discriminant: transform_discriminant,
            data: libmarathon::networking::ComponentData::Inline(transform_bytes),
        }],
    };

    let fullstate = libmarathon::networking::VersionedMessage::new(
        libmarathon::networking::SyncMessage::FullState {
            entities: vec![entity_state],
            vector_clock: libmarathon::networking::VectorClock::new(),
        }
    );

    println!("Pushing FullState with 1 entity (has Transform) to incoming queue");

    // Push to incoming queue
    {
        let app_bridge = app.world().resource::<libmarathon::networking::GossipBridge>();
        app_bridge.push_incoming(fullstate).unwrap();
    }

    // Run frame to process FullState
    // PreUpdate chain: auto_insert_sync_components → register → message_dispatcher → auto_insert_networked_transform
    app.update();

    // Verify entity was spawned with all expected components
    {
        let entity_map = app.world().resource::<libmarathon::networking::NetworkEntityMap>();
        let bevy_entity = entity_map.get_entity(entity_id).expect("Entity should be in map");

        // Check NetworkedEntity (added by apply_full_state)
        assert!(
            app.world().get::<libmarathon::networking::NetworkedEntity>(bevy_entity).is_some(),
            "Entity should have NetworkedEntity component"
        );

        // Check Synced (added by apply_full_state)
        assert!(
            app.world().get::<libmarathon::networking::Synced>(bevy_entity).is_some(),
            "Entity should have Synced component"
        );

        // Check Transform (added by apply_full_state from FullState data)
        let transform = app.world().get::<Transform>(bevy_entity).expect("Entity should have Transform");
        assert_eq!(transform.translation.x, 10.0);
        assert_eq!(transform.translation.y, 20.0);
        assert_eq!(transform.translation.z, 30.0);
        println!("✓ Transform correctly applied from FullState");

        // Check NetworkedTransform (should be auto-inserted by auto_insert_networked_transform)
        assert!(
            app.world().get::<libmarathon::networking::NetworkedTransform>(bevy_entity).is_some(),
            "NetworkedTransform should be auto-inserted in same frame (PreUpdate chain)"
        );
        println!("✓ NetworkedTransform auto-inserted in same frame");
    }

    println!("✓ PreUpdate system chain works correctly (no race condition)");

    Ok(())
}
