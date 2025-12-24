//! Replicated cube demo - macOS and iPad
//!
//! This demonstrates real-time CRDT synchronization with Apple Pencil input.

use bevy::prelude::*;
use clap::Parser;
use libmarathon::{
    engine::{
        EngineBridge,
        EngineCore,
    },
    persistence::PersistenceConfig,
};

#[cfg(feature = "headless")]
use bevy::app::ScheduleRunnerPlugin;
#[cfg(feature = "headless")]
use std::time::Duration;

/// Marathon - CRDT-based collaborative editing engine
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the database file
    #[arg(long, default_value = "marathon.db")]
    db_path: String,

    /// Path to the control socket (Unix domain socket)
    #[arg(long, default_value = "/tmp/marathon-control.sock")]
    control_socket: String,
}

mod camera;
mod control;
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
    // Parse command-line arguments
    let args = Args::parse();

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

    // Use database path from CLI args
    let db_path = std::path::PathBuf::from(&args.db_path);
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

    // Plugin setup based on headless vs rendering mode
    #[cfg(not(feature = "headless"))]
    {
        info!("Adding DefaultPlugins (rendering mode)");
        app.add_plugins(
            DefaultPlugins
                .build()
                .disable::<bevy::log::LogPlugin>() // Using tracing-subscriber
                .disable::<bevy::winit::WinitPlugin>() // We own winit
                .disable::<bevy::window::WindowPlugin>() // We own the window
                .disable::<bevy::input::InputPlugin>() // We provide InputEvents directly
                .disable::<bevy::gilrs::GilrsPlugin>(), // We handle gamepad input ourselves
        );
        info!("DefaultPlugins added");
    }

    #[cfg(feature = "headless")]
    {
        info!("Adding MinimalPlugins (headless mode)");
        app.add_plugins(
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(
                Duration::from_secs_f64(1.0 / 60.0), // 60 FPS
            )),
        );
        info!("MinimalPlugins added");
    }

    // Marathon core plugins based on mode
    #[cfg(not(feature = "headless"))]
    {
        info!("Adding MarathonPlugin (with debug UI)");
        app.add_plugins(libmarathon::MarathonPlugin::new(
            APP_NAME,
            PersistenceConfig {
                flush_interval_secs: 2,
                checkpoint_interval_secs: 30,
                battery_adaptive: true,
                ..Default::default()
            },
        ));
    }

    #[cfg(feature = "headless")]
    {
        info!("Adding networking and persistence (headless, no UI)");
        app.add_plugins(libmarathon::networking::NetworkingPlugin::new(Default::default()));
        app.add_plugins(libmarathon::persistence::PersistencePlugin::with_config(
            db_path.clone(),
            PersistenceConfig {
                flush_interval_secs: 2,
                checkpoint_interval_secs: 30,
                battery_adaptive: true,
                ..Default::default()
            },
        ));
    }

    info!("Marathon plugins added");

    // App-specific bridge for polling engine events
    info!("Adding app plugins");
    app.add_plugins(EngineBridgePlugin);
    app.add_plugins(CubePlugin);
    app.add_systems(Startup, initialize_offline_resources);

    // Insert control socket path as resource
    app.insert_resource(control::ControlSocketPath(args.control_socket.clone()));
    app.add_systems(Startup, control::start_control_socket_system);

    // Rendering-only plugins
    #[cfg(not(feature = "headless"))]
    {
        app.add_plugins(CameraPlugin);
        app.add_plugins(RenderingPlugin);
        app.add_plugins(input::InputHandlerPlugin);
        app.add_plugins(SelectionPlugin);
        app.add_plugins(DebugUiPlugin);
        app.add_plugins(SessionUiPlugin);
    }

    info!("All plugins added");

    // Run the app based on mode
    #[cfg(not(feature = "headless"))]
    {
        info!("Running platform executor (rendering mode)");
        libmarathon::platform::run_executor(app).expect("Failed to run executor");
    }

    #[cfg(feature = "headless")]
    {
        info!("Running headless app loop");
        app.run();
    }
}
