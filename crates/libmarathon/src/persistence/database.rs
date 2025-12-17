//! Database schema and operations for persistence layer

use std::path::Path;

use chrono::Utc;
use rusqlite::{
    Connection,
    OptionalExtension,
};

use crate::persistence::{
    error::{
        PersistenceError,
        Result,
    },
    types::*,
};

/// Default SQLite page size in bytes (4KB)
const DEFAULT_PAGE_SIZE: i64 = 4096;

/// Cache size for SQLite in KB (negative value = KB instead of pages)
const CACHE_SIZE_KB: i64 = -20000; // 20MB

/// Get current Unix timestamp in seconds
///
/// Helper to avoid repeating `Utc::now().timestamp()` throughout the code
#[inline]
fn current_timestamp() -> i64 {
    Utc::now().timestamp()
}

/// Initialize SQLite connection with WAL mode and optimizations
pub fn initialize_persistence_db<P: AsRef<Path>>(path: P) -> Result<Connection> {
    let mut conn = Connection::open(path)?;

    configure_sqlite_for_persistence(&conn)?;

    // Run migrations to ensure schema is up to date
    crate::persistence::run_migrations(&mut conn)?;

    Ok(conn)
}

/// Configure SQLite with WAL mode and battery-friendly settings
pub fn configure_sqlite_for_persistence(conn: &Connection) -> Result<()> {
    // Enable Write-Ahead Logging for better concurrency and fewer fsyncs
    conn.execute_batch("PRAGMA journal_mode = WAL;")?;

    // Don't auto-checkpoint on every transaction - we'll control this manually
    conn.execute_batch("PRAGMA wal_autocheckpoint = 0;")?;

    // NORMAL synchronous mode - fsync WAL on commit, but not every write
    // This is a good balance between durability and performance
    conn.execute_batch("PRAGMA synchronous = NORMAL;")?;

    // Larger page size for better sequential write performance on mobile
    // Note: This must be set before the database is created or after VACUUM
    // We'll skip setting it if database already exists to avoid issues
    let page_size: i64 = conn.query_row("PRAGMA page_size", [], |row| row.get(0))?;
    if page_size == DEFAULT_PAGE_SIZE {
        // Try to set larger page size, but only if we're at default
        // This will only work on a fresh database
        let _ = conn.execute_batch("PRAGMA page_size = 8192;");
    }

    // Increase cache size for better performance (in pages, negative = KB)
    conn.execute_batch(&format!("PRAGMA cache_size = {};", CACHE_SIZE_KB))?;

    // Use memory for temp tables (faster, we don't need temp table durability)
    conn.execute_batch("PRAGMA temp_store = MEMORY;")?;

    Ok(())
}

/// Create the database schema for persistence
pub fn create_persistence_schema(conn: &Connection) -> Result<()> {
    // Entities table - stores entity metadata
    conn.execute(
        "CREATE TABLE IF NOT EXISTS entities (
            id BLOB PRIMARY KEY,
            entity_type TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    )?;

    // Components table - stores serialized component data
    conn.execute(
        "CREATE TABLE IF NOT EXISTS components (
            entity_id BLOB NOT NULL,
            component_type TEXT NOT NULL,
            data BLOB NOT NULL,
            updated_at INTEGER NOT NULL,
            PRIMARY KEY (entity_id, component_type),
            FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE
        )",
        [],
    )?;

    // Index for querying components by entity
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_components_entity
         ON components(entity_id)",
        [],
    )?;

    // Operation log - for CRDT sync protocol
    conn.execute(
        "CREATE TABLE IF NOT EXISTS operation_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            node_id TEXT NOT NULL,
            sequence_number INTEGER NOT NULL,
            operation BLOB NOT NULL,
            timestamp INTEGER NOT NULL,
            UNIQUE(node_id, sequence_number)
        )",
        [],
    )?;

    // Index for efficient operation log queries
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_oplog_node_seq
         ON operation_log(node_id, sequence_number)",
        [],
    )?;

    // Vector clock table - for causality tracking
    conn.execute(
        "CREATE TABLE IF NOT EXISTS vector_clock (
            node_id TEXT PRIMARY KEY,
            counter INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    )?;

    // Session state table - for crash detection
    conn.execute(
        "CREATE TABLE IF NOT EXISTS session_state (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    )?;

    // WAL checkpoint tracking
    conn.execute(
        "CREATE TABLE IF NOT EXISTS checkpoint_state (
            last_checkpoint INTEGER NOT NULL,
            wal_size_bytes INTEGER NOT NULL
        )",
        [],
    )?;

    // Initialize checkpoint state if not exists
    conn.execute(
        "INSERT OR IGNORE INTO checkpoint_state (rowid, last_checkpoint, wal_size_bytes)
         VALUES (1, ?, 0)",
        [current_timestamp()],
    )?;

    Ok(())
}

