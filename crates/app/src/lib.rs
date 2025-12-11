//! Replicated cube demo app
//!
//! This app demonstrates real-time CRDT synchronization between iPad and Mac
//! with Apple Pencil input controlling a 3D cube.

pub mod camera;
pub mod cube;
pub mod debug_ui;
pub mod rendering;
pub mod setup;

pub use cube::CubeMarker;
