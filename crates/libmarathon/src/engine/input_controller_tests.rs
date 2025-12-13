//! Unit tests for InputController

use super::{AccessibilitySettings, InputContext, InputController};
use crate::engine::game_actions::GameAction;
use crate::engine::input_events::{InputEvent, KeyCode, MouseButton, TouchPhase};
use glam::Vec2;

#[test]
fn test_mouse_left_drag_produces_move_entity() {
    let mut controller = InputController::new();

    // Mouse down at (100, 100)
    let actions = controller.process_event(&InputEvent::Mouse {
        pos: Vec2::new(100.0, 100.0),
        button: MouseButton::Left,
        phase: TouchPhase::Started,
    });

    // Should select entity and begin drag
    assert!(actions.iter().any(|a| matches!(a, GameAction::SelectEntity { .. })));
    assert!(actions.iter().any(|a| matches!(a, GameAction::BeginDrag { .. })));

    // Mouse drag to (150, 120)
    let actions = controller.process_event(&InputEvent::Mouse {
        pos: Vec2::new(150.0, 120.0),
        button: MouseButton::Left,
        phase: TouchPhase::Moved,
    });

    // Should produce MoveEntity with delta
    let move_action = actions.iter().find_map(|a| {
        if let GameAction::MoveEntity { delta } = a {
            Some(delta)
        } else {
            None
        }
    });

    assert!(move_action.is_some());
    let delta = move_action.unwrap();
    assert_eq!(*delta, Vec2::new(50.0, 20.0));
}

#[test]
fn test_mouse_right_drag_produces_rotate_entity() {
    let mut controller = InputController::new();

    // Right mouse down
    controller.process_event(&InputEvent::Mouse {
        pos: Vec2::new(100.0, 100.0),
        button: MouseButton::Right,
        phase: TouchPhase::Started,
    });

    // Right mouse drag
    let actions = controller.process_event(&InputEvent::Mouse {
        pos: Vec2::new(120.0, 130.0),
        button: MouseButton::Right,
        phase: TouchPhase::Moved,
    });

    // Should produce RotateEntity
    assert!(actions.iter().any(|a| matches!(a, GameAction::RotateEntity { .. })));
}

#[test]
fn test_mouse_wheel_produces_depth_movement() {
    let mut controller = InputController::new();

    let actions = controller.process_event(&InputEvent::MouseWheel {
        delta: Vec2::new(0.0, 10.0),
        pos: Vec2::new(100.0, 100.0),
    });

    // Should produce MoveEntityDepth
    let depth_action = actions.iter().find_map(|a| {
        if let GameAction::MoveEntityDepth { delta } = a {
            Some(*delta)
        } else {
            None
        }
    });

    assert_eq!(depth_action, Some(10.0));
}

#[test]
fn test_keyboard_r_resets_entity() {
    let mut controller = InputController::new();

    let actions = controller.process_event(&InputEvent::Keyboard {
        key: KeyCode::KeyR,
        pressed: true,
        modifiers: Default::default(),
    });

    assert!(actions.contains(&GameAction::ResetEntity));
}

#[test]
fn test_keyboard_delete_removes_entity() {
    let mut controller = InputController::new();

    let actions = controller.process_event(&InputEvent::Keyboard {
        key: KeyCode::Delete,
        pressed: true,
        modifiers: Default::default(),
    });

    assert!(actions.contains(&GameAction::DeleteEntity));
}

#[test]
fn test_drag_threshold_prevents_tiny_movements() {
    let mut controller = InputController::new();
    controller.set_accessibility(AccessibilitySettings {
        drag_threshold: 10.0,
        ..Default::default()
    });

    // Start drag
    controller.process_event(&InputEvent::Mouse {
        pos: Vec2::new(100.0, 100.0),
        button: MouseButton::Left,
        phase: TouchPhase::Started,
    });

    // Move only 2 pixels (below threshold of 10)
    let actions = controller.process_event(&InputEvent::Mouse {
        pos: Vec2::new(102.0, 100.0),
        button: MouseButton::Left,
        phase: TouchPhase::Moved,
    });

    // Should NOT produce MoveEntity (below threshold)
    assert!(!actions.iter().any(|a| matches!(a, GameAction::MoveEntity { .. })));

    // Move 15 pixels total (above threshold)
    let actions = controller.process_event(&InputEvent::Mouse {
        pos: Vec2::new(115.0, 100.0),
        button: MouseButton::Left,
        phase: TouchPhase::Moved,
    });

    // NOW should produce MoveEntity
    assert!(actions.iter().any(|a| matches!(a, GameAction::MoveEntity { .. })));
}

