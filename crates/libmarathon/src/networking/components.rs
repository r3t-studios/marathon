//! Networked entity components
//!
//! This module defines components that mark entities as networked and track
//! their network identity across the distributed system.

use bevy::prelude::*;
use serde::{
    Deserialize,
    Serialize,
};

use crate::networking::vector_clock::NodeId;

/// Marker component to skip delta generation for one frame after receiving
/// remote updates
///
/// When we apply remote operations via `apply_entity_delta()`, the
/// `insert_fn()` call triggers Bevy's change detection. This would normally
/// cause `generate_delta_system` to create and broadcast a new delta, creating
/// an infinite feedback loop.
///
/// By adding this marker when we apply remote updates, we tell
/// `generate_delta_system` to skip this entity for one frame. A cleanup system
/// removes the marker after delta generation runs, allowing future local
/// changes to be broadcast normally.
///
/// This is an implementation detail of the feedback loop prevention mechanism.
/// User code should never need to interact with this component.
#[derive(Component, Debug)]
pub struct SkipNextDeltaGeneration;

/// Marker component indicating an entity should be synchronized over the
/// network
///
/// Add this component to any entity that should have its state synchronized
/// across peers. The networking system will automatically track changes and
/// broadcast deltas.
///
/// # Relationship with Persisted
///
/// NetworkedEntity and Persisted are complementary:
/// - `Persisted` - Entity state saved to local SQLite database
/// - `NetworkedEntity` - Entity state synchronized across network peers
///
/// Most entities will have both components for full durability and sync.
///
/// # Network Identity
///
/// Each networked entity has:
/// - `network_id` - Globally unique UUID for this entity across all peers
/// - `owner_node_id` - Node that originally created this entity
///
/// # Example
///
/// ```
/// use bevy::prelude::*;
/// use libmarathon::networking::NetworkedEntity;
/// use uuid::Uuid;
///
/// fn spawn_networked_entity(mut commands: Commands) {
///     let node_id = Uuid::new_v4();
///
///     commands.spawn((NetworkedEntity::new(node_id), Transform::default()));
/// }
/// ```
#[derive(Component, Reflect, Debug, Clone, Serialize, Deserialize)]
#[reflect(Component)]
pub struct NetworkedEntity {
    /// Globally unique network ID for this entity
    ///
    /// This ID is used to identify the entity across all peers in the network.
    /// When a peer receives an EntityDelta, it uses this ID to locate the
    /// corresponding local entity.
    pub network_id: uuid::Uuid,

    /// Node that created this entity
    ///
    /// Used for conflict resolution and ownership tracking. When two nodes
    /// concurrently create entities, the owner_node_id can be used as a
    /// tiebreaker.
    pub owner_node_id: NodeId,
}

impl NetworkedEntity {
    /// Create a new networked entity
    ///
    /// Generates a new random network_id and sets the owner to the specified
    /// node.
    ///
    /// # Example
    ///
    /// ```
    /// use libmarathon::networking::NetworkedEntity;
    /// use uuid::Uuid;
    ///
    /// let node_id = Uuid::new_v4();
    /// let entity = NetworkedEntity::new(node_id);
    ///
    /// assert_eq!(entity.owner_node_id, node_id);
    /// ```
    pub fn new(owner_node_id: NodeId) -> Self {
        Self {
            network_id: uuid::Uuid::new_v4(),
            owner_node_id,
        }
    }

    /// Create a networked entity with a specific network ID
    ///
    /// Used when receiving entities from remote peers - we need to use their
    /// network_id rather than generating a new one.
    ///
    /// # Example
    ///
    /// ```
    /// use libmarathon::networking::NetworkedEntity;
    /// use uuid::Uuid;
    ///
    /// let network_id = Uuid::new_v4();
    /// let owner_id = Uuid::new_v4();
    /// let entity = NetworkedEntity::with_id(network_id, owner_id);
    ///
    /// assert_eq!(entity.network_id, network_id);
    /// assert_eq!(entity.owner_node_id, owner_id);
    /// ```
    pub fn with_id(network_id: uuid::Uuid, owner_node_id: NodeId) -> Self {
        Self {
            network_id,
            owner_node_id,
        }
    }

