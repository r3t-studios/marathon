//! Bridge Bevy's input to the engine's InputEvent system
//!
//! This temporarily reads Bevy's input and converts to InputEvents.
//! Later, we'll replace this with direct winit ownership.

use bevy::prelude::*;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::{MouseButtonInput, MouseWheel};
use bevy::window::CursorMoved;
use libmarathon::engine::{InputEvent, InputEventBuffer, KeyCode as EngineKeyCode, MouseButton as EngineMouseButton, TouchPhase, Modifiers};

/// Convert Bevy's Vec2 to glam::Vec2
///
/// Bevy re-exports glam types, so they're the same layout.
/// We just construct a new one to be safe.
#[inline]
fn to_glam_vec2(v: bevy::math::Vec2) -> glam::Vec2 {
    glam::Vec2::new(v.x, v.y)
}

/// Convert Bevy's KeyCode to engine's KeyCode (winit::keyboard::KeyCode)
///
/// Bevy re-exports winit's KeyCode but wraps it, so we need to extract it.
/// For now, we'll just match the common keys. TODO: Complete mapping.
fn bevy_to_engine_keycode(bevy_key: KeyCode) -> Option<EngineKeyCode> {
    // In Bevy 0.17, KeyCode variants match winit directly
    // We can use format matching as a temporary solution
    use EngineKeyCode as E;

    Some(match bevy_key {
        KeyCode::KeyA => E::KeyA,
        KeyCode::KeyB => E::KeyB,
        KeyCode::KeyC => E::KeyC,
        KeyCode::KeyD => E::KeyD,
        KeyCode::KeyE => E::KeyE,
        KeyCode::KeyF => E::KeyF,
        KeyCode::KeyG => E::KeyG,
        KeyCode::KeyH => E::KeyH,
        KeyCode::KeyI => E::KeyI,
        KeyCode::KeyJ => E::KeyJ,
        KeyCode::KeyK => E::KeyK,
        KeyCode::KeyL => E::KeyL,
        KeyCode::KeyM => E::KeyM,
        KeyCode::KeyN => E::KeyN,
        KeyCode::KeyO => E::KeyO,
        KeyCode::KeyP => E::KeyP,
        KeyCode::KeyQ => E::KeyQ,
        KeyCode::KeyR => E::KeyR,
        KeyCode::KeyS => E::KeyS,
        KeyCode::KeyT => E::KeyT,
        KeyCode::KeyU => E::KeyU,
        KeyCode::KeyV => E::KeyV,
        KeyCode::KeyW => E::KeyW,
        KeyCode::KeyX => E::KeyX,
        KeyCode::KeyY => E::KeyY,
        KeyCode::KeyZ => E::KeyZ,
        KeyCode::Digit1 => E::Digit1,
        KeyCode::Digit2 => E::Digit2,
        KeyCode::Digit3 => E::Digit3,
        KeyCode::Digit4 => E::Digit4,
        KeyCode::Digit5 => E::Digit5,
        KeyCode::Digit6 => E::Digit6,
        KeyCode::Digit7 => E::Digit7,
        KeyCode::Digit8 => E::Digit8,
        KeyCode::Digit9 => E::Digit9,
        KeyCode::Digit0 => E::Digit0,
        KeyCode::Space => E::Space,
        KeyCode::Enter => E::Enter,
        KeyCode::Escape => E::Escape,
        KeyCode::Backspace => E::Backspace,
        KeyCode::Tab => E::Tab,
        KeyCode::ShiftLeft => E::ShiftLeft,
        KeyCode::ShiftRight => E::ShiftRight,
        KeyCode::ControlLeft => E::ControlLeft,
        KeyCode::ControlRight => E::ControlRight,
        KeyCode::AltLeft => E::AltLeft,
        KeyCode::AltRight => E::AltRight,
        KeyCode::SuperLeft => E::SuperLeft,
        KeyCode::SuperRight => E::SuperRight,
        KeyCode::ArrowUp => E::ArrowUp,
        KeyCode::ArrowDown => E::ArrowDown,
        KeyCode::ArrowLeft => E::ArrowLeft,
        KeyCode::ArrowRight => E::ArrowRight,
        _ => return None, // Unmapped keys
    })
}

