//! Build CRDT operations from ECS component changes
//!
//! This module provides utilities to convert Bevy component changes into
//! ComponentOp operations that can be synchronized across the network.

use bevy::prelude::*;

use crate::networking::{
    blob_support::{
        BlobStore,
        create_component_data,
    },
    messages::ComponentData,
    operations::ComponentOp,
    vector_clock::{
        NodeId,
        VectorClock,
    },
};

/// Build Set operations for all components on an entity
///
/// This iterates over all registered Synced components and creates Set
/// operations for each one. Automatically uses blob storage for large
/// components.
///
/// # Parameters
///
/// - `entity`: The entity to serialize
/// - `world`: Bevy world
/// - `node_id`: Our node ID
/// - `vector_clock`: Current vector clock
/// - `type_registry`: Component type registry (for Synced components)
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
    type_registry: &crate::persistence::ComponentTypeRegistry,
    blob_store: Option<&BlobStore>,
) -> Vec<ComponentOp> {
    let mut operations = Vec::new();

    debug!(
        "build_entity_operations: Building operations for entity {:?}",
        entity
    );

    // Serialize all Synced components on this entity
    let serialized_components = type_registry.serialize_entity_components(world, entity);

    for (discriminant, _type_path, serialized) in serialized_components {
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
        // Use the vector_clock as-is - it's already been incremented by the caller (delta_generation.rs:116)
        // All operations in the same EntityDelta share the same vector clock (same logical timestamp)
        operations.push(ComponentOp::Set {
            discriminant,
            data,
            vector_clock: vector_clock.clone(),
        });

        debug!("  ✓ Added Set operation for discriminant {}", discriminant);
    }

    debug!(
        "build_entity_operations: Built {} operations for entity {:?}",
        operations.len(),
        entity
    );
    operations
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;
    use crate::networking::NetworkedEntity;
    use crate::persistence::{ComponentTypeRegistry, Persisted};

    #[test]
    fn test_operations_use_passed_vector_clock_without_extra_increment() {
        // Setup: Create a minimal world with an entity
        let mut world = World::new();
        let node_id = uuid::Uuid::new_v4();

        // Use the global registry (Transform is already registered via inventory)
        let registry = ComponentTypeRegistry::init();

        // Create test entity with Transform
        let entity_id = uuid::Uuid::new_v4();
        let entity = world.spawn((
            NetworkedEntity::with_id(entity_id, node_id),
            Persisted::with_id(entity_id),
            Transform::from_xyz(1.0, 2.0, 3.0),
        )).id();

        // Create a vector clock that's already been ticked
        let mut vector_clock = VectorClock::new();
        vector_clock.increment(node_id); // Simulate the tick that delta_generation does
        let expected_clock = vector_clock.clone();

        // Build operations
        let operations = build_entity_operations(
            entity,
            &world,
            node_id,
            vector_clock.clone(),
            &registry,
            None,
        );

        // Verify: All operations should use the EXACT clock that was passed in
        assert!(!operations.is_empty(), "Should have created at least one operation");

        for op in &operations {
            if let ComponentOp::Set { vector_clock: op_clock, .. } = op {
                assert_eq!(
                    *op_clock, expected_clock,
                    "Operation clock should match the input clock exactly. \
                     The bug was that operation_builder would increment the clock again, \
                     causing EntityDelta.vector_clock and ComponentOp.vector_clock to be misaligned."
                );

                // Verify the sequence number matches
                let op_seq = op_clock.get(node_id);
                let expected_seq = expected_clock.get(node_id);
                assert_eq!(
                    op_seq, expected_seq,
                    "Operation sequence should be {} (same as input clock), but got {}",
                    expected_seq, op_seq
                );
            }
        }
    }
}
