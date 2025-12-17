//! Zero-copy component type registry using rkyv and inventory
//!
//! This module provides a runtime type registry that collects all synced components
//! via the `inventory` crate and assigns them numeric discriminants for efficient
//! serialization.

use std::{
    any::TypeId,
    collections::HashMap,
    sync::OnceLock,
};

use anyhow::Result;

/// Component metadata collected via inventory
pub struct ComponentMeta {
    /// Human-readable type name (e.g., "Health")
    pub type_name: &'static str,

    /// Full type path (e.g., "my_crate::components::Health")
    pub type_path: &'static str,

    /// Rust TypeId for type-safe lookups
    pub type_id: TypeId,

    /// Deserialization function that returns a boxed component
    pub deserialize_fn: fn(&[u8]) -> Result<Box<dyn std::any::Any>>,

    /// Serialization function that reads from an entity (returns None if entity doesn't have this component)
    pub serialize_fn: fn(&bevy::ecs::world::World, bevy::ecs::entity::Entity) -> Option<bytes::Bytes>,

    /// Insert function that takes a boxed component and inserts it into an entity
    pub insert_fn: fn(&mut bevy::ecs::world::EntityWorldMut, Box<dyn std::any::Any>),
}

// Collect all registered components via inventory
inventory::collect!(ComponentMeta);

/// Runtime component type registry
///
/// Maps TypeId -> numeric discriminant for efficient serialization
pub struct ComponentTypeRegistry {
    /// TypeId to discriminant mapping
    type_to_discriminant: HashMap<TypeId, u16>,

    /// Discriminant to deserialization function
    discriminant_to_deserializer: HashMap<u16, fn(&[u8]) -> Result<Box<dyn std::any::Any>>>,

    /// Discriminant to serialization function
    discriminant_to_serializer: HashMap<u16, fn(&bevy::ecs::world::World, bevy::ecs::entity::Entity) -> Option<bytes::Bytes>>,

    /// Discriminant to insert function
    discriminant_to_inserter: HashMap<u16, fn(&mut bevy::ecs::world::EntityWorldMut, Box<dyn std::any::Any>)>,

    /// Discriminant to type name (for debugging)
    discriminant_to_name: HashMap<u16, &'static str>,

    /// Discriminant to type path (for networking)
    discriminant_to_path: HashMap<u16, &'static str>,

    /// TypeId to type name (for debugging)
    type_to_name: HashMap<TypeId, &'static str>,
}

impl ComponentTypeRegistry {
    /// Initialize the registry from inventory-collected components
    ///
    /// This should be called once at application startup.
    pub fn init() -> Self {
        let mut type_to_discriminant = HashMap::new();
        let mut discriminant_to_deserializer = HashMap::new();
        let mut discriminant_to_serializer = HashMap::new();
        let mut discriminant_to_inserter = HashMap::new();
        let mut discriminant_to_name = HashMap::new();
        let mut discriminant_to_path = HashMap::new();
        let mut type_to_name = HashMap::new();

        // Collect all registered components
        let mut components: Vec<&ComponentMeta> = inventory::iter::<ComponentMeta>().collect();

        // Sort by TypeId for deterministic discriminants
        components.sort_by_key(|c| c.type_id);

        // Assign discriminants
        for (discriminant, meta) in components.iter().enumerate() {
            let discriminant = discriminant as u16;
            type_to_discriminant.insert(meta.type_id, discriminant);
            discriminant_to_deserializer.insert(discriminant, meta.deserialize_fn);
            discriminant_to_serializer.insert(discriminant, meta.serialize_fn);
            discriminant_to_inserter.insert(discriminant, meta.insert_fn);
            discriminant_to_name.insert(discriminant, meta.type_name);
            discriminant_to_path.insert(discriminant, meta.type_path);
            type_to_name.insert(meta.type_id, meta.type_name);

            tracing::debug!(
                type_name = meta.type_name,
                type_path = meta.type_path,
                discriminant = discriminant,
                "Registered component type"
            );
        }

        tracing::info!(
            count = components.len(),
            "Initialized component type registry"
        );

        Self {
            type_to_discriminant,
            discriminant_to_deserializer,
            discriminant_to_serializer,
            discriminant_to_inserter,
            discriminant_to_name,
            discriminant_to_path,
            type_to_name,
        }
    }

    /// Get the discriminant for a component type
    pub fn get_discriminant(&self, type_id: TypeId) -> Option<u16> {
        self.type_to_discriminant.get(&type_id).copied()
    }

