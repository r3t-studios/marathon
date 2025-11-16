//! Core types for the persistence layer

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use bevy::prelude::Resource;

/// Maximum size for a single component in bytes (10MB)
/// Components larger than this may indicate serialization issues or unbounded data growth
const MAX_COMPONENT_SIZE_BYTES: usize = 10 * 1024 * 1024;

/// Critical flush deadline in milliseconds (1 second for tier-1 operations)
const CRITICAL_FLUSH_DEADLINE_MS: u64 = 1000;

/// Unique identifier for entities that can be synced across nodes
pub type EntityId = uuid::Uuid;

/// Node identifier for CRDT operations
pub type NodeId = String;

/// Priority level for persistence operations
///
/// Determines how quickly an operation should be flushed to disk:
/// - **Normal**: Regular batched flushing (5-60s intervals based on battery)
/// - **Critical**: Flush within 1 second (tier-1 operations like user actions, CRDT ops)
/// - **Immediate**: Flush immediately (shutdown, background suspension)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FlushPriority {
    /// Normal priority - regular batched flushing
    Normal,
    /// Critical priority - flush within 1 second
    Critical,
    /// Immediate priority - flush right now
    Immediate,
}

/// Resource to track entities with uncommitted changes
#[derive(Debug, Default)]
pub struct DirtyEntities {
    /// Set of entity IDs with changes not yet in write buffer
    pub entities: HashSet<EntityId>,

    /// Map of entity ID to set of dirty component type names
    pub components: HashMap<EntityId, HashSet<String>>,

    /// Track when each entity was last modified (for prioritization)
    pub last_modified: HashMap<EntityId, Instant>,
}

impl DirtyEntities {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark an entity's component as dirty
    pub fn mark_dirty(&mut self, entity_id: EntityId, component_type: impl Into<String>) {
        self.entities.insert(entity_id);
        self.components
            .entry(entity_id)
            .or_default()
            .insert(component_type.into());
        self.last_modified.insert(entity_id, Instant::now());
    }

    /// Clear all dirty tracking (called after flush to write buffer)
    pub fn clear(&mut self) {
        self.entities.clear();
        self.components.clear();
        self.last_modified.clear();
    }

    /// Check if an entity is dirty
    pub fn is_dirty(&self, entity_id: &EntityId) -> bool {
        self.entities.contains(entity_id)
    }

    /// Get the number of dirty entities
    pub fn count(&self) -> usize {
        self.entities.len()
    }
}

/// Operations that can be persisted to the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PersistenceOp {
    /// Insert or update an entity's existence
    UpsertEntity {
        id: EntityId,
        data: EntityData,
    },

    /// Insert or update a component on an entity
    UpsertComponent {
        entity_id: EntityId,
        component_type: String,
        data: Vec<u8>,
    },

    /// Log an operation for CRDT sync
    LogOperation {
        node_id: NodeId,
        sequence: u64,
        operation: Vec<u8>,
    },

    /// Update vector clock for causality tracking
    UpdateVectorClock {
        node_id: NodeId,
        counter: u64,
    },

    /// Delete an entity
    DeleteEntity {
        id: EntityId,
    },

    /// Delete a component from an entity
    DeleteComponent {
        entity_id: EntityId,
        component_type: String,
    },
}

impl PersistenceOp {
    /// Get the default priority for this operation type
    ///
    /// CRDT operations (LogOperation, UpdateVectorClock) are critical tier-1 operations
    /// that should be flushed within 1 second to maintain causality across nodes.
    /// Other operations use normal priority by default.
    pub fn default_priority(&self) -> FlushPriority {
        match self {
            // CRDT operations are tier-1 (critical)
            PersistenceOp::LogOperation { .. } | PersistenceOp::UpdateVectorClock { .. } => {
                FlushPriority::Critical
            }
            // All other operations are normal priority by default
            _ => FlushPriority::Normal,
        }
    }
}

/// Metadata about an entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityData {
    pub id: EntityId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub entity_type: String,
}

/// Write buffer for batching persistence operations
#[derive(Debug)]
pub struct WriteBuffer {
    /// Pending operations not yet committed to SQLite
    pub pending_operations: Vec<PersistenceOp>,

    /// When the buffer was last flushed
    pub last_flush: Instant,

    /// Maximum number of operations before forcing a flush
    pub max_operations: usize,

    /// Highest priority operation currently in the buffer
    pub highest_priority: FlushPriority,

    /// When the first critical operation was added (for deadline tracking)
    pub first_critical_time: Option<Instant>,
}