#[test]
fn test_mouse_sensitivity_multiplier() {
    let mut controller = InputController::new();
    controller.set_accessibility(AccessibilitySettings {
        mouse_sensitivity: 2.0,
        ..Default::default()
    });

    // Start drag
    controller.process_event(&InputEvent::Mouse {
        pos: Vec2::new(100.0, 100.0),
        button: MouseButton::Left,
        phase: TouchPhase::Started,
    });

    // Move 10 pixels
    let actions = controller.process_event(&InputEvent::Mouse {
        pos: Vec2::new(110.0, 100.0),
        button: MouseButton::Left,
        phase: TouchPhase::Moved,
    });

    // Delta should be doubled (10 * 2.0 = 20)
    let delta = actions.iter().find_map(|a| {
        if let GameAction::MoveEntity { delta } = a {
            Some(*delta)
        } else {
            None
        }
    });

    assert_eq!(delta, Some(Vec2::new(20.0, 0.0)));
}

#[test]
fn test_invert_y_axis() {
    let mut controller = InputController::new();
    controller.set_accessibility(AccessibilitySettings {
        invert_y: true,
        ..Default::default()
    });

    // Start right-click drag
    controller.process_event(&InputEvent::Mouse {
        pos: Vec2::new(100.0, 100.0),
        button: MouseButton::Right,
        phase: TouchPhase::Started,
    });

    // Drag down (positive Y)
    let actions = controller.process_event(&InputEvent::Mouse {
        pos: Vec2::new(100.0, 110.0),
        button: MouseButton::Right,
        phase: TouchPhase::Moved,
    });

    // Y delta should be inverted
    let delta = actions.iter().find_map(|a| {
        if let GameAction::RotateEntity { delta } = a {
            Some(*delta)
        } else {
            None
        }
    });

    assert!(delta.is_some());
    assert!(delta.unwrap().y < 0.0); // Should be negative (inverted)
}

#[test]
fn test_drag_sequence_produces_begin_continue_end() {
    let mut controller = InputController::new();

    // Started
    let actions = controller.process_event(&InputEvent::Mouse {
        pos: Vec2::new(100.0, 100.0),
        button: MouseButton::Left,
        phase: TouchPhase::Started,
    });
    assert!(actions.iter().any(|a| matches!(a, GameAction::BeginDrag { .. })));

    // Moved
    let actions = controller.process_event(&InputEvent::Mouse {
        pos: Vec2::new(150.0, 100.0),
        button: MouseButton::Left,
        phase: TouchPhase::Moved,
    });
    assert!(actions.iter().any(|a| matches!(a, GameAction::ContinueDrag { .. })));

    // Ended
    let actions = controller.process_event(&InputEvent::Mouse {
        pos: Vec2::new(150.0, 100.0),
        button: MouseButton::Left,
        phase: TouchPhase::Ended,
    });
    assert!(actions.iter().any(|a| matches!(a, GameAction::EndDrag { .. })));
}

#[test]
fn test_stylus_produces_move_entity() {
    let mut controller = InputController::new();

    // Stylus down
    controller.process_event(&InputEvent::Stylus {
        pos: Vec2::new(100.0, 100.0),
        pressure: 0.5,
        tilt: Vec2::ZERO,
        phase: TouchPhase::Started,
        timestamp: 0.0,
    });

    // Stylus drag
    let actions = controller.process_event(&InputEvent::Stylus {
        pos: Vec2::new(150.0, 120.0),
        pressure: 0.8,
        tilt: Vec2::ZERO,
        phase: TouchPhase::Moved,
        timestamp: 0.016,
    });

    // Should produce MoveEntity
    assert!(actions.iter().any(|a| matches!(a, GameAction::MoveEntity { .. })));
}

#[test]
fn test_context_switching() {
    let mut controller = InputController::new();

    // Start in EntityManipulation context
    assert_eq!(controller.context(), InputContext::EntityManipulation);

    // Switch to CameraControl
    controller.set_context(InputContext::CameraControl);
    assert_eq!(controller.context(), InputContext::CameraControl);

    // Start drag
    controller.process_event(&InputEvent::Mouse {
        pos: Vec2::new(100.0, 100.0),
        button: MouseButton::Left,
        phase: TouchPhase::Started,
    });

    // Drag in CameraControl context
    let actions = controller.process_event(&InputEvent::Mouse {
        pos: Vec2::new(150.0, 100.0),
        button: MouseButton::Left,
        phase: TouchPhase::Moved,
    });

    // Should produce MoveCamera instead of MoveEntity
    assert!(actions.iter().any(|a| matches!(a, GameAction::MoveCamera { .. })));
    assert!(!actions.iter().any(|a| matches!(a, GameAction::MoveEntity { .. })));
}

#[test]
fn test_scroll_sensitivity() {
    let mut controller = InputController::new();
    controller.set_accessibility(AccessibilitySettings {
        scroll_sensitivity: 3.0,
        ..Default::default()
    });

    let actions = controller.process_event(&InputEvent::MouseWheel {
        delta: Vec2::new(0.0, 5.0),
        pos: Vec2::ZERO,
    });

    // Delta should be tripled (5.0 * 3.0 = 15.0)
    let depth_delta = actions.iter().find_map(|a| {
        if let GameAction::MoveEntityDepth { delta } = a {
            Some(*delta)
        } else {
            None
        }
    });

    assert_eq!(depth_delta, Some(15.0));
}
