use chrono::{
    TimeZone,
    Utc,
};
use rusqlite::{
    Connection,
    OptionalExtension,
    Result,
    Row,
    params,
};

use crate::{
    db::schema::{
        deserialize_embedding,
        serialize_embedding,
    },
    models::*,
};

/// Insert a new message into the database
pub fn insert_message(conn: &Connection, msg: &lib::Message) -> Result<i64> {
    let timestamp = msg.date.map(|dt| dt.timestamp());
    let created_at = Utc::now().timestamp();

    conn.execute(
        "INSERT INTO messages (chat_db_rowid, text, timestamp, is_from_me, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(chat_db_rowid) DO NOTHING",
        params![msg.rowid, msg.text, timestamp, msg.is_from_me, created_at],
    )?;

    Ok(conn.last_insert_rowid())
}

/// Get message ID by chat.db rowid
pub fn get_message_id_by_chat_rowid(conn: &Connection, chat_db_rowid: i64) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM messages WHERE chat_db_rowid = ?1",
        params![chat_db_rowid],
        |row| row.get(0),
    )
    .optional()
}

/// Get message by ID
pub fn get_message(conn: &Connection, id: i64) -> Result<Message> {
    conn.query_row(
        "SELECT id, chat_db_rowid, text, timestamp, is_from_me, created_at FROM messages WHERE id = ?1",
        params![id],
        map_message_row,
    )
}

fn map_message_row(row: &Row) -> Result<Message> {
    let timestamp: Option<i64> = row.get(3)?;
    let created_at: i64 = row.get(5)?;

    Ok(Message {
        id: row.get(0)?,
        chat_db_rowid: row.get(1)?,
        text: row.get(2)?,
        timestamp: timestamp.map(|ts| Utc.timestamp_opt(ts, 0).unwrap()),
        is_from_me: row.get(4)?,
        created_at: Utc.timestamp_opt(created_at, 0).unwrap(),
    })
}

/// Insert message embedding
pub fn insert_message_embedding(
    conn: &Connection,
    message_id: i64,
    embedding: &[f32],
    model_name: &str,
) -> Result<i64> {
    let embedding_bytes = serialize_embedding(embedding);
    let created_at = Utc::now().timestamp();

    conn.execute(
        "INSERT INTO message_embeddings (message_id, embedding, model_name, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![message_id, embedding_bytes, model_name, created_at],
    )?;

    Ok(conn.last_insert_rowid())
}

/// Get message embedding
pub fn get_message_embedding(
    conn: &Connection,
    message_id: i64,
) -> Result<Option<MessageEmbedding>> {
    conn.query_row(
        "SELECT id, message_id, embedding, model_name, created_at
         FROM message_embeddings WHERE message_id = ?1",
        params![message_id],
        |row| {
            let embedding_bytes: Vec<u8> = row.get(2)?;
            let created_at: i64 = row.get(4)?;

            Ok(MessageEmbedding {
                id: row.get(0)?,
                message_id: row.get(1)?,
                embedding: deserialize_embedding(&embedding_bytes),
                model_name: row.get(3)?,
                created_at: Utc.timestamp_opt(created_at, 0).unwrap(),
            })
        },
    )
    .optional()
}

/// Insert or get word embedding
pub fn insert_word_embedding(
    conn: &Connection,
    word: &str,
    embedding: &[f32],
    model_name: &str,
) -> Result<i64> {
    let embedding_bytes = serialize_embedding(embedding);
    let created_at = Utc::now().timestamp();

    conn.execute(
        "INSERT INTO word_embeddings (word, embedding, model_name, created_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(word) DO NOTHING",
        params![word, embedding_bytes, model_name, created_at],
    )?;

    Ok(conn.last_insert_rowid())
}

/// Get word embedding
pub fn get_word_embedding(conn: &Connection, word: &str) -> Result<Option<WordEmbedding>> {
    conn.query_row(
        "SELECT id, word, embedding, model_name, created_at
         FROM word_embeddings WHERE word = ?1",
        params![word],
        |row| {
            let embedding_bytes: Vec<u8> = row.get(2)?;
            let created_at: i64 = row.get(4)?;

            Ok(WordEmbedding {
                id: row.get(0)?,
                word: row.get(1)?,
                embedding: deserialize_embedding(&embedding_bytes),
                model_name: row.get(3)?,
                created_at: Utc.timestamp_opt(created_at, 0).unwrap(),
            })
        },
    )
    .optional()
}

/// Insert emotion classification
pub fn insert_emotion(
    conn: &Connection,
    message_id: i64,
    emotion: &str,
    confidence: f64,
    model_version: &str,
) -> Result<i64> {
    let now = Utc::now().timestamp();

    conn.execute(
        "INSERT INTO emotions (message_id, emotion, confidence, model_version, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![message_id, emotion, confidence, model_version, now, now],
    )?;

    Ok(conn.last_insert_rowid())
}