    /// Check if this node owns the entity
    pub fn is_owned_by(&self, node_id: NodeId) -> bool {
        self.owner_node_id == node_id
    }
}

impl Default for NetworkedEntity {
    fn default() -> Self {
        Self {
            network_id: uuid::Uuid::new_v4(),
            owner_node_id: uuid::Uuid::new_v4(),
        }
    }
}

/// Wrapper for Transform component that enables CRDT synchronization
///
/// This is a marker component used alongside Transform to indicate that
/// Transform changes should be synchronized using Last-Write-Wins semantics.
///
/// # Example
///
/// ```
/// use bevy::prelude::*;
/// use libmarathon::networking::{
///     NetworkedEntity,
///     NetworkedTransform,
/// };
/// use uuid::Uuid;
///
/// fn spawn_synced_transform(mut commands: Commands) {
///     let node_id = Uuid::new_v4();
///
///     commands.spawn((
///         NetworkedEntity::new(node_id),
///         Transform::default(),
///         NetworkedTransform,
///     ));
/// }
/// ```
#[derive(Component, Reflect, Debug, Clone, Copy, Default)]
#[reflect(Component)]
pub struct NetworkedTransform;

/// Local selection tracking resource
///
/// This global resource tracks which entities are currently selected by THIS
/// node. It's used in conjunction with the entity lock system to coordinate
/// concurrent editing.
///
/// **Selections are local-only UI state** and are NOT synchronized across the
/// network. Each node maintains its own independent selection.
///
/// # Example
///
/// ```
/// use bevy::prelude::*;
/// use libmarathon::networking::LocalSelection;
/// use uuid::Uuid;
///
/// fn handle_click(mut selection: ResMut<LocalSelection>) {
///     // Clear previous selection
///     selection.clear();
///
///     // Select a new entity
///     selection.insert(Uuid::new_v4());
/// }
/// ```
#[derive(Resource, Debug, Clone, Default)]
pub struct LocalSelection {
    /// Set of selected entity network IDs
    selected_ids: std::collections::HashSet<uuid::Uuid>,
}

impl LocalSelection {
    /// Create a new empty selection
    pub fn new() -> Self {
        Self {
            selected_ids: std::collections::HashSet::new(),
        }
    }

    /// Add an entity to the selection
    pub fn insert(&mut self, entity_id: uuid::Uuid) -> bool {
        self.selected_ids.insert(entity_id)
    }

    /// Remove an entity from the selection
    pub fn remove(&mut self, entity_id: uuid::Uuid) -> bool {
        self.selected_ids.remove(&entity_id)
    }

    /// Check if an entity is selected
    pub fn contains(&self, entity_id: uuid::Uuid) -> bool {
        self.selected_ids.contains(&entity_id)
    }

    /// Clear all selections
    pub fn clear(&mut self) {
        self.selected_ids.clear();
    }

    /// Get the number of selected entities
    pub fn len(&self) -> usize {
        self.selected_ids.len()
    }

    /// Check if the selection is empty
    pub fn is_empty(&self) -> bool {
        self.selected_ids.is_empty()
    }

    /// Get an iterator over selected entity IDs
    pub fn iter(&self) -> impl Iterator<Item = &uuid::Uuid> {
        self.selected_ids.iter()
    }
}

