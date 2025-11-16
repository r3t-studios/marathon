//! Configuration for the persistence layer

use crate::persistence::error::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Default critical flush delay in milliseconds
const DEFAULT_CRITICAL_FLUSH_DELAY_MS: u64 = 1000;

/// Default maximum buffer operations before forced flush
const DEFAULT_MAX_BUFFER_OPERATIONS: usize = 1000;

/// Configuration for the persistence layer
#[derive(Debug, Clone, Serialize, Deserialize, bevy::prelude::Resource)]
pub struct PersistenceConfig {
    /// Base flush interval (may be adjusted by battery level)
    pub flush_interval_secs: u64,

    /// Max time to defer critical writes (entity creation, etc.)
    pub critical_flush_delay_ms: u64,

    /// WAL checkpoint interval
    pub checkpoint_interval_secs: u64,

    /// Max WAL size before forced checkpoint (in bytes)
    pub max_wal_size_bytes: usize,

    /// Maximum number of operations in write buffer before forcing flush
    pub max_buffer_operations: usize,

    /// Enable adaptive flushing based on battery
    pub battery_adaptive: bool,

    /// Battery tier configuration
    pub battery_tiers: BatteryTiers,

    /// Platform-specific settings
    #[serde(default)]
    pub platform: PlatformConfig,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            flush_interval_secs: 10,
            critical_flush_delay_ms: DEFAULT_CRITICAL_FLUSH_DELAY_MS,
            checkpoint_interval_secs: 30,
            max_wal_size_bytes: 5 * 1024 * 1024, // 5MB
            max_buffer_operations: DEFAULT_MAX_BUFFER_OPERATIONS,
            battery_adaptive: true,
            battery_tiers: BatteryTiers::default(),
            platform: PlatformConfig::default(),
        }
    }
}

impl PersistenceConfig {
    /// Get the flush interval based on battery status
    pub fn get_flush_interval(&self, battery_level: f32, is_charging: bool) -> Duration {
        if !self.battery_adaptive {
            return Duration::from_secs(self.flush_interval_secs);
        }

        let interval_secs = if is_charging {
            self.battery_tiers.charging
        } else if battery_level > 0.5 {
            self.battery_tiers.high
        } else if battery_level > 0.2 {
            self.battery_tiers.medium
        } else {
            self.battery_tiers.low
        };

        Duration::from_secs(interval_secs)
    }

    /// Get the critical flush delay
    pub fn get_critical_flush_delay(&self) -> Duration {
        Duration::from_millis(self.critical_flush_delay_ms)
    }

    /// Get the checkpoint interval
    pub fn get_checkpoint_interval(&self) -> Duration {
        Duration::from_secs(self.checkpoint_interval_secs)
    }
}

/// Battery tier flush intervals (in seconds)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryTiers {
    /// Flush interval when charging
    pub charging: u64,

    /// Flush interval when battery > 50%
    pub high: u64,

    /// Flush interval when battery 20-50%
    pub medium: u64,

    /// Flush interval when battery < 20%
    pub low: u64,
}

impl Default for BatteryTiers {
    fn default() -> Self {
        Self {
            charging: 5,
            high: 10,
            medium: 30,
            low: 60,
        }
    }
}

/// Platform-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlatformConfig {
    /// iOS-specific settings
    #[serde(default)]
    pub ios: IosConfig,
}

/// iOS-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IosConfig {
    /// How long to wait for background flush before giving up (seconds)
    pub background_flush_timeout_secs: u64,

    /// Flush interval when in low power mode (seconds)
    pub low_power_mode_interval_secs: u64,
}

impl Default for IosConfig {
    fn default() -> Self {
        Self {
            background_flush_timeout_secs: 5,
            low_power_mode_interval_secs: 60,
        }
    }
}

/// Load persistence configuration from a TOML string
///
/// Parses TOML configuration and validates all settings. Use this for
/// loading configuration from embedded strings or dynamic sources.
///
/// # Parameters
/// - `toml`: TOML-formatted configuration string
///
/// # Returns
/// - `Ok(PersistenceConfig)`: Parsed and validated configuration
/// - `Err`: If TOML is invalid or contains invalid values
///
/// # Example TOML
/// ```toml
/// flush_interval_secs = 10
/// battery_adaptive = true
/// [battery_tiers]
/// charging = 5
/// high = 10
/// ```
pub fn load_config_from_str(toml: &str) -> Result<PersistenceConfig> {
    Ok(toml::from_str(toml)?)
}

/// Load persistence configuration from a TOML file
///
/// Reads and parses a TOML configuration file. This is the recommended way
/// to load configuration for production use, allowing runtime configuration
/// changes without recompilation.
///
/// # Parameters
/// - `path`: Path to TOML configuration file
///
/// # Returns
/// - `Ok(PersistenceConfig)`: Loaded configuration
/// - `Err`: If file can't be read or TOML is invalid
///
/// # Examples
/// ```no_run
/// # use lib::persistence::*;
/// # fn example() -> Result<()> {
/// let config = load_config_from_file("persistence.toml")?;
/// # Ok(())
/// # }
/// ```
pub fn load_config_from_file(path: impl AsRef<std::path::Path>) -> Result<PersistenceConfig> {
    let content = std::fs::read_to_string(path)?;
    Ok(load_config_from_str(&content)?)
}

/// Serialize persistence configuration to a TOML string
///
/// Converts configuration to human-readable TOML format. Use this to
/// save configuration to files or display current settings.
///
/// # Parameters
/// - `config`: Configuration to serialize
///
/// # Returns
/// - `Ok(String)`: Pretty-printed TOML configuration
/// - `Err`: If serialization fails (rare)
pub fn save_config_to_str(config: &PersistenceConfig) -> Result<String> {
    Ok(toml::to_string_pretty(config)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PersistenceConfig::default();
        assert_eq!(config.flush_interval_secs, 10);
        assert_eq!(config.battery_adaptive, true);
    }

    #[test]
    fn test_battery_adaptive_intervals() {
        let config = PersistenceConfig::default();

        // Charging
        let interval = config.get_flush_interval(0.3, true);
        assert_eq!(interval, Duration::from_secs(5));

        // High battery
        let interval = config.get_flush_interval(0.8, false);
        assert_eq!(interval, Duration::from_secs(10));

        // Medium battery
        let interval = config.get_flush_interval(0.4, false);
        assert_eq!(interval, Duration::from_secs(30));

        // Low battery
        let interval = config.get_flush_interval(0.1, false);
        assert_eq!(interval, Duration::from_secs(60));
    }

    #[test]
    fn test_config_serialization() {
        let config = PersistenceConfig::default();
        let toml = save_config_to_str(&config).unwrap();
        let loaded = load_config_from_str(&toml).unwrap();

        assert_eq!(config.flush_interval_secs, loaded.flush_interval_secs);
        assert_eq!(config.battery_adaptive, loaded.battery_adaptive);
    }
}
