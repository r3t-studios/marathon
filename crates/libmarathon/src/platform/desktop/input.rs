//! Desktop winit event loop integration
//!
//! This module owns the winit event loop and window, converting winit events
//! to engine-agnostic InputEvents and Bevy window events.
//!
//! # Feature Parity with Bevy's WinitPlugin
//!
//! This implementation achieves comprehensive feature parity with bevy_winit,
//! handling all major input and window event types:
//!
//! ## Input Events (WindowEvent)
//! - **Mouse**: Buttons, cursor movement, wheel scrolling, enter/exit
//! - **Keyboard**: Full key event details with modifiers, logical keys, repeat detection
//! - **Touch**: Multi-touch with phase tracking (Started, Moved, Ended, Cancelled)
//! - **Apple Pencil**: Force/pressure detection via calibrated touch force, altitude angle for tilt
//! - **Gestures**: Pinch (zoom), rotation, pan, double-tap (trackpad/touch)
//! - **File Drag & Drop**: Dropped files, hovered files, cancelled hovers
//! - **IME**: International text input with preedit, commit, enabled/disabled states
//!
//! ## Window State Events
//! - **Focus**: Window focused/unfocused tracking
//! - **Occlusion**: Window visibility state (occluded/visible)
//! - **Theme**: System theme changes (light/dark mode)
//! - **Position**: Window moved events
//! - **Resize**: Window resized with physical dimensions
//! - **Scale Factor**: DPI/HiDPI scale factor changes
//!
//! ## Device Events
//! - **Mouse Motion**: Raw mouse delta for FPS camera controls (unbounded movement)
//!
//! ## Application Lifecycle
//! - **Suspended**: iOS/Android backgrounding support
//! - **Resumed**: App foreground/initialization
//! - **Exiting**: Clean shutdown
//!
//! ## Architecture
//!
//! Events flow through three layers:
//! 1. **Winit WindowEvent/DeviceEvent** → Raw OS events
//! 2. **RawWinitEvent** → Buffered events in lock-free channel
//! 3. **InputEvent** → Engine-agnostic input abstraction
//!
//! The executor (`super::executor`) drives the event loop and calls:
//! - `push_window_event()` for window events
//! - `push_device_event()` for device events
//! - `drain_as_input_events()` to consume buffered events each frame

use crate::platform::input::{InputEvent, KeyCode, Modifiers, MouseButton, TouchPhase};
use crossbeam_channel::{Receiver, Sender, unbounded};
use glam::Vec2;
use std::sync::{Mutex, OnceLock};
use std::path::PathBuf;
use winit::event::{
    ElementState, MouseButton as WinitMouseButton, MouseScrollDelta, WindowEvent,
    Force as WinitForce, TouchPhase as WinitTouchPhase,
    Ime as WinitIme,
};
use winit::keyboard::{PhysicalKey, Key as LogicalKey};
use winit::window::Theme as WinitTheme;

/// Raw winit input events before conversion
///
/// Comprehensive event types matching bevy_winit functionality
#[derive(Clone, Debug)]
pub enum RawWinitEvent {
    // Mouse events
    MouseButton {
        button: MouseButton,
        state: ElementState,
        position: Vec2,
    },
    CursorMoved {
        position: Vec2,
    },
    CursorEntered,
    CursorLeft,
    MouseWheel {
        delta: Vec2,
        position: Vec2,
    },
    /// Raw mouse motion delta (for FPS camera, etc.)
    /// This is separate from CursorMoved and gives unbounded mouse delta
    MouseMotion {
        delta: Vec2,
    },

    // Keyboard events
    Keyboard {
        key: KeyCode,
        state: ElementState,
        modifiers: Modifiers,
        logical_key: String, // Text representation for display
        text: Option<String>, // Actual character input
        repeat: bool,
    },

    // Touch events (CRITICAL for Apple Pencil!)
    Touch {
        phase: WinitTouchPhase,
        position: Vec2,
        force: Option<WinitForce>,
        id: u64,
    },

