//! Semantic game actions
//!
//! Actions represent what the player wants to do, independent of how they
//! triggered it. This enables input remapping and accessibility.

use glam::Vec2;

/// High-level game actions that result from input processing
#[derive(Debug, Clone, PartialEq)]
pub enum GameAction {
    /// Move an entity in 2D (XY plane)
    MoveEntity {
        /// Movement delta (in screen/world space)
        delta: Vec2,
    },

    /// Rotate an entity
    RotateEntity {
        /// Rotation delta (yaw, pitch)
        delta: Vec2,
    },

    /// Move entity along Z axis (depth)
    MoveEntityDepth {
        /// Depth delta
        delta: f32,
    },

    /// Select/deselect an entity at a position
    SelectEntity {
        /// Screen position
        position: Vec2,
    },

    /// Begin dragging at a position
    BeginDrag {
        /// Screen position
        position: Vec2,
    },

    /// Continue dragging
    ContinueDrag {
        /// Current screen position
        position: Vec2,
        /// Delta since last drag event
        delta: Vec2,
    },

    /// End dragging
    EndDrag {
        /// Final screen position
        position: Vec2,
    },

    /// Reset entity to default state
    ResetEntity,

    /// Delete selected entity
    DeleteEntity,

    /// Spawn new entity at position
    SpawnEntity {
        /// Screen position
        position: Vec2,
    },

    /// Camera movement
    MoveCamera {
        /// Movement delta
        delta: Vec2,
    },

    /// Camera zoom
    ZoomCamera {
        /// Zoom delta
        delta: f32,
    },

    /// Toggle UI panel
    ToggleUI,

    /// Confirm action (Enter, Space, etc.)
    Confirm,

    /// Cancel action (Escape, etc.)
    Cancel,

    /// Undo last action
    Undo,

    /// Redo last undone action
    Redo,
}

impl GameAction {
    /// Get a human-readable description of this action
    pub fn description(&self) -> &'static str {
        match self {
            GameAction::MoveEntity { .. } => "Move entity in XY plane",
            GameAction::RotateEntity { .. } => "Rotate entity",
            GameAction::MoveEntityDepth { .. } => "Move entity along Z axis",
            GameAction::SelectEntity { .. } => "Select/deselect entity",
            GameAction::BeginDrag { .. } => "Begin dragging",
            GameAction::ContinueDrag { .. } => "Continue dragging",
            GameAction::EndDrag { .. } => "End dragging",
            GameAction::ResetEntity => "Reset entity to default",
            GameAction::DeleteEntity => "Delete selected entity",
            GameAction::SpawnEntity { .. } => "Spawn new entity",
            GameAction::MoveCamera { .. } => "Move camera",
            GameAction::ZoomCamera { .. } => "Zoom camera",
            GameAction::ToggleUI => "Toggle UI panel",
            GameAction::Confirm => "Confirm",
            GameAction::Cancel => "Cancel",
            GameAction::Undo => "Undo",
            GameAction::Redo => "Redo",
        }
    }
}
