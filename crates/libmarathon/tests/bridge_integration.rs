//! Integration tests for EngineBridge command/event routing

use libmarathon::engine::{EngineBridge, EngineCommand, EngineCore, EngineEvent};
use libmarathon::networking::SessionId;
use std::time::Duration;
use tokio::time::timeout;

/// Test that commands sent from "Bevy side" reach the engine
#[tokio::test]
async fn test_command_routing() {
    let (bridge, handle) = EngineBridge::new();

    // Spawn engine in background
    let engine_handle = tokio::spawn(async move {
        // Run engine for a short time
        let core = EngineCore::new(handle, ":memory:");
        timeout(Duration::from_millis(100), core.run())
            .await
            .ok();
    });

    // Give engine time to start
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Send a command from "Bevy side"
    let session_id = SessionId::new();
    bridge.send_command(EngineCommand::StartNetworking {
        session_id: session_id.clone(),
    });

    // Give engine time to process
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Poll events
    let events = bridge.poll_events();

    // Verify we got a NetworkingStarted event
    assert!(!events.is_empty(), "Should receive at least one event");

    let has_networking_started = events.iter().any(|e| {
        matches!(
            e,
            EngineEvent::NetworkingStarted {
                session_id: sid,
                ..
            } if sid == &session_id
        )
    });

    assert!(
        has_networking_started,
        "Should receive NetworkingStarted event"
    );

    // Cleanup
    drop(bridge);
    let _ = engine_handle.await;
}

/// Test that events from engine reach "Bevy side"
#[tokio::test]
async fn test_event_routing() {
    let (bridge, handle) = EngineBridge::new();

    // Spawn engine
    let engine_handle = tokio::spawn(async move {
        let core = EngineCore::new(handle, ":memory:");
        timeout(Duration::from_millis(100), core.run())
            .await
            .ok();
    });

    tokio::time::sleep(Duration::from_millis(10)).await;

    // Send StartNetworking command
    let session_id = SessionId::new();
    bridge.send_command(EngineCommand::StartNetworking {
        session_id: session_id.clone(),
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Poll events multiple times to verify queue works
    let events1 = bridge.poll_events();
    let events2 = bridge.poll_events();

    assert!(!events1.is_empty(), "First poll should return events");
    assert!(
        events2.is_empty(),
        "Second poll should be empty (events already drained)"
    );

    // Cleanup
    drop(bridge);
    let _ = engine_handle.await;
}

/// Test full lifecycle: Start → Stop networking
#[tokio::test]
async fn test_networking_lifecycle() {
    let (bridge, handle) = EngineBridge::new();

    let engine_handle = tokio::spawn(async move {
        let core = EngineCore::new(handle, ":memory:");
        timeout(Duration::from_millis(200), core.run())
            .await
            .ok();
    });

    tokio::time::sleep(Duration::from_millis(10)).await;

    // Start networking
    let session_id = SessionId::new();
    bridge.send_command(EngineCommand::StartNetworking {
        session_id: session_id.clone(),
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let events = bridge.poll_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::NetworkingStarted { .. })),
        "Should receive NetworkingStarted"
    );

    // Stop networking
    bridge.send_command(EngineCommand::StopNetworking);

    tokio::time::sleep(Duration::from_millis(50)).await;

    let events = bridge.poll_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::NetworkingStopped)),
        "Should receive NetworkingStopped"
    );

    // Cleanup
    drop(bridge);
    let _ = engine_handle.await;
}

/// Test JoinSession command routing
#[tokio::test]
async fn test_join_session_routing() {
    let (bridge, handle) = EngineBridge::new();

    let engine_handle = tokio::spawn(async move {
        let core = EngineCore::new(handle, ":memory:");
        timeout(Duration::from_millis(200), core.run())
            .await
            .ok();
    });

    tokio::time::sleep(Duration::from_millis(10)).await;

    // Join a new session (should start networking)
    let session_id = SessionId::new();
    bridge.send_command(EngineCommand::JoinSession {
        session_id: session_id.clone(),
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let events = bridge.poll_events();
    assert!(
        events.iter().any(|e| {
            matches!(
                e,
                EngineEvent::NetworkingStarted {
                    session_id: sid,
                    ..
                } if sid == &session_id
            )
        }),
        "JoinSession should start networking"
    );

    // Cleanup
    drop(bridge);
    let _ = engine_handle.await;
}

/// Test that multiple commands are processed in order
#[tokio::test]
async fn test_command_ordering() {
    let (bridge, handle) = EngineBridge::new();

    let engine_handle = tokio::spawn(async move {
        let core = EngineCore::new(handle, ":memory:");
        timeout(Duration::from_millis(200), core.run())
            .await
            .ok();
    });

    tokio::time::sleep(Duration::from_millis(10)).await;

    // Send multiple commands
    let session1 = SessionId::new();
    let session2 = SessionId::new();

    bridge.send_command(EngineCommand::StartNetworking {
        session_id: session1.clone(),
    });
    bridge.send_command(EngineCommand::StopNetworking);
    bridge.send_command(EngineCommand::JoinSession {
        session_id: session2.clone(),
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let events = bridge.poll_events();

    // Should see: NetworkingStarted(session1), NetworkingStopped, NetworkingStarted(session2)
    let started_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, EngineEvent::NetworkingStarted { .. }))
        .collect();

    let stopped_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, EngineEvent::NetworkingStopped))
        .collect();

    assert_eq!(started_events.len(), 2, "Should have 2 NetworkingStarted events");
    assert_eq!(stopped_events.len(), 1, "Should have 1 NetworkingStopped event");

    // Cleanup
    drop(bridge);
    let _ = engine_handle.await;
}
