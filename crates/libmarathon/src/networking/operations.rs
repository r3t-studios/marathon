//! CRDT operations for component synchronization
//!
//! This module defines the different types of operations that can be performed
//! on components in the distributed system. Each operation type corresponds to
//! a specific CRDT merge strategy.



use crate::networking::{
    messages::ComponentData,
    vector_clock::VectorClock,
};

/// Component operations for CRDT synchronization
///
/// Different operation types support different CRDT semantics:
///
/// - **Set** - Last-Write-Wins (LWW) using vector clocks
/// - **SetAdd/SetRemove** - OR-Set for concurrent add/remove
/// - **SequenceInsert/SequenceDelete** - RGA for ordered sequences
/// - **Delete** - Entity deletion with tombstone
///
/// # CRDT Merge Semantics
///
/// ## Last-Write-Wins (Set)
/// - Use vector clock to determine which operation happened later
/// - If concurrent, use node ID as tiebreaker
/// - Example: Transform component position changes
///
/// ## OR-Set (SetAdd/SetRemove)
/// - Add wins over remove when concurrent
/// - Uses unique operation IDs to track add/remove pairs
/// - Example: Selection of multiple entities, tags
///
/// ## Sequence CRDT (SequenceInsert/SequenceDelete)
/// - Maintains ordering across concurrent inserts
/// - Uses RGA (Replicated Growable Array) algorithm
/// - Example: Collaborative drawing paths
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ComponentOp {
    /// Set a component value (Last-Write-Wins)
    ///
    /// Used for components where the latest value should win. The vector clock
    /// determines which operation is "later". If operations are concurrent,
    /// the node ID is used as a tiebreaker for deterministic results.
    ///
    /// The data field can be either inline (for small components) or a blob
    /// reference (for components >64KB).
    Set {
        /// Discriminant identifying the component type
        discriminant: u16,

        /// Component data (inline or blob reference)
        data: ComponentData,

        /// Vector clock when this set operation was created
        vector_clock: VectorClock,
    },

    /// Add an element to an OR-Set
    ///
    /// Adds an element to a set that supports concurrent add/remove. Each add
    /// has a unique ID so that removes can reference specific adds.
    SetAdd {
        /// Discriminant identifying the component type
        discriminant: u16,

        /// Unique ID for this add operation
        operation_id: uuid::Uuid,

        /// Element being added (serialized)
        element: bytes::Bytes,

        /// Vector clock when this add was created
        vector_clock: VectorClock,
    },

    /// Remove an element from an OR-Set
    ///
    /// Removes an element by referencing the add operation IDs that added it.
    /// If concurrent with an add, the add wins (observed-remove semantics).
    SetRemove {
        /// Discriminant identifying the component type
        discriminant: u16,

        /// IDs of the add operations being removed
        removed_ids: Vec<uuid::Uuid>,

        /// Vector clock when this remove was created
        vector_clock: VectorClock,
    },

    /// Insert an element into a sequence (RGA)
    ///
    /// Inserts an element after a specific position in a sequence. Uses RGA
    /// (Replicated Growable Array) to maintain consistent ordering across
    /// concurrent inserts.
    SequenceInsert {
        /// Discriminant identifying the component type
        discriminant: u16,

        /// Unique ID for this insert operation
        operation_id: uuid::Uuid,

        /// ID of the element to insert after (None = beginning)
        after_id: Option<uuid::Uuid>,

        /// Element being inserted (serialized)
        element: bytes::Bytes,

        /// Vector clock when this insert was created
        vector_clock: VectorClock,
    },

    /// Delete an element from a sequence (RGA)
    ///
    /// Marks an element as deleted in the sequence. The element remains in the
    /// structure (tombstone) to preserve ordering for concurrent operations.
    SequenceDelete {
        /// Discriminant identifying the component type
        discriminant: u16,

        /// ID of the element to delete
        element_id: uuid::Uuid,

        /// Vector clock when this delete was created
        vector_clock: VectorClock,
    },

    /// Delete an entire entity
    ///
    /// Marks an entity as deleted (tombstone). The entity remains in the
    /// system to prevent resurrection if old operations arrive.
    Delete {
        /// Vector clock when this delete was created
        vector_clock: VectorClock,
    },
}

