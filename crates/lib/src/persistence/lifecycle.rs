//! iOS lifecycle event handling for persistence
//!
//! This module provides event types and handlers for iOS application lifecycle
//! events that require immediate persistence (e.g., background suspension).
//!
//! # iOS Integration
//!
//! To integrate with iOS, wire up these handlers in your app delegate:
//!
//! ```swift
//! // In your iOS app delegate:
//! func applicationWillResignActive(_ application: UIApplication) {
//!     // Send AppLifecycleEvent::WillResignActive to Bevy
//! }
//!
//! func applicationDidEnterBackground(_ application: UIApplication) {
//!     // Send AppLifecycleEvent::DidEnterBackground to Bevy
//! }
//! ```

use crate::persistence::*;
use bevy::prelude::*;

/// Application lifecycle events that require persistence handling
///
/// These events are critical moments where data must be flushed immediately
/// to avoid data loss.
#[derive(Debug, Clone, Message)]
pub enum AppLifecycleEvent {
    /// Application will resign active (iOS: `applicationWillResignActive`)
    ///
    /// Sent when the app is about to move from active to inactive state.
    /// Example: incoming phone call, user switches to another app
    WillResignActive,

    /// Application did enter background (iOS: `applicationDidEnterBackground`)
    ///
    /// Sent when the app has moved to the background. The app has approximately
    /// 5 seconds to complete critical tasks before suspension.
    DidEnterBackground,

    /// Application will enter foreground (iOS: `applicationWillEnterForeground`)
    ///
    /// Sent when the app is about to enter the foreground (user returning to app).
    WillEnterForeground,

    /// Application did become active (iOS: `applicationDidBecomeActive`)
    ///
    /// Sent when the app has become active and is ready to receive user input.
    DidBecomeActive,

    /// Application will terminate (iOS: `applicationWillTerminate`)
    ///
    /// Sent when the app is about to terminate. Similar to shutdown but from OS.
    WillTerminate,
}

/// System to handle iOS lifecycle events and trigger immediate persistence
///
/// This system listens for lifecycle events and performs immediate flushes
/// when the app is backgrounding or terminating.
pub fn lifecycle_event_system(
    mut events: MessageReader<AppLifecycleEvent>,
    mut write_buffer: ResMut<WriteBufferResource>,
    db: Res<PersistenceDb>,
    mut metrics: ResMut<PersistenceMetrics>,
    mut health: ResMut<PersistenceHealth>,
    mut pending_tasks: ResMut<PendingFlushTasks>,
) {
    for event in events.read() {
        match event {
            AppLifecycleEvent::WillResignActive => {
                // App is becoming inactive - perform immediate flush
                info!("App will resign active - performing immediate flush");

                if let Err(e) = force_flush(&mut write_buffer, &db, &mut metrics) {
                    error!("Failed to flush on resign active: {}", e);
                    health.record_flush_failure();
                } else {
                    health.record_flush_success();
                }
            }

            AppLifecycleEvent::DidEnterBackground => {
                // App entered background - perform immediate flush and checkpoint
                info!("App entered background - performing immediate flush and checkpoint");

                // Force immediate flush
                if let Err(e) = force_flush(&mut write_buffer, &db, &mut metrics) {
                    error!("Failed to flush on background: {}", e);
                    health.record_flush_failure();
                } else {
                    health.record_flush_success();
                }

                // Also checkpoint the WAL to ensure durability
                let start = std::time::Instant::now();
                match db.lock() {
                    Ok(mut conn) => {
                        match checkpoint_wal(&mut conn, CheckpointMode::Passive) {
                            Ok(_) => {
                                let duration = start.elapsed();
                                metrics.record_checkpoint(duration);
                                health.record_checkpoint_success();
                                info!("Background checkpoint completed successfully");
                            }
                            Err(e) => {
                                error!("Failed to checkpoint on background: {}", e);
                                health.record_checkpoint_failure();
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to acquire database lock for checkpoint: {}", e);
                        health.record_checkpoint_failure();
                    }
                }
            }

            AppLifecycleEvent::WillTerminate => {
                // App will terminate - perform shutdown sequence
                warn!("App will terminate - performing shutdown sequence");

                if let Err(e) = shutdown_system(&mut write_buffer, &db, &mut metrics, Some(&mut pending_tasks)) {
                    error!("Failed to perform shutdown on terminate: {}", e);
                } else {
                    info!("Clean shutdown completed on terminate");
                }
            }

            AppLifecycleEvent::WillEnterForeground => {
                // App returning from background - no immediate action needed
                info!("App will enter foreground");
            }

            AppLifecycleEvent::DidBecomeActive => {
                // App became active - no immediate action needed
                info!("App did become active");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifecycle_event_creation() {
        let event = AppLifecycleEvent::WillResignActive;
        match event {
            AppLifecycleEvent::WillResignActive => {
                // Success
            }
            _ => panic!("Event type mismatch"),
        }
    }
}
