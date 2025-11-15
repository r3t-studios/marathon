use crate::error::Result;
use crate::models::*;
use rusqlite::{Connection, OpenFlags, Row, params};

pub struct ChatDb {
    conn: Connection,
}

impl ChatDb {
    /// Open a connection to the chat database in read-only mode
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Ok(Self { conn })
    }

    /// Get messages from the conversation with +31 6 39 13 29 13
    ///
    /// Returns messages from January 1, 2024 to present from the conversation
    /// with the specified Dutch phone number.
    ///
    /// # Arguments
    ///
    /// * `start_date` - Start date (defaults to January 1, 2024 if None)
    /// * `end_date` - End date (defaults to current time if None)
    pub fn get_our_messages(
        &self,
        start_date: Option<chrono::DateTime<chrono::Utc>>,
        end_date: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<Message>> {
        use chrono::{TimeZone, Utc};

        // Default date range: January 1, 2024 to now
        let start =
            start_date.unwrap_or_else(|| Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap());
        let end = end_date.unwrap_or_else(|| Utc::now());

        // Convert to Apple timestamps (nanoseconds since 2001-01-01)
        let start_timestamp = datetime_to_apple_timestamp(start);
        let end_timestamp = datetime_to_apple_timestamp(end);

        // The phone number might be stored with or without spaces
        let phone_with_spaces = "+31 6 39 13 29 13";
        let phone_without_spaces = "+31639132913";

        // Find the chat with this phone number (try both formats)
        let chat = self
            .get_chat_for_phone_number(phone_with_spaces)
            .or_else(|_| self.get_chat_for_phone_number(phone_without_spaces))?;

        // Get messages from this chat within the date range
        let mut stmt = self.conn.prepare(
            "SELECT m.ROWID, m.guid, m.text, m.service, m.handle_id, m.date, m.date_read, m.date_delivered,
                    m.is_from_me, m.is_read, m.is_delivered, m.is_sent, m.is_emote, m.is_audio_message,
                    m.cache_has_attachments, m.associated_message_guid, m.associated_message_type,
                    m.thread_originator_guid, m.reply_to_guid, m.is_spam
             FROM message m
             INNER JOIN chat_message_join cmj ON m.ROWID = cmj.message_id
             WHERE cmj.chat_id = ?
               AND m.date >= ?
               AND m.date <= ?
             ORDER BY m.date ASC"
        )?;

        let messages = stmt
            .query_map(
                params![chat.rowid, start_timestamp, end_timestamp],
                map_message_row,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(messages)
    }

    /// Helper function to find the largest chat with a specific phone number
    fn get_chat_for_phone_number(&self, phone_number: &str) -> Result<Chat> {
        let mut stmt = self.conn.prepare(
            "SELECT c.ROWID, c.guid, c.chat_identifier, c.service_name, c.display_name,
                    c.group_id, c.room_name, c.is_archived, c.is_filtered,
                    c.last_read_message_timestamp, COUNT(cmj.message_id) as msg_count
             FROM chat c
             INNER JOIN chat_handle_join chj ON c.ROWID = chj.chat_id
             INNER JOIN handle h ON chj.handle_id = h.ROWID
             INNER JOIN chat_message_join cmj ON c.ROWID = cmj.chat_id
             WHERE h.id = ?
             GROUP BY c.ROWID
             ORDER BY msg_count DESC
             LIMIT 1"
        )?;

        let chat = stmt.query_row(params![phone_number], |row| {
            Ok(Chat {
                rowid: row.get(0)?,
                guid: row.get(1)?,
                chat_identifier: row.get(2)?,
                service_name: row.get(3)?,
                display_name: row.get(4)?,
                group_id: row.get(5)?,
                room_name: row.get(6)?,
                is_archived: row.get::<_, i64>(7)? != 0,
                is_filtered: row.get::<_, i64>(8)? != 0,
                last_read_message_timestamp: row.get::<_, Option<i64>>(9)?.map(apple_timestamp_to_datetime),
            })
        })?;

        Ok(chat)
    }
}

// Helper function to map database rows to structs
fn map_message_row(row: &Row) -> rusqlite::Result<Message> {
    Ok(Message {
        rowid: row.get(0)?,
        guid: row.get(1)?,
        text: row.get(2)?,
        service: row.get(3)?,
        handle_id: row.get(4)?,
        date: row
            .get::<_, Option<i64>>(5)?
            .map(apple_timestamp_to_datetime),
        date_read: row
            .get::<_, Option<i64>>(6)?
            .map(apple_timestamp_to_datetime),
        date_delivered: row
            .get::<_, Option<i64>>(7)?
            .map(apple_timestamp_to_datetime),
        is_from_me: row.get::<_, i64>(8)? != 0,
        is_read: row.get::<_, i64>(9)? != 0,
        is_delivered: row.get::<_, i64>(10)? != 0,
        is_sent: row.get::<_, i64>(11)? != 0,
        is_emote: row.get::<_, i64>(12)? != 0,
        is_audio_message: row.get::<_, i64>(13)? != 0,
        cache_has_attachments: row.get::<_, i64>(14)? != 0,
        associated_message_guid: row.get(15)?,
        associated_message_type: row.get(16)?,
        thread_originator_guid: row.get(17)?,
        reply_to_guid: row.get(18)?,
        is_spam: row.get::<_, i64>(19)? != 0,
    })
}
