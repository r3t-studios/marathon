mod assets;
mod components;
mod config;
mod db;
mod entities;
mod iroh_sync;
mod models;
mod services;
mod systems;

use std::{
    path::Path,
    sync::Arc,
};

use anyhow::{
    Context,
    Result,
};
use bevy::prelude::*;
// Import components and systems
use components::*;
use config::Config;
use iroh_gossip::proto::TopicId;
// Re-export init function
pub use iroh_sync::init_iroh_gossip;
use parking_lot::Mutex;
use rusqlite::Connection;
use systems::*;

fn main() {
    println!("Starting server");

    // Load configuration and initialize database
    let (config, us_db) = match initialize_app() {
        | Ok(data) => data,
        | Err(e) => {
            eprintln!("Failed to initialize app: {}", e);
            return;
        },
    };

    // Create a topic ID for gossip (use a fixed topic for now)
    let mut topic_bytes = [0u8; 32];
    topic_bytes[..10].copy_from_slice(b"us-sync-v1");
    let topic_id = TopicId::from_bytes(topic_bytes);

    // Start Bevy app (headless)
    App::new()
        .add_plugins(MinimalPlugins)
        .add_message::<PublishMessageEvent>()
        .add_message::<GossipMessageReceived>()
        .insert_resource(AppConfig(config))
        .insert_resource(Database(us_db))
        .insert_resource(GossipTopic(topic_id))
        .add_systems(Startup, (setup_database, setup_gossip))
        .add_systems(
            Update,
            (
                poll_gossip_init,
                poll_chat_db,
                detect_new_messages,
                publish_to_gossip,
                receive_from_gossip,
                save_gossip_messages,
            ),
        )
        .run();
}

/// Initialize configuration and database
fn initialize_app() -> Result<(Config, Arc<Mutex<Connection>>)> {
    let config = if Path::new("config.toml").exists() {
        println!("Loading config from config.toml");
        Config::from_file("config.toml")?
    } else {
        println!("No config.toml found, using default configuration");
        let config = Config::default_config();
        config
            .save("config.toml")
            .context("Failed to save default config")?;
        println!("Saved default configuration to config.toml");
        config
    };

    println!("Configuration loaded");
    println!("  Database: {}", config.database.path);
    println!("  Chat DB: {}", config.database.chat_db_path);

    // Initialize database
    println!("Initializing database at {}", config.database.path);
    let conn = Connection::open(&config.database.path).context("Failed to open database")?;

    db::initialize_database(&conn).context("Failed to initialize database schema")?;

    let us_db = Arc::new(Mutex::new(conn));

    Ok((config, us_db))
}
