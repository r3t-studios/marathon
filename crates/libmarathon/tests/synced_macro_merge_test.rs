//! Integration tests for `#[synced(merge = ...)]` wiring.
//!
//! The macro path is the way applications register CRDT components, so this
//! verifies the full loop: attribute → registration → delta apply path merge.
//! End-to-end gossip convergence for merge components is covered in
//! merge_hook_test.rs.

use std::{
    any::TypeId,
    collections::HashMap,
};

use bevy::prelude::*;
use libmarathon::{
    networking::{
        ComponentData,
        ComponentOp,
        ComponentVectorClocks,
        CrdtMerge,
        EntityDelta,
        NetworkEntityMap,
        NetworkedEntity,
        NodeVectorClock,
        Synced,
        VectorClock,
        apply_entity_delta,
    },
    persistence,
};
use uuid::Uuid;

/// A CRDT tally registered entirely through the macro.
#[libmarathon_macros::synced(merge = libmarathon::networking::merge_into::<Tally>)]
struct Tally {
    counts: HashMap<Uuid, u64>,
}

impl CrdtMerge for Tally {
    fn crdt_merge(&mut self, remote: Self) {
        for (node, count) in remote.counts {
            let entry = self.counts.entry(node).or_insert(0);
            *entry = (*entry).max(count);
        }
    }
}

impl Tally {
    fn total(&self) -> u64 {
        self.counts.values().sum()
    }
}

/// A non-Copy type registered through the macro with default (LWW) semantics.
#[libmarathon_macros::synced]
struct Note {
    text: String,
    tags: Vec<String>,
}

fn registry() -> &'static persistence::ComponentTypeRegistry {
    persistence::component_registry()
}

fn discriminant_of<T: 'static>() -> u16 {
    registry()
        .get_discriminant(TypeId::of::<T>())
        .expect("component registered")
}

#[test]
fn macro_merge_type_registers_merge_fn() {
    assert!(
        registry()
            .get_merge_fn(discriminant_of::<Tally>())
            .is_some()
    );
}

#[test]
fn macro_default_type_has_no_merge_fn() {
    assert!(registry().get_merge_fn(discriminant_of::<Note>()).is_none());
}

/// The full loop: a macro-registered merge component merges a concurrent
/// remote Set instead of being gated by whole-component LWW.
#[test]
fn macro_merge_type_merges_through_delta_path() {
    let mut world = World::new();
    world.insert_resource(persistence::ComponentTypeRegistryResource::default());
    world.insert_resource(NodeVectorClock::new(Uuid::new_v4()));
    world.insert_resource(NetworkEntityMap::default());
    world.insert_resource(ComponentVectorClocks::new());

    let entity_id = Uuid::new_v4();
    // Ensure local loses the LWW tiebreak, so the gate would drop the op for
    // a non-merge type
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let (local_node, remote_node) = if a < b { (a, b) } else { (b, a) };

    let local = Tally {
        counts: HashMap::from([(local_node, 5)]),
    };

    let entity = world
        .spawn((
            NetworkedEntity::with_id(entity_id, local_node),
            local,
            Synced,
        ))
        .id();
    world
        .resource_mut::<NetworkEntityMap>()
        .insert(entity_id, entity);

    // Concurrent clocks, pre-seeded so the gate is exercised
    let mut local_clock = VectorClock::new();
    local_clock.increment(local_node);
    let mut remote_clock = VectorClock::new();
    remote_clock.increment(remote_node);
    world.resource_mut::<ComponentVectorClocks>().set(
        entity_id,
        "Tally".to_string(),
        local_clock,
        local_node,
    );

    let remote = Tally {
        counts: HashMap::from([(remote_node, 3)]),
    };
    let payload = bytes::Bytes::from(
        rkyv::to_bytes::<rkyv::rancor::Failure>(&remote)
            .expect("serialize")
            .to_vec(),
    );
    let delta = EntityDelta::new(
        entity_id,
        remote_node,
        remote_clock.clone(),
        vec![ComponentOp::Set {
            discriminant: discriminant_of::<Tally>(),
            data: ComponentData::Inline(payload),
            vector_clock: remote_clock,
        }],
    );

    apply_entity_delta(&delta, &mut world);

    let mut query = world.query::<(&NetworkedEntity, &Tally)>();
    let total = query
        .iter(&world)
        .find(|(ne, _)| ne.network_id == entity_id)
        .map(|(_, t)| t.total());
    assert_eq!(total, Some(8));
}

/// Non-Copy macro types round-trip through the registry's serialize and
/// deserialize functions.
#[test]
fn macro_non_copy_type_roundtrips_through_registry() {
    let note = Note {
        text: "water the ferns".into(),
        tags: vec!["plants".into(), "weekly".into()],
    };

    let bytes = rkyv::to_bytes::<rkyv::rancor::Failure>(&note).expect("serialize");
    let deserialize_fn = registry()
        .get_deserialize_fn(discriminant_of::<Note>())
        .expect("deserialize fn");
    let boxed = deserialize_fn(&bytes).expect("deserialize");
    let back = boxed.downcast::<Note>().expect("downcast");

    assert_eq!(back.text, "water the ferns");
    assert_eq!(back.tags, ["plants", "weekly"]);
}
