//! OR-Set (Observed-Remove Set) CRDT implementation
//!
//! This module provides a conflict-free replicated set that supports concurrent
//! add and remove operations with "add-wins" semantics.
//!
//! ## OR-Set Semantics
//!
//! - **Add-wins**: If an element is concurrently added and removed, the add
//!   wins
//! - **Observed-remove**: Removes only affect adds that have been observed
//!   (happened-before)
//! - **Unique operation IDs**: Each add generates a unique ID to track
//!   add/remove pairs
//!
//! ## Example
//!
//! ```
//! use libmarathon::networking::{
//!     OrElement,
//!     OrSet,
//! };
//! use uuid::Uuid;
//!
//! let node1 = Uuid::new_v4();
//! let node2 = Uuid::new_v4();
//!
//! // Node 1 adds "foo"
//! let mut set1: OrSet<String> = OrSet::new();
//! let (add_id, _) = set1.add("foo".to_string(), node1);
//!
//! // Node 2 concurrently adds "bar"
//! let mut set2: OrSet<String> = OrSet::new();
//! set2.add("bar".to_string(), node2);
//!
//! // Node 1 removes "foo" (observes own add)
//! set1.remove(vec![add_id]);
//!
//! // Merge sets - "bar" should be present, "foo" should be removed
//! set1.merge(&set2);
//! assert_eq!(set1.len(), 1);
//! assert!(set1.contains(&"bar".to_string()));
//! assert!(!set1.contains(&"foo".to_string()));
//! ```

use std::collections::{
    HashMap,
    HashSet,
};

use bevy::prelude::*;
use serde::{
    Deserialize,
    Serialize,
};

use crate::networking::vector_clock::NodeId;

/// An element in an OR-Set with its unique operation ID
///
/// Each add operation generates a unique ID. The same logical element can have
/// multiple IDs if it's added multiple times (e.g., removed then re-added).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrElement<T> {
    /// The actual element value
    pub value: T,

    /// Unique ID for this add operation
    pub operation_id: uuid::Uuid,

    /// Node that performed the add
    pub adding_node: NodeId,
}

/// OR-Set (Observed-Remove Set) CRDT
///
/// A replicated set supporting concurrent add/remove with add-wins semantics.
/// This is based on the "Optimized Observed-Remove Set" algorithm.
///
/// # Type Parameters
///
/// - `T`: The element type (must be Clone, Eq, Hash, Serialize, Deserialize)
///
/// # Internal Structure
///
/// - `elements`: Map from operation_id → (value, adding_node)
/// - `tombstones`: Set of removed operation IDs
///
/// An element is "present" if it has an operation ID in `elements` that's
/// not in `tombstones`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrSet<T> {
    /// Map from operation ID to (value, adding_node)
    elements: HashMap<uuid::Uuid, (T, NodeId)>,

    /// Set of removed operation IDs
    tombstones: HashSet<uuid::Uuid>,
}

