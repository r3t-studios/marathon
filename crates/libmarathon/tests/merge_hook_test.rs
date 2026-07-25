//! Tests for the CRDT merge hook (`ComponentMeta::merge_fn`).
//!
//! A component type registered with a merge function uses CRDT semantics:
//! remote `Set` operations are always applied via the merge function instead
//! of being gated by whole-component last-writer-wins. These tests cover:
//!
//! 1. CRDT laws (commutativity, associativity, idempotency) for the test CRDT
//! 2. Registry plumbing (`get_merge_fn`)
//! 3. Delta apply path: merge components bypass the LWW gate, others don't
//! 4. FullState join path: merge instead of wholesale overwrite
//! 5. End-to-end convergence of concurrent edits over real gossip

mod test_utils;

use std::{
    any::TypeId,
    collections::HashMap,
    time::Duration,
};

use anyhow::Result;
use bevy::prelude::*;
use libmarathon::{
    networking::{
        ComponentData,
        ComponentOp,
        ComponentState,
        ComponentVectorClocks,
        CrdtMerge,
        EntityDelta,
        EntityState,
        NetworkEntityMap,
        NetworkedEntity,
        NodeVectorClock,
        Synced,
        VectorClock,
        apply_entity_delta,
        apply_full_state,
        merge_into,
    },
    persistence::{
        self,
        Persisted,
    },
};
use proptest::prelude::*;
use test_utils::TestContext;
use uuid::Uuid;

// ============================================================================
// Test components
// ============================================================================

/// A PN-counter: per-node (increment, decrement) totals. Concurrent updates
/// from different nodes must ALL survive a merge — exactly what
/// whole-component LWW loses.
#[derive(
    Component,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
struct TestPnCounter {
    p: HashMap<Uuid, u64>,
    n: HashMap<Uuid, u64>,
}

impl TestPnCounter {
    fn value(&self) -> i64 {
        self.p.values().sum::<u64>() as i64 - self.n.values().sum::<u64>() as i64
    }

    fn increment(&mut self, node: Uuid) {
        *self.p.entry(node).or_insert(0) += 1;
    }
}

impl CrdtMerge for TestPnCounter {
    fn crdt_merge(&mut self, remote: Self) {
        for (node, count) in remote.p {
            let entry = self.p.entry(node).or_insert(0);
            *entry = (*entry).max(count);
        }
        for (node, count) in remote.n {
            let entry = self.n.entry(node).or_insert(0);
            *entry = (*entry).max(count);
        }
    }
}

/// A plain component with no merge function — the LWW control case.
#[derive(
    Component, Clone, Copy, Debug, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
struct TestLwwValue {
    value: u64,
}

// ============================================================================
// Registration (hand-written: the #[synced] macro does not emit merge_fn)
// ============================================================================

inventory::submit! {
    persistence::ComponentMeta {
        type_name: "TestPnCounter",
        type_path: concat!(module_path!(), "::TestPnCounter"),
        type_id: TypeId::of::<TestPnCounter>(),

        deserialize_fn: |bytes: &[u8]| -> anyhow::Result<Box<dyn std::any::Any>> {
            let component = rkyv::from_bytes::<TestPnCounter, rkyv::rancor::Failure>(bytes)?;
            Ok(Box::new(component))
        },

        serialize_fn: |world: &World, entity: Entity| -> Option<bytes::Bytes> {
            world.get::<TestPnCounter>(entity).map(|component| {
                let serialized = rkyv::to_bytes::<rkyv::rancor::Failure>(component)
                    .expect("Failed to serialize TestPnCounter");
                bytes::Bytes::from(serialized.to_vec())
            })
        },

        insert_fn: |entity_mut: &mut EntityWorldMut, boxed: Box<dyn std::any::Any>| {
            if let Ok(component) = boxed.downcast::<TestPnCounter>() {
                entity_mut.insert(*component);
            }
        },

        merge_fn: Some(merge_into::<TestPnCounter>),
    }
}

inventory::submit! {
    persistence::ComponentMeta {
        type_name: "TestLwwValue",
        type_path: concat!(module_path!(), "::TestLwwValue"),
        type_id: TypeId::of::<TestLwwValue>(),

        deserialize_fn: |bytes: &[u8]| -> anyhow::Result<Box<dyn std::any::Any>> {
            let component = rkyv::from_bytes::<TestLwwValue, rkyv::rancor::Failure>(bytes)?;
            Ok(Box::new(component))
        },

        serialize_fn: |world: &World, entity: Entity| -> Option<bytes::Bytes> {
            world.get::<TestLwwValue>(entity).map(|component| {
                let serialized = rkyv::to_bytes::<rkyv::rancor::Failure>(component)
                    .expect("Failed to serialize TestLwwValue");
                bytes::Bytes::from(serialized.to_vec())
            })
        },

        insert_fn: |entity_mut: &mut EntityWorldMut, boxed: Box<dyn std::any::Any>| {
            if let Ok(component) = boxed.downcast::<TestLwwValue>() {
                entity_mut.insert(*component);
            }
        },

        merge_fn: None,
    }
}

