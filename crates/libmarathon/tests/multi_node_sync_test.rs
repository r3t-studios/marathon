//! Multi-node sync integration tests with real iroh-gossip networking
//!
//! These tests verify actual message passing and synchronization between nodes
//! using real iroh-gossip with localhost connections.

mod test_utils;

use anyhow::Result;
use test_utils::{TestContext, create_test_app, wait_for_sync, count_entities_with_id, setup_gossip_pair, setup_gossip_trio};
use bevy::prelude::*;
use iroh::{Endpoint, protocol::Router};
use libmarathon::networking::{
    CurrentSession,
    NetworkEntityMap,
    NetworkedEntity,
    SessionState,
    Synced,
};
use libmarathon::persistence::Persisted;
use std::time::Duration;
use tokio::time::Instant;
use uuid::Uuid;

// ============================================================================
// Integration Tests
// ============================================================================
// Note: All test utilities (gossip setup, test app creation, sync helpers)
// are now in the shared test_utils module to avoid duplication

/// Test: Two nodes can synchronize a cube spawn using real iroh-gossip
#[tokio::test(flavor = "multi_thread")]
async fn test_two_nodes_sync_cube_spawn() -> Result<()> {
    println!("=== Starting test_two_nodes_sync_cube_spawn ===");

    let ctx1 = TestContext::new();
    let ctx2 = TestContext::new();

    let (ep1, ep2, router1, router2, bridge1, bridge2) = setup_gossip_pair().await?;

    let node1_id = bridge1.node_id();
    let node2_id = bridge2.node_id();

    let mut app1 = create_test_app(node1_id, ctx1.db_path(), bridge1);
    let mut app2 = create_test_app(node2_id, ctx2.db_path(), bridge2);

    println!("Node1 ID: {}", node1_id);
    println!("Node2 ID: {}", node2_id);

    // Initialize both apps
    app1.update();
    app2.update();

    // Connect both to the same session
    use libmarathon::networking::SessionId;
    let session_id = SessionId::new();

    {
        let mut session1 = app1.world_mut().resource_mut::<CurrentSession>();
        session1.session.id = session_id.clone();
        session1.transition_to(SessionState::Active);
    }
    {
        let mut session2 = app2.world_mut().resource_mut::<CurrentSession>();
        session2.session.id = session_id.clone();
        session2.transition_to(SessionState::Active);
    }

    // Node 1: Spawn a cube
    let cube_id = Uuid::new_v4();
    {
        app1.world_mut().spawn((
            NetworkedEntity::with_id(cube_id, node1_id),
            Persisted::with_id(cube_id),
            Synced,
            Transform::from_xyz(1.0, 2.0, 3.0),
        ));
    }

    println!("Node1: Spawned cube {}", cube_id);

    // Wait for sync
    wait_for_sync(&mut app1, &mut app2, Duration::from_secs(5), |_w1, w2| {
        let entity_map = w2.resource::<NetworkEntityMap>();
        entity_map.get_entity(cube_id).is_some()
    })
    .await?;

    println!("✓ Node2 successfully received cube from Node1");

    // Verify vector clocks converged
    {
        use libmarathon::networking::NodeVectorClock;

        let clock1 = app1.world().resource::<NodeVectorClock>();
        let clock2 = app2.world().resource::<NodeVectorClock>();

        println!("Node1 clock: {:?}", clock1.clock);
        println!("Node2 clock: {:?}", clock2.clock);

        // Both clocks should know about both nodes
        assert!(
            clock2.clock.node_count() >= 2,
            "Node2 should know about both nodes in its clock"
        );
    }

    // Cleanup
    router1.shutdown().await?;
    router2.shutdown().await?;
    ep1.close().await;
    ep2.close().await;

    println!("✓ Two nodes sync cube spawn test passed");

    Ok(())
}

