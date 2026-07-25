//! Bridge between Bevy and Core Engine
//!
//! TODO(Phase 3): Create a Bevy-specific system (in app crate) that polls
//! `EngineBridge::poll_events()` every tick and dispatches EngineEvents to Bevy
//! (spawn entities, update transforms, update locks, emit Bevy messages, etc.)
//!
//! NOTE: The bridge is ECS-agnostic. Later we can create adapters for other
//! engines like Flecs once we're closer to release.

use std::sync::Arc;

use bevy::prelude::Resource;
use tokio::sync::{
    Mutex,
    mpsc,
};

use super::{
    EngineCommand,
    EngineEvent,
};

/// Shared bridge between Bevy and Core Engine
#[derive(Clone, Resource)]
pub struct EngineBridge {
    command_tx: mpsc::UnboundedSender<EngineCommand>,
    event_rx: Arc<Mutex<mpsc::UnboundedReceiver<EngineEvent>>>,
}

/// Engine-side handle for receiving commands and sending events
pub struct EngineHandle {
    pub(crate) command_rx: mpsc::UnboundedReceiver<EngineCommand>,
    pub(crate) event_tx: mpsc::UnboundedSender<EngineEvent>,
}

impl EngineBridge {
    /// Create a new bridge and return both the Bevy-side bridge and Engine-side
    /// handle
    pub fn new() -> (Self, EngineHandle) {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let bridge = Self {
            command_tx,
            event_rx: Arc::new(Mutex::new(event_rx)),
        };

        let handle = EngineHandle {
            command_rx,
            event_tx,
        };

        (bridge, handle)
    }

    /// Send command from Bevy to Engine
    pub fn send_command(&self, cmd: EngineCommand) {
        // Ignore send errors (engine might be shut down)
        let _ = self.command_tx.send(cmd);
    }

    /// Poll events from Engine to Bevy (non-blocking)
    /// Returns all available events in the queue
    pub fn poll_events(&self) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        // Try to lock without blocking (returns immediately if locked)
        if let Ok(mut rx) = self.event_rx.try_lock() {
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
        }
        events
    }
}

impl Default for EngineBridge {
    fn default() -> Self {
        Self::new().0
    }
}
