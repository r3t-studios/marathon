//! Bevy systems for the persistence layer
//!
//! This module provides systems that integrate the persistence layer with
//! Bevy's ECS. These systems handle dirty tracking, write buffering, and
//! flushing to SQLite.

use std::{
    sync::{
        Arc,
        Mutex,
    },
    time::Instant,
};

use bevy::{
    prelude::*,
    tasks::{
        IoTaskPool,
        Task,
    },
};
use futures_lite::future;
use rusqlite::Connection;

use crate::persistence::{
    error::Result,
    *,
};

/// Resource wrapping the SQLite connection
#[derive(Clone, bevy::prelude::Resource)]
pub struct PersistenceDb {
    pub conn: Arc<Mutex<Connection>>,
}

impl PersistenceDb {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    pub fn from_path(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let conn = initialize_persistence_db(path)?;
        Ok(Self::new(conn))
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        configure_sqlite_for_persistence(&conn)?;
        create_persistence_schema(&conn)?;
        Ok(Self::new(conn))
    }

    /// Acquire the database connection with proper error handling
    ///
    /// Handles mutex poisoning gracefully by converting to PersistenceError.
    /// If a thread panics while holding the mutex, subsequent lock attempts
    /// will fail with a poisoned error, which this method converts to a
    /// recoverable error instead of panicking.
    ///
    /// # Returns
    /// - `Ok(MutexGuard<Connection>)`: Locked connection ready for use
    /// - `Err(PersistenceError)`: If mutex is poisoned
    pub fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|e| {
            PersistenceError::Other(format!("Database connection mutex poisoned: {}", e))
        })
    }
}

/// Resource for tracking when the last checkpoint occurred
#[derive(Debug, bevy::prelude::Resource)]
pub struct CheckpointTimer {
    pub last_checkpoint: Instant,
}

impl Default for CheckpointTimer {
    fn default() -> Self {
        Self {
            last_checkpoint: Instant::now(),
        }
    }
}

/// Resource for tracking pending async flush tasks
#[derive(Default, bevy::prelude::Resource)]
pub struct PendingFlushTasks {
    pub tasks: Vec<Task<Result<FlushResult>>>,
}

/// Result of an async flush operation
#[derive(Debug, Clone)]
pub struct FlushResult {
    pub operations_count: usize,
    pub duration: std::time::Duration,
    pub bytes_written: u64,
}

/// Helper function to calculate total bytes written from operations
fn calculate_bytes_written(ops: &[PersistenceOp]) -> u64 {
    ops.iter()
        .map(|op| match op {
            | PersistenceOp::UpsertComponent { data, .. } => data.len() as u64,
            | PersistenceOp::LogOperation { operation, .. } => operation.len() as u64,
            | _ => 0,
        })
        .sum()
}

/// Helper function to perform a flush with metrics tracking (synchronous)
///
/// Used for critical operations like shutdown where we need to block
fn perform_flush_sync(
    ops: &[PersistenceOp],
    db: &PersistenceDb,
    metrics: &mut PersistenceMetrics,
) -> Result<()> {
    if ops.is_empty() {
        return Ok(());
    }

    let start = Instant::now();
    let count = {
        let mut conn = db.lock()?;
        flush_to_sqlite(ops, &mut conn)?
    };
    let duration = start.elapsed();

    let bytes_written = calculate_bytes_written(ops);
    metrics.record_flush(count, duration, bytes_written);

    Ok(())
}

/// Helper function to perform a flush asynchronously (for normal operations)
///
/// This runs on the I/O task pool to avoid blocking the main thread
fn perform_flush_async(ops: Vec<PersistenceOp>, db: PersistenceDb) -> Result<FlushResult> {
    if ops.is_empty() {
        return Ok(FlushResult {
            operations_count: 0,
            duration: std::time::Duration::ZERO,
            bytes_written: 0,
        });
    }

    let bytes_written = calculate_bytes_written(&ops);
    let start = Instant::now();

    let count = {
        let mut conn = db.lock()?;
        flush_to_sqlite(&ops, &mut conn)?
    };

    let duration = start.elapsed();

    Ok(FlushResult {
        operations_count: count,
        duration,
        bytes_written,
    })
}

