//! Bevy plugin for polling engine events and dispatching them
//!
//! This plugin bridges the gap between the tokio-based engine and Bevy's ECS.
//! It polls events from the EngineBridge every frame and dispatches them to
//! Bevy systems.

use bevy::prelude::*;
use libmarathon::{
    engine::{EngineBridge, EngineCommand, EngineEvent},
    networking::{CurrentSession, NetworkedEntity, NodeVectorClock, Session, SessionState, VectorClock},
};

pub struct EngineBridgePlugin;

impl Plugin for EngineBridgePlugin {
    fn build(&self, app: &mut App) {
        // Add the event polling system - runs every tick in Update
        app.add_systems(Update, poll_engine_events);
        // Detect changes and send clock tick commands to engine
        app.add_systems(PostUpdate, detect_changes_and_tick);
    }
}

/// Detect changes to networked entities and send tick commands to engine
///
/// Uses Bevy's change detection to detect when Transform changes on any
/// NetworkedEntity. When changes are detected, sends a TickClock command
/// to the engine, which will increment its clock and send back a ClockTicked event.
fn detect_changes_and_tick(
    bridge: Res<EngineBridge>,
    changed_query: Query<(), (With<NetworkedEntity>, Changed<Transform>)>,
) {
    // If any networked transforms changed this frame, tick the clock
    if !changed_query.is_empty() {
        bridge.send_command(EngineCommand::TickClock);
    }
}

/// Poll events from the engine and dispatch to Bevy
///
/// This system runs every tick and:
/// 1. Polls all available events from the EngineBridge
/// 2. Dispatches them to update Bevy resources and state
fn poll_engine_events(
    mut commands: Commands,
    bridge: Res<EngineBridge>,
    mut current_session: Option<ResMut<CurrentSession>>,
    mut node_clock: ResMut<NodeVectorClock>,
) {
    let events = (*bridge).poll_events();

    if !events.is_empty() {
        for event in events {
            match event {
                EngineEvent::NetworkingStarted { session_id, node_id } => {
                    info!("🌐 Networking started: session={}, node={}",
                        session_id.to_code(), node_id);

                    // Create session if it doesn't exist
                    if current_session.is_none() {
                        let mut session = Session::new(session_id.clone());
                        session.state = SessionState::Active;
                        commands.insert_resource(CurrentSession::new(session, VectorClock::new()));
                        info!("Created new session resource: {}", session_id.to_code());
                    } else if let Some(ref mut session) = current_session {
                        // Update existing session state to Active
                        session.session.state = SessionState::Active;
                    }

                    // Update node ID in clock
                    node_clock.node_id = node_id;
                }
                EngineEvent::NetworkingFailed { error } => {
                    error!("❌ Networking failed: {}", error);

                    // Keep session state as Created (if session exists)
                    if let Some(ref mut session) = current_session {
                        session.session.state = SessionState::Created;
                    }
                }
                EngineEvent::NetworkingStopped => {
                    info!("🔌 Networking stopped");

                    // Update session state to Disconnected (if session exists)
                    if let Some(ref mut session) = current_session {
                        session.session.state = SessionState::Disconnected;
                    }
                }
                EngineEvent::PeerJoined { node_id } => {
                    info!("👋 Peer joined: {}", node_id);
                    // TODO(Phase 3.3): Trigger sync
                }
                EngineEvent::PeerLeft { node_id } => {
                    info!("👋 Peer left: {}", node_id);
                }
                EngineEvent::LockAcquired { entity_id, holder } => {
                    debug!("🔒 Lock acquired: entity={}, holder={}", entity_id, holder);
                    // TODO(Phase 3.4): Update lock visuals
                }
                EngineEvent::LockReleased { entity_id } => {
                    debug!("🔓 Lock released: entity={}", entity_id);
                    // TODO(Phase 3.4): Update lock visuals
                }
                EngineEvent::LockDenied { entity_id, current_holder } => {
                    debug!("⛔ Lock denied: entity={}, holder={}", entity_id, current_holder);
                    // TODO(Phase 3.4): Show visual feedback
                }
                EngineEvent::LockExpired { entity_id } => {
                    debug!("⏰ Lock expired: entity={}", entity_id);
                    // TODO(Phase 3.4): Update lock visuals
                }
                EngineEvent::ClockTicked { sequence, clock } => {
                    debug!("🕐 Clock ticked to {}", sequence);

                    // Update the NodeVectorClock resource with the new clock state
                    node_clock.clock = clock;
                }
                _ => {
                    debug!("Unhandled engine event: {:?}", event);
                }
            }
        }
    }
}
