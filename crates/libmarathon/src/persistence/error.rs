//! Error types for the persistence layer

use std::fmt;

/// Result type for persistence operations
pub type Result<T> = std::result::Result<T, PersistenceError>;

/// Errors that can occur in the persistence layer
#[derive(Debug)]
pub enum PersistenceError {
    /// Database operation failed
    Database(rusqlite::Error),

    /// Serialization failed
    Serialization(String),

    /// Deserialization failed
    Deserialization(String),

    /// Configuration error
    Config(String),

    /// I/O error (file operations, WAL checks, etc.)
    Io(std::io::Error),

    /// Type not found in registry
    TypeNotRegistered(String),

    /// Entity or component not found
    NotFound(String),

    /// Circuit breaker is open, operation blocked
    CircuitBreakerOpen {
        consecutive_failures: u32,
        retry_after_secs: u64,
    },

    /// Component data exceeds maximum size
    ComponentTooLarge {
        component_type: String,
        size_bytes: usize,
        max_bytes: usize,
    },

    /// Other error
    Other(String),
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Database(err) => write!(f, "Database error: {}", err),
            | Self::Serialization(err) => write!(f, "Serialization error: {}", err),
            | Self::Deserialization(msg) => write!(f, "Deserialization error: {}", msg),
            | Self::Config(msg) => write!(f, "Configuration error: {}", msg),
            | Self::Io(err) => write!(f, "I/O error: {}", err),
            | Self::TypeNotRegistered(type_name) => {
                write!(f, "Type not registered in type registry: {}", type_name)
            },
            | Self::NotFound(msg) => write!(f, "Not found: {}", msg),
            | Self::CircuitBreakerOpen {
                consecutive_failures,
                retry_after_secs,
            } => write!(
                f,
                "Circuit breaker open after {} consecutive failures, retry after {} seconds",
                consecutive_failures, retry_after_secs
            ),
            | Self::ComponentTooLarge {
                component_type,
                size_bytes,
                max_bytes,
            } => write!(
                f,
                "Component '{}' size ({} bytes) exceeds maximum ({} bytes). \
                This may indicate unbounded data growth or serialization issues.",
                component_type, size_bytes, max_bytes
            ),
            | Self::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for PersistenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            | Self::Database(err) => Some(err),
            | Self::Io(err) => Some(err),
            | _ => None,
        }
    }
}

// Conversions from common error types
impl From<rusqlite::Error> for PersistenceError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Database(err)
    }
}

impl From<std::io::Error> for PersistenceError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<toml::de::Error> for PersistenceError {
    fn from(err: toml::de::Error) -> Self {
        Self::Config(err.to_string())
    }
}

impl From<toml::ser::Error> for PersistenceError {
    fn from(err: toml::ser::Error) -> Self {
        Self::Config(err.to_string())
    }
}
