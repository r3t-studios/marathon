use std::fmt;

///! Session identification and lifecycle management
///!
///! This module provides session-scoped collaborative sessions with
/// human-readable ! session codes, ALPN-based network isolation, and persistent
/// session tracking.
use bevy::prelude::*;
use uuid::Uuid;

use crate::networking::VectorClock;

/// Session identifier - UUID internally, human-readable code for display
///
/// Session IDs provide both technical uniqueness (UUID) and human usability
/// (abc-def-123 codes). All peers in a session share the same session ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct SessionId {
    uuid: Uuid,
    code: String,
}

impl SessionId {
    /// Create a new random session ID
    pub fn new() -> Self {
        // Generate a random 9-character code
        use rand::Rng;
        const CHARSET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
        let mut rng = rand::thread_rng();

        let mut code = String::with_capacity(11);
        for i in 0..9 {
            let idx = rng.gen_range(0..CHARSET.len());
            code.push(CHARSET[idx] as char);
            if i == 2 || i == 5 {
                code.push('-');
            }
        }

        // Hash the code to get a UUID
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"/app/v1/session-code/");
        hasher.update(code.as_bytes());
        let hash = hasher.finalize();

        let mut uuid_bytes = [0u8; 16];
        uuid_bytes.copy_from_slice(&hash.as_bytes()[..16]);
        let uuid = Uuid::from_bytes(uuid_bytes);

        Self { uuid, code }
    }

    /// Parse a session code (format: abc-def-123)
    ///
    /// Hashes the code to derive a deterministic UUID.
    /// Returns error if code format is invalid.
    pub fn from_code(code: &str) -> Result<Self, SessionError> {
        // Validate format: xxx-yyy-zzz (11 chars total: 3 + dash + 3 + dash + 3)
        if code.len() != 11 {
            return Err(SessionError::InvalidCodeFormat);
        }

        // Check dashes at positions 3 and 7
        let chars: Vec<char> = code.chars().collect();
        if chars.len() != 11 || chars[3] != '-' || chars[7] != '-' {
            return Err(SessionError::InvalidCodeFormat);
        }

        // Validate all characters are in the charset
        const CHARSET: &str = "abcdefghjkmnpqrstuvwxyz23456789-";
        let code_lower = code.to_lowercase();
        if !code_lower.chars().all(|c| CHARSET.contains(c)) {
            return Err(SessionError::InvalidCodeFormat);
        }

        // Hash the code to get a UUID (deterministic)
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"/app/v1/session-code/");
        hasher.update(code_lower.as_bytes());
        let hash = hasher.finalize();

        let mut uuid_bytes = [0u8; 16];
        uuid_bytes.copy_from_slice(&hash.as_bytes()[..16]);
        let uuid = Uuid::from_bytes(uuid_bytes);

        Ok(Self {
            uuid,
            code: code_lower,
        })
    }

    /// Convert to human-readable code (abc-def-123 format)
    pub fn to_code(&self) -> &str {
        &self.code
    }

    /// Derive ALPN identifier for network isolation
    ///
    /// Computes deterministic 32-byte BLAKE3 hash from session UUID.
    /// All peers independently compute the same ALPN from session code.
    ///
    /// # Security
    /// The domain separation prefix (`/app/v1/session-id/`) ensures ALPNs
    /// cannot collide with other protocol uses of the same hash space.
    pub fn to_alpn(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"/app/v1/session-id/");
        hasher.update(self.uuid.as_bytes());

        let hash = hasher.finalize();
        *hash.as_bytes()
    }

    /// Derive deterministic pkarr keypair for DHT-based peer discovery
    ///
    /// All peers in the same session derive the same keypair from the session
    /// code. This shared keypair is used to publish and discover peer
    /// EndpointIds in the DHT.
    ///
    /// # Security
    /// The session code is the secret - anyone with the code can discover
    /// peers. The domain separation prefix ensures no collision with other
    /// uses.
    pub fn to_pkarr_keypair(&self) -> pkarr::Keypair {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"/app/v1/session-pkarr-key/");
        hasher.update(self.uuid.as_bytes());
        let hash = hasher.finalize();

        let secret_bytes: [u8; 32] = *hash.as_bytes();
        pkarr::Keypair::from_secret_key(&secret_bytes)
    }

    /// Get raw UUID
    pub fn as_uuid(&self) -> &Uuid {
        &self.uuid
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.code)
    }
}

