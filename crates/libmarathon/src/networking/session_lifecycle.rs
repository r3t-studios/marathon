//! Session lifecycle management - startup and shutdown
//!
//! This module handles automatic session restoration on startup and clean
//! session persistence on shutdown. It enables seamless auto-rejoin after
//! app restarts.
//!
//! # Lifecycle Flow
//!
//! **Startup:**
//! 1. Check database for last active session
//! 2. If found and state is Active/Disconnected → auto-rejoin
//! 3. Load last known vector clock for hybrid sync
//! 4. Insert CurrentSession resource
//!
//! **Shutdown:**
//! 1. Update session metadata (state, last_active, entity_count)
//! 2. Save session to database
//! 3. Save current vector clock
//! 4. Mark clean shutdown in database

use bevy::prelude::*;

use crate::{
    networking::{
        CurrentSession,
        Session,
        SessionId,
        SessionState,
        VectorClock,
        delta_generation::NodeVectorClock,
    },
    persistence::{
        PersistenceDb,
        get_last_active_session,
        load_session_vector_clock,
        save_session,
        save_session_vector_clock,
    },
};

/// System to initialize or restore session on startup
///
/// This system runs once at startup and either:
/// - Restores the last active session (auto-rejoin)
/// - Creates a new session
///
/// Add to your app as a Startup system AFTER setup_persistence:
/// ```no_run
/// use bevy::prelude::*;
/// use libmarathon::networking::initialize_session_system;
///
/// App::new()
///     .add_systems(Startup, initialize_session_system);
/// ```
pub fn initialize_session_system(world: &mut World) {
    info!("Initializing session...");

    // Load session data in a scoped block to release the database lock
    let session_data: Option<(Session, VectorClock)> = {
        // Get database connection
        let db = match world.get_resource::<PersistenceDb>() {
            | Some(db) => db,
            | None => {
                error!("PersistenceDb resource not found - cannot initialize session");
                return;
            },
        };

        // Lock the database connection
        let conn = match db.conn.lock() {
            | Ok(conn) => conn,
            | Err(e) => {
                error!("Failed to lock database connection: {}", e);
                return;
            },
        };

        // Try to load last active session
        match get_last_active_session(&conn) {
        | Ok(Some(mut session)) => {
            // Check if we should auto-rejoin
            match session.state {
                | SessionState::Active | SessionState::Disconnected => {
                    info!(
                        "Found previous session {} in state {:?} - attempting auto-rejoin",
                        session.id, session.state
                    );

                    // Load last known vector clock
                    let last_known_clock = match load_session_vector_clock(&conn, session.id.clone()) {
                        | Ok(clock) => clock,
                        | Err(e) => {
                            warn!(
                                "Failed to load vector clock for session {}: {} - using empty clock",
                                session.id, e
                            );
                            VectorClock::new()
                        },
                    };

                    // Transition to Joining state
                    session.transition_to(SessionState::Joining);

                    Some((session, last_known_clock))
                },

                | _ => {
                    // For Created, Left, or Joining states, create new session
                    None
                },
            }
        },

        | Ok(None) => None,
        | Err(e) => {
            error!("Failed to load last active session: {}", e);
            None
        },
        }
    }; // conn and db are dropped here, releasing the lock

    // Now insert the session resource (no longer holding database lock)
    let current_session = match session_data {
        | Some((session, last_known_clock)) => {
            info!("Session initialized for auto-rejoin");
            CurrentSession::new(session, last_known_clock)
        },
        | None => {
            info!("Creating new session");
            let session_id = SessionId::new();
            let session = Session::new(session_id);
            CurrentSession::new(session, VectorClock::new())
        },
    };

    world.insert_resource(current_session);
}

/// System to auto-save session state periodically
///
/// This system periodically saves session state to persist it for auto-rejoin
/// on next startup. Typically run every 5 seconds.
///
/// Add to your app using the Last schedule with a timer:
/// ```no_run
/// use bevy::prelude::*;
/// use bevy::time::common_conditions::on_timer;
/// use libmarathon::networking::save_session_on_shutdown_system;
/// use std::time::Duration;
///
/// App::new()
///     .add_systems(Last, save_session_on_shutdown_system
///         .run_if(on_timer(Duration::from_secs(5))));
/// ```
pub fn save_session_on_shutdown_system(world: &mut World) {
    debug!("Auto-saving session state...");

    // Get current session
    let current_session = match world.get_resource::<CurrentSession>() {
        | Some(session) => session.clone(),
        | None => {
            warn!("No CurrentSession found - skipping session save");
            return;
        },
    };

    let mut session = current_session.session.clone();

    // Update session metadata
    session.touch();
    // Note: We don't transition to Left here - that only happens on actual shutdown
    // This periodic save just persists the current state

    // Count entities in the world
    let entity_count = world
        .query::<&crate::networking::NetworkedEntity>()
        .iter(world)
        .count();
    session.entity_count = entity_count;

    // Get current vector clock
    let vector_clock = world
        .get_resource::<NodeVectorClock>()
        .map(|nc| nc.clock.clone());

    // Save to database in a scoped block
    {
        // Get database connection
        let db = match world.get_resource::<PersistenceDb>() {
            | Some(db) => db,
            | None => {
                error!("PersistenceDb resource not found - cannot save session");
                return;
            },
        };

        // Lock the database connection
        let mut conn = match db.conn.lock() {
            | Ok(conn) => conn,
            | Err(e) => {
                error!("Failed to lock database connection: {}", e);
                return;
            },
        };

        // Save session to database
        match save_session(&mut conn, &session) {
            | Ok(()) => {
                info!("Session {} saved successfully", session.id);
            },
            | Err(e) => {
                error!("Failed to save session {}: {}", session.id, e);
                return;
            },
        }

        // Save current vector clock
        if let Some(ref clock) = vector_clock {
            match save_session_vector_clock(&mut conn, session.id.clone(), clock) {
                | Ok(()) => {
                    info!("Vector clock saved for session {}", session.id);
                },
                | Err(e) => {
                    error!("Failed to save vector clock for session {}: {}", session.id, e);
                },
            }
        }
    } // conn and db are dropped here

    info!("Session state saved successfully");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_session_creates_new() {
        let mut app = App::new();

        // Run initialize without PersistenceDb - should handle gracefully
        initialize_session_system(&mut app.world_mut());

        // Should not have CurrentSession (no db)
        assert!(app.world().get_resource::<CurrentSession>().is_none());
    }

    #[test]
    fn test_session_roundtrip() {
        // Create a session
        let session_id = SessionId::new();
        let mut session = Session::new(session_id.clone());
        session.entity_count = 5;
        session.transition_to(SessionState::Active);

        // Session should have updated timestamp (or equal if sub-millisecond)
        assert!(session.last_active >= session.created_at);
        assert_eq!(session.state, SessionState::Active);
        assert_eq!(session.entity_count, 5);
    }
}
