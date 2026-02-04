//! Tests for the channels module

use super::*;

#[test]
fn test_incoming_message_text() {
    let msg = IncomingMessage::text("telegram", "chat-123", "user-456", "Hello world!");

    assert_eq!(msg.channel, "telegram");
    assert_eq!(msg.chat_id, "chat-123");
    assert_eq!(msg.user_id, "user-456");
    assert!(msg.user_name.is_none());
    assert!(msg.content.is_text());
    assert_eq!(msg.content.as_text(), Some("Hello world!"));
}

#[test]
fn test_incoming_message_with_user_name() {
    let msg = IncomingMessage::text("discord", "guild-123", "user-789", "Test")
        .with_user_name("John Doe");

    assert_eq!(msg.user_name, Some("John Doe".to_string()));
}

#[test]
fn test_incoming_message_with_metadata() {
    let metadata = serde_json::json!({
        "reply_to": "msg-999",
        "is_bot": false
    });

    let msg = IncomingMessage::text("slack", "channel-1", "user-1", "Hi")
        .with_metadata(metadata.clone());

    assert_eq!(msg.metadata, metadata);
}

#[test]
fn test_incoming_message_serialization() {
    let msg = IncomingMessage::text("telegram", "chat-1", "user-1", "Test message");

    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: IncomingMessage = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.channel, msg.channel);
    assert_eq!(deserialized.chat_id, msg.chat_id);
    assert_eq!(deserialized.content.as_text(), msg.content.as_text());
}

#[test]
fn test_message_content_text() {
    let content = MessageContent::Text {
        text: "Hello".to_string(),
    };

    assert!(content.is_text());
    assert_eq!(content.as_text(), Some("Hello"));
}

#[test]
fn test_message_content_image() {
    let content = MessageContent::Image {
        url: "https://example.com/img.png".to_string(),
        caption: Some("A photo".to_string()),
    };

    assert!(!content.is_text());
    assert_eq!(content.as_text(), None);
}

#[test]
fn test_message_content_audio() {
    let content = MessageContent::Audio {
        url: "https://example.com/audio.mp3".to_string(),
        duration: Some(120),
    };

    assert!(!content.is_text());
}

#[test]
fn test_message_content_video() {
    let content = MessageContent::Video {
        url: "https://example.com/video.mp4".to_string(),
        duration: Some(300),
    };

    assert!(!content.is_text());
}

#[test]
fn test_message_content_file() {
    let content = MessageContent::File {
        url: "https://example.com/doc.pdf".to_string(),
        name: "document.pdf".to_string(),
    };

    assert!(!content.is_text());
}

#[test]
fn test_message_content_location() {
    let content = MessageContent::Location {
        latitude: 37.7749,
        longitude: -122.4194,
    };

    assert!(!content.is_text());
}

#[test]
fn test_message_content_sticker() {
    let content = MessageContent::Sticker {
        url: "https://example.com/sticker.webp".to_string(),
        emoji: Some("😀".to_string()),
    };

    assert!(!content.is_text());
}

#[test]
fn test_message_content_serialization() {
    let cases = vec![
        MessageContent::Text {
            text: "Hello".to_string(),
        },
        MessageContent::Image {
            url: "http://example.com/img.png".to_string(),
            caption: None,
        },
        MessageContent::Audio {
            url: "http://example.com/audio.mp3".to_string(),
            duration: Some(60),
        },
        MessageContent::Video {
            url: "http://example.com/video.mp4".to_string(),
            duration: None,
        },
        MessageContent::File {
            url: "http://example.com/file.pdf".to_string(),
            name: "file.pdf".to_string(),
        },
        MessageContent::Location {
            latitude: 0.0,
            longitude: 0.0,
        },
        MessageContent::Sticker {
            url: "http://example.com/sticker.webp".to_string(),
            emoji: Some("👍".to_string()),
        },
    ];

    for content in cases {
        let json = serde_json::to_string(&content).unwrap();
        let deserialized: MessageContent = serde_json::from_str(&json).unwrap();

        // Verify type tag is preserved
        assert_eq!(content.is_text(), deserialized.is_text());
    }
}

#[test]
fn test_outgoing_message_text() {
    let msg = OutgoingMessage::text("chat-123", "Hello!");

    assert_eq!(msg.chat_id, "chat-123");
    assert!(msg.content.is_text());
    assert!(msg.reply_to.is_none());
    assert!(msg.parse_mode.is_none());
}

#[test]
fn test_outgoing_message_reply_to() {
    let msg = OutgoingMessage::text("chat-123", "Reply").reply_to("msg-999");

    assert_eq!(msg.reply_to, Some("msg-999".to_string()));
}

#[test]
fn test_outgoing_message_with_parse_mode() {
    let msg = OutgoingMessage::text("chat-123", "*bold*").with_parse_mode(ParseMode::Markdown);

    assert_eq!(msg.parse_mode, Some(ParseMode::Markdown));
}

#[test]
fn test_outgoing_message_serialization() {
    let msg = OutgoingMessage::text("chat-1", "Test")
        .reply_to("msg-1")
        .with_parse_mode(ParseMode::Html);

    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: OutgoingMessage = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.chat_id, msg.chat_id);
    assert_eq!(deserialized.reply_to, msg.reply_to);
    assert_eq!(deserialized.parse_mode, msg.parse_mode);
}

#[test]
fn test_parse_mode_serialization() {
    let modes = vec![ParseMode::Plain, ParseMode::Markdown, ParseMode::Html];

    for mode in modes {
        let json = serde_json::to_string(&mode).unwrap();
        let deserialized: ParseMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, deserialized);
    }
}

#[test]
fn test_channel_status() {
    let status = ChannelStatus {
        id: "telegram".to_string(),
        name: "Telegram".to_string(),
        connected: true,
        error: None,
    };

    let json = serde_json::to_string(&status).unwrap();
    let deserialized: ChannelStatus = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.id, "telegram");
    assert!(deserialized.connected);
    assert!(deserialized.error.is_none());
}

#[test]
fn test_channel_status_with_error() {
    let status = ChannelStatus {
        id: "discord".to_string(),
        name: "Discord".to_string(),
        connected: false,
        error: Some("Connection timeout".to_string()),
    };

    assert!(!status.connected);
    assert_eq!(status.error, Some("Connection timeout".to_string()));
}
