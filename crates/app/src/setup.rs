//! Gossip networking setup with dedicated tokio runtime
//!
//! This module manages iroh-gossip networking with a tokio runtime running as a sidecar to Bevy.
//! The tokio runtime runs in a dedicated background thread, separate from Bevy's ECS loop.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │  Bevy Main Thread                   │
//! │  ┌────────────────────────────────┐ │
//! │  │ setup_gossip_networking()      │ │  Startup System
//! │  │ - Creates channel               │ │
//! │  │ - Spawns background thread      │ │
//! │  └────────────────────────────────┘ │
//! │  ┌────────────────────────────────┐ │
//! │  │ poll_gossip_bridge()           │ │  Update System
//! │  │ - Receives GossipBridge        │ │  (runs every frame)
//! │  │ - Inserts as resource          │ │
//! │  └────────────────────────────────┘ │
//! └─────────────────────────────────────┘
//!           ↕ (crossbeam channel)
//! ┌─────────────────────────────────────┐
//! │  Background Thread (macOS only)     │
//! │  ┌────────────────────────────────┐ │
//! │  │ Tokio Runtime                   │ │
//! │  │ ┌────────────────────────────┐ │ │
//! │  │ │ init_gossip()              │ │ │
//! │  │ │ - Creates iroh endpoint    │ │ │
//! │  │ │ - Sets up mDNS discovery   │ │ │
//! │  │ │ - Subscribes to topic      │ │ │
//! │  │ │ - Creates GossipBridge     │ │ │
//! │  │ └────────────────────────────┘ │ │
//! │  │ ┌────────────────────────────┐ │ │
//! │  │ │ spawn_bridge_tasks()       │ │ │
//! │  │ │ - Task 1: Forward outgoing │ │ │
//! │  │ │ - Task 2: Forward incoming │ │ │
//! │  │ └────────────────────────────┘ │ │
//! │  └────────────────────────────────┘ │
//! └─────────────────────────────────────┘
//! ```
//!
//! # Communication Pattern
//!
//! 1. **Bevy → Tokio**: GossipBridge's internal queue (try_recv_outgoing)
//! 2. **Tokio → Bevy**: GossipBridge's internal queue (push_incoming)
//! 3. **Thread handoff**: crossbeam_channel (one-time GossipBridge transfer)

use anyhow::Result;
use bevy::prelude::*;
use lib::networking::GossipBridge;
use uuid::Uuid;

/// Channel for receiving the GossipBridge from the background thread
///
/// This resource exists temporarily during startup. Once the GossipBridge
/// is received and inserted, this channel resource is removed.
#[cfg(not(target_os = "ios"))]
#[derive(Resource)]
pub struct GossipBridgeChannel(crossbeam_channel::Receiver<GossipBridge>);

/// Set up gossip networking with iroh
///
/// This is a Bevy startup system that initializes the gossip networking stack.
/// On macOS, it spawns a dedicated thread with a tokio runtime. On iOS, it logs
/// a warning (iOS networking not yet implemented).
///
/// # Platform Support
///
/// - **macOS**: Full support with mDNS discovery
/// - **iOS**: Not yet implemented
pub fn setup_gossip_networking(mut commands: Commands) {
    info!("Setting up gossip networking...");

    // Spawn dedicated thread with Tokio runtime for gossip initialization
    #[cfg(not(target_os = "ios"))]
    {
        let (sender, receiver) = crossbeam_channel::unbounded();
        commands.insert_resource(GossipBridgeChannel(receiver));

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                match init_gossip().await {
                    Ok(bridge) => {
                        info!("Gossip bridge initialized successfully");
                        if let Err(e) = sender.send(bridge) {
                            error!("Failed to send bridge to main thread: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Failed to initialize gossip: {}", e);
                    }
                }
            });
        });
    }

    #[cfg(target_os = "ios")]
    {
        warn!("iOS networking not yet implemented - gossip networking disabled");
    }
}

/// Poll the channel for the GossipBridge and insert it when ready
///
/// This is a Bevy update system that runs every frame. It checks the channel
/// for the GossipBridge created in the background thread. Once received, it
/// inserts the bridge as a resource and removes the channel.
///
/// # Platform Support
///
/// - **macOS**: Polls the channel and inserts GossipBridge
/// - **iOS**: No-op (networking not implemented)
pub fn poll_gossip_bridge(
    mut commands: Commands,
    #[cfg(not(target_os = "ios"))]
    channel: Option<Res<GossipBridgeChannel>>,
) {
    #[cfg(not(target_os = "ios"))]
    if let Some(channel) = channel {
        if let Ok(bridge) = channel.0.try_recv() {
            info!("Inserting GossipBridge resource into Bevy world");
            commands.insert_resource(bridge);
            commands.remove_resource::<GossipBridgeChannel>();
        }
    }
}

