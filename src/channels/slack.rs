//! Slack channel implementation using Web API and Socket Mode
//!
//! This implementation uses:
//! - Socket Mode for real-time event delivery (WebSocket)
//! - Web API for sending messages
//!
//! Required scopes:
//! - app_mentions:read - To receive @mentions
//! - chat:write - To send messages
//! - channels:history - To read channel messages (if needed)
//! - im:history - To read DM messages
//! - users:read - To get user info

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::tungstenite::Message as WsMessage;

use super::{Channel, ChannelEvent, ChannelStatus, IncomingMessage, MessageContent, OutgoingMessage};

/// Slack channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackConfig {
    /// Bot OAuth token (xoxb-...)
    pub bot_token: String,

    /// App-level token for Socket Mode (xapp-...)
    pub app_token: String,

    /// Allowed channel IDs (if empty, allows all)
    #[serde(default)]
    pub allowed_channels: Vec<String>,

    /// Whether to respond to direct messages
    #[serde(default = "default_allow_dms")]
    pub allow_dms: bool,

    /// Whether to only respond to @mentions
    #[serde(default)]
    pub mentions_only: bool,
}

fn default_allow_dms() -> bool {
    true
}

impl Default for SlackConfig {
    fn default() -> Self {
        Self {
            bot_token: String::new(),
            app_token: String::new(),
            allowed_channels: vec![],
            allow_dms: true,
            mentions_only: false,
        }
    }
}

/// Slack API response wrapper
#[derive(Debug, Deserialize)]
struct SlackResponse<T> {
    ok: bool,
    #[serde(flatten)]
    data: Option<T>,
    error: Option<String>,
}

/// Socket Mode connection response
#[derive(Debug, Deserialize)]
struct SocketModeConnection {
    url: String,
}

/// Socket Mode envelope
#[derive(Debug, Deserialize)]
struct SocketModeEnvelope {
    envelope_id: String,
    #[serde(rename = "type")]
    envelope_type: String,
    payload: Option<serde_json::Value>,
}

/// Socket Mode acknowledgment
#[derive(Debug, Serialize)]
struct SocketModeAck {
    envelope_id: String,
}

/// Slack event payload
#[derive(Debug, Deserialize)]
struct EventPayload {
    event: SlackEvent,
}

/// Slack event types
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum SlackEvent {
    #[serde(rename = "message")]
    Message(SlackMessage),
    #[serde(rename = "app_mention")]
    AppMention(SlackMessage),
    #[serde(other)]
    Unknown,
}

/// Slack message
#[derive(Debug, Deserialize)]
struct SlackMessage {
    channel: String,
    user: Option<String>,
    text: String,
    ts: String,
    #[serde(default)]
    thread_ts: Option<String>,
    #[serde(default)]
    files: Vec<SlackFile>,
    #[serde(default)]
    subtype: Option<String>,
}

/// Slack file attachment
#[derive(Debug, Deserialize)]
struct SlackFile {
    id: String,
    name: String,
    mimetype: String,
    url_private: String,
}

/// Chat post message response
#[derive(Debug, Deserialize)]
struct PostMessageResponse {
    ts: String,
    channel: String,
}

/// Slack channel
pub struct SlackChannel {
    config: SlackConfig,
    client: reqwest::Client,
    connected: Arc<AtomicBool>,
    bot_user_id: Arc<RwLock<Option<String>>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl SlackChannel {
    /// Create a new Slack channel
    pub fn new(config: SlackConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            connected: Arc::new(AtomicBool::new(false)),
            bot_user_id: Arc::new(RwLock::new(None)),
            shutdown_tx: None,
        }
    }

    /// Get Socket Mode WebSocket URL
    async fn get_websocket_url(&self) -> anyhow::Result<String> {
        let response: SlackResponse<SocketModeConnection> = self
            .client
            .post("https://slack.com/api/apps.connections.open")
            .header("Authorization", format!("Bearer {}", self.config.app_token))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send()
            .await?
            .json()
            .await?;

        if !response.ok {
            return Err(anyhow::anyhow!(
                "Failed to get Socket Mode URL: {}",
                response.error.unwrap_or_else(|| "Unknown error".to_string())
            ));
        }

        response
            .data
            .map(|d| d.url)
            .ok_or_else(|| anyhow::anyhow!("No WebSocket URL in response"))
    }

