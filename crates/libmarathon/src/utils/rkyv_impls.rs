//! Custom rkyv implementations for external types
//!
//! This module provides rkyv serialization support for external types that
//! don't have native rkyv support, using wrapper types to comply with Rust's
//! orphan rules.

use rkyv::{
    Archive,
    Deserialize,
    Serialize,
};

/// Newtype wrapper for uuid::Uuid to provide rkyv support
///
/// Stores UUID as bytes [u8; 16] for rkyv compatibility.
/// Provides conversions to/from uuid::Uuid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Archive, Serialize, Deserialize)]
pub struct RkyvUuid([u8; 16]);

impl RkyvUuid {
    pub fn new(uuid: uuid::Uuid) -> Self {
        Self(*uuid.as_bytes())
    }

    pub fn as_uuid(&self) -> uuid::Uuid {
        uuid::Uuid::from_bytes(self.0)
    }

    pub fn into_uuid(self) -> uuid::Uuid {
        uuid::Uuid::from_bytes(self.0)
    }
}

impl From<uuid::Uuid> for RkyvUuid {
    fn from(uuid: uuid::Uuid) -> Self {
        Self::new(uuid)
    }
}

impl From<RkyvUuid> for uuid::Uuid {
    fn from(wrapper: RkyvUuid) -> Self {
        wrapper.into_uuid()
    }
}
