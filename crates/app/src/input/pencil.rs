//! Apple Pencil input system for iOS
//!
//! This module integrates the platform-agnostic pencil bridge with Bevy.

use bevy::prelude::*;
use libmarathon::{platform::input::InputEvent, platform::ios};

pub struct PencilInputPlugin;

impl Plugin for PencilInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, attach_pencil_capture)
            .add_systems(PreUpdate, poll_pencil_input);
    }
}

/// Resource to track the latest pencil state
#[derive(Resource, Default)]
pub struct PencilState {
    pub latest: Option<InputEvent>,
    pub points_this_frame: usize,
}

/// Attach the Swift pencil capture to Bevy's window
#[cfg(target_os = "ios")]
fn attach_pencil_capture(windows: Query<&bevy::window::RawHandleWrapper, With<bevy::window::PrimaryWindow>>) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = windows.get_single() else {
        warn!("No primary window for pencil capture");
        return;
    };

    unsafe {
        if let Ok(raw) = handle.window_handle() {
            if let RawWindowHandle::UiKit(h) = raw.as_ref() {
                ios::swift_attach_pencil_capture(h.ui_view.as_ptr() as *mut _);
                info!("✏️ Apple Pencil capture attached");
            }
        }
    }
}

#[cfg(not(target_os = "ios"))]
fn attach_pencil_capture() {
    // No-op on non-iOS platforms
}

/// Poll pencil input from the platform layer and update PencilState
fn poll_pencil_input(mut commands: Commands, state: Option<ResMut<PencilState>>) {
    let events = ios::drain_as_input_events();

    if events.is_empty() {
        return;
    }

    // Insert resource if it doesn't exist
    if state.is_none() {
        commands.insert_resource(PencilState::default());
        return;
    }

    if let Some(mut state) = state {
        state.points_this_frame = events.len();
        if let Some(latest) = events.last() {
            state.latest = Some(*latest);
        }
    }
}
