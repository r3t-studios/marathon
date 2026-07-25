//! iOS platform support
//!
//! This module contains iOS-specific implementations:
//! - Apple Pencil input capture
//! - iOS executor (winit + UIKit integration)

pub mod executor;
pub mod pencil_bridge;

pub use executor::run_executor;
pub use pencil_bridge::{
    RawPencilPoint,
    drain_as_input_events,
    drain_raw,
    pencil_point_received,
    rust_push_pencil_point,
    swift_attach_pencil_capture,
    swift_detach_pencil_capture,
};
