use crate::db;
use anyhow::Result;
use rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{error, info, warn};

/// Service responsible for generating embeddings for messages and words
pub struct EmbeddingService {
    us_db: Arc<Mutex<Connection>>,
    rx: mpsc::Receiver<lib::Message>,
    model_name: String,
}

impl EmbeddingService {
    pub fn new(
        us_db: Arc<Mutex<Connection>>,
        rx: mpsc::Receiver<lib::Message>,
        model_name: String,
    ) -> Self {
        Self {
            us_db,
            rx,
            model_name,
        }
    }

    pub async fn run(mut self) -> Result<()> {
        info!("Starting embedding service with model: {}", self.model_name);

        // TODO: Load the embedding model here
        // For now, we'll create a placeholder implementation
        info!("Loading embedding model...");
        // let model = load_embedding_model(&self.model_name)?;
        info!("Embedding model loaded (placeholder)");

        while let Some(msg) = self.rx.recv().await {
            if let Err(e) = self.process_message(&msg).await {
                error!("Error processing message {}: {}", msg.rowid, e);
            }
        }

        Ok(())
    }

    async fn process_message(&self, msg: &lib::Message) -> Result<()> {
        // Get message ID from our database
        let us_db = self.us_db.lock().await;
        let message_id = match db::get_message_id_by_chat_rowid(&us_db, msg.rowid)? {
            Some(id) => id,
            None => {
                warn!("Message {} not found in database, skipping", msg.rowid);
                return Ok(());
            }
        };

        // Check if embedding already exists
        if db::get_message_embedding(&us_db, message_id)?.is_some() {
            return Ok(());
        }

        // Skip if message has no text
        let text = match &msg.text {
            Some(t) if !t.is_empty() => t,
            _ => return Ok(()),
        };

        drop(us_db);

        // Generate embedding for the full message
        // TODO: Replace with actual model inference
        let message_embedding = self.generate_embedding(text)?;

        // Store message embedding
        let us_db = self.us_db.lock().await;
        db::insert_message_embedding(&us_db, message_id, &message_embedding, &self.model_name)?;

        // Tokenize and generate word embeddings
        let words = self.tokenize(text);
        for word in words {
            // Check if word embedding exists
            if db::get_word_embedding(&us_db, &word)?.is_none() {
                // Generate embedding for word
                let word_embedding = self.generate_embedding(&word)?;
                db::insert_word_embedding(&us_db, &word, &word_embedding, &self.model_name)?;
            }
        }

        drop(us_db);
        info!("Generated embeddings for message {}", msg.rowid);

        Ok(())
    }

    fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
        // TODO: Replace with actual model inference using Candle
        // For now, return a placeholder embedding of dimension 1024
        let embedding = vec![0.0f32; 1024];
        Ok(embedding)
    }

    fn tokenize(&self, text: &str) -> Vec<String> {
        // Simple word tokenization (split on whitespace and punctuation)
        // TODO: Replace with proper tokenizer
        text.split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase())
            .collect()
    }
}
