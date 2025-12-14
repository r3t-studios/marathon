//! Core Engine module - application logic and coordination
//!
//! This module handles the core application logic that sits between the
//! platform layer and the game systems:
//! - **bridge**: Communication bridge between async EngineCore and Bevy ECS
//! - **core**: Async EngineCore running on tokio (CRDT sync, networking, persistence)
//! - **commands**: Commands that can be sent to EngineCore
//! - **events**: Events emitted by EngineCore
//! - **game_actions**: High-level game actions (SelectEntity, MoveEntity, etc.)

mod bridge;
mod commands;
mod core;
mod events;
mod game_actions;
mod networking;
mod persistence;

pub use bridge::{EngineBridge, EngineHandle};
pub use commands::EngineCommand;
pub use core::EngineCore;
pub use events::EngineEvent;
pub use game_actions::GameAction;
pub use networking::NetworkingManager;
pub use persistence::PersistenceManager;
