//! Marathon control CLI
//!
//! Send control commands to a running Marathon instance via Unix domain socket.
//!
//! # Usage
//!
//! ```bash
//! # Get session status
//! marathonctl status
//!
//! # Start networking with a session
//! marathonctl start <session-code>
//!
//! # Use custom socket
//! marathonctl --socket /tmp/marathon1.sock status
//! ```

mod ui;

use clap::{Parser, Subcommand};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use libmarathon::networking::{ControlCommand, ControlResponse};

/// Marathon control CLI
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the control socket
    #[arg(long, default_value = "/tmp/marathon-control.sock")]
    socket: String,

    /// Show sensitive information (session IDs, etc.) in full
    #[arg(short, long)]
    show_sensitive: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start networking with a session
    Start {
        /// Session code (e.g., abc-def-123)
        session_code: String,
    },
    /// Stop networking
    Stop,
    /// Get current session status
    Status,
    /// Send a test message
    Test {
        /// Message content
        content: String,
    },
    /// Broadcast a ping message
    Ping,
    /// Spawn an entity
    Spawn {
        /// Entity type (e.g., "cube")
        entity_type: String,
        /// X position
        #[arg(short, long, default_value = "0.0")]
        x: f32,
        /// Y position
        #[arg(short, long, default_value = "0.0")]
        y: f32,
        /// Z position
        #[arg(short, long, default_value = "0.0")]
        z: f32,
    },
    /// Delete an entity by UUID
    Delete {
        /// Entity UUID
        entity_id: String,
    },
}

/// Redacts a session ID for safe logging
/// Shows only the first 8 characters to prevent exposure of sensitive information
/// unless show_sensitive is true
fn redact_session_id(session_id: impl std::fmt::Display, show_sensitive: bool) -> String {
    if show_sensitive {
        session_id.to_string()
    } else {
        let session_str = session_id.to_string();
        if session_str.len() > 8 {
            format!("{}...", &session_str[..8])
        } else {
            "<redacted>".to_string()
        }
    }
}

fn main() {
    let args = Args::parse();

    // Build command from subcommand
    let command = match args.command {
        Commands::Start { session_code } => ControlCommand::JoinSession { session_code },
        Commands::Stop => ControlCommand::LeaveSession,
        Commands::Status => ControlCommand::GetStatus,
        Commands::Test { content } => ControlCommand::SendTestMessage { content },
        Commands::Ping => {
            use libmarathon::networking::{SyncMessage, VectorClock};
            use uuid::Uuid;

            // For ping, we send a SyncRequest (lightweight ping-like message)
            let node_id = Uuid::new_v4();
            ControlCommand::BroadcastMessage {
                message: SyncMessage::SyncRequest {
                    node_id,
                    vector_clock: VectorClock::new(),
                },
            }
        }
        Commands::Spawn { entity_type, x, y, z } => {
            ControlCommand::SpawnEntity {
                entity_type,
                position: [x, y, z],
            }
        }
        Commands::Delete { entity_id } => {
            use uuid::Uuid;
            match Uuid::parse_str(&entity_id) {
                Ok(uuid) => ControlCommand::DeleteEntity { entity_id: uuid },
                Err(e) => {
                    eprintln!("Invalid UUID '{}': {}", entity_id, e);
                    std::process::exit(1);
                }
            }
        }
    };

    // Connect to Unix socket
    let socket_path = &args.socket;
    let mut stream = match UnixStream::connect(&socket_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to connect to {}: {}", socket_path, e);
            eprintln!("Is the Marathon app running?");
            std::process::exit(1);
        }
    };

    // Send command
    if let Err(e) = send_command(&mut stream, &command) {
        eprintln!("Failed to send command: {}", e);
        std::process::exit(1);
    }

    // Receive response
    match receive_response(&mut stream) {
        Ok(response) => {
            print_response(response, args.show_sensitive);
        }
        Err(e) => {
            eprintln!("Failed to receive response: {}", e);
            std::process::exit(1);
        }
    }
}

fn send_command(stream: &mut UnixStream, command: &ControlCommand) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = command.to_bytes()?;
    let len = bytes.len() as u32;

    // Write length prefix
    stream.write_all(&len.to_le_bytes())?;
    // Write command bytes
    stream.write_all(&bytes)?;
    stream.flush()?;

    Ok(())
}

fn receive_response(stream: &mut UnixStream) -> Result<ControlResponse, Box<dyn std::error::Error>> {
    // Read length prefix
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;

    // Read response bytes
    let mut response_buf = vec![0u8; len];
    stream.read_exact(&mut response_buf)?;

    // Deserialize response
    let response = ControlResponse::from_bytes(&response_buf)?;
    Ok(response)
}

fn print_response(response: ControlResponse, show_sensitive: bool) {
    match response {
        ControlResponse::Status {
            node_id,
            session_id,
            outgoing_queue_size,
            incoming_queue_size,
            connected_peers,
        } => {
            let mut builder = ui::table("Session Status")
                .row("Node ID", node_id)
                .row("Session", redact_session_id(session_id, show_sensitive))
                .row("Outgoing Queue", format!("{} messages", outgoing_queue_size))
                .row("Incoming Queue", format!("{} messages", incoming_queue_size));

            if let Some(peers) = connected_peers {
                builder = builder.row("Connected Peers", peers);
            }

            builder.render();
        }
        ControlResponse::SessionInfo(info) => {
            let mut builder = ui::table("Session Info")
                .row("ID", redact_session_id(&info.session_id, show_sensitive));

            if let Some(ref name) = info.session_name {
                builder = builder.row("Name", name);
            }

            builder
                .row("State", format!("{:?}", info.state))
                .row("Entities", info.entity_count)
                .row("Created", info.created_at)
                .row("Last Active", info.last_active)
                .render();
        }
        ControlResponse::Sessions(sessions) => {
            if sessions.is_empty() {
                println!("No sessions found");
                return;
            }

            let mut builder = ui::grid(&format!("Sessions ({})", sessions.len()))
                .header(&["Session ID", "State", "Entities"]);

            for session in sessions {
                builder = builder.row(&[
                    redact_session_id(&session.session_id, show_sensitive),
                    format!("{:?}", session.state),
                    session.entity_count.to_string(),
                ]);
            }

            builder.render();
        }
        ControlResponse::Peers(peers) => {
            if peers.is_empty() {
                println!("No connected peers");
                return;
            }

            let mut builder = ui::list(&format!("Connected Peers ({})", peers.len()));

            for peer in peers {
                let item = if let Some(since) = peer.connected_since {
                    format!("{} (connected since: {})", peer.node_id, since)
                } else {
                    peer.node_id.to_string()
                };
                builder = builder.item(item);
            }

            builder.render();
        }
        ControlResponse::Ok { message } => {
            println!("Success: {}", message);
        }
        ControlResponse::Error { error } => {
            eprintln!("Error: {}", error);
            std::process::exit(1);
        }
    }
}