/// Registered via the engine's own `register_component!` macro to prove the
/// macro path defaults to no merge function.
#[derive(Component, Clone, Copy, Debug, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
struct MacroRegistered {
    #[allow(dead_code)]
    v: u8,
}

libmarathon::register_component!(MacroRegistered, "merge_hook_test::MacroRegistered");

// ============================================================================
// Helpers
// ============================================================================

fn registry() -> &'static persistence::ComponentTypeRegistry {
    persistence::component_registry()
}

fn discriminant_of<T: 'static>() -> u16 {
    registry()
        .get_discriminant(TypeId::of::<T>())
        .expect("component registered")
}

fn counter_bytes(counter: &TestPnCounter) -> bytes::Bytes {
    bytes::Bytes::from(
        rkyv::to_bytes::<rkyv::rancor::Failure>(counter)
            .expect("serialize")
            .to_vec(),
    )
}

fn lww_bytes(value: &TestLwwValue) -> bytes::Bytes {
    bytes::Bytes::from(
        rkyv::to_bytes::<rkyv::rancor::Failure>(value)
            .expect("serialize")
            .to_vec(),
    )
}

/// A World with the resources the apply paths require.
fn world_with_resources() -> World {
    let mut world = World::new();
    world.insert_resource(persistence::ComponentTypeRegistryResource::default());
    world.insert_resource(NodeVectorClock::new(Uuid::new_v4()));
    world.insert_resource(NetworkEntityMap::default());
    world.insert_resource(ComponentVectorClocks::new());
    world
}

/// Spawn a networked entity and register it in the NetworkEntityMap (what
/// spawn_networked_entity does in production).
fn spawn_networked(
    world: &mut World,
    network_id: Uuid,
    node: Uuid,
    counter: TestPnCounter,
) -> Entity {
    let entity = world
        .spawn((NetworkedEntity::with_id(network_id, node), counter, Synced))
        .id();
    world
        .resource_mut::<NetworkEntityMap>()
        .insert(network_id, entity);
    entity
}

fn set_delta(
    network_id: Uuid,
    node: Uuid,
    clock: &VectorClock,
    discriminant: u16,
    payload: bytes::Bytes,
) -> EntityDelta {
    EntityDelta::new(
        network_id,
        node,
        clock.clone(),
        vec![ComponentOp::Set {
            discriminant,
            data: ComponentData::Inline(payload),
            vector_clock: clock.clone(),
        }],
    )
}

fn counter_value(world: &mut World, network_id: Uuid) -> Option<i64> {
    let mut query = world.query::<(&NetworkedEntity, &TestPnCounter)>();
    query
        .iter(world)
        .find(|(ne, _)| ne.network_id == network_id)
        .map(|(_, c)| c.value())
}

fn counter_contrib(world: &mut World, network_id: Uuid, node: Uuid) -> u64 {
    let mut query = world.query::<(&NetworkedEntity, &TestPnCounter)>();
    query
        .iter(world)
        .find(|(ne, _)| ne.network_id == network_id)
        .map(|(_, c)| c.p.get(&node).copied().unwrap_or(0))
        .unwrap_or(0)
}

fn lww_value(world: &mut World, network_id: Uuid) -> Option<u64> {
    let mut query = world.query::<(&NetworkedEntity, &TestLwwValue)>();
    query
        .iter(world)
        .find(|(ne, _)| ne.network_id == network_id)
        .map(|(_, v)| v.value)
}

/// Two distinct node IDs, ordered so the first is greater (wins the LWW
/// concurrent tiebreak).
fn ordered_nodes() -> (Uuid, Uuid) {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    if a > b { (a, b) } else { (b, a) }
}

// ============================================================================
// 1. CRDT laws (proptest)
// ============================================================================

fn arb_counter() -> impl Strategy<Value = TestPnCounter> {
    (
        prop::collection::vec((any::<[u8; 16]>(), 0u64..100), 0..4),
        prop::collection::vec((any::<[u8; 16]>(), 0u64..100), 0..4),
    )
        .prop_map(|(p, n)| {
            let mut counter = TestPnCounter::default();
            for (node, count) in p {
                counter.p.insert(Uuid::from_bytes(node), count);
            }
            for (node, count) in n {
                counter.n.insert(Uuid::from_bytes(node), count);
            }
            counter
        })
}