impl ComponentOp {
    /// Get the component discriminant for this operation
    pub fn discriminant(&self) -> Option<u16> {
        match self {
            | ComponentOp::Set { discriminant, .. } |
            ComponentOp::SetAdd { discriminant, .. } |
            ComponentOp::SetRemove { discriminant, .. } |
            ComponentOp::SequenceInsert { discriminant, .. } |
            ComponentOp::SequenceDelete { discriminant, .. } => Some(*discriminant),
            | ComponentOp::Delete { .. } => None,
        }
    }

    /// Get the vector clock for this operation
    pub fn vector_clock(&self) -> &VectorClock {
        match self {
            | ComponentOp::Set { vector_clock, .. } |
            ComponentOp::SetAdd { vector_clock, .. } |
            ComponentOp::SetRemove { vector_clock, .. } |
            ComponentOp::SequenceInsert { vector_clock, .. } |
            ComponentOp::SequenceDelete { vector_clock, .. } |
            ComponentOp::Delete { vector_clock } => vector_clock,
        }
    }

    /// Check if this is a Set operation (LWW)
    pub fn is_set(&self) -> bool {
        matches!(self, ComponentOp::Set { .. })
    }

    /// Check if this is an OR-Set operation
    pub fn is_or_set(&self) -> bool {
        matches!(
            self,
            ComponentOp::SetAdd { .. } | ComponentOp::SetRemove { .. }
        )
    }

    /// Check if this is a Sequence operation (RGA)
    pub fn is_sequence(&self) -> bool {
        matches!(
            self,
            ComponentOp::SequenceInsert { .. } | ComponentOp::SequenceDelete { .. }
        )
    }

    /// Check if this is a Delete operation
    pub fn is_delete(&self) -> bool {
        matches!(self, ComponentOp::Delete { .. })
    }
}

/// Builder for creating ComponentOp instances
///
/// Provides a fluent API for constructing operations with proper vector clock
/// timestamps.
pub struct ComponentOpBuilder {
    node_id: uuid::Uuid,
    vector_clock: VectorClock,
}

impl ComponentOpBuilder {
    /// Create a new operation builder
    pub fn new(node_id: uuid::Uuid, vector_clock: VectorClock) -> Self {
        Self {
            node_id,
            vector_clock,
        }
    }

    /// Build a Set operation (LWW)
    pub fn set(mut self, discriminant: u16, data: ComponentData) -> ComponentOp {
        self.vector_clock.increment(self.node_id);
        ComponentOp::Set {
            discriminant,
            data,
            vector_clock: self.vector_clock,
        }
    }

    /// Build a SetAdd operation (OR-Set)
    pub fn set_add(mut self, discriminant: u16, element: bytes::Bytes) -> ComponentOp {
        self.vector_clock.increment(self.node_id);
        ComponentOp::SetAdd {
            discriminant,
            operation_id: uuid::Uuid::new_v4(),
            element,
            vector_clock: self.vector_clock,
        }
    }

    /// Build a SetRemove operation (OR-Set)
    pub fn set_remove(
        mut self,
        discriminant: u16,
        removed_ids: Vec<uuid::Uuid>,
    ) -> ComponentOp {
        self.vector_clock.increment(self.node_id);
        ComponentOp::SetRemove {
            discriminant,
            removed_ids,
            vector_clock: self.vector_clock,
        }
    }

    /// Build a SequenceInsert operation (RGA)
    pub fn sequence_insert(
        mut self,
        discriminant: u16,
        after_id: Option<uuid::Uuid>,
        element: bytes::Bytes,
    ) -> ComponentOp {
        self.vector_clock.increment(self.node_id);
        ComponentOp::SequenceInsert {
            discriminant,
            operation_id: uuid::Uuid::new_v4(),
            after_id,
            element,
            vector_clock: self.vector_clock,
        }
    }

