//! Desktop platform implementation (winit-based)
//!
//! This module provides the concrete desktop implementation of the platform
//! layer using winit.
//!
//! ## Modules
//!
//! - **executor**: Application executor that owns the winit event loop
//! - **input**: Winit event conversion to platform-agnostic InputEvents
//! - **event_loop**: Legacy event loop wrapper (to be removed)

mod event_loop;
mod executor;
mod input;

pub use event_loop::run;
pub use executor::run_executor;
pub use input::{
    drain_as_input_events,
    push_device_event,
    push_window_event,
    set_scale_factor,
};