/// Flush a batch of operations to SQLite in a single transaction
pub fn flush_to_sqlite(ops: &[PersistenceOp], conn: &mut Connection) -> Result<usize> {
    if ops.is_empty() {
        return Ok(0);
    }

    let tx = conn.transaction()?;
    let mut count = 0;

    for op in ops {
        match op {
            | PersistenceOp::UpsertEntity { id, data } => {
                tx.execute(
                    "INSERT OR REPLACE INTO entities (id, entity_type, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![
                        id.as_bytes(),
                        data.entity_type,
                        data.created_at.timestamp(),
                        data.updated_at.timestamp(),
                    ],
                )?;
                count += 1;
            },

            | PersistenceOp::UpsertComponent {
                entity_id,
                component_type,
                data,
            } => {
                tx.execute(
                    "INSERT OR REPLACE INTO components (entity_id, component_type, data, updated_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![
                        entity_id.as_bytes(),
                        component_type,
                        data.as_ref(),
                        current_timestamp(),
                    ],
                )?;
                count += 1;
            },

            | PersistenceOp::LogOperation {
                node_id,
                sequence,
                operation,
            } => {
                tx.execute(
                    "INSERT OR REPLACE INTO operation_log (node_id, sequence_number, operation, timestamp)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![
                        &node_id.to_string(), // Convert UUID to string for SQLite TEXT column
                        sequence,
                        operation.as_ref(),
                        current_timestamp(),
                    ],
                )?;
                count += 1;
            },

            | PersistenceOp::UpdateVectorClock { node_id, counter } => {
                tx.execute(
                    "INSERT OR REPLACE INTO vector_clock (node_id, counter, updated_at)
                     VALUES (?1, ?2, ?3)",
                    rusqlite::params![&node_id.to_string(), counter, current_timestamp()], // Convert UUID to string
                )?;
                count += 1;
            },

            | PersistenceOp::DeleteEntity { id } => {
                tx.execute(
                    "DELETE FROM entities WHERE id = ?1",
                    rusqlite::params![id.as_bytes()],
                )?;
                count += 1;
            },

            | PersistenceOp::DeleteComponent {
                entity_id,
                component_type,
            } => {
                tx.execute(
                    "DELETE FROM components WHERE entity_id = ?1 AND component_type = ?2",
                    rusqlite::params![entity_id.as_bytes(), component_type],
                )?;
                count += 1;
            },
        }
    }

    tx.commit()?;
    Ok(count)
}