/// Update emotion classification
pub fn update_emotion(
    conn: &Connection,
    message_id: i64,
    emotion: &str,
    confidence: f64,
) -> Result<()> {
    let updated_at = Utc::now().timestamp();

    conn.execute(
        "UPDATE emotions SET emotion = ?1, confidence = ?2, updated_at = ?3
         WHERE message_id = ?4",
        params![emotion, confidence, updated_at, message_id],
    )?;

    Ok(())
}

/// Get emotion by message ID
pub fn get_emotion_by_message_id(conn: &Connection, message_id: i64) -> Result<Option<Emotion>> {
    conn.query_row(
        "SELECT id, message_id, emotion, confidence, model_version, created_at, updated_at
         FROM emotions WHERE message_id = ?1",
        params![message_id],
        map_emotion_row,
    )
    .optional()
}

/// Get emotion by ID
pub fn get_emotion_by_id(conn: &Connection, id: i64) -> Result<Option<Emotion>> {
    conn.query_row(
        "SELECT id, message_id, emotion, confidence, model_version, created_at, updated_at
         FROM emotions WHERE id = ?1",
        params![id],
        map_emotion_row,
    )
    .optional()
}

/// List all emotions with optional filters
pub fn list_emotions(
    conn: &Connection,
    emotion_filter: Option<&str>,
    min_confidence: Option<f64>,
    limit: Option<i32>,
    offset: Option<i32>,
) -> Result<Vec<Emotion>> {
    let mut query = String::from(
        "SELECT id, message_id, emotion, confidence, model_version, created_at, updated_at
         FROM emotions WHERE 1=1",
    );

    if emotion_filter.is_some() {
        query.push_str(" AND emotion = ?1");
    }

    if min_confidence.is_some() {
        query.push_str(" AND confidence >= ?2");
    }

    query.push_str(" ORDER BY created_at DESC");

    if limit.is_some() {
        query.push_str(" LIMIT ?3");
    }

    if offset.is_some() {
        query.push_str(" OFFSET ?4");
    }

    let mut stmt = conn.prepare(&query)?;
    let emotions = stmt
        .query_map(
            params![
                emotion_filter.unwrap_or(""),
                min_confidence.unwrap_or(0.0),
                limit.unwrap_or(1000),
                offset.unwrap_or(0),
            ],
            map_emotion_row,
        )?
        .collect::<Result<Vec<_>>>()?;

    Ok(emotions)
}

/// Delete emotion by ID
pub fn delete_emotion(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM emotions WHERE id = ?1", params![id])?;
    Ok(())
}

/// Count total emotions
pub fn count_emotions(conn: &Connection) -> Result<i32> {
    conn.query_row("SELECT COUNT(*) FROM emotions", [], |row| row.get(0))
}

fn map_emotion_row(row: &Row) -> Result<Emotion> {
    let created_at: i64 = row.get(5)?;
    let updated_at: i64 = row.get(6)?;

    Ok(Emotion {
        id: row.get(0)?,
        message_id: row.get(1)?,
        emotion: row.get(2)?,
        confidence: row.get(3)?,
        model_version: row.get(4)?,
        created_at: Utc.timestamp_opt(created_at, 0).unwrap(),
        updated_at: Utc.timestamp_opt(updated_at, 0).unwrap(),
    })
}

/// Insert emotion training sample
pub fn insert_training_sample(
    conn: &Connection,
    message_id: Option<i64>,
    text: &str,
    expected_emotion: &str,
) -> Result<i64> {
    let now = Utc::now().timestamp();

    conn.execute(
        "INSERT INTO emotions_training_set (message_id, text, expected_emotion, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![message_id, text, expected_emotion, now, now],
    )?;

    Ok(conn.last_insert_rowid())
}

/// Get state value from daemon_state table
pub fn get_state(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM daemon_state WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
}

/// Set state value in daemon_state table
pub fn set_state(conn: &Connection, key: &str, value: &str) -> Result<()> {
    let updated_at = Utc::now().timestamp();

    conn.execute(
        "INSERT INTO daemon_state (key, value, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = ?3",
        params![key, value, updated_at],
    )?;

    Ok(())
}

/// Get last processed chat.db rowid from database or return 0
pub fn get_last_processed_rowid(conn: &Connection) -> Result<i64> {
    Ok(get_state(conn, "last_processed_rowid")?
        .and_then(|s| s.parse().ok())
        .unwrap_or(0))
}

/// Save last processed chat.db rowid to database
pub fn save_last_processed_rowid(conn: &Connection, rowid: i64) -> Result<()> {
    set_state(conn, "last_processed_rowid", &rowid.to_string())
}
