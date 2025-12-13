//! iOS platform support
//!
//! This module contains iOS-specific input capture code.

pub mod pencil_bridge;

pub use pencil_bridge::{
    drain_as_input_events, drain_raw, pencil_point_received, swift_attach_pencil_capture,
    RawPencilPoint,
};
