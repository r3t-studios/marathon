//! Transform component with rkyv support
//!
//! Vendored from bevy_transform with rkyv derives added for network
//! synchronization.

mod math;

pub use math::{
    Quat,
    Vec3,
};

/// Describe the position of an entity. If the entity has a parent, the position
/// is relative to its parent position.
///
/// This is a pure data type used for serialization. Use
/// bevy::transform::components::Transform for actual ECS components.
#[derive(Debug, PartialEq, Clone, Copy, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct Transform {
    /// Position of the entity. In 2d, the last value of the `Vec3` is used for
    /// z-ordering.
    pub translation: Vec3,
    /// Rotation of the entity.
    pub rotation: Quat,
    /// Scale of the entity.
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

impl Transform {
    /// Creates a new [`Transform`] at the position `(x, y, z)`. In 2d, the `z`
    /// component is used for z-ordering elements: higher `z`-value will be
    /// in front of lower `z`-value.
    #[inline]
    pub const fn from_xyz(x: f32, y: f32, z: f32) -> Self {
        Self::from_translation(Vec3::new(x, y, z))
    }

    /// Creates a new [`Transform`] with the specified `translation`.
    #[inline]
    pub const fn from_translation(translation: Vec3) -> Self {
        Self {
            translation,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }

    /// Creates a new [`Transform`] with the specified `rotation`.
    #[inline]
    pub const fn from_rotation(rotation: Quat) -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation,
            scale: Vec3::ONE,
        }
    }

    /// Creates a new [`Transform`] with the specified `scale`.
    #[inline]
    pub const fn from_scale(scale: Vec3) -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale,
        }
    }
}
