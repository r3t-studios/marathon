use std::sync::Arc;

use bevy::prelude::*;
use iroh::{
    Endpoint,
    protocol::Router,
};
use iroh_gossip::{
    api::{
        GossipReceiver,
        GossipSender,
    },
    net::Gossip,
    proto::TopicId,
};
use parking_lot::Mutex;
use serde::{
    Deserialize,
    Serialize,
};

/// Message envelope for gossip sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMessage {
    /// The actual message from iMessage
    pub message: lib::Message,
    /// Timestamp when this was published to gossip
    pub sync_timestamp: i64,
    /// ID of the node that published this
    pub publisher_node_id: String,
}

/// Bevy resource wrapping the gossip handle
#[derive(Resource, Clone)]
pub struct IrohGossipHandle {
    pub gossip: Gossip,
}

/// Bevy resource wrapping the gossip sender
#[derive(Resource)]
pub struct IrohGossipSender {
    pub sender: Arc<Mutex<GossipSender>>,
}

/// Bevy resource wrapping the gossip receiver
#[derive(Resource)]
pub struct IrohGossipReceiver {
    pub receiver: Arc<Mutex<GossipReceiver>>,
}

/// Bevy resource with Iroh router
#[derive(Resource)]
pub struct IrohRouter {
    pub router: Router,
}

/// Bevy resource with Iroh endpoint
#[derive(Resource, Clone)]
pub struct IrohEndpoint {
    pub endpoint: Endpoint,
    pub node_id: String,
}

/// Bevy resource for gossip topic ID
#[derive(Resource)]
pub struct GossipTopic(pub TopicId);

/// Bevy resource for tracking gossip initialization task
#[derive(Resource)]
pub struct GossipInitTask(
    pub bevy::tasks::Task<Option<(Endpoint, Gossip, Router, GossipSender, GossipReceiver)>>,
);

/// Bevy message: a new message that needs to be published to gossip
#[derive(Message, Clone, Debug)]
pub struct PublishMessageEvent {
    pub message: lib::Message,
}

/// Bevy message: a message received from gossip that needs to be saved to
/// SQLite
#[derive(Message, Clone, Debug)]
pub struct GossipMessageReceived {
    pub sync_message: SyncMessage,
}

/// Helper to serialize a sync message
pub fn serialize_sync_message(msg: &SyncMessage) -> anyhow::Result<Vec<u8>> {
    Ok(serde_json::to_vec(msg)?)
}

/// Helper to deserialize a sync message
pub fn deserialize_sync_message(data: &[u8]) -> anyhow::Result<SyncMessage> {
    Ok(serde_json::from_slice(data)?)
}
