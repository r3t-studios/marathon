use rusqlite::{Connection, Result};
use tracing::info;

pub fn initialize_database(conn: &Connection) -> Result<()> {
    info!("Initializing database schema");

    // Load sqlite-vec extension (macOS only)
    let vec_path = "./extensions/vec0.dylib";

    // Try to load the vector extension (non-fatal if it fails for now)
    match unsafe { conn.load_extension_enable() } {
        Ok(_) => {
            match unsafe { conn.load_extension(vec_path, None::<&str>) } {
                Ok(_) => info!("Loaded sqlite-vec extension"),
                Err(e) => info!("Could not load sqlite-vec extension: {}. Vector operations will not be available.", e),
            }
            let _ = unsafe { conn.load_extension_disable() };
        }
        Err(e) => info!("Extension loading not enabled: {}", e),
    }

    // Create messages table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            chat_db_rowid INTEGER UNIQUE NOT NULL,
            text TEXT,
            timestamp INTEGER,
            is_from_me BOOLEAN NOT NULL,
            created_at INTEGER NOT NULL
        )",
        [],
    )?;

    // Create index on chat_db_rowid for fast lookups
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_messages_chat_db_rowid ON messages(chat_db_rowid)",
        [],
    )?;

    // Create message_embeddings table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS message_embeddings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            message_id INTEGER NOT NULL,
            embedding BLOB NOT NULL,
            model_name TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
        )",
        [],
    )?;

    // Create index on message_id
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_message_embeddings_message_id ON message_embeddings(message_id)",
        [],
    )?;

    // Create word_embeddings table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS word_embeddings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            word TEXT UNIQUE NOT NULL,
            embedding BLOB NOT NULL,
            model_name TEXT NOT NULL,
            created_at INTEGER NOT NULL
        )",
        [],
    )?;

    // Create index on word
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_word_embeddings_word ON word_embeddings(word)",
        [],
    )?;

    // Create emotions table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS emotions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            message_id INTEGER NOT NULL,
            emotion TEXT NOT NULL,
            confidence REAL NOT NULL,
            model_version TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
        )",
        [],
    )?;

    // Create indexes for emotions
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_emotions_message_id ON emotions(message_id)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_emotions_emotion ON emotions(emotion)",
        [],
    )?;

    // Create emotions_training_set table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS emotions_training_set (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            message_id INTEGER,
            text TEXT NOT NULL,
            expected_emotion TEXT NOT NULL,
            actual_emotion TEXT,
            confidence REAL,
            is_validated BOOLEAN NOT NULL DEFAULT 0,
            notes TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE SET NULL
        )",
        [],
    )?;

    // Create index on emotions_training_set
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_emotions_training_set_message_id ON emotions_training_set(message_id)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_emotions_training_set_validated ON emotions_training_set(is_validated)",
        [],
    )?;

    // Create state table for daemon state persistence
    conn.execute(
        "CREATE TABLE IF NOT EXISTS daemon_state (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    )?;

    // Create models table for storing ML model files
    conn.execute(
        "CREATE TABLE IF NOT EXISTS models (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT UNIQUE NOT NULL,
            model_type TEXT NOT NULL,
            version TEXT NOT NULL,
            file_data BLOB NOT NULL,
            metadata TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    )?;

    // Create index on model name and type
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_models_name ON models(name)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_models_type ON models(model_type)",
        [],
    )?;

    info!("Database schema initialized successfully");
    Ok(())
}

/// Helper function to serialize f32 vector to bytes for storage
pub fn serialize_embedding(embedding: &[f32]) -> Vec<u8> {
    embedding
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect()
}

/// Helper function to deserialize bytes back to f32 vector
pub fn deserialize_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| {
            let array: [u8; 4] = chunk.try_into().unwrap();
            f32::from_le_bytes(array)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_serialization() {
        let original = vec![1.0f32, 2.5, -3.7, 0.0, 100.5];
        let serialized = serialize_embedding(&original);
        let deserialized = deserialize_embedding(&serialized);

        assert_eq!(original.len(), deserialized.len());
        for (a, b) in original.iter().zip(deserialized.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }
}
