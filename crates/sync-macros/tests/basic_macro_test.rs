/// Basic tests for the Synced derive macro
use bevy::prelude::*;
use lib::networking::{
    ClockComparison,
    ComponentMergeDecision,
    SyncComponent,
    SyncStrategy,
    Synced,
};
use sync_macros::Synced as SyncedDerive;

// Test 1: Basic struct with LWW strategy compiles
#[derive(Component, Reflect, Clone, serde::Serialize, serde::Deserialize, Debug, PartialEq)]
#[reflect(Component)]
#[derive(SyncedDerive)]
#[sync(version = 1, strategy = "LastWriteWins")]
struct Health(f32);

#[test]
fn test_health_compiles() {
    let health = Health(100.0);
    assert_eq!(health.0, 100.0);
}

#[test]
fn test_health_serialization() {
    let health = Health(100.0);
    let bytes = health.serialize_sync().unwrap();
    let deserialized = Health::deserialize_sync(&bytes).unwrap();
    assert_eq!(health, deserialized);
}

#[test]
fn test_health_lww_merge_remote_newer() {
    let mut local = Health(50.0);
    let remote = Health(100.0);

    let decision = local.merge(remote, ClockComparison::RemoteNewer);
    assert_eq!(decision, ComponentMergeDecision::TookRemote);
    assert_eq!(local.0, 100.0);
}

#[test]
fn test_health_lww_merge_local_newer() {
    let mut local = Health(50.0);
    let remote = Health(100.0);

    let decision = local.merge(remote, ClockComparison::LocalNewer);
    assert_eq!(decision, ComponentMergeDecision::KeptLocal);
    assert_eq!(local.0, 50.0); // Local value kept
}

#[test]
fn test_health_lww_merge_concurrent() {
    let mut local = Health(50.0);
    let remote = Health(100.0);

    let decision = local.merge(remote, ClockComparison::Concurrent);
    // With concurrent, we use hash tiebreaker
    // Either TookRemote or KeptLocal depending on hash
    assert!(
        decision == ComponentMergeDecision::TookRemote ||
            decision == ComponentMergeDecision::KeptLocal
    );
}

// Test 2: Struct with multiple fields
#[derive(Component, Reflect, Clone, serde::Serialize, serde::Deserialize, Debug, PartialEq)]
#[reflect(Component)]
#[derive(SyncedDerive)]
#[sync(version = 1, strategy = "LastWriteWins")]
struct Position {
    x: f32,
    y: f32,
}

#[test]
fn test_position_compiles() {
    let pos = Position { x: 10.0, y: 20.0 };
    assert_eq!(pos.x, 10.0);
    assert_eq!(pos.y, 20.0);
}

#[test]
fn test_position_serialization() {
    let pos = Position { x: 10.0, y: 20.0 };
    let bytes = pos.serialize_sync().unwrap();
    let deserialized = Position::deserialize_sync(&bytes).unwrap();
    assert_eq!(pos, deserialized);
}

#[test]
fn test_position_merge() {
    let mut local = Position { x: 10.0, y: 20.0 };
    let remote = Position { x: 30.0, y: 40.0 };

    let decision = local.merge(remote, ClockComparison::RemoteNewer);
    assert_eq!(decision, ComponentMergeDecision::TookRemote);
    assert_eq!(local.x, 30.0);
    assert_eq!(local.y, 40.0);
}
