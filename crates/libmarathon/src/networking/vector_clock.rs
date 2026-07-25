//! Vector clock implementation for distributed causality tracking
//!
//! Vector clocks allow us to determine the causal relationship between events
//! in a distributed system. This is critical for CRDT merge semantics.

use std::collections::HashMap;

use crate::networking::error::{
    NetworkingError,
    Result,
};

/// Unique identifier for a node in the distributed system
pub type NodeId = uuid::Uuid;

/// Vector clock for tracking causality in distributed operations
///
/// A vector clock is a map from node IDs to logical timestamps (sequence
/// numbers). Each node maintains its own vector clock and increments its own
/// counter for each local operation.
///
/// # Causal Relationships
///
/// Given two vector clocks A and B:
/// - **A happened-before B** if all of A's counters ≤ B's counters and at least
///   one is <
/// - **A and B are concurrent** if neither happened-before the other
/// - **A and B are identical** if all counters are equal
///
/// # Example
///
/// ```
/// use libmarathon::networking::VectorClock;
/// use uuid::Uuid;
///
/// let node1 = Uuid::new_v4();
/// let node2 = Uuid::new_v4();
///
/// let mut clock1 = VectorClock::new();
/// clock1.increment(node1); // node1: 1
///
/// let mut clock2 = VectorClock::new();
/// clock2.increment(node2); // node2: 1
///
/// // These are concurrent - neither happened before the other
/// assert!(clock1.is_concurrent_with(&clock2));
///
/// // Merge the clocks
/// clock1.merge(&clock2); // node1: 1, node2: 1
/// assert!(clock1.happened_before(&clock2) == false);
/// ```
#[derive(
    Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Default,
)]
pub struct VectorClock {
    /// Map from node ID to logical timestamp
    pub timestamps: HashMap<NodeId, u64>,
}

impl VectorClock {
    /// Create a new empty vector clock
    pub fn new() -> Self {
        Self {
            timestamps: HashMap::new(),
        }
    }

    /// Get the number of nodes tracked in this clock
    pub fn node_count(&self) -> usize {
        self.timestamps.len()
    }

    /// Increment the clock for a given node
    ///
    /// This should be called by a node before performing a local operation.
    /// It increments that node's counter in the vector clock.
    ///
    /// # Example
    ///
    /// ```
    /// use libmarathon::networking::VectorClock;
    /// use uuid::Uuid;
    ///
    /// let node = Uuid::new_v4();
    /// let mut clock = VectorClock::new();
    ///
    /// clock.increment(node);
    /// assert_eq!(clock.get(node), 1);
    ///
    /// clock.increment(node);
    /// assert_eq!(clock.get(node), 2);
    /// ```
    pub fn increment(&mut self, node_id: NodeId) -> u64 {
        let counter = self.timestamps.entry(node_id).or_insert(0);
        *counter += 1;
        *counter
    }

    /// Get the current counter value for a node
    ///
    /// Returns 0 if the node has never been seen in this vector clock.
    pub fn get(&self, node_id: NodeId) -> u64 {
        self.timestamps.get(&node_id).copied().unwrap_or(0)
    }

    /// Merge another vector clock into this one
    ///
    /// Takes the maximum counter value for each node. This is used when
    /// receiving a message to update our knowledge of remote operations.
    ///
    /// # Example
    ///
    /// ```
    /// use libmarathon::networking::VectorClock;
    /// use uuid::Uuid;
    ///
    /// let node1 = Uuid::new_v4();
    /// let node2 = Uuid::new_v4();
    ///
    /// let mut clock1 = VectorClock::new();
    /// clock1.increment(node1); // node1: 1
    /// clock1.increment(node1); // node1: 2
    ///
    /// let mut clock2 = VectorClock::new();
    /// clock2.increment(node2); // node2: 1
    ///
    /// clock1.merge(&clock2);
    /// assert_eq!(clock1.get(node1), 2);
    /// assert_eq!(clock1.get(node2), 1);
    /// ```
    pub fn merge(&mut self, other: &VectorClock) {
        for (node_id, &counter) in &other.timestamps {
            let current = self.timestamps.entry(*node_id).or_insert(0);
            *current = (*current).max(counter);
        }
    }

