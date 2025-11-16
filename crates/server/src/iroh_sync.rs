use anyhow::Result;
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

/// Initialize Iroh endpoint and gossip for the given topic
pub async fn init_iroh_gossip(
    topic_id: TopicId,
) -> Result<(Endpoint, Gossip, Router, GossipSender, GossipReceiver)> {
    println!("Initializing Iroh endpoint...");

    // Create the Iroh endpoint
    let endpoint = Endpoint::bind().await?;
    println!("Endpoint created");

    // Build the gossip protocol
    println!("Building gossip protocol...");
    let gossip = Gossip::builder().spawn(endpoint.clone());

    // Setup the router to handle incoming connections
    println!("Setting up router...");
    let router = Router::builder(endpoint.clone())
        .accept(iroh_gossip::ALPN, gossip.clone())
        .spawn();

    // Subscribe to the topic (no bootstrap peers for now)
    println!("Subscribing to topic: {:?}", topic_id);
    let bootstrap_peers = vec![];
    let subscribe_handle = gossip.subscribe(topic_id, bootstrap_peers).await?;

    // Split into sender and receiver
    let (sender, mut receiver) = subscribe_handle.split();

    // Wait for join to complete
    println!("Waiting for gossip join...");
    receiver.joined().await?;
    println!("Gossip initialized successfully");

    Ok((endpoint, gossip, router, sender, receiver))
}
