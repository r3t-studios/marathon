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

use clap::{Parser, Subcommand};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use libmarathon::networking::{ControlCommand, ControlResponse, SessionId};

/// Marathon control CLI
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the control socket
    #[arg(long, default_value = "/tmp/marathon-control.sock")]
    socket: String,

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
            print_response(response);
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

fn print_response(response: ControlResponse) {
    use libmarathon::networking::{SessionInfo, PeerInfo};

    match response {
        ControlResponse::Status {
            node_id,
            session_id,
            outgoing_queue_size,
            incoming_queue_size,
            connected_peers,
        } => {
            println!("Session Status:");
            println!("  Node ID: {}", node_id);
            println!("  Session: {}", session_id);
            println!("  Outgoing Queue: {} messages", outgoing_queue_size);
            println!("  Incoming Queue: {} messages", incoming_queue_size);
            if let Some(peers) = connected_peers {
                println!("  Connected Peers: {}", peers);
            }
        }
        ControlResponse::SessionInfo(info) => {
            println!("Session Info:");
            println!("  ID: {}", info.session_id);
            if let Some(ref name) = info.session_name {
                println!("  Name: {}", name);
            }
            println!("  State: {:?}", info.state);
            println!("  Entities: {}", info.entity_count);
            println!("  Created: {}", info.created_at);
            println!("  Last Active: {}", info.last_active);
        }
        ControlResponse::Sessions(sessions) => {
            println!("Sessions ({} total):", sessions.len());
            for session in sessions {
                println!("  {}: {:?} ({} entities)", session.session_id, session.state, session.entity_count);
            }
        }
        ControlResponse::Peers(peers) => {
            println!("Connected Peers ({} total):", peers.len());
            for peer in peers {
                print!("  {}", peer.node_id);
                if let Some(since) = peer.connected_since {
                    println!(" (connected since: {})", since);
                } else {
                    println!();
                }
            }
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
