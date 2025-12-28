//! Persistence layer for battery-efficient state management
//!
//! This module implements the persistence strategy defined in RFC 0002.
//! It provides a three-tier system to minimize disk I/O while maintaining data
//! durability:
//!
//! 1. **In-Memory Dirty Tracking** - Track changes without writing immediately
//! 2. **Write Buffer** - Batch and coalesce operations before writing
//! 3. **SQLite with WAL Mode** - Controlled checkpoints to minimize fsync()
//!    calls
//!
//! # Example
//!
//! ```no_run
//! use bevy::prelude::*;
//! use libmarathon::persistence::*;
//!
//! fn setup(mut commands: Commands) {
//!     // Spawn an entity with the Persisted marker
//!     commands.spawn(Persisted::new());
//! }
//!
//! // The persistence plugin automatically tracks changes to Persisted components
//! fn main() {
//!     App::new()
//!         .add_plugins(DefaultPlugins)
//!         .add_plugins(PersistencePlugin::new("app.db"))
//!         .add_systems(Startup, setup)
//!         .run();
//! }
//! ```

mod config;
mod database;
mod error;
mod health;
mod lifecycle;
mod metrics;
mod migrations;
mod plugin;
pub mod reflection;
mod registered_components;
mod systems;
mod type_registry;
mod types;

pub use config::*;
pub use database::*;
pub use error::*;
pub use health::*;
pub use lifecycle::*;
pub use metrics::*;
pub use migrations::*;
pub use plugin::*;
pub use reflection::*;
pub use systems::*;
pub use type_registry::*;
pub use types::*;
