//! Channel module - Multi-platform messaging integrations
//!
//! Supported channels:
//! - Telegram (via teloxide)
//! - Discord (via serenity)
//! - Slack
//! - LINE
//! - Signal
//! - iMessage (via BlueBubbles)
//! - Feishu

pub mod registry;

#[cfg(test)]
mod tests;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

pub use registry::ChannelRegistry;

/// Unified message from any channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingMessage {
    /// Unique message ID
    pub id: String,

    /// Channel identifier (e.g., "telegram", "discord")
    pub channel: String,

    /// Channel-specific chat/conversation ID
    pub chat_id: String,

    /// Channel-specific user ID
    pub user_id: String,

    /// User display name (if available)
    pub user_name: Option<String>,

    /// Message content
    pub content: MessageContent,

    /// Original timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Channel-specific metadata
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl IncomingMessage {
    /// Create a new text message
    pub fn text(
        channel: impl Into<String>,
        chat_id: impl Into<String>,
        user_id: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            channel: channel.into(),
            chat_id: chat_id.into(),
            user_id: user_id.into(),
            user_name: None,
            content: MessageContent::Text { text: text.into() },
            timestamp: chrono::Utc::now(),
            metadata: serde_json::Value::Null,
        }
    }

    /// Set user name
    pub fn with_user_name(mut self, name: impl Into<String>) -> Self {
        self.user_name = Some(name.into());
        self
    }

    /// Set metadata
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Message content types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessageContent {
    #[serde(rename = "text")]
    Text { text: String },

    #[serde(rename = "image")]
    Image { url: String, caption: Option<String> },

    #[serde(rename = "audio")]
    Audio { url: String, duration: Option<u32> },

    #[serde(rename = "video")]
    Video { url: String, duration: Option<u32> },

    #[serde(rename = "file")]
    File { url: String, name: String },

    #[serde(rename = "location")]
    Location { latitude: f64, longitude: f64 },

    #[serde(rename = "sticker")]
    Sticker { url: String, emoji: Option<String> },
}

impl MessageContent {
    /// Get text content if this is a text message
    pub fn as_text(&self) -> Option<&str> {
        match self {
            MessageContent::Text { text } => Some(text),
            _ => None,
        }
    }

    /// Check if this is a text message
    pub fn is_text(&self) -> bool {
        matches!(self, MessageContent::Text { .. })
    }
}

/// Outgoing message to a channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingMessage {
    /// Target chat/conversation ID
    pub chat_id: String,

    /// Message content
    pub content: MessageContent,

    /// Reply to message ID (optional)
    pub reply_to: Option<String>,

    /// Parse mode for text (markdown, html, etc.)
    pub parse_mode: Option<ParseMode>,
}

impl OutgoingMessage {
    /// Create a new text message
    pub fn text(chat_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            chat_id: chat_id.into(),
            content: MessageContent::Text { text: text.into() },
            reply_to: None,
            parse_mode: None,
        }
    }

    /// Set reply to message ID
    pub fn reply_to(mut self, message_id: impl Into<String>) -> Self {
        self.reply_to = Some(message_id.into());
        self
    }

    /// Set parse mode
    pub fn with_parse_mode(mut self, mode: ParseMode) -> Self {
        self.parse_mode = Some(mode);
        self
    }
}

/// Text parse mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParseMode {
    Plain,
    Markdown,
    Html,
}

/// Channel event - emitted by channels
#[derive(Debug, Clone)]
pub enum ChannelEvent {
    /// New message received
    Message(IncomingMessage),

    /// Channel connected
    Connected { channel: String },

    /// Channel disconnected
    Disconnected { channel: String, reason: Option<String> },

    /// Error occurred
    Error { channel: String, error: String },
}

/// Channel trait - implement this for each platform
#[async_trait]
pub trait Channel: Send + Sync {
    /// Unique channel identifier
    fn id(&self) -> &str;

    /// Human-readable name
    fn name(&self) -> &str;

    /// Start the channel (connect, authenticate, etc.)
    async fn start(&mut self, event_tx: mpsc::Sender<ChannelEvent>) -> anyhow::Result<()>;

    /// Stop the channel
    async fn stop(&mut self) -> anyhow::Result<()>;

    /// Send a message
    async fn send(&self, message: OutgoingMessage) -> anyhow::Result<String>;

    /// Check if the channel is connected
    fn is_connected(&self) -> bool;

    /// Check if the channel supports message editing (for streaming updates)
    fn supports_edit(&self) -> bool {
        false
    }

    /// Edit an existing message (for streaming updates)
    /// Returns Ok(()) if successful, Err if not supported or failed
    async fn edit_message(
        &self,
        _chat_id: &str,
        _message_id: &str,
        _new_content: &str,
    ) -> anyhow::Result<()> {
        anyhow::bail!("Message editing not supported by this channel")
    }

    /// Get channel status
    fn status(&self) -> ChannelStatus {
        ChannelStatus {
            id: self.id().to_string(),
            name: self.name().to_string(),
            connected: self.is_connected(),
            error: None,
        }
    }
}

/// Channel status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelStatus {
    pub id: String,
    pub name: String,
    pub connected: bool,
    pub error: Option<String>,
}

// Feature-gated channel implementations
#[cfg(feature = "channel-telegram")]
pub mod telegram;

#[cfg(feature = "channel-discord")]
pub mod discord;

#[cfg(feature = "channel-slack")]
pub mod slack;

#[cfg(feature = "channel-line")]
pub mod line;

#[cfg(feature = "channel-signal")]
pub mod signal;

#[cfg(feature = "channel-feishu")]
pub mod feishu;

// Stub implementations for when features are disabled
#[cfg(not(feature = "channel-telegram"))]
pub mod telegram {
    //! Telegram channel stub (feature not enabled)
}

#[cfg(not(feature = "channel-discord"))]
pub mod discord {
    //! Discord channel stub (feature not enabled)
}

#[cfg(not(feature = "channel-slack"))]
pub mod slack {
    //! Slack channel stub (feature not enabled)
}

#[cfg(not(feature = "channel-line"))]
pub mod line {
    //! LINE channel stub (feature not enabled)
}

#[cfg(not(feature = "channel-signal"))]
pub mod signal {
    //! Signal channel stub (feature not enabled)
}

#[cfg(not(feature = "channel-feishu"))]
pub mod feishu {
    //! Feishu channel stub (feature not enabled)
}