/// Session lifecycle states
#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum SessionState {
    /// Session exists in database but hasn't connected to network yet
    Created,
    /// Currently attempting to join network and sync state
    Joining,
    /// Fully synchronized and actively collaborating
    Active,
    /// Temporarily offline, will attempt to rejoin when network restored
    Disconnected,
    /// User explicitly left the session (clean shutdown)
    Left,
}

impl fmt::Display for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            | SessionState::Created => write!(f, "created"),
            | SessionState::Joining => write!(f, "joining"),
            | SessionState::Active => write!(f, "active"),
            | SessionState::Disconnected => write!(f, "disconnected"),
            | SessionState::Left => write!(f, "left"),
        }
    }
}

impl SessionState {
    /// Parse from string representation
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            | "created" => Some(SessionState::Created),
            | "joining" => Some(SessionState::Joining),
            | "active" => Some(SessionState::Active),
            | "disconnected" => Some(SessionState::Disconnected),
            | "left" => Some(SessionState::Left),
            | _ => None,
        }
    }
}

/// Session metadata
///
/// Tracks session identity, creation time, entity count, and lifecycle state.
/// Persisted to database for crash recovery and auto-rejoin.
#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct Session {
    /// Unique session identifier
    pub id: SessionId,

    /// Optional human-readable name
    pub name: Option<String>,

    /// When the session was created (Unix timestamp)
    pub created_at: i64,

    /// When this node was last active in the session (Unix timestamp)
    pub last_active: i64,

    /// Cached count of entities in this session
    pub entity_count: usize,

    /// Current lifecycle state
    pub state: SessionState,

    /// Optional encrypted session secret for access control
    pub secret: Option<bytes::Bytes>,
}

impl Session {
    /// Create a new session with default values
    pub fn new(id: SessionId) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id,
            name: None,
            created_at: now,
            last_active: now,
            entity_count: 0,
            state: SessionState::Created,
            secret: None,
        }
    }

    /// Update the last active timestamp to current time
    pub fn touch(&mut self) {
        self.last_active = chrono::Utc::now().timestamp();
    }

    /// Transition to a new state and update last active time
    pub fn transition_to(&mut self, new_state: SessionState) {
        tracing::info!(
            "Session {} transitioning: {:?} -> {:?}",
            self.id,
            self.state,
            new_state
        );
        self.state = new_state;
        self.touch();
    }
}

/// Current session resource for Bevy ECS
///
/// Contains both session metadata and the vector clock snapshot from when
/// we joined (for hybrid sync protocol).
#[derive(Resource, Clone)]
pub struct CurrentSession {
    /// Session metadata
    pub session: Session,

    /// Vector clock when we last left/joined this session
    /// Used for hybrid sync to request only missing deltas
    pub last_known_clock: VectorClock,
}

impl CurrentSession {
    /// Create a new current session
    pub fn new(session: Session, last_known_clock: VectorClock) -> Self {
        Self {
            session,
            last_known_clock,
        }
    }

    /// Transition the session to a new state
    pub fn transition_to(&mut self, new_state: SessionState) {
        self.session.transition_to(new_state);
    }
}