proptest! {
    #[test]
    fn pn_counter_merge_commutative(a in arb_counter(), b in arb_counter()) {
        let mut ab = a.clone();
        ab.crdt_merge(b.clone());
        let mut ba = b;
        ba.crdt_merge(a);
        prop_assert_eq!(ab, ba);
    }

    #[test]
    fn pn_counter_merge_associative(a in arb_counter(), b in arb_counter(), c in arb_counter()) {
        let mut ab_c = a.clone();
        ab_c.crdt_merge(b.clone());
        ab_c.crdt_merge(c.clone());
        let mut bc = b;
        bc.crdt_merge(c);
        let mut a_bc = a;
        a_bc.crdt_merge(bc);
        prop_assert_eq!(ab_c, a_bc);
    }

    #[test]
    fn pn_counter_merge_idempotent(a in arb_counter()) {
        let mut merged = a.clone();
        merged.crdt_merge(a.clone());
        prop_assert_eq!(merged, a);
    }

    #[test]
    fn pn_counter_merge_is_per_node_max(a in arb_counter(), b in arb_counter()) {
        let mut merged = a.clone();
        merged.crdt_merge(b.clone());
        for (node, count) in &merged.p {
            let expected = a.p.get(node).copied().unwrap_or(0).max(b.p.get(node).copied().unwrap_or(0));
            prop_assert_eq!(*count, expected);
        }
        for (node, count) in &merged.n {
            let expected = a.n.get(node).copied().unwrap_or(0).max(b.n.get(node).copied().unwrap_or(0));
            prop_assert_eq!(*count, expected);
        }
    }
}

// ============================================================================
// 2. Registry plumbing
// ============================================================================

#[test]
fn registry_returns_merge_fn_only_for_crdt_types() {
    assert!(
        registry()
            .get_merge_fn(discriminant_of::<TestPnCounter>())
            .is_some()
    );
    assert!(
        registry()
            .get_merge_fn(discriminant_of::<TestLwwValue>())
            .is_none()
    );
}

// ============================================================================
// 3. Delta apply path
// ============================================================================

/// The headline behavior: a concurrent remote Set on a merge component is
/// MERGED, not dropped by the LWW gate.
#[test]
fn merge_component_merges_concurrent_remote() {
    let mut world = world_with_resources();
    let entity_id = Uuid::new_v4();
    // local node wins the tiebreak, so the gate WOULD drop the remote op
    let (local_node, remote_node) = ordered_nodes();

    let mut local_counter = TestPnCounter::default();
    local_counter.p.insert(local_node, 5);
    spawn_networked(&mut world, entity_id, local_node, local_counter);

    // Concurrent clocks: each side only sees its own updates
    let mut local_clock = VectorClock::new();
    local_clock.increment(local_node);
    let mut remote_clock = VectorClock::new();
    remote_clock.increment(remote_node);

    // Pre-seed the stored clock so the gate is actively exercised
    world.resource_mut::<ComponentVectorClocks>().set(
        entity_id,
        "TestPnCounter".to_string(),
        local_clock,
        local_node,
    );

    let mut remote_counter = TestPnCounter::default();
    remote_counter.p.insert(remote_node, 3);
    let delta = set_delta(
        entity_id,
        remote_node,
        &remote_clock,
        discriminant_of::<TestPnCounter>(),
        counter_bytes(&remote_counter),
    );

    apply_entity_delta(&delta, &mut world);

    assert_eq!(counter_value(&mut world, entity_id), Some(8));
}

/// Control: the same concurrent remote Set on a non-merge component is still
/// dropped by the LWW gate.
#[test]
fn lww_component_drops_concurrent_remote() {
    let mut world = world_with_resources();
    let entity_id = Uuid::new_v4();
    let (local_node, remote_node) = ordered_nodes(); // local wins tiebreak

    let entity = world
        .spawn((
            NetworkedEntity::with_id(entity_id, local_node),
            TestLwwValue { value: 5 },
            Synced,
        ))
        .id();
    world
        .resource_mut::<NetworkEntityMap>()
        .insert(entity_id, entity);

    let mut local_clock = VectorClock::new();
    local_clock.increment(local_node);
    let mut remote_clock = VectorClock::new();
    remote_clock.increment(remote_node);

    world.resource_mut::<ComponentVectorClocks>().set(
        entity_id,
        "TestLwwValue".to_string(),
        local_clock,
        local_node,
    );

    let delta = set_delta(
        entity_id,
        remote_node,
        &remote_clock,
        discriminant_of::<TestLwwValue>(),
        lww_bytes(&TestLwwValue { value: 3 }),
    );

    apply_entity_delta(&delta, &mut world);

    assert_eq!(lww_value(&mut world, entity_id), Some(5));
}

