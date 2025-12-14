//! Platform abstraction layer
//!
//! This module provides platform-agnostic interfaces for OS/hardware interaction:
//! - **input**: Abstract input events (keyboard, mouse, touch, gestures)
//! - **desktop**: Concrete winit-based implementation for desktop platforms
//! - **ios**: Concrete UIKit-based implementation for iOS

pub mod input;

#[cfg(target_os = "ios")]
pub mod ios;

#[cfg(not(target_os = "ios"))]
pub mod desktop;