/// Session-related errors
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Invalid session code format (expected: abc-def-123)")]
    InvalidCodeFormat,

    #[error("Session not found")]
    NotFound,

    #[error("Database error: {0}")]
    Database(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_creation() {
        let id1 = SessionId::new();
        let id2 = SessionId::new();

        // Different session IDs should be different
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_session_code_roundtrip() {
        let id = SessionId::new();
        let code = id.to_code();

        // Code should be 11 characters: xxx-yyy-zzz
        assert_eq!(code.len(), 11);
        assert_eq!(&code[3..4], "-");
        assert_eq!(&code[7..8], "-");

        // Parse back
        let parsed = SessionId::from_code(&code).expect("Failed to parse code");

        // Should get same session ID
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_session_code_deterministic() {
        // Same code should always produce same SessionId
        let code = "abc-def-234";
        let id1 = SessionId::from_code(code).unwrap();
        let id2 = SessionId::from_code(code).unwrap();

        assert_eq!(id1, id2);
    }

    #[test]
    fn test_session_code_case_insensitive() {
        // Codes should be case-insensitive
        let id1 = SessionId::from_code("abc-def-234").unwrap();
        let id2 = SessionId::from_code("ABC-DEF-234").unwrap();

        assert_eq!(id1, id2);
    }

    #[test]
    fn test_session_code_invalid_format() {
        // Too short
        assert!(SessionId::from_code("abc-def").is_err());

        // Too long
        assert!(SessionId::from_code("abc-def-1234").is_err());

        // Missing dash
        assert!(SessionId::from_code("abcdef-123").is_err());
        assert!(SessionId::from_code("abc-def123").is_err());

        // Wrong dash positions
        assert!(SessionId::from_code("ab-cdef-123").is_err());
    }

    #[test]
    fn test_alpn_derivation_deterministic() {
        // Same session ID should always produce same ALPN
        let id = SessionId::new();
        let alpn1 = id.to_alpn();
        let alpn2 = id.to_alpn();

        assert_eq!(alpn1, alpn2);
    }

    #[test]
    fn test_alpn_derivation_unique() {
        // Different session IDs should produce different ALPNs
        let id1 = SessionId::new();
        let id2 = SessionId::new();

        let alpn1 = id1.to_alpn();
        let alpn2 = id2.to_alpn();

        assert_ne!(alpn1, alpn2);
    }

    #[test]
    fn test_alpn_length() {
        // ALPN should always be 32 bytes
        let id = SessionId::new();
        let alpn = id.to_alpn();
        assert_eq!(alpn.len(), 32);
    }

    #[test]
    fn test_session_state_display() {
        assert_eq!(SessionState::Created.to_string(), "created");
        assert_eq!(SessionState::Joining.to_string(), "joining");
        assert_eq!(SessionState::Active.to_string(), "active");
        assert_eq!(SessionState::Disconnected.to_string(), "disconnected");
        assert_eq!(SessionState::Left.to_string(), "left");
    }

    #[test]
    fn test_session_state_from_str() {
        assert_eq!(
            SessionState::from_str("created"),
            Some(SessionState::Created)
        );
        assert_eq!(
            SessionState::from_str("joining"),
            Some(SessionState::Joining)
        );
        assert_eq!(SessionState::from_str("active"), Some(SessionState::Active));
        assert_eq!(
            SessionState::from_str("disconnected"),
            Some(SessionState::Disconnected)
        );
        assert_eq!(SessionState::from_str("left"), Some(SessionState::Left));
        assert_eq!(SessionState::from_str("invalid"), None);
    }

    #[test]
    fn test_session_creation() {
        let id = SessionId::new();
        let session = Session::new(id.clone());

        assert_eq!(session.id, id);
        assert_eq!(session.name, None);
        assert_eq!(session.entity_count, 0);
        assert_eq!(session.state, SessionState::Created);
        assert_eq!(session.secret, None);
        assert!(session.created_at > 0);
        assert_eq!(session.created_at, session.last_active);
    }

    #[test]
    fn test_session_transition() {
        let id = SessionId::new();
        let mut session = Session::new(id);

        let initial_state = session.state;
        let initial_time = session.last_active;

        session.transition_to(SessionState::Joining);

        assert_ne!(session.state, initial_state);
        assert_eq!(session.state, SessionState::Joining);
        // Timestamp should be updated (greater or equal due to precision)
        assert!(session.last_active >= initial_time);
    }

    #[test]
    fn test_session_display() {
        let id = SessionId::new();
        let code = id.to_code();
        let display = format!("{}", id);

        assert_eq!(code, &display);
    }

    #[test]
    fn test_current_session_creation() {
        let id = SessionId::new();
        let session = Session::new(id);
        let clock = VectorClock::new();

        let current = CurrentSession::new(session.clone(), clock);

        assert_eq!(current.session.id, session.id);
        assert_eq!(current.session.state, SessionState::Created);
    }

    #[test]
    fn test_current_session_transition() {
        let id = SessionId::new();
        let session = Session::new(id);
        let clock = VectorClock::new();

        let mut current = CurrentSession::new(session, clock);
        current.transition_to(SessionState::Active);

        assert_eq!(current.session.state, SessionState::Active);
    }
}
