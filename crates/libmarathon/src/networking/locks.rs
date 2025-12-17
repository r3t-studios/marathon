//! Entity lock system for collaborative editing
//!
//! Provides optimistic entity locking to prevent concurrent modifications.
//! Locks are acquired when entities are selected and released when deselected.
//!
//! # Lock Protocol
//!
//! 1. **Acquisition**: User selects entity → broadcast `LockRequest`
//! 2. **Optimistic Apply**: All peers apply lock locally
//! 3. **Confirm**: Holder broadcasts `LockAcquired`
//! 4. **Conflict Resolution**: If two nodes acquire simultaneously, higher node ID wins
//! 5. **Release**: User deselects entity → broadcast `LockReleased`
//! 6. **Timeout**: 5-second timeout as crash recovery fallback
//!
//! # Example
//!
//! ```no_run
//! use bevy::prelude::*;
//! use libmarathon::networking::{EntityLockRegistry, acquire_entity_lock, release_entity_lock};
//! use uuid::Uuid;
//!
//! fn my_system(world: &mut World) {
//!     let entity_id = Uuid::new_v4();
//!     let node_id = Uuid::new_v4();
//!
//!     let mut registry = world.resource_mut::<EntityLockRegistry>();
//!
//!     // Acquire lock when user selects entity
//!     registry.try_acquire(entity_id, node_id);
//!
//!     // Release lock when user deselects entity
//!     registry.release(entity_id, node_id);
//! }
//! ```

use std::{
    collections::HashMap,
    time::{
        Duration,
        Instant,
    },
};

use bevy::prelude::*;

use uuid::Uuid;

use crate::networking::{
    GossipBridge,
    NetworkedSelection,
    NodeId,
    VersionedMessage,
    delta_generation::NodeVectorClock,
    messages::SyncMessage,
};

/// Duration before a lock automatically expires (crash recovery)
pub const LOCK_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum number of concurrent locks per node (rate limiting)
pub const MAX_LOCKS_PER_NODE: usize = 100;

/// Lock acquisition/release messages
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, PartialEq, Eq)]
pub enum LockMessage {
    /// Request to acquire a lock on an entity
    LockRequest {
        entity_id: Uuid,
        node_id: NodeId,
    },

    /// Confirmation that a lock was successfully acquired
    LockAcquired {
        entity_id: Uuid,
        holder: NodeId,
    },

    /// Lock acquisition failed (already locked by another node)
    LockRejected {
        entity_id: Uuid,
        requester: NodeId,
        current_holder: NodeId,
    },

    /// Heartbeat to renew a held lock (sent ~1/sec by holder)
    ///
    /// If no heartbeat is received for 5 seconds, the lock expires.
    /// This provides automatic crash recovery without explicit timeouts.
    LockHeartbeat {
        entity_id: Uuid,
        holder: NodeId,
    },

    /// Request to release a lock
    LockRelease {
        entity_id: Uuid,
        node_id: NodeId,
    },

    /// Confirmation that a lock was released
    LockReleased {
        entity_id: Uuid,
    },
}

/// Information about an active entity lock
#[derive(Debug, Clone)]
pub struct EntityLock {
    /// ID of the entity being locked
    pub entity_id: Uuid,

    /// Node that holds the lock
    pub holder: NodeId,

    /// When the last heartbeat was received (or when lock was acquired)
    pub last_heartbeat: Instant,

    /// Lock timeout duration (expires if no heartbeat for this long)
    pub timeout: Duration,
}

impl EntityLock {
    /// Create a new entity lock
    pub fn new(entity_id: Uuid, holder: NodeId) -> Self {
        Self {
            entity_id,
            holder,
            last_heartbeat: Instant::now(),
            timeout: LOCK_TIMEOUT,
        }
    }

    /// Renew the lock with a heartbeat
    pub fn renew(&mut self) {
        self.last_heartbeat = Instant::now();
    }

    /// Check if the lock has expired (no heartbeat for > timeout)
    pub fn is_expired(&self) -> bool {
        self.last_heartbeat.elapsed() >= self.timeout
    }

    /// Check if this lock is held by the given node
    pub fn is_held_by(&self, node_id: NodeId) -> bool {
        self.holder == node_id
    }
}

/// Registry of all active entity locks
///
/// This resource tracks which entities are locked and by whom.
/// It's used to prevent concurrent modifications to the same entity.
#[derive(Resource, Default)]
pub struct EntityLockRegistry {
    /// Map of entity ID to lock info
    locks: HashMap<Uuid, EntityLock>,