/// Control: a non-merge component still applies a remote Set that legitimately
/// wins the LWW comparison.
#[test]
fn lww_component_applies_winning_remote() {
    let mut world = world_with_resources();
    let entity_id = Uuid::new_v4();
    let (remote_node, local_node) = ordered_nodes(); // remote wins tiebreak

    let entity = world
        .spawn((
            NetworkedEntity::with_id(entity_id, local_node),
            TestLwwValue { value: 5 },
            Synced,
        ))
        .id();
    world
        .resource_mut::<NetworkEntityMap>()
        .insert(entity_id, entity);

    let mut local_clock = VectorClock::new();
    local_clock.increment(local_node);
    let mut remote_clock = VectorClock::new();
    remote_clock.increment(remote_node);

    world.resource_mut::<ComponentVectorClocks>().set(
        entity_id,
        "TestLwwValue".to_string(),
        local_clock,
        local_node,
    );

    let delta = set_delta(
        entity_id,
        remote_node,
        &remote_clock,
        discriminant_of::<TestLwwValue>(),
        lww_bytes(&TestLwwValue { value: 3 }),
    );

    apply_entity_delta(&delta, &mut world);

    assert_eq!(lww_value(&mut world, entity_id), Some(3));
}

/// A merge delta for an unknown entity spawns it with the remote state.
#[test]
fn merge_delta_for_unknown_entity_inserts() {
    let mut world = world_with_resources();
    let entity_id = Uuid::new_v4();
    let remote_node = Uuid::new_v4();
    let mut remote_clock = VectorClock::new();
    remote_clock.increment(remote_node);

    let mut remote_counter = TestPnCounter::default();
    remote_counter.p.insert(remote_node, 3);
    let delta = set_delta(
        entity_id,
        remote_node,
        &remote_clock,
        discriminant_of::<TestPnCounter>(),
        counter_bytes(&remote_counter),
    );

    apply_entity_delta(&delta, &mut world);

    assert_eq!(counter_value(&mut world, entity_id), Some(3));
}

/// Re-applying the same merge delta does not double-apply it (idempotence) —
/// including after the stored clock has caught up (Equal decision territory).
#[test]
fn merge_delta_reapplication_is_idempotent() {
    let mut world = world_with_resources();
    let entity_id = Uuid::new_v4();
    let remote_node = Uuid::new_v4();
    let mut remote_clock = VectorClock::new();
    remote_clock.increment(remote_node);

    let mut remote_counter = TestPnCounter::default();
    remote_counter.p.insert(remote_node, 3);
    let delta = set_delta(
        entity_id,
        remote_node,
        &remote_clock,
        discriminant_of::<TestPnCounter>(),
        counter_bytes(&remote_counter),
    );

    apply_entity_delta(&delta, &mut world);
    apply_entity_delta(&delta, &mut world);

    assert_eq!(counter_value(&mut world, entity_id), Some(3));
}

/// The component clock is still updated for merge components (bookkeeping for
/// tombstones, sync cursors, and any future non-merge use of the type).
#[test]
fn merge_apply_still_updates_component_clock() {
    let mut world = world_with_resources();
    let entity_id = Uuid::new_v4();
    let remote_node = Uuid::new_v4();
    let mut remote_clock = VectorClock::new();
    remote_clock.increment(remote_node);

    let mut remote_counter = TestPnCounter::default();
    remote_counter.p.insert(remote_node, 1);
    let delta = set_delta(
        entity_id,
        remote_node,
        &remote_clock,
        discriminant_of::<TestPnCounter>(),
        counter_bytes(&remote_counter),
    );

    apply_entity_delta(&delta, &mut world);

    let clocks = world.resource::<ComponentVectorClocks>();
    let (clock, node) = clocks
        .get(entity_id, "TestPnCounter")
        .expect("clock recorded");
    assert_eq!(*node, remote_node);
    assert_eq!(*clock, remote_clock);
}

// ============================================================================
// 4. FullState join path
// ============================================================================

/// On (re)join, FullState merges CRDT components into existing local state
/// instead of clobbering them.
#[test]
fn full_state_merges_crdt_components() {
    let mut world = world_with_resources();
    let entity_id = Uuid::new_v4();
    let owner = Uuid::new_v4();

    let mut local_counter = TestPnCounter::default();
    local_counter.p.insert(Uuid::new_v4(), 5);
    spawn_networked(&mut world, entity_id, owner, local_counter);

    let mut remote_counter = TestPnCounter::default();
    remote_counter.p.insert(Uuid::new_v4(), 3);
    let state = EntityState {
        entity_id,
        owner_node_id: owner,
        vector_clock: VectorClock::new(),
        components: vec![ComponentState {
            discriminant: discriminant_of::<TestPnCounter>(),
            data: ComponentData::Inline(counter_bytes(&remote_counter)),
        }],
        is_deleted: false,
    };

    apply_full_state(vec![state], VectorClock::new(), &mut world, registry());

    assert_eq!(counter_value(&mut world, entity_id), Some(8));
}