    /// Build a SequenceDelete operation (RGA)
    pub fn sequence_delete(
        mut self,
        discriminant: u16,
        element_id: uuid::Uuid,
    ) -> ComponentOp {
        self.vector_clock.increment(self.node_id);
        ComponentOp::SequenceDelete {
            discriminant,
            element_id,
            vector_clock: self.vector_clock,
        }
    }

    /// Build a Delete operation
    pub fn delete(mut self) -> ComponentOp {
        self.vector_clock.increment(self.node_id);
        ComponentOp::Delete {
            vector_clock: self.vector_clock,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discriminant() {
        let op = ComponentOp::Set {
            discriminant: 1,
            data: ComponentData::Inline(bytes::Bytes::from(vec![1, 2, 3])),
            vector_clock: VectorClock::new(),
        };

        assert_eq!(op.discriminant(), Some(1));
    }

    #[test]
    fn test_discriminant_delete() {
        let op = ComponentOp::Delete {
            vector_clock: VectorClock::new(),
        };

        assert_eq!(op.discriminant(), None);
    }

    #[test]
    fn test_is_set() {
        let op = ComponentOp::Set {
            discriminant: 1,
            data: ComponentData::Inline(bytes::Bytes::from(vec![1, 2, 3])),
            vector_clock: VectorClock::new(),
        };

        assert!(op.is_set());
        assert!(!op.is_or_set());
        assert!(!op.is_sequence());
        assert!(!op.is_delete());
    }

    #[test]
    fn test_is_or_set() {
        let op = ComponentOp::SetAdd {
            discriminant: 2,
            operation_id: uuid::Uuid::new_v4(),
            element: bytes::Bytes::from(vec![1, 2, 3]),
            vector_clock: VectorClock::new(),
        };

        assert!(!op.is_set());
        assert!(op.is_or_set());
        assert!(!op.is_sequence());
        assert!(!op.is_delete());
    }

    #[test]
    fn test_is_sequence() {
        let op = ComponentOp::SequenceInsert {
            discriminant: 3,
            operation_id: uuid::Uuid::new_v4(),
            after_id: None,
            element: bytes::Bytes::from(vec![1, 2, 3]),
            vector_clock: VectorClock::new(),
        };

        assert!(!op.is_set());
        assert!(!op.is_or_set());
        assert!(op.is_sequence());
        assert!(!op.is_delete());
    }

    #[test]
    fn test_builder_set() {
        let node_id = uuid::Uuid::new_v4();
        let clock = VectorClock::new();

        let builder = ComponentOpBuilder::new(node_id, clock);
        let op = builder.set(
            1,
            ComponentData::Inline(bytes::Bytes::from(vec![1, 2, 3])),
        );

        assert!(op.is_set());
        assert_eq!(op.vector_clock().get(node_id), 1);
    }

    #[test]
    fn test_builder_set_add() {
        let node_id = uuid::Uuid::new_v4();
        let clock = VectorClock::new();

        let builder = ComponentOpBuilder::new(node_id, clock);
        let op = builder.set_add(2, bytes::Bytes::from(vec![1, 2, 3]));

        assert!(op.is_or_set());
        assert_eq!(op.vector_clock().get(node_id), 1);
    }

    #[test]
    fn test_serialization() -> anyhow::Result<()> {
        let op = ComponentOp::Set {
            discriminant: 1,
            data: ComponentData::Inline(bytes::Bytes::from(vec![1, 2, 3])),
            vector_clock: VectorClock::new(),
        };

        let bytes = rkyv::to_bytes::<rkyv::rancor::Failure>(&op).map(|b| b.to_vec())?;
        let deserialized: ComponentOp = rkyv::from_bytes::<ComponentOp, rkyv::rancor::Failure>(&bytes)?;

        assert!(deserialized.is_set());

        Ok(())
    }
}
