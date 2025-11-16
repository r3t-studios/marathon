use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

/// Represents a message stored in our database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: i64,
    pub chat_db_rowid: i64,
    pub text: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
    pub is_from_me: bool,
    pub created_at: DateTime<Utc>,
}

/// Represents a message embedding (full message vector)
#[derive(Debug, Clone)]
pub struct MessageEmbedding {
    pub id: i64,
    pub message_id: i64,
    pub embedding: Vec<f32>,
    pub model_name: String,
    pub created_at: DateTime<Utc>,
}

/// Represents a word embedding
#[derive(Debug, Clone)]
pub struct WordEmbedding {
    pub id: i64,
    pub word: String,
    pub embedding: Vec<f32>,
    pub model_name: String,
    pub created_at: DateTime<Utc>,
}

/// Represents an emotion classification for a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Emotion {
    pub id: i64,
    pub message_id: i64,
    pub emotion: String,
    pub confidence: f64,
    pub model_version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Represents an emotion training sample
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionTrainingSample {
    pub id: i64,
    pub message_id: Option<i64>,
    pub text: String,
    pub expected_emotion: String,
    pub actual_emotion: Option<String>,
    pub confidence: Option<f64>,
    pub is_validated: bool,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
