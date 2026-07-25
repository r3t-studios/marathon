//! Tests for per-type change detection (`ChangeDetectionMeta`, backlog #3).
//!
//! Before this, only `Transform` edits auto-synced; edits to any other synced
//! component were invisible to delta generation unless something manually
//! touched `NetworkedEntity`. Now `#[synced]` registers a per-type system that
//! marks the entity when its component changes.

mod test_utils;

use std::time::Duration;

use anyhow::Result;
use bevy::prelude::*;
use libmarathon::{
    networking::{
        ChangeDetectionMeta,
        NetworkEntityMap,
        NetworkedEntity,
        SkipNextDeltaGeneration,
        Synced,
    },
    persistence::Persisted,
};
use test_utils::TestContext;
use uuid::Uuid;

/// A plain synced component (LWW semantics — not the subject here).
#[libmarathon_macros::synced]
struct SyncMeter {
    level: u64,
}

fn detect_system() -> fn(&mut World) {
    inventory::iter::<ChangeDetectionMeta>()
        .find(|meta| meta.type_name == "SyncMeter")
        .map(|meta| meta.system)
        .expect("SyncMeter registered for change detection")
}

/// Records entities whose NetworkedEntity was marked changed during a
/// schedule run.
#[derive(Resource, Default)]
struct TouchedEntities(Vec<Entity>);

fn observe_touched(
    query: Query<Entity, Changed<NetworkedEntity>>,
    mut touched: ResMut<TouchedEntities>,
) {
    touched.0.extend(query.iter());
}

/// Build the detect schedule once: a persisted Schedule keeps each system's
/// state, so `Changed` filters track runs correctly (a fresh query per call
/// would have last_run = 0 and match everything).
fn detect_schedule() -> Schedule {
    let mut schedule = Schedule::default();
    schedule.add_systems((detect_system(), observe_touched).chain());
    schedule
}

/// Run one detect pass and return the entities it touched.
fn run_detect(schedule: &mut Schedule, world: &mut World) -> Vec<Entity> {
    world.insert_resource(TouchedEntities::default());
    schedule.run(world);
    std::mem::take(&mut world.resource_mut::<TouchedEntities>().0)
}

fn spawn_meter(world: &mut World, network_id: Uuid, node: Uuid, level: u64) -> Entity {
    world
        .spawn((
            NetworkedEntity::with_id(network_id, node),
            SyncMeter { level },
            Synced,
        ))
        .id()
}

// ============================================================================
// Registration
// ============================================================================

#[test]
fn synced_macro_registers_change_detection() {
    let names: Vec<&'static str> = inventory::iter::<ChangeDetectionMeta>()
        .map(|meta| meta.type_name)
        .collect();
    assert!(
        names.contains(&"SyncMeter"),
        "SyncMeter registered: {names:?}"
    );
}

// ============================================================================
// Unit behavior
// ============================================================================

/// Editing a synced component marks its entity for delta generation.
#[test]
fn component_edit_marks_entity() {
    let mut world = World::new();
    let entity = spawn_meter(&mut world, Uuid::new_v4(), Uuid::new_v4(), 1);
    let mut schedule = detect_schedule();

    // First pass sees the spawn; second pass with no edits sees nothing
    assert_eq!(run_detect(&mut schedule, &mut world), vec![entity]);
    assert!(
        run_detect(&mut schedule, &mut world).is_empty(),
        "no edits, no marks"
    );

    world
        .get_entity_mut(entity)
        .unwrap()
        .get_mut::<SyncMeter>()
        .unwrap()
        .level = 2;

    assert_eq!(run_detect(&mut schedule, &mut world), vec![entity]);
}

