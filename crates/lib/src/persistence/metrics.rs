//! Metrics tracking for persistence layer

use std::time::Duration;

/// Metrics for monitoring persistence performance
#[derive(Debug, Clone, Default, bevy::prelude::Resource)]
pub struct PersistenceMetrics {
    // Write volume
    pub total_writes: u64,
    pub bytes_written: u64,

    // Timing
    pub flush_count: u64,
    pub total_flush_duration: Duration,
    pub checkpoint_count: u64,
    pub total_checkpoint_duration: Duration,

    // WAL health
    pub wal_size_bytes: u64,
    pub max_wal_size_bytes: u64,

    // Recovery
    pub crash_recovery_count: u64,
    pub clean_shutdown_count: u64,

    // Buffer stats
    pub max_buffer_size: usize,
    pub total_coalesced_ops: u64,
}

impl PersistenceMetrics {
    /// Record a flush operation
    pub fn record_flush(&mut self, operations: usize, duration: Duration, bytes_written: u64) {
        self.flush_count += 1;
        self.total_writes += operations as u64;
        self.total_flush_duration += duration;
        self.bytes_written += bytes_written;
    }

    /// Record a checkpoint operation
    pub fn record_checkpoint(&mut self, duration: Duration) {
        self.checkpoint_count += 1;
        self.total_checkpoint_duration += duration;
    }

    /// Update WAL size
    pub fn update_wal_size(&mut self, size: u64) {
        self.wal_size_bytes = size;
        if size > self.max_wal_size_bytes {
            self.max_wal_size_bytes = size;
        }
    }

    /// Record a crash recovery
    pub fn record_crash_recovery(&mut self) {
        self.crash_recovery_count += 1;
    }

    /// Record a clean shutdown
    pub fn record_clean_shutdown(&mut self) {
        self.clean_shutdown_count += 1;
    }

    /// Record buffer stats
    pub fn record_buffer_stats(&mut self, buffer_size: usize, coalesced: u64) {
        if buffer_size > self.max_buffer_size {
            self.max_buffer_size = buffer_size;
        }
        self.total_coalesced_ops += coalesced;
    }

    /// Get average flush duration
    pub fn avg_flush_duration(&self) -> Duration {
        if self.flush_count == 0 {
            Duration::from_secs(0)
        } else {
            self.total_flush_duration / self.flush_count as u32
        }
    }

    /// Get average checkpoint duration
    pub fn avg_checkpoint_duration(&self) -> Duration {
        if self.checkpoint_count == 0 {
            Duration::from_secs(0)
        } else {
            self.total_checkpoint_duration / self.checkpoint_count as u32
        }
    }

    /// Get crash recovery rate
    pub fn crash_recovery_rate(&self) -> f64 {
        let total = self.crash_recovery_count + self.clean_shutdown_count;
        if total == 0 {
            0.0
        } else {
            self.crash_recovery_count as f64 / total as f64
        }
    }

    /// Check if metrics indicate performance issues
    pub fn check_health(&self) -> Vec<HealthWarning> {
        let mut warnings = Vec::new();

        // Check flush duration
        if self.avg_flush_duration() > Duration::from_millis(50) {
            warnings.push(HealthWarning::SlowFlush(self.avg_flush_duration()));
        }

        // Check WAL size
        if self.wal_size_bytes > 5 * 1024 * 1024 {
            // 5MB
            warnings.push(HealthWarning::LargeWal(self.wal_size_bytes));
        }

        // Check crash rate
        if self.crash_recovery_rate() > 0.1 {
            warnings.push(HealthWarning::HighCrashRate(self.crash_recovery_rate()));
        }

        warnings
    }

    /// Reset all metrics
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Health warnings for persistence metrics
#[derive(Debug, Clone)]
pub enum HealthWarning {
    /// Flush operations are taking too long
    SlowFlush(Duration),

    /// WAL file is too large
    LargeWal(u64),

    /// High crash recovery rate
    HighCrashRate(f64),
}

impl std::fmt::Display for HealthWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthWarning::SlowFlush(duration) => {
                write!(
                    f,
                    "Flush duration ({:?}) exceeds 50ms threshold",
                    duration
                )
            }
            HealthWarning::LargeWal(size) => {
                write!(f, "WAL size ({} bytes) exceeds 5MB threshold", size)
            }
            HealthWarning::HighCrashRate(rate) => {
                write!(f, "Crash recovery rate ({:.1}%) exceeds 10% threshold", rate * 100.0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_recording() {
        let mut metrics = PersistenceMetrics::default();

        metrics.record_flush(10, Duration::from_millis(5), 1024);
        assert_eq!(metrics.flush_count, 1);
        assert_eq!(metrics.total_writes, 10);
        assert_eq!(metrics.bytes_written, 1024);

        metrics.record_checkpoint(Duration::from_millis(10));
        assert_eq!(metrics.checkpoint_count, 1);
    }

    #[test]
    fn test_average_calculations() {
        let mut metrics = PersistenceMetrics::default();

        metrics.record_flush(10, Duration::from_millis(10), 1024);
        metrics.record_flush(20, Duration::from_millis(20), 2048);

        assert_eq!(metrics.avg_flush_duration(), Duration::from_millis(15));
    }

    #[test]
    fn test_health_warnings() {
        let mut metrics = PersistenceMetrics::default();

        // Add slow flush
        metrics.record_flush(10, Duration::from_millis(100), 1024);

        let warnings = metrics.check_health();
        assert_eq!(warnings.len(), 1);
        assert!(matches!(warnings[0], HealthWarning::SlowFlush(_)));
    }

    #[test]
    fn test_crash_recovery_rate() {
        let mut metrics = PersistenceMetrics::default();

        metrics.record_crash_recovery();
        metrics.record_clean_shutdown();
        metrics.record_clean_shutdown();

        assert_eq!(metrics.crash_recovery_rate(), 1.0 / 3.0);
    }
}
