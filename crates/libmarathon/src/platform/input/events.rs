//! Abstract input event types for the engine
//!
//! These types are platform-agnostic and represent all forms of input
//! (stylus, mouse, touch) in a unified way. Platform-specific code
//! (iOS pencil bridge, desktop mouse) converts to these types.

use glam::Vec2;

/// Phase of a touch/stylus/mouse input
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    /// Input just started
    Started,
    /// Input moved
    Moved,
    /// Input ended normally
    Ended,
    /// Input was cancelled (e.g., system gesture)
    Cancelled,
}

/// Mouse button types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Keyboard key (using winit's KeyCode for now - can abstract later)
pub use winit::keyboard::KeyCode;

/// Keyboard modifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool, // Command on macOS, Windows key on Windows
}

/// Input event buffer for Bevy ECS integration
///
/// The executor fills this buffer each frame with input events from winit,
/// and Bevy systems (like egui) consume these events.
#[derive(bevy::prelude::Resource, Default, Clone)]
pub struct InputEventBuffer {
    pub events: Vec<InputEvent>,
}

/// Abstract input event that the engine processes
///
/// Platform-specific code converts native input (UITouch, winit events)
/// into these engine-agnostic events.
#[derive(Debug, Clone, Copy)]
pub enum InputEvent {
    /// Stylus input (Apple Pencil, Surface Pen, etc.)
    Stylus {
        /// Screen position in pixels
        pos: Vec2,
        /// Pressure (0.0 = no pressure, 1.0+ = max pressure)
        /// Note: Apple Pencil reports 0.0-4.0 range
        pressure: f32,
        /// Tilt vector:
        /// - x: altitude angle (0 = flat on screen, π/2 = perpendicular)
        /// - y: azimuth angle (rotation around vertical axis)
        tilt: Vec2,
        /// Touch phase
        phase: TouchPhase,
        /// Platform timestamp (for input prediction)
        timestamp: f64,
    },

    /// Mouse input (desktop)
    Mouse {
        /// Screen position in pixels
        pos: Vec2,
        /// Which button
        button: MouseButton,
        /// Touch phase
        phase: TouchPhase,
    },

    /// Mouse cursor movement (no button pressed)
    /// This is separate from Mouse to distinguish hover from drag
    MouseMove {
        /// Screen position in pixels
        pos: Vec2,
    },

    /// Touch input (fingers on touchscreen)
    Touch {
        /// Screen position in pixels
        pos: Vec2,
        /// Touch phase
        phase: TouchPhase,
        /// Touch ID (for multi-touch tracking)
        id: u64,
    },

    /// Keyboard input
    Keyboard {
        /// Physical key code
        key: KeyCode,
        /// Whether the key was pressed or released
        pressed: bool,
        /// Modifier keys held during the event
        modifiers: Modifiers,
    },

    /// Mouse wheel scroll
    MouseWheel {
        /// Scroll delta (pixels or lines depending on device)
        delta: Vec2,
        /// Current mouse position
        pos: Vec2,
    },

    /// Raw mouse motion delta (for FPS camera, etc.)
    /// This is unbounded mouse movement, separate from cursor position
    MouseMotion {
        /// Raw mouse delta
        delta: Vec2,
    },

    /// Pinch gesture (trackpad/touch - zoom in/out)
    PinchGesture {
        /// Delta amount (positive = zoom in, negative = zoom out)
        delta: f32,
    },

    /// Rotation gesture (trackpad/touch - rotate with two fingers)
    RotationGesture {
        /// Rotation delta in radians
        delta: f32,
    },

    /// Pan gesture (trackpad/touch - swipe with two fingers)
    PanGesture {
        /// Pan delta
        delta: Vec2,
    },

    /// Double-tap gesture (trackpad/touch)
    DoubleTapGesture,
}

impl InputEvent {
    /// Get the position for positional input types
    pub fn position(&self) -> Option<Vec2> {
        match self {
            InputEvent::Stylus { pos, .. } => Some(*pos),
            InputEvent::Mouse { pos, .. } => Some(*pos),
            InputEvent::MouseMove { pos } => Some(*pos),
            InputEvent::Touch { pos, .. } => Some(*pos),
            InputEvent::MouseWheel { pos, .. } => Some(*pos),
            InputEvent::Keyboard { .. } |
            InputEvent::MouseMotion { .. } |
            InputEvent::PinchGesture { .. } |
            InputEvent::RotationGesture { .. } |
            InputEvent::PanGesture { .. } |
            InputEvent::DoubleTapGesture => None,
        }
    }

    /// Get the phase for input types that have phases
    pub fn phase(&self) -> Option<TouchPhase> {
        match self {
            InputEvent::Stylus { phase, .. } => Some(*phase),
            InputEvent::Mouse { phase, .. } => Some(*phase),
            InputEvent::Touch { phase, .. } => Some(*phase),
            InputEvent::Keyboard { .. } |
            InputEvent::MouseWheel { .. } |
            InputEvent::MouseMove { .. } |
            InputEvent::MouseMotion { .. } |
            InputEvent::PinchGesture { .. } |
            InputEvent::RotationGesture { .. } |
            InputEvent::PanGesture { .. } |
            InputEvent::DoubleTapGesture => None,
        }
    }

    /// Check if this is an active input (not ended/cancelled)
    pub fn is_active(&self) -> bool {
        match self.phase() {
            Some(phase) => !matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled),
            None => true, // Gestures, keyboard, and wheel events are instantaneous
        }
    }
}
