//! Reflection-based component serialization for persistence
//!
//! This module provides utilities to serialize and deserialize Bevy components
//! using reflection, allowing the persistence layer to work with any component
//! that implements Reflect.

use bevy::{
    prelude::*,
    reflect::{
        TypeRegistry,
        serde::{
            ReflectDeserializer,
            ReflectSerializer,
        },
    },
};

use crate::persistence::error::{
    PersistenceError,
    Result,
};

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
/// # use lib::persistence::*;
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

/// Trait for components that can be persisted
pub trait Persistable: Component + Reflect {
    /// Get the type name for this component (used as key in database)
    fn type_name() -> &'static str {
        std::any::type_name::<Self>()
    }
}

/// Serialize a component using Bevy's reflection system
///
/// This converts any component implementing `Reflect` into bytes for storage.
/// Uses bincode for efficient binary serialization with type information from
/// the registry to handle polymorphic types correctly.
///
/// # Parameters
/// - `component`: Component to serialize (must implement `Reflect`)
/// - `type_registry`: Bevy's type registry for reflection metadata
///
/// # Returns
/// - `Ok(Vec<u8>)`: Serialized component data
/// - `Err`: If serialization fails (e.g., type not properly registered)
///
/// # Examples
/// ```no_run
/// # use bevy::prelude::*;
/// # use lib::persistence::*;
/// # fn example(component: &Transform, registry: &AppTypeRegistry) -> anyhow::Result<()> {
/// let registry = registry.read();
/// let bytes = serialize_component(component.as_reflect(), &registry)?;
/// # Ok(())
/// # }
/// ```
pub fn serialize_component(
    component: &dyn Reflect,
    type_registry: &TypeRegistry,
) -> Result<Vec<u8>> {
    let serializer = ReflectSerializer::new(component, type_registry);
    bincode::serialize(&serializer).map_err(PersistenceError::from)
}

/// Deserialize a component using Bevy's reflection system
///
/// Converts serialized bytes back into a reflected component. The returned
/// component is boxed and must be downcast to the concrete type for use.
///
/// # Parameters
/// - `bytes`: Serialized component data from [`serialize_component`]
/// - `type_registry`: Bevy's type registry for reflection metadata
///
/// # Returns
/// - `Ok(Box<dyn PartialReflect>)`: Deserialized component (needs downcasting)
/// - `Err`: If deserialization fails (e.g., type not registered, data
///   corruption)
///
/// # Examples
/// ```no_run
/// # use bevy::prelude::*;
/// # use lib::persistence::*;
/// # fn example(bytes: &[u8], registry: &AppTypeRegistry) -> anyhow::Result<()> {
/// let registry = registry.read();
/// let reflected = deserialize_component(bytes, &registry)?;
/// // Downcast to concrete type as needed
/// # Ok(())
/// # }
/// ```
pub fn deserialize_component(
    bytes: &[u8],
    type_registry: &TypeRegistry,
) -> Result<Box<dyn PartialReflect>> {
    let mut deserializer = bincode::Deserializer::from_slice(bytes, bincode::options());
    let reflect_deserializer = ReflectDeserializer::new(type_registry);

    use serde::de::DeserializeSeed;
    reflect_deserializer
        .deserialize(&mut deserializer)
        .map_err(|e| PersistenceError::Deserialization(e.to_string()))
}

/// Serialize a component directly from an entity using its type path
///
/// This is a convenience function that combines type lookup, reflection, and
/// serialization. It's the primary method used by the persistence system to
/// save component state without knowing the concrete type at compile time.
///
/// # Parameters
/// - `entity`: Bevy entity to read the component from
/// - `component_type`: Type path string (e.g.,
///   "bevy_transform::components::Transform")
/// - `world`: Bevy world containing the entity
/// - `type_registry`: Bevy's type registry for reflection metadata
///
/// # Returns
/// - `Some(Vec<u8>)`: Serialized component data
/// - `None`: If entity doesn't have the component or type isn't registered
///
/// # Examples
/// ```no_run
/// # use bevy::prelude::*;
/// # use lib::persistence::*;
/// # fn example(entity: Entity, world: &World, registry: &AppTypeRegistry) -> Option<()> {
/// let registry = registry.read();
/// let bytes = serialize_component_from_entity(
///     entity,
///     "bevy_transform::components::Transform",
///     world,
///     &registry,
/// )?;
/// # Some(())
/// # }
/// ```
pub fn serialize_component_from_entity(
    entity: Entity,
    component_type: &str,
    world: &World,
    type_registry: &TypeRegistry,
) -> Option<Vec<u8>> {
    // Get the type registration
    let registration = type_registry.get_with_type_path(component_type)?;

    // Get the ReflectComponent data
    let reflect_component = registration.data::<ReflectComponent>()?;

    // Reflect the component from the entity
    let reflected = reflect_component.reflect(world.entity(entity))?;

    // Serialize it directly
    serialize_component(reflected, type_registry).ok()
}

/// Serialize all components from an entity that have reflection data
///
/// This iterates over all components on an entity and serializes those that:
/// - Are registered in the type registry
/// - Have `ReflectComponent` data (meaning they support reflection)
/// - Are not the `Persisted` marker component (to avoid redundant storage)
///
/// # Parameters
/// - `entity`: Bevy entity to serialize components from
/// - `world`: Bevy world containing the entity
/// - `type_registry`: Bevy's type registry for reflection metadata
///
/// # Returns
/// Vector of tuples containing (component_type_path, serialized_data) for each
/// component
pub fn serialize_all_components_from_entity(
    entity: Entity,
    world: &World,
    type_registry: &TypeRegistry,
) -> Vec<(String, Vec<u8>)> {
    let mut components = Vec::new();

    // Get the entity reference
    let entity_ref = world.entity(entity);

    // Iterate over all type registrations
    for registration in type_registry.iter() {
        // Skip if no ReflectComponent data (not a component)
        let Some(reflect_component) = registration.data::<ReflectComponent>() else {
            continue;
        };

        // Get the type path for this component
        let type_path = registration.type_info().type_path();

        // Skip the Persisted marker component itself (we don't need to persist it)
        if type_path.ends_with("::Persisted") {
            continue;
        }

        // Try to reflect this component from the entity
        if let Some(reflected) = reflect_component.reflect(entity_ref) {
            // Serialize the component
            if let Ok(data) = serialize_component(reflected, type_registry) {
                components.push((type_path.to_string(), data));
            }
        }
    }

    components
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component, Reflect, Default)]
    #[reflect(Component)]
    struct TestComponent {
        value: i32,
    }

    #[test]
    fn test_component_serialization() -> Result<()> {
        let mut registry = TypeRegistry::default();
        registry.register::<TestComponent>();

        let component = TestComponent { value: 42 };
        let bytes = serialize_component(&component, &registry)?;

        assert!(!bytes.is_empty());

        Ok(())
    }
}
