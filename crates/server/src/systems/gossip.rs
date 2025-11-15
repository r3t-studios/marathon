use bevy::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;

use crate::components::*;

/// System: Poll the gossip init task and insert resources when complete
pub fn poll_gossip_init(
    mut commands: Commands,
    mut init_task: Option<ResMut<GossipInitTask>>,
) {
    if let Some(mut task) = init_task {
        // Check if the task is finished (non-blocking)
        if let Some(result) = bevy::tasks::block_on(bevy::tasks::futures_lite::future::poll_once(&mut task.0)) {
            if let Some((endpoint, gossip, router, sender, receiver)) = result {
                println!("Inserting gossip resources");

                // Insert all the resources
                commands.insert_resource(IrohEndpoint {
                    endpoint,
                    node_id: "TODO".to_string(), // TODO: Figure out how to get node_id in iroh 0.95
                });
                commands.insert_resource(IrohGossipHandle { gossip });
                commands.insert_resource(IrohRouter { router });
                commands.insert_resource(IrohGossipSender {
                    sender: Arc::new(Mutex::new(sender)),
                });
                commands.insert_resource(IrohGossipReceiver {
                    receiver: Arc::new(Mutex::new(receiver)),
                });

                // Remove the init task
                commands.remove_resource::<GossipInitTask>();
            }
        }
    }
}

/// System: Detect new messages in SQLite that need to be published to gossip
pub fn detect_new_messages(
    _db: Res<Database>,
    _last_synced: Local<i64>,
    _publish_events: MessageWriter<PublishMessageEvent>,
) {
    // TODO: Query SQLite for messages with rowid > last_synced
    // When we detect new messages, we'll send PublishMessageEvent
}

/// System: Publish messages to gossip when PublishMessageEvent is triggered
pub fn publish_to_gossip(
    mut events: MessageReader<PublishMessageEvent>,
    sender: Option<Res<IrohGossipSender>>,
    endpoint: Option<Res<IrohEndpoint>>,
) {
    if sender.is_none() || endpoint.is_none() {
        // Gossip not initialized yet, skip
        return;
    }

    let sender = sender.unwrap();
    let endpoint = endpoint.unwrap();

    for event in events.read() {
        println!("Publishing message {} to gossip", event.message.rowid);

        // Create sync message
        let sync_message = SyncMessage {
            message: event.message.clone(),
            sync_timestamp: chrono::Utc::now().timestamp(),
            publisher_node_id: endpoint.node_id.clone(),
        };

        // Serialize the message
        match serialize_sync_message(&sync_message) {
            Ok(bytes) => {
                // TODO: Publish to gossip
                // For now, just log that we would publish
                println!("Would publish {} bytes to gossip", bytes.len());

                // Note: Direct async broadcasting from Bevy systems is tricky due to Sync requirements
                // We'll need to use a different approach, possibly with channels or a dedicated task
            }
            Err(e) => {
                eprintln!("Failed to serialize sync message: {}", e);
            }
        }
    }
}

/// System: Receive messages from gossip
pub fn receive_from_gossip(
    mut _gossip_events: MessageWriter<GossipMessageReceived>,
    receiver: Option<Res<IrohGossipReceiver>>,
) {
    if receiver.is_none() {
        // Gossip not initialized yet, skip
        return;
    }

    // TODO: Implement proper async message reception
    // This will require spawning a long-running task that listens for gossip events
    // and sends them as Bevy messages. For now, this is a placeholder.
}

/// System: Save received gossip messages to SQLite
pub fn save_gossip_messages(
    mut events: MessageReader<GossipMessageReceived>,
    _db: Res<Database>,
) {
    for event in events.read() {
        println!("Received message {} from gossip (published by {})",
            event.sync_message.message.rowid,
            event.sync_message.publisher_node_id);
        // TODO: Save to SQLite if we don't already have it
    }
}
