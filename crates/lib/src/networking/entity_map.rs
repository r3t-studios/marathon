//! Bidirectional mapping between network IDs and Bevy entities
//!
//! This module provides efficient lookup in both directions:
//! - network_id → Entity (when receiving remote operations)
//! - Entity → network_id (when broadcasting local changes)

use std::collections::HashMap;

use bevy::prelude::*;

/// Bidirectional mapping between network IDs and Bevy entities
///
/// This resource maintains two HashMaps for O(1) lookup in both directions.
/// It's updated automatically by the networking systems when entities are
/// spawned or despawned.
///
/// # Thread Safety
///
/// This is a Bevy Resource, so it's automatically synchronized across systems.
///
/// # Example
///
/// ```
/// use bevy::prelude::*;
/// use lib::networking::{
///     NetworkEntityMap,
///     NetworkedEntity,
/// };
/// use uuid::Uuid;
///
/// fn example_system(mut map: ResMut<NetworkEntityMap>, query: Query<(Entity, &NetworkedEntity)>) {
///     // Register networked entities
///     for (entity, networked) in query.iter() {
///         map.insert(networked.network_id, entity);
///     }
///
///     // Later, look up by network ID
///     let network_id = Uuid::new_v4();
///     if let Some(entity) = map.get_entity(network_id) {
///         println!("Found entity: {:?}", entity);
///     }
/// }
/// ```
#[derive(Resource, Default, Debug)]
pub struct NetworkEntityMap {
    /// Map from network ID to Bevy Entity
    network_id_to_entity: HashMap<uuid::Uuid, Entity>,

    /// Map from Bevy Entity to network ID
    entity_to_network_id: HashMap<Entity, uuid::Uuid>,
}

impl NetworkEntityMap {
    /// Create a new empty entity map
    pub fn new() -> Self {
        Self {
            network_id_to_entity: HashMap::new(),
            entity_to_network_id: HashMap::new(),
        }
    }

    /// Insert a bidirectional mapping
    ///
    /// If the network_id or entity already exists in the map, the old mapping
    /// is removed first to maintain consistency.
    ///
    /// # Example
    ///
    /// ```
    /// use bevy::prelude::*;
    /// use lib::networking::NetworkEntityMap;
    /// use uuid::Uuid;
    ///
    /// # let mut world = World::new();
    /// # let entity = world.spawn_empty().id();
    /// let mut map = NetworkEntityMap::new();
    /// let network_id = Uuid::new_v4();
    ///
    /// map.insert(network_id, entity);
    /// assert_eq!(map.get_entity(network_id), Some(entity));
    /// assert_eq!(map.get_network_id(entity), Some(network_id));
    /// ```
    pub fn insert(&mut self, network_id: uuid::Uuid, entity: Entity) {
        // Remove old mappings if they exist
        if let Some(old_entity) = self.network_id_to_entity.get(&network_id) {
            self.entity_to_network_id.remove(old_entity);
        }
        if let Some(old_network_id) = self.entity_to_network_id.get(&entity) {
            self.network_id_to_entity.remove(old_network_id);
        }

        // Insert new mappings
        self.network_id_to_entity.insert(network_id, entity);
        self.entity_to_network_id.insert(entity, network_id);
    }

    /// Get the Bevy Entity for a network ID
    ///
    /// Returns None if the network ID is not in the map.
    ///
    /// # Example
    ///
    /// ```
    /// use bevy::prelude::*;
    /// use lib::networking::NetworkEntityMap;
    /// use uuid::Uuid;
    ///
    /// # let mut world = World::new();
    /// # let entity = world.spawn_empty().id();
    /// let mut map = NetworkEntityMap::new();
    /// let network_id = Uuid::new_v4();
    ///
    /// map.insert(network_id, entity);
    /// assert_eq!(map.get_entity(network_id), Some(entity));
    ///
    /// let unknown_id = Uuid::new_v4();
    /// assert_eq!(map.get_entity(unknown_id), None);
    /// ```
    pub fn get_entity(&self, network_id: uuid::Uuid) -> Option<Entity> {
        self.network_id_to_entity.get(&network_id).copied()
    }

