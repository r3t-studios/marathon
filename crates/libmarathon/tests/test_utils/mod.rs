//! Shared test utilities for integration tests
//!
//! This module provides common test infrastructure that all integration tests
//! use:
//! - Real iroh-gossip setup with localhost connections
//! - Test app creation with networking + persistence
//! - Wait helpers for async sync verification

pub mod gossip;

use std::{
    path::PathBuf,
    time::Duration,
};

use anyhow::Result;
use bevy::{
    MinimalPlugins,
    app::{
        App,
        ScheduleRunnerPlugin,
    },
    prelude::*,
};
pub use gossip::{
    init_gossip_node,
    setup_gossip_pair,
    setup_gossip_trio,
    spawn_gossip_bridge_tasks,
};
use libmarathon::{
    networking::{
        GossipBridge,
        NetworkingConfig,
        NetworkingPlugin,
    },
    persistence::{
        PersistenceConfig,
        PersistencePlugin,
    },
};
use tempfile::TempDir;
use tokio::time::Instant;
use uuid::Uuid;

/// Test context that manages temporary directories with RAII cleanup
pub struct TestContext {
    temp_dir: TempDir,
}

impl TestContext {
    pub fn new() -> Self {
        Self {
            temp_dir: TempDir::new().expect("Failed to create temp directory"),
        }
    }

    pub fn db_path(&self) -> PathBuf {
        self.temp_dir.path().join("test.db")
    }
}

/// Create a test app with networking and persistence
pub fn create_test_app(node_id: Uuid, db_path: PathBuf, bridge: GossipBridge) -> App {
    create_test_app_maybe_offline(node_id, db_path, Some(bridge))
}

/// Create a test app with optional bridge (for testing offline scenarios)
pub fn create_test_app_maybe_offline(
    node_id: Uuid,
    db_path: PathBuf,
    bridge: Option<GossipBridge>,
) -> App {
    let mut app = App::new();

    app.add_plugins(
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
            1.0 / 60.0,
        ))),
    )
    .add_plugins(NetworkingPlugin::new(NetworkingConfig {
        node_id,
        sync_interval_secs: 0.5,
        prune_interval_secs: 10.0,
        tombstone_gc_interval_secs: 30.0,
    }))
    .add_plugins(PersistencePlugin::with_config(
        db_path,
        PersistenceConfig {
            flush_interval_secs: 1,
            checkpoint_interval_secs: 5,
            battery_adaptive: false,
            ..Default::default()
        },
    ));

    // Insert bridge if provided (online mode)
    if let Some(bridge) = bridge {
        app.insert_resource(bridge);
    }

    app
}

/// Helper to ensure FixedUpdate and FixedPostUpdate run (since they're on a
/// fixed timestep)
pub fn update_with_fixed(app: &mut App) {
    use bevy::prelude::{
        FixedPostUpdate,
        FixedUpdate,
    };
    // Run Main schedule (which includes Update)
    app.update();
    // Explicitly run FixedUpdate to ensure systems there execute
    app.world_mut().run_schedule(FixedUpdate);
    // Explicitly run FixedPostUpdate to ensure delta generation executes
    app.world_mut().run_schedule(FixedPostUpdate);
}

/// Wait for sync condition to be met, polling both apps
pub async fn wait_for_sync<F>(
    app1: &mut App,
    app2: &mut App,
    timeout: Duration,
    check_fn: F,
) -> Result<()>
where
    F: Fn(&mut World, &mut World) -> bool, {
    let start = Instant::now();
    let mut tick_count = 0;

    while start.elapsed() < timeout {
        // Tick both apps
        update_with_fixed(app1);
        update_with_fixed(app2);
        tick_count += 1;

        if tick_count % 50 == 0 {
            println!(
                "Waiting for sync... tick {} ({:.1}s elapsed)",
                tick_count,
                start.elapsed().as_secs_f32()
            );
        }

        // Check condition
        if check_fn(app1.world_mut(), app2.world_mut()) {
            println!(
                "Sync completed after {} ticks ({:.3}s)",
                tick_count,
                start.elapsed().as_secs_f32()
            );
            return Ok(());
        }

        // Small delay to avoid spinning
        tokio::time::sleep(Duration::from_millis(16)).await;
    }

    println!("Sync timeout after {} ticks", tick_count);
    anyhow::bail!("Sync timeout after {:?}. Condition not met.", timeout)
}

/// Count entities with a specific network_id
pub fn count_entities_with_id(world: &mut World, network_id: Uuid) -> usize {
    use libmarathon::networking::NetworkedEntity;

    let mut query = world.query::<&NetworkedEntity>();
    query
        .iter(world)
        .filter(|ne| ne.network_id == network_id)
        .count()
}