/// The SkipNextDeltaGeneration marker suppresses marking (echo prevention
/// after a remote apply).
#[test]
fn skip_marker_suppresses_marking() {
    let mut world = World::new();
    let entity = spawn_meter(&mut world, Uuid::new_v4(), Uuid::new_v4(), 1);
    let mut schedule = detect_schedule();
    run_detect(&mut schedule, &mut world); // consume the spawn

    world
        .get_entity_mut(entity)
        .unwrap()
        .get_mut::<SyncMeter>()
        .unwrap()
        .level = 2;
    world
        .get_entity_mut(entity)
        .unwrap()
        .insert(SkipNextDeltaGeneration);

    assert!(run_detect(&mut schedule, &mut world).is_empty());
}

/// Entities without NetworkedEntity are ignored without panicking.
#[test]
fn untracked_entities_are_ignored() {
    let mut world = World::new();
    let entity = world.spawn(SyncMeter { level: 1 }).id();
    let mut schedule = detect_schedule();
    assert!(run_detect(&mut schedule, &mut world).is_empty());

    world
        .get_entity_mut(entity)
        .unwrap()
        .get_mut::<SyncMeter>()
        .unwrap()
        .level = 2;

    assert!(run_detect(&mut schedule, &mut world).is_empty());
}

/// A change on one entity does not mark its neighbor.
#[test]
fn marks_are_per_entity() {
    let mut world = World::new();
    let a = spawn_meter(&mut world, Uuid::new_v4(), Uuid::new_v4(), 1);
    let b = spawn_meter(&mut world, Uuid::new_v4(), Uuid::new_v4(), 1);
    let mut schedule = detect_schedule();
    let mut first = run_detect(&mut schedule, &mut world);
    first.sort();
    assert_eq!(first, {
        let mut expected = vec![a, b];
        expected.sort();
        expected
    });

    world
        .get_entity_mut(a)
        .unwrap()
        .get_mut::<SyncMeter>()
        .unwrap()
        .level = 2;

    assert_eq!(run_detect(&mut schedule, &mut world), vec![a]);
}

// ============================================================================
// End to end over gossip
// ============================================================================

/// The headline regression test: editing a non-Transform synced component
/// (with no manual NetworkedEntity touch) must reach the peer. On mainline
/// this edit was silently never broadcast.
#[tokio::test(flavor = "multi_thread")]
async fn component_edit_reaches_peer_without_manual_touch() -> Result<()> {
    let ctx1 = TestContext::new();
    let ctx2 = TestContext::new();

    let (_ep1, _ep2, _r1, _r2, bridge1, bridge2) = test_utils::setup_gossip_pair().await?;
    let node1 = bridge1.node_id();
    let node2 = bridge2.node_id();

    let mut app1 = test_utils::create_test_app(node1, ctx1.db_path(), bridge1);
    let mut app2 = test_utils::create_test_app(node2, ctx2.db_path(), bridge2);

    let entity_id = Uuid::new_v4();
    app1.world_mut().spawn((
        NetworkedEntity::with_id(entity_id, node1),
        SyncMeter { level: 1 },
        Persisted::with_id(entity_id),
        Synced,
    ));

    let meter_level = |world: &mut World, id: Uuid| -> Option<u64> {
        let mut query = world.query::<(&NetworkedEntity, &SyncMeter)>();
        query
            .iter(world)
            .find(|(ne, _)| ne.network_id == id)
            .map(|(_, m)| m.level)
    };

    test_utils::wait_for_sync(&mut app1, &mut app2, Duration::from_secs(15), |_, w2| {
        meter_level(w2, entity_id) == Some(1)
    })
    .await?;

    // Edit the component only — no NetworkedEntity touch anywhere.
    let entity = {
        let map = app1.world().resource::<NetworkEntityMap>();
        map.get_entity(entity_id).expect("entity present")
    };
    app1.world_mut()
        .get_entity_mut(entity)
        .unwrap()
        .get_mut::<SyncMeter>()
        .unwrap()
        .level = 2;

    // The peer must see the edit, driven purely by automatic change detection.
    test_utils::wait_for_sync(&mut app1, &mut app2, Duration::from_secs(15), |_, w2| {
        meter_level(w2, entity_id) == Some(2)
    })
    .await?;

    Ok(())
}
