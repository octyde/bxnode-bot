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

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Unified message from any channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingMessage {
    /// Channel identifier (e.g., "telegram", "discord")
    pub channel: String,

    /// Channel-specific chat/conversation ID
    pub chat_id: String,

    /// Channel-specific user ID
    pub user_id: String,

    /// Message content
    pub content: MessageContent,

    /// Original timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Channel-specific metadata
    #[serde(default)]
    pub metadata: serde_json::Value,
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

    #[serde(rename = "file")]
    File { url: String, name: String },
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
}

/// Channel trait - implement this for each platform
#[async_trait]
pub trait Channel: Send + Sync {
    /// Unique channel identifier
    fn id(&self) -> &str;

    /// Human-readable name
    fn name(&self) -> &str;

    /// Start the channel (connect, authenticate, etc.)
    async fn start(&self) -> anyhow::Result<()>;

    /// Stop the channel
    async fn stop(&self) -> anyhow::Result<()>;

    /// Send a message
    async fn send(&self, message: OutgoingMessage) -> anyhow::Result<()>;

    /// Check if the channel is connected
    fn is_connected(&self) -> bool;
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
