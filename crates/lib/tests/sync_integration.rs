use lib::sync::{synced, SyncMessage, Syncable};
use iroh::{Endpoint, protocol::{Router, ProtocolHandler, AcceptError}};
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Test configuration that can be synced
#[synced]
struct TestConfig {
    value: i32,
    name: String,

    #[sync(skip)]
    node_id: String,
}

/// ALPN identifier for our sync protocol
const SYNC_ALPN: &[u8] = b"/lonni/sync/1";

/// Protocol handler for receiving sync messages
#[derive(Debug, Clone)]
struct SyncProtocol {
    config: Arc<Mutex<TestConfig>>,
}

impl ProtocolHandler for SyncProtocol {
    async fn accept(&self, connection: iroh::endpoint::Connection) -> Result<(), AcceptError> {
        println!("Accepting connection from: {}", connection.remote_id());

        // Accept the bidirectional stream
        let (mut send, mut recv) = connection.accept_bi().await
            .map_err(AcceptError::from_err)?;

        println!("Stream accepted, reading message...");

        // Read the sync message
        let bytes = recv.read_to_end(1024 * 1024).await
            .map_err(AcceptError::from_err)?;

        println!("Received {} bytes", bytes.len());

        // Deserialize and apply
        let msg = SyncMessage::<TestConfigOp>::from_bytes(&bytes)
            .map_err(|e| AcceptError::from_err(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;

        println!("Applying operation from node: {}", msg.node_id);

        let mut config = self.config.lock().await;
        config.apply_op(&msg.operation);

        println!("Operation applied successfully");

        // Close the stream
        send.finish()
            .map_err(AcceptError::from_err)?;

        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_sync_between_two_nodes() -> Result<()> {
    println!("\n=== Testing Sync Between Two Nodes ===\n");

    // Create two endpoints
    let node1 = Endpoint::builder().bind().await?;
    let node2 = Endpoint::builder().bind().await?;

    let node1_addr = node1.addr();
    let node2_addr = node2.addr();

    let node1_id = node1_addr.id.to_string();
    let node2_id = node2_addr.id.to_string();

    println!("Node 1: {}", node1_id);
    println!("Node 2: {}", node2_id);

    // Create synced configs on both nodes
    let mut config1 = TestConfig::new(
        42,
        "initial".to_string(),
        node1_id.clone(),
    );

    let config2 = TestConfig::new(
        42,
        "initial".to_string(),
        node2_id.clone(),
    );
    let config2_shared = Arc::new(Mutex::new(config2));

    println!("\nInitial state:");
    println!("  Node 1: value={}, name={}", config1.value(), config1.name());
    {
        let config2 = config2_shared.lock().await;
        println!("  Node 2: value={}, name={}", config2.value(), config2.name());
    }

    // Set up router on node2 to accept incoming connections
    println!("\nSetting up node2 router...");
    let protocol = SyncProtocol {
        config: config2_shared.clone(),
    };
    let router = Router::builder(node2)
        .accept(SYNC_ALPN, protocol)
        .spawn();

    router.endpoint().online().await;
    println!("✓ Node2 router ready");

    // Node 1 changes the value
    println!("\nNode 1 changing value to 100...");
    let op = config1.set_value(100);

    // Serialize the operation
    let sync_msg = SyncMessage::new(node1_id.clone(), op);
    let bytes = sync_msg.to_bytes()?;
    println!("Serialized to {} bytes", bytes.len());

    // Establish QUIC connection from node1 to node2
    println!("\nEstablishing QUIC connection...");
    let conn = node1.connect(node2_addr.clone(), SYNC_ALPN).await?;
    println!("✓ Connection established");

    // Open a bidirectional stream
    let (mut send, _recv) = conn.open_bi().await?;

    // Send the sync message
    println!("Sending sync message...");
    send.write_all(&bytes).await?;
    send.finish()?;
    println!("✓ Message sent");

    // Wait a bit for the message to be processed
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Verify both configs have the same value
    println!("\nFinal state:");
    println!("  Node 1: value={}, name={}", config1.value(), config1.name());
    {
        let config2 = config2_shared.lock().await;
        println!("  Node 2: value={}, name={}", config2.value(), config2.name());

        assert_eq!(*config1.value(), 100);
        assert_eq!(*config2.value(), 100);
        assert_eq!(config1.name(), "initial");
        assert_eq!(config2.name(), "initial");
    }

    println!("\n✓ Sync successful!");

    // Cleanup
    router.shutdown().await?;
    node1.close().await;

    Ok(())
}