/// Test: Three nodes maintain consistent state using real iroh-gossip
#[tokio::test(flavor = "multi_thread")]
async fn test_three_nodes_consistency() -> Result<()> {
    println!("=== Starting test_three_nodes_consistency ===");

    let ctx1 = TestContext::new();
    let ctx2 = TestContext::new();
    let ctx3 = TestContext::new();

    let (ep1, ep2, ep3, router1, router2, router3, bridge1, bridge2, bridge3) =
        setup_gossip_trio().await?;

    let node1_id = bridge1.node_id();
    let node2_id = bridge2.node_id();
    let node3_id = bridge3.node_id();

    let mut app1 = create_test_app(node1_id, ctx1.db_path(), bridge1);
    let mut app2 = create_test_app(node2_id, ctx2.db_path(), bridge2);
    let mut app3 = create_test_app(node3_id, ctx3.db_path(), bridge3);

    println!("Node1 ID: {}", node1_id);
    println!("Node2 ID: {}", node2_id);
    println!("Node3 ID: {}", node3_id);

    // Initialize all apps
    app1.update();
    app2.update();
    app3.update();

    // Connect all to the same session
    use libmarathon::networking::SessionId;
    let session_id = SessionId::new();

    for app in [&mut app1, &mut app2, &mut app3] {
        let mut session = app.world_mut().resource_mut::<CurrentSession>();
        session.session.id = session_id.clone();
        session.transition_to(SessionState::Active);
    }

    // Each node spawns a cube
    let cube1_id = Uuid::new_v4();
    let cube2_id = Uuid::new_v4();
    let cube3_id = Uuid::new_v4();

    app1.world_mut().spawn((
        NetworkedEntity::with_id(cube1_id, node1_id),
        Persisted::with_id(cube1_id),
        Synced,
        Transform::from_xyz(1.0, 0.0, 0.0),
    ));

    app2.world_mut().spawn((
        NetworkedEntity::with_id(cube2_id, node2_id),
        Persisted::with_id(cube2_id),
        Synced,
        Transform::from_xyz(0.0, 2.0, 0.0),
    ));

    app3.world_mut().spawn((
        NetworkedEntity::with_id(cube3_id, node3_id),
        Persisted::with_id(cube3_id),
        Synced,
        Transform::from_xyz(0.0, 0.0, 3.0),
    ));

    println!("All nodes spawned their cubes");

    // Wait for convergence - all nodes should have all 3 cubes
    let start = Instant::now();
    let mut converged = false;

    while start.elapsed() < Duration::from_secs(10) {
        app1.update();
        app2.update();
        app3.update();

        let map1 = app1.world().resource::<NetworkEntityMap>();
        let map2 = app2.world().resource::<NetworkEntityMap>();
        let map3 = app3.world().resource::<NetworkEntityMap>();

        let count1 = [cube1_id, cube2_id, cube3_id].iter().filter(|id| map1.get_entity(**id).is_some()).count();
        let count2 = [cube1_id, cube2_id, cube3_id].iter().filter(|id| map2.get_entity(**id).is_some()).count();
        let count3 = [cube1_id, cube2_id, cube3_id].iter().filter(|id| map3.get_entity(**id).is_some()).count();

        if count1 == 3 && count2 == 3 && count3 == 3 {
            println!("✓ All nodes converged to 3 cubes");
            converged = true;
            break;
        }

        tokio::time::sleep(Duration::from_millis(16)).await;
    }

    assert!(converged, "Nodes did not converge to consistent state");

    // Cleanup
    router1.shutdown().await?;
    router2.shutdown().await?;
    router3.shutdown().await?;
    ep1.close().await;
    ep2.close().await;
    ep3.close().await;

    println!("✓ Three nodes consistency test passed");

    Ok(())
}