/// Control: FullState still overwrites non-merge components wholesale.
#[test]
fn full_state_overwrites_lww_components() {
    let mut world = world_with_resources();
    let entity_id = Uuid::new_v4();
    let owner = Uuid::new_v4();

    let entity = world
        .spawn((
            NetworkedEntity::with_id(entity_id, owner),
            TestLwwValue { value: 5 },
            Synced,
        ))
        .id();
    world
        .resource_mut::<NetworkEntityMap>()
        .insert(entity_id, entity);

    let state = EntityState {
        entity_id,
        owner_node_id: owner,
        vector_clock: VectorClock::new(),
        components: vec![ComponentState {
            discriminant: discriminant_of::<TestLwwValue>(),
            data: ComponentData::Inline(lww_bytes(&TestLwwValue { value: 3 })),
        }],
        is_deleted: false,
    };

    apply_full_state(vec![state], VectorClock::new(), &mut world, registry());

    assert_eq!(lww_value(&mut world, entity_id), Some(3));
}

// ============================================================================
// 5. End-to-end convergence over real gossip
// ============================================================================

/// Two nodes edit the same counter concurrently. Under whole-component LWW
/// one side's increments would be silently lost; with the merge hook both
/// contributions survive and both nodes converge to the same total.
#[tokio::test(flavor = "multi_thread")]
async fn concurrent_edits_converge_over_gossip() -> Result<()> {
    let ctx1 = TestContext::new();
    let ctx2 = TestContext::new();

    let (_ep1, _ep2, _r1, _r2, bridge1, bridge2) = test_utils::setup_gossip_pair().await?;
    let node1 = bridge1.node_id();
    let node2 = bridge2.node_id();

    let mut app1 = test_utils::create_test_app(node1, ctx1.db_path(), bridge1);
    let mut app2 = test_utils::create_test_app(node2, ctx2.db_path(), bridge2);

    // Node 1 spawns the entity with its own contribution (2).
    let entity_id = Uuid::new_v4();
    let mut counter = TestPnCounter::default();
    counter.increment(node1);
    counter.increment(node1);
    let spawned = app1
        .world_mut()
        .spawn((
            NetworkedEntity::with_id(entity_id, node1),
            counter,
            Persisted::with_id(entity_id),
            Synced,
        ))
        .id();
    // Trigger change detection so delta generation picks the entity up
    if let Ok(mut entity_mut) = app1.world_mut().get_entity_mut(spawned) &&
        let Some(mut networked) = entity_mut.get_mut::<NetworkedEntity>()
    {
        let _ = &mut *networked;
    }

    // Wait for the entity to reach node 2.
    test_utils::wait_for_sync(&mut app1, &mut app2, Duration::from_secs(15), |_, w2| {
        counter_value(w2, entity_id) == Some(2)
    })
    .await?;

    // Concurrent edits on both sides: +2 from node 1, +3 from node 2.
    for (world, node, times) in [(app1.world_mut(), node1, 2), (app2.world_mut(), node2, 3)] {
        let entity = {
            let map = world.resource::<NetworkEntityMap>();
            map.get_entity(entity_id).expect("entity present")
        };
        let mut entity_mut = world.get_entity_mut(entity).unwrap();
        if let Some(mut counter) = entity_mut.get_mut::<TestPnCounter>() {
            for _ in 0..times {
                counter.increment(node);
            }
        }
        // Touch NetworkedEntity so delta generation fires (change detection
        // for non-Transform components is whole-entity for now)
        if let Some(mut networked) = entity_mut.get_mut::<NetworkedEntity>() {
            let _ = &mut *networked;
        }
    }

    // Both nodes must converge to 7 (= 2 + 2 + 3) with both contributions.
    test_utils::wait_for_sync(&mut app1, &mut app2, Duration::from_secs(20), |w1, w2| {
        counter_value(w1, entity_id) == Some(7) && counter_value(w2, entity_id) == Some(7)
    })
    .await?;

    for world in [app1.world_mut(), app2.world_mut()] {
        assert_eq!(counter_contrib(world, entity_id, node1), 4);
        assert_eq!(counter_contrib(world, entity_id, node2), 3);
    }

    Ok(())
}

// ============================================================================
// 6. Additional merge semantics
// ============================================================================

/// The decrement side of the PN-counter merges too.
#[test]
fn pn_counter_merges_decrements() {
    let (na, nb) = (Uuid::new_v4(), Uuid::new_v4());
    let mut a = TestPnCounter::default();
    a.p.insert(na, 10);
    a.n.insert(na, 2);
    let mut b = TestPnCounter::default();
    b.n.insert(nb, 5);

    a.crdt_merge(b);

    assert_eq!(a.value(), 3); // (10 - 2) - 5
}

/// The empty counter is the merge identity element.
#[test]
fn pn_counter_empty_is_identity() {
    let mut a = TestPnCounter::default();
    a.p.insert(Uuid::new_v4(), 7);

    let mut with_empty = a.clone();
    with_empty.crdt_merge(TestPnCounter::default());
    assert_eq!(with_empty, a);

    let mut empty = TestPnCounter::default();
    empty.crdt_merge(a.clone());
    assert_eq!(empty, a);
}