    /// Count of locks held by each node (for rate limiting)
    locks_per_node: HashMap<NodeId, usize>,
}

impl EntityLockRegistry {
    /// Create a new empty lock registry
    pub fn new() -> Self {
        Self {
            locks: HashMap::new(),
            locks_per_node: HashMap::new(),
        }
    }

    /// Try to acquire a lock on an entity
    ///
    /// Returns Ok(()) if lock was acquired, Err with current holder if already locked.
    pub fn try_acquire(&mut self, entity_id: Uuid, node_id: NodeId) -> Result<(), NodeId> {
        // Check if already locked
        if let Some(existing_lock) = self.locks.get(&entity_id) {
            // If expired, allow re-acquisition
            if !existing_lock.is_expired() {
                return Err(existing_lock.holder);
            }

            // Remove expired lock
            self.remove_lock(entity_id);
        }

        // Check rate limit
        let node_lock_count = self.locks_per_node.get(&node_id).copied().unwrap_or(0);
        if node_lock_count >= MAX_LOCKS_PER_NODE {
            warn!(
                "Node {} at lock limit ({}/{}), rejecting acquisition",
                node_id, node_lock_count, MAX_LOCKS_PER_NODE
            );
            return Err(node_id); // Return self as "holder" to indicate rate limit
        }

        // Acquire the lock
        let lock = EntityLock::new(entity_id, node_id);
        self.locks.insert(entity_id, lock);

        // Update node lock count
        *self.locks_per_node.entry(node_id).or_insert(0) += 1;

        debug!("Lock acquired: entity {} by node {}", entity_id, node_id);
        Ok(())
    }

    /// Release a lock on an entity
    ///
    /// Only succeeds if the node currently holds the lock.
    pub fn release(&mut self, entity_id: Uuid, node_id: NodeId) -> bool {
        if let Some(lock) = self.locks.get(&entity_id) {
            if lock.holder == node_id {
                self.remove_lock(entity_id);
                debug!("Lock released: entity {} by node {}", entity_id, node_id);
                return true;
            } else {
                warn!(
                    "Node {} tried to release lock held by node {}",
                    node_id, lock.holder
                );
            }
        }
        false
    }

    /// Force release a lock (for timeout cleanup)
    pub fn force_release(&mut self, entity_id: Uuid) {
        if self.locks.remove(&entity_id).is_some() {
            debug!("Lock force-released: entity {}", entity_id);
        }
    }

    /// Check if an entity is locked by any node
    ///
    /// Takes the local node ID to properly handle expiration:
    /// - Our own locks are never considered expired (held exactly as long as selected)
    /// - Remote locks are subject to the 5-second timeout
    pub fn is_locked(&self, entity_id: Uuid, local_node_id: NodeId) -> bool {
        self.locks.get(&entity_id).map_or(false, |lock| {
            // Our own locks never expire
            lock.holder == local_node_id || !lock.is_expired()
        })
    }

    /// Check if an entity is locked by a specific node
    ///
    /// Takes the local node ID to properly handle expiration:
    /// - If checking our own lock, ignore expiration (held exactly as long as selected)
    /// - If checking another node's lock, apply 5-second timeout
    pub fn is_locked_by(&self, entity_id: Uuid, node_id: NodeId, local_node_id: NodeId) -> bool {
        self.locks.get(&entity_id).map_or(false, |lock| {
            if lock.holder != node_id {
                // Not held by the queried node
                false
            } else if lock.holder == local_node_id {
                // Checking our own lock - never expires
                true
            } else {
                // Checking remote lock - check expiration
                !lock.is_expired()
            }
        })
    }

    /// Get the holder of a lock (if locked and not expired)
    ///
    /// Takes the local node ID to properly handle expiration:
    /// - Our own locks are never considered expired
    /// - Remote locks are subject to the 5-second timeout
    pub fn get_holder(&self, entity_id: Uuid, local_node_id: NodeId) -> Option<NodeId> {
        self.locks.get(&entity_id).and_then(|lock| {
            // Our own locks never expire
            if lock.holder == local_node_id || !lock.is_expired() {
                Some(lock.holder)
            } else {
                None
            }
        })
    }

    /// Renew a lock's heartbeat
    ///
    /// Returns true if the heartbeat was renewed, false if lock doesn't exist
    /// or is held by a different node.
    pub fn renew_heartbeat(&mut self, entity_id: Uuid, node_id: NodeId) -> bool {
        if let Some(lock) = self.locks.get_mut(&entity_id) {
            if lock.holder == node_id {
                lock.renew();
                return true;
            }
        }
        false
    }

