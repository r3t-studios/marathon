//! Apple Pencil input bridge for iOS
//!
//! This module captures raw Apple Pencil input via Swift/UIKit and converts
//! it to engine-agnostic InputEvents.

use std::sync::Mutex;

use glam::Vec2;

use crate::platform::input::{
    InputEvent,
    TouchPhase,
};

/// Raw pencil point data from Swift UITouch
///
/// This matches the C struct defined in PencilBridge.h
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)] // Use C memory layout so Swift can interop
pub struct RawPencilPoint {
    /// Screen X coordinate in points (not pixels)
    pub x: f32,
    /// Screen Y coordinate in points (not pixels)
    pub y: f32,
    /// Force/pressure (0.0 - 4.0 on Apple Pencil)
    pub force: f32,
    /// Altitude angle in radians (0 = flat, π/2 = perpendicular)
    pub altitude: f32,
    /// Azimuth angle in radians (rotation around vertical)
    pub azimuth: f32,
    /// iOS timestamp (seconds since system boot)
    pub timestamp: f64,
    /// Touch phase: 0=began, 1=moved, 2=ended
    pub phase: u8,
}

/// Thread-safe buffer for pencil points
///
/// Swift's main thread pushes points here via C FFI.
/// Bevy's Update schedule drains them each frame.
static BUFFER: Mutex<Vec<RawPencilPoint>> = Mutex::new(Vec::new());

/// FFI function called from Swift when a pencil point is received
///
/// This is exposed as a C function so Swift can call it.
/// The `#[no_mangle]` prevents Rust from changing the function name.
#[unsafe(no_mangle)]
pub extern "C" fn rust_push_pencil_point(point: RawPencilPoint) {
    if let Ok(mut buf) = BUFFER.lock() {
        buf.push(point);
    }
}

/// Legacy alias for compatibility
#[unsafe(no_mangle)]
pub extern "C" fn pencil_point_received(point: RawPencilPoint) {
    rust_push_pencil_point(point);
}

/// Drain all buffered pencil points and convert to InputEvents
///
/// Call this from your Bevy Update system to consume input.
pub fn drain_as_input_events() -> Vec<InputEvent> {
    BUFFER
        .lock()
        .ok()
        .map(|mut b| {
            std::mem::take(&mut *b)
                .into_iter()
                .map(raw_to_input_event)
                .collect()
        })
        .unwrap_or_default()
}

/// Drain raw pencil points without conversion
///
/// Useful for debugging or custom processing.
pub fn drain_raw() -> Vec<RawPencilPoint> {
    BUFFER
        .lock()
        .ok()
        .map(|mut b| std::mem::take(&mut *b))
        .unwrap_or_default()
}

/// Convert a raw pencil point to an engine InputEvent
fn raw_to_input_event(p: RawPencilPoint) -> InputEvent {
    InputEvent::Stylus {
        pos: Vec2::new(p.x, p.y),
        pressure: p.force,
        tilt: Vec2::new(p.altitude, p.azimuth),
        phase: match p.phase {
            | 0 => TouchPhase::Started,
            | 1 => TouchPhase::Moved,
            | 2 => TouchPhase::Ended,
            | _ => TouchPhase::Cancelled,
        },
        timestamp: p.timestamp,
    }
}

#[cfg(target_os = "ios")]
unsafe extern "C" {
    /// Attach the pencil capture system to a UIView
    pub fn swift_attach_pencil_capture(view: *mut std::ffi::c_void);
    /// Detach the pencil capture system from a UIView
    pub fn swift_detach_pencil_capture(view: *mut std::ffi::c_void);
}

#[cfg(not(target_os = "ios"))]
pub unsafe fn swift_attach_pencil_capture(_: *mut std::ffi::c_void) {
    // No-op on non-iOS platforms
}

#[cfg(not(target_os = "ios"))]
pub unsafe fn swift_detach_pencil_capture(_: *mut std::ffi::c_void) {
    // No-op on non-iOS platforms
}
