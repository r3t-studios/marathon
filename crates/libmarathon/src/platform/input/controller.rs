//! Input controller - maps raw InputEvents to semantic GameActions
//!
//! This layer provides:
//! - Input remapping (change key bindings)
//! - Accessibility (alternative input methods)
//! - Context-aware bindings (different actions in different modes)

use std::collections::HashMap;

use glam::Vec2;

use super::events::{
    InputEvent,
    KeyCode,
    MouseButton,
    TouchPhase,
};
use crate::engine::GameAction;

/// Input binding - maps an input trigger to a game action
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InputBinding {
    /// Mouse button press/release
    MouseButton(MouseButton),

    /// Mouse drag with a specific button
    MouseDrag(MouseButton),

    /// Mouse wheel scroll
    MouseWheel,

    /// Keyboard key press
    Key(KeyCode),

    /// Keyboard key with modifiers
    KeyWithModifiers {
        key: KeyCode,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    },

    /// Stylus input (Apple Pencil, etc.)
    StylusDrag,

    /// Touch input
    TouchDrag,
}

/// Input context - different binding sets for different game modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputContext {
    /// Manipulating 3D entities
    EntityManipulation,

    /// Camera control
    CameraControl,

    /// UI interaction
    UI,

    /// Text input
    TextInput,
}

/// Accessibility settings for input processing
#[derive(Debug, Clone)]
pub struct AccessibilitySettings {
    /// Mouse sensitivity multiplier (1.0 = normal)
    pub mouse_sensitivity: f32,

    /// Scroll sensitivity multiplier (1.0 = normal)
    pub scroll_sensitivity: f32,

    /// Stylus pressure sensitivity (1.0 = normal)
    pub stylus_sensitivity: f32,

    /// Enable one-handed mode (use keyboard for rotation)
    pub one_handed_mode: bool,

    /// Invert Y axis for rotation
    pub invert_y: bool,

    /// Minimum drag distance before registering as drag (in pixels)
    pub drag_threshold: f32,
}

impl Default for AccessibilitySettings {
    fn default() -> Self {
        Self {
            mouse_sensitivity: 1.0,
            scroll_sensitivity: 1.0,
            stylus_sensitivity: 1.0,
            one_handed_mode: false,
            invert_y: false,
            drag_threshold: 2.0,
        }
    }
}

/// Input controller - converts InputEvents to GameActions
pub struct InputController {
    /// Current input context
    current_context: InputContext,

    /// Bindings for each context
    bindings: HashMap<InputContext, HashMap<InputBinding, GameAction>>,

    /// Accessibility settings
    accessibility: AccessibilitySettings,

    /// Drag state tracking
    drag_state: DragState,
}

#[derive(Default)]
struct DragState {
    /// Is currently dragging
    active: bool,

    /// Which button/input is dragging
    source: Option<DragSource>,

    /// Start position
    start_pos: Vec2,

    /// Last position
    last_pos: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragSource {
    MouseLeft,
    MouseRight,
    Stylus,
    Touch,
}

impl InputController {
    /// Create a new input controller with default bindings
    pub fn new() -> Self {
        let mut controller = Self {
            current_context: InputContext::EntityManipulation,
            bindings: HashMap::new(),
            accessibility: AccessibilitySettings::default(),
            drag_state: DragState::default(),
        };

        controller.setup_default_bindings();
        controller
    }

    /// Set the current input context
    pub fn set_context(&mut self, context: InputContext) {
        self.current_context = context;
    }

    /// Get the current context
    pub fn context(&self) -> InputContext {
        self.current_context
    }

    /// Update accessibility settings
    pub fn set_accessibility(&mut self, settings: AccessibilitySettings) {
        self.accessibility = settings;
    }

    /// Get current accessibility settings
    pub fn accessibility(&self) -> &AccessibilitySettings {
        &self.accessibility
    }