    /// Get all expired locks
    pub fn get_expired_locks(&self) -> Vec<Uuid> {
        self.locks
            .iter()
            .filter(|(_, lock)| lock.is_expired())
            .map(|(entity_id, _)| *entity_id)
            .collect()
    }

    /// Get number of locks held by a node
    pub fn get_node_lock_count(&self, node_id: NodeId) -> usize {
        self.locks_per_node.get(&node_id).copied().unwrap_or(0)
    }

    /// Get total number of active locks
    pub fn total_locks(&self) -> usize {
        self.locks.len()
    }

    /// Remove a lock and update bookkeeping
    fn remove_lock(&mut self, entity_id: Uuid) {
        if let Some(lock) = self.locks.remove(&entity_id) {
            // Decrement node lock count
            if let Some(count) = self.locks_per_node.get_mut(&lock.holder) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.locks_per_node.remove(&lock.holder);
                }
            }
        }
    }

    /// Test helper: Manually expire a lock by setting its heartbeat timestamp to the past
    ///
    /// This is only intended for testing purposes to simulate lock expiration without waiting.
    pub fn expire_lock_for_testing(&mut self, entity_id: Uuid) {
        if let Some(lock) = self.locks.get_mut(&entity_id) {
            lock.last_heartbeat = Instant::now() - Duration::from_secs(10);
        }
    }
}

/// System to release locks when entities are deselected
///
/// This system detects when entities are removed from selection and releases
/// any locks held on those entities, broadcasting the release to other peers.
///
/// Add to your app as an Update system:
/// ```no_run
/// use bevy::prelude::*;
/// use libmarathon::networking::release_locks_on_deselection_system;
///
/// App::new().add_systems(Update, release_locks_on_deselection_system);
/// ```
pub fn release_locks_on_deselection_system(
    mut registry: ResMut<EntityLockRegistry>,
    node_clock: Res<NodeVectorClock>,
    bridge: Option<Res<GossipBridge>>,
    mut selection_query: Query<&mut NetworkedSelection, Changed<NetworkedSelection>>,
) {
    let node_id = node_clock.node_id;

    for selection in selection_query.iter_mut() {
        // Find entities that were previously locked but are no longer selected
        let currently_selected: std::collections::HashSet<Uuid> = selection.selected_ids.clone();

        // Check all locks held by this node
        let locks_to_release: Vec<Uuid> = registry
            .locks
            .iter()
            .filter(|(entity_id, lock)| {
                // Release if held by us and not currently selected
                lock.holder == node_id && !currently_selected.contains(entity_id)
            })
            .map(|(entity_id, _)| *entity_id)
            .collect();

        // Release each lock and broadcast
        for entity_id in locks_to_release {
            if registry.release(entity_id, node_id) {
                debug!("Releasing lock on deselected entity {}", entity_id);

                // Broadcast LockRelease
                if let Some(ref bridge) = bridge {
                    let msg = VersionedMessage::new(SyncMessage::Lock(LockMessage::LockRelease {
                        entity_id,
                        node_id,
                    }));

                    if let Err(e) = bridge.send(msg) {
                        error!("Failed to broadcast LockRelease on deselection: {}", e);
                    } else {
                        info!("Lock released on deselection: entity {}", entity_id);
                    }
                }
            }
        }
    }
}

/// System to clean up expired locks (crash recovery)
///
/// This system periodically removes locks that have exceeded their timeout
/// duration (default 5 seconds). This provides crash recovery - if a **remote**
/// node crashes while holding a lock, it will eventually expire.
///
/// **Important**: Only remote locks are cleaned up. Local locks (held by this node)
/// are never timed out - they're held exactly as long as entities are selected,
/// and only released via deselection.
///
/// Add to your app as an Update system:
/// ```no_run
/// use bevy::prelude::*;
/// use libmarathon::networking::cleanup_expired_locks_system;
///
/// App::new().add_systems(Update, cleanup_expired_locks_system);
/// ```
pub fn cleanup_expired_locks_system(
    mut registry: ResMut<EntityLockRegistry>,
    node_clock: Res<NodeVectorClock>,
    bridge: Option<Res<GossipBridge>>,
) {
    let node_id = node_clock.node_id;

    // Only clean up REMOTE locks (locks held by other nodes)
    // Our own locks are managed by release_locks_on_deselection_system
    let expired: Vec<Uuid> = registry
        .locks
        .iter()
        .filter(|(_, lock)| {
            // Only expire locks held by OTHER nodes
            lock.is_expired() && lock.holder != node_id
        })
        .map(|(entity_id, _)| *entity_id)
        .collect();

    if !expired.is_empty() {
        info!("Cleaning up {} expired remote locks", expired.len());

        for entity_id in expired {
            debug!("Force-releasing expired remote lock on entity {}", entity_id);
            registry.force_release(entity_id);

            // Broadcast LockReleased
            if let Some(ref bridge) = bridge {
                let msg =
                    VersionedMessage::new(SyncMessage::Lock(LockMessage::LockReleased { entity_id }));

                if let Err(e) = bridge.send(msg) {
                    error!("Failed to broadcast LockReleased for expired lock: {}", e);
                } else {
                    info!("Expired remote lock cleaned up: entity {}", entity_id);
                }
            }
        }
    }
}