impl<T> OrSet<T>
where
    T: Clone + Eq + std::hash::Hash + Serialize + for<'de> Deserialize<'de>,
{
    /// Create a new empty OR-Set
    pub fn new() -> Self {
        Self {
            elements: HashMap::new(),
            tombstones: HashSet::new(),
        }
    }

    /// Add an element to the set
    ///
    /// Returns (operation_id, was_new) where was_new indicates if this value
    /// wasn't already present.
    ///
    /// # Example
    ///
    /// ```
    /// use libmarathon::networking::OrSet;
    /// use uuid::Uuid;
    ///
    /// let node = Uuid::new_v4();
    /// let mut set: OrSet<String> = OrSet::new();
    ///
    /// let (id, was_new) = set.add("foo".to_string(), node);
    /// assert!(was_new);
    /// assert!(set.contains(&"foo".to_string()));
    /// ```
    pub fn add(&mut self, value: T, node_id: NodeId) -> (uuid::Uuid, bool) {
        let operation_id = uuid::Uuid::new_v4();
        let was_new = !self.contains(&value);

        self.elements.insert(operation_id, (value, node_id));

        (operation_id, was_new)
    }

    /// Remove elements by their operation IDs
    ///
    /// This implements observed-remove semantics: only the specific add
    /// operations identified by these IDs are removed.
    ///
    /// # Example
    ///
    /// ```
    /// use libmarathon::networking::OrSet;
    /// use uuid::Uuid;
    ///
    /// let node = Uuid::new_v4();
    /// let mut set: OrSet<String> = OrSet::new();
    ///
    /// let (id, _) = set.add("foo".to_string(), node);
    /// assert!(set.contains(&"foo".to_string()));
    ///
    /// set.remove(vec![id]);
    /// assert!(!set.contains(&"foo".to_string()));
    /// ```
    pub fn remove(&mut self, operation_ids: Vec<uuid::Uuid>) {
        for id in operation_ids {
            self.tombstones.insert(id);
        }
    }

    /// Check if a value is present in the set
    ///
    /// A value is present if it has at least one operation ID that's not
    /// tombstoned.
    pub fn contains(&self, value: &T) -> bool {
        self.elements
            .iter()
            .any(|(id, (v, _))| v == value && !self.tombstones.contains(id))
    }

    /// Get all present values
    ///
    /// Returns an iterator over values that are currently in the set
    /// (not tombstoned).
    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.elements
            .iter()
            .filter(|(id, _)| !self.tombstones.contains(id))
            .map(|(_, (value, _))| value)
    }

    /// Get all operation IDs for a specific value
    ///
    /// This is used when removing a value - we need to tombstone all its
    /// operation IDs.
    ///
    /// # Example
    ///
    /// ```
    /// use libmarathon::networking::OrSet;
    /// use uuid::Uuid;
    ///
    /// let node = Uuid::new_v4();
    /// let mut set: OrSet<String> = OrSet::new();
    ///
    /// set.add("foo".to_string(), node);
    /// set.add("foo".to_string(), node); // Add same value again
    ///
    /// let ids = set.get_operation_ids(&"foo".to_string());
    /// assert_eq!(ids.len(), 2); // Two operation IDs for "foo"
    /// ```
    pub fn get_operation_ids(&self, value: &T) -> Vec<uuid::Uuid> {
        self.elements
            .iter()
            .filter(|(id, (v, _))| v == value && !self.tombstones.contains(id))
            .map(|(id, _)| *id)
            .collect()
    }

    /// Get the number of distinct values in the set
    pub fn len(&self) -> usize {
        let mut seen = HashSet::new();
        self.elements
            .iter()
            .filter(|(id, (value, _))| !self.tombstones.contains(id) && seen.insert(value))
            .count()
    }

    /// Check if the set is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Merge another OR-Set into this one
    ///
    /// This implements the CRDT merge operation:
    /// - Union all elements
    /// - Union all tombstones
    /// - Add-wins: elements not in tombstones are present
    ///
    /// # Example
    ///
    /// ```
    /// use libmarathon::networking::OrSet;
    /// use uuid::Uuid;
    ///
    /// let node1 = Uuid::new_v4();
    /// let node2 = Uuid::new_v4();
    ///
    /// let mut set1: OrSet<String> = OrSet::new();
    /// set1.add("foo".to_string(), node1);
    ///
    /// let mut set2: OrSet<String> = OrSet::new();
    /// set2.add("bar".to_string(), node2);
    ///
    /// set1.merge(&set2);
    /// assert_eq!(set1.len(), 2);
    /// assert!(set1.contains(&"foo".to_string()));
    /// assert!(set1.contains(&"bar".to_string()));
    /// ```
    pub fn merge(&mut self, other: &OrSet<T>) {
        // Union elements
        for (id, (value, node)) in &other.elements {
            self.elements
                .entry(*id)
                .or_insert_with(|| (value.clone(), *node));
        }

        // Union tombstones
        for id in &other.tombstones {
            self.tombstones.insert(*id);
        }
    }

    /// Clear the set
    ///
    /// Removes all elements and tombstones.
    pub fn clear(&mut self) {
        self.elements.clear();
        self.tombstones.clear();
    }

    /// Garbage collect tombstoned elements
    ///
    /// Removes elements that are tombstoned to save memory. This is safe
    /// because once an operation is tombstoned, it stays tombstoned.
    ///
    /// This should be called periodically to prevent unbounded growth.
    pub fn garbage_collect(&mut self) {
        self.elements.retain(|id, _| !self.tombstones.contains(id));
    }
}