    /// Get the network ID for a Bevy Entity
    ///
    /// Returns None if the entity is not in the map.
    ///
    /// # Example
    ///
    /// ```
    /// use bevy::prelude::*;
    /// use lib::networking::NetworkEntityMap;
    /// use uuid::Uuid;
    ///
    /// # let mut world = World::new();
    /// # let entity = world.spawn_empty().id();
    /// let mut map = NetworkEntityMap::new();
    /// let network_id = Uuid::new_v4();
    ///
    /// map.insert(network_id, entity);
    /// assert_eq!(map.get_network_id(entity), Some(network_id));
    ///
    /// # let unknown_entity = world.spawn_empty().id();
    /// assert_eq!(map.get_network_id(unknown_entity), None);
    /// ```
    pub fn get_network_id(&self, entity: Entity) -> Option<uuid::Uuid> {
        self.entity_to_network_id.get(&entity).copied()
    }

    /// Remove a mapping by network ID
    ///
    /// Returns the Entity that was mapped to this network ID, if any.
    ///
    /// # Example
    ///
    /// ```
    /// use bevy::prelude::*;
    /// use lib::networking::NetworkEntityMap;
    /// use uuid::Uuid;
    ///
    /// # let mut world = World::new();
    /// # let entity = world.spawn_empty().id();
    /// let mut map = NetworkEntityMap::new();
    /// let network_id = Uuid::new_v4();
    ///
    /// map.insert(network_id, entity);
    /// assert_eq!(map.remove_by_network_id(network_id), Some(entity));
    /// assert_eq!(map.get_entity(network_id), None);
    /// ```
    pub fn remove_by_network_id(&mut self, network_id: uuid::Uuid) -> Option<Entity> {
        if let Some(entity) = self.network_id_to_entity.remove(&network_id) {
            self.entity_to_network_id.remove(&entity);
            Some(entity)
        } else {
            None
        }
    }

    /// Remove a mapping by Entity
    ///
    /// Returns the network ID that was mapped to this entity, if any.
    ///
    /// # Example
    ///
    /// ```
    /// use bevy::prelude::*;
    /// use lib::networking::NetworkEntityMap;
    /// use uuid::Uuid;
    ///
    /// # let mut world = World::new();
    /// # let entity = world.spawn_empty().id();
    /// let mut map = NetworkEntityMap::new();
    /// let network_id = Uuid::new_v4();
    ///
    /// map.insert(network_id, entity);
    /// assert_eq!(map.remove_by_entity(entity), Some(network_id));
    /// assert_eq!(map.get_network_id(entity), None);
    /// ```
    pub fn remove_by_entity(&mut self, entity: Entity) -> Option<uuid::Uuid> {
        if let Some(network_id) = self.entity_to_network_id.remove(&entity) {
            self.network_id_to_entity.remove(&network_id);
            Some(network_id)
        } else {
            None
        }
    }

    /// Check if a network ID exists in the map
    pub fn contains_network_id(&self, network_id: uuid::Uuid) -> bool {
        self.network_id_to_entity.contains_key(&network_id)
    }

    /// Check if an entity exists in the map
    pub fn contains_entity(&self, entity: Entity) -> bool {
        self.entity_to_network_id.contains_key(&entity)
    }

    /// Get the number of mapped entities
    pub fn len(&self) -> usize {
        self.network_id_to_entity.len()
    }

    /// Check if the map is empty
    pub fn is_empty(&self) -> bool {
        self.network_id_to_entity.is_empty()
    }

    /// Clear all mappings
    pub fn clear(&mut self) {
        self.network_id_to_entity.clear();
        self.entity_to_network_id.clear();
    }

    /// Get an iterator over all (network_id, entity) pairs
    pub fn iter(&self) -> impl Iterator<Item = (&uuid::Uuid, &Entity)> {
        self.network_id_to_entity.iter()
    }

    /// Get all network IDs
    pub fn network_ids(&self) -> impl Iterator<Item = &uuid::Uuid> {
        self.network_id_to_entity.keys()
    }

    /// Get all entities
    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.entity_to_network_id.keys()
    }
}

/// System to automatically register NetworkedEntity components in the map
///
/// This system runs in PostUpdate to catch newly spawned networked entities
/// and add them to the NetworkEntityMap.
///
/// Add this to your app:
/// ```no_run
/// use bevy::prelude::*;
/// use lib::networking::register_networked_entities_system;
///
/// App::new().add_systems(PostUpdate, register_networked_entities_system);
/// ```
pub fn register_networked_entities_system(
    mut map: ResMut<NetworkEntityMap>,
    query: Query<
        (Entity, &crate::networking::NetworkedEntity),
        Added<crate::networking::NetworkedEntity>,
    >,
) {
    for (entity, networked) in query.iter() {
        map.insert(networked.network_id, entity);
    }
}

