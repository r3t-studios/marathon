//! Marathon - A collaborative real-time editing engine
//!
//! This library provides the core functionality for building collaborative
//! applications with CRDT-based synchronization, persistence, and networking.
//!
//! # Features
//!
//! - **Networking**: Real-time collaborative editing with gossip-based sync
//! - **Persistence**: SQLite-backed storage with automatic migration
//! - **Debug UI**: Built-in egui integration for development
//! - **Engine**: Event-driven architecture with async task coordination
//!
//! # Example
//!
//! ```no_run
//! use bevy::prelude::*;
//! use libmarathon::{MarathonPlugin, persistence::PersistenceConfig};
//!
//! fn main() {
//!     App::new()
//!         .add_plugins(MarathonPlugin::new("my_app.db", PersistenceConfig::default()))
//!         .run();
//! }
//! ```

pub mod debug_ui;
pub mod engine;
pub mod networking;
pub mod persistence;
pub mod platform;
pub mod sync;

/// Unified Marathon plugin that bundles all core functionality.
///
/// This plugin combines:
/// - Networking for collaborative editing (with CRDT-based synchronization)
/// - Debug UI using egui
/// - Persistence for local storage
///
/// For simple integration, just add this single plugin to your Bevy app.
/// Note: You'll still need to add your app-specific bridge/event handling.
pub struct MarathonPlugin {
    /// Path to the persistence database
    pub db_path: std::path::PathBuf,
    /// Persistence configuration
    pub persistence_config: persistence::PersistenceConfig,
}

impl MarathonPlugin {
    /// Create a new MarathonPlugin with custom database path and config
    pub fn new(db_path: impl Into<std::path::PathBuf>, config: persistence::PersistenceConfig) -> Self {
        Self {
            db_path: db_path.into(),
            persistence_config: config,
        }
    }

    /// Create with default settings (database in current directory)
    pub fn with_default_db() -> Self {
        Self {
            db_path: "marathon.db".into(),
            persistence_config: Default::default(),
        }
    }
}

impl bevy::app::Plugin for MarathonPlugin {
    fn build(&self, app: &mut bevy::app::App) {
        // Networking for collaboration (uses default config with random node_id)
        app.add_plugins(networking::NetworkingPlugin::new(Default::default()));

        // Debug UI
        app.add_plugins(debug_ui::EguiPlugin::default());

        // Persistence
        app.add_plugins(persistence::PersistencePlugin::with_config(
            self.db_path.clone(),
            self.persistence_config.clone(),
        ));
    }
}
