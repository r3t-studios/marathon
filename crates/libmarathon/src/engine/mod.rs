//! Core Engine module - networking and persistence outside Bevy

mod bridge;
mod commands;
mod core;
mod events;
mod game_actions;
mod input_controller;
mod input_events;
mod networking;
mod persistence;

pub use bridge::{EngineBridge, EngineHandle};
pub use commands::EngineCommand;
pub use core::EngineCore;
pub use events::EngineEvent;
pub use game_actions::GameAction;
pub use input_controller::{AccessibilitySettings, InputContext, InputController};
pub use input_events::{InputEvent, KeyCode, Modifiers, MouseButton, TouchPhase};
pub use networking::NetworkingManager;
pub use persistence::PersistenceManager;