/// Initialize iroh-gossip networking stack
///
/// This async function runs in the background tokio runtime and:
/// 1. Creates an iroh endpoint with mDNS discovery
/// 2. Spawns the gossip protocol
/// 3. Sets up the router to accept gossip connections
/// 4. Subscribes to a shared topic (ID: [42; 32])
/// 5. Waits for join with a 2-second timeout
/// 6. Creates and configures the GossipBridge
/// 7. Spawns forwarding tasks to bridge messages
///
/// # Returns
///
/// - `Ok(GossipBridge)`: Successfully initialized networking
/// - `Err(anyhow::Error)`: Initialization failed
///
/// # Platform Support
///
/// This function is only compiled on non-iOS platforms.
#[cfg(not(target_os = "ios"))]
async fn init_gossip() -> Result<GossipBridge> {
    use iroh::discovery::mdns::MdnsDiscovery;
    use iroh::protocol::Router;
    use iroh::Endpoint;
    use iroh_gossip::net::Gossip;
    use iroh_gossip::proto::TopicId;

    info!("Creating endpoint with mDNS discovery...");
    let endpoint = Endpoint::builder()
        .discovery(MdnsDiscovery::builder())
        .bind()
        .await?;

    let endpoint_id = endpoint.addr().id;
    info!("Endpoint created: {}", endpoint_id);

    // Convert endpoint ID to UUID
    let id_bytes = endpoint_id.as_bytes();
    let mut uuid_bytes = [0u8; 16];
    uuid_bytes.copy_from_slice(&id_bytes[..16]);
    let node_id = Uuid::from_bytes(uuid_bytes);

    info!("Spawning gossip protocol...");
    let gossip = Gossip::builder().spawn(endpoint.clone());

    info!("Setting up router...");
    let router = Router::builder(endpoint.clone())
        .accept(iroh_gossip::ALPN, gossip.clone())
        .spawn();

    // Subscribe to shared topic
    let topic_id = TopicId::from_bytes([42; 32]);
    info!("Subscribing to topic...");
    let subscribe_handle = gossip.subscribe(topic_id, vec![]).await?;

    let (sender, mut receiver) = subscribe_handle.split();

    // Wait for join (with timeout since we might be the first node)
    info!("Waiting for gossip join...");
    match tokio::time::timeout(std::time::Duration::from_secs(2), receiver.joined()).await {
        Ok(Ok(())) => info!("Joined gossip swarm"),
        Ok(Err(e)) => warn!("Join error: {} (proceeding anyway)", e),
        Err(_) => info!("Join timeout (first node in swarm)"),
    }

    // Create bridge
    let bridge = GossipBridge::new(node_id);
    info!("GossipBridge created with node ID: {}", node_id);

    // Spawn forwarding tasks - pass endpoint, router, gossip to keep them alive
    spawn_bridge_tasks(sender, receiver, bridge.clone(), endpoint, router, gossip);

    Ok(bridge)
}

/// Spawn tokio tasks to forward messages between iroh-gossip and GossipBridge
///
/// This function spawns two concurrent tokio tasks that run for the lifetime of the application:
///
/// 1. **Outgoing Task**: Polls GossipBridge for outgoing messages and broadcasts them via gossip
/// 2. **Incoming Task**: Receives messages from gossip and pushes them into GossipBridge
///
/// # Lifetime Management
///
/// The iroh resources (endpoint, router, gossip) are moved into the first task to keep them
/// alive for the application lifetime. Without this, they would be dropped immediately and
/// the gossip connection would close.
///
/// # Platform Support
///
/// This function is only compiled on non-iOS platforms.
#[cfg(not(target_os = "ios"))]
fn spawn_bridge_tasks(
    sender: iroh_gossip::api::GossipSender,
    mut receiver: iroh_gossip::api::GossipReceiver,
    bridge: GossipBridge,
    _endpoint: iroh::Endpoint,
    _router: iroh::protocol::Router,
    _gossip: iroh_gossip::net::Gossip,
) {
    use bytes::Bytes;
    use futures_lite::StreamExt;
    use lib::networking::VersionedMessage;
    use std::time::Duration;

    let node_id = bridge.node_id();

    // Task 1: Forward outgoing messages from GossipBridge → iroh-gossip
    // Keep endpoint, router, gossip alive by moving them into this task
    let bridge_out = bridge.clone();
    tokio::spawn(async move {
        let _endpoint = _endpoint; // Keep alive for app lifetime
        let _router = _router;     // Keep alive for app lifetime
        let _gossip = _gossip;     // Keep alive for app lifetime

        loop {
            if let Some(msg) = bridge_out.try_recv_outgoing() {
                if let Ok(bytes) = bincode::serialize(&msg) {
                    if let Err(e) = sender.broadcast(Bytes::from(bytes)).await {
                        error!("[Node {}] Broadcast failed: {}", node_id, e);
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });

    // Task 2: Forward incoming messages from iroh-gossip → GossipBridge
    let bridge_in = bridge.clone();
    tokio::spawn(async move {
        loop {
            match tokio::time::timeout(Duration::from_millis(100), receiver.next()).await {
                Ok(Some(Ok(event))) => {
                    if let iroh_gossip::api::Event::Received(msg) = event {
                        if let Ok(versioned_msg) =
                            bincode::deserialize::<VersionedMessage>(&msg.content)
                        {
                            if let Err(e) = bridge_in.push_incoming(versioned_msg) {
                                error!("[Node {}] Push incoming failed: {}", node_id, e);
                            }
                        }
                    }
                }
                Ok(Some(Err(e))) => error!("[Node {}] Receiver error: {}", node_id, e),
                Ok(None) => break,
                Err(_) => {} // Timeout
            }
        }
    });
}
