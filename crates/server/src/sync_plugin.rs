use bevy::prelude::*;
use lib::sync::{Syncable, SyncMessage};
use crate::components::*;

/// Bevy plugin for transparent CRDT sync via gossip
pub struct SyncPlugin;

impl Plugin for SyncPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (
            publish_sync_ops,
            receive_sync_ops,
        ));
    }
}

/// Trait for Bevy resources that can be synced
pub trait SyncedResource: Resource + Syncable + Clone + Send + Sync + 'static {}

/// Queue of sync operations to publish
#[derive(Resource, Default)]
pub struct SyncOpQueue<T: Syncable> {
    pub ops: Vec<T::Operation>,
}

impl<T: Syncable> SyncOpQueue<T> {
    pub fn push(&mut self, op: T::Operation) {
        self.ops.push(op);
    }
}

/// System to publish sync operations to gossip
fn publish_sync_ops<T: SyncedResource>(
    mut queue: ResMut<SyncOpQueue<T>>,
    resource: Res<T>,
    sender: Option<Res<IrohGossipSender>>,
) {
    if sender.is_none() || queue.ops.is_empty() {
        return;
    }

    let sender = sender.unwrap();
    let sender_guard = sender.sender.lock();

    for op in queue.ops.drain(..) {
        let sync_msg = resource.create_sync_message(op);

        match sync_msg.to_bytes() {
            Ok(bytes) => {
                println!("Publishing sync operation: {} bytes", bytes.len());
                // TODO: Actually send via gossip
                // sender_guard.broadcast(bytes)?;
            }
            Err(e) => {
                eprintln!("Failed to serialize sync operation: {}", e);
            }
        }
    }
}

/// System to receive and apply sync operations from gossip
fn receive_sync_ops<T: SyncedResource>(
    mut resource: ResMut<T>,
    receiver: Option<Res<IrohGossipReceiver>>,
) {
    if receiver.is_none() {
        return;
    }

    // TODO: Poll receiver for messages
    // For each message:
    //   1. Deserialize SyncMessage<T::Operation>
    //   2. Apply to resource with resource.apply_sync_op(&op)
}

/// Helper to register a synced resource
pub trait SyncedResourceExt {
    fn add_synced_resource<T: SyncedResource>(&mut self) -> &mut Self;
}

impl SyncedResourceExt for App {
    fn add_synced_resource<T: SyncedResource>(&mut self) -> &mut Self {
        self.init_resource::<SyncOpQueue<T>>();
        self
    }
}

/// Example synced resource
#[cfg(test)]
mod tests {
    use super::*;
    use lib::sync::synced;

    #[synced]
    pub struct TestConfig {
        pub value: i32,

        #[sync(skip)]
        node_id: String,
    }

    impl Resource for TestConfig {}
    impl SyncedResource for TestConfig {}

    #[test]
    fn test_sync_plugin() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(SyncPlugin);
        app.add_synced_resource::<TestConfig>();

        // TODO: Test that operations are queued and published
    }
}
