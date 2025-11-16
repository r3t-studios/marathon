//! Network message types for CRDT synchronization
//!
//! This module defines the protocol messages used for distributed
//! synchronization according to RFC 0001.

use serde::{
    Deserialize,
    Serialize,
};

use crate::networking::{
    operations::ComponentOp,
    vector_clock::{
        NodeId,
        VectorClock,
    },
};

/// Top-level message envelope with versioning
///
/// All messages sent over the network are wrapped in this envelope to support
/// protocol version negotiation and future compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedMessage {
    /// Protocol version (currently 1)
    pub version: u32,

    /// The actual sync message
    pub message: SyncMessage,
}

impl VersionedMessage {
    /// Current protocol version
    pub const CURRENT_VERSION: u32 = 1;

    /// Create a new versioned message with the current protocol version
    pub fn new(message: SyncMessage) -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            message,
        }
    }
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncMessage {
    /// Request to join the network and receive full state
    ///
    /// Sent by a new peer when it first connects. The response will be a
    /// `FullState` message containing all entities and their components.
    JoinRequest {
        /// ID of the node requesting to join
        node_id: NodeId,

        /// Optional session secret for authentication
        session_secret: Option<Vec<u8>>,
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
}

/// Complete state of a single entity
///
/// Used in `FullState` messages to transfer all components of an entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
/// Contains the component type and its serialized data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentState {
    /// Type path of the component (e.g., "bevy_transform::components::Transform")
    pub component_type: String,

    /// Serialized component data (bincode)
    pub data: ComponentData,
}

/// Component data - either inline or a blob reference
///
/// Components larger than 64KB are stored as blobs and referenced by hash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ComponentData {
    /// Inline data for small components (<64KB)
    Inline(Vec<u8>),

    /// Reference to a blob for large components (>64KB)
    BlobRef {
        /// iroh-blobs hash
        hash: Vec<u8>,

        /// Size of the blob in bytes
        size: u64,
    },
}

impl ComponentData {
    /// Threshold for using blobs vs inline data (64KB)
    pub const BLOB_THRESHOLD: usize = 64 * 1024;

    /// Create component data, automatically choosing inline vs blob
    pub fn new(data: Vec<u8>) -> Self {
        if data.len() > Self::BLOB_THRESHOLD {
            // Will be populated later when uploaded to iroh-blobs
            Self::BlobRef {
                hash: Vec::new(),
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        let message = SyncMessage::JoinRequest {
            node_id,
            session_secret: None,
        };

        let versioned = VersionedMessage::new(message);
        assert_eq!(versioned.version, VersionedMessage::CURRENT_VERSION);
    }

    #[test]
    fn test_component_data_inline() {
        let data = vec![1, 2, 3, 4];
        let component_data = ComponentData::new(data.clone());

        assert!(!component_data.is_blob());
        assert_eq!(component_data.as_inline(), Some(data.as_slice()));
    }

    #[test]
    fn test_component_data_blob() {
        // Create data larger than threshold
        let data = vec![0u8; ComponentData::BLOB_THRESHOLD + 1];
        let component_data = ComponentData::new(data.clone());

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
    fn test_message_serialization() -> bincode::Result<()> {
        let node_id = uuid::Uuid::new_v4();
        let message = SyncMessage::JoinRequest {
            node_id,
            session_secret: None,
        };

        let versioned = VersionedMessage::new(message);
        let bytes = bincode::serialize(&versioned)?;
        let deserialized: VersionedMessage = bincode::deserialize(&bytes)?;

        assert_eq!(deserialized.version, versioned.version);

        Ok(())
    }

    #[test]
    fn test_full_state_serialization() -> bincode::Result<()> {
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

        let bytes = bincode::serialize(&message)?;
        let _deserialized: SyncMessage = bincode::deserialize(&bytes)?;

        Ok(())
    }
}
