//! Standalone control socket for engine control
//!
//! This control socket starts at app launch and allows external control
//! of the engine, including starting/stopping networking, before any
//! networking is initialized.

use anyhow::Result;
use bevy::prelude::*;
use crossbeam_channel::{
    Receiver,
    Sender,
    unbounded,
};
use libmarathon::{
    engine::{
        EngineBridge,
        EngineCommand,
    },
    networking::{
        ControlCommand,
        ControlResponse,
        SessionId,
    },
};
use uuid::Uuid;

/// Resource holding the control socket path
#[derive(Resource)]
pub struct ControlSocketPath(pub String);

/// Resource holding the shutdown sender for control socket
#[derive(Resource)]
pub struct ControlSocketShutdown(Option<Sender<()>>);

pub fn cleanup_control_socket(
    mut exit_events: MessageReader<bevy::app::AppExit>,
    socket_path: Option<Res<ControlSocketPath>>,
    shutdown: Option<Res<ControlSocketShutdown>>,
) {
    for _ in exit_events.read() {
        // Send shutdown signal to control socket thread
        if let Some(ref shutdown_res) = shutdown {
            if let Some(ref sender) = shutdown_res.0 {
                info!("Sending shutdown signal to control socket");
                let _ = sender.send(());
            }
        }

        // Clean up socket file
        if let Some(ref path) = socket_path {
            info!("Cleaning up control socket at {}", path.0);
            let _ = std::fs::remove_file(&path.0);
        }
    }
}

/// Commands that can be sent from the control socket to the app
#[derive(Debug, Clone)]
pub enum AppCommand {
    SpawnEntity { entity_type: String, position: Vec3 },
    DeleteEntity { entity_id: Uuid },
}

/// Queue for app-level commands from control socket
#[derive(Resource, Clone)]
pub struct AppCommandQueue {
    sender: Sender<AppCommand>,
    receiver: Receiver<AppCommand>,
}

impl AppCommandQueue {
    pub fn new() -> Self {
        let (sender, receiver) = unbounded();
        Self { sender, receiver }
    }

    pub fn send(&self, command: AppCommand) {
        let _ = self.sender.send(command);
    }

    pub fn try_recv(&self) -> Option<AppCommand> {
        self.receiver.try_recv().ok()
    }
}

impl Default for AppCommandQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Startup system to launch the control socket server
#[cfg(not(target_os = "ios"))]
#[cfg(debug_assertions)]
pub fn start_control_socket_system(
    mut commands: Commands,
    socket_path_res: Res<ControlSocketPath>,
    bridge: Res<EngineBridge>,
) {
    use tokio::{
        io::AsyncReadExt,
        net::UnixListener,
    };

    let socket_path = socket_path_res.0.clone();
    info!("Starting control socket at {}", socket_path);

    // Create app command queue
    let app_queue = AppCommandQueue::new();
    commands.insert_resource(app_queue.clone());

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = unbounded::<()>();
    commands.insert_resource(ControlSocketShutdown(Some(shutdown_tx)));

    // Clone bridge and queue for the async task
    let bridge = bridge.clone();
    let queue = app_queue;

    // Spawn tokio runtime in background thread
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            // Clean up any existing socket
            let _ = std::fs::remove_file(&socket_path);

            let listener = match UnixListener::bind(&socket_path) {
                | Ok(l) => {
                    info!("Control socket listening at {}", socket_path);
                    l
                },
                | Err(e) => {
                    error!("Failed to bind control socket: {}", e);
                    return;
                },
            };

            // Accept connections in a loop with shutdown support
            loop {
                tokio::select! {
                    // Check for shutdown signal
                    _ = tokio::task::spawn_blocking({
                        let rx = shutdown_rx.clone();
                        move || rx.try_recv()
                    }) => {
                        info!("Control socket received shutdown signal");
                        break;
                    }
                    // Accept new connection
                    result = listener.accept() => {
                        match result {
                            Ok((mut stream, _addr)) => {
                                let bridge = bridge.clone();

                                let queue_clone = queue.clone();
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
                            let response = handle_command(command, &bridge, &queue_clone).await;

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
                }
            }
            info!("Control socket server shut down cleanly");
        });
    });
}

/// Handle a control command and generate a response
#[cfg(not(target_os = "ios"))]
#[cfg(debug_assertions)]
async fn handle_command(
    command: ControlCommand,
    bridge: &EngineBridge,
    app_queue: &AppCommandQueue,
) -> ControlResponse {
    match command {
        | ControlCommand::JoinSession { session_code } => {
            match SessionId::from_code(&session_code) {
                | Ok(session_id) => {
                    bridge.send_command(EngineCommand::StartNetworking {
                        session_id: session_id.clone(),
                    });
                    ControlResponse::Ok {
                        message: format!("Starting networking with session: {}", session_id),
                    }
                },
                | Err(e) => ControlResponse::Error {
                    error: format!("Invalid session code: {}", e),
                },
            }
        },

        | ControlCommand::LeaveSession => {
            bridge.send_command(EngineCommand::StopNetworking);
            ControlResponse::Ok {
                message: "Stopping networking".to_string(),
            }
        },

        | ControlCommand::SpawnEntity {
            entity_type,
            position,
        } => {
            app_queue.send(AppCommand::SpawnEntity {
                entity_type,
                position: Vec3::from_array(position),
            });
            ControlResponse::Ok {
                message: "Entity spawn command queued".to_string(),
            }
        },

        | ControlCommand::DeleteEntity { entity_id } => {
            app_queue.send(AppCommand::DeleteEntity { entity_id });
            ControlResponse::Ok {
                message: format!("Entity delete command queued for {}", entity_id),
            }
        },

        | _ => ControlResponse::Error {
            error: format!("Command {:?} not yet implemented", command),
        },
    }
}

/// System to process app commands from the control socket
pub fn process_app_commands(
    queue: Option<Res<AppCommandQueue>>,
    mut spawn_cube_writer: MessageWriter<crate::cube::SpawnCubeEvent>,
    mut delete_cube_writer: MessageWriter<crate::cube::DeleteCubeEvent>,
) {
    let Some(queue) = queue else { return };

    while let Some(command) = queue.try_recv() {
        match command {
            | AppCommand::SpawnEntity {
                entity_type,
                position,
            } => match entity_type.as_str() {
                | "cube" => {
                    info!("Spawning cube at {:?}", position);
                    spawn_cube_writer.write(crate::cube::SpawnCubeEvent { position });
                },
                | _ => {
                    warn!("Unknown entity type: {}", entity_type);
                },
            },
            | AppCommand::DeleteEntity { entity_id } => {
                info!("Deleting entity {}", entity_id);
                delete_cube_writer.write(crate::cube::DeleteCubeEvent { entity_id });
            },
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

    stream.write_all(&len.to_le_bytes()).await?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;

    Ok(())
}

// No-op stubs for iOS and release builds
#[cfg(any(target_os = "ios", not(debug_assertions)))]
pub fn start_control_socket_system(mut commands: Commands) {
    // Insert empty shutdown resource for consistency
    commands.insert_resource(ControlSocketShutdown(None));
}
