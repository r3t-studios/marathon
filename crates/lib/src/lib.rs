//! Data access layer for iMessage chat.db
//!
//! This library provides a read-only interface to query messages from a
//! specific conversation.
//!
//! # Safety
//!
//! All database connections are opened in read-only mode to prevent any
//! accidental modifications to your iMessage database.
//!
//! # Example
//!
//! ```no_run
//! use lib::ChatDb;
//!
//! let db = ChatDb::open("chat.db")?;
//!
//! // Get all messages from January 2024 to now
//! let messages = db.get_our_messages(None, None)?;
//! println!("Found {} messages", messages.len());
//! # Ok::<(), lib::ChatDbError>(())
//! ```

mod db;
mod error;
mod models;
pub mod persistence;
pub mod sync;

pub use db::ChatDb;
pub use error::{
    ChatDbError,
    Result,
};
pub use models::{
    Chat,
    Message,
};
