//! Standalone control socket for engine control
//!
//! This control socket starts at app launch and allows external control
//! of the engine, including starting/stopping networking, before any
//! networking is initialized.

use anyhow::Result;
use bevy::prelude::*;
use libmarathon::{
    engine::{EngineBridge, EngineCommand},
    networking::{ControlCommand, ControlResponse, SessionId},
};

/// Resource holding the control socket path
#[derive(Resource)]
pub struct ControlSocketPath(pub String);

/// Startup system to launch the control socket server
#[cfg(not(target_os = "ios"))]
#[cfg(debug_assertions)]
pub fn start_control_socket_system(socket_path_res: Res<ControlSocketPath>, bridge: Res<EngineBridge>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    let socket_path = socket_path_res.0.clone();
    info!("Starting control socket at {}", socket_path);

    // Clone bridge for the async task
    let bridge = bridge.clone();

    // Spawn tokio runtime in background thread
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            // Clean up any existing socket
            let _ = std::fs::remove_file(&socket_path);

            let listener = match UnixListener::bind(&socket_path) {
                Ok(l) => {
                    info!("Control socket listening at {}", socket_path);
                    l
                }
                Err(e) => {
                    error!("Failed to bind control socket: {}", e);
                    return;
                }
            };

            // Accept connections in a loop
            loop {
                match listener.accept().await {
                    Ok((mut stream, _addr)) => {
                        let bridge = bridge.clone();

                        tokio::spawn(async move {
                            // Read command length
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
                                        error: format!("Failed to deserialize: {}", e),
                                    };
                                    let _ = send_response(&mut stream, response).await;
                                    return;
                                }
                            };

                            info!("Received control command: {:?}", command);

                            // Handle command
                            let response = handle_command(command, &bridge).await;

                            // Send response
                            if let Err(e) = send_response(&mut stream, response).await {
                                error!("Failed to send response: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Failed to accept connection: {}", e);
                    }
                }
            }
        });
    });
}

/// Handle a control command and generate a response
#[cfg(not(target_os = "ios"))]
#[cfg(debug_assertions)]
async fn handle_command(command: ControlCommand, bridge: &EngineBridge) -> ControlResponse {
    match command {
        ControlCommand::JoinSession { session_code } => {
            match SessionId::from_code(&session_code) {
                Ok(session_id) => {
                    bridge.send_command(EngineCommand::StartNetworking {
                        session_id: session_id.clone(),
                    });
                    ControlResponse::Ok {
                        message: format!("Starting networking with session: {}", session_id),
                    }
                }
                Err(e) => ControlResponse::Error {
                    error: format!("Invalid session code: {}", e),
                },
            }
        }

        ControlCommand::LeaveSession => {
            bridge.send_command(EngineCommand::StopNetworking);
            ControlResponse::Ok {
                message: "Stopping networking".to_string(),
            }
        }

        _ => ControlResponse::Error {
            error: format!("Command {:?} not yet implemented", command),
        },
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

    stream.write_all(&len.to_le_bytes()).await?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;

    Ok(())
}

// No-op stubs for iOS and release builds
#[cfg(any(target_os = "ios", not(debug_assertions)))]
pub fn start_control_socket_system() {}
