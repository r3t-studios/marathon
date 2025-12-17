//! Replicated cube demo - macOS and iPad
//!
//! This demonstrates real-time CRDT synchronization with Apple Pencil input.

use bevy::prelude::*;
use libmarathon::{
    engine::{
        EngineBridge,
        EngineCore,
    },
    persistence::PersistenceConfig,
};

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
    // Note: eprintln doesn't work on iOS, but tracing-oslog will once initialized
    eprintln!(">>> RUST ENTRY: main() started");

    // Initialize logging
    eprintln!(">>> Initializing tracing_subscriber");

    #[cfg(target_os = "ios")]
    {
        use tracing_subscriber::prelude::*;

        let filter = tracing_subscriber::EnvFilter::builder()
            .with_default_directive(tracing::Level::DEBUG.into())
            .from_env_lossy()
            .add_directive("wgpu=error".parse().unwrap())
            .add_directive("naga=warn".parse().unwrap())
            .add_directive("winit=error".parse().unwrap());

        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_oslog::OsLogger::new("io.r3t.aspen", "default"))
            .init();

        info!("OSLog initialized successfully");
    }

    #[cfg(not(target_os = "ios"))]
    {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive("wgpu=error".parse().unwrap())
                    .add_directive("naga=warn".parse().unwrap()),
            )
            .init();
    }

    eprintln!(">>> Tracing subscriber initialized");

    // Application configuration
    const APP_NAME: &str = "Aspen";

    // Get platform-appropriate database path
    eprintln!(">>> Getting database path");
    let db_path = libmarathon::platform::get_database_path(APP_NAME);
    let db_path_str = db_path.to_str().unwrap().to_string();
    info!("Database path: {}", db_path_str);
    eprintln!(">>> Database path: {}", db_path_str);

    // Create EngineBridge (for communication between Bevy and EngineCore)
    eprintln!(">>> Creating EngineBridge");
    let (engine_bridge, engine_handle) = EngineBridge::new();
    info!("EngineBridge created");
    eprintln!(">>> EngineBridge created");

    // Spawn EngineCore on tokio runtime (runs in background thread)
    eprintln!(">>> Spawning EngineCore background thread");
    std::thread::spawn(move || {
        eprintln!(">>> [EngineCore thread] Thread started");
        info!("Starting EngineCore on tokio runtime...");
        eprintln!(">>> [EngineCore thread] Creating tokio runtime");
        let rt = tokio::runtime::Runtime::new().unwrap();
        eprintln!(">>> [EngineCore thread] Tokio runtime created");
        rt.block_on(async {
            eprintln!(">>> [EngineCore thread] Creating EngineCore");
            let core = EngineCore::new(engine_handle, &db_path_str);
            eprintln!(">>> [EngineCore thread] Running EngineCore");
            core.run().await;
        });
    });
    info!("EngineCore spawned in background");
    eprintln!(">>> EngineCore thread spawned");

    // Create Bevy app (without winit - we own the event loop)
    eprintln!(">>> Creating Bevy App");
    let mut app = App::new();
    eprintln!(">>> Bevy App created");

    // Insert EngineBridge as a resource for Bevy systems to use
    eprintln!(">>> Inserting EngineBridge resource");
    app.insert_resource(engine_bridge);

    // Use DefaultPlugins but disable winit/window/input (we own those)
    eprintln!(">>> Adding DefaultPlugins");
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<bevy::log::LogPlugin>() // Using tracing-subscriber
            .disable::<bevy::winit::WinitPlugin>() // We own winit
            .disable::<bevy::window::WindowPlugin>() // We own the window
            .disable::<bevy::input::InputPlugin>() // We provide InputEvents directly
            .disable::<bevy::gilrs::GilrsPlugin>(), // We handle gamepad input ourselves
    );
    eprintln!(">>> DefaultPlugins added");

    // Marathon core plugins (networking, debug UI, persistence)
    eprintln!(">>> Adding MarathonPlugin");
    app.add_plugins(libmarathon::MarathonPlugin::new(
        APP_NAME,
        PersistenceConfig {
            flush_interval_secs: 2,
            checkpoint_interval_secs: 30,
            battery_adaptive: true,
            ..Default::default()
        },
    ));
    eprintln!(">>> MarathonPlugin added");

    // App-specific bridge for polling engine events
    eprintln!(">>> Adding app plugins");
    app.add_plugins(EngineBridgePlugin);
    app.add_plugins(CameraPlugin);
    app.add_plugins(RenderingPlugin);
    app.add_plugins(input::InputHandlerPlugin);
    app.add_plugins(CubePlugin);
    app.add_plugins(SelectionPlugin);
    app.add_plugins(DebugUiPlugin);
    app.add_plugins(SessionUiPlugin);
    app.add_systems(Startup, initialize_offline_resources);
    eprintln!(">>> All plugins added");

    eprintln!(">>> Running executor");
    libmarathon::platform::run_executor(app).expect("Failed to run executor");
    eprintln!(">>> Executor returned (should never reach here)");
}
