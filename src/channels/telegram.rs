//! Telegram channel implementation using teloxide

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use teloxide::prelude::*;
use teloxide::types::{MediaKind, MessageKind, ParseMode as TgParseMode};
use tokio::sync::mpsc;

use super::{
    Channel, ChannelEvent, ChannelStatus, IncomingMessage, MessageContent, OutgoingMessage,
    ParseMode,
};

/// Telegram channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    /// Bot token from @BotFather
    pub bot_token: String,

    /// Optional webhook URL (if not set, uses long polling)
    #[serde(default)]
    pub webhook_url: Option<String>,

    /// Allowed user IDs (if empty, allows all)
    #[serde(default)]
    pub allowed_users: Vec<i64>,

    /// Allowed chat IDs (if empty, allows all)
    #[serde(default)]
    pub allowed_chats: Vec<i64>,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            bot_token: String::new(),
            webhook_url: None,
            allowed_users: vec![],
            allowed_chats: vec![],
        }
    }
}

/// Telegram channel
pub struct TelegramChannel {
    config: TelegramConfig,
    bot: Option<Bot>,
    connected: Arc<AtomicBool>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl TelegramChannel {
    /// Create a new Telegram channel
    pub fn new(config: TelegramConfig) -> Self {
        Self {
            config,
            bot: None,
            connected: Arc::new(AtomicBool::new(false)),
            shutdown_tx: None,
        }
    }

    /// Check if a user is allowed
    fn is_user_allowed(&self, user_id: i64) -> bool {
        self.config.allowed_users.is_empty() || self.config.allowed_users.contains(&user_id)
    }

    /// Check if a chat is allowed
    fn is_chat_allowed(&self, chat_id: i64) -> bool {
        self.config.allowed_chats.is_empty() || self.config.allowed_chats.contains(&chat_id)
    }

    /// Convert teloxide message to our IncomingMessage
    fn convert_message(msg: &Message) -> Option<IncomingMessage> {
        // Build composite chat_id with topic support for forum groups
        let chat_id = if let Some(thread_id) = msg.thread_id {
            format!("{}:topic:{}", msg.chat.id, thread_id)
        } else {
            msg.chat.id.to_string()
        };
        let user_id = msg.from.as_ref()?.id.to_string();
        let user_name = msg
            .from
            .as_ref()
            .and_then(|u| u.username.clone().or_else(|| Some(u.first_name.clone())));

        let content = match &msg.kind {
            MessageKind::Common(common) => match &common.media_kind {
                MediaKind::Text(text) => MessageContent::Text {
                    text: text.text.clone(),
                },
                MediaKind::Photo(photo) => {
                    // Get the largest photo
                    let largest = photo.photo.last()?;
                    MessageContent::Image {
                        url: largest.file.id.clone(),
                        caption: photo.caption.clone(),
                    }
                }
                MediaKind::Audio(audio) => MessageContent::Audio {
                    url: audio.audio.file.id.clone(),
                    duration: Some(audio.audio.duration.seconds()),
                },
                MediaKind::Video(video) => MessageContent::Video {
                    url: video.video.file.id.clone(),
                    duration: Some(video.video.duration.seconds()),
                },
                MediaKind::Document(doc) => MessageContent::File {
                    url: doc.document.file.id.clone(),
                    name: doc
                        .document
                        .file_name
                        .clone()
                        .unwrap_or_else(|| "document".to_string()),
                },
                MediaKind::Location(loc) => MessageContent::Location {
                    latitude: loc.location.latitude,
                    longitude: loc.location.longitude,
                },
                MediaKind::Sticker(sticker) => MessageContent::Sticker {
                    url: sticker.sticker.file.id.clone(),
                    emoji: sticker.sticker.emoji.clone(),
                },
                MediaKind::Voice(voice) => MessageContent::Audio {
                    url: voice.voice.file.id.clone(),
                    duration: Some(voice.voice.duration.seconds()),
                },
                MediaKind::VideoNote(video_note) => MessageContent::Video {
                    url: video_note.video_note.file.id.clone(),
                    duration: Some(video_note.video_note.duration.seconds()),
                },
                _ => return None,
            },
            _ => return None,
        };

        let timestamp = chrono::DateTime::from_timestamp(msg.date.timestamp(), 0)
            .unwrap_or_else(chrono::Utc::now);

        let mut incoming = IncomingMessage {
            id: msg.id.to_string(),
            channel: "telegram".to_string(),
            chat_id,
            user_id,
            user_name,
            content,
            timestamp,
            metadata: serde_json::json!({
                "message_id": msg.id.0,
                "chat_type": format!("{:?}", msg.chat.kind),
                "thread_id": msg.thread_id,
            }),
        };

        // Add reply_to if present
        if let Some(reply) = &msg.reply_to_message() {
            incoming.metadata["reply_to_message_id"] = serde_json::json!(reply.id.0);
        }

        Some(incoming)
    }

    /// Convert our ParseMode to teloxide's
    #[allow(dead_code)]
    fn convert_parse_mode(mode: Option<ParseMode>) -> Option<TgParseMode> {
        mode.and_then(|m| match m {
            ParseMode::Plain => None,
            ParseMode::Markdown => Some(TgParseMode::MarkdownV2),
            ParseMode::Html => Some(TgParseMode::Html),
        })
    }
}

/// Parse a composite chat_id (e.g., "-1001234567890:topic:42") into (chat_id, optional thread_id).
fn parse_composite_chat_id(composite: &str) -> anyhow::Result<(i64, Option<i32>)> {
    if let Some(idx) = composite.find(":topic:") {
        let chat_part = &composite[..idx];
        let topic_part = &composite[idx + 7..];
        let chat_id: i64 = chat_part
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid chat ID: {}", chat_part))?;
        let thread_id: i32 = topic_part
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid topic ID: {}", topic_part))?;
        Ok((chat_id, Some(thread_id)))
    } else {
        let chat_id: i64 = composite
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid chat ID: {}", composite))?;
        Ok((chat_id, None))
    }
}

#[async_trait]
impl Channel for TelegramChannel {
    fn id(&self) -> &str {
        "telegram"
    }