/// System to automatically unregister despawned entities from the map
///
/// This system cleans up the NetworkEntityMap when networked entities are
/// despawned.
///
/// Add this to your app:
/// ```no_run
/// use bevy::prelude::*;
/// use lib::networking::cleanup_despawned_entities_system;
///
/// App::new().add_systems(PostUpdate, cleanup_despawned_entities_system);
/// ```
pub fn cleanup_despawned_entities_system(
    mut map: ResMut<NetworkEntityMap>,
    mut removed: RemovedComponents<crate::networking::NetworkedEntity>,
) {
    for entity in removed.read() {
        map.remove_by_entity(entity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let mut map = NetworkEntityMap::new();
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let network_id = uuid::Uuid::new_v4();

        map.insert(network_id, entity);

        assert_eq!(map.get_entity(network_id), Some(entity));
        assert_eq!(map.get_network_id(entity), Some(network_id));
    }

    #[test]
    fn test_get_nonexistent() {
        let map = NetworkEntityMap::new();
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let network_id = uuid::Uuid::new_v4();

        assert_eq!(map.get_entity(network_id), None);
        assert_eq!(map.get_network_id(entity), None);
    }

    #[test]
    fn test_remove_by_network_id() {
        let mut map = NetworkEntityMap::new();
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let network_id = uuid::Uuid::new_v4();

        map.insert(network_id, entity);
        assert_eq!(map.remove_by_network_id(network_id), Some(entity));
        assert_eq!(map.get_entity(network_id), None);
        assert_eq!(map.get_network_id(entity), None);
    }

    #[test]
    fn test_remove_by_entity() {
        let mut map = NetworkEntityMap::new();
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let network_id = uuid::Uuid::new_v4();

        map.insert(network_id, entity);
        assert_eq!(map.remove_by_entity(entity), Some(network_id));
        assert_eq!(map.get_entity(network_id), None);
        assert_eq!(map.get_network_id(entity), None);
    }

    #[test]
    fn test_contains() {
        let mut map = NetworkEntityMap::new();
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let network_id = uuid::Uuid::new_v4();

        assert!(!map.contains_network_id(network_id));
        assert!(!map.contains_entity(entity));

        map.insert(network_id, entity);

        assert!(map.contains_network_id(network_id));
        assert!(map.contains_entity(entity));
    }

    #[test]
    fn test_len_and_is_empty() {
        let mut map = NetworkEntityMap::new();
        let mut world = World::new();

        assert!(map.is_empty());
        assert_eq!(map.len(), 0);

        let entity1 = world.spawn_empty().id();
        let id1 = uuid::Uuid::new_v4();
        map.insert(id1, entity1);

        assert!(!map.is_empty());
        assert_eq!(map.len(), 1);

        let entity2 = world.spawn_empty().id();
        let id2 = uuid::Uuid::new_v4();
        map.insert(id2, entity2);

        assert_eq!(map.len(), 2);
    }

    #[test]
    fn test_clear() {
        let mut map = NetworkEntityMap::new();
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let network_id = uuid::Uuid::new_v4();

        map.insert(network_id, entity);
        assert_eq!(map.len(), 1);

        map.clear();
        assert!(map.is_empty());
    }

    #[test]
    fn test_insert_overwrites_old_mapping() {
        let mut map = NetworkEntityMap::new();
        let mut world = World::new();
        let entity1 = world.spawn_empty().id();
        let entity2 = world.spawn_empty().id();
        let network_id = uuid::Uuid::new_v4();

        // Insert first mapping
        map.insert(network_id, entity1);
        assert_eq!(map.get_entity(network_id), Some(entity1));

        // Insert same network_id with different entity
        map.insert(network_id, entity2);
        assert_eq!(map.get_entity(network_id), Some(entity2));
        assert_eq!(map.get_network_id(entity1), None); // Old mapping removed
        assert_eq!(map.len(), 1); // Still only one mapping
    }

    #[test]
    fn test_iter() {
        let mut map = NetworkEntityMap::new();
        let mut world = World::new();
        let entity1 = world.spawn_empty().id();
        let entity2 = world.spawn_empty().id();
        let id1 = uuid::Uuid::new_v4();
        let id2 = uuid::Uuid::new_v4();

        map.insert(id1, entity1);
        map.insert(id2, entity2);

        let mut count = 0;
        for (network_id, entity) in map.iter() {
            assert!(network_id == &id1 || network_id == &id2);
            assert!(entity == &entity1 || entity == &entity2);
            count += 1;
        }
        assert_eq!(count, 2);
    }
}
