//! Replicated cube demo - macOS and iPad
//!
//! This demonstrates real-time CRDT synchronization with Apple Pencil input.

use bevy::prelude::*;
use libmarathon::{
    engine::{EngineBridge, EngineCore},
    persistence::PersistenceConfig,
};
use std::path::PathBuf;

mod camera;
mod cube;
mod debug_ui;
mod engine_bridge;
mod rendering;
mod selection;
mod session;
mod session_ui;
mod setup;

use debug_ui::DebugUiPlugin;
use engine_bridge::EngineBridgePlugin;

mod input;

use camera::*;
use cube::*;
use rendering::*;
use selection::*;
use session::*;
use session_ui::*;

fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("wgpu=error".parse().unwrap())
                .add_directive("naga=warn".parse().unwrap()),
        )
        .init();

    // Application configuration
    const APP_NAME: &str = "Aspen";

    // Get platform-appropriate database path
    let db_path = libmarathon::platform::get_database_path(APP_NAME);
    let db_path_str = db_path.to_str().unwrap().to_string();
    info!("Database path: {}", db_path_str);

    // Create EngineBridge (for communication between Bevy and EngineCore)
    let (engine_bridge, engine_handle) = EngineBridge::new();
    info!("EngineBridge created");

    // Spawn EngineCore on tokio runtime (runs in background thread)
    std::thread::spawn(move || {
        info!("Starting EngineCore on tokio runtime...");
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let core = EngineCore::new(engine_handle, &db_path_str);
            core.run().await;
        });
    });
    info!("EngineCore spawned in background");

    // Create Bevy app (without winit - we own the event loop)
    let mut app = App::new();

    // Insert EngineBridge as a resource for Bevy systems to use
    app.insert_resource(engine_bridge);

    // Use DefaultPlugins but disable winit/window/input (we own those)
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<bevy::log::LogPlugin>()  // Using tracing-subscriber
            .disable::<bevy::winit::WinitPlugin>()  // We own winit
            .disable::<bevy::window::WindowPlugin>()  // We own the window
            .disable::<bevy::input::InputPlugin>()  // We provide InputEvents directly
            .disable::<bevy::gilrs::GilrsPlugin>()  // We handle gamepad input ourselves
    );

    // Marathon core plugins (networking, debug UI, persistence)
    app.add_plugins(libmarathon::MarathonPlugin::new(
        APP_NAME,
        PersistenceConfig {
            flush_interval_secs: 2,
            checkpoint_interval_secs: 30,
            battery_adaptive: true,
            ..Default::default()
        },
    ));

    // App-specific bridge for polling engine events
    app.add_plugins(EngineBridgePlugin);
    app.add_plugins(CameraPlugin);
    app.add_plugins(RenderingPlugin);
    app.add_plugins(input::InputHandlerPlugin);
    app.add_plugins(CubePlugin);
    app.add_plugins(SelectionPlugin);
    app.add_plugins(DebugUiPlugin);
    app.add_plugins(SessionUiPlugin);
    app.add_systems(Startup, initialize_offline_resources);

    libmarathon::platform::run_executor(app).expect("Failed to run executor");
}
