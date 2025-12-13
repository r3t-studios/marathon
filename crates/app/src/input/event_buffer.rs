//! Input event buffer shared between executor and ECS

use bevy::prelude::*;
use libmarathon::engine::InputEvent;

/// Input event buffer resource for Bevy ECS
#[derive(Resource, Default)]
pub struct InputEventBuffer {
    pub events: Vec<InputEvent>,
}

impl InputEventBuffer {
    /// Get all events from this frame
    pub fn events(&self) -> &[InputEvent] {
        &self.events
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.events.clear();
    }
}
