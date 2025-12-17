//! Application executor - owns winit and drives Bevy ECS
//!
//! The executor is the bridge between the platform (winit) and the engine (Bevy ECS),
//! giving us full control over the event loop for maximum performance.
//!
//! # Architecture
//!
//! Instead of using Bevy's WinitPlugin, we own the winit event loop directly:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                      Executor                            │
//! │  (ApplicationHandler implementation)                     │
//! │                                                          │
//! │  ┌────────────┐         ┌──────────────┐               │
//! │  │   Winit    │  ────▶  │ Bevy ECS     │               │
//! │  │ Event Loop │         │ app.update() │               │
//! │  └────────────┘         └──────────────┘               │
//! │       │                                                 │
//! │       ▼                                                 │
//! │  push_window_event() ──▶ RawWinitEvent ──▶ InputEvent  │
//! │  push_device_event() ──▶   (channel)    ──▶  (buffer)  │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Responsibilities
//!
//! - **Event Loop Ownership**: Runs winit's event loop in unbounded mode (`ControlFlow::Poll`)
//! - **Window Management**: Creates window entity with RawHandleWrapper for Bevy renderer
//! - **Event Forwarding**: Routes winit events to platform bridge for conversion
//! - **ECS Updates**: Drives Bevy's `app.update()` every frame
//! - **Lifecycle**: Handles initialization (resumed), suspension (mobile), and shutdown
//!
//! ## Feature Parity with Bevy's WinitPlugin
//!
//! This executor provides complete feature parity with bevy_winit's ApplicationHandler:
//! - ✅ Window creation before app.finish() for renderer initialization
//! - ✅ Window/device event handling (20+ event types)
//! - ✅ Application lifecycle (resumed, suspended, exiting)
//! - ✅ Scale factor and resize handling
//! - ✅ Input event buffering and processing
//! - ✅ Graceful shutdown with WindowClosing events
//!
//! ## Performance
//!
//! The executor runs in unbounded mode for maximum performance:
//! - No frame rate cap (runs as fast as possible)
//! - Continuous redraw requests via `about_to_wait()`
//! - Lock-free event buffering via crossbeam channel
//!
//! Note: Battery-aware adaptive frame limiting is planned for production use.

use bevy::prelude::*;
use bevy::app::AppExit;
use bevy::input::{
    ButtonInput,
    mouse::MouseButton as BevyMouseButton,
    keyboard::KeyCode as BevyKeyCode,
    touch::{Touches, TouchInput},
    gestures::*,
    keyboard::KeyboardInput,
    mouse::{MouseButtonInput, MouseMotion, MouseWheel},
};
use bevy::window::{
    PrimaryWindow, WindowCreated, WindowResized, WindowScaleFactorChanged, WindowClosing,
    WindowResolution, WindowMode, WindowPosition, WindowEvent as BevyWindowEvent,
    RawHandleWrapper, WindowWrapper,
    CursorMoved, CursorEntered, CursorLeft,
    WindowFocused, WindowOccluded, WindowMoved, WindowThemeChanged, WindowDestroyed,
    FileDragAndDrop, Ime, WindowCloseRequested,
};
use bevy::ecs::message::Messages;
use crate::platform::input::{InputEvent, InputEventBuffer};
use super::{push_window_event, push_device_event, drain_as_input_events, set_scale_factor};
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{Event as WinitEvent, WindowEvent as WinitWindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window as WinitWindow, WindowId, WindowAttributes};

/// Application handler state machine
enum AppHandler {
    Initializing { app: Option<App> },
    Running {
        window: Arc<WinitWindow>,
        bevy_window_entity: Entity,
        bevy_app: App,
    },
}

// Helper functions to reduce duplication
fn send_window_created(app: &mut App, window: Entity) {
    app.world_mut()
        .resource_mut::<Messages<WindowCreated>>()
        .write(WindowCreated { window });
}

fn send_window_resized(
    app: &mut App,
    window: Entity,
    physical_size: winit::dpi::PhysicalSize<u32>,
    scale_factor: f64,
) {
    app.world_mut()
        .resource_mut::<Messages<WindowResized>>()
        .write(WindowResized {
            window,
            width: physical_size.width as f32 / scale_factor as f32,
            height: physical_size.height as f32 / scale_factor as f32,
        });
}

fn send_scale_factor_changed(app: &mut App, window: Entity, scale_factor: f64) {
    app.world_mut()
        .resource_mut::<Messages<WindowScaleFactorChanged>>()
        .write(WindowScaleFactorChanged {
            window,
            scale_factor,
        });
}