    /// Check if this vector clock happened-before another
    ///
    /// Returns true if all of our counters are ≤ the other's counters,
    /// and at least one is strictly less.
    ///
    /// # Example
    ///
    /// ```
    /// use libmarathon::networking::VectorClock;
    /// use uuid::Uuid;
    ///
    /// let node = Uuid::new_v4();
    ///
    /// let mut clock1 = VectorClock::new();
    /// clock1.increment(node); // node: 1
    ///
    /// let mut clock2 = VectorClock::new();
    /// clock2.increment(node); // node: 1
    /// clock2.increment(node); // node: 2
    ///
    /// assert!(clock1.happened_before(&clock2));
    /// assert!(!clock2.happened_before(&clock1));
    /// ```
    pub fn happened_before(&self, other: &VectorClock) -> bool {
        // Single-pass optimization: check both conditions simultaneously
        let mut any_strictly_less = false;

        // Check our nodes in a single pass
        for (node_id, &our_counter) in &self.timestamps {
            let their_counter = other.get(*node_id);

            // Early exit if we have a counter greater than theirs
            if our_counter > their_counter {
                return false;
            }

            // Track if any counter is strictly less
            if our_counter < their_counter {
                any_strictly_less = true;
            }
        }

        // If we haven't found a strictly less counter yet, check if they have
        // nodes we don't know about with non-zero values (those count as strictly less)
        if !any_strictly_less {
            any_strictly_less = other.timestamps.iter().any(|(node_id, &their_counter)| {
                !self.timestamps.contains_key(node_id) && their_counter > 0
            });
        }

        any_strictly_less
    }

    /// Check if this vector clock is concurrent with another
    ///
    /// Two clocks are concurrent if neither happened-before the other and they
    /// are not identical. This means the operations are causally independent
    /// and need CRDT merge semantics.
    ///
    /// # Example
    ///
    /// ```
    /// use libmarathon::networking::VectorClock;
    /// use uuid::Uuid;
    ///
    /// let node1 = Uuid::new_v4();
    /// let node2 = Uuid::new_v4();
    ///
    /// let mut clock1 = VectorClock::new();
    /// clock1.increment(node1); // node1: 1
    ///
    /// let mut clock2 = VectorClock::new();
    /// clock2.increment(node2); // node2: 1
    ///
    /// assert!(clock1.is_concurrent_with(&clock2));
    /// assert!(clock2.is_concurrent_with(&clock1));
    /// ```
    pub fn is_concurrent_with(&self, other: &VectorClock) -> bool {
        // Identical clocks are not concurrent
        if self == other {
            return false;
        }

        // Concurrent if neither happened-before the other
        !self.happened_before(other) && !other.happened_before(self)
    }

