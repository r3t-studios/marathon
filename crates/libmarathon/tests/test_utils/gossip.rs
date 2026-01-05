//! Shared iroh-gossip setup utilities for integration tests
//!
//! This module provides real iroh-gossip networking infrastructure that all
//! integration tests should use. No shortcuts - always use real localhost connections.

use anyhow::Result;
use futures_lite::StreamExt;
use iroh::{
    Endpoint,
    discovery::static_provider::StaticProvider,
    protocol::Router,
};
use iroh_gossip::{
    api::{GossipReceiver, GossipSender},
    net::Gossip,
    proto::TopicId,
};
use libmarathon::networking::{GossipBridge, VersionedMessage};
use std::time::Duration;
use uuid::Uuid;

/// Initialize a single iroh-gossip node
///
/// Creates a real iroh endpoint bound to localhost, spawns the gossip protocol,
/// sets up routing, and optionally connects to bootstrap peers.
///
/// # Arguments
/// * `topic_id` - The gossip topic to subscribe to
/// * `bootstrap_addrs` - Optional bootstrap peers to connect to
///
/// # Returns
/// * Endpoint - The iroh endpoint for this node
/// * Gossip - The gossip protocol handler
/// * Router - The router handling incoming connections
/// * GossipBridge - The bridge for Bevy ECS integration
pub async fn init_gossip_node(
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
        let static_provider = StaticProvider::default();
        for addr in &bootstrap_addrs {
            static_provider.add_endpoint_info(addr.clone());
        }
        endpoint.discovery().add(static_provider);
        println!("  Added {} bootstrap peers to discovery", bootstrap_count);

        // Connect to bootstrap peers (localhost connections are instant)
        for addr in &bootstrap_addrs {
            match endpoint.connect(addr.clone(), iroh_gossip::ALPN).await {
                Ok(_conn) => println!("  ✓ Connected to {}", addr.id),
                Err(e) => println!("  ✗ Connection failed: {}", e),
            }
        }
    }

    // Subscribe to the topic
    let subscribe_handle = gossip.subscribe(topic_id, bootstrap_ids).await?;
    let (sender, mut receiver) = subscribe_handle.split();

    // Wait for join if we have bootstrap peers (should be instant on localhost)
    if has_bootstrap_peers {
        match tokio::time::timeout(Duration::from_millis(500), receiver.joined()).await {
            Ok(Ok(())) => println!("  ✓ Join completed"),
            Ok(Err(e)) => println!("  ✗ Join error: {}", e),
            Err(_) => println!("  ⚠ Join timeout (proceeding anyway)"),
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

/// Spawn background tasks to forward messages between iroh-gossip and GossipBridge
///
/// This creates two tokio tasks:
/// 1. Forward from bridge.outgoing → gossip sender (broadcasts to peers)
/// 2. Forward from gossip receiver → bridge.incoming (receives from peers)
///
/// These tasks run indefinitely and handle serialization/deserialization.
pub fn spawn_gossip_bridge_tasks(
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
                    Ok(bytes) => {
                        // Broadcast via gossip
                        if let Err(e) = sender.broadcast(bytes.into()).await {
                            eprintln!("[Node {}] Failed to broadcast message: {}", node_id, e);
                        } else {
                            println!(
                                "[Node {}] Message #{} broadcasted successfully",
                                node_id, msg_count
                            );
                        }
                    }
                    Err(e) => eprintln!(
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
                Ok(Some(Ok(event))) => {
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
                            Ok(versioned_msg) => {
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
                            }
                            Err(e) => eprintln!(
                                "[Node {}] Failed to deserialize gossip message: {}",
                                node_id, e
                            ),
                        }
                    }
                }
                Ok(Some(Err(e))) => {
                    eprintln!("[Node {}] Gossip receiver error: {}", node_id, e)
                }
                Ok(None) => {
                    // Stream ended
                    println!("[Node {}] Gossip stream ended", node_id);
                    break;
                }
                Err(_) => {
                    // Timeout, no message available
                }
            }
        }
    });
}

/// Setup a pair of iroh-gossip nodes connected to the same topic
///
/// This creates two nodes:
/// - Node 1: Initialized first with no bootstrap peers
/// - Node 2: Bootstraps from Node 1's address
///
/// Both nodes are subscribed to the same topic and connected via localhost.
///
/// # Returns
/// Tuple of (endpoint1, endpoint2, router1, router2, bridge1, bridge2)
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

    // Get node 1's full address (ID + network addresses) for node 2 to bootstrap from
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

/// Setup three iroh-gossip nodes connected to the same topic
///
/// This creates three nodes:
/// - Node 1: Initialized first with no bootstrap peers
/// - Node 2: Bootstraps from Node 1
/// - Node 3: Bootstraps from both Node 1 and Node 2
///
/// All nodes are subscribed to the same topic and connected via localhost.
///
/// # Returns
/// Tuple of (ep1, ep2, ep3, router1, router2, router3, bridge1, bridge2, bridge3)
pub async fn setup_gossip_trio() -> Result<(
    Endpoint,
    Endpoint,
    Endpoint,
    Router,
    Router,
    Router,
    GossipBridge,
    GossipBridge,
    GossipBridge,
)> {
    let topic_id = TopicId::from_bytes([42; 32]);
    println!("Using topic ID: {:?}", topic_id);

    // Initialize node 1
    println!("Initializing node 1...");
    let (ep1, _gossip1, router1, bridge1) = init_gossip_node(topic_id, vec![]).await?;
    println!("Node 1 initialized with ID: {}", ep1.addr().id);

    let node1_addr = ep1.addr().clone();

    // Initialize node 2 with node 1 as bootstrap
    println!("Initializing node 2 with bootstrap peer: {}", node1_addr.id);
    let (ep2, _gossip2, router2, bridge2) =
        init_gossip_node(topic_id, vec![node1_addr.clone()]).await?;
    println!("Node 2 initialized with ID: {}", ep2.addr().id);

    // Initialize node 3 with both node 1 and node 2 as bootstrap
    let node2_addr = ep2.addr().clone();
    println!("Initializing node 3 with bootstrap peers: {} and {}", node1_addr.id, node2_addr.id);
    let (ep3, _gossip3, router3, bridge3) =
        init_gossip_node(topic_id, vec![node1_addr, node2_addr]).await?;
    println!("Node 3 initialized with ID: {}", ep3.addr().id);

    // Brief wait for gossip protocol to stabilize
    tokio::time::sleep(Duration::from_millis(300)).await;

    Ok((ep1, ep2, ep3, router1, router2, router3, bridge1, bridge2, bridge3))
}
