#!/usr/bin/env -S cargo +nightly -Zscript
---
[dependencies]
rusqlite = { version = "0.37.0", features = ["bundled"] }
csv = "1.3"
chrono = "0.4"
plist = "1.8"
ns-keyed-archive = "0.1.4"
anyhow = "1.0"
---

use rusqlite::{Connection, OpenFlags};
use std::fs::File;
use csv::Writer;
use chrono::{DateTime, Utc};
use anyhow::Result;
use ns_keyed_archive::decode::from_bytes as decode_keyed_archive;

const PHONE_NUMBER: &str = "+31639132913";
const COCOA_EPOCH_OFFSET: i64 = 978307200;

fn cocoa_timestamp_to_datetime(timestamp: i64) -> String {
    if timestamp == 0 {
        return String::new();
    }

    let seconds_since_2001 = timestamp / 1_000_000_000;
    let nanoseconds = (timestamp % 1_000_000_000) as u32;
    let unix_timestamp = COCOA_EPOCH_OFFSET + seconds_since_2001;

    DateTime::from_timestamp(unix_timestamp, nanoseconds)
        .map(|dt: DateTime<Utc>| dt.to_rfc3339())
        .unwrap_or_default()
}

fn extract_text_from_attributed_body(attributed_body: &[u8]) -> String {
    if attributed_body.is_empty() {
        return String::new();
    }

    // Try to parse as NSKeyedArchiver using the specialized crate
    match decode_keyed_archive(attributed_body) {
        Ok(value) => {
            // Try to extract the string value from the decoded archive
            if let Some(s) = extract_string_from_value(&value) {
                return s;
            }
        }
        Err(_) => {
            // If ns-keyed-archive fails, try regular plist parsing
            if let Ok(value) = plist::from_bytes::<plist::Value>(attributed_body) {
                if let Some(dict) = value.as_dictionary() {
                    if let Some(objects) = dict.get("$objects").and_then(|v| v.as_array()) {
                        for obj in objects {
                            if let Some(s) = obj.as_string() {
                                if !s.is_empty()
                                    && s != "$null"
                                    && !s.starts_with("NS")
                                    && !s.starts_with("__k")
                                {
                                    return s.to_string();
                                }
                            }
                        }
                    }
                }
            }

            // Last resort: simple string extraction
            return extract_text_fallback(attributed_body);
        }
    }

    String::new()
}

fn extract_string_from_value(value: &plist::Value) -> Option<String> {
    match value {
        plist::Value::String(s) => Some(s.clone()),
        plist::Value::Dictionary(dict) => {
            // Look for common NSAttributedString keys
            for key in &["NSString", "NS.string", "string"] {
                if let Some(val) = dict.get(*key) {
                    if let Some(s) = extract_string_from_value(val) {
                        return Some(s);
                    }
                }
            }
            None
        }
        plist::Value::Array(arr) => {
            // Find first non-empty string in array
            for item in arr {
                if let Some(s) = extract_string_from_value(item) {
                    if !s.is_empty() && !s.starts_with("NS") && !s.starts_with("__k") {
                        return Some(s);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn extract_text_fallback(attributed_body: &[u8]) -> String {
    // Simple fallback: extract printable ASCII strings
    let mut current_str = String::new();
    let mut best_string = String::new();

    for &byte in attributed_body {
        if (32..127).contains(&byte) {
            current_str.push(byte as char);
        } else {
            if current_str.len() > best_string.len()
                && !current_str.starts_with("NS")
                && !current_str.starts_with("__k")
                && current_str != "streamtyped"
                && current_str != "NSDictionary"
            {
                best_string = current_str.clone();
            }
            current_str.clear();
        }
    }

    // Check final string
    if current_str.len() > best_string.len() {
        best_string = current_str;
    }

    // Clean up common artifacts
    best_string = best_string.trim_start_matches(|c: char| {
        c == '+' && best_string.len() > 2
    }).trim().to_string();

    best_string
}

fn main() -> Result<()> {
    let home = std::env::var("HOME")?;
    let chat_db_path = format!("{}/Library/Messages/chat.db", home);
    let conn = Connection::open_with_flags(&chat_db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;

    let mut stmt = conn.prepare(
        "SELECT
            m.ROWID,
            m.text,
            m.attributedBody,
            m.date,
            m.date_read,
            m.date_delivered,
            m.is_from_me,
            m.is_read,
            COALESCE(h.id, 'unknown') as handle_id,
            c.chat_identifier,
            m.service
         FROM message m
         LEFT JOIN handle h ON m.handle_id = h.ROWID
         LEFT JOIN chat_message_join cmj ON m.ROWID = cmj.message_id
         LEFT JOIN chat c ON cmj.chat_id = c.ROWID
         WHERE h.id = ?1 OR c.chat_identifier = ?1
         ORDER BY m.date ASC",
    )?;

    let messages = stmt.query_map([PHONE_NUMBER], |row| {
        Ok((
            row.get::<_, i64>(0)?,                      // ROWID
            row.get::<_, Option<String>>(1)?,           // text
            row.get::<_, Option<Vec<u8>>>(2)?,          // attributedBody
            row.get::<_, i64>(3)?,                      // date
            row.get::<_, Option<i64>>(4)?,              // date_read
            row.get::<_, Option<i64>>(5)?,              // date_delivered
            row.get::<_, i32>(6)?,                      // is_from_me
            row.get::<_, i32>(7)?,                      // is_read
            row.get::<_, String>(8)?,                   // handle_id
            row.get::<_, Option<String>>(9)?,           // chat_identifier
            row.get::<_, Option<String>>(10)?,          // service
        ))
    })?;

    let file = File::create("lonni_messages.csv")?;
    let mut wtr = Writer::from_writer(file);

    wtr.write_record(&[
        "id",
        "date",
        "date_read",
        "date_delivered",
        "is_from_me",
        "is_read",
        "handle",
        "chat_identifier",
        "service",
        "text",
    ])?;

    let mut count = 0;
    for message in messages {
        let (
            rowid,
            text,
            attributed_body,
            date,
            date_read,
            date_delivered,
            is_from_me,
            is_read,
            handle_id,
            chat_identifier,
            service,
        ) = message?;

        // Extract text from attributedBody if text field is empty
        let message_text = text.unwrap_or_else(|| {
            attributed_body
                .as_ref()
                .map(|body| extract_text_from_attributed_body(body))
                .unwrap_or_default()
        });

        wtr.write_record(&[
            rowid.to_string(),
            cocoa_timestamp_to_datetime(date),
            date_read.map(cocoa_timestamp_to_datetime).unwrap_or_default(),
            date_delivered.map(cocoa_timestamp_to_datetime).unwrap_or_default(),
            is_from_me.to_string(),
            is_read.to_string(),
            handle_id,
            chat_identifier.unwrap_or_default(),
            service.unwrap_or_default(),
            message_text,
        ])?;

        count += 1;
        if count % 1000 == 0 {
            println!("Exported {} messages...", count);
        }
    }

    wtr.flush()?;
    println!("Successfully exported {} messages to lonni_messages.csv", count);

    Ok(())
}