/// System to flush the write buffer to SQLite asynchronously
///
/// This system runs on a schedule based on the configuration and battery
/// status. It spawns async tasks to avoid blocking the main thread and handles
/// errors gracefully.
///
/// The system also polls pending flush tasks and updates metrics when they
/// complete.
pub fn flush_system(
    mut write_buffer: ResMut<WriteBufferResource>,
    db: Res<PersistenceDb>,
    config: Res<PersistenceConfig>,
    battery: Res<BatteryStatus>,
    mut metrics: ResMut<PersistenceMetrics>,
    mut pending_tasks: ResMut<PendingFlushTasks>,
    mut health: ResMut<PersistenceHealth>,
    mut failure_events: MessageWriter<PersistenceFailureEvent>,
    mut recovery_events: MessageWriter<PersistenceRecoveryEvent>,
) {
    // First, poll and handle completed async flush tasks
    pending_tasks.tasks.retain_mut(|task| {
        if let Some(result) = future::block_on(future::poll_once(task)) {
            match result {
                | Ok(flush_result) => {
                    let previous_failures = health.consecutive_flush_failures;
                    health.record_flush_success();

                    // Update metrics
                    metrics.record_flush(
                        flush_result.operations_count,
                        flush_result.duration,
                        flush_result.bytes_written,
                    );

                    // Emit recovery event if we recovered from failures
                    if previous_failures > 0 {
                        recovery_events.write(PersistenceRecoveryEvent { previous_failures });
                    }
                },
                | Err(e) => {
                    health.record_flush_failure();

                    let error_msg = format!("{}", e);
                    error!(
                        "Async flush failed (attempt {}/{}): {}",
                        health.consecutive_flush_failures,
                        PersistenceHealth::CIRCUIT_BREAKER_THRESHOLD,
                        error_msg
                    );

                    // Emit failure event
                    failure_events.write(PersistenceFailureEvent {
                        error: error_msg,
                        consecutive_failures: health.consecutive_flush_failures,
                        circuit_breaker_open: health.circuit_breaker_open,
                    });
                },
            }
            false // Remove completed task
        } else {
            true // Keep pending task
        }
    });

    // Check circuit breaker before spawning new flush
    if !health.should_attempt_operation() {
        return;
    }

    let flush_interval = config.get_flush_interval(battery.level, battery.is_charging);

    // Check if we should flush
    if !write_buffer.should_flush(flush_interval) {
        return;
    }

    // Take operations from buffer
    let ops = write_buffer.take_operations();
    if ops.is_empty() {
        return;
    }

    // Spawn async flush task on I/O thread pool
    let task_pool = IoTaskPool::get();
    let db_clone = db.clone();

    let task = task_pool.spawn(async move { perform_flush_async(ops, db_clone.clone()) });

    pending_tasks.tasks.push(task);

    // Update last flush time
    write_buffer.last_flush = Instant::now();
}

/// System to checkpoint the WAL file
///
/// This runs less frequently than flush_system to merge the WAL into the main
/// database.
pub fn checkpoint_system(
    db: &PersistenceDb,
    config: &PersistenceConfig,
    timer: &mut CheckpointTimer,
    metrics: &mut PersistenceMetrics,
) -> Result<()> {
    let checkpoint_interval = config.get_checkpoint_interval();

    // Check if it's time to checkpoint
    if timer.last_checkpoint.elapsed() < checkpoint_interval {
        // Also check WAL size
        let wal_size = {
            let conn = db.lock()?;
            get_wal_size(&conn)?
        };

        metrics.update_wal_size(wal_size as u64);

        // Force checkpoint if WAL is too large
        if wal_size < config.max_wal_size_bytes as i64 {
            return Ok(());
        }
    }

    // Perform checkpoint
    let start = Instant::now();
    let info = {
        let mut conn = db.lock()?;
        checkpoint_wal(&mut conn, CheckpointMode::Passive)?
    };
    let duration = start.elapsed();

    // Update metrics
    metrics.record_checkpoint(duration);
    timer.last_checkpoint = Instant::now();

    // Log if checkpoint was busy
    if info.busy {
        tracing::warn!("WAL checkpoint was busy - some pages may not have been checkpointed");
    }

    Ok(())
}

