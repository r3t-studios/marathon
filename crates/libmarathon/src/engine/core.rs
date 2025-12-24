//! Core Engine event loop - runs on tokio outside Bevy

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{EngineCommand, EngineEvent, EngineHandle, NetworkingManager, PersistenceManager};
use crate::networking::{SessionId, VectorClock};

pub struct EngineCore {
    handle: EngineHandle,
    networking_task: Option<JoinHandle<()>>,
    networking_cancel_token: Option<CancellationToken>,
    #[allow(dead_code)]
    persistence: PersistenceManager,

    // Clock state
    node_id: Uuid,
    clock: VectorClock,
}

impl EngineCore {
    pub fn new(handle: EngineHandle, db_path: &str) -> Self {
        let persistence = PersistenceManager::new(db_path);
        let node_id = Uuid::new_v4();
        let clock = VectorClock::new();

        tracing::info!("EngineCore node ID: {}", node_id);

        Self {
            handle,
            networking_task: None, // Start offline
            networking_cancel_token: None,
            persistence,
            node_id,
            clock,
        }
    }

    /// Start the engine event loop (runs on tokio)
    /// Processes commands unbounded - tokio handles internal polling
    pub async fn run(mut self) {
        tracing::info!("EngineCore starting (unbounded)...");

        // Process commands as they arrive
        while let Some(cmd) = self.handle.command_rx.recv().await {
            self.handle_command(cmd).await;
        }

        tracing::info!("EngineCore shutting down (command channel closed)");
    }

    async fn handle_command(&mut self, cmd: EngineCommand) {
        match cmd {
            EngineCommand::StartNetworking { session_id } => {
                self.start_networking(session_id).await;
            }
            EngineCommand::StopNetworking => {
                self.stop_networking().await;
            }
            EngineCommand::JoinSession { session_id } => {
                self.join_session(session_id).await;
            }
            EngineCommand::LeaveSession => {
                self.stop_networking().await;
            }
            EngineCommand::SaveSession => {
                // TODO: Save current session state
                tracing::debug!("SaveSession command received (stub)");
            }
            EngineCommand::LoadSession { session_id } => {
                tracing::debug!("LoadSession command received for {} (stub)", session_id.to_code());
            }
            EngineCommand::TickClock => {
                self.tick_clock();
            }
            // TODO: Handle CRDT and lock commands in Phase 2
            _ => {
                tracing::debug!("Unhandled command: {:?}", cmd);
            }
        }
    }

    fn tick_clock(&mut self) {
        let seq = self.clock.increment(self.node_id);
        let _ = self.handle.event_tx.send(EngineEvent::ClockTicked {
            sequence: seq,
            clock: self.clock.clone(),
        });
        tracing::debug!("Clock ticked to {}", seq);
    }

    async fn start_networking(&mut self, session_id: SessionId) {
        if self.networking_task.is_some() {
            tracing::warn!("Networking already started");
            return;
        }

        tracing::info!("Starting networking initialization for session {}", session_id.to_code());

        // Create cancellation token for graceful shutdown
        let cancel_token = CancellationToken::new();
        let cancel_token_clone = cancel_token.clone();

        // Spawn NetworkingManager initialization in background to avoid blocking
        // DHT peer discovery can take 15+ seconds with retries
        let event_tx = self.handle.event_tx.clone();
        
        // Create channel for progress updates
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel();
        
        // Spawn task to forward progress updates to Bevy
        let event_tx_clone = event_tx.clone();
        let session_id_clone = session_id.clone();
        tokio::spawn(async move {
            while let Some(status) = progress_rx.recv().await {
                let _ = event_tx_clone.send(EngineEvent::NetworkingInitializing {
                    session_id: session_id_clone.clone(),
                    status,
                });
            }
        });
        
        let task = tokio::spawn(async move {
            match NetworkingManager::new(session_id.clone(), Some(progress_tx), cancel_token_clone.clone()).await {
                Ok((net_manager, bridge)) => {
                    let node_id = net_manager.node_id();

                    // Notify Bevy that networking started
                    let _ = event_tx.send(EngineEvent::NetworkingStarted {
                        session_id: session_id.clone(),
                        node_id,
                        bridge,
                    });
                    tracing::info!("Networking started for session {}", session_id.to_code());

                    // Run the networking manager loop with cancellation support
                    net_manager.run(event_tx.clone(), cancel_token_clone).await;
                }
                Err(e) => {
                    let _ = event_tx.send(EngineEvent::NetworkingFailed {
                        error: e.to_string(),
                    });
                    tracing::error!("Failed to start networking: {}", e);
                }
            }
        });

        self.networking_task = Some(task);
        self.networking_cancel_token = Some(cancel_token);
    }

    async fn stop_networking(&mut self) {
        // Cancel the task gracefully
        if let Some(cancel_token) = self.networking_cancel_token.take() {
            cancel_token.cancel();
            tracing::info!("Networking cancellation requested");
        }

        // Abort the task immediately - don't wait for graceful shutdown
        // This is fine because NetworkingManager doesn't hold critical resources
        if let Some(task) = self.networking_task.take() {
            task.abort();
            tracing::info!("Networking task aborted");
            let _ = self.handle.event_tx.send(EngineEvent::NetworkingStopped);
        }
    }

    async fn join_session(&mut self, session_id: SessionId) {
        // Stop existing networking if any
        if self.networking_task.is_some() {
            self.stop_networking().await;
        }

        // Start networking with new session
        self.start_networking(session_id).await;
    }
}