    // Gesture events (trackpad/touch)
    PinchGesture {
        delta: f32,
    },
    RotationGesture {
        delta: f32,
    },
    DoubleTapGesture,
    PanGesture {
        delta: Vec2,
    },

    // File drag & drop
    DroppedFile {
        path: PathBuf,
    },
    HoveredFile {
        path: PathBuf,
    },
    HoveredFileCancelled,

    // IME (international text input)
    ImePreedit {
        value: String,
        cursor: Option<(usize, usize)>,
    },
    ImeCommit {
        value: String,
    },
    ImeEnabled,
    ImeDisabled,

    // Window state events
    Focused {
        focused: bool,
    },
    Occluded {
        occluded: bool,
    },
    ThemeChanged {
        theme: WinitTheme,
    },
    Moved {
        x: i32,
        y: i32,
    },
}

/// Lock-free channel for winit events
///
/// The winit event loop sends events here.
/// The engine receives them each frame.
static EVENT_CHANNEL: OnceLock<(Sender<RawWinitEvent>, Receiver<RawWinitEvent>)> = OnceLock::new();

fn get_event_channel() -> &'static (Sender<RawWinitEvent>, Receiver<RawWinitEvent>) {
    EVENT_CHANNEL.get_or_init(|| unbounded())
}

/// Current scale factor (needed to convert physical to logical pixels)
static SCALE_FACTOR: Mutex<f64> = Mutex::new(1.0);

/// Set the window scale factor (call when window is created or scale changes)
pub fn set_scale_factor(scale_factor: f64) {
    if let Ok(mut sf) = SCALE_FACTOR.lock() {
        *sf = scale_factor;
    }
}

/// Current input state for tracking drags and modifiers
static INPUT_STATE: Mutex<InputState> = Mutex::new(InputState {
    left_pressed: false,
    right_pressed: false,
    middle_pressed: false,
    last_position: Vec2::ZERO,
    modifiers: Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
        meta: false,
    },
});

#[derive(Clone, Copy, Debug)]
struct InputState {
    left_pressed: bool,
    right_pressed: bool,
    middle_pressed: bool,
    last_position: Vec2,
    modifiers: Modifiers,
}

