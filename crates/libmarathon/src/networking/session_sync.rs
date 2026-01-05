//! Session synchronization systems
//!
//! This module handles automatic session lifecycle:
//! - Sending JoinRequest when networking starts
//! - Transitioning session state when receiving FullState
//! - Persisting session state changes

use bevy::prelude::*;

use crate::networking::{
    CurrentSession,
    GossipBridge,
    JoinType,
    NodeVectorClock,
    SessionState,
    build_join_request,
    plugin::SessionSecret,
};

/// System to send JoinRequest when networking comes online
///
/// This system detects when GossipBridge is added and sends a JoinRequest
/// to discover peers and sync state. It only runs once when networking starts.
///
/// Add to your app as a Startup system AFTER GossipBridge is created:
/// ```no_run
/// use bevy::prelude::*;
/// use libmarathon::networking::send_join_request_on_connect_system;
///
/// App::new()
///     .add_systems(Update, send_join_request_on_connect_system);
/// ```
pub fn send_join_request_on_connect_system(
    current_session: ResMut<CurrentSession>,
    node_clock: Res<NodeVectorClock>,
    bridge: Option<Res<GossipBridge>>,
    session_secret: Option<Res<SessionSecret>>,
) {
    // Only run when bridge exists and session is in Joining state
    let Some(bridge) = bridge else {
        return;
    };

    // Only send JoinRequest when in Joining state
    if current_session.session.state != SessionState::Joining {
        return;
    }

    let node_id = node_clock.node_id;
    let session_id = current_session.session.id.clone();

    // Determine join type based on whether we have a last known clock
    let join_type = if current_session.last_known_clock.node_count() > 0 {
        // Rejoin - we have a previous clock snapshot
        JoinType::Rejoin {
            last_active: current_session.session.last_active,
            entity_count: current_session.session.entity_count,
        }
    } else {
        // Fresh join - no previous state
        JoinType::Fresh
    };

    // Get session secret if configured
    let secret = session_secret.as_ref().map(|s| bytes::Bytes::from(s.as_bytes().to_vec()));

    // Build JoinRequest
    let last_known_clock = if current_session.last_known_clock.node_count() > 0 {
        Some(current_session.last_known_clock.clone())
    } else {
        None
    };

    let request = build_join_request(
        node_id,
        session_id.clone(),
        secret,
        last_known_clock,
        join_type.clone(),
    );

    // Send JoinRequest
    match bridge.send(request) {
        Ok(()) => {
            info!(
                "Sent JoinRequest for session {} (type: {:?})",
                session_id.to_code(),
                join_type
            );
        }
        Err(e) => {
            error!("Failed to send JoinRequest: {}", e);
        }
    }
}

