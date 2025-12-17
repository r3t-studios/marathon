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
        let mut clock = vector_clock.clone();
        clock.increment(node_id);

        operations.push(ComponentOp::Set {
            discriminant,
            data,
            vector_clock: clock.clone(),
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
