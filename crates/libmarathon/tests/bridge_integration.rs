//! Integration tests for EngineBridge command/event routing

use std::time::Duration;

use libmarathon::{
    engine::{
        EngineBridge,
        EngineCommand,
        EngineCore,
        EngineEvent,
    },
    networking::SessionId,
};
use tokio::time::timeout;

/// Get appropriate timeout for engine operations
/// - With fast_tests: short timeout (networking is mocked)
/// - Without fast_tests: long timeout (real networking with DHT discovery)
fn engine_timeout() -> Duration {
    #[cfg(feature = "fast_tests")]
    {
        Duration::from_millis(200)
    }
    #[cfg(not(feature = "fast_tests"))]
    {
        Duration::from_secs(30)
    }
}

/// Get appropriate wait time for command processing
fn processing_delay() -> Duration {
    #[cfg(feature = "fast_tests")]
    {
        Duration::from_millis(50)
    }
    #[cfg(not(feature = "fast_tests"))]
    {
        // Real networking needs more time for initialization
        Duration::from_secs(20)
    }
}

/// Test that commands sent from "Bevy side" reach the engine
#[tokio::test]
async fn test_command_routing() {
    let (bridge, handle) = EngineBridge::new();

    // Spawn engine in background
    let engine_handle = tokio::spawn(async move {
        // Run engine for a short time
        let core = EngineCore::new(handle, ":memory:");
        timeout(engine_timeout(), core.run()).await.ok();
    });

    // Give engine time to start
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Send a command from "Bevy side"
    let session_id = SessionId::new();
    bridge.send_command(EngineCommand::StartNetworking {
        session_id: session_id.clone(),
    });

    // Give engine time to process
    tokio::time::sleep(processing_delay()).await;

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
        timeout(engine_timeout(), core.run()).await.ok();
    });

    tokio::time::sleep(Duration::from_millis(10)).await;

    // Send StartNetworking command
    let session_id = SessionId::new();
    bridge.send_command(EngineCommand::StartNetworking {
        session_id: session_id.clone(),
    });

    tokio::time::sleep(processing_delay()).await;

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
        timeout(engine_timeout(), core.run()).await.ok();
    });

    tokio::time::sleep(Duration::from_millis(10)).await;

    // Start networking
    let session_id = SessionId::new();
    bridge.send_command(EngineCommand::StartNetworking {
        session_id: session_id.clone(),
    });

    tokio::time::sleep(processing_delay()).await;

    let events = bridge.poll_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::NetworkingStarted { .. })),
        "Should receive NetworkingStarted"
    );

    // Stop networking
    bridge.send_command(EngineCommand::StopNetworking);

    tokio::time::sleep(processing_delay()).await;

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
        timeout(engine_timeout(), core.run()).await.ok();
    });

    tokio::time::sleep(Duration::from_millis(10)).await;

    // Join a new session (should start networking)
    let session_id = SessionId::new();
    bridge.send_command(EngineCommand::JoinSession {
        session_id: session_id.clone(),
    });

    tokio::time::sleep(processing_delay()).await;

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
        timeout(engine_timeout(), core.run()).await.ok();
    });

    tokio::time::sleep(Duration::from_millis(10)).await;

    // Send first command and wait for it to complete
    let session1 = SessionId::new();
    bridge.send_command(EngineCommand::StartNetworking {
        session_id: session1.clone(),
    });

    // Wait for first networking to start
    tokio::time::sleep(processing_delay()).await;

    let events1 = bridge.poll_events();
    assert!(
        events1
            .iter()
            .any(|e| matches!(e, EngineEvent::NetworkingStarted { .. })),
        "Should receive first NetworkingStarted"
    );

    // Now send stop and start second session
    let session2 = SessionId::new();
    bridge.send_command(EngineCommand::StopNetworking);
    bridge.send_command(EngineCommand::JoinSession {
        session_id: session2.clone(),
    });

    // Wait for second networking to start
    tokio::time::sleep(processing_delay()).await;

    let events2 = bridge.poll_events();

    // Should see: NetworkingStopped, NetworkingStarted(session2)
    let started_events: Vec<_> = events2
        .iter()
        .filter(|e| matches!(e, EngineEvent::NetworkingStarted { .. }))
        .collect();

    let stopped_events: Vec<_> = events2
        .iter()
        .filter(|e| matches!(e, EngineEvent::NetworkingStopped))
        .collect();

    assert_eq!(
        started_events.len(),
        1,
        "Should have 1 NetworkingStarted event in second batch"
    );
    assert_eq!(
        stopped_events.len(),
        1,
        "Should have 1 NetworkingStopped event"
    );

    // Cleanup
    drop(bridge);
    let _ = engine_handle.await;
}

/// Test: Shutdown command causes EngineCore to exit gracefully
#[tokio::test]
async fn test_shutdown_command() {
    let (bridge, handle) = EngineBridge::new();

    let engine_handle = tokio::spawn(async move {
        let core = EngineCore::new(handle, ":memory:");
        core.run().await;
    });

    tokio::time::sleep(Duration::from_millis(10)).await;

    // Send Shutdown command
    bridge.send_command(EngineCommand::Shutdown);

    // Wait for engine to exit (should be quick since it's just processing the
    // command)
    let result = timeout(Duration::from_millis(100), engine_handle).await;

    assert!(
        result.is_ok(),
        "Engine should exit within 100ms after receiving Shutdown command"
    );

    // Verify that the engine actually exited (not errored)
    assert!(
        result.unwrap().is_ok(),
        "Engine should exit cleanly without panic"
    );
}