/// Wrapper for a drawing path component using Sequence CRDT semantics
///
/// Represents an ordered sequence of points that can be collaboratively edited.
/// Uses RGA (Replicated Growable Array) CRDT to maintain consistent ordering
/// across concurrent insertions.
///
/// # RGA Semantics
///
/// - Each point has a unique operation ID
/// - Points reference the ID of the point they're inserted after
/// - Concurrent insertions maintain consistent ordering
///
/// # Example
///
/// ```
/// use bevy::prelude::*;
/// use libmarathon::networking::{
///     NetworkedDrawingPath,
///     NetworkedEntity,
/// };
/// use uuid::Uuid;
///
/// fn create_path(mut commands: Commands) {
///     let node_id = Uuid::new_v4();
///     let mut path = NetworkedDrawingPath::new();
///
///     // Add some points to the path
///     path.points.push(Vec2::new(0.0, 0.0));
///     path.points.push(Vec2::new(10.0, 10.0));
///     path.points.push(Vec2::new(20.0, 5.0));
///
///     commands.spawn((NetworkedEntity::new(node_id), path));
/// }
/// ```
#[derive(Component, Reflect, Debug, Clone, Default)]
#[reflect(Component)]
pub struct NetworkedDrawingPath {
    /// Ordered sequence of points in the path
    ///
    /// This will be synchronized using RGA (Sequence CRDT) semantics in later
    /// phases. For now, it's a simple Vec.
    pub points: Vec<Vec2>,

    /// Drawing stroke color
    pub color: Color,

    /// Stroke width
    pub width: f32,
}

impl NetworkedDrawingPath {
    /// Create a new empty drawing path
    pub fn new() -> Self {
        Self {
            points: Vec::new(),
            color: Color::BLACK,
            width: 2.0,
        }
    }

    /// Create a path with a specific color and width
    pub fn with_style(color: Color, width: f32) -> Self {
        Self {
            points: Vec::new(),
            color,
            width,
        }
    }

    /// Add a point to the end of the path
    pub fn push(&mut self, point: Vec2) {
        self.points.push(point);
    }

    /// Get the number of points in the path
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Check if the path is empty
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Clear all points from the path
    pub fn clear(&mut self) {
        self.points.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_networked_entity_new() {
        let node_id = uuid::Uuid::new_v4();
        let entity = NetworkedEntity::new(node_id);

        assert_eq!(entity.owner_node_id, node_id);
        assert_ne!(entity.network_id, uuid::Uuid::nil());
    }

    #[test]
    fn test_networked_entity_with_id() {
        let network_id = uuid::Uuid::new_v4();
        let owner_id = uuid::Uuid::new_v4();
        let entity = NetworkedEntity::with_id(network_id, owner_id);

        assert_eq!(entity.network_id, network_id);
        assert_eq!(entity.owner_node_id, owner_id);
    }

    #[test]
    fn test_networked_entity_is_owned_by() {
        let owner_id = uuid::Uuid::new_v4();
        let other_id = uuid::Uuid::new_v4();
        let entity = NetworkedEntity::new(owner_id);

        assert!(entity.is_owned_by(owner_id));
        assert!(!entity.is_owned_by(other_id));
    }

    #[test]
    fn test_local_selection() {
        let mut selection = LocalSelection::new();
        let id1 = uuid::Uuid::new_v4();
        let id2 = uuid::Uuid::new_v4();

        assert!(selection.is_empty());

        selection.insert(id1);
        assert_eq!(selection.len(), 1);
        assert!(selection.contains(id1));

        selection.insert(id2);
        assert_eq!(selection.len(), 2);
        assert!(selection.contains(id2));

        selection.remove(id1);
        assert_eq!(selection.len(), 1);
        assert!(!selection.contains(id1));

        selection.clear();
        assert!(selection.is_empty());
    }

    #[test]
    fn test_networked_drawing_path() {
        let mut path = NetworkedDrawingPath::new();

        assert!(path.is_empty());

        path.push(Vec2::new(0.0, 0.0));
        assert_eq!(path.len(), 1);

        path.push(Vec2::new(10.0, 10.0));
        assert_eq!(path.len(), 2);

        path.clear();
        assert!(path.is_empty());
    }

    #[test]
    fn test_drawing_path_with_style() {
        let path = NetworkedDrawingPath::with_style(Color::srgb(1.0, 0.0, 0.0), 5.0);

        assert_eq!(path.color, Color::srgb(1.0, 0.0, 0.0));
        assert_eq!(path.width, 5.0);
    }
}
