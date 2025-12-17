//! Desktop event loop - owns winit window and event handling
//!
//! This module creates and manages the main window and event loop.
//! It converts winit events to InputEvents and provides them to the engine.

use super::input;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

/// Main event loop runner for desktop platforms
pub struct DesktopApp {
    window: Option<Window>,
}

impl DesktopApp {
    pub fn new() -> Self {
        Self { window: None }
    }
}

impl ApplicationHandler for DesktopApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window_attributes = Window::default_attributes()
                .with_title("Marathon")
                .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));

            match event_loop.create_window(window_attributes) {
                Ok(window) => {
                    tracing::info!("Created winit window");
                    self.window = Some(window);
                }
                Err(e) => {
                    tracing::error!("Failed to create window: {}", e);
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // Forward all input events to the bridge first
        input::push_window_event(&event);

        match event {
            WindowEvent::CloseRequested => {
                tracing::info!("Window close requested");
                event_loop.exit();
            }

            WindowEvent::RedrawRequested => {
                // Rendering happens via Bevy
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Request redraw for next frame
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

/// Run the desktop application with the provided game update function
///
/// This takes ownership of the main thread and runs the winit event loop.
/// The update_fn is called each frame to update game logic.
pub fn run(_update_fn: impl FnMut() + 'static) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll); // Run as fast as possible

    let mut app = DesktopApp::new();

    // Run the event loop, calling update_fn each frame
    event_loop.run_app(&mut app)?;

    Ok(())
}
