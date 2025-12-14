//! Desktop platform integration
//!
//! Owns the winit event loop and converts winit events to InputEvents.

mod event_loop;
mod winit_bridge;

pub use event_loop::run;
pub use winit_bridge::{drain_as_input_events, push_window_event, set_scale_factor};