/// Manually checkpoint the WAL file to merge changes into the main database
///
/// This function performs a SQLite WAL checkpoint, which copies frames from the
/// write-ahead log back into the main database file. This is crucial for:
/// - Reducing WAL file size to save disk space
/// - Ensuring durability of committed transactions
/// - Maintaining database integrity
///
/// # Parameters
/// - `conn`: Mutable reference to the SQLite connection
/// - `mode`: Checkpoint mode controlling blocking behavior (see
///   [`CheckpointMode`])
///
/// # Returns
/// - `Ok(CheckpointInfo)`: Information about the checkpoint operation
/// - `Err`: If the checkpoint fails or database state update fails
///
/// # Examples
/// ```no_run
/// # use rusqlite::Connection;
/// # use libmarathon::persistence::*;
/// # fn example() -> anyhow::Result<()> {
/// let mut conn = Connection::open("app.db")?;
/// let info = checkpoint_wal(&mut conn, CheckpointMode::Passive)?;
/// if info.busy {
///     // Some pages couldn't be checkpointed due to active readers
/// }
/// # Ok(())
/// # }
/// ```
pub fn checkpoint_wal(conn: &mut Connection, mode: CheckpointMode) -> Result<CheckpointInfo> {
    let mode_str = match mode {
        | CheckpointMode::Passive => "PASSIVE",
        | CheckpointMode::Full => "FULL",
        | CheckpointMode::Restart => "RESTART",
        | CheckpointMode::Truncate => "TRUNCATE",
    };

    let query = format!("PRAGMA wal_checkpoint({})", mode_str);

    // Returns (busy, log_pages, checkpointed_pages)
    let (busy, log_pages, checkpointed_pages): (i32, i32, i32) =
        conn.query_row(&query, [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;

    // Update checkpoint state
    conn.execute(
        "UPDATE checkpoint_state SET last_checkpoint = ?1 WHERE rowid = 1",
        [current_timestamp()],
    )?;

    Ok(CheckpointInfo {
        busy: busy != 0,
        log_pages,
        checkpointed_pages,
    })
}

/// Get the size of the WAL file in bytes
///
/// This checks the actual WAL file size on disk without triggering a
/// checkpoint. Large WAL files consume disk space and can slow down recovery,
/// so monitoring size helps maintain optimal performance.
///
/// # Parameters
/// - `conn`: Reference to the SQLite connection
///
/// # Returns
/// - `Ok(i64)`: WAL file size in bytes (0 if no WAL exists or in-memory
///   database)
/// - `Err`: If the database path query fails
///
/// # Note
/// For in-memory databases, always returns 0.
pub fn get_wal_size(conn: &Connection) -> Result<i64> {
    // Get the database file path
    let db_path: Option<String> = conn
        .query_row("PRAGMA database_list", [], |row| row.get::<_, String>(2))
        .optional()?;

    // If no path (in-memory database), return 0
    let Some(db_path) = db_path else {
        return Ok(0);
    };

    // WAL file has same name as database but with -wal suffix
    let wal_path = format!("{}-wal", db_path);

    // Check if WAL file exists and get its size
    match std::fs::metadata(&wal_path) {
        | Ok(metadata) => Ok(metadata.len() as i64),
        | Err(_) => Ok(0), // WAL doesn't exist yet
    }
}

/// Checkpoint mode for WAL
#[derive(Debug, Clone, Copy)]
pub enum CheckpointMode {
    /// Passive checkpoint - doesn't block readers/writers
    Passive,
    /// Full checkpoint - waits for writers to finish
    Full,
    /// Restart checkpoint - like Full, but restarts WAL file
    Restart,
    /// Truncate checkpoint - like Restart, but truncates WAL file to 0 bytes
    Truncate,
}

/// Information about a checkpoint operation
#[derive(Debug)]
pub struct CheckpointInfo {
    pub busy: bool,
    pub log_pages: i32,
    pub checkpointed_pages: i32,
}

/// Set a session state value in the database
///
/// Session state is used to track application lifecycle events and detect
/// crashes. Values persist across restarts, enabling crash detection and
/// recovery.
///
/// # Parameters
/// - `conn`: Mutable reference to the SQLite connection
/// - `key`: State key (e.g., "clean_shutdown", "session_id")
/// - `value`: State value to store
///
/// # Returns
/// - `Ok(())`: State was successfully saved
/// - `Err`: If the database write fails
pub fn set_session_state(conn: &mut Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO session_state (key, value, updated_at)
         VALUES (?1, ?2, ?3)",
        rusqlite::params![key, value, current_timestamp()],
    )?;
    Ok(())
}

/// Get a session state value from the database
///
/// Retrieves persistent state information stored across application sessions.
///
/// # Parameters
/// - `conn`: Reference to the SQLite connection
/// - `key`: State key to retrieve
///
/// # Returns
/// - `Ok(Some(value))`: State exists and was retrieved
/// - `Ok(None)`: State key doesn't exist
/// - `Err`: If the database query fails
pub fn get_session_state(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM session_state WHERE key = ?1",
        rusqlite::params![key],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| PersistenceError::Database(e))
}