    /// Compare two vector clocks
    ///
    /// Returns:
    /// - `Ordering::Less` if self happened-before other
    /// - `Ordering::Greater` if other happened-before self
    /// - `Ordering::Equal` if they are identical
    /// - `Err` if they are concurrent
    pub fn compare(&self, other: &VectorClock) -> Result<std::cmp::Ordering> {
        if self == other {
            return Ok(std::cmp::Ordering::Equal);
        }

        if self.happened_before(other) {
            return Ok(std::cmp::Ordering::Less);
        }

        if other.happened_before(self) {
            return Ok(std::cmp::Ordering::Greater);
        }

        Err(NetworkingError::VectorClockError(
            "Clocks are concurrent".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_clock() {
        let clock = VectorClock::new();
        assert_eq!(clock.timestamps.len(), 0);
    }

    #[test]
    fn test_increment() {
        let node = uuid::Uuid::new_v4();
        let mut clock = VectorClock::new();

        assert_eq!(clock.increment(node), 1);
        assert_eq!(clock.get(node), 1);

        assert_eq!(clock.increment(node), 2);
        assert_eq!(clock.get(node), 2);
    }

    #[test]
    fn test_get_unknown_node() {
        let clock = VectorClock::new();
        let node = uuid::Uuid::new_v4();

        assert_eq!(clock.get(node), 0);
    }

    #[test]
    fn test_merge() {
        let node1 = uuid::Uuid::new_v4();
        let node2 = uuid::Uuid::new_v4();

        let mut clock1 = VectorClock::new();
        clock1.increment(node1);
        clock1.increment(node1);

        let mut clock2 = VectorClock::new();
        clock2.increment(node2);

        clock1.merge(&clock2);

        assert_eq!(clock1.get(node1), 2);
        assert_eq!(clock1.get(node2), 1);
    }

    #[test]
    fn test_merge_takes_max() {
        let node = uuid::Uuid::new_v4();

        let mut clock1 = VectorClock::new();
        clock1.increment(node);

        let mut clock2 = VectorClock::new();
        clock2.increment(node);
        clock2.increment(node);

        clock1.merge(&clock2);
        assert_eq!(clock1.get(node), 2);
    }

    #[test]
    fn test_happened_before() {
        let node = uuid::Uuid::new_v4();

        let mut clock1 = VectorClock::new();
        clock1.increment(node);

        let mut clock2 = VectorClock::new();
        clock2.increment(node);
        clock2.increment(node);

        assert!(clock1.happened_before(&clock2));
        assert!(!clock2.happened_before(&clock1));
    }

    #[test]
    fn test_happened_before_multiple_nodes() {
        let node1 = uuid::Uuid::new_v4();
        let node2 = uuid::Uuid::new_v4();

        let mut clock1 = VectorClock::new();
        clock1.increment(node1);

        let mut clock2 = VectorClock::new();
        clock2.increment(node1);
        clock2.increment(node2);

        assert!(clock1.happened_before(&clock2));
        assert!(!clock2.happened_before(&clock1));
    }

    #[test]
    fn test_concurrent() {
        let node1 = uuid::Uuid::new_v4();
        let node2 = uuid::Uuid::new_v4();

        let mut clock1 = VectorClock::new();
        clock1.increment(node1);

        let mut clock2 = VectorClock::new();
        clock2.increment(node2);

        assert!(clock1.is_concurrent_with(&clock2));
        assert!(clock2.is_concurrent_with(&clock1));
    }

    #[test]
    fn test_happened_before_with_disjoint_nodes() {
        // Critical test case: clocks with completely different nodes are concurrent,
        // not happened-before. This test would fail with the old buggy implementation.
        let node1 = uuid::Uuid::new_v4();
        let node2 = uuid::Uuid::new_v4();

        let mut clock1 = VectorClock::new();
        clock1.increment(node1); // {node1: 1}

        let mut clock2 = VectorClock::new();
        clock2.increment(node2); // {node2: 1}

        // These clocks are concurrent - neither happened before the other
        assert!(!clock1.happened_before(&clock2));
        assert!(!clock2.happened_before(&clock1));
        assert!(clock1.is_concurrent_with(&clock2));
    }

    #[test]
    fn test_happened_before_with_superset_nodes() {
        // When one clock has all nodes from another PLUS more nodes,
        // the smaller clock happened-before the larger one
        let node1 = uuid::Uuid::new_v4();
        let node2 = uuid::Uuid::new_v4();

        let mut clock1 = VectorClock::new();
        clock1.increment(node1); // {node1: 1}

        let mut clock2 = VectorClock::new();
        clock2.increment(node1); // {node1: 1, node2: 1}
        clock2.increment(node2);

        // clock1 happened before clock2
        assert!(clock1.happened_before(&clock2));
        assert!(!clock2.happened_before(&clock1));
        assert!(!clock1.is_concurrent_with(&clock2));
    }

    #[test]
    fn test_identical_clocks() {
        let node = uuid::Uuid::new_v4();

        let mut clock1 = VectorClock::new();
        clock1.increment(node);

        let mut clock2 = VectorClock::new();
        clock2.increment(node);

        assert_eq!(clock1, clock2);
        assert!(!clock1.happened_before(&clock2));
        assert!(!clock2.happened_before(&clock1));
        assert!(!clock1.is_concurrent_with(&clock2));
    }

    #[test]
    fn test_compare() {
        let node = uuid::Uuid::new_v4();

        let mut clock1 = VectorClock::new();
        clock1.increment(node);

        let mut clock2 = VectorClock::new();
        clock2.increment(node);
        clock2.increment(node);

        assert_eq!(clock1.compare(&clock2).unwrap(), std::cmp::Ordering::Less);
        assert_eq!(
            clock2.compare(&clock1).unwrap(),
            std::cmp::Ordering::Greater
        );
        assert_eq!(clock1.compare(&clock1).unwrap(), std::cmp::Ordering::Equal);
    }

    #[test]
    fn test_compare_concurrent() {
        let node1 = uuid::Uuid::new_v4();
        let node2 = uuid::Uuid::new_v4();

        let mut clock1 = VectorClock::new();
        clock1.increment(node1);

        let mut clock2 = VectorClock::new();
        clock2.increment(node2);

        assert!(clock1.compare(&clock2).is_err());
    }

    #[test]
    fn test_serialization() -> anyhow::Result<()> {
        let node = uuid::Uuid::new_v4();
        let mut clock = VectorClock::new();
        clock.increment(node);

        let bytes = rkyv::to_bytes::<rkyv::rancor::Failure>(&clock).map(|b| b.to_vec())?;
        let deserialized: VectorClock =
            rkyv::from_bytes::<VectorClock, rkyv::rancor::Failure>(&bytes)?;

        assert_eq!(clock, deserialized);

        Ok(())
    }
}
