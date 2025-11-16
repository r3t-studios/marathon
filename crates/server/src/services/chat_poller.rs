use std::{
    path::Path,
    sync::Arc,
    time::Duration,
};

use anyhow::{
    Context,
    Result,
};
use chrono::Utc;
use rusqlite::Connection;
use tokio::{
    sync::{
        Mutex,
        mpsc,
    },
    time,
};
use tracing::{
    debug,
    error,
    info,
    warn,
};

use crate::db;

pub struct ChatPollerService {
    chat_db_path: String,
    us_db: Arc<Mutex<Connection>>,
    tx: mpsc::Sender<lib::Message>,
    poll_interval: Duration,
}

impl ChatPollerService {
    pub fn new(
        chat_db_path: String,
        us_db: Arc<Mutex<Connection>>,
        tx: mpsc::Sender<lib::Message>,
        poll_interval_ms: u64,
    ) -> Self {
        Self {
            chat_db_path,
            us_db,
            tx,
            poll_interval: Duration::from_millis(poll_interval_ms),
        }
    }

    pub async fn run(&self) -> Result<()> {
        info!("Starting chat poller service");
        info!(
            "Polling {} every {:?}",
            self.chat_db_path, self.poll_interval
        );

        // Get last processed rowid from database
        let us_db = self.us_db.lock().await;
        let mut last_rowid =
            db::get_last_processed_rowid(&us_db).context("Failed to get last processed rowid")?;
        drop(us_db);

        info!("Starting from rowid: {}", last_rowid);

        let mut interval = time::interval(self.poll_interval);

        loop {
            interval.tick().await;

            match self.poll_messages(last_rowid).await {
                | Ok(new_messages) => {
                    if !new_messages.is_empty() {
                        info!("Found {} new messages", new_messages.len());

                        for msg in new_messages {
                            // Update last_rowid
                            if msg.rowid > last_rowid {
                                last_rowid = msg.rowid;
                            }

                            // Send message to processing pipeline
                            if let Err(e) = self.tx.send(msg).await {
                                error!("Failed to send message to processing pipeline: {}", e);
                            }
                        }

                        // Save state to database
                        let us_db = self.us_db.lock().await;
                        if let Err(e) = db::save_last_processed_rowid(&us_db, last_rowid) {
                            warn!("Failed to save last processed rowid: {}", e);
                        }
                        drop(us_db);
                    } else {
                        debug!("No new messages");
                    }
                },
                | Err(e) => {
                    error!("Error polling messages: {}", e);
                },
            }
        }
    }

    async fn poll_messages(&self, last_rowid: i64) -> Result<Vec<lib::Message>> {
        // Check if chat.db exists
        if !Path::new(&self.chat_db_path).exists() {
            return Err(anyhow::anyhow!(
                "chat.db not found at {}",
                self.chat_db_path
            ));
        }

        // Open chat.db (read-only)
        let chat_db = lib::ChatDb::open(&self.chat_db_path).context("Failed to open chat.db")?;

        // Get messages with rowid > last_rowid
        // We'll use the existing get_our_messages but need to filter by rowid
        // For now, let's get recent messages and filter in-memory
        let start_date = Some(Utc::now() - chrono::Duration::days(7));
        let end_date = Some(Utc::now());

        let messages = chat_db
            .get_our_messages(start_date, end_date)
            .context("Failed to get messages from chat.db")?;

        // Filter messages with rowid > last_rowid and ensure they're not duplicates
        let new_messages: Vec<lib::Message> = messages
            .into_iter()
            .filter(|msg| msg.rowid > last_rowid)
            .collect();

        // Insert new messages into our database
        let us_db = self.us_db.lock().await;
        for msg in &new_messages {
            if let Err(e) = db::insert_message(&us_db, msg) {
                warn!("Failed to insert message {}: {}", msg.rowid, e);
            }
        }

        Ok(new_messages)
    }
}