/// System to transition session to Active when sync completes
///
/// This system monitors for session state changes and handles transitions:
/// - Joining → Active: When we receive FullState or initial sync completes
///
/// This is an exclusive system to allow world queries.
///
/// Add to your app as an Update system:
/// ```no_run
/// use bevy::prelude::*;
/// use libmarathon::networking::transition_session_state_system;
///
/// App::new()
///     .add_systems(Update, transition_session_state_system);
/// ```
pub fn transition_session_state_system(world: &mut World) {
    // Only process state transitions when we have networking
    if world.get_resource::<GossipBridge>().is_none() {
        return;
    }

    // Get values we need (clone to avoid holding references)
    let (session_state, session_id, join_request_sent, clock_node_count) = {
        let Some(current_session) = world.get_resource::<CurrentSession>() else {
            return;
        };

        let Some(node_clock) = world.get_resource::<NodeVectorClock>() else {
            return;
        };

        let Some(join_sent) = world.get_resource::<JoinRequestSent>() else {
            return;
        };

        (
            current_session.session.state,
            current_session.session.id.clone(),
            join_sent.sent,
            node_clock.clock.node_count(),
        )
    };

    // Use a non-send resource for the timer to ensure it's stored per-world
    #[derive(Default, Resource)]
    struct JoinTimer(Option<std::time::Instant>);

    if !world.contains_resource::<JoinTimer>() {
        world.insert_resource(JoinTimer::default());
    }

    match session_state {
        SessionState::Joining => {
            // Start timer when JoinRequest is sent
            {
                let mut timer = world.resource_mut::<JoinTimer>();
                if join_request_sent && timer.0.is_none() {
                    timer.0 = Some(std::time::Instant::now());
                    debug!("Started join timer - will transition to Active after timeout if no peers respond");
                }
            }

            // Count entities in world
            let entity_count = world
                .query::<&crate::networking::NetworkedEntity>()
                .iter(world)
                .count();

            // Transition to Active if:
            // 1. We have received entities (entity_count > 0) AND have multiple nodes in clock
            //    This ensures FullState was received and applied, OR
            // 2. We've waited 3 seconds and either:
            //    a) We have entities (sync completed), OR
            //    b) No entities exist yet (we're the first node in session)
            let should_transition = if entity_count > 0 && clock_node_count > 1 {
                // We've received and applied FullState with entities
                info!(
                    "Session {} transitioning to Active (received {} entities from {} peers)",
                    session_id.to_code(),
                    entity_count,
                    clock_node_count - 1
                );
                true
            } else {
                let timer = world.resource::<JoinTimer>();
                if let Some(start_time) = timer.0 {
                    // Check if 3 seconds have passed since JoinRequest
                    if start_time.elapsed().as_secs() >= 3 {
                        if entity_count > 0 {
                            info!(
                                "Session {} transitioning to Active (timeout reached, have {} entities)",
                                session_id.to_code(),
                                entity_count
                            );
                        } else {
                            info!(
                                "Session {} transitioning to Active (timeout - no peers or empty session, first node)",
                                session_id.to_code()
                            );
                        }
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            };

            if should_transition {
                let mut current_session = world.resource_mut::<CurrentSession>();
                current_session.transition_to(SessionState::Active);

                // Reset timer
                let mut timer = world.resource_mut::<JoinTimer>();
                timer.0 = None;
            }
        }
        SessionState::Active => {
            // Already active, reset timer
            if let Some(mut timer) = world.get_resource_mut::<JoinTimer>() {
                timer.0 = None;
            }
        }
        SessionState::Disconnected => {
            // If we reconnected (bridge exists), transition to Joining
            // This is handled by the networking startup logic
            if let Some(mut timer) = world.get_resource_mut::<JoinTimer>() {
                timer.0 = None;
            }
        }
        SessionState::Created | SessionState::Left => {
            // Should not be in these states when networking is active
            if let Some(mut timer) = world.get_resource_mut::<JoinTimer>() {
                timer.0 = None;
            }
        }
    }
}

/// Resource to track if we've sent the initial JoinRequest
///
/// This prevents sending multiple JoinRequests on subsequent frame updates
#[derive(Resource)]
pub struct JoinRequestSent {
    pub sent: bool,
    /// Timer to wait for peers before sending JoinRequest
    /// If no peers connect after 1 second, send anyway (we're first node)
    pub wait_started: Option<std::time::Instant>,
}

impl Default for JoinRequestSent {
    fn default() -> Self {
        Self {
            sent: false,
            wait_started: None,
        }
    }
}

/// One-shot system to send JoinRequest only once when networking starts
///
/// CRITICAL: Waits for at least one peer to connect via pkarr+DHT before sending
/// JoinRequest. This prevents broadcasting to an empty network.
///
/// Timing:
/// - If peers connect: Send JoinRequest immediately (they'll receive it)
/// - If no peers after 1 second: Send anyway (we're probably first node in session)
pub fn send_join_request_once_system(
    mut join_sent: ResMut<JoinRequestSent>,
    current_session: ResMut<CurrentSession>,
    node_clock: Res<NodeVectorClock>,
    bridge: Option<Res<GossipBridge>>,
    session_secret: Option<Res<SessionSecret>>,
) {
    // Skip if already sent
    if join_sent.sent {
        return;
    }

    // Only run when bridge exists and session is in Joining state
    let Some(bridge) = bridge else {
        return;
    };

    if current_session.session.state != SessionState::Joining {
        return;
    }

    // Start wait timer when conditions are met
    if join_sent.wait_started.is_none() {
        join_sent.wait_started = Some(std::time::Instant::now());
        debug!("Started waiting for peers before sending JoinRequest (max 1 second)");
    }

    // Check if we have any peers connected (node_count > 1 means we + at least 1 peer)
    let peer_count = node_clock.clock.node_count().saturating_sub(1);
    let wait_elapsed = join_sent.wait_started.unwrap().elapsed();

    // Send JoinRequest if:
    // 1. At least one peer has connected (they'll receive our JoinRequest), OR
    // 2. We've waited 1 second with no peers (we're probably the first node)
    let should_send = if peer_count > 0 {
        debug!(
            "Sending JoinRequest now - {} peer(s) connected (waited {:?})",
            peer_count, wait_elapsed
        );
        true
    } else if wait_elapsed.as_millis() >= 1000 {
        debug!(
            "Sending JoinRequest after timeout - no peers connected, assuming first node (waited {:?})",
            wait_elapsed
        );
        true
    } else {
        // Still waiting for peers
        false
    };

    if !should_send {
        return;
    }

    let node_id = node_clock.node_id;
    let session_id = current_session.session.id.clone();

    // Determine join type
    let join_type = if current_session.last_known_clock.node_count() > 0 {
        JoinType::Rejoin {
            last_active: current_session.session.last_active,
            entity_count: current_session.session.entity_count,
        }
    } else {
        JoinType::Fresh
    };

    // Get session secret if configured
    let secret = session_secret.as_ref().map(|s| bytes::Bytes::from(s.as_bytes().to_vec()));

    // Build JoinRequest
    let last_known_clock = if current_session.last_known_clock.node_count() > 0 {
        Some(current_session.last_known_clock.clone())
    } else {
        None
    };

    let request = build_join_request(
        node_id,
        session_id.clone(),
        secret,
        last_known_clock,
        join_type.clone(),
    );

    // Send JoinRequest
    match bridge.send(request) {
        Ok(()) => {
            info!(
                "Sent JoinRequest for session {} (type: {:?})",
                session_id.to_code(),
                join_type
            );
            join_sent.sent = true;

            // Transition to Active immediately if we're the first node
            // (Otherwise we'll wait for FullState)
            // Actually, let's always wait a bit for potential peers
        }
        Err(e) => {
            error!("Failed to send JoinRequest: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::networking::{Session, SessionId, VectorClock};

    #[test]
    fn test_join_request_sent_tracking() {
        let mut sent = JoinRequestSent::default();
        assert!(!sent.sent);

        sent.sent = true;
        assert!(sent.sent);
    }

    #[test]
    fn test_session_state_transitions() {
        let session_id = SessionId::new();
        let session = Session::new(session_id);
        let mut current = CurrentSession::new(session, VectorClock::new());

        assert_eq!(current.session.state, SessionState::Created);

        current.transition_to(SessionState::Joining);
        assert_eq!(current.session.state, SessionState::Joining);

        current.transition_to(SessionState::Active);
        assert_eq!(current.session.state, SessionState::Active);
    }
}
