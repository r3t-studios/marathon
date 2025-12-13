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

    // CRDT state
    vector_clock: VectorClock,
    operation_log: OperationLog,
    tombstones: TombstoneRegistry,
    locks: EntityLockRegistry,

    // Track locks we own for heartbeat broadcasting
    our_locks: std::collections::HashSet<uuid::Uuid>,
}

impl NetworkingManager {
    pub async fn new(session_id: SessionId) -> anyhow::Result<Self> {
        use iroh::{
            discovery::mdns::MdnsDiscovery,
            protocol::Router,
            Endpoint,
        };
        use iroh_gossip::{
            net::Gossip,
            proto::TopicId,
        };

        // Create iroh endpoint with mDNS discovery
        let endpoint = Endpoint::builder()
            .discovery(MdnsDiscovery::builder())
            .bind()
            .await?;

        let endpoint_id = endpoint.addr().id;

        // Convert endpoint ID to NodeId (using first 16 bytes)
        let id_bytes = endpoint_id.as_bytes();
        let mut node_id_bytes = [0u8; 16];
        node_id_bytes.copy_from_slice(&id_bytes[..16]);
        let node_id = NodeId::from_bytes(node_id_bytes);

        // Create gossip protocol
        let gossip = Gossip::builder().spawn(endpoint.clone());

        // Derive session-specific ALPN for network isolation
        let session_alpn = session_id.to_alpn();

        // Set up router to accept session ALPN
        let router = Router::builder(endpoint.clone())
            .accept(session_alpn.as_slice(), gossip.clone())
            .spawn();

        // Subscribe to topic derived from session ALPN
        let topic_id = TopicId::from_bytes(session_alpn);
        let subscribe_handle = gossip.subscribe(topic_id, vec![]).await?;

        let (sender, receiver) = subscribe_handle.split();

        tracing::info!(
            "NetworkingManager started for session {} with node {}",
            session_id.to_code(),
            node_id
        );

        let manager = Self {
            session_id,
            node_id,
            sender,
            receiver,
            _endpoint: endpoint,
            _router: router,
            _gossip: gossip,
            vector_clock: VectorClock::new(),
            operation_log: OperationLog::new(),
            tombstones: TombstoneRegistry::new(),
            locks: EntityLockRegistry::new(),
            our_locks: std::collections::HashSet::new(),
        };

        Ok(manager)
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn session_id(&self) -> SessionId {
        self.session_id.clone()
    }

    /// Process gossip events (unbounded) and periodic tasks (heartbeats, lock cleanup)
    pub async fn run(mut self, event_tx: mpsc::UnboundedSender<EngineEvent>) {
        let mut heartbeat_interval = time::interval(Duration::from_secs(1));

        loop {
            tokio::select! {
                // Process gossip events unbounded (as fast as they arrive)
                Some(result) = self.receiver.next() => {
                    match result {
                        Ok(event) => {
                            use iroh_gossip::api::Event;
                            if let Event::Received(msg) = event {
                                self.handle_sync_message(&msg.content, &event_tx).await;
                            }
                            // Note: Neighbor events are not exposed in the current API
                        }
                        Err(e) => {
                            tracing::warn!("Gossip receiver error: {}", e);
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
        let versioned: VersionedMessage = match bincode::deserialize(msg_bytes) {
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

            if let Ok(bytes) = bincode::serialize(&msg) {
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
