//! iOS application executor - owns winit and drives Bevy ECS
//!
//! iOS-specific implementation of the executor pattern, adapted for UIKit
//! integration. See platform/desktop/executor.rs for detailed architecture
//! documentation.

use std::sync::Arc;

use bevy::{
    app::AppExit,
    ecs::message::Messages,
    input::{
        ButtonInput,
        gestures::*,
        keyboard::{
            KeyCode as BevyKeyCode,
            KeyboardInput,
        },
        mouse::{
            MouseButton as BevyMouseButton,
            MouseButtonInput,
            MouseMotion,
            MouseWheel,
        },
        touch::{
            TouchInput,
            Touches,
        },
    },
    prelude::*,
    window::{
        CursorEntered,
        CursorLeft,
        CursorMoved,
        FileDragAndDrop,
        Ime,
        PrimaryWindow,
        RawHandleWrapper,
        WindowCloseRequested,
        WindowClosing,
        WindowCreated,
        WindowDestroyed,
        WindowEvent as BevyWindowEvent,
        WindowFocused,
        WindowMode,
        WindowMoved,
        WindowOccluded,
        WindowPosition,
        WindowResized,
        WindowResolution,
        WindowScaleFactorChanged,
        WindowThemeChanged,
        WindowWrapper,
    },
};
use glam;
use winit::{
    application::ApplicationHandler,
    event::{
        Event as WinitEvent,
        WindowEvent as WinitWindowEvent,
    },
    event_loop::{
        ActiveEventLoop,
        ControlFlow,
        EventLoop,
        EventLoopProxy,
    },
    window::{
        Window as WinitWindow,
        WindowAttributes,
        WindowId,
    },
};

use crate::platform::input::{
    InputEvent,
    InputEventBuffer,
};

/// Application handler state machine
enum AppHandler {
    Initializing {
        app: Option<App>,
    },
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
    fn initialize(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
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
        // Let winit choose the default size for iOS
        let window_attributes = WindowAttributes::default()
            .with_title("Marathon");

        let winit_window = event_loop
            .create_window(window_attributes)
            .map_err(|e| format!("Failed to create window: {}", e))?;
        let winit_window = Arc::new(winit_window);
        info!("Created iOS window before app.finish()");

        let physical_size = winit_window.inner_size();
        let scale_factor = winit_window.scale_factor();

        // Log everything for debugging
        info!("iOS window diagnostics:");
        info!("  Physical size (pixels): {}×{}", physical_size.width, physical_size.height);
        info!("  Scale factor: {}", scale_factor);

        // WindowResolution::new() expects PHYSICAL size
        let mut window = bevy::window::Window {
            title: "Marathon".to_string(),
            resolution: WindowResolution::new(physical_size.width, physical_size.height),
            mode: WindowMode::BorderlessFullscreen(bevy::window::MonitorSelection::Current),
            position: WindowPosition::Automatic,
            focused: true,
            ..Default::default()
        };

        // Set scale factor so Bevy can calculate logical size
        window.resolution.set_scale_factor(scale_factor as f32);

        // Log final window state
        info!("  Final window resolution: {:.1}×{:.1} (logical)",
              window.resolution.width(), window.resolution.height());
        info!("  Final physical resolution: {}×{}",
              window.resolution.physical_width(), window.resolution.physical_height());
        info!("  Final scale factor: {}", window.resolution.scale_factor());
        info!("  Window mode: BorderlessFullscreen");

        // Create WindowWrapper and RawHandleWrapper for renderer
        let window_wrapper = WindowWrapper::new(winit_window.clone());
        let raw_handle_wrapper = RawHandleWrapper::new(&window_wrapper)
            .map_err(|e| format!("Failed to create RawHandleWrapper: {}", e))?;

        let window_entity = bevy_app
            .world_mut()
            .spawn((window, PrimaryWindow, raw_handle_wrapper))
            .id();
        info!("Created window entity {}", window_entity);

        // Send initialization event
        send_window_created(&mut bevy_app, window_entity);

        // Now finish the app - the renderer will initialize with the window
        bevy_app.finish();
        bevy_app.cleanup();
        info!("iOS app finished and cleaned up");

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
            info!("Shutting down iOS app gracefully");

            // Send WindowClosing event
            send_window_closing(bevy_app, *bevy_window_entity);

            // Run one final update to process close event
            bevy_app.update();
        }

