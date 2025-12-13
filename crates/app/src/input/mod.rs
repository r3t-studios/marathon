//! Input handling modules
//!
//! This module contains platform-specific input adapters that bridge
//! native input (Bevy/winit, iOS pencil) to libmarathon's InputEvent system.

pub mod event_buffer;
pub mod input_handler;

#[cfg(target_os = "ios")]
pub mod pencil;

#[cfg(not(target_os = "ios"))]
pub mod desktop_bridge;

#[cfg(not(target_os = "ios"))]
pub mod mouse;

pub use event_buffer::InputEventBuffer;
pub use input_handler::InputHandlerPlugin;

#[cfg(target_os = "ios")]
pub use pencil::PencilInputPlugin;

#[cfg(not(target_os = "ios"))]
pub use desktop_bridge::DesktopInputBridgePlugin;

#[cfg(not(target_os = "ios"))]
pub use mouse::MouseInputPlugin;