impl WriteBuffer {
    pub fn new(max_operations: usize) -> Self {
        Self {
            pending_operations: Vec::new(),
            last_flush: Instant::now(),
            max_operations,
            highest_priority: FlushPriority::Normal,
            first_critical_time: None,
        }
    }

    /// Add an operation to the write buffer with normal priority
    ///
    /// This is a convenience method that calls `add_with_priority` with `FlushPriority::Normal`.
    ///
    /// # Panics
    /// Panics if component data exceeds MAX_COMPONENT_SIZE_BYTES (10MB)
    pub fn add(&mut self, op: PersistenceOp) {
        self.add_with_priority(op, FlushPriority::Normal);
    }

    /// Add an operation using its default priority
    ///
    /// Uses `PersistenceOp::default_priority()` to determine priority automatically.
    /// CRDT operations will be added as Critical, others as Normal.
    ///
    /// # Panics
    /// Panics if component data exceeds MAX_COMPONENT_SIZE_BYTES (10MB)
    pub fn add_with_default_priority(&mut self, op: PersistenceOp) {
        let priority = op.default_priority();
        self.add_with_priority(op, priority);
    }

    /// Add an operation to the write buffer with the specified priority
    ///
    /// If an operation for the same entity+component already exists,
    /// it will be replaced (keeping only the latest state). The priority
    /// is tracked separately to determine flush urgency.
    ///
    /// # Panics
    /// Panics if component data exceeds MAX_COMPONENT_SIZE_BYTES (10MB)
    pub fn add_with_priority(&mut self, op: PersistenceOp, priority: FlushPriority) {
        // Validate component size to prevent unbounded memory growth
        match &op {
            PersistenceOp::UpsertComponent { data, component_type, .. } => {
                if data.len() > MAX_COMPONENT_SIZE_BYTES {
                    panic!(
                        "Component {} size ({} bytes) exceeds maximum ({} bytes). \
                        This may indicate unbounded data growth or serialization issues.",
                        component_type,
                        data.len(),
                        MAX_COMPONENT_SIZE_BYTES
                    );
                }
            }
            PersistenceOp::LogOperation { operation, .. } => {
                if operation.len() > MAX_COMPONENT_SIZE_BYTES {
                    panic!(
                        "Operation size ({} bytes) exceeds maximum ({} bytes)",
                        operation.len(),
                        MAX_COMPONENT_SIZE_BYTES
                    );
                }
            }
            _ => {}
        }

        match &op {
            PersistenceOp::UpsertComponent { entity_id, component_type, .. } => {
                // Remove any existing pending write for this entity+component
                self.pending_operations.retain(|existing_op| {
                    !matches!(existing_op,
                        PersistenceOp::UpsertComponent {
                            entity_id: e_id,
                            component_type: c_type,
                            ..
                        } if e_id == entity_id && c_type == component_type
                    )
                });
            }
            PersistenceOp::UpsertEntity { id, .. } => {
                // Remove any existing pending write for this entity
                self.pending_operations.retain(|existing_op| {
                    !matches!(existing_op,
                        PersistenceOp::UpsertEntity { id: e_id, .. }
                        if e_id == id
                    )
                });
            }
            _ => {
                // Other operations don't need coalescing
            }
        }

        // Track priority for flush urgency
        if priority > self.highest_priority {
            self.highest_priority = priority;
        }

        // Track when first critical operation was added (for deadline enforcement)
        if priority >= FlushPriority::Critical && self.first_critical_time.is_none() {
            self.first_critical_time = Some(Instant::now());
        }

        self.pending_operations.push(op);
    }

    /// Take all pending operations and return them for flushing
    ///
    /// This resets the priority tracking state.
    pub fn take_operations(&mut self) -> Vec<PersistenceOp> {
        // Reset priority tracking when operations are taken
        self.highest_priority = FlushPriority::Normal;
        self.first_critical_time = None;
        std::mem::take(&mut self.pending_operations)
    }

    /// Check if buffer should be flushed
    ///
    /// Returns true if any of these conditions are met:
    /// - Buffer is at capacity (max_operations reached)
    /// - Regular flush interval has elapsed (for normal priority)
    /// - Critical operation deadline exceeded (1 second for critical ops)
    /// - Immediate priority operation exists
    pub fn should_flush(&self, flush_interval: std::time::Duration) -> bool {
        // Immediate priority always flushes
        if self.highest_priority == FlushPriority::Immediate {
            return true;
        }

        // Critical priority flushes after 1 second deadline
        if self.highest_priority == FlushPriority::Critical {
            if let Some(critical_time) = self.first_critical_time {
                if critical_time.elapsed().as_millis() >= CRITICAL_FLUSH_DEADLINE_MS as u128 {
                    return true;
                }
            }
        }

        // Normal flushing conditions
        self.pending_operations.len() >= self.max_operations
            || self.last_flush.elapsed() >= flush_interval
    }

