//! Property-based tests for CRDT invariants
//!
//! This module uses proptest to verify that our CRDT implementations maintain
//! their mathematical properties under all possible inputs and operation
//! sequences.

use lib::{
    networking::{
        NodeId,
        VectorClock,
    },
    persistence::{
        EntityId,
        PersistenceError,
        PersistenceOp,
        WriteBuffer,
    },
    sync::SyncedValue,
};
use proptest::prelude::*;

// ============================================================================
// VectorClock Property Tests
// ============================================================================

/// Generate arbitrary NodeId (UUID)
fn arb_node_id() -> impl Strategy<Value = NodeId> {
    any::<[u8; 16]>().prop_map(|bytes| uuid::Uuid::from_bytes(bytes))
}

/// Generate arbitrary VectorClock with 1-10 nodes
fn arb_vector_clock() -> impl Strategy<Value = VectorClock> {
    prop::collection::vec((arb_node_id(), 0u64..100), 1..10).prop_map(|entries| {
        let mut clock = VectorClock::new();
        for (node_id, count) in entries {
            for _ in 0..count {
                clock.increment(node_id);
            }
        }
        clock
    })
}

proptest! {
    /// Test: VectorClock merge is idempotent
    /// Property: merge(A, A) = A
    #[test]
    fn vector_clock_merge_idempotent(clock in arb_vector_clock()) {
        let mut merged = clock.clone();
        merged.merge(&clock);
        prop_assert_eq!(merged, clock);
    }

    /// Test: VectorClock merge is commutative
    /// Property: merge(A, B) = merge(B, A)
    #[test]
    fn vector_clock_merge_commutative(
        clock_a in arb_vector_clock(),
        clock_b in arb_vector_clock()
    ) {
        let mut result1 = clock_a.clone();
        result1.merge(&clock_b);

        let mut result2 = clock_b.clone();
        result2.merge(&clock_a);

        prop_assert_eq!(result1, result2);
    }

    /// Test: VectorClock merge is associative
    /// Property: merge(merge(A, B), C) = merge(A, merge(B, C))
    #[test]
    fn vector_clock_merge_associative(
        clock_a in arb_vector_clock(),
        clock_b in arb_vector_clock(),
        clock_c in arb_vector_clock()
    ) {
        // (A merge B) merge C
        let mut result1 = clock_a.clone();
        result1.merge(&clock_b);
        result1.merge(&clock_c);

        // A merge (B merge C)
        let mut temp = clock_b.clone();
        temp.merge(&clock_c);
        let mut result2 = clock_a.clone();
        result2.merge(&temp);

        prop_assert_eq!(result1, result2);
    }

    /// Test: happened_before is transitive
    /// Property: If A < B and B < C, then A < C
    #[test]
    fn vector_clock_happened_before_transitive(node_id in arb_node_id()) {
        let mut clock_a = VectorClock::new();
        clock_a.increment(node_id);

        let mut clock_b = clock_a.clone();
        clock_b.increment(node_id);

        let mut clock_c = clock_b.clone();
        clock_c.increment(node_id);

        prop_assert!(clock_a.happened_before(&clock_b));
        prop_assert!(clock_b.happened_before(&clock_c));
        prop_assert!(clock_a.happened_before(&clock_c)); // Transitivity
    }

    /// Test: happened_before is antisymmetric
    /// Property: If A < B, then NOT (B < A)
    #[test]
    fn vector_clock_happened_before_antisymmetric(
        clock_a in arb_vector_clock(),
        clock_b in arb_vector_clock()
    ) {
        if clock_a.happened_before(&clock_b) {
            prop_assert!(!clock_b.happened_before(&clock_a));
        }
    }

    /// Test: A clock never happens before itself
    /// Property: NOT (A < A)
    #[test]
    fn vector_clock_not_happened_before_self(clock in arb_vector_clock()) {
        prop_assert!(!clock.happened_before(&clock));
    }

    /// Test: Merge creates upper bound
    /// Property: If C = merge(A, B), then A ≤ C and B ≤ C
    #[test]
    fn vector_clock_merge_upper_bound(
        clock_a in arb_vector_clock(),
        clock_b in arb_vector_clock()
    ) {
        let mut merged = clock_a.clone();
        merged.merge(&clock_b);

        // A ≤ merged (either happened_before or equal)
        prop_assert!(clock_a.happened_before(&merged) || clock_a == merged);
        // B ≤ merged (either happened_before or equal)
        prop_assert!(clock_b.happened_before(&merged) || clock_b == merged);
    }
}

// ============================================================================
// WriteBuffer Property Tests
// ============================================================================

/// Generate arbitrary EntityId (UUID)
fn arb_entity_id() -> impl Strategy<Value = EntityId> {
    any::<[u8; 16]>().prop_map(|bytes| uuid::Uuid::from_bytes(bytes))
}

/// Generate arbitrary component name
fn arb_component_type() -> impl Strategy<Value = String> {
    prop::string::string_regex("[A-Z][a-zA-Z0-9]{0,20}").unwrap()
}

/// Generate arbitrary component data (small to avoid size limits)
fn arb_component_data() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..1000)
}

