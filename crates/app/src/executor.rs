//! Application executor - owns winit and drives Bevy ECS
//!
//! The executor gives us full control over the event loop and allows
//! both the window and ECS to run unbounded (maximum performance).

use bevy::prelude::*;
use bevy::app::AppExit;
use bevy::input::{
    ButtonInput,
    mouse::MouseButton as BevyMouseButton,
    keyboard::KeyCode as BevyKeyCode,
    touch::{Touches, TouchInput},
};
use bevy::window::{
    PrimaryWindow, WindowCreated, WindowResized, WindowScaleFactorChanged, WindowClosing,
    WindowResolution, WindowMode, WindowPosition, WindowEvent as BevyWindowEvent,
    RawHandleWrapper, WindowWrapper,
};
use bevy::ecs::message::Messages;
use libmarathon::engine::InputEvent;
use libmarathon::platform::desktop;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent as WinitWindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window as WinitWindow, WindowId, WindowAttributes};

// Re-export InputEventBuffer from the input module
pub use crate::input::event_buffer::InputEventBuffer;

/// Application handler state machine
enum AppHandler {
    Initializing { app: App },
    Running {
        window: Arc<WinitWindow>,
        bevy_window_entity: Entity,
        bevy_app: App,
    },
}

impl AppHandler {
    fn initialize(&mut self, event_loop: &ActiveEventLoop) {
        // Only initialize if we're in the Initializing state
        if !matches!(self, AppHandler::Initializing { .. }) {
            return;
        }

        // Take ownership of the app (replace with placeholder temporarily)
        let temp_state = std::mem::replace(self, AppHandler::Initializing { app: App::new() });
        let AppHandler::Initializing { app } = temp_state else { unreachable!() };
        let mut bevy_app = app;

        // Insert InputEventBuffer resource
        bevy_app.insert_resource(InputEventBuffer::default());

        // Initialize window message channels
        bevy_app.init_resource::<Messages<WindowCreated>>();
        bevy_app.init_resource::<Messages<WindowResized>>();
        bevy_app.init_resource::<Messages<WindowScaleFactorChanged>>();
        bevy_app.init_resource::<Messages<WindowClosing>>();
        bevy_app.init_resource::<Messages<BevyWindowEvent>>();

        // Initialize input resources that Bevy UI and picking expect
        bevy_app.init_resource::<ButtonInput<BevyMouseButton>>();
        bevy_app.init_resource::<ButtonInput<BevyKeyCode>>();
        bevy_app.init_resource::<Touches>();
        bevy_app.init_resource::<Messages<TouchInput>>();

        // Create the winit window BEFORE finishing the app
        let window_attributes = WindowAttributes::default()
            .with_title("Marathon")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));

        let winit_window = event_loop.create_window(window_attributes)
            .expect("Failed to create window");
        let winit_window = Arc::new(winit_window);
        info!("Created window before app.finish()");

        let physical_size = winit_window.inner_size();
        let scale_factor = winit_window.scale_factor();

        // Create window entity with all required components
        let mut window = bevy::window::Window {
            title: "Marathon".to_string(),
            resolution: WindowResolution::new(
                physical_size.width,
                physical_size.height,
            ),
            mode: WindowMode::Windowed,
            position: WindowPosition::Automatic,
            focused: true,
            ..Default::default()
        };
        window.resolution.set_scale_factor_override(Some(scale_factor as f32));

        // Create WindowWrapper and RawHandleWrapper for renderer
        let window_wrapper = WindowWrapper::new(winit_window.clone());
        let raw_handle_wrapper = RawHandleWrapper::new(&window_wrapper)
            .expect("Failed to create RawHandleWrapper");

        let window_entity = bevy_app.world_mut().spawn((
            window,
            PrimaryWindow,
            raw_handle_wrapper,
        )).id();
        info!("Created window entity {}", window_entity);

        // Send WindowCreated event
        bevy_app.world_mut()
            .resource_mut::<Messages<WindowCreated>>()
            .write(WindowCreated { window: window_entity });

        // Send WindowResized event
        bevy_app.world_mut()
            .resource_mut::<Messages<WindowResized>>()
            .write(WindowResized {
                window: window_entity,
                width: physical_size.width as f32 / scale_factor as f32,
                height: physical_size.height as f32 / scale_factor as f32,
            });

        // Now finish the app - the renderer will initialize with the window
        bevy_app.finish();
        bevy_app.cleanup();
        info!("App finished and cleaned up");

        // Transition to Running state
        *self = AppHandler::Running {
            window: winit_window,
            bevy_window_entity: window_entity,
            bevy_app,
        };
    }
}

impl ApplicationHandler for AppHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Initialize on first resumed() call
        self.initialize(event_loop);
        info!("App resumed");
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WinitWindowEvent,
    ) {
        // Only handle events if we're in Running state
        let AppHandler::Running {
            ref window,
            bevy_window_entity,
            ref mut bevy_app,
        } = self
        else {
            return;
        };

        // Forward input events to platform bridge
        desktop::push_window_event(&event);

        match event {
            WinitWindowEvent::CloseRequested => {
                info!("Window close requested");
                event_loop.exit();
            }

            WinitWindowEvent::Resized(physical_size) => {
                // Notify Bevy of window resize
                let scale_factor = window.scale_factor();
                bevy_app.world_mut()
                    .resource_mut::<Messages<WindowResized>>()
                    .write(WindowResized {
                        window: *bevy_window_entity,
                        width: physical_size.width as f32 / scale_factor as f32,
                        height: physical_size.height as f32 / scale_factor as f32,
                    });
            }

            WinitWindowEvent::RedrawRequested => {
                // Collect input events from platform bridge
                let input_events = desktop::drain_as_input_events();

                // Write events to InputEventBuffer resource
                bevy_app.world_mut().resource_mut::<InputEventBuffer>().events = input_events;

                // Run one Bevy ECS update (unbounded)
                bevy_app.update();

                // Check if app should exit
                if let Some(exit) = bevy_app.should_exit() {
                    info!("App exit requested: {:?}", exit);
                    event_loop.exit();
                }

                // Clear input buffer for next frame
                bevy_app.world_mut().resource_mut::<InputEventBuffer>().clear();

                // Request next frame immediately (unbounded loop)
                window.request_redraw();
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Request redraw to keep loop running
        if let AppHandler::Running { ref window, .. } = self {
            window.request_redraw();
        }
    }
}

/// Run the application executor
pub fn run(app: App) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;

    // TODO: Add battery power detection and adaptive frame/tick rate limiting
    // When on battery: reduce to 60fps cap, lower ECS tick rate
    // When plugged in: run unbounded for maximum performance

    // Run as fast as possible (unbounded)
    event_loop.set_control_flow(ControlFlow::Poll);

    info!("Starting executor (unbounded mode)");

    // Create handler in Initializing state
    // It will transition to Running state on first resumed() callback
    let mut handler = AppHandler::Initializing { app };

    event_loop.run_app(&mut handler)?;

    Ok(())
}
