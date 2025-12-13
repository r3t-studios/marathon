//! Platform-specific input bridges
//!
//! This module contains platform-specific code for capturing input
//! and converting it to engine-agnostic InputEvents.

#[cfg(target_os = "ios")]
pub mod ios;

#[cfg(not(target_os = "ios"))]
pub mod desktop;