proptest! {
    /// Test: WriteBuffer index consistency after multiple operations
    /// Property: Index always points to valid operations in the buffer
    #[test]
    fn write_buffer_index_consistency(
        operations in prop::collection::vec(
            (arb_entity_id(), arb_component_type(), arb_component_data()),
            1..50
        )
    ) {
        let mut buffer = WriteBuffer::new(1000);

        for (entity_id, component_type, data) in operations {
            let op = PersistenceOp::UpsertComponent {
                entity_id,
                component_type,
                data,
            };

            // Should never fail for valid data
            let result = buffer.add(op);
            prop_assert!(result.is_ok());
        }

        // After all operations, buffer length should be valid
        prop_assert!(buffer.len() <= 1000);
    }

    /// Test: WriteBuffer deduplication correctness
    /// Property: Adding same (entity, component) twice keeps only latest
    #[test]
    fn write_buffer_deduplication(
        entity_id in arb_entity_id(),
        component_type in arb_component_type(),
        data1 in arb_component_data(),
        data2 in arb_component_data()
    ) {
        let mut buffer = WriteBuffer::new(100);

        // Add first version
        let op1 = PersistenceOp::UpsertComponent {
            entity_id,
            component_type: component_type.clone(),
            data: data1.clone(),
        };
        prop_assert!(buffer.add(op1).is_ok());

        // Add second version (should replace)
        let op2 = PersistenceOp::UpsertComponent {
            entity_id,
            component_type: component_type.clone(),
            data: data2.clone(),
        };
        prop_assert!(buffer.add(op2).is_ok());

        // Should only have 1 operation
        prop_assert_eq!(buffer.len(), 1);

        // Should be the latest data
        let ops = buffer.take_operations();
        prop_assert_eq!(ops.len(), 1);
        if let PersistenceOp::UpsertComponent { data, .. } = &ops[0] {
            prop_assert_eq!(data, &data2);
        }
    }

    /// Test: WriteBuffer respects size limits
    /// Property: Operations larger than MAX_SIZE are rejected
    ///
    /// Note: This test uses a smaller sample size (5 cases) to avoid generating
    /// massive amounts of data. We only need to verify the size check works.
    #[test]
    fn write_buffer_respects_size_limits(
        entity_id in arb_entity_id(),
        component_type in arb_component_type(),
    ) {
        let mut buffer = WriteBuffer::new(100);

        // Create data that's definitely too large (11MB)
        // We don't need to randomize the content, just verify size check works
        let oversized_data = vec![0u8; 11_000_000];

        let op = PersistenceOp::UpsertComponent {
            entity_id,
            component_type,
            data: oversized_data,
        };

        let result = buffer.add(op);
        prop_assert!(result.is_err());

        // Verify it's the right error type
        if let Err(PersistenceError::ComponentTooLarge { .. }) = result {
            // Expected
        } else {
            prop_assert!(false, "Expected ComponentTooLarge error");
        }
    }
}

// ============================================================================
// LWW (Last-Write-Wins) Property Tests
// ============================================================================

proptest! {
    /// Test: LWW convergence
    /// Property: Two replicas applying same updates in different order converge
    #[test]
    fn lww_convergence(
        node1 in arb_node_id(),
        node2 in arb_node_id(),
        initial_value in any::<i32>(),
        value1 in any::<i32>(),
        value2 in any::<i32>(),
    ) {
        // Create two replicas with same initial value
        let mut replica_a = SyncedValue::new(initial_value, node1);
        let mut replica_b = SyncedValue::new(initial_value, node1);

        // Create two updates
        let ts1 = chrono::Utc::now();
        let ts2 = ts1 + chrono::Duration::milliseconds(100);

        // Replica A applies updates in order: update1, update2
        replica_a.apply_lww(value1, ts1, node1);
        replica_a.apply_lww(value2, ts2, node2);

        // Replica B applies updates in reverse: update2, update1
        replica_b.apply_lww(value2, ts2, node2);
        replica_b.apply_lww(value1, ts1, node1);

        // Both should converge to same value (latest timestamp wins)
        prop_assert_eq!(*replica_a.get(), *replica_b.get());
        prop_assert_eq!(*replica_a.get(), value2); // ts2 is newer
    }

    /// Test: LWW merge idempotence
    /// Property: Merging the same value multiple times has no effect
    #[test]
    fn lww_merge_idempotent(
        node_id in arb_node_id(),
        value in any::<i32>(),
    ) {
        let original = SyncedValue::new(value, node_id);
        let mut replica = original.clone();

        // Merge with itself multiple times
        replica.merge(&original);
        replica.merge(&original);
        replica.merge(&original);

        prop_assert_eq!(*replica.get(), *original.get());
    }

    /// Test: LWW respects timestamp ordering
    /// Property: Older updates don't overwrite newer ones
    #[test]
    fn lww_respects_timestamp(
        node_id in arb_node_id(),
        old_value in any::<i32>(),
        new_value in any::<i32>(),
    ) {
        let mut lww = SyncedValue::new(old_value, node_id);

        let old_ts = chrono::Utc::now();
        let new_ts = old_ts + chrono::Duration::seconds(10);

        // Apply newer update first
        lww.apply_lww(new_value, new_ts, node_id);

        // Apply older update (should be ignored)
        lww.apply_lww(old_value, old_ts, node_id);

        // Should keep the newer value
        prop_assert_eq!(*lww.get(), new_value);
    }

    /// Test: LWW tiebreaker uses node_id
    /// Property: When timestamps equal, higher node_id wins
    #[test]
    fn lww_tiebreaker(
        value1 in any::<i32>(),
        value2 in any::<i32>(),
    ) {
        let node1 = uuid::Uuid::from_u128(1);
        let node2 = uuid::Uuid::from_u128(2);

        // Create SyncedValue FIRST, then capture a timestamp that's guaranteed to be newer
        let mut lww = SyncedValue::new(value1, node1);
        std::thread::sleep(std::time::Duration::from_millis(1)); // Ensure ts is after init
        let ts = chrono::Utc::now();

        // Apply update from node1 at timestamp ts
        lww.apply_lww(value1, ts, node1);

        // Apply conflicting update from node2 at SAME timestamp
        lww.apply_lww(value2, ts, node2);

        // node2 > node1, so value2 should win
        prop_assert_eq!(*lww.get(), value2);
    }
}
