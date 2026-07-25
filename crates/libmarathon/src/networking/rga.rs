//! RGA (Replicated Growable Array) CRDT implementation
//!
//! This module provides a conflict-free replicated sequence that maintains
//! consistent ordering across concurrent insert and delete operations.
//!
//! ## RGA Semantics
//!
//! - **Causal ordering**: Elements inserted after position P stay after P
//! - **Concurrent inserts**: Resolved by timestamp + node ID tiebreaker
//! - **Tombstones**: Deleted elements remain in structure to preserve positions
//! - **Unique operation IDs**: Each insert gets a UUID for referencing
//!
//! ## Example
//!
//! ```
//! use libmarathon::networking::Rga;
//! use uuid::Uuid;
//!
//! let node1 = Uuid::new_v4();
//! let node2 = Uuid::new_v4();
//!
//! // Node 1 creates sequence: [A, B]
//! let mut seq1: Rga<char> = Rga::new();
//! let (id_a, _) = seq1.insert_at_beginning('A', node1);
//! let (id_b, _) = seq1.insert_after(Some(id_a), 'B', node1);
//!
//! // Node 2 concurrently inserts C after A
//! let mut seq2 = seq1.clone();
//! seq2.insert_after(Some(id_a), 'C', node2);
//!
//! // Node 1 inserts D after A
//! seq1.insert_after(Some(id_a), 'D', node1);
//!
//! // Merge - concurrent inserts after A are ordered by timestamp + node ID
//! seq1.merge(&seq2);
//!
//! let values: Vec<char> = seq1.values().copied().collect();
//! assert_eq!(values.len(), 4); // A, (C or D), (D or C), B
//! ```

use std::collections::HashMap;

use bevy::prelude::*;

use crate::networking::vector_clock::{
    NodeId,
    VectorClock,
};

/// An element in an RGA sequence
///
/// Each element has a unique ID and tracks its logical position in the sequence
/// via the "after" pointer.
#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct RgaElement<T> {
    /// Unique ID for this element
    pub id: uuid::Uuid,

    /// The actual value
    pub value: T,

    /// ID of the element this was inserted after (None = beginning)
    pub after_id: Option<uuid::Uuid>,

    /// Node that performed the insert
    pub inserting_node: NodeId,

    /// Vector clock when inserted (for ordering concurrent inserts)
    pub vector_clock: VectorClock,

    /// Whether this element has been deleted (tombstone)
    pub is_deleted: bool,
}

/// RGA (Replicated Growable Array) CRDT
///
/// A replicated sequence supporting concurrent insert/delete with consistent
/// ordering based on causal relationships.
///
/// # Type Parameters
///
/// - `T`: The element type (must be Clone, Serialize, Deserialize)
///
/// # Internal Structure
///
/// Elements are stored in a HashMap by ID. Each element tracks which element
/// it was inserted after, forming a linked list structure. Deleted elements
/// remain as tombstones to preserve positions for concurrent operations.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct Rga<T> {
    /// Map from element ID to element
    elements: HashMap<uuid::Uuid, RgaElement<T>>,
}