        event_loop.exit();
    }
}

impl ApplicationHandler for AppHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        eprintln!(">>> iOS executor: resumed() callback called");
        // Initialize on first resumed() call
        if let Err(e) = self.initialize(event_loop) {
            error!("Failed to initialize iOS app: {}", e);
            eprintln!(">>> iOS executor: Initialization failed: {}", e);
            event_loop.exit();
            return;
        }
        info!("iOS app resumed");
        eprintln!(">>> iOS executor: App resumed successfully");
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

        match event {
            | WinitWindowEvent::CloseRequested => {
                self.shutdown(event_loop);
            },

            | WinitWindowEvent::Resized(physical_size) => {
                // Update the Bevy Window component's physical resolution
                if let Some(mut window_component) =
                    bevy_app.world_mut().get_mut::<Window>(*bevy_window_entity)
                {
                    window_component
                        .resolution
                        .set_physical_resolution(physical_size.width, physical_size.height);
                }

                // Notify Bevy systems of window resize
                let scale_factor = window.scale_factor();
                send_window_resized(bevy_app, *bevy_window_entity, physical_size, scale_factor);
            },

            | WinitWindowEvent::RedrawRequested => {
                // Log viewport/window dimensions every 60 frames
                static mut FRAME_COUNT: u32 = 0;
                let should_log = unsafe {
                    FRAME_COUNT += 1;
                    FRAME_COUNT % 60 == 0
                };

                if should_log {
                    if let Some(window_component) = bevy_app.world().get::<Window>(*bevy_window_entity) {
                        let frame_num = unsafe { FRAME_COUNT };
                        info!("Frame {} - Window state:", frame_num);
                        info!("  Logical: {:.1}×{:.1}",
                              window_component.resolution.width(),
                              window_component.resolution.height());
                        info!("  Physical: {}×{}",
                              window_component.resolution.physical_width(),
                              window_component.resolution.physical_height());
                        info!("  Scale: {}", window_component.resolution.scale_factor());
                    }
                }

                // iOS-specific: Get pencil input from the bridge
                #[cfg(target_os = "ios")]
                let pencil_events = super::drain_as_input_events();

                #[cfg(not(target_os = "ios"))]
                let pencil_events = vec![];

                // Reuse buffer capacity instead of replacing (optimization)
                {
                    let mut buffer = bevy_app.world_mut().resource_mut::<InputEventBuffer>();
                    buffer.events.clear();
                    buffer.events.extend(pencil_events);
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
            },

            | WinitWindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // Update the Bevy Window component's scale factor
                if let Some(mut window_component) =
                    bevy_app.world_mut().get_mut::<Window>(*bevy_window_entity)
                {
                    let prior_factor = window_component.resolution.scale_factor();

                    window_component
                        .resolution
                        .set_scale_factor_and_apply_to_physical_size(scale_factor as f32);

                    send_scale_factor_changed(bevy_app, *bevy_window_entity, scale_factor);

                    info!(
                        "iOS scale factor changed from {} to {} for window {:?}",
                        prior_factor, scale_factor, bevy_window_entity
                    );
                }
            },

            // Mouse support for iPad simulator (simulator uses mouse, not touch)
            | WinitWindowEvent::CursorMoved { position, .. } => {
                let scale_factor = window.scale_factor();
                let mut buffer = bevy_app.world_mut().resource_mut::<InputEventBuffer>();
                buffer
                    .events
                    .push(crate::platform::input::InputEvent::MouseMove {
                        pos: glam::Vec2::new(
                            (position.x / scale_factor) as f32,
                            (position.y / scale_factor) as f32,
                        ),
                    });
            },

            | WinitWindowEvent::MouseInput { state, button, .. } => {
                use crate::platform::input::{
                    MouseButton as EngineButton,
                    TouchPhase,
                };

                let (engine_button, phase) = match (button, state) {
                    | (winit::event::MouseButton::Left, winit::event::ElementState::Pressed) => {
                        (EngineButton::Left, TouchPhase::Started)
                    },
                    | (winit::event::MouseButton::Left, winit::event::ElementState::Released) => {
                        (EngineButton::Left, TouchPhase::Ended)
                    },
                    | (winit::event::MouseButton::Right, winit::event::ElementState::Pressed) => {
                        (EngineButton::Right, TouchPhase::Started)
                    },
                    | (winit::event::MouseButton::Right, winit::event::ElementState::Released) => {
                        (EngineButton::Right, TouchPhase::Ended)
                    },
                    | (winit::event::MouseButton::Middle, winit::event::ElementState::Pressed) => {
                        (EngineButton::Middle, TouchPhase::Started)
                    },
                    | (winit::event::MouseButton::Middle, winit::event::ElementState::Released) => {
                        (EngineButton::Middle, TouchPhase::Ended)
                    },
                    | _ => return, // Ignore other buttons
                };

                let mut buffer = bevy_app.world_mut().resource_mut::<InputEventBuffer>();
                // Use last known cursor position - extract position first to avoid borrow issues
                let last_pos = buffer
                    .events
                    .iter()
                    .rev()
                    .find_map(|e| match e {
                        crate::platform::input::InputEvent::MouseMove { pos } => Some(*pos),
                        _ => None,
                    });

                if let Some(pos) = last_pos {
                    buffer.events.push(crate::platform::input::InputEvent::Mouse {
                        pos,
                        button: engine_button,
                        phase,
                    });
                }
            },

            | WinitWindowEvent::MouseWheel { delta, .. } => {
                let (delta_x, delta_y) = match delta {
                    | winit::event::MouseScrollDelta::LineDelta(x, y) => {
                        (x * 20.0, y * 20.0) // Convert lines to pixels
                    },
                    | winit::event::MouseScrollDelta::PixelDelta(pos) => {
                        (pos.x as f32, pos.y as f32)
                    },
                };

                let mut buffer = bevy_app.world_mut().resource_mut::<InputEventBuffer>();
                // Use last known cursor position
                let pos = buffer
                    .events
                    .iter()
                    .rev()
                    .find_map(|e| match e {
                        | crate::platform::input::InputEvent::MouseMove { pos } => Some(*pos),
                        | crate::platform::input::InputEvent::MouseWheel { pos, .. } => Some(*pos),
                        | _ => None,
                    })
                    .unwrap_or(glam::Vec2::ZERO);

                buffer
                    .events
                    .push(crate::platform::input::InputEvent::MouseWheel {
                        delta: glam::Vec2::new(delta_x, delta_y),
                        pos,
                    });
            },

            | _ => {},
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // On iOS, the app is being backgrounded
        info!("iOS app suspended (backgrounded)");

        // TODO: Implement AppLifecycle resource to track app state
        // iOS-specific considerations:
        // 1. Release GPU resources to avoid being killed by iOS
        // 2. Stop requesting redraws to save battery
        // 3. Save any persistent state
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Ensure we keep rendering even if no window events arrive
        if let AppHandler::Running { window, .. } = self {
            window.request_redraw();
        }
    }
}

