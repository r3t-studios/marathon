//! Health monitoring and error recovery for persistence layer

use bevy::prelude::*;
use std::time::{Duration, Instant};

/// Base delay for exponential backoff in milliseconds
const BASE_RETRY_DELAY_MS: u64 = 1000; // 1 second

/// Maximum retry delay in milliseconds (caps exponential backoff)
const MAX_RETRY_DELAY_MS: u64 = 30000; // 30 seconds

/// Maximum exponent for exponential backoff calculation
const MAX_BACKOFF_EXPONENT: u32 = 5;

/// Resource to track persistence health and failures
#[derive(Resource, Debug)]
pub struct PersistenceHealth {
    /// Number of consecutive flush failures
    pub consecutive_flush_failures: u32,

    /// Number of consecutive checkpoint failures
    pub consecutive_checkpoint_failures: u32,

    /// Time of last successful flush
    pub last_successful_flush: Option<Instant>,

    /// Time of last successful checkpoint
    pub last_successful_checkpoint: Option<Instant>,

    /// Whether the persistence layer is in circuit breaker mode
    pub circuit_breaker_open: bool,

    /// When the circuit breaker was opened
    pub circuit_breaker_opened_at: Option<Instant>,

    /// Total number of failures across the session
    pub total_failures: u64,
}

impl Default for PersistenceHealth {
    fn default() -> Self {
        Self {
            consecutive_flush_failures: 0,
            consecutive_checkpoint_failures: 0,
            last_successful_flush: None,
            last_successful_checkpoint: None,
            circuit_breaker_open: false,
            circuit_breaker_opened_at: None,
            total_failures: 0,
        }
    }
}

impl PersistenceHealth {
    /// Circuit breaker threshold - open after this many consecutive failures
    pub const CIRCUIT_BREAKER_THRESHOLD: u32 = 5;

    /// How long to keep circuit breaker open before attempting recovery
    pub const CIRCUIT_BREAKER_COOLDOWN: Duration = Duration::from_secs(60);

    /// Record a successful flush
    pub fn record_flush_success(&mut self) {
        self.consecutive_flush_failures = 0;
        self.last_successful_flush = Some(Instant::now());

        // Close circuit breaker if it was open
        if self.circuit_breaker_open {
            info!("Persistence recovered - closing circuit breaker");
            self.circuit_breaker_open = false;
            self.circuit_breaker_opened_at = None;
        }
    }

    /// Record a flush failure
    pub fn record_flush_failure(&mut self) {
        self.consecutive_flush_failures += 1;
        self.total_failures += 1;

        if self.consecutive_flush_failures >= Self::CIRCUIT_BREAKER_THRESHOLD {
            if !self.circuit_breaker_open {
                warn!(
                    "Opening circuit breaker after {} consecutive flush failures",
                    self.consecutive_flush_failures
                );
                self.circuit_breaker_open = true;
                self.circuit_breaker_opened_at = Some(Instant::now());
            }
        }
    }

    /// Record a successful checkpoint
    pub fn record_checkpoint_success(&mut self) {
        self.consecutive_checkpoint_failures = 0;
        self.last_successful_checkpoint = Some(Instant::now());
    }

    /// Record a checkpoint failure
    pub fn record_checkpoint_failure(&mut self) {
        self.consecutive_checkpoint_failures += 1;
        self.total_failures += 1;
    }

    /// Check if we should attempt operations (circuit breaker state)
    ///
    /// **CRITICAL FIX**: Now takes `&mut self` to properly reset the circuit breaker
    /// after cooldown expires. This prevents the circuit breaker from remaining
    /// permanently open after one post-cooldown failure.
    pub fn should_attempt_operation(&mut self) -> bool {
        if !self.circuit_breaker_open {
            return true;
        }

        // Check if cooldown period has elapsed
        if let Some(opened_at) = self.circuit_breaker_opened_at {
            if opened_at.elapsed() >= Self::CIRCUIT_BREAKER_COOLDOWN {
                // Transition to half-open state by resetting the breaker
                info!("Circuit breaker cooldown elapsed - entering half-open state (testing recovery)");
                self.circuit_breaker_open = false;
                self.circuit_breaker_opened_at = None;
                // consecutive_flush_failures is kept to track if this probe succeeds
                return true;
            }
        }

        false
    }

    /// Get exponential backoff delay based on consecutive failures
    pub fn get_retry_delay(&self) -> Duration {
        // Exponential backoff: 1s, 2s, 4s, 8s, 16s, max 30s
        let delay_ms = BASE_RETRY_DELAY_MS * 2u64.pow(self.consecutive_flush_failures.min(MAX_BACKOFF_EXPONENT));
        Duration::from_millis(delay_ms.min(MAX_RETRY_DELAY_MS))
    }
}

/// Message emitted when persistence fails
#[derive(Message, Debug, Clone)]
pub struct PersistenceFailureEvent {
    pub error: String,
    pub consecutive_failures: u32,
    pub circuit_breaker_open: bool,
}

/// Message emitted when persistence recovers from failures
#[derive(Message, Debug, Clone)]
pub struct PersistenceRecoveryEvent {
    pub previous_failures: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker() {
        let mut health = PersistenceHealth::default();

        // Should allow operations initially
        assert!(health.should_attempt_operation());
        assert!(!health.circuit_breaker_open);

        // Record failures
        for _ in 0..PersistenceHealth::CIRCUIT_BREAKER_THRESHOLD {
            health.record_flush_failure();
        }

        // Circuit breaker should now be open
        assert!(health.circuit_breaker_open);
        assert!(!health.should_attempt_operation());

        // Should still block immediately after opening
        assert!(!health.should_attempt_operation());
    }

    #[test]
    fn test_recovery() {
        let mut health = PersistenceHealth::default();

        // Trigger circuit breaker
        for _ in 0..PersistenceHealth::CIRCUIT_BREAKER_THRESHOLD {
            health.record_flush_failure();
        }
        assert!(health.circuit_breaker_open);

        // Successful flush should close circuit breaker
        health.record_flush_success();
        assert!(!health.circuit_breaker_open);
        assert_eq!(health.consecutive_flush_failures, 0);
    }

    #[test]
    fn test_exponential_backoff() {
        let mut health = PersistenceHealth::default();

        // No failures = 1s delay
        assert_eq!(health.get_retry_delay(), Duration::from_secs(1));

        // 1 failure = 2s
        health.record_flush_failure();
        assert_eq!(health.get_retry_delay(), Duration::from_secs(2));

        // 2 failures = 4s
        health.record_flush_failure();
        assert_eq!(health.get_retry_delay(), Duration::from_secs(4));

        // Max out at 30s
        for _ in 0..10 {
            health.record_flush_failure();
        }
        assert_eq!(health.get_retry_delay(), Duration::from_secs(30));
    }
}