/// merge_into ignores a boxed component of the wrong type (logs an error
/// instead of panicking or inserting).
#[test]
fn merge_into_wrong_type_is_ignored() {
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    let mut entity_mut = world.get_entity_mut(entity).unwrap();

    merge_into::<TestPnCounter>(&mut entity_mut, Box::new(TestLwwValue { value: 1 }));

    assert!(entity_mut.get::<TestPnCounter>().is_none());
}

/// insert_or_merge_component falls back to wholesale insert for non-merge
/// types, and ignores unknown discriminants without panicking.
#[test]
fn insert_or_merge_dispatch() {
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    let mut entity_mut = world.get_entity_mut(entity).unwrap();

    libmarathon::networking::insert_or_merge_component(
        registry(),
        &mut entity_mut,
        discriminant_of::<TestLwwValue>(),
        Box::new(TestLwwValue { value: 9 }),
    );
    assert_eq!(entity_mut.get::<TestLwwValue>().unwrap().value, 9);

    libmarathon::networking::insert_or_merge_component(
        registry(),
        &mut entity_mut,
        u16::MAX,
        Box::new(TestLwwValue { value: 0 }),
    );
}

/// A merge component applies remote state even when the remote clock is
/// strictly OLDER — a CvRDT merge of an older state cannot lose data, so the
/// gate's KeepLocal decision is wrong to make for these types.
#[test]
fn merge_component_applies_older_remote() {
    let mut world = world_with_resources();
    let entity_id = Uuid::new_v4();
    let (local_node, remote_node) = ordered_nodes();

    let mut local_counter = TestPnCounter::default();
    local_counter.p.insert(local_node, 5);
    spawn_networked(&mut world, entity_id, local_node, local_counter);

    // Local clock is strictly AHEAD of the incoming clock
    let mut local_clock = VectorClock::new();
    local_clock.increment(local_node);
    local_clock.increment(local_node);
    let mut remote_clock = VectorClock::new();
    remote_clock.increment(remote_node);
    world.resource_mut::<ComponentVectorClocks>().set(
        entity_id,
        "TestPnCounter".to_string(),
        local_clock,
        local_node,
    );

    let mut remote_counter = TestPnCounter::default();
    remote_counter.p.insert(remote_node, 3);
    let delta = set_delta(
        entity_id,
        remote_node,
        &remote_clock,
        discriminant_of::<TestPnCounter>(),
        counter_bytes(&remote_counter),
    );
    apply_entity_delta(&delta, &mut world);

    assert_eq!(counter_value(&mut world, entity_id), Some(8));
}

/// Control: equal clocks on a non-merge component are a no-op.
#[test]
fn lww_component_ignores_equal_clock() {
    let mut world = world_with_resources();
    let entity_id = Uuid::new_v4();
    let node = Uuid::new_v4();

    let entity = world
        .spawn((
            NetworkedEntity::with_id(entity_id, node),
            TestLwwValue { value: 5 },
            Synced,
        ))
        .id();
    world
        .resource_mut::<NetworkEntityMap>()
        .insert(entity_id, entity);

    let mut clock = VectorClock::new();
    clock.increment(node);
    world.resource_mut::<ComponentVectorClocks>().set(
        entity_id,
        "TestLwwValue".to_string(),
        clock.clone(),
        node,
    );

    let delta = set_delta(
        entity_id,
        node,
        &clock,
        discriminant_of::<TestLwwValue>(),
        lww_bytes(&TestLwwValue { value: 9 }),
    );
    apply_entity_delta(&delta, &mut world);

    assert_eq!(lww_value(&mut world, entity_id), Some(5));
}

/// A merge delta must not resurrect a tombstoned entity.
#[test]
fn merge_delta_does_not_resurrect_tombstoned_entity() {
    use libmarathon::networking::TombstoneRegistry;

    let mut world = world_with_resources();
    world.insert_resource(TombstoneRegistry::default());
    let entity_id = Uuid::new_v4();
    let node = Uuid::new_v4();

    // Deletion clock strictly dominates the incoming delta's clock
    let mut deletion_clock = VectorClock::new();
    deletion_clock.increment(node);
    deletion_clock.increment(node);
    world
        .resource_mut::<TombstoneRegistry>()
        .record_deletion(entity_id, node, deletion_clock);

    let mut delta_clock = VectorClock::new();
    delta_clock.increment(node);
    let mut counter = TestPnCounter::default();
    counter.p.insert(node, 3);
    let delta = set_delta(
        entity_id,
        node,
        &delta_clock,
        discriminant_of::<TestPnCounter>(),
        counter_bytes(&counter),
    );
    apply_entity_delta(&delta, &mut world);

    assert_eq!(counter_value(&mut world, entity_id), None);
}

