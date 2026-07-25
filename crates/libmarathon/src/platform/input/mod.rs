//! Platform input abstraction layer
//!
//! This module provides a platform-agnostic input event system that can be
//! implemented by different platforms (desktop/winit, iOS, Android, web).
//!
//! ## Architecture
//!
//! - **events.rs**: Platform-agnostic input event types (InputEvent,
//!   TouchPhase, etc.)
//! - **controller.rs**: Maps InputEvents to game-specific GameActions
//!
//! Platform implementations (like desktop/input.rs) convert native input to
//! InputEvents.

mod controller;
mod events;

pub use controller::{
    AccessibilitySettings,
    InputContext,
    InputController,
};
pub use events::{
    InputEvent,
    InputEventBuffer,
    KeyCode,
    Modifiers,
    MouseButton,
    TouchPhase,
};