/// Check if the previous session had a clean shutdown
///
/// This is critical for crash detection. When the application starts, this
/// checks if the previous session ended cleanly. If not, it indicates a crash
/// occurred, and recovery procedures may be needed.
///
/// **Side effect**: Resets the clean_shutdown flag to "false" for the current
/// session. Call [`mark_clean_shutdown`] during normal shutdown to set it back
/// to "true".
///
/// # Parameters
/// - `conn`: Mutable reference to the SQLite connection (mutates session state)
///
/// # Returns
/// - `Ok(true)`: Previous session shut down cleanly
/// - `Ok(false)`: Previous session crashed or this is first run
/// - `Err`: If database operations fail
pub fn check_clean_shutdown(conn: &mut Connection) -> Result<bool> {
    let clean = get_session_state(conn, "clean_shutdown")?
        .map(|v| v == "true")
        .unwrap_or(false);

    // Reset for this session
    set_session_state(conn, "clean_shutdown", "false")?;

    Ok(clean)
}

/// Mark the current session as cleanly shut down
///
/// Call this during normal application shutdown to indicate clean termination.
/// The next startup will detect this flag via [`check_clean_shutdown`] and know
/// no crash occurred.
///
/// # Parameters
/// - `conn`: Mutable reference to the SQLite connection
///
/// # Returns
/// - `Ok(())`: Clean shutdown flag was set
/// - `Err`: If the database write fails
pub fn mark_clean_shutdown(conn: &mut Connection) -> Result<()> {
    set_session_state(conn, "clean_shutdown", "true")
}

//
// ============================================================================
// Session Management Operations
// ============================================================================
//

/// Save session metadata to database
pub fn save_session(conn: &mut Connection, session: &crate::networking::Session) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO sessions (id, code, name, created_at, last_active, entity_count, state, secret)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            session.id.as_uuid().as_bytes(),
            session.id.to_code(),
            session.name,
            session.created_at,
            session.last_active,
            session.entity_count as i64,
            session.state.to_string(),
            session.secret.as_ref().map(|b| b.as_ref()),
        ],
    )?;
    Ok(())
}

/// Load session by ID
pub fn load_session(
    conn: &Connection,
    session_id: crate::networking::SessionId,
) -> Result<Option<crate::networking::Session>> {
    conn.query_row(
        "SELECT code, name, created_at, last_active, entity_count, state, secret
         FROM sessions WHERE id = ?1",
        [session_id.as_uuid().as_bytes()],
        |row| {
            let code: String = row.get(0)?;
            let state_str: String = row.get(5)?;
            let state = crate::networking::SessionState::from_str(&state_str)
                .unwrap_or(crate::networking::SessionState::Created);

            // Reconstruct SessionId from the stored code
            let id = crate::networking::SessionId::from_code(&code)
                .map_err(|_| rusqlite::Error::InvalidQuery)?;

            Ok(crate::networking::Session {
                id,
                name: row.get(1)?,
                created_at: row.get(2)?,
                last_active: row.get(3)?,
                entity_count: row.get::<_, i64>(4)? as usize,
                state,
                secret: row.get::<_, Option<std::borrow::Cow<'_, [u8]>>>(6)?
                    .map(|cow| bytes::Bytes::copy_from_slice(&cow)),
            })
        },
    )
    .optional()
    .map_err(PersistenceError::from)
}