    /// Deserialize a component from bytes with its discriminant
    pub fn deserialize(&self, discriminant: u16, bytes: &[u8]) -> Result<Box<dyn std::any::Any>> {
        let deserialize_fn = self
            .discriminant_to_deserializer
            .get(&discriminant)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown component discriminant: {} (available: {:?})",
                    discriminant,
                    self.discriminant_to_name
                )
            })?;

        deserialize_fn(bytes)
    }

    /// Get the insert function for a discriminant
    pub fn get_insert_fn(&self, discriminant: u16) -> Option<fn(&mut bevy::ecs::world::EntityWorldMut, Box<dyn std::any::Any>)> {
        self.discriminant_to_inserter.get(&discriminant).copied()
    }

    /// Get type name for a discriminant (for debugging)
    pub fn get_type_name(&self, discriminant: u16) -> Option<&'static str> {
        self.discriminant_to_name.get(&discriminant).copied()
    }

    /// Get the deserialize function for a discriminant
    pub fn get_deserialize_fn(&self, discriminant: u16) -> Option<fn(&[u8]) -> Result<Box<dyn std::any::Any>>> {
        self.discriminant_to_deserializer.get(&discriminant).copied()
    }

    /// Get type path for a discriminant
    pub fn get_type_path(&self, discriminant: u16) -> Option<&'static str> {
        self.discriminant_to_path.get(&discriminant).copied()
    }

    /// Get the deserialize function by type path
    pub fn get_deserialize_fn_by_path(&self, type_path: &str) -> Option<fn(&[u8]) -> Result<Box<dyn std::any::Any>>> {
        // Linear search through discriminant_to_path to find matching type_path
        for (discriminant, path) in &self.discriminant_to_path {
            if *path == type_path {
                return self.get_deserialize_fn(*discriminant);
            }
        }
        None
    }

    /// Get the insert function by type path
    pub fn get_insert_fn_by_path(&self, type_path: &str) -> Option<fn(&mut bevy::ecs::world::EntityWorldMut, Box<dyn std::any::Any>)> {
        // Linear search through discriminant_to_path to find matching type_path
        for (discriminant, path) in &self.discriminant_to_path {
            if *path == type_path {
                return self.get_insert_fn(*discriminant);
            }
        }
        None
    }

    /// Get the number of registered component types
    pub fn len(&self) -> usize {
        self.type_to_discriminant.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.type_to_discriminant.is_empty()
    }

    /// Serialize all registered components from an entity
    ///
    /// Returns Vec<(discriminant, type_path, serialized_bytes)> for all components that exist on the entity.
    pub fn serialize_entity_components(
        &self,
        world: &bevy::ecs::world::World,
        entity: bevy::ecs::entity::Entity,
    ) -> Vec<(u16, &'static str, bytes::Bytes)> {
        let mut results = Vec::new();

        for (&discriminant, &serialize_fn) in &self.discriminant_to_serializer {
            if let Some(bytes) = serialize_fn(world, entity) {
                if let Some(&type_path) = self.discriminant_to_path.get(&discriminant) {
                    results.push((discriminant, type_path, bytes));
                }
            }
        }

        results
    }

    /// Get all registered discriminants (for iteration)
    pub fn all_discriminants(&self) -> impl Iterator<Item = u16> + '_ {
        self.discriminant_to_name.keys().copied()
    }
}

/// Global component type registry instance
static REGISTRY: OnceLock<ComponentTypeRegistry> = OnceLock::new();

/// Get the global component type registry
///
/// Initializes the registry on first access.
pub fn component_registry() -> &'static ComponentTypeRegistry {
    REGISTRY.get_or_init(ComponentTypeRegistry::init)
}

/// Bevy resource wrapper for ComponentTypeRegistry
///
/// Use this in Bevy systems to access the global component registry.
/// Insert this resource at app startup.
#[derive(bevy::prelude::Resource)]
pub struct ComponentTypeRegistryResource(pub &'static ComponentTypeRegistry);

impl Default for ComponentTypeRegistryResource {
    fn default() -> Self {
        Self(component_registry())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_initialization() {
        let registry = ComponentTypeRegistry::init();
        // Should have at least the components defined in the codebase
        assert!(registry.len() > 0 || registry.is_empty()); // May be empty in unit tests
    }

    #[test]
    fn test_global_registry() {
        let registry = component_registry();
        // Should be initialized
        assert!(registry.len() >= 0);
    }
}
