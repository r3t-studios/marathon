//! Persistence layer for battery-efficient state management
//!
//! This module implements the persistence strategy defined in RFC 0002.
//! It provides a three-tier system to minimize disk I/O while maintaining data durability:
//!
//! 1. **In-Memory Dirty Tracking** - Track changes without writing immediately
//! 2. **Write Buffer** - Batch and coalesce operations before writing
//! 3. **SQLite with WAL Mode** - Controlled checkpoints to minimize fsync() calls
//!
//! # Example
//!
//! ```no_run
//! use lib::persistence::*;
//! use bevy::prelude::*;
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

mod types;
mod database;
mod systems;
mod config;
mod metrics;
mod plugin;
mod reflection;
mod health;
mod error;
mod lifecycle;

pub use types::*;
pub use database::*;
pub use systems::*;
pub use config::*;
pub use metrics::*;
pub use plugin::*;
pub use reflection::*;
pub use health::*;
pub use error::*;
pub use lifecycle::*;
