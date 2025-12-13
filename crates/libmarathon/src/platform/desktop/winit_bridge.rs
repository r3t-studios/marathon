//! Desktop winit event loop integration
//!
//! This module owns the winit event loop and window, converting winit events
//! to engine-agnostic InputEvents.

use crate::engine::{InputEvent, KeyCode, Modifiers, MouseButton, TouchPhase};
use glam::Vec2;
use std::sync::Mutex;
use winit::event::{ElementState, MouseButton as WinitMouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::PhysicalKey;

/// Raw winit input events before conversion
#[derive(Clone, Debug)]
pub enum RawWinitEvent {
    MouseButton {
        button: MouseButton,
        state: ElementState,
        position: Vec2,
    },
    CursorMoved {
        position: Vec2,
    },
    Keyboard {
        key: KeyCode,
        state: ElementState,
        modifiers: Modifiers,
    },
    MouseWheel {
        delta: Vec2,
        position: Vec2,
    },
}

/// Thread-safe buffer for winit events
///
/// The winit event loop pushes events here.
/// The engine drains them each frame.
static BUFFER: Mutex<Vec<RawWinitEvent>> = Mutex::new(Vec::new());

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
/// Call this from the winit event loop
pub fn push_window_event(event: &WindowEvent) {
    match event {
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

                if let Ok(mut buf) = BUFFER.lock() {
                    buf.push(RawWinitEvent::MouseButton {
                        button: mouse_button,
                        state: *state,
                        position,
                    });
                }
            }
        }

        WindowEvent::CursorMoved { position, .. } => {
            let pos = Vec2::new(position.x as f32, position.y as f32);

            if let Ok(mut input_state) = INPUT_STATE.lock() {
                input_state.last_position = pos;

                // Generate drag events for any pressed buttons
                if input_state.left_pressed || input_state.right_pressed || input_state.middle_pressed {
                    if let Ok(mut buf) = BUFFER.lock() {
                        buf.push(RawWinitEvent::CursorMoved { position: pos });
                    }
                }
            }
        }

        WindowEvent::KeyboardInput { event: key_event, .. } => {
            // Only handle physical keys
            if let PhysicalKey::Code(key_code) = key_event.physical_key {
                if let Ok(input_state) = INPUT_STATE.lock() {
                    if let Ok(mut buf) = BUFFER.lock() {
                        buf.push(RawWinitEvent::Keyboard {
                            key: key_code,
                            state: key_event.state,
                            modifiers: input_state.modifiers,
                        });
                    }
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

        WindowEvent::MouseWheel { delta, .. } => {
            let scroll_delta = match delta {
                MouseScrollDelta::LineDelta(x, y) => Vec2::new(*x, *y) * 20.0, // Scale line deltas
                MouseScrollDelta::PixelDelta(pos) => Vec2::new(pos.x as f32, pos.y as f32),
            };

            if let Ok(input_state) = INPUT_STATE.lock() {
                if let Ok(mut buf) = BUFFER.lock() {
                    buf.push(RawWinitEvent::MouseWheel {
                        delta: scroll_delta,
                        position: input_state.last_position,
                    });
                }
            }
        }

        _ => {}
    }
}

/// Drain all buffered winit events and convert to InputEvents
///
/// Call this from your engine's input processing to consume events.
pub fn drain_as_input_events() -> Vec<InputEvent> {
    BUFFER
        .lock()
        .ok()
        .map(|mut b| {
            std::mem::take(&mut *b)
                .into_iter()
                .filter_map(raw_to_input_event)
                .collect()
        })
        .unwrap_or_default()
}

/// Convert a raw winit event to an engine InputEvent
fn raw_to_input_event(event: RawWinitEvent) -> Option<InputEvent> {
    match event {
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
            // Determine which button is pressed for drag events
            let input_state = INPUT_STATE.lock().ok()?;

            let button = if input_state.left_pressed {
                MouseButton::Left
            } else if input_state.right_pressed {
                MouseButton::Right
            } else if input_state.middle_pressed {
                MouseButton::Middle
            } else {
                return None; // No button pressed, ignore
            };

            Some(InputEvent::Mouse {
                pos: position,
                button,
                phase: TouchPhase::Moved,
            })
        }

        RawWinitEvent::Keyboard { key, state, modifiers } => {
            Some(InputEvent::Keyboard {
                key,
                pressed: state == ElementState::Pressed,
                modifiers,
            })
        }

        RawWinitEvent::MouseWheel { delta, position } => {
            Some(InputEvent::MouseWheel {
                delta,
                pos: position,
            })
        }
    }
}
