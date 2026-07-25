//! Gossip networking setup with dedicated tokio runtime
//!
//! This module manages iroh-gossip networking with a tokio runtime running as a
//! sidecar to Bevy. The tokio runtime runs in a dedicated background thread,
//! separate from Bevy's ECS loop.
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

mod control_socket;

use anyhow::Result;
use bevy::prelude::*;
use control_socket::spawn_control_socket;
use libmarathon::networking::{
    GossipBridge,
    SessionId,
};
use uuid::Uuid;

/// Session ID to use for network initialization
///
/// This resource must be inserted before setup_gossip_networking runs.
/// It provides the session ID used to derive the session-specific ALPN.
#[derive(Resource, Clone)]
pub struct InitialSessionId(pub SessionId);

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
///
/// # Requirements
///
/// The InitialSessionId resource must be inserted before this system runs.
/// If not present, an error is logged and networking is disabled.
pub fn setup_gossip_networking(mut commands: Commands, session_id: Option<Res<InitialSessionId>>) {
    let Some(session_id) = session_id else {
        error!("InitialSessionId resource not found - cannot initialize networking");
        return;
    };

    info!(
        "Setting up gossip networking for session {}...",
        session_id.0
    );

    // Spawn dedicated thread with Tokio runtime for gossip initialization
    #[cfg(not(target_os = "ios"))]
    {
        let (sender, receiver) = crossbeam_channel::unbounded();
        commands.insert_resource(GossipBridgeChannel(receiver));

        let session_id = session_id.0.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                match init_gossip(session_id).await {
                    | Ok(bridge) => {
                        info!("Gossip bridge initialized successfully");
                        if let Err(e) = sender.send(bridge) {
                            error!("Failed to send bridge to main thread: {}", e);
                        }
                    },
                    | Err(e) => {
                        error!("Failed to initialize gossip: {}", e);
                    },
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
    #[cfg(not(target_os = "ios"))] channel: Option<Res<GossipBridgeChannel>>,
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

/// Initialize iroh-gossip networking stack with session-specific ALPN
///
/// This async function runs in the background tokio runtime and:
/// 1. Creates an iroh endpoint with mDNS discovery
/// 2. Spawns the gossip protocol
/// 3. Derives session-specific ALPN from session ID (using BLAKE3)
/// 4. Sets up the router to accept connections on the session ALPN
/// 5. Subscribes to a topic derived from the session ALPN
/// 6. Waits for join with a 2-second timeout
/// 7. Creates and configures the GossipBridge
/// 8. Spawns forwarding tasks to bridge messages
///
/// # Parameters
///
/// - `session_id`: The session ID used to derive the ALPN for network isolation
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
async fn init_gossip(session_id: SessionId) -> Result<GossipBridge> {
    use iroh::{
        Endpoint,
        discovery::mdns::MdnsDiscovery,
        protocol::Router,
    };
    use iroh_gossip::{
        net::Gossip,
        proto::TopicId,
    };

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

    // Derive session-specific ALPN for network isolation
    let session_alpn = session_id.to_alpn();
    info!("Using session-specific ALPN (session: {})", session_id);

    info!("Setting up router...");
    let router = Router::builder(endpoint.clone())
        .accept(session_alpn.as_slice(), gossip.clone())
        .spawn();

    // Subscribe to topic derived from session ALPN (use same bytes for consistency)
    let topic_id = TopicId::from_bytes(session_alpn);
    info!("Subscribing to session topic...");
    let subscribe_handle = gossip.subscribe(topic_id, vec![]).await?;

    let (sender, mut receiver) = subscribe_handle.split();

    // Wait for join (with timeout since we might be the first node)
    // Increased timeout to 10s to allow mDNS discovery to work
    info!("Waiting for gossip join (10s timeout for mDNS discovery)...");
    match tokio::time::timeout(std::time::Duration::from_secs(10), receiver.joined()).await {
        | Ok(Ok(())) => info!("Joined gossip swarm successfully"),
        | Ok(Err(e)) => warn!("Join error: {} (proceeding anyway)", e),
        | Err(_) => info!("Join timeout - likely first node in swarm (proceeding anyway)"),
    }

    // Create bridge
    let bridge = GossipBridge::new(node_id);
    info!("GossipBridge created with node ID: {}", node_id);

    // Spawn forwarding tasks - pass endpoint, router, gossip to keep them alive
    spawn_bridge_tasks(sender, receiver, bridge.clone(), endpoint, router, gossip);

    // Spawn control socket server for remote control (debug only)
    spawn_control_socket(session_id, bridge.clone(), node_id);

    Ok(bridge)
}

/// Spawn tokio tasks to forward messages between iroh-gossip and GossipBridge
///
/// This function spawns two concurrent tokio tasks that run for the lifetime of
/// the application:
///
/// 1. **Outgoing Task**: Polls GossipBridge for outgoing messages and
///    broadcasts them via gossip
/// 2. **Incoming Task**: Receives messages from gossip and pushes them into
///    GossipBridge
///
/// # Lifetime Management
///
/// The iroh resources (endpoint, router, gossip) are moved into the first task
/// to keep them alive for the application lifetime. Without this, they would be
/// dropped immediately and the gossip connection would close.
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
    use std::time::Duration;

    use bytes::Bytes;
    use futures_lite::StreamExt;
    use libmarathon::networking::VersionedMessage;

    let node_id = bridge.node_id();

    // Task 1: Forward outgoing messages from GossipBridge → iroh-gossip
    // Keep endpoint, router, gossip alive by moving them into this task
    let bridge_out = bridge.clone();
    tokio::spawn(async move {
        let _endpoint = _endpoint; // Keep alive for app lifetime
        let _router = _router; // Keep alive for app lifetime
        let _gossip = _gossip; // Keep alive for app lifetime

        loop {
            if let Some(msg) = bridge_out.try_recv_outgoing() {
                if let Ok(bytes) = rkyv::to_bytes::<rkyv::rancor::Failure>(&msg).map(|b| b.to_vec())
                {
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
                | Ok(Some(Ok(event))) => match event {
                    | iroh_gossip::api::Event::Received(msg) => {
                        info!("[Node {}] Received message from gossip", node_id);
                        if let Ok(versioned_msg) = rkyv::from_bytes::<
                            VersionedMessage,
                            rkyv::rancor::Failure,
                        >(&msg.content)
                        {
                            if let Err(e) = bridge_in.push_incoming(versioned_msg) {
                                error!("[Node {}] Push incoming failed: {}", node_id, e);
                            }
                        }
                    },
                    | iroh_gossip::api::Event::NeighborUp(peer_id) => {
                        info!("[Node {}] Peer connected: {}", node_id, peer_id);
                    },
                    | iroh_gossip::api::Event::NeighborDown(peer_id) => {
                        warn!("[Node {}] Peer disconnected: {}", node_id, peer_id);
                    },
                    | iroh_gossip::api::Event::Lagged => {
                        warn!(
                            "[Node {}] Event stream lagged - some events may have been missed",
                            node_id
                        );
                    },
                },
                | Ok(Some(Err(e))) => error!("[Node {}] Receiver error: {}", node_id, e),
                | Ok(None) => break,
                | Err(_) => {}, // Timeout
            }
        }
    });
}
