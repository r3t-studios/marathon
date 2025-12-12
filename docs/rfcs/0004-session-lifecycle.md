# RFC 0004: Session Lifecycle Management

**Status:** Draft
**Authors:** Sienna
**Created:** 2025-12-11
**Updated:** 2025-12-11

## Abstract

This RFC proposes a session-based lifecycle management system for peer-to-peer collaborative sessions. It introduces explicit session identities, session-scoped network isolation via ALPN, hybrid state restoration combining database persistence with delta synchronization, temporary entity ownership locks to prevent conflicts, and persistent session tracking with automatic rejoin capabilities.

## Motivation

The current architecture supports basic CRDT synchronization across all peers on a global gossip topic, but lacks:

1. **Session Isolation**: All peers share the same global network topic, preventing multiple independent sessions from coexisting
2. **State Management**: No concept of "joining a specific session" vs "creating a new one"
3. **Crash Recovery**: Nodes don't remember which session they were in before shutdown
4. **Entity Conflict Prevention**: Multiple nodes can simultaneously modify the same entity, requiring complex merge logic
5. **Hybrid Sync**: Always sends full state on join, even when rejoining a known session

### Requirements

From user specifications:

1. **Explicit Session IDs** - UUID or human-readable codes that identify unique collaborative sessions (let's do like 6 character abcd123 style so it's easy for humans)
2. **ALPN-based Network Isolation** - Each session gets its own ALPN protocol for gossip isolation (and security)
3. **Hybrid Initial Sync** - Restore from local DB first, then request deltas from peers 
4. **Temporary Lock-based Ownership** - Only one node can modify an entity at a time (prevents conflicts) (but it should be initiator-driven, like i select the cube and move it i don't wait for upstream locking, the lock happens when i "select it" (or whatever))
5. **Persistent Sessions** - Sessions persist to DB and auto-rejoin on restart

## High-Level Architecture

The session lifecycle consists of four major phases: startup, network join, active collaboration, and shutdown. Each phase builds on the previous one to provide seamless session management and state restoration.

```mermaid
flowchart TD
    A[Application Startup] --> B[Load Session from DB]
    B --> C[Restore Entities from DB]
    C --> D[Connect to Gossip with Session ALPN]

    D --> E[Network Join Protocol]
    E --> F[Send JoinRequest + VectorClock]
    F --> G{First Join or Rejoin?}
    G -->|Fresh Join| H[Receive FullState]
    G -->|Rejoin| I[Receive Deltas]
    H --> J[Apply Updates to Local State]
    I --> J

    J --> K[Active Session]
    K --> L[User Interaction]
    L --> M[Acquire Lock]
    M --> N[Modify Entity]
    N --> O[Broadcast Delta]
    O --> P[Release Lock]
    P --> L

    K --> Q{Session Ending?}
    Q -->|Yes| R[Shutdown]
    R --> S[Save Session State to DB]
    S --> T[Mark Clean Shutdown]
    T --> U[Exit]
```

## Session Data Model

The session data model defines how collaborative sessions are identified, tracked, and persisted. It consists of three main components: unique session identifiers, session metadata for tracking state, and database schemas for persistence.

### Session Identification

Each collaborative session needs a globally unique identifier that's both machine-readable and human-friendly. We use UUIDs internally for uniqueness while providing a human-readable "session code" format (like `abc-def-123`) that users can easily share and enter manually.

The `SessionId` type wraps a UUID but provides bidirectional conversion to/from 6-character alphanumeric codes. This code format makes it easy to verbally communicate session IDs ("join session abc-def-one-two-three") or manually type them into a join dialog.

Additionally, each session ID can be deterministically converted to an ALPN (Application-Layer Protocol Negotiation) identifier using BLAKE3 hashing. This ensures that peers in different sessions are cryptographically isolated at the network transport layer - they literally cannot discover or communicate with each other.

```rust
/// Unique identifier for a collaborative session
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct SessionId(Uuid);

impl SessionId {
    /// Create a new random session ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from a human-readable code (e.g., "abc-def-ghi")
    pub fn from_code(code: &str) -> Result<Self> {
        // Parse format: xxx-yyy-zzz (3 groups of 3 lowercase alphanumeric)
        // Maps to deterministic UUID using hash
        let uuid = generate_uuid_from_code(code)?;
        Ok(Self(uuid))
    }

    /// Get human-readable code (first 9 characters of UUID formatted)
    pub fn to_code(&self) -> String {
        format_uuid_as_code(&self.0)
    }

    /// Derive ALPN protocol identifier from session ID
    pub fn to_alpn(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"lonni-session-v1");
        hasher.update(self.0.as_bytes());
        *hasher.finalize().as_bytes()
    }
}
```

### Session Metadata

Beyond the unique identifier, each session needs metadata to track its lifecycle and state. The `Session` struct captures when the session was created, when it was last active, how many entities it contains, and its current state (created, joining, active, disconnected, or left).

This metadata serves several purposes:
- **Crash recovery**: When the app restarts, we can detect incomplete sessions and decide whether to rejoin
- **UI display**: Show users their recent sessions with entity counts and last access times
- **Network isolation**: Optional session secrets provide a basic authentication layer
- **State machine**: The `SessionState` enum tracks where we are in the session lifecycle

The `CurrentSession` resource represents the active session within the Bevy ECS world. It includes both the session metadata and the vector clock state at the time of joining, which is essential for the hybrid sync protocol.

```rust
/// Metadata about a collaborative session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique identifier for this session
    pub id: SessionId,

    /// Human-readable name (optional)
    pub name: Option<String>,

    /// When this session was created
    pub created_at: DateTime<Utc>,

    /// Last time this node was active in this session
    pub last_active: DateTime<Utc>,

    /// How many entities are in this session (cached)
    pub entity_count: usize,

    /// Session state
    pub state: SessionState,

    /// Optional session secret for authentication
    pub secret: Option<Vec<u8>>,
}

/// Current state of a session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    /// Session created but not yet joined network
    Created,

    /// Currently joining (waiting for FullState or deltas)
    Joining,

    /// Fully synchronized and active
    Active,

    /// Temporarily disconnected (will attempt rejoin)
    Disconnected,

    /// Cleanly left (archived, can rejoin later)
    Left,
}

/// Bevy resource tracking the current session
#[derive(Resource)]
pub struct CurrentSession {
    pub session: Session,
    pub vector_clock_at_join: VectorClock,
}
```

### Database Schema

To support persistent sessions, we need to extend the existing database schema with session-aware tables and indexes. The schema changes fall into three categories: session tracking, session membership history, and session-scoping of existing tables.

**Session Tracking**: The `sessions` table stores all session metadata including the session UUID, optional human-readable name, creation and last-active timestamps, entity count cache, current state, and optional encrypted secret. The `idx_sessions_last_active` index enables fast queries for "recent sessions" in the UI.

**Membership History**: The `session_membership` table tracks which nodes have participated in which sessions and when. This provides an audit trail and helps detect cases where a node attempts to rejoin a session it was previously kicked from (future enhancement).

**Session-Scoped Data**: Existing tables (`entities`, `vector_clock`, and `operation_log`) are extended with `session_id` foreign keys. This ensures that:
- Entities are scoped to sessions (prevents accidental cross-session entity leakage)
- Vector clocks are per-session (each session has independent causality tracking)
- Operation logs are per-session (enables efficient delta calculation for rejoins)

The composite indexes ensure that common queries like "get all entities in session X" or "get vector clock for session Y, node Z" remain fast even with thousands of sessions in the database.

```sql
-- Sessions table
CREATE TABLE sessions (
    id BLOB PRIMARY KEY,           -- Session UUID (16 bytes)
    name TEXT,                      -- Optional human-readable name
    created_at INTEGER NOT NULL,   -- Unix timestamp
    last_active INTEGER NOT NULL,  -- Unix timestamp
    entity_count INTEGER NOT NULL DEFAULT 0,
    state TEXT NOT NULL,            -- 'created' | 'joining' | 'active' | 'disconnected' | 'left'
    secret BLOB,                    -- Optional session secret (encrypted)
    UNIQUE(id)
);

-- Index for finding recent sessions
CREATE INDEX idx_sessions_last_active
ON sessions(last_active DESC);

-- Session membership (which node was in which session)
CREATE TABLE session_membership (
    session_id BLOB NOT NULL,
    node_id TEXT NOT NULL,
    joined_at INTEGER NOT NULL,
    left_at INTEGER,                -- NULL if still active
    PRIMARY KEY (session_id, node_id),
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

-- Link entities to sessions
ALTER TABLE entities ADD COLUMN session_id BLOB NOT NULL REFERENCES sessions(id);

-- Index for session-scoped entity queries
CREATE INDEX idx_entities_session
ON entities(session_id);

-- Update vector clock to be session-scoped
ALTER TABLE vector_clock ADD COLUMN session_id BLOB NOT NULL REFERENCES sessions(id);

-- Composite index for session + node lookups
CREATE INDEX idx_vector_clock_session_node
ON vector_clock(session_id, node_id);

-- Update operation log to be session-scoped
ALTER TABLE operation_log ADD COLUMN session_id BLOB NOT NULL REFERENCES sessions(id);

-- Index for session-scoped operation queries
CREATE INDEX idx_operation_log_session
ON operation_log(session_id, node_id, sequence_number);
```

### Session State Transitions

Sessions progress through a well-defined state machine that handles normal operation, network failures, and clean shutdown. The five states capture every phase of a session's lifecycle:

- **Created**: Session exists in database but hasn't connected to the network yet
- **Joining**: Currently attempting to join the network and sync state with peers
- **Active**: Fully synchronized and actively collaborating with peers
- **Disconnected**: Temporarily offline, will attempt to rejoin when network is restored
- **Left**: User explicitly left the session (clean shutdown)

The state machine allows for automatic reconnection after temporary network failures while respecting explicit user actions like leaving a session.

```mermaid
stateDiagram-v2
    [*] --> Created
    Created --> Joining: Connect to network
    Joining --> Active: Sync complete
    Joining --> Disconnected: Network loss
    Active --> Disconnected: Network loss
    Disconnected --> Joining: Network restored
    Disconnected --> Left: Explicit leave
    Active --> Left: Explicit leave
    Left --> [*]
```

## ALPN-based Network Isolation

### Overview

Instead of using a single global gossip topic, each session gets its own ALPN (Application-Layer Protocol Negotiation) identifier derived from the session ID. This provides true network isolation at the QUIC transport layer: peers in different sessions cannot even discover each other. (this is to protect the players)

### ALPN Derivation

Each session derives a unique ALPN identifier using BLAKE3 cryptographic hashing. The derivation is deterministic - the same session ID always produces the same ALPN - which allows all peers to independently compute the correct ALPN for a session they want to join.

```rust
/// Derive a unique ALPN protocol identifier from a session ID
pub fn derive_alpn_from_session(session_id: &SessionId) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"/app/v1/session-id/");  // Domain separation prefix
    hasher.update(session_id.0.as_bytes());
    *hasher.finalize().as_bytes()
}
```

The design provides several security and isolation guarantees:

- **Cryptographic isolation**: BLAKE3's uniform distribution ensures no ALPN collisions between sessions
- **Protocol versioning**: The `/app/v1/` prefix provides domain separation and version control
- **Forward compatibility**: Future protocol versions use different prefixes (`/app/v2/session-id/`, etc.)
- **Deterministic**: All peers joining session `abc-def-123` independently compute the same ALPN
- **Sufficient entropy**: 32-byte (256-bit) output prevents brute-force session discovery

### Modified Gossip Setup

The gossip network initialization is modified to use session-specific ALPNs instead of a global protocol identifier. This requires configuring the iroh endpoint with the session's derived ALPN and ensuring the router only accepts connections using that ALPN.

**Peer Discovery Strategy**: We use a multi-layered approach to discover peers within a session:

1. **mDNS (Multicast DNS)**: For local network discovery - peers on the same LAN can find each other automatically
2. **Pkarr DNS Discovery**: For Internet-wide discovery - iroh's built-in pkarr-based DNS discovery provides decentralized peer discovery without requiring centralized infrastructure. Pkarr (Public Key Addressable Resource Records) allows peers to publish signed DNS records using their public keys.

The combination ensures both local and remote sessions work seamlessly.

```rust
/// Initialize gossip with session-specific ALPN
async fn init_gossip_for_session(session: &Session) -> Result<GossipBridge> {
    info!("Creating endpoint with discovery...");
    let endpoint = Endpoint::builder()
        .discovery(MdnsDiscovery::builder())  // Local network
        .discovery(PkarrDiscovery::builder()) // Internet-wide via pkarr DNS
        .bind()
        .await?;

    let endpoint_id = endpoint.addr().id;
    let node_id = endpoint_id_to_uuid(&endpoint_id);

    info!("Node ID: {}", node_id);
    info!("Session ID: {}", session.id.to_code());

    // Derive session-specific ALPN
    let alpn = session.id.to_alpn();
    info!("Using session ALPN: {}", hex::encode(&alpn[..8]));

    info!("Spawning gossip protocol...");
    let gossip = Gossip::builder().spawn(endpoint.clone());

    // Router accepts connections with this specific ALPN only
    info!("Setting up router with session ALPN...");
    let router = Router::builder(endpoint.clone())
        .accept(alpn, gossip.clone())  // NOTE: Use session ALPN, not iroh_gossip::ALPN
        .spawn();

    // Subscribe to topic (can use session ID directly as topic)
    let topic_id = TopicId::from_bytes(alpn);
    info!("Subscribing to session topic...");
    let subscribe_handle = gossip.subscribe(topic_id, vec![]).await?;

    let (sender, mut receiver) = subscribe_handle.split();

    // Wait for join (with timeout)
    info!("Waiting for gossip join...");
    match tokio::time::timeout(Duration::from_secs(2), receiver.joined()).await {
        Ok(Ok(())) => info!("Joined session gossip swarm"),
        Ok(Err(e)) => warn!("Join error: {} (proceeding anyway)", e),
        Err(_) => info!("Join timeout (first node in session)"),
    }

    // Create bridge with session context
    let bridge = GossipBridge::new_with_session(node_id, session.id.clone());

    // Spawn forwarding tasks
    spawn_bridge_tasks(sender, receiver, bridge.clone(), endpoint, router, gossip);

    Ok(bridge)
}
```

### Session Discovery

How do peers discover which session to join?

**Primary Method: Manual Entry**

Users manually type the session code into a join dialog. This is the primary and most reliable method:
- Simple and foolproof
- Works across all platforms
- No dependency on clipboard, URL handlers, or QR scanning
- Easy to communicate verbally ("join session abc-def-123")

**Secondary Method: Invite Links**

For convenience, shareable links can encode session information:
```
lonni://join/<session-code>/<optional-secret>

Example: lonni://join/abc-def-ghi/dGVzdHNlY3JldA==
```

These links can be shared via chat, email, or other communication channels. When clicked, they auto-populate the join dialog, but manual entry remains the fallback if URL handling isn't configured.

## Join Protocol (Hybrid Sync)

The join protocol is the mechanism by which a node synchronizes its state when connecting to a session. The protocol is "hybrid" because it intelligently chooses between two strategies based on the node's history with the session.

### Overview

When a node connects to a session, it needs to synchronize its local state with the distributed state maintained by all peers. The naive approach would be to always transfer the complete session state - all entities, components, and resources. However, this is inefficient for nodes that are rejoining a session after a temporary disconnection or app restart.

The hybrid join protocol addresses this by supporting two distinct scenarios:

1. **Fresh Join**: The node is joining this session for the first time and has no local state. The protocol sends a complete snapshot of the entire session state (all entities and components). This is unavoidable for first-time joins but can be bandwidth-intensive.

2. **Rejoin**: The node has previously joined this session and has persistent local state in its database. Instead of transferring the entire state again, the protocol calculates which operations occurred since the node was last active and sends only those deltas. This dramatically reduces bandwidth and latency for reconnection scenarios.

### Extended JoinRequest Message

To enable hybrid sync, the `JoinRequest` message needs to communicate the joining node's state to existing peers. This allows peers to make an intelligent decision about whether to send full state or just deltas.

The key fields are:

- **session_id**: Which session the node wants to join (validates this matches the receiving peer's session)
- **session_secret**: Optional authentication credential (if the session is password-protected)
- **last_known_clock**: The vector clock from when the node was last active in this session. If `None`, this is a fresh join. If `Some`, the node has previous state and only needs updates since that clock.
- **join_type**: Metadata about the join (fresh vs rejoin with entity count) - helps peers optimize their response

Existing peers use this information to decide: "Can I send just deltas, or do I need to send the full state?"

```rust
/// Request to join a session
JoinRequest {
    /// ID of the node requesting to join
    node_id: NodeId,

    /// Session ID we're trying to join
    session_id: SessionId,

    /// Optional session secret for authentication
    session_secret: Option<Vec<u8>>,

    /// Our last known vector clock for this session (if rejoining)
    /// None = fresh join (need full state)
    /// Some = rejoin (only need deltas since this clock)
    last_known_clock: Option<VectorClock>,

    /// Are we rejoining or joining fresh?
    join_type: JoinType,
},

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JoinType {
    /// First time joining this session
    Fresh,

    /// Rejoining after disconnect/restart
    Rejoin {
        /// When we last left
        last_active: DateTime<Utc>,

        /// How many entities we had
        entity_count: usize,
    },
}
```

### Join Flow: Fresh Join

When a node joins a session for the first time, it has no local state and needs the complete world snapshot. This flow involves:

1. **Local Setup**: Create the session in the local database
2. **Network Connection**: Connect to the gossip network using the session's unique ALPN
3. **Join Request**: Broadcast a `JoinRequest` with `last_known_clock: None` to indicate this is a fresh join
4. **Peer Response**: An existing peer validates the session ID and builds a complete snapshot of all entities and components
5. **State Transfer**: The peer sends a `FullState` message containing the serialized world
6. **Local Application**: The new node deserializes and spawns all entities into its Bevy world
7. **Persistence**: Save the received state to the local database
8. **Final Sync**: Request any deltas that may have occurred during the transfer
9. **Active**: Transition to the active state

This ensures that fresh joins receive a complete, consistent snapshot of the session state.

```mermaid
sequenceDiagram
    participant NewNode
    participant Database
    participant Network
    participant ExistingPeer

    NewNode->>Database: Create new session
    NewNode->>Network: Connect to gossip (session ALPN)
    NewNode->>ExistingPeer: JoinRequest {session_id, last_known: None}
    ExistingPeer->>ExistingPeer: Validate session ID
    ExistingPeer->>ExistingPeer: Build FullState (all entities)
    ExistingPeer->>NewNode: FullState {entities, resources, clock}
    NewNode->>NewNode: Clear old entities
    NewNode->>NewNode: Spawn entities from FullState
    NewNode->>Database: Persist state
    NewNode->>ExistingPeer: SyncRequest (catch any missed deltas)
    NewNode->>NewNode: Transition to Active state
```

### Join Flow: Rejoin (Hybrid Sync)

The rejoin flow is optimized for the common case where a node is reconnecting to a session it was previously part of. This happens frequently: app restarts, temporary network disconnections, or laptop sleep/wake cycles.

The key insight is that the node already has most of the session state in its local database. Instead of downloading everything again, it can:

1. **Optimistic Restore**: Load the session and all entities from the local database immediately - the world appears instantly
2. **Clock Comparison**: Send the vector clock from when it last left the session
3. **Delta Calculation**: The peer compares vector clocks and calculates which operations are missing
4. **Incremental Sync**: If the delta count is reasonable (<1000 operations), send only those deltas
5. **Fallback**: If too many deltas accumulated (e.g., node was offline for days), fall back to sending full state
6. **Apply Updates**: Incrementally apply the deltas to bring the world up to date
7. **Final Catchup**: Request any additional deltas that arrived during the sync

This hybrid approach provides several critical advantages:

- **Instant UI**: The world appears immediately from database cache, no loading screen
- **Bandwidth Efficiency**: Only 10-50 KB of deltas instead of 1+ MB full state transfer
- **Lower Latency**: Rejoins complete in under 2 seconds instead of 5-10 seconds
- **Graceful Degradation**: Automatically falls back to full sync if needed
- **Progressive Refinement**: The UI shows cached state immediately, then updates as deltas arrive

```mermaid
sequenceDiagram
    participant RejoiningNode
    participant Database
    participant Network
    participant Peer

    RejoiningNode->>Database: Load session metadata
    RejoiningNode->>Database: Restore entities from DB (optimistic)
    Note over RejoiningNode: World populated from cache
    RejoiningNode->>Network: Connect to gossip (session ALPN)
    RejoiningNode->>Peer: JoinRequest {session_id, last_known: Some(clock)}
    Peer->>Peer: Compare vector clocks
    Peer->>Peer: Calculate missing operations

    alt Delta count ≤ 1000
        Peer->>RejoiningNode: MissingDeltas [operations since clock]
        RejoiningNode->>RejoiningNode: Apply deltas incrementally
    else Too many deltas
        Peer->>RejoiningNode: FullState (more efficient than deltas)
        RejoiningNode->>RejoiningNode: Clear and reload from FullState
    end

    RejoiningNode->>Database: Persist updated state
    RejoiningNode->>Peer: SyncRequest (catch any missed deltas)
    RejoiningNode->>RejoiningNode: Transition to Active state
```

### Join Handler Implementation

The join handler is a Bevy system that runs on existing peers and responds to incoming `JoinRequest` messages. Its responsibility is to decide whether to send full state or deltas based on the joining node's vector clock.

The handler performs several critical validations:

1. **Session ID Validation**: Ensures the request is for the current session (prevents cross-session pollution)
2. **Secret Validation**: If the session is password-protected, validates the provided secret using constant-time comparison
3. **Clock Analysis**: Compares the requester's `last_known_clock` with the operation log to determine if deltas are feasible
4. **Response Selection**: Chooses between sending `MissingDeltas` (for rejoins with <1000 operations) or `FullState` (for fresh joins or large deltas)

The 1000-operation threshold is a heuristic: below this, sending individual deltas is more efficient than serializing the entire world. Above it, the overhead of transmitting many small deltas exceeds the cost of sending a snapshot.

```rust
pub fn handle_join_request_system(
    world: &World,
    bridge: Res<GossipBridge>,
    current_session: Res<CurrentSession>,
    operation_log: Res<OperationLog>,
    networked_entities: Query<(Entity, &NetworkedEntity)>,
    type_registry: Res<AppTypeRegistry>,
    node_clock: Res<NodeVectorClock>,
) {
    while let Some(message) = bridge.try_recv() {
        match message.message {
            SyncMessage::JoinRequest {
                node_id,
                session_id,
                session_secret,
                last_known_clock,
                join_type,
            } => {
                // Validate session ID matches
                if session_id != current_session.session.id {
                    warn!("JoinRequest for wrong session: expected {}, got {}",
                        current_session.session.id.to_code(),
                        session_id.to_code());
                    continue;
                }

                // Validate session secret if configured
                if let Some(expected_secret) = &current_session.session.secret {
                    match &session_secret {
                        Some(provided) if validate_session_secret(provided, expected_secret).is_ok() => {
                            info!("Session secret validated for node {}", node_id);
                        }
                        _ => {
                            error!("JoinRequest from {} rejected: invalid secret", node_id);
                            continue;
                        }
                    }
                }

                info!("Handling JoinRequest from {} ({:?})", node_id, join_type);

                // Decide: send deltas or full state?
                let response = match (join_type, last_known_clock) {
                    (JoinType::Rejoin { .. }, Some(their_clock)) => {
                        // Check if we can send deltas
                        let missing_deltas = operation_log.get_all_operations_newer_than(&their_clock);

                        const MAX_DELTA_OPS: usize = 1000;
                        if missing_deltas.len() <= MAX_DELTA_OPS {
                            info!("Sending {} deltas to rejoining node {}", missing_deltas.len(), node_id);
                            VersionedMessage::new(SyncMessage::MissingDeltas {
                                deltas: missing_deltas,
                            })
                        } else {
                            info!("Too many deltas ({}), sending FullState instead", missing_deltas.len());
                            build_full_state_for_session(
                                world,
                                &networked_entities,
                                &type_registry.read(),
                                &node_clock,
                                &session_id,
                            )
                        }
                    }
                    _ => {
                        // Fresh join - send full state
                        info!("Sending FullState to node {}", node_id);
                        build_full_state_for_session(
                            world,
                            &networked_entities,
                            &type_registry.read(),
                            &node_clock,
                            &session_id,
                        )
                    }
                };

                // Send response
                if let Err(e) = bridge.send(response) {
                    error!("Failed to send join response: {}", e);
                }
            }
            _ => {}
        }
    }
}
```

## Temporary Lock-based Ownership

While CRDTs provide automatic conflict resolution for concurrent edits, they can produce unexpected results when users perform complex, multi-step operations. Consider a user rotating and scaling a 3D object - if another user starts editing the same object mid-operation, the CRDT merge could produce a bizarre intermediate state that neither user intended.

Temporary locks solve this by providing **optimistic, short-lived exclusive access** to entities during active editing. The lock model is intentionally simple and user-driven: when a user selects or begins editing an entity, their client immediately requests a lock. If granted, they have exclusive edit rights for a few seconds. If another user already holds the lock, the request is denied and the user sees visual feedback (e.g., the object is grayed out or shows "locked by Alice").

### Overview

To prevent CRDT conflicts on complex operations (e.g., multi-step drawing, entity transformations), we introduce **temporary exclusive locks** on entities. Only the node holding the lock can modify the entity.

**Design Principles:**
- **Initiator-driven**: Locks are requested immediately when user interaction begins (e.g., clicking an object), not after waiting for server approval
- **Optimistic**: The local client assumes the lock will succeed and allows immediate interaction; conflicts are resolved asynchronously
- **Temporary**: Locks auto-expire after 5 seconds (default) to prevent orphaned locks from crashed nodes
- **Advisory**: Locks are checked before delta generation, but the underlying CRDT still handles conflicts if locks fail
- **Deterministic conflict resolution**: When two nodes request the same lock simultaneously, the higher node ID wins
- **Auto-release**: Disconnected nodes automatically lose all their locks

### Lock State Model

The lock system is implemented as a simple in-memory registry that tracks which entities are currently locked and by whom. Each lock contains:
- **Entity ID**: Which entity is locked
- **Holder**: Which node owns the lock
- **Acquisition timestamp**: When the lock was acquired
- **Timeout duration**: How long until auto-expiry (default 5 seconds)

The `EntityLockRegistry` resource maintains a HashMap of entity ID to lock state, plus an acquisition history queue for rate limiting.

**Lock Acquisition Logic**:

When a node requests a lock, the registry performs several checks:

1. **Existing lock check**: Is this entity already locked?
   - If locked by the requesting node: refresh the timeout and succeed
   - If locked by another node and not expired: reject with `AlreadyLocked` error
   - If locked but expired: proceed to acquire

2. **Rate limiting**: Has this node acquired more than 10 locks in the last second?
   - If yes: reject with `RateLimited` error
   - If no: proceed to acquire

3. **Grant lock**: Create lock entry, add to registry, record acquisition time

If all checks pass, the lock is granted and broadcasted to all peers via a `LockAcquired` message. All peers apply the lock to their local registry, ensuring everyone sees a consistent view of which entities are locked.

**Lock Release Logic**:

When a lock is released - either explicitly by the user (e.g., deselecting an object) or automatically via timeout - a `LockReleased` message is broadcast to all peers. The registry validates that the releasing node actually holds the lock, then removes it from the HashMap. This broadcast-on-release pattern prevents scenarios where one peer thinks an entity is locked while others think it's free.

**Automatic Cleanup**:

A periodic Bevy system runs every second to scan the registry and remove expired locks. This ensures that crashed nodes don't leave orphaned locks indefinitely - after 5 seconds, any lock automatically becomes available again.

The registry also maintains a rolling 60-second history of lock acquisitions for rate limit calculations, pruning old entries to prevent unbounded memory growth.

### Lock Protocol Messages

The lock protocol uses five message types broadcast over the gossip network:

**LockRequest**: Initiates a lock acquisition attempt
- Includes entity ID, requesting node ID, desired timeout, and optional debug reason
- Broadcast to all peers when user begins editing

**LockAcquired**: Confirms successful lock acquisition
- Contains entity ID, holder node ID, and expiration timestamp
- All peers update their local registry to reflect the new lock

**LockRejected**: Indicates lock acquisition failed
- Specifies which entity, who requested it, who currently holds it, and why it failed
- Sent when entity is already locked or rate limit exceeded

**LockRelease**: Explicitly releases a held lock
- Contains entity ID and releasing node ID
- Broadcast when user finishes editing (e.g., deselects object)

**LockReleased**: Confirms lock was released
- Notifies all peers the entity is now available
- All peers remove the lock from their local registry

### Lock Acquisition Flow

The lock acquisition flow is optimistic and user-driven. When a user clicks on an entity to begin editing (e.g., selecting a cube to move it), the client immediately:

1. **Local Check**: Consult the local lock registry - is this entity already locked?
2. **Optimistic Request**: If not locked (or lock expired), immediately broadcast a `LockRequest` to all peers
3. **Peer Application**: All peers (including the requester) apply the lock locally
4. **Grant or Reject**: Each peer validates the lock in their registry:
   - If successful: Broadcast `LockAcquired` confirmation
   - If failed (already locked by someone else): Broadcast `LockRejected` with reason
5. **User Feedback**: Show the user whether they got the lock (enable editing) or didn't (show "locked by Alice")
6. **Edit Phase**: While holding the lock, user can freely modify the entity and generate deltas
7. **Explicit Release**: When done editing (e.g., deselecting), broadcast `LockRelease` to free the entity

The flow is designed for responsiveness - users see immediate feedback rather than waiting for server round-trips.

```mermaid
flowchart TD
    A[User clicks entity] --> B{Check local lock registry}
    B -->|Already locked| C[Show 'Locked by X' message]
    B -->|Not locked or expired| D[Broadcast LockRequest]
    D --> E[All peers apply locally]
    E --> F{Lock acquired?}
    F -->|No, already locked| G[Broadcast LockRejected]
    G --> C
    F -->|Yes, granted| H[Broadcast LockAcquired]
    H --> I[User edits entity]
    I --> J[Generate deltas]
    J --> K{User finished?}
    K -->|No| I
    K -->|Yes| L[Broadcast LockRelease]
    L --> M[Entity available again]
```

### Conflict Resolution

**Scenario**: Two nodes request the same lock simultaneously

The most interesting edge case occurs when two users click the same entity at nearly the same time. Due to network latency, both nodes might broadcast `LockRequest` messages before receiving the other's request. This creates a race condition that must be resolved deterministically.

**Resolution Strategy**: Deterministic tiebreaker using node ID comparison

The resolution protocol works as follows:

1. **Optimistic Locking**: Both Node A and Node B broadcast `LockRequest` for the same entity
2. **Local Application**: Both nodes apply the lock locally (optimistic assumption it will succeed)
3. **Broadcast Confirmation**: Both nodes broadcast `LockAcquired`
4. **Conflict Detection**: When Node A receives Node B's `LockAcquired`, it detects a conflict
5. **Deterministic Resolution**: Compare node IDs - the higher node ID wins, the lower releases
6. **Convergence**: The losing node broadcasts `LockReleased`, and the system converges to a single lock holder

This approach is:
- **Deterministic**: All peers reach the same conclusion about who holds the lock
- **Fair**: Neither node has priority; it's based on random UUIDs
- **Fast**: Conflicts resolve in one round-trip (detect conflict → release)
- **No central authority**: Peers coordinate via gossip without requiring a master

```mermaid
sequenceDiagram
    participant NodeA
    participant NodeB
    participant OtherPeers

    Note over NodeA,NodeB: Both click entity simultaneously

    NodeA->>OtherPeers: LockRequest(entity_id)
    NodeB->>OtherPeers: LockRequest(entity_id)

    NodeA->>NodeA: Apply lock locally
    NodeB->>NodeB: Apply lock locally

    NodeA->>OtherPeers: LockAcquired(entity_id)
    NodeB->>OtherPeers: LockAcquired(entity_id)

    NodeB->>NodeA: LockAcquired(entity_id)
    NodeA->>NodeB: LockAcquired(entity_id)

    Note over NodeA,NodeB: Both detect conflict

    alt NodeA.id > NodeB.id
        NodeB->>NodeB: Release lock (lost)
        NodeB->>OtherPeers: LockReleased(entity_id)
        Note over NodeA: NodeA keeps lock
    else NodeB.id > NodeA.id
        NodeA->>NodeA: Release lock (lost)
        NodeA->>OtherPeers: LockReleased(entity_id)
        Note over NodeB: NodeB keeps lock
    end
```

**Implementation Notes**:

When a node receives a `LockAcquired` message for an entity it also just acquired, it detects the conflict and compares node IDs:
- **Higher node ID**: Keep the lock, ignore the conflict
- **Lower node ID**: Release the lock after a short timeout (100ms) and broadcast `LockReleased`

The brief timeout allows both nodes to detect the conflict before either releases. Without this delay, nodes might race to release, potentially leaving the entity unlocked. The 100ms window ensures both sides see the conflict before the loser releases, guaranteeing exactly one lock holder remains.

This approach provides fast, deterministic convergence without requiring additional coordination rounds or central authority.

### Integration with Change Detection

Locks integrate with Bevy's change detection system to prevent unauthorized modifications. The delta generation system checks the lock registry before broadcasting entity changes:

**Lock Check on Component Changes**:

When a Bevy component changes (detected via `Changed<Transform>` queries), the delta generation system:

1. **Queries changed entities**: Iterate through all networked entities with modified components
2. **Lock validation**: For each changed entity, check `lock_registry.is_locked_by(entity_id, our_node_id)`
3. **Decision**:
   - If we hold the lock: serialize the component change and broadcast an `EntityDelta` message
   - If we don't hold the lock: log a warning and skip delta generation (local change only, not synchronized)

This enforcement ensures that only the lock holder can propagate changes to peers. If a buggy client or edge case causes a component to change without holding the lock, the change remains local and doesn't corrupt the distributed state.

The advisory nature of locks means the underlying CRDT can still handle conflicts if lock enforcement fails, providing defense in depth.

## Persistence Integration

Session lifecycle management requires tight integration with the persistence layer to support automatic rejoin after crashes or restarts. The persistence systems handle three critical responsibilities:

1. **Session Discovery**: On startup, check if there's a previous session to rejoin
2. **State Restoration**: Load session metadata, entities, and vector clocks from the database
3. **Clean Shutdown**: Save current session state before exit

The integration is implemented through Bevy systems that run at specific lifecycle events: startup, shutdown, and periodic checkpoints.

### Session Lifecycle Systems

The session lifecycle is managed through two primary Bevy systems: one for initialization on startup, and one for persisting state on shutdown.

```rust
/// Load or create session on startup
pub fn initialize_session_system(
    mut commands: Commands,
    db: Res<PersistenceDb>,
) {
    let conn = db.lock().unwrap();

    // Check for previous session
    let session = match get_last_active_session(&conn) {
        Ok(Some(session)) => {
            info!("Resuming previous session: {}", session.id.to_code());
            session
        }
        Ok(None) | Err(_) => {
            info!("No previous session, creating new one");
            let session = Session {
                id: SessionId::new(),
                name: None,
                created_at: Utc::now(),
                last_active: Utc::now(),
                entity_count: 0,
                state: SessionState::Created,
                secret: None,
            };

            // Persist to database
            if let Err(e) = save_session(&conn, &session) {
                error!("Failed to save new session: {}", e);
            }

            session
        }
    };

    // Load vector clock for this session
    let vector_clock = load_session_vector_clock(&conn, &session.id)
        .unwrap_or_else(|_| VectorClock::new());

    // Insert as resource
    commands.insert_resource(CurrentSession {
        session: session.clone(),
        vector_clock_at_join: vector_clock.clone(),
    });

    info!("Session initialized: {}", session.id.to_code());
}

/// Save session state on shutdown
pub fn save_session_state_system(
    current_session: Res<CurrentSession>,
    node_clock: Res<NodeVectorClock>,
    networked_entities: Query<&NetworkedEntity>,
    db: Res<PersistenceDb>,
) {
    let mut conn = db.lock().unwrap();

    // Update session metadata
    let mut session = current_session.session.clone();
    session.last_active = Utc::now();
    session.entity_count = networked_entities.iter().count();
    session.state = SessionState::Left;

    if let Err(e) = save_session(&conn, &session) {
        error!("Failed to save session state: {}", e);
    }

    // Save vector clock
    if let Err(e) = save_session_vector_clock(&conn, &session.id, &node_clock.clock) {
        error!("Failed to save vector clock: {}", e);
    }

    info!("Session state saved for {}", session.id.to_code());
}
```

### Database Operations

```rust
/// Load the most recent active session
pub fn get_last_active_session(conn: &Connection) -> Result<Option<Session>> {
    let row = conn.query_row(
        "SELECT id, name, created_at, last_active, entity_count, state, secret
         FROM sessions
         ORDER BY last_active DESC
         LIMIT 1",
        [],
        |row| {
            Ok(Session {
                id: SessionId(Uuid::from_slice(&row.get::<_, Vec<u8>>(0)?)?),
                name: row.get(1)?,
                created_at: timestamp_to_datetime(row.get(2)?),
                last_active: timestamp_to_datetime(row.get(3)?),
                entity_count: row.get(4)?,
                state: parse_session_state(&row.get::<_, String>(5)?),
                secret: row.get(6)?,
            })
        },
    ).optional()?;

    Ok(row)
}

/// Save session metadata
pub fn save_session(conn: &Connection, session: &Session) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO sessions (id, name, created_at, last_active, entity_count, state, secret)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            session.id.0.as_bytes(),
            session.name,
            session.created_at.timestamp(),
            session.last_active.timestamp(),
            session.entity_count,
            format!("{:?}", session.state).to_lowercase(),
            session.secret,
        ],
    )?;

    Ok(())
}

/// Load vector clock for a session
pub fn load_session_vector_clock(conn: &Connection, session_id: &SessionId) -> Result<VectorClock> {
    let mut clock = VectorClock::new();

    let mut stmt = conn.prepare(
        "SELECT node_id, counter
         FROM vector_clock
         WHERE session_id = ?1"
    )?;

    let rows = stmt.query_map([session_id.0.as_bytes()], |row| {
        let node_str: String = row.get(0)?;
        let counter: u64 = row.get(1)?;
        Ok((Uuid::parse_str(&node_str).unwrap(), counter))
    })?;

    for row in rows {
        let (node_id, counter) = row?;
        clock.clocks.insert(node_id, counter);
    }

    Ok(clock)
}

/// Save vector clock for a session
pub fn save_session_vector_clock(
    conn: &mut Connection,
    session_id: &SessionId,
    clock: &VectorClock,
) -> Result<()> {
    let tx = conn.transaction()?;

    // Clear old clocks for this session
    tx.execute(
        "DELETE FROM vector_clock WHERE session_id = ?1",
        [session_id.0.as_bytes()],
    )?;

    // Insert current clocks
    for (node_id, counter) in &clock.clocks {
        tx.execute(
            "INSERT INTO vector_clock (session_id, node_id, counter, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                session_id.0.as_bytes(),
                node_id.to_string(),
                counter,
                Utc::now().timestamp(),
            ],
        )?;
    }

    tx.commit()?;
    Ok(())
}
```

## Implementation Roadmap

### Phase 1: Session Data Model & Persistence
- Create `SessionId`, `Session`, `SessionState` types
- Add database schema migration
- Implement session persistence (save/load)
- Add `CurrentSession` resource

**Critical files:**
- NEW: `crates/lib/src/networking/session.rs`
- MODIFY: `crates/lib/src/persistence/database.rs`
- NEW: `crates/lib/src/persistence/migrations/004_sessions.sql`

### Phase 2: ALPN Network Isolation
- Implement `SessionId::to_alpn()` derivation
- Modify gossip setup to use session-specific ALPN
- Update router configuration
- Test session isolation (two sessions can't see each other)

**Critical files:**
- MODIFY: `crates/app/src/setup.rs`
- MODIFY: `crates/lib/src/networking/session.rs`

### Phase 3: Hybrid Join Protocol
- Extend `JoinRequest` with `last_known_clock` and `join_type`
- Implement fresh join flow
- Implement rejoin flow with delta sync
- Add delta size threshold logic

**Critical files:**
- NEW: `crates/lib/src/networking/join_protocol.rs`
- MODIFY: `crates/lib/src/networking/messages.rs`
- MODIFY: `crates/lib/src/networking/operation_log.rs`

### Phase 4: Entity Lock System
- Create `EntityLock`, `EntityLockRegistry` types
- Add lock protocol messages
- Implement lock acquisition/release/timeout
- Add conflict resolution
- Integrate with change detection

**Critical files:**
- NEW: `crates/lib/src/networking/locks.rs`
- MODIFY: `crates/lib/src/networking/messages.rs`
- MODIFY: `crates/lib/src/networking/delta_generation.rs`
- MODIFY: `crates/lib/src/networking/plugin.rs`

### Phase 5: Auto-Rejoin on Restart
- Implement session load on startup
- Test crash recovery → auto-rejoin
- Verify vector clock restoration
- Handle edge case: session no longer exists

**Critical files:**
- MODIFY: `crates/app/src/main.rs`
- NEW: `crates/lib/src/persistence/session_persistence.rs`

## Edge Cases and Failure Modes

### 1. Session No Longer Exists
**Scenario**: Node tries to rejoin a session that no peers are in anymore.

**Handling**:
- JoinRequest times out (no response after 5 seconds)
- Offer user choice: "Create new session" or "Join different session"
- Clean up orphaned session from database

### 2. Clock Divergence Too Large
**Scenario**: Node rejoins after weeks, vector clock gap is massive.

**Handling**:
- Peer detects: delta count > 1000 operations
- Send FullState instead of deltas
- Node clears local state and reloads from FullState

### 3. Concurrent Lock Requests
**Scenario**: Two nodes request same entity lock within milliseconds.

**Handling**:
- Both apply lock locally (optimistic)
- On receiving peer's LockAcquired, compare node IDs
- Lower node ID releases and broadcasts LockReleased
- Deterministic convergence (higher node ID wins)

### 4. Lock Holder Crashes
**Scenario**: Node A acquires lock, then crashes without releasing.

**Handling**:
- 5-second timeout expires automatically
- Other nodes can acquire lock after timeout
- No special crash detection needed

### 5. Network Partition During Lock
**Scenario**: Network partitions while node holds lock.

**Handling**:
- Both partitions think they have lock (acceptable during partition)
- On partition heal, CRDT merge semantics resolve conflicts
- Vector clock + LWW determines final state
- Locks are advisory (CRDTs provide safety net)

## Security Considerations

Security in a peer-to-peer collaborative environment requires careful balance between usability and protection. This RFC addresses two primary security concerns: session access control and protocol integrity.

### Session Secret Validation

Session secrets provide optional password-based access control. When a session is created with a secret, any peer attempting to join must provide the matching secret in their `JoinRequest`. The validation uses constant-time comparison to prevent timing attacks that could leak information about the secret.

The secret is hashed using BLAKE3 before comparison, ensuring that:
- Secrets are never transmitted in plaintext
- Timing analysis cannot reveal secret length or content
- Fast validation (BLAKE3 is extremely performant)

```rust
/// Validate session secret using constant-time comparison
pub fn validate_session_secret(provided: &[u8], expected: &[u8]) -> Result<(), AuthError> {
    use subtle::ConstantTimeEq;

    let provided_hash = blake3::hash(provided);
    let expected_hash = blake3::hash(expected);

    if provided_hash.as_bytes().ct_eq(expected_hash.as_bytes()).into() {
        Ok(())
    } else {
        Err(AuthError::InvalidSecret)
    }
}
```

### Rate Limiting

```rust
// In EntityLockRegistry
const MAX_LOCKS_PER_NODE: usize = 100;
const MAX_LOCK_REQUESTS_PER_SEC: usize = 10;

// Prevent lock spamming
if self.locks_held_by(node_id) >= MAX_LOCKS_PER_NODE {
    return Err(LockError::TooManyLocks);
}
```

## Performance Considerations

### Database Indexing

```sql
-- Fast session lookup by last active
CREATE INDEX idx_sessions_last_active ON sessions(last_active DESC);

-- Fast entity lookup by session
CREATE INDEX idx_entities_session ON entities(session_id);

-- Fast vector clock lookup
CREATE INDEX idx_vector_clock_session_node ON vector_clock(session_id, node_id);

-- Fast operation log queries
CREATE INDEX idx_operation_log_session ON operation_log(session_id, node_id, sequence_number);
```

### Memory Usage
- Lock Registry: O(num_locked_entities) - typically <100 locks
- Session Metadata: O(1) - single active session
- Vector Clock per Session: O(num_peers) - typically 2-5 entries

### Network Bandwidth

**Rejoin Optimization**:
- Delta transfer: ~10 KB (100 operations @ 100 bytes each) - most common case for rejoins
- Full state transfer: ~1 MB - for fresh joins or large deltas

**Streaming Full State Transfer**:

For full state transfers, instead of building and sending the entire world as a single monolithic message, we stream entities incrementally:

1. **Entity Count Header**: Send the total count of networked entities first (`FullStateHeader { total_entities: 1500 }`)
2. **Streaming Entities**: Send entities in batches of 50-100, each as a separate message
3. **Progress Tracking**: The receiving node knows exactly how many entities to expect and can display a progress meter ("Syncing: 450/1500 entities")
4. **Progressive Rendering**: Entities appear in the world as they arrive, rather than waiting for the entire transfer
5. **Interruptibility**: If the connection drops mid-transfer, the node knows which entities are missing and can request them specifically

This streaming approach provides several UX benefits:
- **Visual feedback**: Users see the world populate gradually instead of staring at a blank screen
- **Perceived performance**: The UI feels responsive even during large transfers
- **Early interaction**: Users can start interacting with entities that have already loaded
- **Bandwidth monitoring**: Progress meter shows transfer is active and not stalled

The 100x bandwidth improvement for delta-based rejoins remains the primary optimization, but streaming ensures fresh joins also have good UX.

## Testing Strategy

### Unit Tests
- `test_session_alpn_derivation()` - Verify deterministic ALPN generation
- `test_lock_timeout()` - Verify locks expire after 5 seconds
- `test_lock_conflict_resolution()` - Verify deterministic tiebreaker
- `test_session_persistence()` - Verify save/load session
- `test_vector_clock_restoration()` - Verify clock persists per session

### Integration Tests
- `test_session_isolation()` - Two sessions can't see each other's messages
- `test_hybrid_rejoin_flow()` - Node rejoins and receives only deltas
- `test_fresh_join_flow()` - Node joins for first time, receives FullState
- `test_lock_acquisition()` - Node acquires lock, others see rejection
- `test_concurrent_lock_conflict()` - Two nodes request same lock, higher ID wins
- `test_auto_rejoin_after_crash()` - Node restarts, auto-rejoins last session

### Performance Tests
- Rejoin bandwidth: <50 KB (vs >1 MB for full sync)
- Rejoin latency: <2 seconds
- Lock acquisition latency: <100ms
- 60 FPS maintained with 100 active locks

## Success Criteria

- [ ] Two sessions run simultaneously without interference
- [ ] Node rejoins session in <2 seconds after restart
- [ ] Entity locks prevent concurrent modifications
- [ ] Bandwidth for rejoin is <50 KB
- [ ] Lock conflicts resolved deterministically
- [ ] Session persists across app restarts
- [ ] 60 FPS with 100 locked entities
- [ ] Crash recovery restores session correctly

## Future Enhancements

### Session UI/UX with egui (Tech Demo)

For the initial technical demonstration, implement a basic session management UI using egui:

**Create/Join Session Dialog**:
- Text input for session code (6-character format: `abc-def-123`)
- Optional password field for session secret
- "Create New Session" button (generates random session ID)
- "Join Existing Session" button (validates code format)
- Display current session code prominently once connected

**Session Status Panel**:
- Current session code (large, copyable text)
- Connection status indicator (Created → Joining → Active)
- Peer count ("2 peers connected")
- Sync progress bar (for initial join showing "Syncing: 450/1500 entities")
- Entity count in session

**Lock Feedback**:
- Visual indicator on locked entities (outline color, glow effect)
- Tooltip showing "Locked by Alice" when hovering over locked entities
- Your own locks shown in a different color (e.g., green vs. red for others)
- Lock acquisition feedback ("Lock acquired" / "Already locked by Bob")

**Session List** (startup screen):
- Recent sessions with metadata (name, last active, entity count)
- "Resume" button for quick rejoin
- "Delete" button to remove old sessions

This minimal UI provides enough functionality to demonstrate and test the session lifecycle features without building a full application interface.

### Phase 6: Session Invites
- Generate shareable invite links
- QR code support for mobile
- Time-limited invites

### Phase 7: Session Migration
- Merge two sessions
- Split session (fork)
- Export/import session data

### Phase 8: Advanced Lock Modes
- Read/write locks (multiple readers, single writer)
- Lock priorities
- Lock queuing

### Phase 9: Session Discovery
- LAN session browsing (mDNS)
- Recent sessions list
- Session search by name

## References

- RFC 0001: CRDT Gossip Sync (foundation)
- RFC 0002: Persistence Strategy (database layer)
- iroh documentation: https://docs.rs/iroh
- ALPN specification: RFC 7301