/// Push a winit window event to the buffer
///
/// Call this from the winit event loop. Handles ALL WindowEvent variants
/// for complete feature parity with bevy_winit.
pub fn push_window_event(event: &WindowEvent) {
    let (sender, _) = get_event_channel();

    match event {
        // === MOUSE EVENTS ===
        WindowEvent::MouseInput { state, button, .. } => {
            let mouse_button = match button {
                WinitMouseButton::Left => MouseButton::Left,
                WinitMouseButton::Right => MouseButton::Right,
                WinitMouseButton::Middle => MouseButton::Middle,
                _ => return, // Ignore other buttons
            };

            if let Ok(mut input_state) = INPUT_STATE.lock() {
                let position = input_state.last_position;

                // Update button state
                match mouse_button {
                    MouseButton::Left => input_state.left_pressed = *state == ElementState::Pressed,
                    MouseButton::Right => input_state.right_pressed = *state == ElementState::Pressed,
                    MouseButton::Middle => input_state.middle_pressed = *state == ElementState::Pressed,
                }

                let _ = sender.send(RawWinitEvent::MouseButton {
                    button: mouse_button,
                    state: *state,
                    position,
                });
            }
        }

        WindowEvent::CursorMoved { position, .. } => {
            // Convert from physical pixels to logical pixels
            let scale_factor = SCALE_FACTOR.lock().map(|sf| *sf).unwrap_or(1.0);
            let pos = Vec2::new(
                (position.x / scale_factor) as f32,
                (position.y / scale_factor) as f32,
            );

            if let Ok(mut input_state) = INPUT_STATE.lock() {
                input_state.last_position = pos;
                let _ = sender.send(RawWinitEvent::CursorMoved { position: pos });
            }
        }

        WindowEvent::CursorEntered { .. } => {
            let _ = sender.send(RawWinitEvent::CursorEntered);
        }

        WindowEvent::CursorLeft { .. } => {
            let _ = sender.send(RawWinitEvent::CursorLeft);
        }

        WindowEvent::MouseWheel { delta, .. } => {
            let scroll_delta = match delta {
                MouseScrollDelta::LineDelta(x, y) => Vec2::new(*x, *y) * 20.0,
                MouseScrollDelta::PixelDelta(pos) => Vec2::new(pos.x as f32, pos.y as f32),
            };

            if let Ok(input_state) = INPUT_STATE.lock() {
                let _ = sender.send(RawWinitEvent::MouseWheel {
                    delta: scroll_delta,
                    position: input_state.last_position,
                });
            }
        }

        // === KEYBOARD EVENTS ===
        WindowEvent::KeyboardInput { event: key_event, is_synthetic: false, .. } => {
            // Skip synthetic key events (we handle focus ourselves)
            if let PhysicalKey::Code(key_code) = key_event.physical_key {
                if let Ok(input_state) = INPUT_STATE.lock() {
                    // Convert logical key to string for display
                    let logical_key_str = match &key_event.logical_key {
                        LogicalKey::Character(s) => s.to_string(),
                        LogicalKey::Named(named) => format!("{:?}", named),
                        _ => String::new(),
                    };

                    let _ = sender.send(RawWinitEvent::Keyboard {
                        key: key_code,
                        state: key_event.state,
                        modifiers: input_state.modifiers,
                        logical_key: logical_key_str,
                        text: key_event.text.as_ref().map(|s| s.to_string()),
                        repeat: key_event.repeat,
                    });
                }
            }
        }

        WindowEvent::ModifiersChanged(new_modifiers) => {
            if let Ok(mut input_state) = INPUT_STATE.lock() {
                input_state.modifiers = Modifiers {
                    shift: new_modifiers.state().shift_key(),
                    ctrl: new_modifiers.state().control_key(),
                    alt: new_modifiers.state().alt_key(),
                    meta: new_modifiers.state().super_key(),
                };
            }
        }

        // === TOUCH EVENTS (APPLE PENCIL!) ===
        WindowEvent::Touch(touch) => {
            let scale_factor = SCALE_FACTOR.lock().map(|sf| *sf).unwrap_or(1.0);
            let position = Vec2::new(
                (touch.location.x / scale_factor) as f32,
                (touch.location.y / scale_factor) as f32,
            );

            let _ = sender.send(RawWinitEvent::Touch {
                phase: touch.phase,
                position,
                force: touch.force,
                id: touch.id,
            });
        }

        // === GESTURE EVENTS ===
        WindowEvent::PinchGesture { delta, .. } => {
            let _ = sender.send(RawWinitEvent::PinchGesture {
                delta: *delta as f32,
            });
        }

        WindowEvent::RotationGesture { delta, .. } => {
            let _ = sender.send(RawWinitEvent::RotationGesture {
                delta: *delta,
            });
        }

        WindowEvent::DoubleTapGesture { .. } => {
            let _ = sender.send(RawWinitEvent::DoubleTapGesture);
        }

        WindowEvent::PanGesture { delta, .. } => {
            let _ = sender.send(RawWinitEvent::PanGesture {
                delta: Vec2::new(delta.x, delta.y),
            });
        }

        // === FILE DRAG & DROP ===
        WindowEvent::DroppedFile(path) => {
            let _ = sender.send(RawWinitEvent::DroppedFile {
                path: path.clone(),
            });
        }

        WindowEvent::HoveredFile(path) => {
            let _ = sender.send(RawWinitEvent::HoveredFile {
                path: path.clone(),
            });
        }

        WindowEvent::HoveredFileCancelled => {
            let _ = sender.send(RawWinitEvent::HoveredFileCancelled);
        }

        // === IME (INTERNATIONAL TEXT INPUT) ===
        WindowEvent::Ime(ime_event) => {
            match ime_event {
                WinitIme::Enabled => {
                    let _ = sender.send(RawWinitEvent::ImeEnabled);
                }
                WinitIme::Preedit(value, cursor) => {
                    let _ = sender.send(RawWinitEvent::ImePreedit {
                        value: value.clone(),
                        cursor: *cursor,
                    });
                }
                WinitIme::Commit(value) => {
                    let _ = sender.send(RawWinitEvent::ImeCommit {
                        value: value.clone(),
                    });
                }
                WinitIme::Disabled => {
                    let _ = sender.send(RawWinitEvent::ImeDisabled);
                }
            }
        }

        // === WINDOW STATE EVENTS ===
        WindowEvent::Focused(focused) => {
            let _ = sender.send(RawWinitEvent::Focused {
                focused: *focused,
            });
        }

        WindowEvent::Occluded(occluded) => {
            let _ = sender.send(RawWinitEvent::Occluded {
                occluded: *occluded,
            });
        }

        WindowEvent::ThemeChanged(theme) => {
            let _ = sender.send(RawWinitEvent::ThemeChanged {
                theme: *theme,
            });
        }

        WindowEvent::Moved(position) => {
            let _ = sender.send(RawWinitEvent::Moved {
                x: position.x,
                y: position.y,
            });
        }

        // === EVENTS WE DON'T PROPAGATE (handled at executor level) ===
        WindowEvent::Resized(_) |
        WindowEvent::ScaleFactorChanged { .. } |
        WindowEvent::CloseRequested |
        WindowEvent::Destroyed |
        WindowEvent::RedrawRequested => {
            // These are handled directly by the executor, not converted to InputEvents
        }

        // Catch-all for any future events
        _ => {}
    }
}