/// Run the iOS application executor.
///
/// This is the iOS equivalent of desktop::run_executor().
/// See desktop/executor.rs for detailed documentation.
///
/// # iOS-Specific Notes
///
/// - Uses winit's iOS backend (backed by UIKit)
/// - Supports Apple Pencil input via the pencil_bridge
/// - Handles iOS lifecycle (suspended/resumed) for backgrounding
/// - Uses Retina display scale factors (2.0-3.0)
///
/// # Errors
///
/// Returns an error if:
/// - The event loop cannot be created
/// - Window creation fails during initialization
/// - The event loop encounters a fatal error
pub fn run_executor(app: App) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!(">>> iOS executor: run_executor() called");

    eprintln!(">>> iOS executor: Creating event loop");
    let event_loop = EventLoop::new()?;
    eprintln!(">>> iOS executor: Event loop created");

    // Run as fast as possible (unbounded)
    eprintln!(">>> iOS executor: Setting control flow");
    event_loop.set_control_flow(ControlFlow::Poll);

    info!("Starting iOS executor (unbounded mode)");
    eprintln!(">>> iOS executor: Starting (unbounded mode)");

    // Create handler in Initializing state with the app
    eprintln!(">>> iOS executor: Creating AppHandler");
    let mut handler = AppHandler::Initializing { app: Some(app) };

    eprintln!(">>> iOS executor: Running event loop (blocking call)");
    event_loop.run_app(&mut handler)?;

    eprintln!(">>> iOS executor: Event loop returned (should never reach here)");
    Ok(())
}
