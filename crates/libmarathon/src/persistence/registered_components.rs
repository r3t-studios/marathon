//! Component registrations for CRDT synchronization
//!
//! This module registers all components that should be synchronized across
//! the network using the inventory-based type registry.
//!
//! # When to use this file vs `#[synced]` attribute
//!
//! **Use `#[synced]` attribute for:**
//! - Your own component types defined in this codebase
//! - Any type you have source access to
//! - Most game components (entities, markers, etc.)
//! - Example: `#[synced] pub struct CubeMarker { ... }`
//!
//! **Use manual `inventory::submit!` here for:**
//! - Third-party types (Bevy's Transform, external crates)
//! - Types that need custom serialization logic
//! - Types where the serialized format differs from in-memory format
//!
//! # Currently registered external types
//!
//! - `Transform` - Bevy's transform component (needs custom rkyv conversion)

use std::any::TypeId;

// Register Transform for synchronization
// We serialize Bevy's Transform but convert to our rkyv-compatible type
inventory::submit! {
    crate::persistence::ComponentMeta {
        type_name: "Transform",
        type_path: "bevy::transform::components::transform::Transform",
        type_id: TypeId::of::<bevy::prelude::Transform>(),

        deserialize_fn: |bytes: &[u8]| -> anyhow::Result<Box<dyn std::any::Any>> {
            let transform: crate::transform::Transform = rkyv::from_bytes::<crate::transform::Transform, rkyv::rancor::Failure>(bytes)?;
            // Convert back to Bevy Transform
            let bevy_transform = bevy::prelude::Transform {
                translation: transform.translation.into(),
                rotation: transform.rotation.into(),
                scale: transform.scale.into(),
            };
            Ok(Box::new(bevy_transform))
        },

        serialize_fn: |world: &bevy::ecs::world::World, entity: bevy::ecs::entity::Entity| -> Option<bytes::Bytes> {
            world.get::<bevy::prelude::Transform>(entity).map(|bevy_transform| {
                // Convert to our rkyv-compatible Transform
                let transform = crate::transform::Transform {
                    translation: bevy_transform.translation.into(),
                    rotation: bevy_transform.rotation.into(),
                    scale: bevy_transform.scale.into(),
                };
                let serialized = rkyv::to_bytes::<rkyv::rancor::Failure>(&transform)
                    .expect("Failed to serialize Transform");
                bytes::Bytes::from(serialized.to_vec())
            })
        },

        insert_fn: |entity_mut: &mut bevy::ecs::world::EntityWorldMut, boxed: Box<dyn std::any::Any>| {
            if let Ok(transform) = boxed.downcast::<bevy::prelude::Transform>() {
                entity_mut.insert(*transform);
            }
        },
    }
}
