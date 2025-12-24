//! Unix domain socket control server for remote engine control
//!
//! This module provides a Unix socket server for controlling the engine
//! programmatically without needing screen access or network ports.
//!
//! # Security
//!
//! Currently debug-only. See issue #135 for production security requirements.

use anyhow::Result;
use bevy::prelude::*;
use libmarathon::networking::{ControlCommand, ControlResponse, GossipBridge, SessionId};
use uuid::Uuid;

/// Spawn Unix domain socket control server for remote engine control
///
/// This spawns a tokio task that listens on a Unix socket for control commands.
/// The socket path is `/tmp/marathon-{session_id}.sock`.
///
/// **Security Note**: This is currently debug-only. See issue #135 for production
/// security requirements (authentication, rate limiting, etc.).
///
/// # Platform Support
///
/// This function is only compiled on non-iOS platforms.
#[cfg(not(target_os = "ios"))]
#[cfg(debug_assertions)]
pub fn spawn_control_socket(session_id: SessionId, bridge: GossipBridge, node_id: Uuid) {
    use tokio::io::AsyncReadExt;
    use tokio::net::UnixListener;

    let socket_path = format!("/tmp/marathon-{}.sock", session_id);

    tokio::spawn(async move {
        // Clean up any existing socket
        let _ = std::fs::remove_file(&socket_path);

        let listener = match UnixListener::bind(&socket_path) {
            Ok(l) => {
                info!("Control socket listening at {}", socket_path);
                l
            }
            Err(e) => {
                error!("Failed to bind control socket at {}: {}", socket_path, e);
                return;
            }
        };

        // Accept connections in a loop
        loop {
            match listener.accept().await {
                Ok((mut stream, _addr)) => {
                    let bridge = bridge.clone();
                    let session_id = session_id.clone();

                    // Spawn a task to handle this connection
                    tokio::spawn(async move {
                        // Read command length (4 bytes)
                        let mut len_buf = [0u8; 4];
                        if let Err(e) = stream.read_exact(&mut len_buf).await {
                            error!("Failed to read command length: {}", e);
                            return;
                        }
                        let len = u32::from_le_bytes(len_buf) as usize;

                        // Read command bytes
                        let mut cmd_buf = vec![0u8; len];
                        if let Err(e) = stream.read_exact(&mut cmd_buf).await {
                            error!("Failed to read command: {}", e);
                            return;
                        }

                        // Deserialize command
                        let command = match ControlCommand::from_bytes(&cmd_buf) {
                            Ok(cmd) => cmd,
                            Err(e) => {
                                error!("Failed to deserialize command: {}", e);
                                let response = ControlResponse::Error {
                                    error: format!("Failed to deserialize command: {}", e),
                                };
                                let _ = send_response(&mut stream, response).await;
                                return;
                            }
                        };

                        info!("Received control command: {:?}", command);

                        // Execute command
                        let response = handle_control_command(command, &bridge, session_id, node_id).await;

                        // Send response
                        if let Err(e) = send_response(&mut stream, response).await {
                            error!("Failed to send response: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept control socket connection: {}", e);
                }
            }
        }
    });
}

/// Handle a control command and return a response
#[cfg(not(target_os = "ios"))]
#[cfg(debug_assertions)]
async fn handle_control_command(
    command: ControlCommand,
    bridge: &GossipBridge,
    session_id: SessionId,
    node_id: Uuid,
) -> ControlResponse {
    match command {
        ControlCommand::GetStatus => {
            // Get queue sizes from bridge
            let outgoing_size = bridge.try_recv_outgoing().map(|msg| {
                // Put it back
                let _ = bridge.send(msg);
                1
            }).unwrap_or(0);

            ControlResponse::Status {
                node_id,
                session_id,
                outgoing_queue_size: outgoing_size,
                incoming_queue_size: 0, // We'd need to peek without consuming
                connected_peers: None, // Not easily available from bridge
            }
        }
        ControlCommand::SendTestMessage { content } => {
            use libmarathon::networking::{VersionedMessage, VectorClock, SyncMessage};

            // Send a SyncRequest as a test message (lightweight ping-like message)
            let message = SyncMessage::SyncRequest {
                node_id,
                vector_clock: VectorClock::new(),
            };
            let versioned = VersionedMessage::new(message);

            match bridge.send(versioned) {
                Ok(_) => ControlResponse::Ok {
                    message: format!("Sent test message: {}", content),
                },
                Err(e) => ControlResponse::Error {
                    error: format!("Failed to send: {}", e),
                },
            }
        }
        ControlCommand::InjectMessage { message } => {
            match bridge.push_incoming(message) {
                Ok(_) => ControlResponse::Ok {
                    message: "Message injected into incoming queue".to_string(),
                },
                Err(e) => ControlResponse::Error {
                    error: format!("Failed to inject message: {}", e),
                },
            }
        }
        ControlCommand::BroadcastMessage { message } => {
            use libmarathon::networking::VersionedMessage;

            let versioned = VersionedMessage::new(message);
            match bridge.send(versioned) {
                Ok(_) => ControlResponse::Ok {
                    message: "Message broadcast".to_string(),
                },
                Err(e) => ControlResponse::Error {
                    error: format!("Failed to broadcast: {}", e),
                },
            }
        }
        ControlCommand::Shutdown => {
            warn!("Shutdown command received via control socket");
            ControlResponse::Ok {
                message: "Shutdown not yet implemented".to_string(),
            }
        }

        // Session lifecycle commands (TODO: implement these properly)
        ControlCommand::JoinSession { session_code } => {
            ControlResponse::Error {
                error: format!("JoinSession not yet implemented (requested: {})", session_code),
            }
        }
        ControlCommand::LeaveSession => {
            ControlResponse::Error {
                error: "LeaveSession not yet implemented".to_string(),
            }
        }
        ControlCommand::GetSessionInfo => {
            ControlResponse::Error {
                error: "GetSessionInfo not yet implemented".to_string(),
            }
        }
        ControlCommand::ListSessions => {
            ControlResponse::Error {
                error: "ListSessions not yet implemented".to_string(),
            }
        }
        ControlCommand::DeleteSession { session_code } => {
            ControlResponse::Error {
                error: format!("DeleteSession not yet implemented (requested: {})", session_code),
            }
        }
        ControlCommand::ListPeers => {
            ControlResponse::Error {
                error: "ListPeers not yet implemented".to_string(),
            }
        }
        ControlCommand::SpawnEntity { .. } => {
            ControlResponse::Error {
                error: "SpawnEntity not available on session-level socket. Use app-level socket.".to_string(),
            }
        }
        ControlCommand::DeleteEntity { .. } => {
            ControlResponse::Error {
                error: "DeleteEntity not available on session-level socket. Use app-level socket.".to_string(),
            }
        }
    }
}

/// Send a response back through the Unix socket
#[cfg(not(target_os = "ios"))]
#[cfg(debug_assertions)]
async fn send_response(
    stream: &mut tokio::net::UnixStream,
    response: ControlResponse,
) -> Result<()> {
    use tokio::io::AsyncWriteExt;

    let bytes = response.to_bytes()?;
    let len = bytes.len() as u32;

    // Write length prefix
    stream.write_all(&len.to_le_bytes()).await?;
    // Write response bytes
    stream.write_all(&bytes).await?;
    stream.flush().await?;

    Ok(())
}

// No-op stub for iOS builds
#[cfg(target_os = "ios")]
pub fn spawn_control_socket(_session_id: SessionId, _bridge: GossipBridge, _node_id: Uuid) {}

// No-op stub for release builds
#[cfg(all(not(target_os = "ios"), not(debug_assertions)))]
pub fn spawn_control_socket(_session_id: SessionId, _bridge: GossipBridge, _node_id: Uuid) {
    // TODO(#135): Implement secure control socket for release builds with authentication
}