/// System to broadcast heartbeats for all locks we currently hold
///
/// This system runs periodically (~1/sec) and broadcasts a heartbeat for each
/// lock this node holds. This keeps locks alive and provides crash detection -
/// if a node crashes, heartbeats stop and locks expire after 5 seconds.
///
/// Add to your app as an Update system with a run condition to throttle it:
/// ```no_run
/// use bevy::prelude::*;
/// use bevy::time::common_conditions::on_timer;
/// use std::time::Duration;
/// use libmarathon::networking::broadcast_lock_heartbeats_system;
///
/// App::new().add_systems(Update,
///     broadcast_lock_heartbeats_system.run_if(on_timer(Duration::from_secs(1)))
/// );
/// ```
pub fn broadcast_lock_heartbeats_system(
    mut registry: ResMut<EntityLockRegistry>,
    node_clock: Res<NodeVectorClock>,
    bridge: Option<Res<GossipBridge>>,
) {
    let node_id = node_clock.node_id;

    // Find all locks held by this node
    let our_locks: Vec<Uuid> = registry
        .locks
        .iter()
        .filter(|(_, lock)| lock.holder == node_id && !lock.is_expired())
        .map(|(entity_id, _)| *entity_id)
        .collect();

    if our_locks.is_empty() {
        return;
    }

    debug!("Broadcasting {} lock heartbeats", our_locks.len());

    // Renew local locks and broadcast heartbeat for each lock
    for entity_id in &our_locks {
        // Renew the lock locally first (don't rely on network loopback)
        registry.renew_heartbeat(*entity_id, node_id);
    }

    // Broadcast heartbeat messages to peers
    if let Some(ref bridge) = bridge {
        for entity_id in our_locks {
            let msg = VersionedMessage::new(SyncMessage::Lock(LockMessage::LockHeartbeat {
                entity_id,
                holder: node_id,
            }));

            if let Err(e) = bridge.send(msg) {
                error!(
                    "Failed to broadcast heartbeat for entity {}: {}",
                    entity_id, e
                );
            } else {
                trace!("Heartbeat sent for locked entity {}", entity_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_acquisition() {
        let mut registry = EntityLockRegistry::new();
        let entity_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();

        // Should acquire successfully
        assert!(registry.try_acquire(entity_id, node_id).is_ok());
        assert!(registry.is_locked(entity_id, node_id));
        assert!(registry.is_locked_by(entity_id, node_id, node_id));
        assert_eq!(registry.get_holder(entity_id, node_id), Some(node_id));
    }

    #[test]
    fn test_lock_conflict() {
        let mut registry = EntityLockRegistry::new();
        let entity_id = Uuid::new_v4();
        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();

        // Node 1 acquires
        assert!(registry.try_acquire(entity_id, node1).is_ok());

        // Node 2 should be rejected
        assert_eq!(registry.try_acquire(entity_id, node2), Err(node1));
    }

    #[test]
    fn test_lock_release() {
        let mut registry = EntityLockRegistry::new();
        let entity_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();

        // Acquire and release
        registry.try_acquire(entity_id, node_id).unwrap();
        assert!(registry.release(entity_id, node_id));
        assert!(!registry.is_locked(entity_id, node_id));
    }

    #[test]
    fn test_wrong_node_cannot_release() {
        let mut registry = EntityLockRegistry::new();
        let entity_id = Uuid::new_v4();
        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();

        // Node 1 acquires
        registry.try_acquire(entity_id, node1).unwrap();

        // Node 2 cannot release
        assert!(!registry.release(entity_id, node2));
        assert!(registry.is_locked(entity_id, node2));
        assert!(registry.is_locked_by(entity_id, node1, node2));
    }

    #[test]
    fn test_lock_timeout() {
        let mut registry = EntityLockRegistry::new();
        let entity_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();

        // Acquire with very short timeout
        registry.try_acquire(entity_id, node_id).unwrap();

        // Manually set timeout to 0 for testing
        if let Some(lock) = registry.locks.get_mut(&entity_id) {
            lock.timeout = Duration::from_secs(0);
        }

        // Should be detected as expired
        let expired = registry.get_expired_locks();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0], entity_id);
    }

    #[test]
    fn test_force_release() {
        let mut registry = EntityLockRegistry::new();
        let entity_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();

        registry.try_acquire(entity_id, node_id).unwrap();
        registry.force_release(entity_id);
        assert!(!registry.is_locked(entity_id, node_id));
    }

    #[test]
    fn test_rate_limiting() {
        let mut registry = EntityLockRegistry::new();
        let node_id = Uuid::new_v4();

        // Acquire MAX_LOCKS_PER_NODE locks
        for _ in 0..MAX_LOCKS_PER_NODE {
            let entity_id = Uuid::new_v4();
            assert!(registry.try_acquire(entity_id, node_id).is_ok());
        }

        // Next acquisition should fail (rate limit)
        let entity_id = Uuid::new_v4();
        assert!(registry.try_acquire(entity_id, node_id).is_err());
    }

    #[test]
    fn test_node_lock_count() {
        let mut registry = EntityLockRegistry::new();
        let node_id = Uuid::new_v4();

        assert_eq!(registry.get_node_lock_count(node_id), 0);

        // Acquire 3 locks
        for _ in 0..3 {
            let entity_id = Uuid::new_v4();
            registry.try_acquire(entity_id, node_id).unwrap();
        }

        assert_eq!(registry.get_node_lock_count(node_id), 3);
        assert_eq!(registry.total_locks(), 3);
    }

    #[test]
    fn test_lock_message_serialization() {
        let entity_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();

        let messages = vec![
            LockMessage::LockRequest { entity_id, node_id },
            LockMessage::LockAcquired {
                entity_id,
                holder: node_id,
            },
            LockMessage::LockRejected {
                entity_id,
                requester: node_id,
                current_holder: Uuid::new_v4(),
            },
            LockMessage::LockHeartbeat {
                entity_id,
                holder: node_id,
            },
            LockMessage::LockRelease { entity_id, node_id },
            LockMessage::LockReleased { entity_id },
        ];

        for message in messages {
            let bytes = rkyv::to_bytes::<rkyv::rancor::Failure>(&message).map(|b| b.to_vec()).unwrap();
            let deserialized: LockMessage = rkyv::from_bytes::<LockMessage, rkyv::rancor::Failure>(&bytes).unwrap();
            assert_eq!(message, deserialized);
        }
    }

    #[test]
    fn test_heartbeat_renewal() {
        let mut registry = EntityLockRegistry::new();
        let entity_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();

        // Acquire lock
        registry.try_acquire(entity_id, node_id).unwrap();

        // Get initial heartbeat time
        let initial_heartbeat = registry.locks.get(&entity_id).unwrap().last_heartbeat;

        // Sleep a bit to ensure time difference
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Renew heartbeat
        assert!(registry.renew_heartbeat(entity_id, node_id));

        // Check that heartbeat was updated
        let updated_heartbeat = registry.locks.get(&entity_id).unwrap().last_heartbeat;
        assert!(updated_heartbeat > initial_heartbeat);
    }

    #[test]
    fn test_heartbeat_wrong_node() {
        let mut registry = EntityLockRegistry::new();
        let entity_id = Uuid::new_v4();
        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();

        // Node 1 acquires
        registry.try_acquire(entity_id, node1).unwrap();

        // Node 2 tries to renew heartbeat - should fail
        assert!(!registry.renew_heartbeat(entity_id, node2));
    }

    #[test]
    fn test_heartbeat_expiration() {
        let mut registry = EntityLockRegistry::new();
        let entity_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();

        // Acquire with very short timeout
        registry.try_acquire(entity_id, node_id).unwrap();

        // Manually set timeout to 0 for testing
        if let Some(lock) = registry.locks.get_mut(&entity_id) {
            lock.timeout = Duration::from_secs(0);
        }

        // Should be detected as expired
        let expired = registry.get_expired_locks();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0], entity_id);
    }
}
