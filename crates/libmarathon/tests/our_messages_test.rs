use chrono::Datelike;
use libmarathon::{
    ChatDb,
    Result,
};

/// Test that we can get messages from the Dutch phone number conversation
#[test]
fn test_get_our_messages_default_range() -> Result<()> {
    let db = ChatDb::open("chat.db")?;

    // Get messages from January 2024 to now (default)
    let messages = db.get_our_messages(None, None)?;

    println!("Found {} messages from January 2024 to now", messages.len());

    // Verify we got some messages
    assert!(
        messages.len() > 0,
        "Should find messages in the conversation"
    );

    // Verify messages are in chronological order (ASC)
    for i in 1..messages.len().min(10) {
        if let (Some(prev_date), Some(curr_date)) = (messages[i - 1].date, messages[i].date) {
            assert!(
                prev_date <= curr_date,
                "Messages should be in ascending date order"
            );
        }
    }

    // Verify all messages are from 2024 or later
    for msg in messages.iter().take(10) {
        if let Some(date) = msg.date {
            assert!(date.year() >= 2024, "Messages should be from 2024 or later");
            println!(
                "Message date: {}, from_me: {}, text: {:?}",
                date,
                msg.is_from_me,
                msg.text.as_ref().map(|s| &s[..s.len().min(50)])
            );
        }
    }

    Ok(())
}

/// Test that we can get messages with a custom date range
#[test]
fn test_get_our_messages_custom_range() -> Result<()> {
    use chrono::{
        TimeZone,
        Utc,
    };

    let db = ChatDb::open("chat.db")?;

    // Get messages from March 2024 to June 2024
    let start = Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();

    let messages = db.get_our_messages(Some(start), Some(end))?;

    println!("Found {} messages from March to June 2024", messages.len());

    // Verify all messages are within the date range
    for msg in &messages {
        if let Some(date) = msg.date {
            assert!(
                date >= start && date <= end,
                "Message date {} should be between {} and {}",
                date,
                start,
                end
            );
        }
    }

    Ok(())
}

/// Test displaying a summary of the conversation
#[test]
fn test_conversation_summary() -> Result<()> {
    let db = ChatDb::open("chat.db")?;

    let messages = db.get_our_messages(None, None)?;

    println!("\n=== Conversation Summary ===");
    println!("Total messages: {}", messages.len());

    let from_me = messages.iter().filter(|m| m.is_from_me).count();
    let from_them = messages.len() - from_me;

    println!("From me: {}", from_me);
    println!("From them: {}", from_them);

    // Show first few messages
    println!("\nFirst 5 messages:");
    for (i, msg) in messages.iter().take(5).enumerate() {
        if let Some(date) = msg.date {
            let sender = if msg.is_from_me { "Me" } else { "Them" };
            let text = msg
                .text
                .as_ref()
                .map(|t| {
                    if t.len() > 60 {
                        format!("{}...", &t[..60])
                    } else {
                        t.clone()
                    }
                })
                .unwrap_or_else(|| "[No text]".to_string());

            println!(
                "{}. {} ({}): {}",
                i + 1,
                date.format("%Y-%m-%d %H:%M"),
                sender,
                text
            );
        }
    }

    Ok(())
}