    /// Process an input event and produce game actions
    pub fn process_event(&mut self, event: &InputEvent) -> Vec<GameAction> {
        let mut actions = Vec::new();

        match event {
            | InputEvent::MouseMove { pos: _ } => {
                // Mouse hover - no game actions, just UI tracking
                // This is handled by egui's custom_input_system
            },

            | InputEvent::Mouse { pos, button, phase } => {
                self.process_mouse(*pos, *button, *phase, &mut actions);
            },

            | InputEvent::MouseWheel { delta, pos: _ } => {
                let adjusted_delta = delta.y * self.accessibility.scroll_sensitivity;
                actions.push(GameAction::MoveEntityDepth {
                    delta: adjusted_delta,
                });
            },

            | InputEvent::Keyboard {
                key,
                pressed,
                modifiers: _,
            } => {
                if *pressed {
                    self.process_key(*key, &mut actions);
                }
            },

            | InputEvent::Stylus {
                pos,
                pressure: _,
                tilt: _,
                phase,
                timestamp: _,
            } => {
                self.process_stylus(*pos, *phase, &mut actions);
            },

            | InputEvent::Touch { pos, phase, id: _ } => {
                self.process_touch(*pos, *phase, &mut actions);
            },

            | InputEvent::PinchGesture { delta } => {
                // Pinch gesture - use for zoom/scale
                // Positive delta = pinch out (zoom in), negative = pinch in (zoom out)
                let adjusted_delta = delta * self.accessibility.scroll_sensitivity;
                actions.push(GameAction::MoveEntityDepth {
                    delta: adjusted_delta,
                });
            },

            | InputEvent::RotationGesture { delta } => {
                // Rotation gesture - use for rotating entities or camera
                let adjusted_delta = if self.accessibility.invert_y {
                    -*delta
                } else {
                    *delta
                };
                // Convert rotation delta to 2D delta for rotation action
                let delta_vec = Vec2::new(adjusted_delta, 0.0);
                actions.push(GameAction::RotateEntity { delta: delta_vec });
            },

            | InputEvent::PanGesture { delta } => {
                // Pan gesture - use for camera movement or entity translation
                let adjusted_delta = *delta * self.accessibility.mouse_sensitivity;
                match self.current_context {
                    | InputContext::CameraControl => {
                        actions.push(GameAction::MoveCamera {
                            delta: adjusted_delta,
                        });
                    },
                    | InputContext::EntityManipulation => {
                        actions.push(GameAction::MoveEntity {
                            delta: adjusted_delta,
                        });
                    },
                    | _ => {},
                }
            },

            | InputEvent::DoubleTapGesture => {
                // Double-tap gesture - quick reset/center action
                actions.push(GameAction::ResetEntity);
            },

            | InputEvent::MouseMotion { delta } => {
                // Raw mouse motion delta - only used in CameraControl mode for FPS-style camera
                // This is unbounded mouse movement (different from cursor position)
                // and would conflict with normal cursor-based dragging if used elsewhere
                if self.current_context == InputContext::CameraControl {
                    let adjusted_delta = *delta * self.accessibility.mouse_sensitivity;
                    actions.push(GameAction::MoveCamera {
                        delta: adjusted_delta,
                    });
                }
                // In other contexts, ignore MouseMotion to avoid conflicts with
                // cursor-based input
            },

            | InputEvent::Text { text: _ } => {
                // Text input is handled by egui, not by game actions
                // This is for typing in text fields, not game controls
            },
        }

        actions
    }