/// Get the most recently active session
pub fn get_last_active_session(conn: &Connection) -> Result<Option<crate::networking::Session>> {
    conn.query_row(
        "SELECT code, name, created_at, last_active, entity_count, state, secret
         FROM sessions ORDER BY last_active DESC LIMIT 1",
        [],
        |row| {
            let code: String = row.get(0)?;
            let state_str: String = row.get(5)?;
            let state = crate::networking::SessionState::from_str(&state_str)
                .unwrap_or(crate::networking::SessionState::Created);

            // Reconstruct SessionId from the stored code
            let id = crate::networking::SessionId::from_code(&code)
                .map_err(|_| rusqlite::Error::InvalidQuery)?;

            Ok(crate::networking::Session {
                id,
                name: row.get(1)?,
                created_at: row.get(2)?,
                last_active: row.get(3)?,
                entity_count: row.get::<_, i64>(4)? as usize,
                state,
                secret: row.get::<_, Option<std::borrow::Cow<'_, [u8]>>>(6)?
                    .map(|cow| bytes::Bytes::copy_from_slice(&cow)),
            })
        },
    )
    .optional()
    .map_err(PersistenceError::from)
}

/// Save session vector clock to database
pub fn save_session_vector_clock(
    conn: &mut Connection,
    session_id: crate::networking::SessionId,
    clock: &crate::networking::VectorClock,
) -> Result<()> {
    let tx = conn.transaction()?;

    // Delete old clock entries for this session
    tx.execute(
        "DELETE FROM vector_clock WHERE session_id = ?1",
        [session_id.as_uuid().as_bytes()],
    )?;

    // Insert current clock state
    for (node_id, &counter) in &clock.clocks {
        tx.execute(
            "INSERT INTO vector_clock (session_id, node_id, counter, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                session_id.as_uuid().as_bytes(),
                node_id.to_string(),
                counter as i64,
                current_timestamp(),
            ],
        )?;
    }

    tx.commit()?;
    Ok(())
}

/// Load session vector clock from database
pub fn load_session_vector_clock(
    conn: &Connection,
    session_id: crate::networking::SessionId,
) -> Result<crate::networking::VectorClock> {
    let mut stmt =
        conn.prepare("SELECT node_id, counter FROM vector_clock WHERE session_id = ?1")?;

    let mut clock = crate::networking::VectorClock::new();
    let rows = stmt.query_map([session_id.as_uuid().as_bytes()], |row| {
        let node_id_str: String = row.get(0)?;
        let counter: i64 = row.get(1)?;
        Ok((node_id_str, counter))
    })?;

    for row in rows {
        let (node_id_str, counter) = row?;
        if let Ok(node_id) = uuid::Uuid::parse_str(&node_id_str) {
            clock.clocks.insert(node_id, counter as u64);
        }
    }

    Ok(clock)
}

/// Loaded entity data from database
#[derive(Debug)]
pub struct LoadedEntity {
    pub id: uuid::Uuid,
    pub entity_type: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub components: Vec<LoadedComponent>,
}

/// Loaded component data from database
#[derive(Debug)]
pub struct LoadedComponent {
    pub component_type: String,
    pub data: bytes::Bytes,
}