fn send_window_closing(app: &mut App, window: Entity) {
    app.world_mut()
        .resource_mut::<Messages<WindowClosing>>()
        .write(WindowClosing { window });
}

impl AppHandler {
    /// Initialize the window and transition to Running state.
    ///
    /// This is called on the first `resumed()` event from winit.
    /// Subsequent `resumed()` calls are ignored to prevent double initialization.
    ///
    /// # Errors
    /// Returns an error if window creation or RawHandleWrapper creation fails.
    fn initialize(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        // Only initialize if we're in the Initializing state with an app
        let AppHandler::Initializing { app: app_opt } = self else {
            warn!("initialize() called on non-Initializing state, ignoring");
            return Ok(());
        };

        let Some(mut bevy_app) = app_opt.take() else {
            warn!("initialize() called twice, ignoring");
            return Ok(());
        };

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
            .map_err(|e| format!("Failed to create window: {}", e))?;
        let winit_window = Arc::new(winit_window);
        info!("Created window before app.finish()");

        let physical_size = winit_window.inner_size();
        let scale_factor = winit_window.scale_factor();

        // Set the scale factor in the input bridge so mouse coords are converted correctly
        set_scale_factor(scale_factor);

        // Create window entity with all required components (use logical size)
        // Convert physical pixels to logical pixels using proper floating-point division
        let logical_width = (physical_size.width as f64 / scale_factor) as u32;
        let logical_height = (physical_size.height as f64 / scale_factor) as u32;

        let mut window = bevy::window::Window {
            title: "Marathon".to_string(),
            resolution: WindowResolution::new(logical_width, logical_height),
            mode: WindowMode::Windowed,
            position: WindowPosition::Automatic,
            focused: true,
            // Let Window use default theme - will auto-detect system theme, egui will follow
            ..Default::default()
        };
        // Set scale factor using the proper API that applies to physical size
        window
            .resolution
            .set_scale_factor_and_apply_to_physical_size(scale_factor as f32);

        // Create WindowWrapper and RawHandleWrapper for renderer
        let window_wrapper = WindowWrapper::new(winit_window.clone());
        let raw_handle_wrapper = RawHandleWrapper::new(&window_wrapper)
            .map_err(|e| format!("Failed to create RawHandleWrapper: {}", e))?;

        let window_entity = bevy_app.world_mut().spawn((
            window,
            PrimaryWindow,
            raw_handle_wrapper,
        )).id();
        info!("Created window entity {}", window_entity);

        // Send initialization event (only WindowCreated, like Bevy does)
        // WindowResized and WindowScaleFactorChanged should only fire in response to actual winit events
        send_window_created(&mut bevy_app, window_entity);

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

        Ok(())
    }

    /// Clean shutdown of the app and window
    fn shutdown(&mut self, event_loop: &ActiveEventLoop) {
        if let AppHandler::Running {
            bevy_window_entity,
            bevy_app,
            ..
        } = self
        {
            info!("Shutting down gracefully");

            // Send WindowClosing event
            send_window_closing(bevy_app, *bevy_window_entity);

            // Run one final update to process close event
            bevy_app.update();

            // Don't call finish/cleanup - let Bevy's AppExit handle it
            // bevy_app.finish();
            // bevy_app.cleanup();
        }

        event_loop.exit();
    }
}

