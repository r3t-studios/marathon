use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents a message in the iMessage database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub rowid: i64,
    pub guid: String,
    pub text: Option<String>,
    pub service: Option<String>,
    pub handle_id: i64,
    pub date: Option<DateTime<Utc>>,
    pub date_read: Option<DateTime<Utc>>,
    pub date_delivered: Option<DateTime<Utc>>,
    pub is_from_me: bool,
    pub is_read: bool,
    pub is_delivered: bool,
    pub is_sent: bool,
    pub is_emote: bool,
    pub is_audio_message: bool,
    pub cache_has_attachments: bool,
    pub associated_message_guid: Option<String>,
    pub associated_message_type: i64,
    pub thread_originator_guid: Option<String>,
    pub reply_to_guid: Option<String>,
    pub is_spam: bool,
}

/// Represents a chat/conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chat {
    pub rowid: i64,
    pub guid: String,
    pub chat_identifier: Option<String>,
    pub service_name: Option<String>,
    pub display_name: Option<String>,
    pub group_id: Option<String>,
    pub room_name: Option<String>,
    pub is_archived: bool,
    pub is_filtered: bool,
    pub last_read_message_timestamp: Option<DateTime<Utc>>,
}

/// Helper function to convert Apple's Cocoa timestamp (seconds since 2001-01-01) to DateTime
pub fn apple_timestamp_to_datetime(timestamp: i64) -> DateTime<Utc> {
    // Apple's Cocoa timestamps are in nanoseconds since 2001-01-01 00:00:00 UTC
    // Convert to Unix timestamp (seconds since 1970-01-01 00:00:00 UTC)
    const APPLE_EPOCH_OFFSET: i64 = 978307200; // Seconds between 1970-01-01 and 2001-01-01

    let seconds = timestamp / 1_000_000_000 + APPLE_EPOCH_OFFSET;
    let nanos = (timestamp % 1_000_000_000) as u32;

    DateTime::from_timestamp(seconds, nanos).unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap())
}

/// Helper function to convert DateTime to Apple's Cocoa timestamp
pub fn datetime_to_apple_timestamp(dt: DateTime<Utc>) -> i64 {
    const APPLE_EPOCH_OFFSET: i64 = 978307200;

    let unix_timestamp = dt.timestamp();
    let nanos = dt.timestamp_subsec_nanos() as i64;

    (unix_timestamp - APPLE_EPOCH_OFFSET) * 1_000_000_000 + nanos
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, TimeZone, Timelike};

    #[test]
    fn test_apple_timestamp_to_datetime_zero() {
        let dt = apple_timestamp_to_datetime(0);
        assert_eq!(dt.year(), 2001);
        assert_eq!(dt.month(), 1);
        assert_eq!(dt.day(), 1);
        assert_eq!(dt.hour(), 0);
        assert_eq!(dt.minute(), 0);
        assert_eq!(dt.second(), 0);
    }

    #[test]
    fn test_apple_timestamp_to_datetime_known_value() {
        let timestamp = 694224000000000000i64;
        let dt = apple_timestamp_to_datetime(timestamp);
        assert_eq!(dt.year(), 2023);
        assert_eq!(dt.month(), 1);
        assert_eq!(dt.day(), 1);
    }

    #[test]
    fn test_apple_timestamp_roundtrip() {
        let original = 694224000000000000i64;
        let dt = apple_timestamp_to_datetime(original);
        let converted_back = datetime_to_apple_timestamp(dt);
        assert_eq!(original, converted_back);
    }

    #[test]
    fn test_datetime_to_apple_timestamp_epoch() {
        let dt = Utc.with_ymd_and_hms(2001, 1, 1, 0, 0, 0).unwrap();
        let timestamp = datetime_to_apple_timestamp(dt);
        assert_eq!(timestamp, 0);
    }

    #[test]
    fn test_negative_apple_timestamp() {
        let timestamp = -31536000000000000i64;
        let dt = apple_timestamp_to_datetime(timestamp);
        assert_eq!(dt.year(), 2000);
    }
}