    /// Process mouse input
    fn process_mouse(
        &mut self,
        pos: Vec2,
        button: MouseButton,
        phase: TouchPhase,
        actions: &mut Vec<GameAction>,
    ) {
        match phase {
            | TouchPhase::Started => {
                // Single click = select
                actions.push(GameAction::SelectEntity { position: pos });

                // Start drag tracking
                self.drag_state.active = true;
                self.drag_state.source = Some(match button {
                    | MouseButton::Left => DragSource::MouseLeft,
                    | MouseButton::Right => DragSource::MouseRight,
                    | MouseButton::Middle => return, // Don't handle middle button
                });
                self.drag_state.start_pos = pos;
                self.drag_state.last_pos = pos;

                actions.push(GameAction::BeginDrag { position: pos });
            },

            | TouchPhase::Moved => {
                if self.drag_state.active {
                    let delta =
                        (pos - self.drag_state.last_pos) * self.accessibility.mouse_sensitivity;
                    self.drag_state.last_pos = pos;

                    // Check if we've exceeded drag threshold
                    let total_delta = pos - self.drag_state.start_pos;
                    if total_delta.length() < self.accessibility.drag_threshold {
                        return; // Too small to count as drag
                    }

                    actions.push(GameAction::ContinueDrag {
                        position: pos,
                        delta,
                    });

                    // Context-specific drag actions
                    match self.current_context {
                        | InputContext::EntityManipulation => match self.drag_state.source {
                            | Some(DragSource::MouseLeft) => {
                                actions.push(GameAction::MoveEntity { delta });
                            },
                            | Some(DragSource::MouseRight) => {
                                let adjusted_delta = if self.accessibility.invert_y {
                                    Vec2::new(delta.x, -delta.y)
                                } else {
                                    delta
                                };
                                actions.push(GameAction::RotateEntity {
                                    delta: adjusted_delta,
                                });
                            },
                            | _ => {},
                        },
                        | InputContext::CameraControl => {
                            actions.push(GameAction::MoveCamera { delta });
                        },
                        | _ => {},
                    }
                }
            },

            | TouchPhase::Ended | TouchPhase::Cancelled => {
                if self.drag_state.active {
                    actions.push(GameAction::EndDrag { position: pos });
                    self.drag_state.active = false;
                    self.drag_state.source = None;
                }
            },
        }
    }

    /// Process keyboard input
    fn process_key(&mut self, key: KeyCode, actions: &mut Vec<GameAction>) {
        match key {
            | KeyCode::KeyR => actions.push(GameAction::ResetEntity),
            | KeyCode::Delete | KeyCode::Backspace => actions.push(GameAction::DeleteEntity),
            | KeyCode::KeyZ if self.accessibility.one_handed_mode => {
                // In one-handed mode, Z key can trigger actions
                actions.push(GameAction::Undo);
            },
            | KeyCode::Escape => actions.push(GameAction::Cancel),
            | KeyCode::Enter => actions.push(GameAction::Confirm),
            | KeyCode::Tab => actions.push(GameAction::ToggleUI),
            | _ => {},
        }
    }

    /// Process stylus input (Apple Pencil, etc.)
    fn process_stylus(&mut self, pos: Vec2, phase: TouchPhase, actions: &mut Vec<GameAction>) {
        match phase {
            | TouchPhase::Started => {
                actions.push(GameAction::SelectEntity { position: pos });
                actions.push(GameAction::BeginDrag { position: pos });
                self.drag_state.active = true;
                self.drag_state.source = Some(DragSource::Stylus);
                self.drag_state.start_pos = pos;
                self.drag_state.last_pos = pos;
            },

            | TouchPhase::Moved => {
                if self.drag_state.active {
                    let delta =
                        (pos - self.drag_state.last_pos) * self.accessibility.stylus_sensitivity;
                    self.drag_state.last_pos = pos;

                    actions.push(GameAction::ContinueDrag {
                        position: pos,
                        delta,
                    });
                    actions.push(GameAction::MoveEntity { delta });
                }
            },

            | TouchPhase::Ended | TouchPhase::Cancelled => {
                if self.drag_state.active {
                    actions.push(GameAction::EndDrag { position: pos });
                    self.drag_state.active = false;
                    self.drag_state.source = None;
                }
            },
        }
    }

    /// Process touch input
    fn process_touch(&mut self, pos: Vec2, phase: TouchPhase, actions: &mut Vec<GameAction>) {
        // For now, treat touch like stylus
        self.process_stylus(pos, phase, actions);
    }

    /// Set up default input bindings
    fn setup_default_bindings(&mut self) {
        // For now, bindings are hardcoded in process_event
        // Later, we can make this fully data-driven
    }
}

impl Default for InputController {
    fn default() -> Self {
        Self::new()
    }
}

// Tests are in crates/libmarathon/src/engine/input_controller_tests.rs
// #[cfg(test)]
// #[path = "input_controller_tests.rs"]
// mod tests;
