use bevy::prelude::*;
use parking_lot::Mutex;
use rusqlite::Connection;
use std::sync::Arc;

use crate::config::Config;

/// Bevy resource wrapping application configuration
#[derive(Resource)]
pub struct AppConfig(pub Config);

/// Bevy resource wrapping database connection
#[derive(Resource)]
pub struct Database(pub Arc<Mutex<Connection>>);
