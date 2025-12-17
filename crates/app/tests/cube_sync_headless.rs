//! Headless integration tests for cube synchronization
//!
//! These tests validate end-to-end CRDT synchronization of the cube entity
//! using multiple headless Bevy apps with real iroh-gossip networking.

use std::{
    path::PathBuf,
    time::{
        Duration,
        Instant,
    },
};

use anyhow::Result;
use app::CubeMarker;
use bevy::{
    app::{
        App,
        ScheduleRunnerPlugin,
    },
    ecs::world::World,
    prelude::*,
    MinimalPlugins,
};
use bytes::Bytes;
use futures_lite::StreamExt;
use iroh::{
    protocol::Router,
    Endpoint,
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
        GossipBridge,
        NetworkedEntity,
        NetworkedTransform,
        NetworkingConfig,
        NetworkingPlugin,
        Synced,
        VersionedMessage,
    },
    persistence::{
        Persisted,
        PersistenceConfig,
        PersistencePlugin,
    },
};
use tempfile::TempDir;
use uuid::Uuid;

// ============================================================================
// Test Utilities
// ============================================================================

mod test_utils {
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

    /// Create a headless Bevy app configured for cube testing
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

        // Register cube component types for reflection
        app.register_type::<CubeMarker>();

        app
    }

    /// Count entities with CubeMarker component
    #[allow(dead_code)]
    pub fn count_cubes(world: &mut World) -> usize {
        let mut query = world.query::<&CubeMarker>();
        query.iter(world).count()
    }

    /// Count entities with a specific network ID
    pub fn count_entities_with_id(world: &mut World, network_id: Uuid) -> usize {
        let mut query = world.query::<&NetworkedEntity>();
        query
            .iter(world)
            .filter(|entity| entity.network_id == network_id)
            .count()
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
        println!("  Creating endpoint with mDNS discovery...");
        let endpoint = Endpoint::builder()
            .discovery(iroh::discovery::mdns::MdnsDiscovery::builder())
            .bind()
            .await?;
        let endpoint_id = endpoint.addr().id;
        println!("  Endpoint created: {}", endpoint_id);

        // Convert 32-byte endpoint ID to 16-byte UUID
        let id_bytes = endpoint_id.as_bytes();
        let mut uuid_bytes = [0u8; 16];
        uuid_bytes.copy_from_slice(&id_bytes[..16]);
        let node_id = Uuid::from_bytes(uuid_bytes);

        println!("  Spawning gossip protocol...");
        let gossip = Gossip::builder().spawn(endpoint.clone());

        println!("  Setting up router...");
        let router = Router::builder(endpoint.clone())
            .accept(iroh_gossip::ALPN, gossip.clone())
            .spawn();

        let bootstrap_count = bootstrap_addrs.len();
        let has_bootstrap_peers = !bootstrap_addrs.is_empty();

        let bootstrap_ids: Vec<_> = bootstrap_addrs.iter().map(|a| a.id).collect();

        if has_bootstrap_peers {
            let static_provider = iroh::discovery::static_provider::StaticProvider::default();
            for addr in &bootstrap_addrs {
                static_provider.add_endpoint_info(addr.clone());
            }
            endpoint.discovery().add(static_provider);
            println!(
                "  Added {} bootstrap peers to static discovery",
                bootstrap_count
            );

            println!("  Connecting to bootstrap peers...");
            for addr in &bootstrap_addrs {
                match endpoint.connect(addr.clone(), iroh_gossip::ALPN).await {
                    | Ok(_conn) => println!("  ✓ Connected to bootstrap peer: {}", addr.id),
                    | Err(e) => {
                        println!("  ✗ Failed to connect to bootstrap peer {}: {}", addr.id, e)
                    },
                }
            }
        }

        println!(
            "  Subscribing to topic with {} bootstrap peers...",
            bootstrap_count
        );
        let subscribe_handle = gossip.subscribe(topic_id, bootstrap_ids).await?;

        println!("  Splitting sender/receiver...");
        let (sender, mut receiver) = subscribe_handle.split();

        if has_bootstrap_peers {
            println!("  Waiting for join to complete (with timeout)...");
            match tokio::time::timeout(Duration::from_secs(3), receiver.joined()).await {
                | Ok(Ok(())) => println!("  Join completed!"),
                | Ok(Err(e)) => println!("  Join error: {}", e),
                | Err(_) => {
                    println!("  Join timeout - proceeding anyway (mDNS may still connect later)")
                },
            }
        } else {
            println!("  No bootstrap peers - skipping join wait (first node in swarm)");
        }

        let bridge = GossipBridge::new(node_id);
        println!("  Spawning bridge tasks...");

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
        let topic_id = TopicId::from_bytes([42; 32]);
        println!("Using topic ID: {:?}", topic_id);

        println!("Initializing node 1...");
        let (ep1, _gossip1, router1, bridge1) = init_gossip_node(topic_id, vec![]).await?;
        println!("Node 1 initialized with ID: {}", ep1.addr().id);

        let node1_addr = ep1.addr().clone();
        println!("Node 1 full address: {:?}", node1_addr);

        println!("Initializing node 2 with bootstrap peer: {}", node1_addr.id);
        let (ep2, _gossip2, router2, bridge2) =
            init_gossip_node(topic_id, vec![node1_addr]).await?;
        println!("Node 2 initialized with ID: {}", ep2.addr().id);

        println!("Waiting for mDNS/gossip peer discovery...");
        tokio::time::sleep(Duration::from_secs(2)).await;
        println!("Peer discovery wait complete");

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
                if let Some(versioned_msg) = bridge_out.try_recv_outgoing() {
                    msg_count += 1;
                    println!(
                        "[Node {}] Sending message #{} via gossip",
                        node_id, msg_count
                    );
                    match rkyv::to_bytes::<rkyv::rancor::Failure>(&versioned_msg).map(|b| b.to_vec()) {
                        | Ok(bytes) => {
                            if let Err(e) = sender.broadcast(Bytes::from(bytes)).await {
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

                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        // Task 2: Forward from gossip receiver → bridge.incoming
        let bridge_in = bridge.clone();
        tokio::spawn(async move {
            let mut msg_count = 0;
            println!("[Node {}] Gossip receiver task started", node_id);
            loop {
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
                            match rkyv::from_bytes::<VersionedMessage, rkyv::rancor::Failure>(&msg.content) {
                                | Ok(versioned_msg) => {
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

/// Test: Basic cube spawn and sync (Node A spawns → Node B receives)
#[tokio::test(flavor = "multi_thread")]
async fn test_cube_spawn_and_sync() -> Result<()> {
    use test_utils::*;

    println!("=== Starting test_cube_spawn_and_sync ===");

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

    // Node 1 spawns cube
    let entity_id = Uuid::new_v4();
    println!("Spawning cube {} on node 1", entity_id);
    let spawned_entity = app1
        .world_mut()
        .spawn((
            CubeMarker,
            Transform::from_xyz(1.0, 2.0, 3.0),
            GlobalTransform::default(),
            NetworkedEntity::with_id(entity_id, node1_id),
            NetworkedTransform,
            Persisted::with_id(entity_id),
            Synced,
        ))
        .id();

    // IMPORTANT: Trigger change detection for persistence
    // Bevy only marks components as "changed" when mutated, not on spawn
    {
        let world = app1.world_mut();
        if let Ok(mut entity_mut) = world.get_entity_mut(spawned_entity) {
            if let Some(mut persisted) = entity_mut.get_mut::<Persisted>() {
                // Dereferencing the mutable borrow triggers change detection
                let _ = &mut *persisted;
            }
        }
    }
    println!("Cube spawned, triggered persistence");

    println!("Cube spawned, starting sync wait...");

    // Wait for cube to sync to node 2
    wait_for_sync(&mut app1, &mut app2, Duration::from_secs(10), |_, w2| {
        count_entities_with_id(w2, entity_id) > 0
    })
    .await?;

    println!("Cube synced to node 2!");

    // Verify cube exists on node 2 with correct Transform
    let cube_transform = app2
        .world_mut()
        .query_filtered::<&Transform, With<CubeMarker>>()
        .single(app2.world())
        .expect("Cube should exist on node 2");

    assert!(
        (cube_transform.translation.x - 1.0).abs() < 0.01,
        "Transform.x should be 1.0"
    );
    assert!(
        (cube_transform.translation.y - 2.0).abs() < 0.01,
        "Transform.y should be 2.0"
    );
    assert!(
        (cube_transform.translation.z - 3.0).abs() < 0.01,
        "Transform.z should be 3.0"
    );

    println!("Transform verified!");

    // Cleanup
    router1.shutdown().await?;
    router2.shutdown().await?;
    drop(ep1);
    drop(ep2);

    println!("=== Test completed successfully ===");
    Ok(())
}
