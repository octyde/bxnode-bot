//! Discord channel implementation using serenity

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serenity::all::{
    ChannelId, Context, CreateMessage, EditMessage, EventHandler, GatewayIntents,
    Message as SerenityMessage, MessageId, Ready,
};
use serenity::Client;
use tokio::sync::{mpsc, RwLock};

use super::{
    Channel, ChannelEvent, ChannelStatus, IncomingMessage, MessageContent, OutgoingMessage,
};

/// Discord channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    /// Bot token from Discord Developer Portal
    pub bot_token: String,

    /// Application ID (optional, for slash commands)
    #[serde(default)]
    pub application_id: Option<u64>,

    /// Allowed guild (server) IDs (if empty, allows all)
    #[serde(default)]
    pub allowed_guilds: Vec<u64>,

    /// Allowed channel IDs (if empty, allows all)
    #[serde(default)]
    pub allowed_channels: Vec<u64>,

    /// Bot prefix for commands (optional)
    #[serde(default)]
    pub prefix: Option<String>,

    /// Whether to respond to DMs
    #[serde(default = "default_allow_dms")]
    pub allow_dms: bool,
}

fn default_allow_dms() -> bool {
    true
}

impl Default for DiscordConfig {
    fn default() -> Self {
        Self {
            bot_token: String::new(),
            application_id: None,
            allowed_guilds: vec![],
            allowed_channels: vec![],
            prefix: None,
            allow_dms: true,
        }
    }
}

/// Discord event handler
struct Handler {
    event_tx: mpsc::Sender<ChannelEvent>,
    config: DiscordConfig,
    connected: Arc<AtomicBool>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _ctx: Context, ready: Ready) {
        tracing::info!("Discord bot connected as {}", ready.user.name);
        self.connected.store(true, Ordering::SeqCst);

        let _ = self
            .event_tx
            .send(ChannelEvent::Connected {
                channel: "discord".to_string(),
            })
            .await;
    }

    async fn message(&self, _ctx: Context, msg: SerenityMessage) {
        // Ignore bot messages
        if msg.author.bot {
            return;
        }

        // Check if DMs are allowed
        if msg.guild_id.is_none() && !self.config.allow_dms {
            tracing::debug!("Ignoring DM from {}", msg.author.name);
            return;
        }

        // Check guild allowlist
        if let Some(guild_id) = msg.guild_id {
            if !self.config.allowed_guilds.is_empty()
                && !self.config.allowed_guilds.contains(&guild_id.get())
            {
                tracing::debug!("Ignoring message from unauthorized guild: {}", guild_id);
                return;
            }
        }

        // Check channel allowlist
        if !self.config.allowed_channels.is_empty()
            && !self.config.allowed_channels.contains(&msg.channel_id.get())
        {
            tracing::debug!(
                "Ignoring message from unauthorized channel: {}",
                msg.channel_id
            );
            return;
        }

        // Check prefix if configured
        if let Some(ref prefix) = self.config.prefix {
            if !msg.content.starts_with(prefix) {
                return;
            }
        }

        // Convert message
        let incoming = Self::convert_message(&msg);

        if let Err(e) = self.event_tx.send(ChannelEvent::Message(incoming)).await {
            tracing::error!("Failed to send Discord message event: {}", e);
        }
    }
}