/// Operations for one entity never touch another entity's state or clocks.
#[test]
fn merge_deltas_are_isolated_per_entity() {
    let mut world = world_with_resources();
    let (a_id, b_id) = (Uuid::new_v4(), Uuid::new_v4());
    let node = Uuid::new_v4();
    spawn_networked(&mut world, a_id, node, TestPnCounter::default());
    spawn_networked(&mut world, b_id, node, TestPnCounter::default());

    let mut clock = VectorClock::new();
    clock.increment(node);
    let mut counter = TestPnCounter::default();
    counter.p.insert(node, 3);
    let delta = set_delta(
        a_id,
        node,
        &clock,
        discriminant_of::<TestPnCounter>(),
        counter_bytes(&counter),
    );
    apply_entity_delta(&delta, &mut world);

    assert_eq!(counter_value(&mut world, a_id), Some(3));
    assert_eq!(counter_value(&mut world, b_id), Some(0));
    assert!(
        world
            .resource::<ComponentVectorClocks>()
            .get(b_id, "TestPnCounter")
            .is_none()
    );
}

/// A single delta carrying both a merge and a non-merge op applies each with
/// its own semantics.
#[test]
fn mixed_delta_applies_merge_and_lww_independently() {
    let mut world = world_with_resources();
    let entity_id = Uuid::new_v4();
    let (local_node, remote_node) = ordered_nodes(); // local wins tiebreak

    let mut local_counter = TestPnCounter::default();
    local_counter.p.insert(local_node, 5);
    let entity = world
        .spawn((
            NetworkedEntity::with_id(entity_id, local_node),
            local_counter,
            TestLwwValue { value: 5 },
            Synced,
        ))
        .id();
    world
        .resource_mut::<NetworkEntityMap>()
        .insert(entity_id, entity);

    let mut local_clock = VectorClock::new();
    local_clock.increment(local_node);
    let mut remote_clock = VectorClock::new();
    remote_clock.increment(remote_node);
    {
        let mut clocks = world.resource_mut::<ComponentVectorClocks>();
        clocks.set(
            entity_id,
            "TestPnCounter".to_string(),
            local_clock.clone(),
            local_node,
        );
        clocks.set(
            entity_id,
            "TestLwwValue".to_string(),
            local_clock,
            local_node,
        );
    }

    let mut remote_counter = TestPnCounter::default();
    remote_counter.p.insert(remote_node, 3);
    let delta = EntityDelta::new(
        entity_id,
        remote_node,
        remote_clock.clone(),
        vec![
            ComponentOp::Set {
                discriminant: discriminant_of::<TestPnCounter>(),
                data: ComponentData::Inline(counter_bytes(&remote_counter)),
                vector_clock: remote_clock.clone(),
            },
            ComponentOp::Set {
                discriminant: discriminant_of::<TestLwwValue>(),
                data: ComponentData::Inline(lww_bytes(&TestLwwValue { value: 3 })),
                vector_clock: remote_clock,
            },
        ],
    );
    apply_entity_delta(&delta, &mut world);

    assert_eq!(counter_value(&mut world, entity_id), Some(8)); // merged
    assert_eq!(lww_value(&mut world, entity_id), Some(5)); // gate held
}

/// Components registered through `register_component!` (which has no merge
/// support) report no merge function.
#[test]
fn macro_registered_components_have_no_merge_fn() {
    assert!(
        registry()
            .get_merge_fn(discriminant_of::<MacroRegistered>())
            .is_none()
    );
}

/// FullState for an unknown entity spawns it with the merge component present.
#[test]
fn full_state_spawns_unknown_entity_with_crdt_state() {
    let mut world = world_with_resources();
    let entity_id = Uuid::new_v4();
    let owner = Uuid::new_v4();

    let mut remote_counter = TestPnCounter::default();
    remote_counter.p.insert(owner, 4);
    let state = EntityState {
        entity_id,
        owner_node_id: owner,
        vector_clock: VectorClock::new(),
        components: vec![ComponentState {
            discriminant: discriminant_of::<TestPnCounter>(),
            data: ComponentData::Inline(counter_bytes(&remote_counter)),
        }],
        is_deleted: false,
    };

    apply_full_state(vec![state], VectorClock::new(), &mut world, registry());

    assert_eq!(counter_value(&mut world, entity_id), Some(4));
}

// ============================================================================
// 7. Persistence and multi-node convergence
// ============================================================================

fn component_exists_in_db(
    db_path: &std::path::Path,
    entity_id: Uuid,
    component_type: &str,
) -> Result<bool> {
    let conn = rusqlite::Connection::open(db_path)?;
    let exists: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM components WHERE entity_id = ?1 AND component_type = ?2",
        rusqlite::params![entity_id.as_bytes().as_slice(), component_type],
        |row| row.get(0),
    )?;
    Ok(exists)
}