    fn name(&self) -> &str {
        "Telegram"
    }

    async fn start(&mut self, event_tx: mpsc::Sender<ChannelEvent>) -> anyhow::Result<()> {
        if self.config.bot_token.is_empty() {
            return Err(anyhow::anyhow!("Telegram bot token is required"));
        }

        let bot = Bot::new(&self.config.bot_token);
        self.bot = Some(bot.clone());

        let connected = self.connected.clone();
        let allowed_users = self.config.allowed_users.clone();
        let allowed_chats = self.config.allowed_chats.clone();

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        // Spawn manual long-polling loop.
        //
        // We avoid teloxide's Dispatcher::dispatch() because it internally uses
        // std::thread::scope + block_on which blocks a tokio worker thread and
        // can starve the HTTP server.  Instead we poll via bot.get_updates()
        // directly — fully async, no thread blocking.
        let handler_tx = event_tx.clone();
        tokio::spawn(async move {
            connected.store(true, Ordering::SeqCst);

            let _ = handler_tx
                .send(ChannelEvent::Connected {
                    channel: "telegram".to_string(),
                })
                .await;

            // Try to delete any existing webhook so long-polling works
            if let Err(e) = bot.delete_webhook().send().await {
                tracing::warn!("Failed to delete webhook (non-fatal): {:?}", e);
            }

            let mut offset: i32 = 0;

            loop {
                // Check shutdown signal (non-blocking)
                if shutdown_rx.try_recv().is_ok() {
                    tracing::info!("Telegram channel shutting down");
                    break;
                }

                // Long-poll for updates.
                // Use timeout=10s which is safely below the default reqwest
                // client timeout (~17s) to avoid TimedOut errors.
                let result = bot
                    .get_updates()
                    .offset(offset)
                    .timeout(10)
                    .allowed_updates(vec![
                        teloxide::types::AllowedUpdate::Message,
                    ])
                    .send()
                    .await;

                match result {
                    Ok(updates) => {
                        for update in updates {
                            // Advance offset past this update
                            offset = update.id.as_offset();

                            // Extract message from the update kind
                            let msg = match update.kind {
                                teloxide::types::UpdateKind::Message(ref m) => m,
                                _ => continue,
                            };

                            // Check user filter
                            if let Some(user) = msg.from.as_ref() {
                                let user_id = user.id.0 as i64;
                                if !allowed_users.is_empty()
                                    && !allowed_users.contains(&user_id)
                                {
                                    tracing::debug!(
                                        "Ignoring message from unauthorized user: {}",
                                        user_id
                                    );
                                    continue;
                                }
                            }

                            // Check chat filter
                            let chat_id = msg.chat.id.0;
                            if !allowed_chats.is_empty()
                                && !allowed_chats.contains(&chat_id)
                            {
                                tracing::debug!(
                                    "Ignoring message from unauthorized chat: {}",
                                    chat_id
                                );
                                continue;
                            }

                            // Convert and forward
                            if let Some(incoming) = TelegramChannel::convert_message(msg) {
                                let _ = handler_tx
                                    .send(ChannelEvent::Message(incoming))
                                    .await;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Telegram get_updates error: {:?}", e);
                        // Backoff on error to avoid tight retry loop
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    }
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
        self.bot = None;
        Ok(())
    }

    async fn send(&self, message: OutgoingMessage) -> anyhow::Result<String> {
        let bot = self
            .bot
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Telegram bot not initialized"))?;

        // Parse composite chat_id with optional topic: "12345:topic:67" or just "12345"
        let (chat_id, thread_id) = parse_composite_chat_id(&message.chat_id)?;

        let sent = match &message.content {
            MessageContent::Text { text } => {
                let mut req = bot.send_message(ChatId(chat_id), text);

                // Reply to the correct forum topic thread
                if let Some(tid) = thread_id {
                    req = req.message_thread_id(teloxide::types::ThreadId(teloxide::types::MessageId(tid)));
                }

                if let Some(parse_mode) = Self::convert_parse_mode(message.parse_mode) {
                    req = req.parse_mode(parse_mode);
                }

                if let Some(reply_to) = &message.reply_to {
                    if let Ok(msg_id) = reply_to.parse::<i32>() {
                        req = req.reply_parameters(teloxide::types::ReplyParameters::new(
                            teloxide::types::MessageId(msg_id),
                        ));
                    }
                }

                req.await?
            }
            MessageContent::Image { url, caption } => {
                let mut req = bot.send_photo(ChatId(chat_id), teloxide::types::InputFile::file_id(url));

                if let Some(cap) = caption {
                    req = req.caption(cap);
                }

                if let Some(parse_mode) = Self::convert_parse_mode(message.parse_mode) {
                    req = req.parse_mode(parse_mode);
                }

                req.await?
            }
            MessageContent::Audio { url, .. } => {
                bot.send_audio(ChatId(chat_id), teloxide::types::InputFile::file_id(url))
                    .await?
            }
            MessageContent::Video { url, .. } => {
                bot.send_video(ChatId(chat_id), teloxide::types::InputFile::file_id(url))
                    .await?
            }
            MessageContent::File { url, .. } => {
                bot.send_document(ChatId(chat_id), teloxide::types::InputFile::file_id(url))
                    .await?
            }
            MessageContent::Location {
                latitude,
                longitude,
            } => {
                bot.send_location(ChatId(chat_id), *latitude, *longitude)
                    .await?
            }
            MessageContent::Sticker { url, .. } => {
                bot.send_sticker(ChatId(chat_id), teloxide::types::InputFile::file_id(url))
                    .await?
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
        let bot = self
            .bot
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Telegram bot not initialized"))?;

        let chat_id: i64 = chat_id
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid chat ID"))?;

        let message_id: i32 = message_id
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid message ID"))?;

        bot.edit_message_text(ChatId(chat_id), teloxide::types::MessageId(message_id), new_content)
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
    fn test_telegram_config_default() {
        let config = TelegramConfig::default();
        assert!(config.bot_token.is_empty());
        assert!(config.webhook_url.is_none());
        assert!(config.allowed_users.is_empty());
        assert!(config.allowed_chats.is_empty());
    }

    #[test]
    fn test_telegram_channel_creation() {
        let config = TelegramConfig {
            bot_token: "test_token".to_string(),
            ..Default::default()
        };

        let channel = TelegramChannel::new(config);
        assert_eq!(channel.id(), "telegram");
        assert_eq!(channel.name(), "Telegram");
        assert!(!channel.is_connected());
    }

    #[test]
    fn test_user_allowed_empty_list() {
        let config = TelegramConfig::default();
        let channel = TelegramChannel::new(config);

        // Empty list means all users allowed
        assert!(channel.is_user_allowed(12345));
        assert!(channel.is_user_allowed(67890));
    }

    #[test]
    fn test_user_allowed_with_list() {
        let config = TelegramConfig {
            allowed_users: vec![12345, 67890],
            ..Default::default()
        };
        let channel = TelegramChannel::new(config);

        assert!(channel.is_user_allowed(12345));
        assert!(channel.is_user_allowed(67890));
        assert!(!channel.is_user_allowed(11111));
    }

    #[test]
    fn test_chat_allowed_empty_list() {
        let config = TelegramConfig::default();
        let channel = TelegramChannel::new(config);

        // Empty list means all chats allowed
        assert!(channel.is_chat_allowed(-100123456));
    }

    #[test]
    fn test_chat_allowed_with_list() {
        let config = TelegramConfig {
            allowed_chats: vec![-100123456],
            ..Default::default()
        };
        let channel = TelegramChannel::new(config);

        assert!(channel.is_chat_allowed(-100123456));
        assert!(!channel.is_chat_allowed(-100789012));
    }

    #[test]
    fn test_convert_parse_mode() {
        assert!(TelegramChannel::convert_parse_mode(None).is_none());
        assert!(TelegramChannel::convert_parse_mode(Some(ParseMode::Plain)).is_none());
        assert_eq!(
            TelegramChannel::convert_parse_mode(Some(ParseMode::Markdown)),
            Some(TgParseMode::MarkdownV2)
        );
        assert_eq!(
            TelegramChannel::convert_parse_mode(Some(ParseMode::Html)),
            Some(TgParseMode::Html)
        );
    }

    #[test]
    fn test_channel_status_no_token() {
        let config = TelegramConfig::default();
        let channel = TelegramChannel::new(config);

        let status = channel.status();
        assert!(!status.connected);
        assert!(status.error.is_some());
        assert!(status.error.unwrap().contains("not configured"));
    }

    #[test]
    fn test_channel_status_with_token() {
        let config = TelegramConfig {
            bot_token: "test_token".to_string(),
            ..Default::default()
        };
        let channel = TelegramChannel::new(config);

        let status = channel.status();
        assert!(!status.connected);
        assert!(status.error.is_none());
    }

    #[test]
    fn test_supports_edit() {
        let config = TelegramConfig {
            bot_token: "test_token".to_string(),
            ..Default::default()
        };
        let channel = TelegramChannel::new(config);

        assert!(channel.supports_edit());
    }
}
