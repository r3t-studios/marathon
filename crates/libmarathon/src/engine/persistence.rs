//! Persistence Manager - handles SQLite storage outside Bevy

use std::sync::{
    Arc,
    Mutex,
};

use rusqlite::{
    Connection,
    OptionalExtension,
};

use crate::networking::{
    Session,
    SessionId,
};

pub struct PersistenceManager {
    conn: Arc<Mutex<Connection>>,
}

impl PersistenceManager {
    pub fn new(db_path: &str) -> Self {
        let conn = Connection::open(db_path).expect("Failed to open database");

        // Schema is handled by the migration system in persistence/migrations
        // No hardcoded schema creation here

        Self {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    pub fn save_session(&self, session: &Session) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();

        // Schema from migration 004: id (BLOB), code, name, created_at, last_active,
        // entity_count, state, secret
        conn.execute(
            "INSERT OR REPLACE INTO sessions (id, code, state, created_at, last_active, entity_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (
                session.id.as_uuid().as_bytes(),  // id is BLOB in migration 004
                session.id.to_code(),             // code is the text representation
                format!("{:?}", session.state),
                session.created_at,
                session.last_active,
                0,  // entity_count default
            ),
        )?;

        Ok(())
    }

    pub fn load_last_active_session(&self) -> anyhow::Result<Option<Session>> {
        let conn = self.conn.lock().unwrap();

        // Query for the most recently active session
        // Schema from migration 004: uses last_active (not last_active_at)
        let mut stmt = conn.prepare(
            "SELECT code, state, created_at, last_active
             FROM sessions
             ORDER BY last_active DESC
             LIMIT 1",
        )?;

        let session = stmt
            .query_row([], |row| {
                let code: String = row.get(0)?;
                let _state: String = row.get(1)?;
                let _created_at: i64 = row.get(2)?;
                let _last_active: i64 = row.get(3)?;

                // Parse session ID from code
                if let Ok(session_id) = SessionId::from_code(&code) {
                    Ok(Some(Session::new(session_id)))
                } else {
                    Ok(None)
                }
            })
            .optional()?;

        Ok(session.flatten())
    }
}
