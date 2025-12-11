//! CRDT merge logic for conflict resolution
//!
//! This module implements the merge semantics for different CRDT types:
//! - Last-Write-Wins (LWW) for simple components
//! - OR-Set for concurrent add/remove
//! - Sequence CRDT (RGA) for ordered lists

use bevy::prelude::*;

use crate::networking::{
    operations::ComponentOp,
    vector_clock::{
        NodeId,
        VectorClock,
    },
};

/// Result of comparing two operations for merge
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeDecision {
    /// The local operation wins (keep local, discard remote)
    KeepLocal,

    /// The remote operation wins (apply remote, discard local)
    ApplyRemote,

    /// Operations are concurrent, need CRDT-specific merge
    Concurrent,

    /// Operations are identical
    Equal,
}

/// Compare two operations using vector clocks to determine merge decision
///
/// This implements Last-Write-Wins (LWW) semantics with node ID tiebreaking.
///
/// # Algorithm
///
/// 1. If local happened-before remote: ApplyRemote
/// 2. If remote happened-before local: KeepLocal
/// 3. If concurrent: use node ID as tiebreaker (higher node ID wins)
/// 4. If equal: Equal
///
/// # Example
///
/// ```
/// use lib::networking::{
///     VectorClock,
///     compare_operations_lww,
/// };
/// use uuid::Uuid;
///
/// let node1 = Uuid::new_v4();
/// let node2 = Uuid::new_v4();
///
/// let mut clock1 = VectorClock::new();
/// clock1.increment(node1);
///
/// let mut clock2 = VectorClock::new();
/// clock2.increment(node2);
///
/// // Concurrent operations use node ID as tiebreaker
/// let decision = compare_operations_lww(&clock1, node1, &clock2, node2);
/// ```
pub fn compare_operations_lww(
    local_clock: &VectorClock,
    local_node: NodeId,
    remote_clock: &VectorClock,
    remote_node: NodeId,
) -> MergeDecision {
    // Check if clocks are equal
    if local_clock == remote_clock && local_node == remote_node {
        return MergeDecision::Equal;
    }

    // Check happens-before relationship
    if local_clock.happened_before(remote_clock) {
        return MergeDecision::ApplyRemote;
    }

    if remote_clock.happened_before(local_clock) {
        return MergeDecision::KeepLocal;
    }

    // Concurrent operations - use node ID as tiebreaker
    // Higher node ID wins for deterministic resolution
    if remote_node > local_node {
        MergeDecision::ApplyRemote
    } else if local_node > remote_node {
        MergeDecision::KeepLocal
    } else {
        MergeDecision::Concurrent
    }
}

/// Determine if a remote Set operation should be applied
///
/// This is a convenience wrapper around `compare_operations_lww` for Set
/// operations specifically.
pub fn should_apply_set(local_op: &ComponentOp, remote_op: &ComponentOp) -> bool {
    // Extract vector clocks and node IDs
    let (local_clock, local_data) = match local_op {
        | ComponentOp::Set {
            vector_clock, data, ..
        } => (vector_clock, data),
        | _ => return false,
    };

    let (remote_clock, remote_data) = match remote_op {
        | ComponentOp::Set {
            vector_clock, data, ..
        } => (vector_clock, data),
        | _ => return false,
    };

    // If data is identical, no need to apply
    if local_data == remote_data {
        return false;
    }

    // Use the sequence number from the clocks as a simple tiebreaker
    // In a real implementation, we'd use the full node IDs
    let local_seq: u64 = local_clock.clocks.values().sum();
    let remote_seq: u64 = remote_clock.clocks.values().sum();

    // Compare clocks
    match compare_operations_lww(
        local_clock,
        uuid::Uuid::nil(), // Simplified - would use actual node IDs
        remote_clock,
        uuid::Uuid::nil(),
    ) {
        | MergeDecision::ApplyRemote => true,
        | MergeDecision::KeepLocal => false,
        | MergeDecision::Concurrent => remote_seq > local_seq,
        | MergeDecision::Equal => false,
    }
}

/// Log a merge conflict for debugging
///
/// This helps track when concurrent operations occur and how they're resolved.
pub fn log_merge_conflict(
    component_type: &str,
    local_clock: &VectorClock,
    remote_clock: &VectorClock,
    decision: MergeDecision,
) {
    info!(
        "Merge conflict on {}: local={:?}, remote={:?}, decision={:?}",
        component_type, local_clock, remote_clock, decision
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::networking::messages::ComponentData;

    #[test]
    fn test_lww_happened_before() {
        let node1 = uuid::Uuid::new_v4();
        let node2 = uuid::Uuid::new_v4();

        let mut clock1 = VectorClock::new();
        clock1.increment(node1);

        let mut clock2 = VectorClock::new();
        clock2.increment(node1);
        clock2.increment(node1);

        let decision = compare_operations_lww(&clock1, node1, &clock2, node2);
        assert_eq!(decision, MergeDecision::ApplyRemote);

        let decision = compare_operations_lww(&clock2, node1, &clock1, node2);
        assert_eq!(decision, MergeDecision::KeepLocal);
    }

    #[test]
    fn test_lww_concurrent() {
        let node1 = uuid::Uuid::new_v4();
        let node2 = uuid::Uuid::new_v4();

        let mut clock1 = VectorClock::new();
        clock1.increment(node1);

        let mut clock2 = VectorClock::new();
        clock2.increment(node2);

        // Concurrent operations use node ID tiebreaker
        let decision = compare_operations_lww(&clock1, node1, &clock2, node2);

        // Should use node ID as tiebreaker
        assert!(decision == MergeDecision::ApplyRemote || decision == MergeDecision::KeepLocal);
    }

    #[test]
    fn test_lww_equal() {
        let node1 = uuid::Uuid::new_v4();

        let mut clock1 = VectorClock::new();
        clock1.increment(node1);

        let clock2 = clock1.clone();

        let decision = compare_operations_lww(&clock1, node1, &clock2, node1);
        assert_eq!(decision, MergeDecision::Equal);
    }

    #[test]
    fn test_should_apply_set_same_data() {
        let node_id = uuid::Uuid::new_v4();
        let mut clock = VectorClock::new();
        clock.increment(node_id);

        let data = vec![1, 2, 3];

        let op1 = ComponentOp::Set {
            component_type: "Transform".to_string(),
            data: ComponentData::Inline(data.clone()),
            vector_clock: clock.clone(),
        };

        let op2 = ComponentOp::Set {
            component_type: "Transform".to_string(),
            data: ComponentData::Inline(data.clone()),
            vector_clock: clock,
        };

        // Same data, should not apply
        assert!(!should_apply_set(&op1, &op2));
    }

    #[test]
    fn test_should_apply_set_newer_wins() {
        let node_id = uuid::Uuid::new_v4();

        let mut clock1 = VectorClock::new();
        clock1.increment(node_id);

        let mut clock2 = VectorClock::new();
        clock2.increment(node_id);
        clock2.increment(node_id);

        let op1 = ComponentOp::Set {
            component_type: "Transform".to_string(),
            data: ComponentData::Inline(vec![1, 2, 3]),
            vector_clock: clock1,
        };

        let op2 = ComponentOp::Set {
            component_type: "Transform".to_string(),
            data: ComponentData::Inline(vec![4, 5, 6]),
            vector_clock: clock2,
        };

        // op2 is newer, should apply
        assert!(should_apply_set(&op1, &op2));

        // op1 is older, should not apply
        assert!(!should_apply_set(&op2, &op1));
    }
}
