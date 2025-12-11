//! Build CRDT operations from ECS component changes
//!
//! This module provides utilities to convert Bevy component changes into
//! ComponentOp operations that can be synchronized across the network.

use bevy::{
    prelude::*,
    reflect::TypeRegistry,
};

use crate::{
    networking::{
        blob_support::{
            BlobStore,
            create_component_data,
        },
        error::Result,
        messages::ComponentData,
        operations::{
            ComponentOp,
            ComponentOpBuilder,
        },
        vector_clock::{
            NodeId,
            VectorClock,
        },
    },
    persistence::reflection::serialize_component_typed,
};

/// Build a Set operation (LWW) from a component
///
/// Serializes the component using Bevy's reflection system and creates a
/// ComponentOp::Set for Last-Write-Wins synchronization. Automatically uses
/// blob storage for components >64KB.
///
/// # Parameters
///
/// - `component`: The component to serialize
/// - `component_type`: Type path string
/// - `node_id`: Our node ID
/// - `vector_clock`: Current vector clock
/// - `type_registry`: Bevy's type registry
/// - `blob_store`: Optional blob store for large components
///
/// # Returns
///
/// A ComponentOp::Set ready to be broadcast
pub fn build_set_operation(
    component: &dyn Reflect,
    component_type: String,
    node_id: NodeId,
    vector_clock: VectorClock,
    type_registry: &TypeRegistry,
    blob_store: Option<&BlobStore>,
) -> Result<ComponentOp> {
    // Serialize the component
    let serialized = serialize_component_typed(component, type_registry)?;

    // Create component data (inline or blob)
    let data = if let Some(store) = blob_store {
        create_component_data(serialized, store)?
    } else {
        ComponentData::Inline(serialized)
    };

    // Build the operation
    let builder = ComponentOpBuilder::new(node_id, vector_clock);
    Ok(builder.set(component_type, data))
}

/// Build Set operations for all components on an entity
///
/// This iterates over all components with reflection data and creates Set
/// operations for each one. Automatically uses blob storage for large
/// components.
///
/// # Parameters
///
/// - `entity`: The entity to serialize
/// - `world`: Bevy world
/// - `node_id`: Our node ID
/// - `vector_clock`: Current vector clock
/// - `type_registry`: Bevy's type registry
/// - `blob_store`: Optional blob store for large components
///
/// # Returns
///
/// Vector of ComponentOp::Set operations, one per component
pub fn build_entity_operations(
    entity: Entity,
    world: &World,
    node_id: NodeId,
    vector_clock: VectorClock,
    type_registry: &TypeRegistry,
    blob_store: Option<&BlobStore>,
) -> Vec<ComponentOp> {
    let mut operations = Vec::new();
    let entity_ref = world.entity(entity);

    debug!(
        "build_entity_operations: Building operations for entity {:?}",
        entity
    );

    // Iterate over all type registrations
    for registration in type_registry.iter() {
        // Skip if no ReflectComponent data
        let Some(reflect_component) = registration.data::<ReflectComponent>() else {
            continue;
        };

        // Get the type path
        let type_path = registration.type_info().type_path();

        // Skip certain components
        if type_path.ends_with("::NetworkedEntity") ||
            type_path.ends_with("::NetworkedTransform") ||
            type_path.ends_with("::NetworkedSelection") ||
            type_path.ends_with("::NetworkedDrawingPath")
        {
            continue;
        }

        // Try to reflect this component from the entity
        if let Some(reflected) = reflect_component.reflect(entity_ref) {
            // Serialize the component
            if let Ok(serialized) = serialize_component_typed(reflected, type_registry) {
                // Create component data (inline or blob)
                let data = if let Some(store) = blob_store {
                    if let Ok(component_data) = create_component_data(serialized, store) {
                        component_data
                    } else {
                        continue; // Skip this component if blob storage fails
                    }
                } else {
                    ComponentData::Inline(serialized)
                };

                // Build the operation
                let mut clock = vector_clock.clone();
                clock.increment(node_id);

                operations.push(ComponentOp::Set {
                    component_type: type_path.to_string(),
                    data,
                    vector_clock: clock.clone(),
                });

                debug!("  ✓ Added Set operation for {}", type_path);
            }
        }
    }

    debug!(
        "build_entity_operations: Built {} operations for entity {:?}",
        operations.len(),
        entity
    );
    operations
}

/// Build a Set operation for Transform component specifically
///
/// This is a helper for the common case of synchronizing Transform changes.
///
/// # Example
///
/// ```
/// use bevy::prelude::*;
/// use lib::networking::{
///     VectorClock,
///     build_transform_operation,
/// };
/// use uuid::Uuid;
///
/// # fn example(transform: &Transform, type_registry: &bevy::reflect::TypeRegistry) {
/// let node_id = Uuid::new_v4();
/// let clock = VectorClock::new();
///
/// let op = build_transform_operation(transform, node_id, clock, type_registry, None).unwrap();
/// # }
/// ```
pub fn build_transform_operation(
    transform: &Transform,
    node_id: NodeId,
    vector_clock: VectorClock,
    type_registry: &TypeRegistry,
    blob_store: Option<&BlobStore>,
) -> Result<ComponentOp> {
    // Use reflection to serialize Transform
    let serialized = serialize_component_typed(transform.as_reflect(), type_registry)?;

    // Create component data (inline or blob)
    let data = if let Some(store) = blob_store {
        create_component_data(serialized, store)?
    } else {
        ComponentData::Inline(serialized)
    };

    let builder = ComponentOpBuilder::new(node_id, vector_clock);
    Ok(builder.set(
        "bevy_transform::components::transform::Transform".to_string(),
        data,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_transform_operation() {
        let mut type_registry = TypeRegistry::new();
        type_registry.register::<Transform>();

        let transform = Transform::default();
        let node_id = uuid::Uuid::new_v4();
        let clock = VectorClock::new();

        let op =
            build_transform_operation(&transform, node_id, clock, &type_registry, None).unwrap();

        assert!(op.is_set());
        assert_eq!(
            op.component_type(),
            Some("bevy_transform::components::transform::Transform")
        );
        assert_eq!(op.vector_clock().get(node_id), 1);
    }

    #[test]
    fn test_build_entity_operations() {
        let mut world = World::new();
        let mut type_registry = TypeRegistry::new();

        // Register Transform
        type_registry.register::<Transform>();

        // Spawn entity with Transform
        let entity = world.spawn(Transform::from_xyz(1.0, 2.0, 3.0)).id();

        let node_id = uuid::Uuid::new_v4();
        let clock = VectorClock::new();

        let ops = build_entity_operations(entity, &world, node_id, clock, &type_registry, None);

        // Should have at least Transform operation
        assert!(!ops.is_empty());
        assert!(ops.iter().all(|op| op.is_set()));
    }

    #[test]
    fn test_vector_clock_increment() {
        let mut type_registry = TypeRegistry::new();
        type_registry.register::<Transform>();

        let transform = Transform::default();
        let node_id = uuid::Uuid::new_v4();
        let mut clock = VectorClock::new();

        let op1 =
            build_transform_operation(&transform, node_id, clock.clone(), &type_registry, None)
                .unwrap();
        assert_eq!(op1.vector_clock().get(node_id), 1);

        clock.increment(node_id);
        let op2 =
            build_transform_operation(&transform, node_id, clock.clone(), &type_registry, None)
                .unwrap();
        assert_eq!(op2.vector_clock().get(node_id), 2);
    }
}