impl<T> Default for OrSet<T>
where
    T: Clone + Eq + std::hash::Hash + Serialize + for<'de> Deserialize<'de>,
{
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orset_new() {
        let set: OrSet<String> = OrSet::new();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn test_orset_add() {
        let node = uuid::Uuid::new_v4();
        let mut set: OrSet<String> = OrSet::new();

        let (_, was_new) = set.add("foo".to_string(), node);
        assert!(was_new);
        assert!(set.contains(&"foo".to_string()));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_orset_add_duplicate() {
        let node = uuid::Uuid::new_v4();
        let mut set: OrSet<String> = OrSet::new();

        let (id1, was_new1) = set.add("foo".to_string(), node);
        assert!(was_new1);

        let (id2, was_new2) = set.add("foo".to_string(), node);
        assert!(!was_new2);
        assert_ne!(id1, id2); // Different operation IDs

        assert_eq!(set.len(), 1); // Still one distinct value
        let ids = set.get_operation_ids(&"foo".to_string());
        assert_eq!(ids.len(), 2); // But two operation IDs
    }

    #[test]
    fn test_orset_remove() {
        let node = uuid::Uuid::new_v4();
        let mut set: OrSet<String> = OrSet::new();

        let (id, _) = set.add("foo".to_string(), node);
        assert!(set.contains(&"foo".to_string()));

        set.remove(vec![id]);
        assert!(!set.contains(&"foo".to_string()));
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn test_orset_add_remove_add() {
        let node = uuid::Uuid::new_v4();
        let mut set: OrSet<String> = OrSet::new();

        // Add
        let (id1, _) = set.add("foo".to_string(), node);
        assert!(set.contains(&"foo".to_string()));

        // Remove
        set.remove(vec![id1]);
        assert!(!set.contains(&"foo".to_string()));

        // Add again (new operation ID)
        let (_id2, was_new) = set.add("foo".to_string(), node);
        assert!(was_new); // It's new because we removed it
        assert!(set.contains(&"foo".to_string()));
    }

    #[test]
    fn test_orset_merge_simple() {
        let node1 = uuid::Uuid::new_v4();
        let node2 = uuid::Uuid::new_v4();

        let mut set1: OrSet<String> = OrSet::new();
        set1.add("foo".to_string(), node1);

        let mut set2: OrSet<String> = OrSet::new();
        set2.add("bar".to_string(), node2);

        set1.merge(&set2);

        assert_eq!(set1.len(), 2);
        assert!(set1.contains(&"foo".to_string()));
        assert!(set1.contains(&"bar".to_string()));
    }

    #[test]
    fn test_orset_merge_add_wins() {
        let node1 = uuid::Uuid::new_v4();
        let node2 = uuid::Uuid::new_v4();

        let mut set1: OrSet<String> = OrSet::new();
        let (id, _) = set1.add("foo".to_string(), node1);
        set1.remove(vec![id]); // Remove it

        let mut set2: OrSet<String> = OrSet::new();
        set2.add("foo".to_string(), node2); // Concurrently add (different ID)

        set1.merge(&set2);

        // Add should win
        assert!(set1.contains(&"foo".to_string()));
    }

    #[test]
    fn test_orset_merge_observed_remove() {
        let node1 = uuid::Uuid::new_v4();

        let mut set1: OrSet<String> = OrSet::new();
        let (id, _) = set1.add("foo".to_string(), node1);

        let mut set2 = set1.clone(); // set2 observes the add

        set2.remove(vec![id]); // set2 removes after observing

        set1.merge(&set2);

        // Remove should win because it observed the add
        assert!(!set1.contains(&"foo".to_string()));
    }

    #[test]
    fn test_orset_values() {
        let node = uuid::Uuid::new_v4();
        let mut set: OrSet<String> = OrSet::new();

        set.add("foo".to_string(), node);
        set.add("bar".to_string(), node);
        set.add("baz".to_string(), node);

        let values: HashSet<_> = set.values().cloned().collect();
        assert_eq!(values.len(), 3);
        assert!(values.contains("foo"));
        assert!(values.contains("bar"));
        assert!(values.contains("baz"));
    }

    #[test]
    fn test_orset_garbage_collect() {
        let node = uuid::Uuid::new_v4();
        let mut set: OrSet<String> = OrSet::new();

        let (id1, _) = set.add("foo".to_string(), node);
        let (_id2, _) = set.add("bar".to_string(), node);

        set.remove(vec![id1]);

        // Before GC
        assert_eq!(set.elements.len(), 2);
        assert_eq!(set.tombstones.len(), 1);

        set.garbage_collect();

        // After GC - tombstoned element removed
        assert_eq!(set.elements.len(), 1);
        assert_eq!(set.tombstones.len(), 1);
        assert!(set.contains(&"bar".to_string()));
        assert!(!set.contains(&"foo".to_string()));
    }

    #[test]
    fn test_orset_clear() {
        let node = uuid::Uuid::new_v4();
        let mut set: OrSet<String> = OrSet::new();

        set.add("foo".to_string(), node);
        set.add("bar".to_string(), node);
        assert_eq!(set.len(), 2);

        set.clear();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn test_orset_serialization() -> bincode::Result<()> {
        let node = uuid::Uuid::new_v4();
        let mut set: OrSet<String> = OrSet::new();

        set.add("foo".to_string(), node);
        set.add("bar".to_string(), node);

        let bytes = bincode::serialize(&set)?;
        let deserialized: OrSet<String> = bincode::deserialize(&bytes)?;

        assert_eq!(deserialized.len(), 2);
        assert!(deserialized.contains(&"foo".to_string()));
        assert!(deserialized.contains(&"bar".to_string()));

        Ok(())
    }
}
