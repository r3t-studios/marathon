//! DEPRECATED: Reflection-based component serialization
//! Marker components for the persistence system
//!
//! All component serialization now uses #[derive(Synced)] with rkyv.
//! This module only provides the Persisted marker component.

use bevy::prelude::*;

/// Marker component to indicate that an entity should be persisted
///
/// Add this component to any entity that should have its state persisted to
/// disk. The persistence system will automatically serialize all components on
/// entities with this marker when they change.
///
/// # Triggering Persistence
///
/// To trigger persistence after modifying components on an entity, access
/// `Persisted` mutably through a query. Bevy's change detection will
/// automatically mark it as changed:
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use libmarathon::persistence::*;
/// fn update_position(mut query: Query<(&mut Transform, &mut Persisted)>) {
///     for (mut transform, mut persisted) in query.iter_mut() {
///         transform.translation.x += 1.0;
///         // Accessing &mut Persisted triggers change detection automatically
///     }
/// }
/// ```
///
/// Alternatively, use `auto_track_transform_changes_system` for automatic
/// persistence of Transform changes without manual queries.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct Persisted {
    /// Unique network ID for this entity
    pub network_id: uuid::Uuid,
}

impl Persisted {
    pub fn new() -> Self {
        Self {
            network_id: uuid::Uuid::new_v4(),
        }
    }

    pub fn with_id(network_id: uuid::Uuid) -> Self {
        Self { network_id }
    }
}

// All component serialization now uses #[derive(Synced)] with rkyv through ComponentTypeRegistry
