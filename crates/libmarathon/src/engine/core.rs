//! Core Engine event loop - runs on tokio outside Bevy

use tokio::task::JoinHandle;
use uuid::Uuid;

use super::{EngineCommand, EngineEvent, EngineHandle, NetworkingManager, PersistenceManager};
use crate::networking::{SessionId, VectorClock};

pub struct EngineCore {
    handle: EngineHandle,
    networking_task: Option<JoinHandle<()>>,
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

        match NetworkingManager::new(session_id.clone()).await {
            Ok(net_manager) => {
                let node_id = net_manager.node_id();

                // Spawn NetworkingManager in background task
                let event_tx = self.handle.event_tx.clone();
                let task = tokio::spawn(async move {
                    net_manager.run(event_tx).await;
                });

                self.networking_task = Some(task);

                let _ = self.handle.event_tx.send(EngineEvent::NetworkingStarted {
                    session_id: session_id.clone(),
                    node_id,
                });
                tracing::info!("Networking started for session {}", session_id.to_code());
            }
            Err(e) => {
                let _ = self.handle.event_tx.send(EngineEvent::NetworkingFailed {
                    error: e.to_string(),
                });
                tracing::error!("Failed to start networking: {}", e);
            }
        }
    }

    async fn stop_networking(&mut self) {
        if let Some(task) = self.networking_task.take() {
            task.abort(); // Cancel the networking task
            let _ = self.handle.event_tx.send(EngineEvent::NetworkingStopped);
            tracing::info!("Networking stopped");
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