impl<T> Rga<T>
where
    T: Clone + rkyv::Archive,
{
    /// Create a new empty RGA sequence
    pub fn new() -> Self {
        Self {
            elements: HashMap::new(),
        }
    }

    /// Insert an element at the beginning of the sequence
    ///
    /// Returns (element_id, position) where position is the index in the
    /// visible sequence.
    ///
    /// # Example
    ///
    /// ```
    /// use libmarathon::networking::Rga;
    /// use uuid::Uuid;
    ///
    /// let node = Uuid::new_v4();
    /// let mut seq: Rga<char> = Rga::new();
    ///
    /// let (id, pos) = seq.insert_at_beginning('A', node);
    /// assert_eq!(pos, 0);
    /// ```
    pub fn insert_at_beginning(&mut self, value: T, node_id: NodeId) -> (uuid::Uuid, usize) {
        let id = uuid::Uuid::new_v4();
        let mut clock = VectorClock::new();
        clock.increment(node_id);

        let element = RgaElement {
            id,
            value,
            after_id: None,
            inserting_node: node_id,
            vector_clock: clock,
            is_deleted: false,
        };

        self.elements.insert(id, element);

        (id, 0)
    }

    /// Insert an element after a specific element ID
    ///
    /// If after_id is None, inserts at the beginning.
    ///
    /// Returns (element_id, position) where position is the index in the
    /// visible sequence.
    ///
    /// # Example
    ///
    /// ```
    /// use libmarathon::networking::Rga;
    /// use uuid::Uuid;
    ///
    /// let node = Uuid::new_v4();
    /// let mut seq: Rga<char> = Rga::new();
    ///
    /// let (id_a, _) = seq.insert_at_beginning('A', node);
    /// let (id_b, pos) = seq.insert_after(Some(id_a), 'B', node);
    /// assert_eq!(pos, 1);
    ///
    /// let values: Vec<char> = seq.values().copied().collect();
    /// assert_eq!(values, vec!['A', 'B']);
    /// ```
    pub fn insert_after(
        &mut self,
        after_id: Option<uuid::Uuid>,
        value: T,
        node_id: NodeId,
    ) -> (uuid::Uuid, usize) {
        let id = uuid::Uuid::new_v4();
        let mut clock = VectorClock::new();
        clock.increment(node_id);

        let element = RgaElement {
            id,
            value,
            after_id,
            inserting_node: node_id,
            vector_clock: clock,
            is_deleted: false,
        };

        self.elements.insert(id, element);

        // Calculate position
        let position = self.calculate_position(id);

        (id, position)
    }

    /// Insert an element with explicit vector clock
    ///
    /// This is used when applying remote operations that already have
    /// a vector clock.
    pub fn insert_with_clock(
        &mut self,
        id: uuid::Uuid,
        after_id: Option<uuid::Uuid>,
        value: T,
        node_id: NodeId,
        vector_clock: VectorClock,
    ) -> usize {
        let element = RgaElement {
            id,
            value,
            after_id,
            inserting_node: node_id,
            vector_clock,
            is_deleted: false,
        };

        self.elements.insert(id, element);

        self.calculate_position(id)
    }

    /// Delete an element by ID
    ///
    /// The element becomes a tombstone - it remains in the structure but
    /// is hidden from the visible sequence.
    ///
    /// # Example
    ///
    /// ```
    /// use libmarathon::networking::Rga;
    /// use uuid::Uuid;
    ///
    /// let node = Uuid::new_v4();
    /// let mut seq: Rga<char> = Rga::new();
    ///
    /// let (id, _) = seq.insert_at_beginning('A', node);
    /// assert_eq!(seq.len(), 1);
    ///
    /// seq.delete(id);
    /// assert_eq!(seq.len(), 0);
    /// assert!(seq.is_deleted(id));
    /// ```
    pub fn delete(&mut self, element_id: uuid::Uuid) {
        if let Some(element) = self.elements.get_mut(&element_id) {
            element.is_deleted = true;
        }
    }

    /// Check if an element is deleted
    pub fn is_deleted(&self, element_id: uuid::Uuid) -> bool {
        self.elements
            .get(&element_id)
            .map(|e| e.is_deleted)
            .unwrap_or(false)
    }

    /// Get the visible length of the sequence (excluding tombstones)
    pub fn len(&self) -> usize {
        self.elements.values().filter(|e| !e.is_deleted).count()
    }

    /// Check if the sequence is empty (no visible elements)
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get all visible values in order
    ///
    /// Returns an iterator over the values in their proper sequence order.
    pub fn values(&self) -> impl Iterator<Item = &T> {
        let ordered = self.get_ordered_elements();
        ordered.into_iter().filter_map(move |id| {
            self.elements
                .get(&id)
                .and_then(|e| if !e.is_deleted { Some(&e.value) } else { None })
        })
    }

    /// Get all visible elements with their IDs in order
    pub fn elements_with_ids(&self) -> Vec<(uuid::Uuid, &T)> {
        let ordered = self.get_ordered_elements();
        ordered
            .into_iter()
            .filter_map(|id| {
                self.elements.get(&id).and_then(|e| {
                    if !e.is_deleted {
                        Some((id, &e.value))
                    } else {
                        None
                    }
                })
            })
            .collect()
    }

    /// Merge another RGA into this one
    ///
    /// Implements CRDT merge by combining all elements from both sequences
    /// and resolving positions based on causal ordering.
    ///
    /// # Example
    ///
    /// ```
    /// use libmarathon::networking::Rga;
    /// use uuid::Uuid;
    ///
    /// let node1 = Uuid::new_v4();
    /// let node2 = Uuid::new_v4();
    ///
    /// let mut seq1: Rga<char> = Rga::new();
    /// seq1.insert_at_beginning('A', node1);
    ///
    /// let mut seq2: Rga<char> = Rga::new();
    /// seq2.insert_at_beginning('B', node2);
    ///
    /// seq1.merge(&seq2);
    /// assert_eq!(seq1.len(), 2);
    /// ```
    pub fn merge(&mut self, other: &Rga<T>) {
        for (id, element) in &other.elements {
            // Insert or update element
            self.elements
                .entry(*id)
                .and_modify(|existing| {
                    // If other's element is deleted, mark ours as deleted too
                    if element.is_deleted {
                        existing.is_deleted = true;
                    }
                })
                .or_insert_with(|| element.clone());
        }
    }

    /// Clear the sequence
    ///
    /// Removes all elements and tombstones.
    pub fn clear(&mut self) {
        self.elements.clear();
    }

    /// Garbage collect tombstones
    ///
    /// Removes deleted elements that have no children (nothing inserted after
    /// them). This is safe because if no element references a tombstone as
    /// its parent, it can be removed without affecting the sequence.
    pub fn garbage_collect(&mut self) {
        // Find all IDs that are referenced as after_id
        let mut referenced_ids = std::collections::HashSet::new();
        for element in self.elements.values() {
            if let Some(after_id) = element.after_id {
                referenced_ids.insert(after_id);
            }
        }

        // Remove deleted elements that aren't referenced
        self.elements
            .retain(|id, element| !element.is_deleted || referenced_ids.contains(id));
    }

    /// Get ordered list of element IDs
    ///
    /// This builds the proper sequence order by following the after_id pointers
    /// and resolving concurrent inserts using vector clocks + node IDs.
    fn get_ordered_elements(&self) -> Vec<uuid::Uuid> {
        // Build a map of after_id -> list of elements inserted after it
        let mut children: HashMap<Option<uuid::Uuid>, Vec<uuid::Uuid>> = HashMap::new();

        for (id, element) in &self.elements {
            children
                .entry(element.after_id)
                .or_insert_with(Vec::new)
                .push(*id);
        }

        // Sort children by vector clock, then node ID (for deterministic ordering)
        for child_list in children.values_mut() {
            child_list.sort_by(|a, b| {
                let elem_a = &self.elements[a];
                let elem_b = &self.elements[b];

                // Compare vector clocks
                match elem_a.vector_clock.compare(&elem_b.vector_clock) {
                    | Ok(std::cmp::Ordering::Less) => std::cmp::Ordering::Less,
                    | Ok(std::cmp::Ordering::Greater) => std::cmp::Ordering::Greater,
                    | Ok(std::cmp::Ordering::Equal) | Err(_) => {
                        // If clocks are equal or concurrent, use node ID as tiebreaker
                        elem_a.inserting_node.cmp(&elem_b.inserting_node)
                    },
                }
            });
        }

        // Build ordered list by traversing from None (beginning)
        let mut result = Vec::new();
        let mut to_visit = vec![None];

        while let Some(current_id) = to_visit.pop() {
            if let Some(child_ids) = children.get(&current_id) {
                // Visit children in reverse order (since we're using a stack)
                for child_id in child_ids.iter().rev() {
                    result.push(*child_id);
                    to_visit.push(Some(*child_id));
                }
            }
        }

        result
    }

    /// Calculate the visible position of an element
    fn calculate_position(&self, element_id: uuid::Uuid) -> usize {
        let ordered = self.get_ordered_elements();
        ordered.iter().position(|id| id == &element_id).unwrap_or(0)
    }
}