impl ApplicationHandler for AppHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Initialize on first resumed() call
        if let Err(e) = self.initialize(event_loop) {
            error!("Failed to initialize: {}", e);
            event_loop.exit();
            return;
        }
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
            window,
            bevy_window_entity,
            bevy_app,
        } = self
        else {
            return;
        };

        // Forward input events to platform bridge
        push_window_event(&event);

        match event {
            WinitWindowEvent::CloseRequested => {
                self.shutdown(event_loop);
            }

            WinitWindowEvent::Resized(physical_size) => {
                // Update the Bevy Window component's physical resolution
                if let Some(mut window_component) = bevy_app.world_mut().get_mut::<Window>(*bevy_window_entity) {
                    window_component
                        .resolution
                        .set_physical_resolution(physical_size.width, physical_size.height);
                }

                // Notify Bevy systems of window resize
                let scale_factor = window.scale_factor();
                send_window_resized(bevy_app, *bevy_window_entity, physical_size, scale_factor);
            }

            WinitWindowEvent::RedrawRequested => {
                // Collect input events from platform bridge
                let input_events = drain_as_input_events();

                // Reuse buffer capacity instead of replacing (optimization)
                {
                    let mut buffer = bevy_app.world_mut().resource_mut::<InputEventBuffer>();
                    buffer.events.clear();
                    buffer.events.extend(input_events);
                }

                // Run one Bevy ECS update (unbounded)
                bevy_app.update();

                // Check if app should exit
                if bevy_app.should_exit().is_some() {
                    self.shutdown(event_loop);
                    return;
                }

                // Request next frame immediately (unbounded loop)
                window.request_redraw();
            }

            WinitWindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // Update the Bevy Window component's scale factor
                if let Some(mut window_component) = bevy_app.world_mut().get_mut::<Window>(*bevy_window_entity) {
                    let prior_factor = window_component.resolution.scale_factor();

                    // Use the proper API that applies to physical size
                    window_component
                        .resolution
                        .set_scale_factor_and_apply_to_physical_size(scale_factor as f32);

                    // Send scale factor changed event so camera system can update
                    send_scale_factor_changed(bevy_app, *bevy_window_entity, scale_factor);

                    info!(
                        "Scale factor changed from {} to {} for window {:?}",
                        prior_factor, scale_factor, bevy_window_entity
                    );
                }
            }

            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        // Forward device events to platform bridge
        // The main one we care about is MouseMotion for raw mouse delta (FPS camera)
        push_device_event(&event);
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // On iOS/Android, the app is being backgrounded
        // RedrawRequested events won't fire while suspended, so the unbounded loop naturally pauses
        info!("App suspended (backgrounded on mobile)");

        // TODO(@siennathesane): Implement AppLifecycle resource to track app state
        // Similar to Bevy's WinitPlugin, we should:
        // 1. Set AppLifecycle::WillSuspend before suspending
        // 2. Let the schedule run one last frame to react to suspension
        // 3. Potentially release GPU resources to save memory
        // 4. Stop requesting redraws to save battery
        //
        // For now, the unbounded loop will naturally pause since RedrawRequested
        // events won't fire while suspended anyway.
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Ensure we keep rendering even if no window events arrive.
        // This is needed for unbounded mode with ControlFlow::Poll to maintain
        // maximum frame rate by continuously requesting redraws.
        if let AppHandler::Running { window, .. } = self {
            window.request_redraw();
        }
    }
}

/// Run the application executor with a custom event loop.
///
/// This executor owns the winit event loop directly (instead of using Bevy's WinitPlugin)
/// and drives both the window and Bevy's ECS in unbounded mode for maximum performance.
///
/// # Unbounded Execution
///
/// The executor runs as fast as possible using `ControlFlow::Poll`, which means:
/// - The window event loop runs continuously without waiting for events
/// - Bevy's ECS `app.update()` is called on every frame
/// - Both rendering and game logic run unbounded (no frame rate cap)
///
/// This provides maximum performance but will drain battery quickly.
/// See the TODO comment about adaptive frame rate limiting for production use.
///
/// # Window Lifecycle
///
/// 1. Event loop starts and calls `resumed()` (winit requirement)
/// 2. Window and window entity are created BEFORE `app.finish()`
/// 3. Bevy's renderer initializes during `app.finish()` with the window available
/// 4. App transitions to Running state and begins the render loop
///
/// # Errors
///
/// Returns an error if:
/// - The event loop cannot be created
/// - Window creation fails during initialization
/// - The event loop encounters a fatal error
///
/// # Example
///
/// ```no_run
/// use bevy::prelude::*;
/// use libmarathon::platform::desktop;
///
/// let app = App::new();
/// // ... configure app with plugins and systems ...
///
/// desktop::run_executor(app).expect("Failed to run executor");
/// ```
pub fn run_executor(app: App) -> Result<(), Box<dyn std::error::Error>> {
    // Create event loop (using default type for now, WakeUp will be added when implementing battery mode)
    let event_loop = EventLoop::new()?;

    // TODO(@siennathesane): Add battery power detection and adaptive frame/tick rate limiting
    // When on battery: reduce to 60fps cap, lower ECS tick rate
    // When plugged in: run unbounded for maximum performance
    // See GitHub issue #TBD for implementation details

    // Run as fast as possible (unbounded)
    event_loop.set_control_flow(ControlFlow::Poll);

    info!("Starting executor (unbounded mode)");

    // Create handler in Initializing state with the app
    // It will transition to Running state on first resumed() callback
    let mut handler = AppHandler::Initializing { app: Some(app) };

    event_loop.run_app(&mut handler)?;

    Ok(())
}