/// Load all components for a single entity from the database
pub fn load_entity_components(
    conn: &Connection,
    entity_id: uuid::Uuid,
) -> Result<Vec<LoadedComponent>> {
    let mut stmt = conn.prepare(
        "SELECT component_type, data
         FROM components
         WHERE entity_id = ?1",
    )?;

    let components: Vec<LoadedComponent> = stmt
        .query_map([entity_id.as_bytes()], |row| {
            let data_cow: std::borrow::Cow<'_, [u8]> = row.get(1)?;
            Ok(LoadedComponent {
                component_type: row.get(0)?,
                data: bytes::Bytes::copy_from_slice(&data_cow),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(components)
}

/// Load a single entity by network ID from the database
///
/// Returns None if the entity doesn't exist.
pub fn load_entity_by_network_id(
    conn: &Connection,
    network_id: uuid::Uuid,
) -> Result<Option<LoadedEntity>> {
    // Load entity metadata
    let entity_data = conn
        .query_row(
            "SELECT id, entity_type, created_at, updated_at
             FROM entities
             WHERE id = ?1",
            [network_id.as_bytes()],
            |row| {
                let id_bytes: std::borrow::Cow<'_, [u8]> = row.get(0)?;
                let mut id_array = [0u8; 16];
                id_array.copy_from_slice(&id_bytes);
                let id = uuid::Uuid::from_bytes(id_array);

                Ok((
                    id,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()?;

    let Some((id, entity_type, created_at_ts, updated_at_ts)) = entity_data else {
        return Ok(None);
    };

    // Load all components for this entity
    let components = load_entity_components(conn, id)?;

    Ok(Some(LoadedEntity {
        id,
        entity_type,
        created_at: chrono::DateTime::from_timestamp(created_at_ts, 0)
            .unwrap_or_else(chrono::Utc::now),
        updated_at: chrono::DateTime::from_timestamp(updated_at_ts, 0)
            .unwrap_or_else(chrono::Utc::now),
        components,
    }))
}

/// Load all entities from the database
///
/// This loads all entity metadata and their components.
/// Used during startup to rehydrate the game state.
pub fn load_all_entities(conn: &Connection) -> Result<Vec<LoadedEntity>> {
    let mut stmt = conn.prepare(
        "SELECT id, entity_type, created_at, updated_at
         FROM entities
         ORDER BY created_at ASC",
    )?;

    let entity_rows = stmt.query_map([], |row| {
        let id_bytes: std::borrow::Cow<'_, [u8]> = row.get(0)?;
        let mut id_array = [0u8; 16];
        id_array.copy_from_slice(&id_bytes);
        let id = uuid::Uuid::from_bytes(id_array);

        Ok((
            id,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })?;

    let mut entities = Vec::new();

    for row in entity_rows {
        let (id, entity_type, created_at_ts, updated_at_ts) = row?;

        // Load all components for this entity
        let components = load_entity_components(conn, id)?;

        entities.push(LoadedEntity {
            id,
            entity_type,
            created_at: chrono::DateTime::from_timestamp(created_at_ts, 0)
                .unwrap_or_else(chrono::Utc::now),
            updated_at: chrono::DateTime::from_timestamp(updated_at_ts, 0)
                .unwrap_or_else(chrono::Utc::now),
            components,
        });
    }

    Ok(entities)
}

/// Load entities by entity type from the database
///
/// Returns all entities matching the specified entity_type.
pub fn load_entities_by_type(conn: &Connection, entity_type: &str) -> Result<Vec<LoadedEntity>> {
    let mut stmt = conn.prepare(
        "SELECT id, entity_type, created_at, updated_at
         FROM entities
         WHERE entity_type = ?1
         ORDER BY created_at ASC",
    )?;

    let entity_rows = stmt.query_map([entity_type], |row| {
        let id_bytes: std::borrow::Cow<'_, [u8]> = row.get(0)?;
        let mut id_array = [0u8; 16];
        id_array.copy_from_slice(&id_bytes);
        let id = uuid::Uuid::from_bytes(id_array);

        Ok((
            id,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })?;

    let mut entities = Vec::new();

    for row in entity_rows {
        let (id, entity_type, created_at_ts, updated_at_ts) = row?;

        // Load all components for this entity
        let components = load_entity_components(conn, id)?;

        entities.push(LoadedEntity {
            id,
            entity_type,
            created_at: chrono::DateTime::from_timestamp(created_at_ts, 0)
                .unwrap_or_else(chrono::Utc::now),
            updated_at: chrono::DateTime::from_timestamp(updated_at_ts, 0)
                .unwrap_or_else(chrono::Utc::now),
            components,
        });
    }

    Ok(entities)
}

/// Rehydrate a loaded entity into the Bevy world
///
/// Takes a `LoadedEntity` from the database and spawns it as a new Bevy entity,
/// deserializing and inserting all components using the ComponentTypeRegistry.
///
/// # Arguments
///
/// * `loaded_entity` - The entity data loaded from SQLite
/// * `world` - The Bevy world to spawn the entity into
/// * `component_registry` - Type registry for component deserialization
///
/// # Returns
///
/// The spawned Bevy `Entity` on success
///
/// # Errors
///
/// Returns an error if:
/// - Component deserialization fails
/// - Component type is not registered
/// - Component insertion fails
pub fn rehydrate_entity(
    loaded_entity: LoadedEntity,
    world: &mut bevy::prelude::World,
    component_registry: &crate::persistence::ComponentTypeRegistry,
) -> Result<bevy::prelude::Entity> {
    use bevy::prelude::*;

    use crate::networking::NetworkedEntity;

    // Spawn a new entity
    let entity = world.spawn_empty().id();

    info!(
        "Rehydrating entity {:?} with type {} and {} components",
        loaded_entity.id,
        loaded_entity.entity_type,
        loaded_entity.components.len()
    );

    // Deserialize and insert each component
    for component in &loaded_entity.components {
        // Get deserialization function for this component type
        let deserialize_fn = component_registry
            .get_deserialize_fn_by_path(&component.component_type)
            .ok_or_else(|| {
                PersistenceError::Deserialization(format!(
                    "No deserialize function registered for component type: {}",
                    component.component_type
                ))
            })?;

        // Get insert function for this component type
        let insert_fn = component_registry
            .get_insert_fn_by_path(&component.component_type)
            .ok_or_else(|| {
                PersistenceError::Deserialization(format!(
                    "No insert function registered for component type: {}",
                    component.component_type
                ))
            })?;

        // Deserialize the component from bytes
        let deserialized = deserialize_fn(&component.data).map_err(|e| {
            PersistenceError::Deserialization(format!(
                "Failed to deserialize component {}: {}",
                component.component_type, e
            ))
        })?;

        // Insert the component into the entity
        // Get an EntityWorldMut to pass to the insert function
        let mut entity_mut = world.entity_mut(entity);
        insert_fn(&mut entity_mut, deserialized);

        debug!(
            "Inserted component {} into entity {:?}",
            component.component_type, entity
        );
    }

    // Add the NetworkedEntity component with the persisted network_id
    // This ensures the entity maintains its identity across restarts
    world.entity_mut(entity).insert(NetworkedEntity {
        network_id: loaded_entity.id,
        owner_node_id: uuid::Uuid::nil(), // Will be set by network system if needed
    });

    // Add the Persisted marker component
    world
        .entity_mut(entity)
        .insert(crate::persistence::Persisted {
            network_id: loaded_entity.id,
        });

    info!(
        "Successfully rehydrated entity {:?} as Bevy entity {:?}",
        loaded_entity.id, entity
    );

    Ok(entity)
}

/// Rehydrate all entities from the database into the Bevy world
///
/// This function is called during startup to restore the entire persisted
/// state. It loads all entities from SQLite and spawns them into the Bevy world
/// with all their components.
///
/// # Arguments
///
/// * `world` - The Bevy world to spawn entities into
///
/// # Errors
///
/// Returns an error if:
/// - Database connection fails
/// - Entity loading fails
/// - Entity rehydration fails
pub fn rehydrate_all_entities(world: &mut bevy::prelude::World) -> Result<()> {
    use bevy::prelude::*;

    // Get database connection from resource
    let loaded_entities = {
        let db_res = world.resource::<crate::persistence::PersistenceDb>();
        let conn = db_res
            .conn
            .lock()
            .map_err(|e| PersistenceError::Other(format!("Failed to lock database: {}", e)))?;

        // Load all entities from database
        load_all_entities(&conn)?
    };

    info!("Loaded {} entities from database", loaded_entities.len());

    if loaded_entities.is_empty() {
        info!("No entities to rehydrate");
        return Ok(());
    }

    // Get component registry
    let component_registry = {
        let registry_res = world.resource::<crate::persistence::ComponentTypeRegistryResource>();
        registry_res.0
    };

    // Rehydrate each entity
    let mut rehydrated_count = 0;
    let mut failed_count = 0;

    for loaded_entity in loaded_entities {
        match rehydrate_entity(loaded_entity, world, component_registry) {
            | Ok(entity) => {
                rehydrated_count += 1;
                debug!("Rehydrated entity {:?}", entity);
            },
            | Err(e) => {
                failed_count += 1;
                error!("Failed to rehydrate entity: {}", e);
            },
        }
    }

    info!(
        "Entity rehydration complete: {} succeeded, {} failed",
        rehydrated_count, failed_count
    );

    if failed_count > 0 {
        warn!(
            "{} entities failed to rehydrate - check logs for details",
            failed_count
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_initialization() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        configure_sqlite_for_persistence(&conn)?;
        create_persistence_schema(&conn)?;

        // Verify tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")?
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        assert!(tables.contains(&"entities".to_string()));
        assert!(tables.contains(&"components".to_string()));
        assert!(tables.contains(&"operation_log".to_string()));
        assert!(tables.contains(&"vector_clock".to_string()));

        Ok(())
    }

    #[test]
    fn test_flush_operations() -> Result<()> {
        let mut conn = Connection::open_in_memory()?;
        create_persistence_schema(&conn)?;

        let entity_id = uuid::Uuid::new_v4();
        let ops = vec![
            PersistenceOp::UpsertEntity {
                id: entity_id,
                data: EntityData {
                    id: entity_id,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    entity_type: "TestEntity".to_string(),
                },
            },
            PersistenceOp::UpsertComponent {
                entity_id,
                component_type: "Transform".to_string(),
                data: bytes::Bytes::from(vec![1, 2, 3, 4]),
            },
        ];

        let count = flush_to_sqlite(&ops, &mut conn)?;
        assert_eq!(count, 2);

        // Verify entity exists
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM entities WHERE id = ?1",
            rusqlite::params![entity_id.as_bytes()],
            |row| row.get(0),
        )?;
        assert!(exists);

        Ok(())
    }

    #[test]
    fn test_session_state() -> Result<()> {
        let mut conn = Connection::open_in_memory()?;
        create_persistence_schema(&conn)?;

        set_session_state(&mut conn, "test_key", "test_value")?;
        let value = get_session_state(&conn, "test_key")?;
        assert_eq!(value, Some("test_value".to_string()));

        Ok(())
    }

    #[test]
    fn test_crash_recovery() -> Result<()> {
        let mut conn = Connection::open_in_memory()?;
        create_persistence_schema(&conn)?;

        // Simulate first startup - should report as crash (no clean shutdown marker)
        let clean = check_clean_shutdown(&mut conn)?;
        assert!(!clean, "First startup should be detected as crash");

        // Mark clean shutdown
        mark_clean_shutdown(&mut conn)?;

        // Next startup should report clean shutdown
        let clean = check_clean_shutdown(&mut conn)?;
        assert!(clean, "Should detect clean shutdown");

        // After checking clean shutdown, flag should be reset to false
        // So if we check again without marking, it should report as crash
        let value = get_session_state(&conn, "clean_shutdown")?;
        assert_eq!(
            value,
            Some("false".to_string()),
            "Flag should be reset after check"
        );

        Ok(())
    }
}