/// Test: FullState sync does not create duplicate entities
/// This tests the fix for the bug where apply_full_state() would spawn
/// duplicate entities instead of reusing existing ones.
#[tokio::test(flavor = "multi_thread")]
async fn test_fullstate_no_duplicate_entities() -> Result<()> {
    println!("=== Starting test_fullstate_no_duplicate_entities ===");

    let ctx1 = TestContext::new();
    let ctx2 = TestContext::new();

    let (ep1, ep2, router1, router2, bridge1, bridge2) = setup_gossip_pair().await?;

    let node1_id = bridge1.node_id();
    let node2_id = bridge2.node_id();

    let mut app1 = create_test_app(node1_id, ctx1.db_path(), bridge1);
    let mut app2 = create_test_app(node2_id, ctx2.db_path(), bridge2);

    println!("Node1 ID: {}", node1_id);
    println!("Node2 ID: {}", node2_id);

    // Initialize both apps
    app1.update();
    app2.update();

    // Connect both to the same session
    use libmarathon::networking::SessionId;
    let session_id = SessionId::new();

    {
        let mut session1 = app1.world_mut().resource_mut::<CurrentSession>();
        session1.session.id = session_id.clone();
        session1.transition_to(SessionState::Active);
    }
    {
        let mut session2 = app2.world_mut().resource_mut::<CurrentSession>();
        session2.session.id = session_id.clone();
        session2.transition_to(SessionState::Active);
    }

    // Node 1: Spawn multiple cubes
    let cube1_id = Uuid::new_v4();
    let cube2_id = Uuid::new_v4();
    let cube3_id = Uuid::new_v4();

    {
        app1.world_mut().spawn((
            NetworkedEntity::with_id(cube1_id, node1_id),
            Persisted::with_id(cube1_id),
            Synced,
            Transform::from_xyz(1.0, 0.0, 0.0),
        ));

        app1.world_mut().spawn((
            NetworkedEntity::with_id(cube2_id, node1_id),
            Persisted::with_id(cube2_id),
            Synced,
            Transform::from_xyz(2.0, 0.0, 0.0),
        ));

        app1.world_mut().spawn((
            NetworkedEntity::with_id(cube3_id, node1_id),
            Persisted::with_id(cube3_id),
            Synced,
            Transform::from_xyz(3.0, 0.0, 0.0),
        ));
    }

    println!("Node1: Spawned 3 cubes: {}, {}, {}", cube1_id, cube2_id, cube3_id);

    // Wait for sync - Node2 should receive all 3 cubes via FullState
    wait_for_sync(&mut app1, &mut app2, Duration::from_secs(5), |_w1, w2| {
        let entity_map = w2.resource::<NetworkEntityMap>();
        entity_map.get_entity(cube1_id).is_some()
            && entity_map.get_entity(cube2_id).is_some()
            && entity_map.get_entity(cube3_id).is_some()
    })
    .await?;

    println!("✓ Node2 received all cubes from Node1");

    // CRITICAL CHECK: Verify no duplicate entities were created
    // Each unique network_id should appear exactly once
    {
        let count1 = count_entities_with_id(app2.world_mut(), cube1_id);
        let count2 = count_entities_with_id(app2.world_mut(), cube2_id);
        let count3 = count_entities_with_id(app2.world_mut(), cube3_id);

        println!("Entity counts in Node2:");
        println!("  Cube1 ({}): {}", cube1_id, count1);
        println!("  Cube2 ({}): {}", cube2_id, count2);
        println!("  Cube3 ({}): {}", cube3_id, count3);

        assert_eq!(
            count1, 1,
            "Cube1 should appear exactly once, found {} instances",
            count1
        );
        assert_eq!(
            count2, 1,
            "Cube2 should appear exactly once, found {} instances",
            count2
        );
        assert_eq!(
            count3, 1,
            "Cube3 should appear exactly once, found {} instances",
            count3
        );
    }

    // Also verify total entity count matches expected
    {
        use libmarathon::networking::NetworkedEntity;
        let mut query = app2.world_mut().query::<&NetworkedEntity>();
        let total_count = query.iter(app2.world()).count();

        println!("Total NetworkedEntity count in Node2: {}", total_count);

        assert_eq!(
            total_count, 3,
            "Node2 should have exactly 3 networked entities, found {}",
            total_count
        );
    }

    println!("✓ No duplicate entities created - FullState correctly reused existing entities");

    // Continue syncing for a bit to ensure no duplicates appear over time
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        app1.update();
        app2.update();
        tokio::time::sleep(Duration::from_millis(16)).await;
    }

    // Final verification - counts should still be correct
    {
        let count1 = count_entities_with_id(app2.world_mut(), cube1_id);
        let count2 = count_entities_with_id(app2.world_mut(), cube2_id);
        let count3 = count_entities_with_id(app2.world_mut(), cube3_id);

        assert_eq!(count1, 1, "Cube1 count changed during continued sync");
        assert_eq!(count2, 1, "Cube2 count changed during continued sync");
        assert_eq!(count3, 1, "Cube3 count changed during continued sync");
    }

    println!("✓ Entity counts remained stable during continued sync");

    // Cleanup
    router1.shutdown().await?;
    router2.shutdown().await?;
    ep1.close().await;
    ep2.close().await;

    println!("✓ FullState no duplicate entities test passed");

    Ok(())
}

