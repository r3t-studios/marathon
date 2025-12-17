//! Input handling for Aspen
//!
//! Input flow:
//! 1. Platform executor (desktop/iOS) captures native input
//! 2. Platform layer converts to InputEvents and populates InputEventBuffer
//! 3. InputHandler reads buffer and converts to GameActions
//! 4. GameActions are applied to entities

pub mod event_buffer;
pub mod input_handler;

pub use input_handler::InputHandlerPlugin;
