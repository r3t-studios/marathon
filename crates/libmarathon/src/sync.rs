use std::ops::Deref;

use chrono::{
    DateTime,
    Utc,
};
// Re-export common CRDT types from the crdts library
pub use crdts::{
    CmRDT,
    CvRDT,
    ctx::ReadCtx,
    lwwreg::LWWReg,
    map::Map,
    orswot::Orswot,
};
use serde::{
    Deserialize,
    Serialize,
};
// TODO: Re-export the Synced derive macro (not part of bevy_render_macros)
// pub use macros::Synced;

pub type NodeId = uuid::Uuid;

/// Transparent wrapper for synced values
///
/// This wraps any value with LWW semantics but allows you to use it like a
/// normal value
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
        // Only clone if we're actually going to use the values (when other is newer)
        if other.timestamp > self.timestamp ||
            (other.timestamp == self.timestamp && other.node_id > self.node_id)
        {
            self.value = other.value.clone();
            self.timestamp = other.timestamp;
            self.node_id = other.node_id; // UUID is Copy, no need to clone
        }
    }
}

// Allow transparent read-only access to the inner value
// Note: DerefMut is intentionally NOT implemented to preserve LWW semantics
// Use `.set()` method to update values, which properly updates timestamps
impl<T: Clone> Deref for SyncedValue<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
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
        use std::sync::atomic::{
            AtomicU64,
            Ordering,
        };
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
        SyncMessage::new(*self.node_id(), op) // UUID is Copy, dereference instead of clone
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synced_value() {
        let node1 = uuid::Uuid::new_v4();
        let mut val = SyncedValue::new(42, node1);
        assert_eq!(*val.get(), 42);

        val.set(100, node1);
        assert_eq!(*val.get(), 100);

        // Test LWW semantics
        let node2 = uuid::Uuid::new_v4();
        let old_time = Utc::now() - chrono::Duration::seconds(10);
        val.apply_lww(50, old_time, node2);
        assert_eq!(*val.get(), 100); // Should not update with older timestamp
    }

    #[test]
    fn test_sync_message() {
        #[derive(Debug, Clone, Serialize, Deserialize)]
        struct TestOp {
            value: i32,
        }

        let node1 = uuid::Uuid::new_v4();
        let op = TestOp { value: 42 };
        let msg = SyncMessage::new(node1, op);

        let bytes = msg.to_bytes().unwrap();
        let decoded = SyncMessage::<TestOp>::from_bytes(&bytes).unwrap();

        assert_eq!(decoded.node_id, node1);
        assert_eq!(decoded.operation.value, 42);
    }

    #[test]
    fn test_uuid_comparison() {
        let node1 = uuid::Uuid::from_u128(1);
        let node2 = uuid::Uuid::from_u128(2);

        println!("node1: {}", node1);
        println!("node2: {}", node2);
        println!("node2 > node1: {}", node2 > node1);

        assert!(node2 > node1, "UUID from_u128(2) should be > from_u128(1)");
    }

    #[test]
    fn test_lww_tiebreaker() {
        let node1 = uuid::Uuid::from_u128(1);
        let node2 = uuid::Uuid::from_u128(2);

        // Create SyncedValue FIRST, then capture a timestamp that's guaranteed to be
        // newer
        let mut lww = SyncedValue::new(100, node1);
        std::thread::sleep(std::time::Duration::from_millis(1)); // Ensure ts is after init
        let ts = Utc::now();

        // Apply update from node1 at timestamp ts
        lww.apply_lww(100, ts, node1);
        println!(
            "After node1 update: value={}, ts={:?}, node={}",
            lww.get(),
            lww.timestamp,
            lww.node_id
        );

        // Apply conflicting update from node2 at SAME timestamp
        lww.apply_lww(200, ts, node2);
        println!(
            "After node2 update: value={}, ts={:?}, node={}",
            lww.get(),
            lww.timestamp,
            lww.node_id
        );

        // node2 > node1, so value2 should win
        assert_eq!(*lww.get(), 200, "Higher node_id should win tiebreaker");
    }
}