    /// Get bot user ID
    async fn get_bot_user_id(&self) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct AuthTest {
            user_id: String,
        }

        let response: SlackResponse<AuthTest> = self
            .client
            .post("https://slack.com/api/auth.test")
            .header("Authorization", format!("Bearer {}", self.config.bot_token))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send()
            .await?
            .json()
            .await?;

        if !response.ok {
            return Err(anyhow::anyhow!(
                "Auth test failed: {}",
                response.error.unwrap_or_else(|| "Unknown error".to_string())
            ));
        }

        response
            .data
            .map(|d| d.user_id)
            .ok_or_else(|| anyhow::anyhow!("No user_id in auth.test response"))
    }

    /// Check if a channel is allowed
    fn is_channel_allowed(&self, channel_id: &str) -> bool {
        self.config.allowed_channels.is_empty()
            || self.config.allowed_channels.contains(&channel_id.to_string())
    }

    /// Convert Slack message to our IncomingMessage
    fn convert_message(msg: &SlackMessage, is_mention: bool) -> Option<IncomingMessage> {
        // Skip bot messages and message edits
        if msg.subtype.is_some() {
            return None;
        }

        let user_id = msg.user.clone()?;

        let content = if !msg.files.is_empty() {
            let file = &msg.files[0];
            if file.mimetype.starts_with("image/") {
                MessageContent::Image {
                    url: file.url_private.clone(),
                    caption: if msg.text.is_empty() {
                        None
                    } else {
                        Some(msg.text.clone())
                    },
                }
            } else if file.mimetype.starts_with("audio/") {
                MessageContent::Audio {
                    url: file.url_private.clone(),
                    duration: None,
                }
            } else if file.mimetype.starts_with("video/") {
                MessageContent::Video {
                    url: file.url_private.clone(),
                    duration: None,
                }
            } else {
                MessageContent::File {
                    url: file.url_private.clone(),
                    name: file.name.clone(),
                }
            }
        } else {
            MessageContent::Text {
                text: msg.text.clone(),
            }
        };

        // Parse timestamp (format: "1234567890.123456")
        let timestamp = msg
            .ts
            .split('.')
            .next()
            .and_then(|s| s.parse::<i64>().ok())
            .and_then(|secs| chrono::DateTime::from_timestamp(secs, 0))
            .unwrap_or_else(chrono::Utc::now);

        Some(IncomingMessage {
            id: msg.ts.clone(),
            channel: "slack".to_string(),
            chat_id: msg.channel.clone(),
            user_id,
            user_name: None, // Would need additional API call to get username
            content,
            timestamp,
            metadata: serde_json::json!({
                "ts": msg.ts,
                "thread_ts": msg.thread_ts,
                "is_mention": is_mention,
            }),
        })
    }
}

#[async_trait]
impl Channel for SlackChannel {
    fn id(&self) -> &str {
        "slack"
    }

    fn name(&self) -> &str {
        "Slack"
    }