pub struct DesktopInputBridgePlugin;

impl Plugin for DesktopInputBridgePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InputEventBuffer>()
            .add_systems(PreUpdate, (
                clear_buffer,
                collect_mouse_buttons,
                collect_mouse_motion,
                collect_mouse_wheel,
                collect_keyboard,
            ).chain());
    }
}

/// Clear the buffer at the start of each frame
fn clear_buffer(mut buffer: ResMut<InputEventBuffer>) {
    buffer.events.clear();
}

/// Collect mouse button events
fn collect_mouse_buttons(
    mut buffer: ResMut<InputEventBuffer>,
    mut mouse_button_events: MessageReader<MouseButtonInput>,
    windows: Query<&Window>,
) {
    let cursor_pos = windows
        .single()
        .ok()
        .and_then(|w| w.cursor_position())
        .unwrap_or(Vec2::ZERO);

    for event in mouse_button_events.read() {
        let button = match event.button {
            MouseButton::Left => EngineMouseButton::Left,
            MouseButton::Right => EngineMouseButton::Right,
            MouseButton::Middle => EngineMouseButton::Middle,
            _ => continue,
        };

        let phase = if event.state.is_pressed() {
            TouchPhase::Started
        } else {
            TouchPhase::Ended
        };

        buffer.events.push(InputEvent::Mouse {
            pos: to_glam_vec2(cursor_pos),
            button,
            phase,
        });
    }
}

/// Collect mouse motion events (for hover and drag tracking)
fn collect_mouse_motion(
    mut buffer: ResMut<InputEventBuffer>,
    mut cursor_moved: MessageReader<CursorMoved>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
) {
    for event in cursor_moved.read() {
        let cursor_pos = event.position;

        // ALWAYS send MouseMove for cursor tracking (hover, tooltips, etc.)
        buffer.events.push(InputEvent::MouseMove {
            pos: to_glam_vec2(cursor_pos),
        });

        // ALSO generate drag events for currently pressed buttons
        if mouse_buttons.pressed(MouseButton::Left) {
            buffer.events.push(InputEvent::Mouse {
                pos: to_glam_vec2(cursor_pos),
                button: EngineMouseButton::Left,
                phase: TouchPhase::Moved,
            });
        }
        if mouse_buttons.pressed(MouseButton::Right) {
            buffer.events.push(InputEvent::Mouse {
                pos: to_glam_vec2(cursor_pos),
                button: EngineMouseButton::Right,
                phase: TouchPhase::Moved,
            });
        }
    }
}

/// Collect mouse wheel events
fn collect_mouse_wheel(
    mut buffer: ResMut<InputEventBuffer>,
    mut wheel_events: MessageReader<MouseWheel>,
    windows: Query<&Window>,
) {
    let cursor_pos = windows
        .single()
        .ok()
        .and_then(|w| w.cursor_position())
        .unwrap_or(Vec2::ZERO);

    for event in wheel_events.read() {
        buffer.events.push(InputEvent::MouseWheel {
            delta: to_glam_vec2(Vec2::new(event.x, event.y)),
            pos: to_glam_vec2(cursor_pos),
        });
    }
}

/// Collect keyboard events
fn collect_keyboard(
    mut buffer: ResMut<InputEventBuffer>,
    mut keyboard_events: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    for event in keyboard_events.read() {
        let modifiers = Modifiers {
            shift: keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
            ctrl: keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]),
            alt: keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]),
            meta: keys.any_pressed([KeyCode::SuperLeft, KeyCode::SuperRight]),
        };

        // Convert Bevy's KeyCode to engine's KeyCode
        if let Some(engine_key) = bevy_to_engine_keycode(event.key_code) {
            buffer.events.push(InputEvent::Keyboard {
                key: engine_key,
                pressed: event.state.is_pressed(),
                modifiers,
            });
        }
    }
}