/// Test: Remote delta does not cause feedback loop
/// Verifies that applying a remote operation doesn't trigger re-broadcasting
/// the same change back to the network (runaway vector clock bug)
#[tokio::test(flavor = "multi_thread")]
async fn test_remote_delta_no_feedback_loop() -> Result<()> {
    println!("=== Starting test_remote_delta_no_feedback_loop ===");

    let ctx1 = TestContext::new();
    let ctx2 = TestContext::new();

    let (ep1, ep2, router1, router2, bridge1, bridge2) = setup_gossip_pair().await?;

    let node1_id = bridge1.node_id();
    let node2_id = bridge2.node_id();

    let mut app1 = create_test_app(node1_id, ctx1.db_path(), bridge1);
    let mut app2 = create_test_app(node2_id, ctx2.db_path(), bridge2);

    println!("Node1 ID: {}", node1_id);
    println!("Node2 ID: {}", node2_id);

    // Initialize both apps
    app1.update();
    app2.update();

    // Connect both to the same session
    use libmarathon::networking::SessionId;
    let session_id = SessionId::new();

    {
        let mut session1 = app1.world_mut().resource_mut::<CurrentSession>();
        session1.session.id = session_id.clone();
        session1.transition_to(SessionState::Active);
    }
    {
        let mut session2 = app2.world_mut().resource_mut::<CurrentSession>();
        session2.session.id = session_id.clone();
        session2.transition_to(SessionState::Active);
    }

    // Node 1: Spawn a cube
    let cube_id = Uuid::new_v4();
    {
        app1.world_mut().spawn((
            NetworkedEntity::with_id(cube_id, node1_id),
            Persisted::with_id(cube_id),
            Synced,
            Transform::from_xyz(1.0, 2.0, 3.0),
        ));
    }

    println!("Node1: Spawned cube {}", cube_id);

    // Wait for initial sync
    wait_for_sync(&mut app1, &mut app2, Duration::from_secs(5), |_w1, w2| {
        let entity_map = w2.resource::<NetworkEntityMap>();
        entity_map.get_entity(cube_id).is_some()
    })
    .await?;

    println!("✓ Node2 received cube from Node1");

    // Get initial clock sequences
    let node1_initial_seq = {
        let clock1 = app1.world().resource::<libmarathon::networking::NodeVectorClock>();
        clock1.sequence()
    };
    let node2_initial_seq = {
        let clock2 = app2.world().resource::<libmarathon::networking::NodeVectorClock>();
        clock2.sequence()
    };

    println!("Initial clocks: Node1={}, Node2={}", node1_initial_seq, node2_initial_seq);

    // Run both apps for a few seconds to see if clocks stabilize
    // If there's a feedback loop, clocks will keep incrementing rapidly
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        app1.update();
        app2.update();
        tokio::time::sleep(Duration::from_millis(16)).await;
    }

    // Check final clock sequences
    let node1_final_seq = {
        let clock1 = app1.world().resource::<libmarathon::networking::NodeVectorClock>();
        clock1.sequence()
    };
    let node2_final_seq = {
        let clock2 = app2.world().resource::<libmarathon::networking::NodeVectorClock>();
        clock2.sequence()
    };

    println!("Final clocks: Node1={}, Node2={}", node1_final_seq, node2_final_seq);

    // Calculate clock growth
    let node1_growth = node1_final_seq - node1_initial_seq;
    let node2_growth = node2_final_seq - node2_initial_seq;

    println!("Clock growth: Node1=+{}, Node2=+{}", node1_growth, node2_growth);

    // With feedback loop: clocks would grow by 100s (every frame generates delta)
    // Without feedback loop: clocks should grow by 0-5 (only periodic sync)
    assert!(
        node1_growth < 10,
        "Node1 clock grew too much ({}) - indicates feedback loop",
        node1_growth
    );
    assert!(
        node2_growth < 10,
        "Node2 clock grew too much ({}) - indicates feedback loop",
        node2_growth
    );

    println!("✓ No runaway vector clock - feedback loop prevented");

    // Cleanup
    router1.shutdown().await?;
    router2.shutdown().await?;
    ep1.close().await;
    ep2.close().await;

    println!("✓ Remote delta feedback loop prevention test passed");

    Ok(())
}

