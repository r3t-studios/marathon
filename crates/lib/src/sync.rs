use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};

// Re-export the macros
pub use sync_macros::{synced, Synced};

// Re-export common CRDT types from the crdts library
pub use crdts::{
    ctx::ReadCtx,
    lwwreg::LWWReg,
    map::Map,
    orswot::Orswot,
    CmRDT, CvRDT,
};

pub type NodeId = String;

/// Transparent wrapper for synced values
///
/// This wraps any value with LWW semantics but allows you to use it like a normal value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedValue<T: Clone> {
    value: T,
    timestamp: DateTime<Utc>,
    node_id: NodeId,
}

impl<T: Clone> SyncedValue<T> {
    pub fn new(value: T, node_id: NodeId) -> Self {
        Self {
            value,
            timestamp: Utc::now(),
            node_id,
        }
    }

    pub fn get(&self) -> &T {
        &self.value
    }

    pub fn set(&mut self, value: T, node_id: NodeId) {
        self.value = value;
        self.timestamp = Utc::now();
        self.node_id = node_id;
    }

    pub fn apply_lww(&mut self, value: T, timestamp: DateTime<Utc>, node_id: NodeId) {
        if timestamp > self.timestamp || (timestamp == self.timestamp && node_id > self.node_id) {
            self.value = value;
            self.timestamp = timestamp;
            self.node_id = node_id;
        }
    }

    pub fn merge(&mut self, other: &Self) {
        self.apply_lww(other.value.clone(), other.timestamp, other.node_id.clone());
    }
}

// Allow transparent access to the inner value
impl<T: Clone> Deref for SyncedValue<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T: Clone> DerefMut for SyncedValue<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

/// Wrapper for a sync message that goes over gossip
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMessage<T> {
    /// Unique message ID
    pub message_id: String,
    /// Node that sent this
    pub node_id: NodeId,
    /// When it was sent
    pub timestamp: DateTime<Utc>,
    /// The actual sync operation
    pub operation: T,
}

impl<T: Serialize> SyncMessage<T> {
    pub fn new(node_id: NodeId, operation: T) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, Ordering::SeqCst);

        Self {
            message_id: format!("{}-{}-{}", node_id, Utc::now().timestamp_millis(), seq),
            node_id,
            timestamp: Utc::now(),
            operation,
        }
    }

    pub fn to_bytes(&self) -> anyhow::Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }
}

impl<T: for<'de> Deserialize<'de>> SyncMessage<T> {
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        Ok(serde_json::from_slice(bytes)?)
    }
}

/// Helper trait for types that can be synced
pub trait Syncable: Sized {
    type Operation: Serialize + for<'de> Deserialize<'de> + Clone;

    /// Apply a sync operation to this value
    fn apply_sync_op(&mut self, op: &Self::Operation);

    /// Get the node ID for this instance
    fn node_id(&self) -> &NodeId;

    /// Create a sync message for an operation
    fn create_sync_message(&self, op: Self::Operation) -> SyncMessage<Self::Operation> {
        SyncMessage::new(self.node_id().clone(), op)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synced_value() {
        let mut val = SyncedValue::new(42, "node1".to_string());
        assert_eq!(*val.get(), 42);

        val.set(100, "node1".to_string());
        assert_eq!(*val.get(), 100);

        // Test LWW semantics
        let old_time = Utc::now() - chrono::Duration::seconds(10);
        val.apply_lww(50, old_time, "node2".to_string());
        assert_eq!(*val.get(), 100); // Should not update with older timestamp
    }

    #[test]
    fn test_sync_message() {
        #[derive(Debug, Clone, Serialize, Deserialize)]
        struct TestOp {
            value: i32,
        }

        let op = TestOp { value: 42 };
        let msg = SyncMessage::new("node1".to_string(), op);

        let bytes = msg.to_bytes().unwrap();
        let decoded = SyncMessage::<TestOp>::from_bytes(&bytes).unwrap();

        assert_eq!(decoded.node_id, "node1");
        assert_eq!(decoded.operation.value, 42);
    }
}