/// System to handle application shutdown
///
/// This ensures a final flush and checkpoint before the application exits.
/// Uses synchronous flush to ensure all data is written before exit.
///
/// **CRITICAL**: Waits for all pending async flush tasks to complete before
/// proceeding with shutdown. This prevents data loss from in-flight operations.
pub fn shutdown_system(
    write_buffer: &mut WriteBuffer,
    db: &PersistenceDb,
    metrics: &mut PersistenceMetrics,
    pending_tasks: Option<&mut PendingFlushTasks>,
) -> Result<()> {
    // CRITICAL: Wait for all pending async flushes to complete
    // This prevents data loss from in-flight operations
    if let Some(pending) = pending_tasks {
        info!(
            "Waiting for {} pending flush tasks to complete before shutdown",
            pending.tasks.len()
        );

        for task in pending.tasks.drain(..) {
            // Block on each pending task to ensure completion
            match future::block_on(task) {
                | Ok(flush_result) => {
                    // Update metrics for completed flush
                    metrics.record_flush(
                        flush_result.operations_count,
                        flush_result.duration,
                        flush_result.bytes_written,
                    );
                    debug!(
                        "Pending flush completed: {} operations",
                        flush_result.operations_count
                    );
                },
                | Err(e) => {
                    error!("Pending flush failed during shutdown: {}", e);
                    // Continue with shutdown even if a task failed
                },
            }
        }

        info!("All pending flush tasks completed");
    }

    // Force flush any remaining operations (synchronous for shutdown)
    let ops = write_buffer.take_operations();
    perform_flush_sync(&ops, db, metrics)?;

    // Checkpoint the WAL
    let start = Instant::now();
    {
        let mut conn = db.lock()?;
        checkpoint_wal(&mut conn, CheckpointMode::Truncate)?;

        // Mark clean shutdown
        mark_clean_shutdown(&mut conn)?;
    }
    let duration = start.elapsed();
    metrics.record_checkpoint(duration);
    metrics.record_clean_shutdown();

    Ok(())
}

/// System to initialize persistence on startup
///
/// This checks for crash recovery and sets up the session.
pub fn startup_system(db: &PersistenceDb, metrics: &mut PersistenceMetrics) -> Result<()> {
    let mut conn = db.lock()?;

    // Check if previous session shut down cleanly
    let clean_shutdown = check_clean_shutdown(&mut conn)?;

    if !clean_shutdown {
        tracing::warn!("Previous session did not shut down cleanly - crash detected");
        metrics.record_crash_recovery();

        // Perform any necessary recovery operations here
        // For now, SQLite's WAL mode handles recovery automatically
    } else {
        tracing::info!("Previous session shut down cleanly");
    }

    // Set up new session
    let session = SessionState::new();
    set_session_state(&mut conn, "session_id", &session.session_id)?;

    Ok(())
}

/// Helper function to force an immediate flush (for critical operations)
///
/// Uses synchronous flush to ensure data is written immediately.
/// Suitable for critical operations like iOS background events.
pub fn force_flush(
    write_buffer: &mut WriteBuffer,
    db: &PersistenceDb,
    metrics: &mut PersistenceMetrics,
) -> Result<()> {
    let ops = write_buffer.take_operations();
    perform_flush_sync(&ops, db, metrics)?;
    write_buffer.last_flush = Instant::now();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_persistence_db_in_memory() -> Result<()> {
        let db = PersistenceDb::in_memory()?;

        // Verify we can write and read
        let entity_id = uuid::Uuid::new_v4();
        let ops = vec![PersistenceOp::UpsertEntity {
            id: entity_id,
            data: EntityData {
                id: entity_id,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                entity_type: "TestEntity".to_string(),
            },
        }];

        let mut conn = db.lock()?;
        flush_to_sqlite(&ops, &mut conn)?;

        Ok(())
    }

    #[test]
    fn test_flush_system() -> Result<()> {
        let db = PersistenceDb::in_memory()?;
        let mut write_buffer = WriteBuffer::new(1000);
        let mut metrics = PersistenceMetrics::default();

        // Add some operations
        let entity_id = uuid::Uuid::new_v4();

        // First add the entity
        write_buffer.add(PersistenceOp::UpsertEntity {
            id: entity_id,
            data: EntityData {
                id: entity_id,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                entity_type: "TestEntity".to_string(),
            },
        });

        // Then add a component
        write_buffer.add(PersistenceOp::UpsertComponent {
            entity_id,
            component_type: "Transform".to_string(),
            data: vec![1, 2, 3],
        });

        // Take operations and flush synchronously (testing the flush logic)
        let ops = write_buffer.take_operations();
        perform_flush_sync(&ops, &db, &mut metrics)?;

        assert_eq!(metrics.flush_count, 1);
        assert_eq!(write_buffer.len(), 0);

        Ok(())
    }
}
