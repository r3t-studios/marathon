//! Network message types for CRDT synchronization
//!
//! This module defines the protocol messages used for distributed
//! synchronization according to RFC 0001.



use crate::networking::{
    locks::LockMessage,
    operations::ComponentOp,
    session::SessionId,
    vector_clock::{
        NodeId,
        VectorClock,
    },
};

/// Top-level message envelope with versioning
///
/// All messages sent over the network are wrapped in this envelope to support
/// protocol version negotiation and future compatibility.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct VersionedMessage {
    /// Protocol version (currently 1)
    pub version: u32,

    /// The actual sync message
    pub message: SyncMessage,

    /// Nonce for selective deduplication control
    ///
    /// - For Lock messages: Unique nonce (counter + timestamp hash) to prevent
    ///   iroh-gossip deduplication, allowing repeated heartbeats.
    /// - For other messages: Constant nonce (0) to enable content-based deduplication
    ///   by iroh-gossip, preventing feedback loops.
    pub nonce: u32,
}

impl VersionedMessage {
    /// Current protocol version
    pub const CURRENT_VERSION: u32 = 1;

    /// Create a new versioned message with the current protocol version
    ///
    /// For Lock messages: Generates a unique nonce to prevent deduplication, since
    /// lock heartbeats need to be sent repeatedly even with identical content.
    ///
    /// For other messages: Uses a constant nonce (0) to enable iroh-gossip's
    /// content-based deduplication. This prevents feedback loops where the same
    /// EntityDelta gets broadcast repeatedly.
    pub fn new(message: SyncMessage) -> Self {
        // Only generate unique nonces for Lock messages (heartbeats need to bypass dedup)
        let nonce = if matches!(message, SyncMessage::Lock(_)) {
            use std::hash::Hasher;
            use std::sync::atomic::{AtomicU32, Ordering};
            use std::time::{SystemTime, UNIX_EPOCH};

            // Per-node rolling counter for sequential uniqueness
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let counter = COUNTER.fetch_add(1, Ordering::Relaxed);

            // Millisecond timestamp for temporal uniqueness
            let timestamp_millis = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u32;

            // Hash counter + timestamp for final nonce
            let mut hasher = rustc_hash::FxHasher::default();
            hasher.write_u32(counter);
            hasher.write_u32(timestamp_millis);
            hasher.finish() as u32
        } else {
            // Use constant nonce for all other messages to enable content deduplication
            0
        };

        Self {
            version: Self::CURRENT_VERSION,
            message,
            nonce,
        }
    }
}

/// Join request type - distinguishes fresh joins from rejoin attempts
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum JoinType {
    /// Fresh join - never connected to this session before
    Fresh,

    /// Rejoin - returning to a session we left earlier
    Rejoin {
        /// When we were last active in this session (Unix timestamp)
        last_active: i64,

        /// Cached entity count from when we left
        entity_count: usize,
    },
}

