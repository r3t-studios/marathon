//! Bevy plugin for polling engine events and dispatching them
//!
//! This plugin bridges the gap between the tokio-based engine and Bevy's ECS.
//! It polls events from the EngineBridge every frame and dispatches them to
//! Bevy systems.

use bevy::prelude::*;
use libmarathon::{
    engine::{EngineBridge, EngineCommand, EngineEvent},
    networking::{CurrentSession, NetworkedEntity, NodeVectorClock, Session, SessionState},
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
    bridge: Res<EngineBridge>,
    mut current_session: ResMut<CurrentSession>,
    mut node_clock: ResMut<NodeVectorClock>,
) {
    let events = (*bridge).poll_events();

    if !events.is_empty() {
        for event in events {
            match event {
                EngineEvent::NetworkingStarted { session_id, node_id } => {
                    info!("Networking started: session={}, node={}",
                        session_id.to_code(), node_id);

                    // Update session to use the new session ID and set state to Active
                    current_session.session = Session::new(session_id.clone());
                    current_session.session.state = SessionState::Active;
                    info!("Updated CurrentSession to Active: {}", session_id.to_code());

                    // Update node ID in clock
                    node_clock.node_id = node_id;
                }
                EngineEvent::NetworkingFailed { error } => {
                    error!("Networking failed: {}", error);

                    // Keep session state as Created
                    current_session.session.state = SessionState::Created;
                }
                EngineEvent::NetworkingStopped => {
                    info!("Networking stopped");

                    // Update session state to Disconnected
                    current_session.session.state = SessionState::Disconnected;
                }
                EngineEvent::PeerJoined { node_id } => {
                    info!("Peer joined: {}", node_id);
                    // TODO(Phase 3.3): Trigger sync
                }
                EngineEvent::PeerLeft { node_id } => {
                    info!("Peer left: {}", node_id);
                }
                EngineEvent::LockAcquired { entity_id, holder } => {
                    debug!("Lock acquired: entity={}, holder={}", entity_id, holder);
                    // TODO(Phase 3.4): Update lock visuals
                }
                EngineEvent::LockReleased { entity_id } => {
                    debug!("Lock released: entity={}", entity_id);
                    // TODO(Phase 3.4): Update lock visuals
                }
                EngineEvent::ClockTicked { sequence, clock: _ } => {
                    debug!("Clock ticked: sequence={}", sequence);
                    // Clock tick confirmed - no action needed
                }
                EngineEvent::SessionJoined { session_id } => {
                    info!("Session joined: {}", session_id.to_code());
                    // Update session state
                    current_session.session.state = SessionState::Joining;
                }
                EngineEvent::SessionLeft => {
                    info!("Session left");
                    // Update session state
                    current_session.session.state = SessionState::Left;
                }
                EngineEvent::EntitySpawned { entity_id, position, rotation, version: _ } => {
                    debug!("Entity spawned: id={}, pos={:?}, rot={:?}", entity_id, position, rotation);
                    // TODO: Spawn entity in Bevy
                }
                EngineEvent::EntityUpdated { entity_id, position, rotation, version: _ } => {
                    debug!("Entity updated: id={}, pos={:?}, rot={:?}", entity_id, position, rotation);
                    // TODO: Update entity in Bevy
                }
                EngineEvent::EntityDeleted { entity_id, version: _ } => {
                    debug!("Entity deleted: id={}", entity_id);
                    // TODO: Delete entity in Bevy
                }
                EngineEvent::LockDenied { entity_id, current_holder } => {
                    debug!("Lock denied: entity={}, current_holder={}", entity_id, current_holder);
                    // TODO: Show lock denied feedback
                }
                EngineEvent::LockExpired { entity_id } => {
                    debug!("Lock expired: entity={}", entity_id);
                    // TODO: Update lock visuals
                }
            }
        }
    }
}
