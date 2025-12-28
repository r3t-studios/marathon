//! Networking Manager - handles iroh networking and CRDT state outside Bevy

use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time;
use bytes::Bytes;
use futures_lite::StreamExt;

use crate::networking::{
    EntityLockRegistry, NodeId, OperationLog, SessionId, TombstoneRegistry, VectorClock,
    VersionedMessage, SyncMessage, LockMessage,
};

use super::EngineEvent;
use super::events::NetworkingInitStatus;

pub struct NetworkingManager {
    session_id: SessionId,
    node_id: NodeId,

    // Iroh networking
    sender: iroh_gossip::api::GossipSender,
    receiver: iroh_gossip::api::GossipReceiver,

    // Keep these alive for the lifetime of the manager
    _endpoint: iroh::Endpoint,
    _router: iroh::protocol::Router,
    _gossip: iroh_gossip::net::Gossip,

    // Bridge to Bevy for message passing
    bridge: crate::networking::GossipBridge,

    // CRDT state
    vector_clock: VectorClock,
    operation_log: OperationLog,
    tombstones: TombstoneRegistry,
    locks: EntityLockRegistry,

    // Track locks we own for heartbeat broadcasting
    our_locks: std::collections::HashSet<uuid::Uuid>,
}

impl NetworkingManager {
    pub async fn new(
        session_id: SessionId,
        progress_tx: Option<tokio::sync::mpsc::UnboundedSender<NetworkingInitStatus>>,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> anyhow::Result<(Self, crate::networking::GossipBridge)> {
        let send_progress = |status: NetworkingInitStatus| {
            if let Some(ref tx) = progress_tx {
                let _ = tx.send(status.clone());
            }
            tracing::info!("Networking init: {:?}", status);
        };
        use iroh::{
            discovery::pkarr::dht::DhtDiscovery,
            protocol::Router,
            Endpoint,
        };
        use iroh_gossip::{
            net::Gossip,
            proto::TopicId,
        };

        // Check for cancellation at start
        if cancel_token.is_cancelled() {
            return Err(anyhow::anyhow!("Initialization cancelled before start"));
        }

        send_progress(NetworkingInitStatus::CreatingEndpoint);

        // Create iroh endpoint with DHT discovery
        // This allows peers to discover each other over the internet via Mainline DHT
        // Security comes from the secret session-derived ALPN, not network isolation
        let dht_discovery = DhtDiscovery::builder().build()?;
        let endpoint = Endpoint::builder()
            .discovery(dht_discovery)
            .bind()
            .await?;

        send_progress(NetworkingInitStatus::EndpointReady);

        let endpoint_id = endpoint.addr().id;

        // Convert endpoint ID to NodeId (using first 16 bytes)
        let id_bytes = endpoint_id.as_bytes();
        let mut node_id_bytes = [0u8; 16];
        node_id_bytes.copy_from_slice(&id_bytes[..16]);
        let node_id = NodeId::from_bytes(node_id_bytes);

        // Create pkarr client for DHT peer discovery
        let pkarr_client = pkarr::Client::builder()
            .no_default_network()
            .dht(|x| x)
            .build()?;

        // Discover existing peers from DHT with retries
        // Retry immediately without delays - if peers aren't in DHT yet, they'll appear soon
        let mut peer_endpoint_ids = vec![];
        for attempt in 1..=3 {
            // Check for cancellation before each attempt
            if cancel_token.is_cancelled() {
                tracing::info!("Networking initialization cancelled during DHT discovery");
                return Err(anyhow::anyhow!("Initialization cancelled"));
            }

            send_progress(NetworkingInitStatus::DiscoveringPeers {
                session_code: session_id.to_code().to_string(),
                attempt,
            });
            match crate::engine::peer_discovery::discover_peers_from_dht(&session_id, &pkarr_client).await {
                Ok(peers) if !peers.is_empty() => {
                    let count = peers.len();
                    peer_endpoint_ids = peers;
                    send_progress(NetworkingInitStatus::PeersFound {
                        count,
                    });
                    break;
                }
                Ok(_) if attempt == 3 => {
                    // Last attempt and no peers found
                    send_progress(NetworkingInitStatus::NoPeersFound);
                }
                Ok(_) => {
                    // No peers found, but will retry immediately
                }
                Err(e) => {
                    tracing::warn!("DHT query attempt {} failed: {}", attempt, e);
                }
            }
        }

        // Check for cancellation before publishing
        if cancel_token.is_cancelled() {
            tracing::info!("Networking initialization cancelled before DHT publish");
            return Err(anyhow::anyhow!("Initialization cancelled"));
        }

        // Publish our presence to DHT
        send_progress(NetworkingInitStatus::PublishingToDHT);
        if let Err(e) = crate::engine::peer_discovery::publish_peer_to_dht(
            &session_id,
            endpoint_id,
            &pkarr_client,
        )
        .await
        {
            tracing::warn!("Failed to publish to DHT: {}", e);
        }

        // Check for cancellation before gossip initialization
        if cancel_token.is_cancelled() {
            tracing::info!("Networking initialization cancelled before gossip init");
            return Err(anyhow::anyhow!("Initialization cancelled"));
        }

        // Derive session-specific ALPN for network isolation
        let session_alpn = session_id.to_alpn();

        // Create gossip protocol with custom session ALPN
        send_progress(NetworkingInitStatus::InitializingGossip);
        let gossip = Gossip::builder()
            .alpn(&session_alpn)
            .spawn(endpoint.clone());

        // Set up router to accept session ALPN
        let router = Router::builder(endpoint.clone())
            .accept(session_alpn.as_slice(), gossip.clone())
            .spawn();

        // Subscribe to topic with discovered peers as bootstrap
        let topic_id = TopicId::from_bytes(session_alpn);
        let subscribe_handle = gossip.subscribe(topic_id, peer_endpoint_ids).await?;

        let (sender, receiver) = subscribe_handle.split();

        tracing::info!(
            "NetworkingManager started for session {} with node {}",
            session_id.to_code(),
            node_id
        );

        // Create GossipBridge for Bevy integration
        let bridge = crate::networking::GossipBridge::new(node_id);

        // Spawn background task to maintain DHT presence
        let session_id_clone = session_id.clone();
        tokio::spawn(crate::engine::peer_discovery::maintain_dht_presence(
            session_id_clone,
            endpoint_id,
            pkarr_client,
        ));

        let manager = Self {
            session_id,
            node_id,
            sender,
            receiver,
            _endpoint: endpoint,
            _router: router,
            _gossip: gossip,
            bridge: bridge.clone(),
            vector_clock: VectorClock::new(),
            operation_log: OperationLog::new(),
            tombstones: TombstoneRegistry::new(),
            locks: EntityLockRegistry::new(),
            our_locks: std::collections::HashSet::new(),
        };

        Ok((manager, bridge))
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn session_id(&self) -> SessionId {
        self.session_id.clone()
    }

    /// Process gossip events (unbounded) and periodic tasks (heartbeats, lock cleanup)
    /// Also bridges messages between iroh-gossip and Bevy's GossipBridge
    pub async fn run(mut self, event_tx: mpsc::UnboundedSender<EngineEvent>, cancel_token: tokio_util::sync::CancellationToken) {
        let mut heartbeat_interval = time::interval(Duration::from_secs(1));
        let mut bridge_poll_interval = time::interval(Duration::from_millis(10));

        loop {
            tokio::select! {
                // Listen for shutdown signal
                _ = cancel_token.cancelled() => {
                    tracing::info!("NetworkingManager received shutdown signal");
                    break;
                }
                // Process incoming gossip messages and forward to GossipBridge
                Some(result) = self.receiver.next() => {
                    match result {
                        Ok(event) => {
                            use iroh_gossip::api::Event;
                            match event {
                                Event::Received(msg) => {
                                    // Deserialize and forward to GossipBridge for Bevy systems
                                    if let Ok(versioned) = rkyv::from_bytes::<VersionedMessage, rkyv::rancor::Failure>(&msg.content) {
                                        if let Err(e) = self.bridge.push_incoming(versioned) {
                                            tracing::error!("Failed to push message to GossipBridge: {}", e);
                                        } else {
                                            tracing::debug!("Forwarded message to Bevy via GossipBridge");
                                        }
                                    }
                                }
                                Event::NeighborUp(peer) => {
                                    tracing::info!("Peer connected: {}", peer);

                                    // Convert PublicKey to NodeId for Bevy
                                    let peer_bytes = peer.as_bytes();
                                    let mut node_id_bytes = [0u8; 16];
                                    node_id_bytes.copy_from_slice(&peer_bytes[..16]);
                                    let peer_node_id = NodeId::from_bytes(node_id_bytes);

                                    // Notify Bevy of peer join
                                    let _ = event_tx.send(EngineEvent::PeerJoined {
                                        node_id: peer_node_id,
                                    });
                                }
                                Event::NeighborDown(peer) => {
                                    tracing::warn!("Peer disconnected: {}", peer);

                                    // Convert PublicKey to NodeId for Bevy
                                    let peer_bytes = peer.as_bytes();
                                    let mut node_id_bytes = [0u8; 16];
                                    node_id_bytes.copy_from_slice(&peer_bytes[..16]);
                                    let peer_node_id = NodeId::from_bytes(node_id_bytes);

                                    // Notify Bevy of peer leave
                                    let _ = event_tx.send(EngineEvent::PeerLeft {
                                        node_id: peer_node_id,
                                    });
                                }
                                Event::Lagged => {
                                    tracing::warn!("Event stream lagged");
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Gossip receiver error: {}", e);
                        }
                    }
                }

                // Poll GossipBridge for outgoing messages and broadcast via iroh
                _ = bridge_poll_interval.tick() => {
                    while let Some(msg) = self.bridge.try_recv_outgoing() {
                        if let Ok(bytes) = rkyv::to_bytes::<rkyv::rancor::Failure>(&msg).map(|b| b.to_vec()) {
                            if let Err(e) = self.sender.broadcast(Bytes::from(bytes)).await {
                                tracing::error!("Failed to broadcast message: {}", e);
                            } else {
                                tracing::debug!("Broadcast message from Bevy via iroh-gossip");
                            }
                        }
                    }
                }

                // Periodic tasks: heartbeats and lock cleanup
                _ = heartbeat_interval.tick() => {
                    self.broadcast_lock_heartbeats(&event_tx).await;
                    self.cleanup_expired_locks(&event_tx);
                }
            }
        }
    }


    async fn handle_sync_message(&mut self, msg_bytes: &[u8], event_tx: &mpsc::UnboundedSender<EngineEvent>) {
        // Deserialize SyncMessage
        let versioned: VersionedMessage = match rkyv::from_bytes::<VersionedMessage, rkyv::rancor::Failure>(msg_bytes) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("Failed to deserialize sync message: {}", e);
                return;
            }
        };

        match versioned.message {
            SyncMessage::Lock(lock_msg) => {
                self.handle_lock_message(lock_msg, event_tx);
            }
            _ => {
                // TODO: Handle other message types (ComponentOp, EntitySpawn, etc.)
                tracing::debug!("Unhandled sync message type");
            }
        }
    }

    fn handle_lock_message(&mut self, msg: LockMessage, event_tx: &mpsc::UnboundedSender<EngineEvent>) {
        match msg {
            LockMessage::LockRequest { entity_id, node_id } => {
                match self.locks.try_acquire(entity_id, node_id) {
                    Ok(()) => {
                        // Track if this is our lock
                        if node_id == self.node_id {
                            self.our_locks.insert(entity_id);
                        }

                        let _ = event_tx.send(EngineEvent::LockAcquired {
                            entity_id,
                            holder: node_id,
                        });
                    }
                    Err(current_holder) => {
                        let _ = event_tx.send(EngineEvent::LockDenied {
                            entity_id,
                            current_holder,
                        });
                    }
                }
            }
            LockMessage::LockHeartbeat { entity_id, holder } => {
                self.locks.renew_heartbeat(entity_id, holder);
            }
            LockMessage::LockRelease { entity_id, node_id } => {
                self.locks.release(entity_id, node_id);

                // Remove from our locks tracking
                if node_id == self.node_id {
                    self.our_locks.remove(&entity_id);
                }

                let _ = event_tx.send(EngineEvent::LockReleased { entity_id });
            }
            _ => {}
        }
    }

    async fn broadcast_lock_heartbeats(&mut self, _event_tx: &mpsc::UnboundedSender<EngineEvent>) {
        // Broadcast heartbeats for locks we hold
        for entity_id in self.our_locks.iter().copied() {
            self.locks.renew_heartbeat(entity_id, self.node_id);

            let msg = VersionedMessage::new(SyncMessage::Lock(LockMessage::LockHeartbeat {
                entity_id,
                holder: self.node_id,
            }));

            if let Ok(bytes) = rkyv::to_bytes::<rkyv::rancor::Failure>(&msg).map(|b| b.to_vec()) {
                let _ = self.sender.broadcast(Bytes::from(bytes)).await;
            }
        }
    }

    fn cleanup_expired_locks(&mut self, event_tx: &mpsc::UnboundedSender<EngineEvent>) {
        // Get expired locks from registry
        let expired = self.locks.get_expired_locks();

        for entity_id in expired {
            // Only cleanup if it's not our lock
            if let Some(holder) = self.locks.get_holder(entity_id, self.node_id) {
                if holder != self.node_id {
                    self.locks.force_release(entity_id);
                    let _ = event_tx.send(EngineEvent::LockExpired { entity_id });
                    tracing::info!("Lock expired for entity {}", entity_id);
                }
            }
        }
    }

    pub async fn shutdown(self) {
        tracing::info!("NetworkingManager shut down");
        // endpoint and gossip will be dropped automatically
    }
}
