//! Replicated cube demo - macOS and iPad
//!
//! This demonstrates real-time CRDT synchronization with Apple Pencil input.

use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use lib::{
    networking::{NetworkingConfig, NetworkingPlugin},
    persistence::{PersistenceConfig, PersistencePlugin},
};
use std::path::PathBuf;
use uuid::Uuid;

mod camera;
mod cube;
mod debug_ui;
mod rendering;
mod setup;

#[cfg(not(target_os = "ios"))]
mod input {
    pub mod mouse;
    pub use mouse::MouseInputPlugin;
}

#[cfg(target_os = "ios")]
mod input {
    pub mod pencil;
    pub use pencil::PencilInputPlugin;
}

use camera::*;
use cube::*;
use debug_ui::*;
use input::*;
use rendering::*;
use setup::*;

fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("wgpu=error".parse().unwrap())
                .add_directive("naga=warn".parse().unwrap()),
        )
        .init();

    // Create node ID (in production, load from config or generate once)
    let node_id = Uuid::new_v4();
    info!("Starting app with node ID: {}", node_id);

    // Database path
    let db_path = PathBuf::from("cube_demo.db");

    // Create Bevy app
    App::new()
        .add_plugins(DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: format!("Replicated Cube Demo - Node {}", &node_id.to_string()[..8]),
                    resolution: (1280, 720).into(),
                    ..default()
                }),
                ..default()
            })
            .disable::<bevy::log::LogPlugin>()  // Disable Bevy's logger, using tracing-subscriber instead
        )
        .add_plugins(EguiPlugin::default())
        // Networking (bridge will be set up in startup)
        .add_plugins(NetworkingPlugin::new(NetworkingConfig {
            node_id,
            sync_interval_secs: 1.0,
            prune_interval_secs: 60.0,
            tombstone_gc_interval_secs: 300.0,
        }))
        // Persistence
        .add_plugins(PersistencePlugin::with_config(
            db_path,
            PersistenceConfig {
                flush_interval_secs: 2,
                checkpoint_interval_secs: 30,
                battery_adaptive: true,
                ..Default::default()
            },
        ))
        // Camera
        .add_plugins(CameraPlugin)
        // Rendering
        .add_plugins(RenderingPlugin)
        // Input
        .add_plugins(MouseInputPlugin)
        // Cube management
        .add_plugins(CubePlugin)
        // Debug UI
        .add_plugins(DebugUiPlugin)
        // Gossip networking setup
        .add_systems(Startup, setup_gossip_networking)
        .add_systems(Update, poll_gossip_bridge)
        .run();
}