/// A merge-registered component flows through the ordinary persistence
/// pipeline like any other component.
#[tokio::test(flavor = "multi_thread")]
async fn merged_entity_persists_to_sqlite() -> Result<()> {
    let ctx = TestContext::new();
    let node = Uuid::new_v4();
    let mut app = test_utils::create_test_app_maybe_offline(node, ctx.db_path(), None);

    let entity_id = Uuid::new_v4();
    let mut counter = TestPnCounter::default();
    counter.increment(node);
    let entity = app
        .world_mut()
        .spawn((
            NetworkedEntity::with_id(entity_id, node),
            counter,
            Persisted::with_id(entity_id),
            Synced,
        ))
        .id();
    if let Ok(mut entity_mut) = app.world_mut().get_entity_mut(entity) &&
        let Some(mut persisted) = entity_mut.get_mut::<Persisted>()
    {
        let _ = &mut *persisted;
    }

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        app.update();
        if component_exists_in_db(&ctx.db_path(), entity_id, "merge_hook_test::TestPnCounter")? {
            break;
        }
        if std::time::Instant::now() > deadline {
            anyhow::bail!("TestPnCounter was not persisted within the deadline");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    Ok(())
}

/// Three nodes edit the same counter concurrently; all three converge with
/// every contribution intact.
#[tokio::test(flavor = "multi_thread")]
async fn three_node_concurrent_edits_converge() -> Result<()> {
    let ctx1 = TestContext::new();
    let ctx2 = TestContext::new();
    let ctx3 = TestContext::new();

    let (_e1, _e2, _e3, _r1, _r2, _r3, bridge1, bridge2, bridge3) =
        test_utils::setup_gossip_trio().await?;
    let node1 = bridge1.node_id();
    let node2 = bridge2.node_id();
    let node3 = bridge3.node_id();

    let mut app1 = test_utils::create_test_app(node1, ctx1.db_path(), bridge1);
    let mut app2 = test_utils::create_test_app(node2, ctx2.db_path(), bridge2);
    let mut app3 = test_utils::create_test_app(node3, ctx3.db_path(), bridge3);

    // Node 1 spawns the entity with a single increment.
    let entity_id = Uuid::new_v4();
    let mut counter = TestPnCounter::default();
    counter.increment(node1);
    let spawned = app1
        .world_mut()
        .spawn((
            NetworkedEntity::with_id(entity_id, node1),
            counter,
            Persisted::with_id(entity_id),
            Synced,
        ))
        .id();
    if let Ok(mut entity_mut) = app1.world_mut().get_entity_mut(spawned) &&
        let Some(mut networked) = entity_mut.get_mut::<NetworkedEntity>()
    {
        let _ = &mut *networked;
    }

    test_utils::wait_for_sync(&mut app1, &mut app2, Duration::from_secs(15), |_, w2| {
        counter_value(w2, entity_id) == Some(1)
    })
    .await?;
    test_utils::wait_for_sync(&mut app1, &mut app3, Duration::from_secs(15), |_, w3| {
        counter_value(w3, entity_id) == Some(1)
    })
    .await?;

    // Concurrent edits on all three nodes: +2, +3, +4 (total 10 with the
    // initial increment).
    for (world, node, times) in [
        (app1.world_mut(), node1, 2),
        (app2.world_mut(), node2, 3),
        (app3.world_mut(), node3, 4),
    ] {
        let entity = {
            let map = world.resource::<NetworkEntityMap>();
            map.get_entity(entity_id).expect("entity present")
        };
        let mut entity_mut = world.get_entity_mut(entity).unwrap();
        if let Some(mut counter) = entity_mut.get_mut::<TestPnCounter>() {
            for _ in 0..times {
                counter.increment(node);
            }
        }
        if let Some(mut networked) = entity_mut.get_mut::<NetworkedEntity>() {
            let _ = &mut *networked;
        }
    }

    // Drive all three until they agree on 10 everywhere. Delta generation runs
    // in FixedPostUpdate, so plain app.update() is not enough.
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        test_utils::update_with_fixed(&mut app1);
        test_utils::update_with_fixed(&mut app2);
        test_utils::update_with_fixed(&mut app3);
        let converged = counter_value(app1.world_mut(), entity_id) == Some(10) &&
            counter_value(app2.world_mut(), entity_id) == Some(10) &&
            counter_value(app3.world_mut(), entity_id) == Some(10);
        if converged {
            break;
        }
        if std::time::Instant::now() > deadline {
            anyhow::bail!(
                "nodes did not converge: {:?}",
                [
                    counter_value(app1.world_mut(), entity_id),
                    counter_value(app2.world_mut(), entity_id),
                    counter_value(app3.world_mut(), entity_id),
                ]
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    for world in [app1.world_mut(), app2.world_mut(), app3.world_mut()] {
        assert_eq!(counter_contrib(world, entity_id, node1), 3);
        assert_eq!(counter_contrib(world, entity_id, node2), 3);
        assert_eq!(counter_contrib(world, entity_id, node3), 4);
    }

    Ok(())
}
