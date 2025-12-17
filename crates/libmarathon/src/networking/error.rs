//! Error types for the networking layer

use std::fmt;

/// Result type for networking operations
pub type Result<T> = std::result::Result<T, NetworkingError>;

/// Errors that can occur in the networking layer
#[derive(Debug)]
pub enum NetworkingError {
    /// Serialization error
    Serialization(String),

    /// Deserialization error
    Deserialization(String),

    /// Gossip error (iroh-gossip)
    Gossip(String),

    /// Blob transfer error (iroh-blobs)
    Blob(String),

    /// Entity not found in network map
    EntityNotFound(uuid::Uuid),

    /// Vector clock comparison failed
    VectorClockError(String),

    /// CRDT merge conflict
    MergeConflict(String),

    /// Invalid message format
    InvalidMessage(String),

    /// Authentication/security error
    SecurityError(String),

    /// Rate limit exceeded
    RateLimitExceeded,

    /// Other networking errors
    Other(String),
}

impl fmt::Display for NetworkingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | NetworkingError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            | NetworkingError::Deserialization(msg) => {
                write!(f, "Deserialization error: {}", msg)
            },
            | NetworkingError::Gossip(msg) => write!(f, "Gossip error: {}", msg),
            | NetworkingError::Blob(msg) => write!(f, "Blob transfer error: {}", msg),
            | NetworkingError::EntityNotFound(id) => write!(f, "Entity not found: {}", id),
            | NetworkingError::VectorClockError(msg) => write!(f, "Vector clock error: {}", msg),
            | NetworkingError::MergeConflict(msg) => write!(f, "CRDT merge conflict: {}", msg),
            | NetworkingError::InvalidMessage(msg) => write!(f, "Invalid message: {}", msg),
            | NetworkingError::SecurityError(msg) => write!(f, "Security error: {}", msg),
            | NetworkingError::RateLimitExceeded => write!(f, "Rate limit exceeded"),
            | NetworkingError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for NetworkingError {}

impl From<crate::persistence::PersistenceError> for NetworkingError {
    fn from(e: crate::persistence::PersistenceError) -> Self {
        NetworkingError::Other(format!("Persistence error: {}", e))
    }
}
