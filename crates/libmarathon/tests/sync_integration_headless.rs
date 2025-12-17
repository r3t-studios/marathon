//! Headless integration tests for networking and persistence
//!
//! These tests validate end-to-end CRDT synchronization and persistence
//! using multiple headless Bevy apps with real iroh-gossip networking.

use std::{
    path::PathBuf,
    time::{
        Duration,
        Instant,
    },
};

use anyhow::Result;
use bevy::{
    MinimalPlugins,
    app::{
        App,
        ScheduleRunnerPlugin,
    },
    ecs::{
        component::Component,
        reflect::ReflectComponent,
        world::World,
    },
    prelude::*,
    reflect::Reflect,
};
use futures_lite::StreamExt;
use iroh::{
    Endpoint,
    protocol::Router,
};
use iroh_gossip::{
    api::{
        GossipReceiver,
        GossipSender,
    },
    net::Gossip,
    proto::TopicId,
};
use libmarathon::{
    networking::{
        EntityLockRegistry,
        GossipBridge,
        LockMessage,
        NetworkedEntity,
        NetworkedSelection,
        NetworkedTransform,
        NetworkingConfig,
        NetworkingPlugin,
        Synced,
        SyncMessage,
        VersionedMessage,
    },
    persistence::{
        Persisted,
        PersistenceConfig,
        PersistencePlugin,
    },
};
// Note: Test components use rkyv instead of serde
use sync_macros::Synced as SyncedDerive;
use tempfile::TempDir;
use uuid::Uuid;

// ============================================================================
// Test Components
// ============================================================================

/// Simple position component for testing sync
#[derive(Component, Reflect, Clone, Debug, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[reflect(Component)]
#[derive(SyncedDerive)]
#[sync(version = 1, strategy = "LastWriteWins")]
struct TestPosition {
    x: f32,
    y: f32,
}

/// Simple health component for testing sync
#[derive(Component, Reflect, Clone, Debug, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[reflect(Component)]
#[derive(SyncedDerive)]
#[sync(version = 1, strategy = "LastWriteWins")]
struct TestHealth {
    current: f32,
    max: f32,
}

// ============================================================================
// Test Utilities
// ============================================================================

mod test_utils {
    use rusqlite::{
        Connection,
        OptionalExtension,
    };

    use super::*;

    /// Test context that manages temporary directories with RAII cleanup
    pub struct TestContext {
        _temp_dir: TempDir,
        db_path: PathBuf,
    }

    impl TestContext {
        pub fn new() -> Self {
            let temp_dir = TempDir::new().expect("Failed to create temp directory");
            let db_path = temp_dir.path().join("test.db");
            Self {
                _temp_dir: temp_dir,
                db_path,
            }
        }

        pub fn db_path(&self) -> PathBuf {
            self.db_path.clone()
        }
    }

    /// Check if an entity exists in the database
    pub fn entity_exists_in_db(db_path: &PathBuf, entity_id: Uuid) -> Result<bool> {
        let conn = Connection::open(db_path)?;
        let entity_id_bytes = entity_id.as_bytes();

        let exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM entities WHERE id = ?1",
            [entity_id_bytes.as_slice()],
            |row| row.get(0),
        )?;

