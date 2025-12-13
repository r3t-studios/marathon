//! Persistence Manager - handles SQLite storage outside Bevy

use rusqlite::{Connection, OptionalExtension};
use std::sync::{Arc, Mutex};

use crate::networking::{Session, SessionId};

pub struct PersistenceManager {
    conn: Arc<Mutex<Connection>>,
}

impl PersistenceManager {
    pub fn new(db_path: &str) -> Self {
        let conn = Connection::open(db_path).expect("Failed to open database");

        // Initialize schema (Phase 1 stub - will load from file in Phase 4)
        let schema = "
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                state TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                last_active_at INTEGER NOT NULL
            );
        ";

        if let Err(e) = conn.execute_batch(schema) {
            tracing::warn!("Failed to initialize schema: {}", e);
        }

        Self {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    pub fn save_session(&self, session: &Session) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "INSERT OR REPLACE INTO sessions (id, state, created_at, last_active_at)
             VALUES (?1, ?2, ?3, ?4)",
            (
                session.id.to_code(),
                format!("{:?}", session.state),
                session.created_at,
                session.last_active,
            ),
        )?;

        Ok(())
    }

    pub fn load_last_active_session(&self) -> anyhow::Result<Option<Session>> {
        let conn = self.conn.lock().unwrap();

        // Query for the most recently active session
        let mut stmt = conn.prepare(
            "SELECT id, state, created_at, last_active_at
             FROM sessions
             ORDER BY last_active_at DESC
             LIMIT 1"
        )?;

        let session = stmt.query_row([], |row| {
            let id_code: String = row.get(0)?;
            let _state: String = row.get(1)?;
            let _created_at: String = row.get(2)?;
            let _last_active_at: String = row.get(3)?;

            // Parse session ID from code
            if let Ok(session_id) = SessionId::from_code(&id_code) {
                Ok(Some(Session::new(session_id)))
            } else {
                Ok(None)
            }
        }).optional()?;

        Ok(session.flatten())
    }
}