/// CRDT synchronization protocol messages
///
/// These messages implement the sync protocol defined in RFC 0001.
///
/// # Protocol Flow
///
/// 1. **Join**: New peer sends `JoinRequest`, receives `FullState`
/// 2. **Normal Operation**: Peers broadcast `EntityDelta` on changes
/// 3. **Anti-Entropy**: Periodic `SyncRequest` to detect missing operations
/// 4. **Recovery**: `MissingDeltas` sent in response to `SyncRequest`
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum SyncMessage {
    /// Request to join the network and receive full state
    ///
    /// Sent by a new peer when it first connects. For fresh joins, the response
    /// will be a `FullState` message. For rejoins with small deltas (<1000 ops),
    /// the response will be `MissingDeltas`.
    JoinRequest {
        /// ID of the node requesting to join
        node_id: NodeId,

        /// Session ID to join
        session_id: SessionId,

        /// Optional session secret for authentication
        session_secret: Option<bytes::Bytes>,

        /// Vector clock from when we last left this session
        /// None = fresh join, Some = rejoin
        last_known_clock: Option<VectorClock>,

        /// Type of join (fresh or rejoin with metadata)
        join_type: JoinType,
    },

    /// Complete world state sent to new peers
    ///
    /// Contains all networked entities and their components. Sent in response
    /// to a `JoinRequest`.
    FullState {
        /// All entities in the world
        entities: Vec<EntityState>,

        /// Current vector clock of the sending node
        vector_clock: VectorClock,
    },

    /// Delta update for a single entity
    ///
    /// Broadcast when a component changes. Recipients apply the operations
    /// using CRDT merge semantics.
    EntityDelta {
        /// Network ID of the entity being updated
        entity_id: uuid::Uuid,

        /// Node that generated this delta
        node_id: NodeId,

        /// Vector clock at the time this delta was created
        vector_clock: VectorClock,

        /// Component operations (Set, SetAdd, SequenceInsert, etc.)
        operations: Vec<ComponentOp>,
    },

    /// Request for operations newer than our vector clock
    ///
    /// Sent periodically for anti-entropy. The recipient compares vector
    /// clocks and sends `MissingDeltas` if they have newer operations.
    SyncRequest {
        /// ID of the node requesting sync
        node_id: NodeId,

        /// Our current vector clock
        vector_clock: VectorClock,
    },

    /// Operations that the recipient is missing
    ///
    /// Sent in response to `SyncRequest` when we have operations the peer
    /// doesn't know about yet.
    MissingDeltas {
        /// Entity deltas that the recipient is missing
        deltas: Vec<EntityDelta>,
    },

    /// Entity lock protocol messages
    ///
    /// Used for collaborative editing to prevent concurrent modifications.
    /// Locks are acquired when entities are selected and released when deselected.
    Lock(LockMessage),
}

/// Complete state of a single entity
///
/// Used in `FullState` messages to transfer all components of an entity.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct EntityState {
    /// Network ID of the entity
    pub entity_id: uuid::Uuid,

    /// Node that originally created this entity
    pub owner_node_id: NodeId,

    /// Vector clock when this entity was last updated
    pub vector_clock: VectorClock,

    /// All components on this entity
    pub components: Vec<ComponentState>,

    /// Whether this entity has been deleted (tombstone)
    pub is_deleted: bool,
}

/// State of a single component
///
/// Contains the component discriminant and its serialized data.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ComponentState {
    /// Discriminant identifying the component type
    pub discriminant: u16,

    /// Serialized component data (rkyv)
    pub data: ComponentData,
}

/// Component data - either inline or a blob reference
///
/// Components larger than 64KB are stored as blobs and referenced by hash.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, PartialEq, Eq)]
pub enum ComponentData {
    /// Inline data for small components (<64KB)
    Inline(bytes::Bytes),

    /// Reference to a blob for large components (>64KB)
    BlobRef {
        /// iroh-blobs hash
        hash: bytes::Bytes,

        /// Size of the blob in bytes
        size: u64,
    },
}

impl ComponentData {
    /// Threshold for using blobs vs inline data (64KB)
    pub const BLOB_THRESHOLD: usize = 64 * 1024;

    /// Create component data, automatically choosing inline vs blob
    pub fn new(data: bytes::Bytes) -> Self {
        if data.len() > Self::BLOB_THRESHOLD {
            // Will be populated later when uploaded to iroh-blobs
            Self::BlobRef {
                hash: bytes::Bytes::new(),
                size: data.len() as u64,
            }
        } else {
            Self::Inline(data)
        }
    }

    /// Check if this is a blob reference
    pub fn is_blob(&self) -> bool {
        matches!(self, ComponentData::BlobRef { .. })
    }

    /// Get inline data, if available
    pub fn as_inline(&self) -> Option<&[u8]> {
        match self {
            | ComponentData::Inline(data) => Some(data),
            | _ => None,
        }
    }

    /// Get blob reference, if this is a blob
    pub fn as_blob_ref(&self) -> Option<(&[u8], u64)> {
        match self {
            | ComponentData::BlobRef { hash, size } => Some((hash, *size)),
            | _ => None,
        }
    }
}

/// Wrapper for EntityDelta to allow it to be used directly
///
/// This struct exists because EntityDelta is defined as an enum variant
/// but we sometimes need to work with it as a standalone type.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct EntityDelta {
    /// Network ID of the entity being updated
    pub entity_id: uuid::Uuid,

    /// Node that generated this delta
    pub node_id: NodeId,

    /// Vector clock at the time this delta was created
    pub vector_clock: VectorClock,

    /// Component operations (Set, SetAdd, SequenceInsert, etc.)
    pub operations: Vec<ComponentOp>,
}