    /// Get the number of pending operations
    pub fn len(&self) -> usize {
        self.pending_operations.len()
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.pending_operations.is_empty()
    }
}

/// Battery status for adaptive flushing
#[derive(Debug, Clone, Copy, Resource)]
pub struct BatteryStatus {
    /// Battery level from 0.0 to 1.0
    pub level: f32,

    /// Whether the device is currently charging
    pub is_charging: bool,

    /// Whether low power mode is enabled (iOS)
    pub is_low_power_mode: bool,
}

impl Default for BatteryStatus {
    fn default() -> Self {
        Self {
            level: 1.0,
            is_charging: false,
            is_low_power_mode: false,
        }
    }
}

impl BatteryStatus {
    /// Update battery status from iOS UIDevice.batteryLevel
    ///
    /// # iOS Integration Example
    ///
    /// ```swift
    /// // In your iOS app code:
    /// UIDevice.current.isBatteryMonitoringEnabled = true
    /// let batteryLevel = UIDevice.current.batteryLevel // Returns 0.0 to 1.0
    /// let isCharging = UIDevice.current.batteryState == .charging ||
    ///                  UIDevice.current.batteryState == .full
    /// let isLowPowerMode = ProcessInfo.processInfo.isLowPowerModeEnabled
    ///
    /// // Update Bevy resource (this is pseudocode - actual implementation depends on your bridge)
    /// battery_status.update_from_ios(batteryLevel, isCharging, isLowPowerMode);
    /// ```
    pub fn update_from_ios(&mut self, level: f32, is_charging: bool, is_low_power_mode: bool) {
        self.level = level.clamp(0.0, 1.0);
        self.is_charging = is_charging;
        self.is_low_power_mode = is_low_power_mode;
    }

    /// Check if the device is in a battery-critical state
    ///
    /// Returns true if battery is low (<20%) and not charging, or low power mode is enabled.
    pub fn is_battery_critical(&self) -> bool {
        (self.level < 0.2 && !self.is_charging) || self.is_low_power_mode
    }
}

