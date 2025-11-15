use crate::db;
use anyhow::Result;
use rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{error, info, warn};

/// Service responsible for classifying emotions in messages
pub struct EmotionService {
    us_db: Arc<Mutex<Connection>>,
    rx: mpsc::Receiver<lib::Message>,
    model_version: String,
    training_sample_rate: f64,
}

impl EmotionService {
    pub fn new(
        us_db: Arc<Mutex<Connection>>,
        rx: mpsc::Receiver<lib::Message>,
        model_version: String,
        training_sample_rate: f64,
    ) -> Self {
        Self {
            us_db,
            rx,
            model_version,
            training_sample_rate,
        }
    }

    pub async fn run(mut self) -> Result<()> {
        info!(
            "Starting emotion classification service with model: {}",
            self.model_version
        );
        info!(
            "Training sample rate: {:.2}%",
            self.training_sample_rate * 100.0
        );

        // TODO: Load the RoBERTa emotion classification model here
        info!("Loading RoBERTa-base-go_emotions model...");
        // let model = load_emotion_model(&self.model_version)?;
        info!("Emotion model loaded (placeholder)");

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

        // Check if emotion classification already exists
        if db::get_emotion_by_message_id(&us_db, message_id)?.is_some() {
            return Ok(());
        }

        // Skip if message has no text
        let text = match &msg.text {
            Some(t) if !t.is_empty() => t,
            _ => return Ok(()),
        };

        drop(us_db);

        // Classify emotion
        // TODO: Replace with actual model inference
        let (emotion, confidence) = self.classify_emotion(text)?;

        // Store emotion classification
        let us_db = self.us_db.lock().await;
        db::insert_emotion(&us_db, message_id, &emotion, confidence, &self.model_version)?;

        // Randomly add to training set based on sample rate
        if rand::random::<f64>() < self.training_sample_rate {
            db::insert_training_sample(&us_db, Some(message_id), text, &emotion)?;
            info!(
                "Added message {} to training set (emotion: {})",
                msg.rowid, emotion
            );
        }

        drop(us_db);
        info!(
            "Classified message {} as {} (confidence: {:.2})",
            msg.rowid, emotion, confidence
        );

        Ok(())
    }

    fn classify_emotion(&self, text: &str) -> Result<(String, f64)> {
        // TODO: Replace with actual RoBERTa-base-go_emotions inference using Candle
        // The model outputs probabilities for 28 emotions:
        // admiration, amusement, anger, annoyance, approval, caring, confusion,
        // curiosity, desire, disappointment, disapproval, disgust, embarrassment,
        // excitement, fear, gratitude, grief, joy, love, nervousness, optimism,
        // pride, realization, relief, remorse, sadness, surprise, neutral

        // For now, return a placeholder
        let emotion = "neutral".to_string();
        let confidence = 0.85;

        Ok((emotion, confidence))
    }
}
