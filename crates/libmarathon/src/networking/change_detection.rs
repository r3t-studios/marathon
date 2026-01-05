//! Change detection for networked entities
//!
//! This module provides systems that detect when networked components change
//! and prepare them for delta generation.

use bevy::prelude::*;

use crate::networking::{
    NetworkedEntity,
    NetworkedTransform,
};

/// System to automatically detect Transform changes and mark entity for sync
///
/// This system detects changes to Transform components on networked entities
/// and triggers persistence by accessing `NetworkedEntity` mutably (which marks
/// it as changed via Bevy's change detection).
///
/// Add this system to your app if you want automatic synchronization of
/// Transform changes:
///
/// ```no_run
/// use bevy::prelude::*;
/// use libmarathon::networking::auto_detect_transform_changes_system;
///
/// App::new().add_systems(Update, auto_detect_transform_changes_system);
/// ```
pub fn auto_detect_transform_changes_system(
    mut query: Query<
        (Entity, &mut NetworkedEntity, &Transform),
        (
            With<NetworkedTransform>,
            Or<(Changed<Transform>, Changed<GlobalTransform>)>,
            Without<crate::networking::SkipNextDeltaGeneration>,
        ),
    >,
) {
    // Count how many changed entities we found
    let count = query.iter().count();
    if count > 0 {
        debug!(
            "auto_detect_transform_changes_system: Found {} entities with changed Transform",
            count
        );
    }

    // Simply accessing &mut NetworkedEntity triggers Bevy's change detection
    for (_entity, mut networked, transform) in query.iter_mut() {
        debug!(
            "Marking NetworkedEntity {:?} as changed due to Transform change (pos: {:?})",
            networked.network_id, transform.translation
        );
        // No-op - the mutable access itself marks NetworkedEntity as changed
        // This will trigger the delta generation system
        let _ = &mut *networked;
    }
}

/// Resource to track the last sync version for each entity
///
/// This helps us avoid sending redundant deltas for the same changes.
#[derive(Resource, Default)]
pub struct LastSyncVersions {
    /// Map from network_id to the last vector clock we synced
    versions: std::collections::HashMap<uuid::Uuid, u64>,
}

impl LastSyncVersions {
    /// Check if we should sync this entity based on version
    pub fn should_sync(&self, network_id: uuid::Uuid, version: u64) -> bool {
        match self.versions.get(&network_id) {
            | Some(&last_version) => version > last_version,
            | None => true, // Never synced before
        }
    }

    /// Update the last synced version for an entity
    pub fn update(&mut self, network_id: uuid::Uuid, version: u64) {
        self.versions.insert(network_id, version);
    }

    /// Remove tracking for an entity (when despawned)
    pub fn remove(&mut self, network_id: uuid::Uuid) {
        self.versions.remove(&network_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_last_sync_versions() {
        let mut versions = LastSyncVersions::default();
        let id = uuid::Uuid::new_v4();

        // Should sync when never synced before
        assert!(versions.should_sync(id, 1));

        // Update to version 1
        versions.update(id, 1);

        // Should not sync same version
        assert!(!versions.should_sync(id, 1));

        // Should not sync older version
        assert!(!versions.should_sync(id, 0));

        // Should sync newer version
        assert!(versions.should_sync(id, 2));

        // Remove and should sync again
        versions.remove(id);
        assert!(versions.should_sync(id, 2));
    }

    #[test]
    fn test_multiple_entities() {
        let mut versions = LastSyncVersions::default();
        let id1 = uuid::Uuid::new_v4();
        let id2 = uuid::Uuid::new_v4();

        versions.update(id1, 5);
        versions.update(id2, 3);

        assert!(!versions.should_sync(id1, 4));
        assert!(versions.should_sync(id1, 6));
        assert!(!versions.should_sync(id2, 2));
        assert!(versions.should_sync(id2, 4));
    }
}
