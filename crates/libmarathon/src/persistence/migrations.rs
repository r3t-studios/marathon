//! Database migration system
//!
//! Provides versioned schema migrations for SQLite database evolution.

use rusqlite::Connection;

use crate::persistence::error::Result;

/// Migration metadata
#[derive(Debug, Clone)]
pub struct Migration {
    /// Migration version number
    pub version: i64,
    /// Migration name/description
    pub name: &'static str,
    /// SQL statements to apply
    pub up: &'static str,
}

/// All available migrations in order
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial_schema",
        up: include_str!("migrations/001_initial_schema.sql"),
    },
    Migration {
        version: 4,
        name: "sessions",
        up: include_str!("migrations/004_sessions.sql"),
    },
    Migration {
        version: 5,
        name: "tombstones",
        up: include_str!("migrations/005_tombstones.sql"),
    },
];

/// Initialize the migrations table
fn create_migrations_table(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at INTEGER NOT NULL
        )",
        [],
    )?;
    Ok(())
}

/// Get the current schema version
pub fn get_current_version(conn: &Connection) -> Result<i64> {
    create_migrations_table(conn)?;

    let version = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    Ok(version)
}

/// Check if a migration has been applied
fn is_migration_applied(conn: &Connection, version: i64) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM schema_migrations WHERE version = ?1",
        [version],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Apply a single migration
fn apply_migration(conn: &mut Connection, migration: &Migration) -> Result<()> {
    tracing::info!(
        "Applying migration {} ({})",
        migration.version,
        migration.name
    );

    let tx = conn.transaction()?;

    // Execute the migration SQL
    tx.execute_batch(migration.up)?;

    // Record that we applied this migration
    tx.execute(
        "INSERT INTO schema_migrations (version, name, applied_at)
         VALUES (?1, ?2, ?3)",
        rusqlite::params![
            migration.version,
            migration.name,
            chrono::Utc::now().timestamp(),
        ],
    )?;

    tx.commit()?;

    tracing::info!(
        "Migration {} ({}) applied successfully",
        migration.version,
        migration.name
    );

    Ok(())
}

/// Run all pending migrations
pub fn run_migrations(conn: &mut Connection) -> Result<()> {
    create_migrations_table(conn)?;

    let current_version = get_current_version(conn)?;
    tracing::info!("Current schema version: {}", current_version);

    let mut applied_count = 0;

    for migration in MIGRATIONS {
        if !is_migration_applied(conn, migration.version)? {
            apply_migration(conn, migration)?;
            applied_count += 1;
        }
    }

    if applied_count > 0 {
        tracing::info!("Applied {} migration(s)", applied_count);
    } else {
        tracing::debug!("No pending migrations");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::*;

    #[test]
    fn test_migration_system() {
        let mut conn = Connection::open_in_memory().unwrap();

        // Initially at version 0
        assert_eq!(get_current_version(&conn).unwrap(), 0);

        // Run migrations
        run_migrations(&mut conn).unwrap();

        // Should be at latest version
        let latest_version = MIGRATIONS.last().unwrap().version;
        assert_eq!(get_current_version(&conn).unwrap(), latest_version);

        // Running again should be a no-op
        run_migrations(&mut conn).unwrap();
        assert_eq!(get_current_version(&conn).unwrap(), latest_version);
    }

    #[test]
    fn test_migrations_table_created() {
        let conn = Connection::open_in_memory().unwrap();
        create_migrations_table(&conn).unwrap();

        // Should be able to query the table
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_is_migration_applied() {
        let conn = Connection::open_in_memory().unwrap();
        create_migrations_table(&conn).unwrap();

        // Migration 1 should not be applied yet
        assert!(!is_migration_applied(&conn, 1).unwrap());

        // Apply migration 1
        conn.execute(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (1, 'test', 0)",
            [],
        )
        .unwrap();

        // Now it should be applied
        assert!(is_migration_applied(&conn, 1).unwrap());
    }
}