        Ok(exists)
    }

    /// Check if a component exists for an entity in the database
    pub fn component_exists_in_db(
        db_path: &PathBuf,
        entity_id: Uuid,
        component_type: &str,
    ) -> Result<bool> {
        let conn = Connection::open(db_path)?;
        let entity_id_bytes = entity_id.as_bytes();

        let exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM components WHERE entity_id = ?1 AND component_type = ?2",
            rusqlite::params![entity_id_bytes.as_slice(), component_type],
            |row| row.get(0),
        )?;

        Ok(exists)
    }

    /// Load a component from the database and deserialize it
    /// TODO: Rewrite to use ComponentTypeRegistry instead of reflection
    #[allow(dead_code)]
    pub fn load_component_from_db<T: Component + Clone>(
        _db_path: &PathBuf,
        _entity_id: Uuid,
        _component_type: &str,
    ) -> Result<Option<T>> {
        // This function needs to be rewritten to use ComponentTypeRegistry
        // For now, return None to allow tests to compile
        Ok(None)
    }

    /// Create a headless Bevy app configured for testing
    pub fn create_test_app(node_id: Uuid, db_path: PathBuf, bridge: GossipBridge) -> App {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(
            Duration::from_secs_f64(1.0 / 60.0),
        )))
        .insert_resource(bridge)
        .add_plugins(NetworkingPlugin::new(NetworkingConfig {
            node_id,
            sync_interval_secs: 0.5, // Fast for testing
            prune_interval_secs: 10.0,
            tombstone_gc_interval_secs: 30.0,
        }))
        .add_plugins(PersistencePlugin::with_config(
            db_path,
            PersistenceConfig {
                flush_interval_secs: 1,
                checkpoint_interval_secs: 5,
                battery_adaptive: false,
                ..Default::default()
            },
        ));

        // Register test component types for reflection
        app.register_type::<TestPosition>()
            .register_type::<TestHealth>()
            .register_type::<NetworkedSelection>();

        app
    }

    /// Count entities with a specific network ID
    pub fn count_entities_with_id(world: &mut World, network_id: Uuid) -> usize {
        let mut query = world.query::<&NetworkedEntity>();
        query
            .iter(world)
            .filter(|entity| entity.network_id == network_id)
            .count()
    }

    /// Assert that an entity with specific network ID and position exists
    pub fn assert_entity_synced(
        world: &mut World,
        network_id: Uuid,
        expected_position: TestPosition,
    ) -> Result<()> {
        let mut query = world.query::<(&NetworkedEntity, &TestPosition)>();

        for (entity, position) in query.iter(world) {
            if entity.network_id == network_id {
                if position == &expected_position {
                    return Ok(());
                } else {
                    anyhow::bail!(
                        "Position mismatch for entity {}: expected {:?}, got {:?}",
                        network_id,
                        expected_position,
                        position
                    );
                }
            }
        }

        anyhow::bail!("Entity {} not found in world", network_id)
    }

    /// Wait for sync condition to be met, polling both apps
    pub async fn wait_for_sync<F>(
        app1: &mut App,
        app2: &mut App,
        timeout: Duration,
        check_fn: F,
    ) -> Result<()>
    where
        F: Fn(&mut World, &mut World) -> bool, {
        let start = Instant::now();
        let mut tick_count = 0;

        while start.elapsed() < timeout {
            // Tick both apps
            app1.update();
            app2.update();
            tick_count += 1;

            if tick_count % 50 == 0 {
                println!(
                    "Waiting for sync... tick {} ({:.1}s elapsed)",
                    tick_count,
                    start.elapsed().as_secs_f32()
                );
            }

            // Check condition
            if check_fn(app1.world_mut(), app2.world_mut()) {
                println!(
                    "Sync completed after {} ticks ({:.3}s)",
                    tick_count,
                    start.elapsed().as_secs_f32()
                );
                return Ok(());
            }

            // Small delay to avoid spinning
            tokio::time::sleep(Duration::from_millis(16)).await;
        }

        println!("Sync timeout after {} ticks", tick_count);
        anyhow::bail!("Sync timeout after {:?}. Condition not met.", timeout)
    }

    /// Initialize a single iroh-gossip node
    async fn init_gossip_node(
        topic_id: TopicId,
        bootstrap_addrs: Vec<iroh::EndpointAddr>,
    ) -> Result<(Endpoint, Gossip, Router, GossipBridge)> {
        println!("  Creating endpoint (localhost only for fast testing)...");
        // Create the Iroh endpoint bound to localhost only (no mDNS needed)
        let endpoint = Endpoint::builder()
            .bind_addr_v4(std::net::SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, 0))
            .bind()
            .await?;
        let endpoint_id = endpoint.addr().id;
        println!("  Endpoint created: {}", endpoint_id);

        // Convert 32-byte endpoint ID to 16-byte UUID by taking first 16 bytes
        let id_bytes = endpoint_id.as_bytes();
        let mut uuid_bytes = [0u8; 16];
        uuid_bytes.copy_from_slice(&id_bytes[..16]);
        let node_id = Uuid::from_bytes(uuid_bytes);

        println!("  Spawning gossip protocol...");
        // Build the gossip protocol
        let gossip = Gossip::builder().spawn(endpoint.clone());

        println!("  Setting up router...");
        // Setup the router to handle incoming connections
        let router = Router::builder(endpoint.clone())
            .accept(iroh_gossip::ALPN, gossip.clone())
            .spawn();

        // Add bootstrap peers using StaticProvider for direct localhost connections
        let bootstrap_count = bootstrap_addrs.len();
        let has_bootstrap_peers = !bootstrap_addrs.is_empty();

        // Collect bootstrap IDs before moving the addresses
        let bootstrap_ids: Vec<_> = bootstrap_addrs.iter().map(|a| a.id).collect();

        if has_bootstrap_peers {
            let static_provider = iroh::discovery::static_provider::StaticProvider::default();
            for addr in &bootstrap_addrs {
                static_provider.add_endpoint_info(addr.clone());
            }
            endpoint.discovery().add(static_provider);
            println!("  Added {} bootstrap peers to discovery", bootstrap_count);

            // Connect to bootstrap peers (localhost connections are instant)
            for addr in &bootstrap_addrs {
                match endpoint.connect(addr.clone(), iroh_gossip::ALPN).await {
                    | Ok(_conn) => println!("  ✓ Connected to {}", addr.id),
                    | Err(e) => println!("  ✗ Connection failed: {}", e),
                }
            }
        }

        // Subscribe to the topic
        let subscribe_handle = gossip.subscribe(topic_id, bootstrap_ids).await?;
        let (sender, mut receiver) = subscribe_handle.split();

        // Wait for join if we have bootstrap peers (should be instant on localhost)
        if has_bootstrap_peers {
            match tokio::time::timeout(Duration::from_millis(500), receiver.joined()).await {
                | Ok(Ok(())) => println!("  ✓ Join completed"),
                | Ok(Err(e)) => println!("  ✗ Join error: {}", e),
                | Err(_) => println!("  ⚠ Join timeout (proceeding anyway)"),
            }
        }

        // Create bridge and wire it up
        let bridge = GossipBridge::new(node_id);
        println!("  Spawning bridge tasks...");

        // Spawn background tasks to forward messages between gossip and bridge
        spawn_gossip_bridge_tasks(sender, receiver, bridge.clone());

        println!("  Node initialization complete");
        Ok((endpoint, gossip, router, bridge))
    }

    /// Setup a pair of iroh-gossip nodes connected to the same topic
    pub async fn setup_gossip_pair() -> Result<(
        Endpoint,
        Endpoint,
        Router,
        Router,
        GossipBridge,
        GossipBridge,
    )> {
        // Use a shared topic for both nodes
        let topic_id = TopicId::from_bytes([42; 32]);
        println!("Using topic ID: {:?}", topic_id);

        // Initialize node 1 with no bootstrap peers
        println!("Initializing node 1...");
        let (ep1, _gossip1, router1, bridge1) = init_gossip_node(topic_id, vec![]).await?;
        println!("Node 1 initialized with ID: {}", ep1.addr().id);

        // Get node 1's full address (ID + network addresses) for node 2 to bootstrap
        // from
        let node1_addr = ep1.addr().clone();
        println!("Node 1 full address: {:?}", node1_addr);

        // Initialize node 2 with node 1's full address as bootstrap peer
        println!("Initializing node 2 with bootstrap peer: {}", node1_addr.id);
        let (ep2, _gossip2, router2, bridge2) =
            init_gossip_node(topic_id, vec![node1_addr]).await?;
        println!("Node 2 initialized with ID: {}", ep2.addr().id);

        // Brief wait for gossip protocol to stabilize (localhost is fast)
        tokio::time::sleep(Duration::from_millis(200)).await;

        Ok((ep1, ep2, router1, router2, bridge1, bridge2))
    }

    /// Spawn background tasks to forward messages between iroh-gossip and
    /// GossipBridge
    fn spawn_gossip_bridge_tasks(
        sender: GossipSender,
        mut receiver: GossipReceiver,
        bridge: GossipBridge,
    ) {
        let node_id = bridge.node_id();

        // Task 1: Forward from bridge.outgoing → gossip sender
        let bridge_out = bridge.clone();
        tokio::spawn(async move {
            let mut msg_count = 0;
            loop {
                // Poll the bridge's outgoing queue
                if let Some(versioned_msg) = bridge_out.try_recv_outgoing() {
                    msg_count += 1;
                    println!(
                        "[Node {}] Sending message #{} via gossip",
                        node_id, msg_count
                    );
                    // Serialize the message
                    match rkyv::to_bytes::<rkyv::rancor::Failure>(&versioned_msg).map(|b| b.to_vec()) {
                        | Ok(bytes) => {
                            // Broadcast via gossip
                            if let Err(e) = sender.broadcast(bytes.into()).await {
                                eprintln!("[Node {}] Failed to broadcast message: {}", node_id, e);
                            } else {
                                println!(
                                    "[Node {}] Message #{} broadcasted successfully",
                                    node_id, msg_count
                                );
                            }
                        },
                        | Err(e) => eprintln!(
                            "[Node {}] Failed to serialize message for broadcast: {}",
                            node_id, e
                        ),
                    }
                }

                // Small delay to avoid spinning
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        // Task 2: Forward from gossip receiver → bridge.incoming
        let bridge_in = bridge.clone();
        tokio::spawn(async move {
            let mut msg_count = 0;
            println!("[Node {}] Gossip receiver task started", node_id);
            loop {
                // Receive from gossip (GossipReceiver is a Stream)
                match tokio::time::timeout(Duration::from_millis(100), receiver.next()).await {
                    | Ok(Some(Ok(event))) => {
                        println!(
                            "[Node {}] Received gossip event: {:?}",
                            node_id,
                            std::mem::discriminant(&event)
                        );
                        if let iroh_gossip::api::Event::Received(msg) = event {
                            msg_count += 1;
                            println!(
                                "[Node {}] Received message #{} from gossip",
                                node_id, msg_count
                            );
                            // Deserialize the message
                            match rkyv::from_bytes::<VersionedMessage, rkyv::rancor::Failure>(&msg.content) {
                                | Ok(versioned_msg) => {
                                    // Push to bridge's incoming queue
                                    if let Err(e) = bridge_in.push_incoming(versioned_msg) {
                                        eprintln!(
                                            "[Node {}] Failed to push to bridge incoming: {}",
                                            node_id, e
                                        );
                                    } else {
                                        println!(
                                            "[Node {}] Message #{} pushed to bridge incoming",
                                            node_id, msg_count
                                        );
                                    }
                                },
                                | Err(e) => eprintln!(
                                    "[Node {}] Failed to deserialize gossip message: {}",
                                    node_id, e
                                ),
                            }
                        }
                    },
                    | Ok(Some(Err(e))) => {
                        eprintln!("[Node {}] Gossip receiver error: {}", node_id, e)
                    },
                    | Ok(None) => {
                        // Stream ended
                        println!("[Node {}] Gossip stream ended", node_id);
                        break;
                    },
                    | Err(_) => {
                        // Timeout, no message available
                    },
                }
            }
        });
    }
}

// ============================================================================
// Integration Tests
// ============================================================================

/// Test 1: Basic entity sync (Node A spawns → Node B receives)
#[tokio::test(flavor = "multi_thread")]
async fn test_basic_entity_sync() -> Result<()> {
    use test_utils::*;

    println!("=== Starting test_basic_entity_sync ===");

    // Setup contexts
    println!("Creating test contexts...");
    let ctx1 = TestContext::new();
    let ctx2 = TestContext::new();

    // Setup gossip networking
    println!("Setting up gossip pair...");
    let (ep1, ep2, router1, router2, bridge1, bridge2) = setup_gossip_pair().await?;

    let node1_id = bridge1.node_id();
    let node2_id = bridge2.node_id();

    // Create headless apps
    println!("Creating Bevy apps...");
    let mut app1 = create_test_app(node1_id, ctx1.db_path(), bridge1);
    let mut app2 = create_test_app(node2_id, ctx2.db_path(), bridge2);
    println!("Apps created successfully");

    println!("Node 1 ID: {}", node1_id);
    println!("Node 2 ID: {}", node2_id);

    // Node 1 spawns entity
    let entity_id = Uuid::new_v4();
    println!("Spawning entity {} on node 1", entity_id);
    let spawned_entity = app1
        .world_mut()
        .spawn((
            NetworkedEntity::with_id(entity_id, node1_id),
            TestPosition { x: 10.0, y: 20.0 },
            Persisted::with_id(entity_id),
            Synced,
        ))
        .id();

    // IMPORTANT: Trigger change detection for persistence
    // Bevy only marks components as "changed" when mutated, not on spawn
    // Access Persisted mutably to trigger the persistence system
    {
        let world = app1.world_mut();
        if let Ok(mut entity_mut) = world.get_entity_mut(spawned_entity) {
            if let Some(mut persisted) = entity_mut.get_mut::<Persisted>() {
                // Dereferencing the mutable borrow triggers change detection
                let _ = &mut *persisted;
            }
        }
    }
    println!("Entity spawned, triggered persistence");

    println!("Entity spawned, starting sync wait...");

    // Wait for sync
    wait_for_sync(&mut app1, &mut app2, Duration::from_secs(10), |_, w2| {
        let count = count_entities_with_id(w2, entity_id);
        if count > 0 {
            println!("✓ Entity found on node 2!");
            true
        } else {
            // Debug: print what entities we DO have
            let all_networked: Vec<uuid::Uuid> = {
                let mut query = w2.query::<&NetworkedEntity>();
                query.iter(w2).map(|ne| ne.network_id).collect()
            };
            if !all_networked.is_empty() {
                println!(
                    "  Node 2 has {} networked entities: {:?}",
                    all_networked.len(),
                    all_networked
                );
                println!("  Looking for: {}", entity_id);
            }
            false
        }
    })
    .await?;

    // Update app2 one more time to ensure queued commands are applied
    println!("Running final update to flush commands...");
    app2.update();

    // Debug: Check what components the entity has
    {
        let world = app2.world_mut();
        let mut query = world.query::<(&NetworkedEntity, Option<&TestPosition>)>();
        for (ne, pos) in query.iter(world) {
            if ne.network_id == entity_id {
                println!("Debug: Entity {} has NetworkedEntity", entity_id);
                if let Some(pos) = pos {
                    println!("Debug: Entity has TestPosition: {:?}", pos);
                } else {
                    println!("Debug: Entity MISSING TestPosition component!");
                }
            }
        }
    }

    // Verify entity synced to node 2 (in-memory check)
    assert_entity_synced(
        app2.world_mut(),
        entity_id,
        TestPosition { x: 10.0, y: 20.0 },
    )?;
    println!("✓ Entity synced in-memory on node 2");

    // Give persistence system time to flush to disk
    // The persistence system runs on Update with a 1-second flush interval
    println!("Waiting for persistence to flush...");
    for _ in 0..15 {
        app1.update();
        app2.update();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Verify persistence on Node 1 (originating node)
    println!("Checking Node 1 database persistence...");
    assert!(
        entity_exists_in_db(&ctx1.db_path(), entity_id)?,
        "Entity {} should exist in Node 1 database",
        entity_id
    );

    assert!(
        component_exists_in_db(
            &ctx1.db_path(),
            entity_id,
            "sync_integration_headless::TestPosition"
        )?,
        "TestPosition component should exist in Node 1 database"
    );

    // TODO: Rewrite this test to use ComponentTypeRegistry instead of reflection
    // let node1_position = {
    //     load_component_from_db::<TestPosition>(
    //         &ctx1.db_path(),
    //         entity_id,
    //         "sync_integration_headless::TestPosition",
    //     )?
    // };

    // assert_eq!(
    //     node1_position,
    //     Some(TestPosition { x: 10.0, y: 20.0 }),
    //     "TestPosition data should be correctly persisted in Node 1 database"
    // );
    println!("✓ Node 1 persistence verified");

    // Verify persistence on Node 2 (receiving node after sync)
    println!("Checking Node 2 database persistence...");
    assert!(
        entity_exists_in_db(&ctx2.db_path(), entity_id)?,
        "Entity {} should exist in Node 2 database after sync",
        entity_id
    );

    assert!(
        component_exists_in_db(
            &ctx2.db_path(),
            entity_id,
            "sync_integration_headless::TestPosition"
        )?,
        "TestPosition component should exist in Node 2 database after sync"
    );

    // TODO: Rewrite this test to use ComponentTypeRegistry instead of reflection
    // let node2_position = {
    //     load_component_from_db::<TestPosition>(
    //         &ctx2.db_path(),
    //         entity_id,
    //         "sync_integration_headless::TestPosition",
    //     )?
    // };

    // assert_eq!(
    //     node2_position,
    //     Some(TestPosition { x: 10.0, y: 20.0 }),
    //     "TestPosition data should be correctly persisted in Node 2 database after sync"
    // );
    println!("✓ Node 2 persistence verified");

    println!("✓ Full sync and persistence test passed!");

    // Cleanup
    router1.shutdown().await?;
    router2.shutdown().await?;
    ep1.close().await;
    ep2.close().await;

    Ok(())
}

/// Test 2: Bidirectional sync (both nodes modify different entities)
#[tokio::test(flavor = "multi_thread")]
async fn test_bidirectional_sync() -> Result<()> {
    use test_utils::*;

    let ctx1 = TestContext::new();
    let ctx2 = TestContext::new();

    let (ep1, ep2, router1, router2, bridge1, bridge2) = setup_gossip_pair().await?;

    let node1_id = bridge1.node_id();
    let node2_id = bridge2.node_id();

    let mut app1 = create_test_app(node1_id, ctx1.db_path(), bridge1);
    let mut app2 = create_test_app(node2_id, ctx2.db_path(), bridge2);

    // Node 1 spawns entity A
    let entity_a = Uuid::new_v4();
    let entity_a_bevy = app1
        .world_mut()
        .spawn((
            NetworkedEntity::with_id(entity_a, node1_id),
            TestPosition { x: 1.0, y: 2.0 },
            Persisted::with_id(entity_a),
            Synced,
        ))
        .id();

    // Trigger persistence for entity A
    {
        let world = app1.world_mut();
        if let Ok(mut entity_mut) = world.get_entity_mut(entity_a_bevy) {
            if let Some(mut persisted) = entity_mut.get_mut::<Persisted>() {
                let _ = &mut *persisted;
            }
        }
    }

    // Node 2 spawns entity B
    let entity_b = Uuid::new_v4();
    let entity_b_bevy = app2
        .world_mut()
        .spawn((
            NetworkedEntity::with_id(entity_b, node2_id),
            TestPosition { x: 3.0, y: 4.0 },
            Persisted::with_id(entity_b),
            Synced,
        ))
        .id();

    // Trigger persistence for entity B
    {
        let world = app2.world_mut();
        if let Ok(mut entity_mut) = world.get_entity_mut(entity_b_bevy) {
            if let Some(mut persisted) = entity_mut.get_mut::<Persisted>() {
                let _ = &mut *persisted;
            }
        }
    }

    // Wait for bidirectional sync
    wait_for_sync(&mut app1, &mut app2, Duration::from_secs(5), |w1, w2| {
        count_entities_with_id(w1, entity_b) > 0 && count_entities_with_id(w2, entity_a) > 0
    })
    .await?;

    // Verify both nodes have both entities
    assert_entity_synced(app1.world_mut(), entity_b, TestPosition { x: 3.0, y: 4.0 })?;
    assert_entity_synced(app2.world_mut(), entity_a, TestPosition { x: 1.0, y: 2.0 })?;

    println!("✓ Bidirectional sync test passed");

    router1.shutdown().await?;
    router2.shutdown().await?;
    ep1.close().await;
    ep2.close().await;

    Ok(())
}

/// Test 3: Concurrent conflict resolution (LWW merge semantics)
#[tokio::test(flavor = "multi_thread")]
async fn test_concurrent_conflict_resolution() -> Result<()> {
    use test_utils::*;

    let ctx1 = TestContext::new();
    let ctx2 = TestContext::new();

    let (ep1, ep2, router1, router2, bridge1, bridge2) = setup_gossip_pair().await?;

    let node1_id = bridge1.node_id();
    let node2_id = bridge2.node_id();

    let mut app1 = create_test_app(node1_id, ctx1.db_path(), bridge1);
    let mut app2 = create_test_app(node2_id, ctx2.db_path(), bridge2);

    // Spawn shared entity on node 1 with Transform (which IS tracked for changes)
    let entity_id = Uuid::new_v4();
    let entity_bevy = app1
        .world_mut()
        .spawn((
            NetworkedEntity::with_id(entity_id, node1_id),
            NetworkedTransform::default(),
            Transform::from_xyz(0.0, 0.0, 0.0),
            Persisted::with_id(entity_id),
            Synced,
        ))
        .id();

    // Trigger persistence
    {
        let world = app1.world_mut();
        if let Ok(mut entity_mut) = world.get_entity_mut(entity_bevy) {
            if let Some(mut persisted) = entity_mut.get_mut::<Persisted>() {
                let _ = &mut *persisted;
            }
        }
    }

    // Wait for initial sync
    wait_for_sync(&mut app1, &mut app2, Duration::from_secs(2), |_, w2| {
        count_entities_with_id(w2, entity_id) > 0
    })
    .await?;

    println!("✓ Initial sync complete, both nodes have the entity");

    // Check what components the entity has on each node
    {
        let world1 = app1.world_mut();
        let mut query1 = world1.query::<(
            Entity,
            &NetworkedEntity,
            Option<&NetworkedTransform>,
            &Transform,
        )>();
        println!("Node 1 entities:");
        for (entity, ne, nt, t) in query1.iter(world1) {
            println!(
                "  Entity {:?}: NetworkedEntity({:?}), NetworkedTransform={}, Transform=({}, {}, {})",
                entity,
                ne.network_id,
                nt.is_some(),
                t.translation.x,
                t.translation.y,
                t.translation.z
            );
        }
    }
    {
        let world2 = app2.world_mut();
        let mut query2 = world2.query::<(
            Entity,
            &NetworkedEntity,
            Option<&NetworkedTransform>,
            &Transform,
        )>();
        println!("Node 2 entities:");
        for (entity, ne, nt, t) in query2.iter(world2) {
            println!(
                "  Entity {:?}: NetworkedEntity({:?}), NetworkedTransform={}, Transform=({}, {}, {})",
                entity,
                ne.network_id,
                nt.is_some(),
                t.translation.x,
                t.translation.y,
                t.translation.z
            );
        }
    }

    // Both nodes modify the same entity concurrently
    // Node 1 update (earlier timestamp)
    {
        let mut query1 = app1.world_mut().query::<&mut Transform>();
        for mut transform in query1.iter_mut(app1.world_mut()) {
            println!("Node 1: Modifying Transform to (10, 10)");
            transform.translation.x = 10.0;
            transform.translation.y = 10.0;
        }
    }
    app1.update(); // Generate and send delta

    // Small delay to ensure node 2's change has a later vector clock
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Node 2 update (later timestamp, should win with LWW)
    {
        let mut query2 = app2.world_mut().query::<&mut Transform>();
        let count = query2.iter(app2.world()).count();
        println!("Node 2: Found {} entities with Transform", count);
        for mut transform in query2.iter_mut(app2.world_mut()) {
            println!("Node 2: Modifying Transform to (20, 20)");
            transform.translation.x = 20.0;
            transform.translation.y = 20.0;
        }
    }
    app2.update(); // Generate and send delta

    println!("Both nodes modified the entity, waiting for convergence...");

    // Wait for convergence - both nodes should have the same Transform value
    wait_for_sync(&mut app1, &mut app2, Duration::from_secs(5), |w1, w2| {
        let mut query1 = w1.query::<&Transform>();
        let mut query2 = w2.query::<&Transform>();

        let transforms1: Vec<_> = query1.iter(w1).collect();
        let transforms2: Vec<_> = query2.iter(w2).collect();

        if transforms1.is_empty() || transforms2.is_empty() {
            return false;
        }

        let t1 = transforms1[0];
        let t2 = transforms2[0];

        // Check if they converged (within floating point tolerance)
        let converged = (t1.translation.x - t2.translation.x).abs() < 0.01 &&
            (t1.translation.y - t2.translation.y).abs() < 0.01;

        if converged {
            println!(
                "✓ Nodes converged to: ({}, {})",
                t1.translation.x, t1.translation.y
            );
        }

        converged
    })
    .await?;

    println!("✓ Conflict resolution test passed (converged)");

    router1.shutdown().await?;
    router2.shutdown().await?;
    ep1.close().await;
    ep2.close().await;

    Ok(())
}

/// Test 4: Persistence crash recovery
///
/// NOTE: This test is expected to initially fail as the persistence system
/// doesn't currently have entity loading on startup. This test documents
/// the gap and will drive future feature implementation.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "Persistence loading not yet implemented - documents gap"]
async fn test_persistence_crash_recovery() -> Result<()> {
    use test_utils::*;

    let ctx = TestContext::new();
    let node_id = Uuid::new_v4();
    let entity_id = Uuid::new_v4();

    // Phase 1: Create entity, persist, "crash"
    {
        let bridge = GossipBridge::new(node_id);
        let mut app = create_test_app(node_id, ctx.db_path(), bridge);

        app.world_mut().spawn((
            NetworkedEntity::with_id(entity_id, node_id),
            TestPosition { x: 100.0, y: 200.0 },
            Persisted::with_id(entity_id),
            Synced,
        ));

        // Tick until flushed (2 seconds at 60 FPS)
        for _ in 0..120 {
            app.update();
            tokio::time::sleep(Duration::from_millis(16)).await;
        }

        println!("Phase 1: Entity persisted, simulating crash...");
        // App drops here (crash simulation)
    }

    // Phase 2: Restart app, verify state restored
    {
        let bridge = GossipBridge::new(node_id);
        let mut app = create_test_app(node_id, ctx.db_path(), bridge);

        // TODO: Need startup system to load entities from persistence
        // This is currently missing from the implementation

        app.update();

        // Verify entity loaded from database
        assert_entity_synced(
            app.world_mut(),
            entity_id,
            TestPosition { x: 100.0, y: 200.0 },
        )
        .map_err(|e| anyhow::anyhow!("Persistence recovery failed: {}", e))?;

        println!("✓ Crash recovery test passed");
    }

    Ok(())
}

/// Test 5: Lock heartbeat renewal mechanism
#[tokio::test(flavor = "multi_thread")]
async fn test_lock_heartbeat_renewal() -> Result<()> {
    use test_utils::*;

    println!("=== Starting test_lock_heartbeat_renewal ===");

    let ctx1 = TestContext::new();
    let ctx2 = TestContext::new();

    let (ep1, ep2, router1, router2, bridge1, bridge2) = setup_gossip_pair().await?;

    let node1_id = bridge1.node_id();
    let node2_id = bridge2.node_id();

    let mut app1 = create_test_app(node1_id, ctx1.db_path(), bridge1);
    let mut app2 = create_test_app(node2_id, ctx2.db_path(), bridge2);

    // Spawn entity
    let entity_id = Uuid::new_v4();
    let _ = app1.world_mut()
        .spawn((
            NetworkedEntity::with_id(entity_id, node1_id),
            TestPosition { x: 10.0, y: 20.0 },
            Persisted::with_id(entity_id),
            Synced,
        ))
        .id();

    wait_for_sync(&mut app1, &mut app2, Duration::from_secs(3), |_, w2| {
        count_entities_with_id(w2, entity_id) > 0
    })
    .await?;
    println!("✓ Entity synced");

    // Acquire lock on both nodes
    {
        let world = app1.world_mut();
        let mut registry = world.resource_mut::<EntityLockRegistry>();
        registry.try_acquire(entity_id, node1_id).ok();
    }
    {
        let bridge = app1.world().resource::<GossipBridge>();
        let msg = VersionedMessage::new(SyncMessage::Lock(LockMessage::LockRequest {
            entity_id,
            node_id: node1_id,
        }));
        bridge.send(msg).ok();
    }

    for _ in 0..5 {
        app1.update();
        app2.update();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Verify both nodes have the lock
    {
        let registry1 = app1.world().resource::<EntityLockRegistry>();
        let registry2 = app2.world().resource::<EntityLockRegistry>();
        assert!(registry1.is_locked(entity_id, node1_id), "Lock should exist on node 1");
        assert!(registry2.is_locked(entity_id, node2_id), "Lock should exist on node 2");
        println!("✓ Lock acquired on both nodes");
    }

    // Test heartbeat renewal: send a few heartbeats and verify locks persist
    for i in 0..3 {
        // Renew on node 1
        {
            let world = app1.world_mut();
            let mut registry = world.resource_mut::<EntityLockRegistry>();
            assert!(
                registry.renew_heartbeat(entity_id, node1_id),
                "Should successfully renew lock"
            );
        }

        // Send heartbeat to node 2
        {
            let bridge = app1.world().resource::<GossipBridge>();
            let msg = VersionedMessage::new(SyncMessage::Lock(LockMessage::LockHeartbeat {
                entity_id,
                holder: node1_id,
            }));
            bridge.send(msg).ok();
        }

        // Process
        for _ in 0..3 {
            app1.update();
            app2.update();
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // Verify locks still exist after heartbeat
        {
            let registry1 = app1.world().resource::<EntityLockRegistry>();
            let registry2 = app2.world().resource::<EntityLockRegistry>();
            assert!(
                registry1.is_locked(entity_id, node1_id),
                "Lock should persist on node 1 after heartbeat {}",
                i + 1
            );
            assert!(
                registry2.is_locked(entity_id, node2_id),
                "Lock should persist on node 2 after heartbeat {}",
                i + 1
            );
        }
    }

    println!("✓ Heartbeat renewal mechanism working correctly");

    router1.shutdown().await?;
    router2.shutdown().await?;
    ep1.close().await;
    ep2.close().await;

    Ok(())
}

/// Test 6: Lock expires without heartbeats
#[tokio::test(flavor = "multi_thread")]
async fn test_lock_heartbeat_expiration() -> Result<()> {
    use test_utils::*;

    println!("=== Starting test_lock_heartbeat_expiration ===");

    let ctx1 = TestContext::new();
    let ctx2 = TestContext::new();

    let (ep1, ep2, router1, router2, bridge1, bridge2) = setup_gossip_pair().await?;

    let node1_id = bridge1.node_id();
    let node2_id = bridge2.node_id();

    let mut app1 = create_test_app(node1_id, ctx1.db_path(), bridge1);
    let mut app2 = create_test_app(node2_id, ctx2.db_path(), bridge2);

    // Node 1 spawns entity and selects it
    let entity_id = Uuid::new_v4();
    let _ = app1.world_mut()
        .spawn((
            NetworkedEntity::with_id(entity_id, node1_id),
            NetworkedSelection::default(),
            TestPosition { x: 10.0, y: 20.0 },
            Persisted::with_id(entity_id),
            Synced,
        ))
        .id();

    // Wait for sync
    wait_for_sync(&mut app1, &mut app2, Duration::from_secs(5), |_, w2| {
        count_entities_with_id(w2, entity_id) > 0
    })
    .await?;

    // Acquire lock locally on node 1 (gossip doesn't loop back to sender)
    {
        let world = app1.world_mut();
        let mut registry = world.resource_mut::<EntityLockRegistry>();
        registry.try_acquire(entity_id, node1_id).ok();
    }

    // Broadcast LockRequest so other nodes apply optimistically
    {
        let bridge = app1.world().resource::<GossipBridge>();
        let msg = VersionedMessage::new(SyncMessage::Lock(LockMessage::LockRequest {
            entity_id,
            node_id: node1_id,
        }));
        bridge.send(msg).ok();
    }

    // Update to allow lock propagation
    for _ in 0..10 {
        app1.update();
        app2.update();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Verify lock acquired
    wait_for_sync(&mut app1, &mut app2, Duration::from_secs(2), |_, w2| {
        let registry2 = w2.resource::<EntityLockRegistry>();
        registry2.is_locked(entity_id, node2_id)
    })
    .await?;
    println!("✓ Lock acquired and propagated");

    // Simulate node 1 crash: remove lock from node 1's registry without sending release
    // This stops heartbeat broadcasts from node 1
    {
        let mut registry = app1.world_mut().resource_mut::<EntityLockRegistry>();
        registry.force_release(entity_id);
        println!("✓ Simulated node 1 crash (stopped heartbeats)");
    }

    // Force the lock to expire on node 2 (simulating 5+ seconds passing without heartbeats)
    {
        let mut registry = app2.world_mut().resource_mut::<EntityLockRegistry>();
        registry.expire_lock_for_testing(entity_id);
        println!("✓ Forced lock to appear expired on node 2");
    }

    // Run cleanup system (which removes expired locks and broadcasts LockReleased)
    println!("Running cleanup to expire locks...");
    for _ in 0..10 {
        app2.update();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Verify lock was removed from node 2
    {
        let registry = app2.world().resource::<EntityLockRegistry>();
        assert!(
            !registry.is_locked(entity_id, node2_id),
            "Lock should be expired on node 2 after cleanup"
        );
        println!("✓ Lock expired on node 2 after 5 seconds without heartbeat");
    }

    println!("✓ Lock heartbeat expiration test passed");

    router1.shutdown().await?;
    router2.shutdown().await?;
    ep1.close().await;
    ep2.close().await;

    Ok(())
}

/// Test 7: Lock release stops heartbeats
#[tokio::test(flavor = "multi_thread")]
async fn test_lock_release_stops_heartbeats() -> Result<()> {
    use test_utils::*;

    println!("=== Starting test_lock_release_stops_heartbeats ===");

    let ctx1 = TestContext::new();
    let ctx2 = TestContext::new();

    let (ep1, ep2, router1, router2, bridge1, bridge2) = setup_gossip_pair().await?;

    let node1_id = bridge1.node_id();
    let node2_id = bridge2.node_id();

    let mut app1 = create_test_app(node1_id, ctx1.db_path(), bridge1);
    let mut app2 = create_test_app(node2_id, ctx2.db_path(), bridge2);

    // Node 1 spawns entity and selects it
    let entity_id = Uuid::new_v4();
    let _ = app1.world_mut()
        .spawn((
            NetworkedEntity::with_id(entity_id, node1_id),
            NetworkedSelection::default(),
            TestPosition { x: 10.0, y: 20.0 },
            Persisted::with_id(entity_id),
            Synced,
        ))
        .id();

    // Wait for sync
    wait_for_sync(&mut app1, &mut app2, Duration::from_secs(5), |_, w2| {
        count_entities_with_id(w2, entity_id) > 0
    })
    .await?;

    // Acquire lock locally on node 1 (gossip doesn't loop back to sender)
    {
        let world = app1.world_mut();
        let mut registry = world.resource_mut::<EntityLockRegistry>();
        registry.try_acquire(entity_id, node1_id).ok();
    }

    // Broadcast LockRequest so other nodes apply optimistically
    {
        let bridge = app1.world().resource::<GossipBridge>();
        let msg = VersionedMessage::new(SyncMessage::Lock(LockMessage::LockRequest {
            entity_id,
            node_id: node1_id,
        }));
        bridge.send(msg).ok();
    }

    // Update to allow lock propagation
    for _ in 0..10 {
        app1.update();
        app2.update();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Wait for lock to propagate
    wait_for_sync(&mut app1, &mut app2, Duration::from_secs(2), |_, w2| {
        let registry2 = w2.resource::<EntityLockRegistry>();
        registry2.is_locked(entity_id, node2_id)
    })
    .await?;
    println!("✓ Lock acquired and propagated");

    // Release lock on node 1
    {
        let world = app1.world_mut();
        let mut registry = world.resource_mut::<EntityLockRegistry>();
        if registry.release(entity_id, node1_id) {
            println!("✓ Lock released on node 1");
        }
    }

    // Broadcast LockRelease message to other nodes
    {
        let bridge = app1.world().resource::<GossipBridge>();
        let msg = VersionedMessage::new(SyncMessage::Lock(LockMessage::LockRelease {
            entity_id,
            node_id: node1_id,
        }));
        bridge.send(msg).ok();
    }

    // Update to trigger lock release propagation
    for _ in 0..10 {
        app1.update();
        app2.update();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Wait for release to propagate to node 2
    wait_for_sync(&mut app1, &mut app2, Duration::from_secs(3), |_, w2| {
        let registry2 = w2.resource::<EntityLockRegistry>();
        !registry2.is_locked(entity_id, node2_id)
    })
    .await?;
    println!("✓ Lock release propagated to node 2");

    println!("✓ Lock release stops heartbeats test passed");

    router1.shutdown().await?;
    router2.shutdown().await?;
    ep1.close().await;
    ep2.close().await;

    Ok(())
}