impl<T> Default for Rga<T>
where
    T: Clone + rkyv::Archive,
{
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rga_new() {
        let seq: Rga<char> = Rga::new();
        assert!(seq.is_empty());
        assert_eq!(seq.len(), 0);
    }

    #[test]
    fn test_rga_insert_at_beginning() {
        let node = uuid::Uuid::new_v4();
        let mut seq: Rga<char> = Rga::new();

        let (_, pos) = seq.insert_at_beginning('A', node);
        assert_eq!(pos, 0);
        assert_eq!(seq.len(), 1);

        let values: Vec<char> = seq.values().copied().collect();
        assert_eq!(values, vec!['A']);
    }

    #[test]
    fn test_rga_insert_after() {
        let node = uuid::Uuid::new_v4();
        let mut seq: Rga<char> = Rga::new();

        let (id_a, _) = seq.insert_at_beginning('A', node);
        let (_, pos_b) = seq.insert_after(Some(id_a), 'B', node);
        assert_eq!(pos_b, 1);

        let values: Vec<char> = seq.values().copied().collect();
        assert_eq!(values, vec!['A', 'B']);
    }

    #[test]
    fn test_rga_delete() {
        let node = uuid::Uuid::new_v4();
        let mut seq: Rga<char> = Rga::new();

        let (id_a, _) = seq.insert_at_beginning('A', node);
        let (id_b, _) = seq.insert_after(Some(id_a), 'B', node);

        assert_eq!(seq.len(), 2);

        seq.delete(id_a);
        assert_eq!(seq.len(), 1);
        assert!(seq.is_deleted(id_a));

        let values: Vec<char> = seq.values().copied().collect();
        assert_eq!(values, vec!['B']);
    }

    #[test]
    fn test_rga_insert_delete_insert() {
        let node = uuid::Uuid::new_v4();
        let mut seq: Rga<char> = Rga::new();

        let (id_a, _) = seq.insert_at_beginning('A', node);
        seq.delete(id_a);
        assert_eq!(seq.len(), 0);

        seq.insert_at_beginning('B', node);
        assert_eq!(seq.len(), 1);

        let values: Vec<char> = seq.values().copied().collect();
        assert_eq!(values, vec!['B']);
    }

    #[test]
    fn test_rga_merge_simple() {
        let node1 = uuid::Uuid::new_v4();
        let node2 = uuid::Uuid::new_v4();

        let mut seq1: Rga<char> = Rga::new();
        seq1.insert_at_beginning('A', node1);

        let mut seq2: Rga<char> = Rga::new();
        seq2.insert_at_beginning('B', node2);

        seq1.merge(&seq2);
        assert_eq!(seq1.len(), 2);
    }

    #[test]
    fn test_rga_merge_preserves_order() {
        let node = uuid::Uuid::new_v4();

        let mut seq1: Rga<char> = Rga::new();
        let (id_a, _) = seq1.insert_at_beginning('A', node);
        let (id_b, _) = seq1.insert_after(Some(id_a), 'B', node);
        seq1.insert_after(Some(id_b), 'C', node);

        let seq2 = seq1.clone();

        seq1.merge(&seq2);

        let values: Vec<char> = seq1.values().copied().collect();
        assert_eq!(values, vec!['A', 'B', 'C']);
    }

    #[test]
    fn test_rga_merge_deletion() {
        let node = uuid::Uuid::new_v4();

        let mut seq1: Rga<char> = Rga::new();
        let (id_a, _) = seq1.insert_at_beginning('A', node);
        seq1.insert_after(Some(id_a), 'B', node);

        let mut seq2 = seq1.clone();
        seq2.delete(id_a);

        seq1.merge(&seq2);

        let values: Vec<char> = seq1.values().copied().collect();
        assert_eq!(values, vec!['B']);
    }

    #[test]
    fn test_rga_concurrent_inserts() {
        let node1 = uuid::Uuid::new_v4();
        let node2 = uuid::Uuid::new_v4();

        // Both start with [A]
        let mut seq1: Rga<char> = Rga::new();
        let (id_a, _) = seq1.insert_at_beginning('A', node1);

        let mut seq2 = seq1.clone();

        // seq1 inserts B after A
        seq1.insert_after(Some(id_a), 'B', node1);

        // seq2 inserts C after A (concurrent)
        seq2.insert_after(Some(id_a), 'C', node2);

        // Merge
        seq1.merge(&seq2);

        // Should have A followed by B and C in some deterministic order
        assert_eq!(seq1.len(), 3);

        let values: Vec<char> = seq1.values().copied().collect();
        assert_eq!(values[0], 'A');
        assert!(values.contains(&'B'));
        assert!(values.contains(&'C'));
    }

    #[test]
    fn test_rga_clear() {
        let node = uuid::Uuid::new_v4();
        let mut seq: Rga<char> = Rga::new();

        seq.insert_at_beginning('A', node);
        seq.insert_at_beginning('B', node);
        assert_eq!(seq.len(), 2);

        seq.clear();
        assert!(seq.is_empty());
    }

    #[test]
    fn test_rga_garbage_collect() {
        let node = uuid::Uuid::new_v4();
        let mut seq: Rga<char> = Rga::new();

        let (id_a, _) = seq.insert_at_beginning('A', node);
        let (id_b, _) = seq.insert_after(Some(id_a), 'B', node);
        let (_, _) = seq.insert_after(Some(id_b), 'C', node);

        // Delete A (has child B, so should be kept)
        seq.delete(id_a);

        // Delete B (has child C, so should be kept)
        seq.delete(id_b);

        assert_eq!(seq.elements.len(), 3);

        seq.garbage_collect();

        // A and B should still be there (referenced by children)
        // Only C is visible
        assert_eq!(seq.len(), 1);
        assert!(seq.elements.contains_key(&id_a));
        assert!(seq.elements.contains_key(&id_b));
    }

    #[test]
    fn test_rga_serialization() -> anyhow::Result<()> {
        let node = uuid::Uuid::new_v4();
        let mut seq: Rga<String> = Rga::new();

        let (id_a, _) = seq.insert_at_beginning("foo".to_string(), node);
        seq.insert_after(Some(id_a), "bar".to_string(), node);

        let bytes = rkyv::to_bytes::<rkyv::rancor::Failure>(&seq).map(|b| b.to_vec())?;
        let deserialized: Rga<String> =
            rkyv::from_bytes::<Rga<String>, rkyv::rancor::Failure>(&bytes)?;

        assert_eq!(deserialized.len(), 2);
        let values: Vec<String> = deserialized.values().cloned().collect();
        assert_eq!(values, vec!["foo".to_string(), "bar".to_string()]);

        Ok(())
    }
}
