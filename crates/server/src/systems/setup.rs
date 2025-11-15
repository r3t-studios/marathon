use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;

use crate::components::*;

/// Startup system: Initialize database
pub fn setup_database(_db: Res<Database>) {
    println!("Database resource initialized");
}

/// Startup system: Initialize Iroh gossip
pub fn setup_gossip(mut commands: Commands, topic: Res<GossipTopic>) {
    println!("Setting up Iroh gossip for topic: {:?}", topic.0);

    let topic_id = topic.0;

    // TODO: Initialize gossip properly
    // For now, skip async initialization due to Sync requirements in Bevy tasks
    // We'll need to use a different initialization strategy

    println!("Gossip initialization skipped (TODO: implement proper async init)");
}