impl EntityDelta {
    /// Create a new entity delta
    pub fn new(
        entity_id: uuid::Uuid,
        node_id: NodeId,
        vector_clock: VectorClock,
        operations: Vec<ComponentOp>,
    ) -> Self {
        Self {
            entity_id,
            node_id,
            vector_clock,
            operations,
        }
    }

    /// Convert to a SyncMessage::EntityDelta variant
    pub fn into_message(self) -> SyncMessage {
        SyncMessage::EntityDelta {
            entity_id: self.entity_id,
            node_id: self.node_id,
            vector_clock: self.vector_clock,
            operations: self.operations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_versioned_message_creation() {
        let node_id = uuid::Uuid::new_v4();
        let session_id = SessionId::new();
        let message = SyncMessage::JoinRequest {
            node_id,
            session_id,
            session_secret: None,
            last_known_clock: None,
            join_type: JoinType::Fresh,
        };

        let versioned = VersionedMessage::new(message);
        assert_eq!(versioned.version, VersionedMessage::CURRENT_VERSION);
    }

    #[test]
    fn test_component_data_inline() {
        let data = vec![1, 2, 3, 4];
        let component_data = ComponentData::new(bytes::Bytes::from(data.clone()));

        assert!(!component_data.is_blob());
        assert_eq!(component_data.as_inline(), Some(data.as_slice()));
    }

    #[test]
    fn test_component_data_blob() {
        // Create data larger than threshold
        let data = vec![0u8; ComponentData::BLOB_THRESHOLD + 1];
        let component_data = ComponentData::new(bytes::Bytes::from(data.clone()));

        assert!(component_data.is_blob());
        assert_eq!(component_data.as_inline(), None);
    }

    #[test]
    fn test_entity_delta_creation() {
        let entity_id = uuid::Uuid::new_v4();
        let node_id = uuid::Uuid::new_v4();
        let vector_clock = VectorClock::new();

        let delta = EntityDelta::new(entity_id, node_id, vector_clock.clone(), vec![]);

        assert_eq!(delta.entity_id, entity_id);
        assert_eq!(delta.node_id, node_id);
        assert_eq!(delta.vector_clock, vector_clock);
    }

    #[test]
    fn test_message_serialization() -> anyhow::Result<()> {
        let node_id = uuid::Uuid::new_v4();
        let session_id = SessionId::new();
        let message = SyncMessage::JoinRequest {
            node_id,
            session_id,
            session_secret: None,
            last_known_clock: None,
            join_type: JoinType::Fresh,
        };

        let versioned = VersionedMessage::new(message);
        let bytes = rkyv::to_bytes::<rkyv::rancor::Failure>(&versioned).map(|b| b.to_vec())?;
        let deserialized: VersionedMessage = rkyv::from_bytes::<VersionedMessage, rkyv::rancor::Failure>(&bytes)?;

        assert_eq!(deserialized.version, versioned.version);

        Ok(())
    }

    #[test]
    fn test_full_state_serialization() -> anyhow::Result<()> {
        let entity_id = uuid::Uuid::new_v4();
        let owner_node = uuid::Uuid::new_v4();

        let entity_state = EntityState {
            entity_id,
            owner_node_id: owner_node,
            vector_clock: VectorClock::new(),
            components: vec![],
            is_deleted: false,
        };

        let message = SyncMessage::FullState {
            entities: vec![entity_state],
            vector_clock: VectorClock::new(),
        };

        let bytes = rkyv::to_bytes::<rkyv::rancor::Failure>(&message).map(|b| b.to_vec())?;
        let _deserialized: SyncMessage = rkyv::from_bytes::<SyncMessage, rkyv::rancor::Failure>(&bytes)?;

        Ok(())
    }

    #[test]
    fn test_join_type_fresh() {
        let join_type = JoinType::Fresh;

        // Fresh join should serialize correctly
        let bytes = rkyv::to_bytes::<rkyv::rancor::Failure>(&join_type).map(|b| b.to_vec()).unwrap();
        let deserialized: JoinType = rkyv::from_bytes::<JoinType, rkyv::rancor::Failure>(&bytes).unwrap();

        assert!(matches!(deserialized, JoinType::Fresh));
    }

    #[test]
    fn test_join_type_rejoin() {
        let join_type = JoinType::Rejoin {
            last_active: 1234567890,
            entity_count: 42,
        };

        // Rejoin should serialize correctly
        let bytes = rkyv::to_bytes::<rkyv::rancor::Failure>(&join_type).map(|b| b.to_vec()).unwrap();
        let deserialized: JoinType = rkyv::from_bytes::<JoinType, rkyv::rancor::Failure>(&bytes).unwrap();

        match deserialized {
            | JoinType::Rejoin {
                last_active,
                entity_count,
            } => {
                assert_eq!(last_active, 1234567890);
                assert_eq!(entity_count, 42);
            },
            | _ => panic!("Expected JoinType::Rejoin"),
        }
    }

    #[test]
    fn test_hybrid_join_protocol_fresh() {
        // Fresh join should have no last_known_clock
        let node_id = uuid::Uuid::new_v4();
        let session_id = SessionId::new();
        let message = SyncMessage::JoinRequest {
            node_id,
            session_id,
            session_secret: None,
            last_known_clock: None,
            join_type: JoinType::Fresh,
        };

        let bytes = rkyv::to_bytes::<rkyv::rancor::Failure>(&message).map(|b| b.to_vec()).unwrap();
        let deserialized: SyncMessage = rkyv::from_bytes::<SyncMessage, rkyv::rancor::Failure>(&bytes).unwrap();

        match deserialized {
            | SyncMessage::JoinRequest {
                join_type,
                last_known_clock,
                ..
            } => {
                assert!(matches!(join_type, JoinType::Fresh));
                assert!(last_known_clock.is_none());
            },
            | _ => panic!("Expected JoinRequest"),
        }
    }

    #[test]
    fn test_hybrid_join_protocol_rejoin() {
        // Rejoin should have last_known_clock
        let node_id = uuid::Uuid::new_v4();
        let session_id = SessionId::new();
        let clock = VectorClock::new();
        let message = SyncMessage::JoinRequest {
            node_id,
            session_id,
            session_secret: None,
            last_known_clock: Some(clock.clone()),
            join_type: JoinType::Rejoin {
                last_active: 1234567890,
                entity_count: 100,
            },
        };

        let bytes = rkyv::to_bytes::<rkyv::rancor::Failure>(&message).map(|b| b.to_vec()).unwrap();
        let deserialized: SyncMessage = rkyv::from_bytes::<SyncMessage, rkyv::rancor::Failure>(&bytes).unwrap();

        match deserialized {
            | SyncMessage::JoinRequest {
                join_type,
                last_known_clock,
                ..
            } => {
                assert!(matches!(join_type, JoinType::Rejoin { .. }));
                assert_eq!(last_known_clock, Some(clock));
            },
            | _ => panic!("Expected JoinRequest"),
        }
    }

    #[test]
    fn test_missing_deltas_serialization() -> anyhow::Result<()> {
        // Test that MissingDeltas message serializes correctly
        let node_id = uuid::Uuid::new_v4();
        let entity_id = uuid::Uuid::new_v4();
        let clock = VectorClock::new();

        let delta = EntityDelta {
            entity_id,
            node_id,
            vector_clock: clock,
            operations: vec![],
        };

        let message = SyncMessage::MissingDeltas {
            deltas: vec![delta],
        };

        let bytes = rkyv::to_bytes::<rkyv::rancor::Failure>(&message).map(|b| b.to_vec())?;
        let deserialized: SyncMessage = rkyv::from_bytes::<SyncMessage, rkyv::rancor::Failure>(&bytes)?;

        match deserialized {
            | SyncMessage::MissingDeltas { deltas } => {
                assert_eq!(deltas.len(), 1);
                assert_eq!(deltas[0].entity_id, entity_id);
            },
            | _ => panic!("Expected MissingDeltas"),
        }

        Ok(())
    }
}