impl Handler {
    /// Convert serenity message to our IncomingMessage
    fn convert_message(msg: &SerenityMessage) -> IncomingMessage {
        let content = if !msg.attachments.is_empty() {
            // Handle first attachment
            let attachment = &msg.attachments[0];

            if let Some(content_type) = &attachment.content_type {
                if content_type.starts_with("image/") {
                    MessageContent::Image {
                        url: attachment.url.clone(),
                        caption: if msg.content.is_empty() {
                            None
                        } else {
                            Some(msg.content.clone())
                        },
                    }
                } else if content_type.starts_with("audio/") {
                    MessageContent::Audio {
                        url: attachment.url.clone(),
                        duration: None,
                    }
                } else if content_type.starts_with("video/") {
                    MessageContent::Video {
                        url: attachment.url.clone(),
                        duration: None,
                    }
                } else {
                    MessageContent::File {
                        url: attachment.url.clone(),
                        name: attachment.filename.clone(),
                    }
                }
            } else {
                MessageContent::File {
                    url: attachment.url.clone(),
                    name: attachment.filename.clone(),
                }
            }
        } else if !msg.sticker_items.is_empty() {
            let sticker = &msg.sticker_items[0];
            MessageContent::Sticker {
                url: sticker
                    .image_url()
                    .unwrap_or_else(|| format!("sticker:{}", sticker.id)),
                emoji: sticker.name.clone().into(),
            }
        } else {
            MessageContent::Text {
                text: msg.content.clone(),
            }
        };

        // Convert serenity's OffsetDateTime to chrono's DateTime<Utc>
        let timestamp = chrono::DateTime::from_timestamp(
            msg.timestamp.unix_timestamp(),
            msg.timestamp.nanosecond(),
        )
        .unwrap_or_else(chrono::Utc::now);

        IncomingMessage {
            id: msg.id.to_string(),
            channel: "discord".to_string(),
            chat_id: msg.channel_id.to_string(),
            user_id: msg.author.id.to_string(),
            user_name: Some(msg.author.name.clone()),
            content,
            timestamp,
            metadata: serde_json::json!({
                "message_id": msg.id.get(),
                "guild_id": msg.guild_id.map(|g| g.get()),
                "is_dm": msg.guild_id.is_none(),
            }),
        }
    }
}

/// Discord channel
pub struct DiscordChannel {
    config: DiscordConfig,
    connected: Arc<AtomicBool>,
    http: Arc<RwLock<Option<Arc<serenity::http::Http>>>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl DiscordChannel {
    /// Create a new Discord channel
    pub fn new(config: DiscordConfig) -> Self {
        Self {
            config,
            connected: Arc::new(AtomicBool::new(false)),
            http: Arc::new(RwLock::new(None)),
            shutdown_tx: None,
        }
    }

    /// Check if a guild is allowed
    fn is_guild_allowed(&self, guild_id: u64) -> bool {
        self.config.allowed_guilds.is_empty() || self.config.allowed_guilds.contains(&guild_id)
    }

    /// Check if a channel is allowed
    fn is_channel_allowed(&self, channel_id: u64) -> bool {
        self.config.allowed_channels.is_empty() || self.config.allowed_channels.contains(&channel_id)
    }
}

#[async_trait]
impl Channel for DiscordChannel {
    fn id(&self) -> &str {
        "discord"
    }

    fn name(&self) -> &str {
        "Discord"
    }