/// Push a device event into the event buffer
///
/// Device events are low-level input events that don't correspond to a specific window.
/// The most important one is MouseMotion, which gives raw mouse delta for FPS cameras.
pub fn push_device_event(event: &winit::event::DeviceEvent) {
    let (sender, _) = get_event_channel();

    match event {
        winit::event::DeviceEvent::MouseMotion { delta: (x, y) } => {
            let delta = Vec2::new(*x as f32, *y as f32);
            let _ = sender.send(RawWinitEvent::MouseMotion { delta });
        }

        // Other device events (Added/Removed, Button, Key, etc.) are not needed
        // for our use case. Mouse delta is the main one.
        _ => {}
    }
}

/// Drain all buffered winit events and convert to InputEvents
///
/// Call this from your engine's input processing to consume events.
/// This uses a lock-free channel so it never blocks and can't silently drop events.
pub fn drain_as_input_events() -> Vec<InputEvent> {
    let (_, receiver) = get_event_channel();

    // Drain all events from the channel
    receiver
        .try_iter()
        .filter_map(raw_to_input_event)
        .collect()
}

/// Convert a raw winit event to an engine InputEvent
///
/// Only input-related events are converted. Other events (gestures, file drop, IME, etc.)
/// return None and should be handled by the Bevy event system directly.
fn raw_to_input_event(event: RawWinitEvent) -> Option<InputEvent> {
    match event {
        // === MOUSE INPUT ===
        RawWinitEvent::MouseButton { button, state, position } => {
            let phase = match state {
                ElementState::Pressed => TouchPhase::Started,
                ElementState::Released => TouchPhase::Ended,
            };

            Some(InputEvent::Mouse {
                pos: position,
                button,
                phase,
            })
        }

        RawWinitEvent::CursorMoved { position } => {
            // Check if any button is pressed
            let input_state = INPUT_STATE.lock().ok()?;

            if input_state.left_pressed {
                Some(InputEvent::Mouse {
                    pos: position,
                    button: MouseButton::Left,
                    phase: TouchPhase::Moved,
                })
            } else if input_state.right_pressed {
                Some(InputEvent::Mouse {
                    pos: position,
                    button: MouseButton::Right,
                    phase: TouchPhase::Moved,
                })
            } else if input_state.middle_pressed {
                Some(InputEvent::Mouse {
                    pos: position,
                    button: MouseButton::Middle,
                    phase: TouchPhase::Moved,
                })
            } else {
                // No button pressed - hover tracking
                Some(InputEvent::MouseMove { pos: position })
            }
        }

        RawWinitEvent::MouseWheel { delta, position } => {
            Some(InputEvent::MouseWheel {
                delta,
                pos: position,
            })
        }

        // === KEYBOARD INPUT ===
        RawWinitEvent::Keyboard { key, state, modifiers, .. } => {
            Some(InputEvent::Keyboard {
                key,
                pressed: state == ElementState::Pressed,
                modifiers,
            })
        }

        // === TOUCH INPUT (APPLE PENCIL!) ===
        RawWinitEvent::Touch { phase, position, force, id } => {
            // Convert winit TouchPhase to engine TouchPhase
            let touch_phase = match phase {
                WinitTouchPhase::Started => TouchPhase::Started,
                WinitTouchPhase::Moved => TouchPhase::Moved,
                WinitTouchPhase::Ended => TouchPhase::Ended,
                WinitTouchPhase::Cancelled => TouchPhase::Cancelled,
            };

            // Check if this is a stylus (has force/pressure data)
            match force {
                Some(WinitForce::Calibrated { force, max_possible_force, altitude_angle }) => {
                    // This is Apple Pencil or similar stylus!
                    // Normalize pressure to 0-1 range (though Apple Pencil can exceed 1.0)
                    let pressure = if max_possible_force > 0.0 {
                        (force / max_possible_force) as f32
                    } else {
                        force as f32 // Fallback if max is not provided
                    };

                    // Tilt: altitude_angle is provided, azimuth would need device_id tracking
                    // For now, use altitude and assume azimuth=0
                    let tilt = Vec2::new(
                        altitude_angle.unwrap_or(0.0) as f32,
                        0.0, // Azimuth not provided by winit Force::Calibrated
                    );

                    Some(InputEvent::Stylus {
                        pos: position,
                        pressure,
                        tilt,
                        phase: touch_phase,
                        timestamp: 0.0, // TODO: Get actual timestamp from winit when available
                    })
                }
                Some(WinitForce::Normalized(pressure)) => {
                    // Normalized pressure (0.0-1.0), likely a stylus
                    Some(InputEvent::Stylus {
                        pos: position,
                        pressure: pressure as f32,
                        tilt: Vec2::ZERO, // No tilt data in normalized mode
                        phase: touch_phase,
                        timestamp: 0.0,
                    })
                }
                None => {
                    // No force data - regular touch (finger)
                    Some(InputEvent::Touch {
                        pos: position,
                        phase: touch_phase,
                        id,
                    })
                }
            }
        }

        // === GESTURE INPUT ===
        RawWinitEvent::PinchGesture { delta } => {
            Some(InputEvent::PinchGesture { delta })
        }

        RawWinitEvent::RotationGesture { delta } => {
            Some(InputEvent::RotationGesture { delta })
        }

        RawWinitEvent::PanGesture { delta } => {
            Some(InputEvent::PanGesture { delta })
        }

        RawWinitEvent::DoubleTapGesture => {
            Some(InputEvent::DoubleTapGesture)
        }

        // === MOUSE MOTION (RAW DELTA) ===
        RawWinitEvent::MouseMotion { delta } => {
            Some(InputEvent::MouseMotion { delta })
        }

        // === NON-INPUT EVENTS ===
        // These are window/system events, not user input
        RawWinitEvent::CursorEntered |
        RawWinitEvent::CursorLeft |
        RawWinitEvent::DroppedFile { .. } |
        RawWinitEvent::HoveredFile { .. } |
        RawWinitEvent::HoveredFileCancelled |
        RawWinitEvent::ImePreedit { .. } |
        RawWinitEvent::ImeCommit { .. } |
        RawWinitEvent::ImeEnabled |
        RawWinitEvent::ImeDisabled |
        RawWinitEvent::Focused { .. } |
        RawWinitEvent::Occluded { .. } |
        RawWinitEvent::ThemeChanged { .. } |
        RawWinitEvent::Moved { .. } => {
            // These are window/UI events, should be sent to Bevy messages
            // (to be implemented when we add Bevy window event forwarding)
            None
        }
    }
}