/// Test: Local change after remote delta gets broadcast
/// Verifies that making a local change after receiving a remote delta
/// still results in broadcasting the local change (with one frame delay)
#[tokio::test(flavor = "multi_thread")]
async fn test_local_change_after_remote_delta() -> Result<()> {
    println!("=== Starting test_local_change_after_remote_delta ===");

    let ctx1 = TestContext::new();
    let ctx2 = TestContext::new();

    let (ep1, ep2, router1, router2, bridge1, bridge2) = setup_gossip_pair().await?;

    let node1_id = bridge1.node_id();
    let node2_id = bridge2.node_id();

    let mut app1 = create_test_app(node1_id, ctx1.db_path(), bridge1);
    let mut app2 = create_test_app(node2_id, ctx2.db_path(), bridge2);

    println!("Node1 ID: {}", node1_id);
    println!("Node2 ID: {}", node2_id);

    // Initialize both apps
    app1.update();
    app2.update();

    // Connect both to the same session
    use libmarathon::networking::SessionId;
    let session_id = SessionId::new();

    {
        let mut session1 = app1.world_mut().resource_mut::<CurrentSession>();
        session1.session.id = session_id.clone();
        session1.transition_to(SessionState::Active);
    }
    {
        let mut session2 = app2.world_mut().resource_mut::<CurrentSession>();
        session2.session.id = session_id.clone();
        session2.transition_to(SessionState::Active);
    }

    // Node 1: Spawn a cube at position (1, 2, 3)
    let cube_id = Uuid::new_v4();
    {
        app1.world_mut().spawn((
            NetworkedEntity::with_id(cube_id, node1_id),
            Persisted::with_id(cube_id),
            Synced,
            Transform::from_xyz(1.0, 2.0, 3.0),
        ));
    }

    println!("Node1: Spawned cube {} at (1, 2, 3)", cube_id);

    // Wait for sync
    wait_for_sync(&mut app1, &mut app2, Duration::from_secs(5), |_w1, w2| {
        let entity_map = w2.resource::<NetworkEntityMap>();
        entity_map.get_entity(cube_id).is_some()
    })
    .await?;

    println!("✓ Node2 received cube");

    // Node 2: Make a local change (move cube to 10, 20, 30)
    {
        let entity_map = app2.world().resource::<NetworkEntityMap>();
        let entity = entity_map.get_entity(cube_id).expect("Cube should exist on Node2");

        if let Ok(mut entity_mut) = app2.world_mut().get_entity_mut(entity) {
            if let Some(mut transform) = entity_mut.get_mut::<Transform>() {
                transform.translation = Vec3::new(10.0, 20.0, 30.0);
                println!("Node2: Moved cube to (10, 20, 30)");
            }
        }
    }

    // Wait for Node1 to receive the update from Node2
    wait_for_sync(&mut app1, &mut app2, Duration::from_secs(5), |w1, _w2| {
        let entity_map = w1.resource::<NetworkEntityMap>();
        if let Some(entity) = entity_map.get_entity(cube_id) {
            if let Ok(entity_ref) = w1.get_entity(entity) {
                if let Some(transform) = entity_ref.get::<Transform>() {
                    // Check if position is close to (10, 20, 30)
                    let pos = transform.translation;
                    (pos.x - 10.0).abs() < 0.1 &&
                    (pos.y - 20.0).abs() < 0.1 &&
                    (pos.z - 30.0).abs() < 0.1
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        }
    })
    .await?;

    println!("✓ Node1 received local change from Node2");

    // Verify final position on Node1
    {
        let entity_map = app1.world().resource::<NetworkEntityMap>();
        let entity = entity_map.get_entity(cube_id).expect("Cube should exist on Node1");

        if let Ok(entity_ref) = app1.world().get_entity(entity) {
            if let Some(transform) = entity_ref.get::<Transform>() {
                let pos = transform.translation;
                println!("Node1 final position: ({}, {}, {})", pos.x, pos.y, pos.z);

                assert!(
                    (pos.x - 10.0).abs() < 0.1,
                    "X position should be ~10.0, got {}",
                    pos.x
                );
                assert!(
                    (pos.y - 20.0).abs() < 0.1,
                    "Y position should be ~20.0, got {}",
                    pos.y
                );
                assert!(
                    (pos.z - 30.0).abs() < 0.1,
                    "Z position should be ~30.0, got {}",
                    pos.z
                );
            }
        }
    }

    println!("✓ Local change after remote delta was successfully broadcast");

    // Cleanup
    router1.shutdown().await?;
    router2.shutdown().await?;
    ep1.close().await;
    ep2.close().await;

    println!("✓ Local change after remote delta test passed");

    Ok(())
}