    async fn start(&mut self, event_tx: mpsc::Sender<ChannelEvent>) -> anyhow::Result<()> {
        if self.config.bot_token.is_empty() {
            return Err(anyhow::anyhow!("Discord bot token is required"));
        }

        let intents = GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT;

        let handler = Handler {
            event_tx,
            config: self.config.clone(),
            connected: self.connected.clone(),
        };

        let mut client = Client::builder(&self.config.bot_token, intents)
            .event_handler(handler)
            .await?;

        // Store HTTP client for sending messages
        {
            let mut http = self.http.write().await;
            *http = Some(client.http.clone());
        }

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        let connected = self.connected.clone();

        tokio::spawn(async move {
            tokio::select! {
                result = client.start() => {
                    if let Err(e) = result {
                        tracing::error!("Discord client error: {}", e);
                    }
                }
                _ = &mut shutdown_rx => {
                    tracing::info!("Discord channel shutting down");
                    client.shard_manager.shutdown_all().await;
                }
            }

            connected.store(false, Ordering::SeqCst);
        });

        Ok(())
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn send(&self, message: OutgoingMessage) -> anyhow::Result<String> {
        let http = self.http.read().await;
        let http = http
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Discord client not initialized"))?;

        let channel_id: u64 = message
            .chat_id
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid channel ID"))?;

        let channel = ChannelId::new(channel_id);

        let sent = match &message.content {
            MessageContent::Text { text } => {
                let builder = CreateMessage::new().content(text);

                channel.send_message(http, builder).await?
            }
            MessageContent::Image { url, caption } => {
                let mut builder = CreateMessage::new();

                if let Some(cap) = caption {
                    builder = builder.content(cap);
                }

                // For Discord, we typically send images as embeds or attachments
                // Here we'll send as a message with the URL
                builder = builder.content(format!(
                    "{}\n{}",
                    caption.as_deref().unwrap_or(""),
                    url
                ));

                channel.send_message(http, builder).await?
            }
            _ => {
                // For other content types, convert to text representation
                let text = match &message.content {
                    MessageContent::Audio { url, .. } => format!("🎵 {}", url),
                    MessageContent::Video { url, .. } => format!("🎬 {}", url),
                    MessageContent::File { url, name } => format!("📎 {}: {}", name, url),
                    MessageContent::Location {
                        latitude,
                        longitude,
                    } => {
                        format!(
                            "📍 https://maps.google.com/?q={},{}",
                            latitude, longitude
                        )
                    }
                    MessageContent::Sticker { emoji, .. } => {
                        emoji.clone().unwrap_or_else(|| "🎨".to_string())
                    }
                    _ => "Unsupported content type".to_string(),
                };

                let builder = CreateMessage::new().content(text);
                channel.send_message(http, builder).await?
            }
        };

        Ok(sent.id.to_string())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn supports_edit(&self) -> bool {
        true
    }

    async fn edit_message(
        &self,
        chat_id: &str,
        message_id: &str,
        new_content: &str,
    ) -> anyhow::Result<()> {
        let http = self.http.read().await;
        let http = http
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Discord client not initialized"))?;

        let channel_id: u64 = chat_id
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid channel ID"))?;

        let msg_id: u64 = message_id
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid message ID"))?;

        let channel = ChannelId::new(channel_id);
        let builder = EditMessage::new().content(new_content);

        channel
            .edit_message(http, MessageId::new(msg_id), builder)
            .await?;

        Ok(())
    }

    fn status(&self) -> ChannelStatus {
        ChannelStatus {
            id: self.id().to_string(),
            name: self.name().to_string(),
            connected: self.is_connected(),
            error: if self.config.bot_token.is_empty() {
                Some("Bot token not configured".to_string())
            } else {
                None
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discord_config_default() {
        let config = DiscordConfig::default();
        assert!(config.bot_token.is_empty());
        assert!(config.application_id.is_none());
        assert!(config.allowed_guilds.is_empty());
        assert!(config.allowed_channels.is_empty());
        assert!(config.prefix.is_none());
        assert!(config.allow_dms);
    }

    #[test]
    fn test_discord_channel_creation() {
        let config = DiscordConfig {
            bot_token: "test_token".to_string(),
            ..Default::default()
        };

        let channel = DiscordChannel::new(config);
        assert_eq!(channel.id(), "discord");
        assert_eq!(channel.name(), "Discord");
        assert!(!channel.is_connected());
    }

    #[test]
    fn test_guild_allowed_empty_list() {
        let config = DiscordConfig::default();
        let channel = DiscordChannel::new(config);

        // Empty list means all guilds allowed
        assert!(channel.is_guild_allowed(123456789));
    }

    #[test]
    fn test_guild_allowed_with_list() {
        let config = DiscordConfig {
            allowed_guilds: vec![123456789, 987654321],
            ..Default::default()
        };
        let channel = DiscordChannel::new(config);

        assert!(channel.is_guild_allowed(123456789));
        assert!(channel.is_guild_allowed(987654321));
        assert!(!channel.is_guild_allowed(111111111));
    }

    #[test]
    fn test_channel_allowed_empty_list() {
        let config = DiscordConfig::default();
        let channel = DiscordChannel::new(config);

        // Empty list means all channels allowed
        assert!(channel.is_channel_allowed(123456789));
    }

    #[test]
    fn test_channel_allowed_with_list() {
        let config = DiscordConfig {
            allowed_channels: vec![123456789],
            ..Default::default()
        };
        let channel = DiscordChannel::new(config);

        assert!(channel.is_channel_allowed(123456789));
        assert!(!channel.is_channel_allowed(987654321));
    }

    #[test]
    fn test_channel_status_no_token() {
        let config = DiscordConfig::default();
        let channel = DiscordChannel::new(config);

        let status = channel.status();
        assert!(!status.connected);
        assert!(status.error.is_some());
        assert!(status.error.unwrap().contains("not configured"));
    }

    #[test]
    fn test_channel_status_with_token() {
        let config = DiscordConfig {
            bot_token: "test_token".to_string(),
            ..Default::default()
        };
        let channel = DiscordChannel::new(config);

        let status = channel.status();
        assert!(!status.connected);
        assert!(status.error.is_none());
    }

    #[test]
    fn test_supports_edit() {
        let config = DiscordConfig {
            bot_token: "test_token".to_string(),
            ..Default::default()
        };
        let channel = DiscordChannel::new(config);

        assert!(channel.supports_edit());
    }
}