/// Session state tracking for crash detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub session_id: String,
    pub started_at: DateTime<Utc>,
    pub clean_shutdown: bool,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            started_at: Utc::now(),
            clean_shutdown: false,
        }
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dirty_entities_tracking() {
        let mut dirty = DirtyEntities::new();
        let entity_id = EntityId::new_v4();

        dirty.mark_dirty(entity_id, "Transform");
        assert!(dirty.is_dirty(&entity_id));
        assert_eq!(dirty.count(), 1);

        dirty.clear();
        assert!(!dirty.is_dirty(&entity_id));
        assert_eq!(dirty.count(), 0);
    }

    #[test]
    fn test_write_buffer_coalescing() {
        let mut buffer = WriteBuffer::new(100);
        let entity_id = EntityId::new_v4();

        // Add first version
        buffer.add(PersistenceOp::UpsertComponent {
            entity_id,
            component_type: "Transform".to_string(),
            data: vec![1, 2, 3],
        });
        assert_eq!(buffer.len(), 1);

        // Add second version (should replace first)
        buffer.add(PersistenceOp::UpsertComponent {
            entity_id,
            component_type: "Transform".to_string(),
            data: vec![4, 5, 6],
        });
        assert_eq!(buffer.len(), 1);

        // Verify only latest version exists
        let ops = buffer.take_operations();
        assert_eq!(ops.len(), 1);
        if let PersistenceOp::UpsertComponent { data, .. } = &ops[0] {
            assert_eq!(data, &vec![4, 5, 6]);
        } else {
            panic!("Expected UpsertComponent");
        }
    }

    #[test]
    fn test_write_buffer_different_components() {
        let mut buffer = WriteBuffer::new(100);
        let entity_id = EntityId::new_v4();

        // Add Transform
        buffer.add(PersistenceOp::UpsertComponent {
            entity_id,
            component_type: "Transform".to_string(),
            data: vec![1, 2, 3],
        });

        // Add Velocity (different component, should not coalesce)
        buffer.add(PersistenceOp::UpsertComponent {
            entity_id,
            component_type: "Velocity".to_string(),
            data: vec![4, 5, 6],
        });

        assert_eq!(buffer.len(), 2);
    }

    #[test]
    fn test_flush_priority_immediate() {
        let mut buffer = WriteBuffer::new(100);
        let entity_id = EntityId::new_v4();

        // Add operation with immediate priority
        buffer.add_with_priority(
            PersistenceOp::UpsertEntity {
                id: entity_id,
                data: EntityData {
                    id: entity_id,
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                    entity_type: "TestEntity".to_string(),
                },
            },
            FlushPriority::Immediate,
        );

        // Should flush immediately regardless of interval
        assert!(buffer.should_flush(std::time::Duration::from_secs(100)));
        assert_eq!(buffer.highest_priority, FlushPriority::Immediate);
    }

    #[test]
    fn test_flush_priority_critical_deadline() {
        let mut buffer = WriteBuffer::new(100);
        let entity_id = EntityId::new_v4();

        // Add operation with critical priority
        buffer.add_with_priority(
            PersistenceOp::UpsertEntity {
                id: entity_id,
                data: EntityData {
                    id: entity_id,
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                    entity_type: "TestEntity".to_string(),
                },
            },
            FlushPriority::Critical,
        );

        assert_eq!(buffer.highest_priority, FlushPriority::Critical);
        assert!(buffer.first_critical_time.is_some());

        // Should not flush immediately
        assert!(!buffer.should_flush(std::time::Duration::from_secs(100)));

        // Simulate deadline passing by manually setting the time
        buffer.first_critical_time =
            Some(Instant::now() - std::time::Duration::from_millis(CRITICAL_FLUSH_DEADLINE_MS + 100));

        // Now should flush due to deadline
        assert!(buffer.should_flush(std::time::Duration::from_secs(100)));
    }

    #[test]
    fn test_flush_priority_normal() {
        let mut buffer = WriteBuffer::new(100);
        let entity_id = EntityId::new_v4();

        // Add normal priority operation
        buffer.add(PersistenceOp::UpsertEntity {
            id: entity_id,
            data: EntityData {
                id: entity_id,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                entity_type: "TestEntity".to_string(),
            },
        });

        assert_eq!(buffer.highest_priority, FlushPriority::Normal);
        assert!(buffer.first_critical_time.is_none());

        // Should not flush before interval
        assert!(!buffer.should_flush(std::time::Duration::from_secs(100)));

        // Set last flush to past
        buffer.last_flush = Instant::now() - std::time::Duration::from_secs(200);

        // Now should flush
        assert!(buffer.should_flush(std::time::Duration::from_secs(100)));
    }

    #[test]
    fn test_priority_reset_on_take() {
        let mut buffer = WriteBuffer::new(100);
        let entity_id = EntityId::new_v4();

        // Add critical operation
        buffer.add_with_priority(
            PersistenceOp::UpsertEntity {
                id: entity_id,
                data: EntityData {
                    id: entity_id,
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                    entity_type: "TestEntity".to_string(),
                },
            },
            FlushPriority::Critical,
        );

        assert_eq!(buffer.highest_priority, FlushPriority::Critical);
        assert!(buffer.first_critical_time.is_some());

        // Take operations
        let ops = buffer.take_operations();
        assert_eq!(ops.len(), 1);

        // Priority should be reset
        assert_eq!(buffer.highest_priority, FlushPriority::Normal);
        assert!(buffer.first_critical_time.is_none());
    }

    #[test]
    fn test_default_priority_for_crdt_ops() {
        let log_op = PersistenceOp::LogOperation {
            node_id: "node1".to_string(),
            sequence: 1,
            operation: vec![1, 2, 3],
        };

        let vector_clock_op = PersistenceOp::UpdateVectorClock {
            node_id: "node1".to_string(),
            counter: 42,
        };

        let entity_op = PersistenceOp::UpsertEntity {
            id: EntityId::new_v4(),
            data: EntityData {
                id: EntityId::new_v4(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                entity_type: "TestEntity".to_string(),
            },
        };

        // CRDT operations should have Critical priority
        assert_eq!(log_op.default_priority(), FlushPriority::Critical);
        assert_eq!(vector_clock_op.default_priority(), FlushPriority::Critical);

        // Other operations should have Normal priority
        assert_eq!(entity_op.default_priority(), FlushPriority::Normal);
    }

    #[test]
    fn test_add_with_default_priority() {
        let mut buffer = WriteBuffer::new(100);

        // Add CRDT operation using default priority
        buffer.add_with_default_priority(PersistenceOp::LogOperation {
            node_id: "node1".to_string(),
            sequence: 1,
            operation: vec![1, 2, 3],
        });

        // Should be tracked as Critical
        assert_eq!(buffer.highest_priority, FlushPriority::Critical);
        assert!(buffer.first_critical_time.is_some());
    }
}