    async fn start(&mut self, event_tx: mpsc::Sender<ChannelEvent>) -> anyhow::Result<()> {
        if self.config.bot_token.is_empty() {
            return Err(anyhow::anyhow!("Slack bot token is required"));
        }
        if self.config.app_token.is_empty() {
            return Err(anyhow::anyhow!("Slack app token is required for Socket Mode"));
        }

        // Get bot user ID for filtering self-messages
        let bot_user_id = self.get_bot_user_id().await?;
        {
            let mut id = self.bot_user_id.write().await;
            *id = Some(bot_user_id.clone());
        }

        // Get WebSocket URL
        let ws_url = self.get_websocket_url().await?;

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        let connected = self.connected.clone();
        let config = self.config.clone();
        let bot_user_id_clone = self.bot_user_id.clone();

        tokio::spawn(async move {
            match tokio_tungstenite::connect_async(&ws_url).await {
                Ok((ws_stream, _)) => {
                    connected.store(true, Ordering::SeqCst);

                    let _ = event_tx
                        .send(ChannelEvent::Connected {
                            channel: "slack".to_string(),
                        })
                        .await;

                    let (mut write, mut read) = ws_stream.split();

                    loop {
                        tokio::select! {
                            msg = read.next() => {
                                match msg {
                                    Some(Ok(WsMessage::Text(text))) => {
                                        if let Ok(envelope) = serde_json::from_str::<SocketModeEnvelope>(&text) {
                                            // Always acknowledge
                                            let ack = SocketModeAck {
                                                envelope_id: envelope.envelope_id.clone(),
                                            };
                                            if let Ok(ack_json) = serde_json::to_string(&ack) {
                                                let _ = write.send(WsMessage::Text(ack_json.into())).await;
                                            }

                                            // Process events
                                            if envelope.envelope_type == "events_api" {
                                                if let Some(payload) = envelope.payload {
                                                    if let Ok(event_payload) = serde_json::from_value::<EventPayload>(payload) {
                                                        let (msg, is_mention) = match event_payload.event {
                                                            SlackEvent::Message(m) => (Some(m), false),
                                                            SlackEvent::AppMention(m) => (Some(m), true),
                                                            SlackEvent::Unknown => (None, false),
                                                        };

                                                        if let Some(slack_msg) = msg {
                                                            // Skip messages from the bot itself
                                                            let bot_id = bot_user_id_clone.read().await;
                                                            if slack_msg.user.as_ref() == bot_id.as_ref() {
                                                                continue;
                                                            }

                                                            // Check channel allowlist
                                                            if !config.allowed_channels.is_empty()
                                                                && !config.allowed_channels.contains(&slack_msg.channel)
                                                            {
                                                                continue;
                                                            }

                                                            // Check DM setting
                                                            let is_dm = slack_msg.channel.starts_with("D");
                                                            if is_dm && !config.allow_dms {
                                                                continue;
                                                            }

                                                            // Check mentions_only setting
                                                            if config.mentions_only && !is_mention && !is_dm {
                                                                continue;
                                                            }

                                                            if let Some(incoming) = SlackChannel::convert_message(&slack_msg, is_mention) {
                                                                let _ = event_tx.send(ChannelEvent::Message(incoming)).await;
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Some(Ok(WsMessage::Ping(data))) => {
                                        let _ = write.send(WsMessage::Pong(data)).await;
                                    }
                                    Some(Ok(WsMessage::Close(_))) | None => {
                                        tracing::info!("Slack WebSocket closed");
                                        break;
                                    }
                                    Some(Err(e)) => {
                                        tracing::error!("Slack WebSocket error: {}", e);
                                        let _ = event_tx.send(ChannelEvent::Error {
                                            channel: "slack".to_string(),
                                            error: e.to_string(),
                                        }).await;
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                            _ = &mut shutdown_rx => {
                                tracing::info!("Slack channel shutting down");
                                let _ = write.send(WsMessage::Close(None)).await;
                                break;
                            }
                        }
                    }

                    let _ = event_tx
                        .send(ChannelEvent::Disconnected {
                            channel: "slack".to_string(),
                            reason: None,
                        })
                        .await;
                }
                Err(e) => {
                    tracing::error!("Failed to connect to Slack: {}", e);
                    let _ = event_tx
                        .send(ChannelEvent::Error {
                            channel: "slack".to_string(),
                            error: e.to_string(),
                        })
                        .await;
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
        let text = match &message.content {
            MessageContent::Text { text } => text.clone(),
            MessageContent::Image { url, caption } => {
                format!("{}\n{}", caption.as_deref().unwrap_or(""), url)
            }
            MessageContent::Audio { url, .. } => format!("🎵 {}", url),
            MessageContent::Video { url, .. } => format!("🎬 {}", url),
            MessageContent::File { url, name } => format!("📎 {}: {}", name, url),
            MessageContent::Location {
                latitude,
                longitude,
            } => format!(
                "📍 https://maps.google.com/?q={},{}",
                latitude, longitude
            ),
            MessageContent::Sticker { emoji, .. } => emoji.clone().unwrap_or_else(|| "🎨".to_string()),
        };

        let mut params = vec![
            ("channel", message.chat_id.clone()),
            ("text", text),
        ];

        // Reply in thread if reply_to is set
        if let Some(thread_ts) = &message.reply_to {
            params.push(("thread_ts", thread_ts.clone()));
        }

        let response: SlackResponse<PostMessageResponse> = self
            .client
            .post("https://slack.com/api/chat.postMessage")
            .header("Authorization", format!("Bearer {}", self.config.bot_token))
            .form(&params)
            .send()
            .await?
            .json()
            .await?;

        if !response.ok {
            return Err(anyhow::anyhow!(
                "Failed to send message: {}",
                response.error.unwrap_or_else(|| "Unknown error".to_string())
            ));
        }

        response
            .data
            .map(|d| d.ts)
            .ok_or_else(|| anyhow::anyhow!("No message ts in response"))
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn status(&self) -> ChannelStatus {
        let error = if self.config.bot_token.is_empty() {
            Some("Bot token not configured".to_string())
        } else if self.config.app_token.is_empty() {
            Some("App token not configured".to_string())
        } else {
            None
        };

        ChannelStatus {
            id: self.id().to_string(),
            name: self.name().to_string(),
            connected: self.is_connected(),
            error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slack_config_default() {
        let config = SlackConfig::default();
        assert!(config.bot_token.is_empty());
        assert!(config.app_token.is_empty());
        assert!(config.allowed_channels.is_empty());
        assert!(config.allow_dms);
        assert!(!config.mentions_only);
    }

    #[test]
    fn test_slack_channel_creation() {
        let config = SlackConfig {
            bot_token: "xoxb-test".to_string(),
            app_token: "xapp-test".to_string(),
            ..Default::default()
        };

        let channel = SlackChannel::new(config);
        assert_eq!(channel.id(), "slack");
        assert_eq!(channel.name(), "Slack");
        assert!(!channel.is_connected());
    }

    #[test]
    fn test_channel_allowed_empty_list() {
        let config = SlackConfig::default();
        let channel = SlackChannel::new(config);

        // Empty list means all channels allowed
        assert!(channel.is_channel_allowed("C1234567890"));
    }

    #[test]
    fn test_channel_allowed_with_list() {
        let config = SlackConfig {
            allowed_channels: vec!["C1234567890".to_string()],
            ..Default::default()
        };
        let channel = SlackChannel::new(config);

        assert!(channel.is_channel_allowed("C1234567890"));
        assert!(!channel.is_channel_allowed("C9876543210"));
    }

    #[test]
    fn test_channel_status_no_tokens() {
        let config = SlackConfig::default();
        let channel = SlackChannel::new(config);

        let status = channel.status();
        assert!(!status.connected);
        assert!(status.error.is_some());
        assert!(status.error.unwrap().contains("Bot token"));
    }

    #[test]
    fn test_channel_status_no_app_token() {
        let config = SlackConfig {
            bot_token: "xoxb-test".to_string(),
            ..Default::default()
        };
        let channel = SlackChannel::new(config);

        let status = channel.status();
        assert!(!status.connected);
        assert!(status.error.is_some());
        assert!(status.error.unwrap().contains("App token"));
    }

    #[test]
    fn test_channel_status_with_tokens() {
        let config = SlackConfig {
            bot_token: "xoxb-test".to_string(),
            app_token: "xapp-test".to_string(),
            ..Default::default()
        };
        let channel = SlackChannel::new(config);

        let status = channel.status();
        assert!(!status.connected);
        assert!(status.error.is_none());
    }

    #[test]
    fn test_convert_message_text() {
        let slack_msg = SlackMessage {
            channel: "C123".to_string(),
            user: Some("U456".to_string()),
            text: "Hello world".to_string(),
            ts: "1234567890.123456".to_string(),
            thread_ts: None,
            files: vec![],
            subtype: None,
        };

        let msg = SlackChannel::convert_message(&slack_msg, false).unwrap();
        assert_eq!(msg.channel, "slack");
        assert_eq!(msg.chat_id, "C123");
        assert_eq!(msg.user_id, "U456");
        assert!(msg.content.is_text());
        assert_eq!(msg.content.as_text(), Some("Hello world"));
    }

    #[test]
    fn test_convert_message_skips_subtype() {
        let slack_msg = SlackMessage {
            channel: "C123".to_string(),
            user: Some("U456".to_string()),
            text: "Changed topic".to_string(),
            ts: "1234567890.123456".to_string(),
            thread_ts: None,
            files: vec![],
            subtype: Some("channel_topic".to_string()),
        };

        let msg = SlackChannel::convert_message(&slack_msg, false);
        assert!(msg.is_none());
    }

    #[test]
    fn test_convert_message_mention() {
        let slack_msg = SlackMessage {
            channel: "C123".to_string(),
            user: Some("U456".to_string()),
            text: "<@U789> hello bot".to_string(),
            ts: "1234567890.123456".to_string(),
            thread_ts: None,
            files: vec![],
            subtype: None,
        };

        let msg = SlackChannel::convert_message(&slack_msg, true).unwrap();
        assert_eq!(msg.metadata["is_mention"], true);
    }
}
