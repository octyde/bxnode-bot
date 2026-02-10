//! LINE channel implementation using Messaging API
//!
//! LINE Messaging API documentation:
//! https://developers.line.biz/en/docs/messaging-api/

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::{
    Channel, ChannelEvent, ChannelStatus, IncomingMessage, MessageContent, OutgoingMessage,
    
};

/// LINE channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineConfig {
    /// Channel access token (long-lived)
    pub channel_access_token: String,

    /// Channel secret (for signature verification)
    pub channel_secret: String,

    /// Webhook port (for receiving messages)
    #[serde(default = "default_webhook_port")]
    pub webhook_port: u16,

    /// Allowed user IDs (if empty, allows all)
    #[serde(default)]
    pub allowed_users: Vec<String>,

    /// Allowed group IDs (if empty, allows all)
    #[serde(default)]
    pub allowed_groups: Vec<String>,
}

fn default_webhook_port() -> u16 {
    8080
}

impl Default for LineConfig {
    fn default() -> Self {
        Self {
            channel_access_token: String::new(),
            channel_secret: String::new(),
            webhook_port: default_webhook_port(),
            allowed_users: vec![],
            allowed_groups: vec![],
        }
    }
}

/// LINE channel
pub struct LineChannel {
    config: LineConfig,
    client: reqwest::Client,
    connected: Arc<AtomicBool>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl LineChannel {
    /// Create a new LINE channel
    pub fn new(config: LineConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            connected: Arc::new(AtomicBool::new(false)),
            shutdown_tx: None,
        }
    }

    /// Check if a user is allowed
    fn is_user_allowed(&self, user_id: &str) -> bool {
        self.config.allowed_users.is_empty() || self.config.allowed_users.contains(&user_id.to_string())
    }

    /// Check if a group is allowed
    fn is_group_allowed(&self, group_id: &str) -> bool {
        self.config.allowed_groups.is_empty() || self.config.allowed_groups.contains(&group_id.to_string())
    }

    /// Convert LINE webhook event to IncomingMessage
    fn convert_event(event: &LineWebhookEvent) -> Option<IncomingMessage> {
        // Only handle message events
        if event.event_type != "message" {
            return None;
        }

        let message = event.message.as_ref()?;
        let source = event.source.as_ref()?;

        let (chat_id, user_id) = match source.source_type.as_str() {
            "user" => {
                let uid = source.user_id.as_ref()?;
                (uid.clone(), uid.clone())
            }
            "group" => {
                let gid = source.group_id.as_ref()?;
                let uid = source.user_id.as_ref().cloned().unwrap_or_else(|| "unknown".to_string());
                (gid.clone(), uid)
            }
            "room" => {
                let rid = source.room_id.as_ref()?;
                let uid = source.user_id.as_ref().cloned().unwrap_or_else(|| "unknown".to_string());
                (rid.clone(), uid)
            }
            _ => return None,
        };

        let content = match message.message_type.as_str() {
            "text" => MessageContent::Text {
                text: message.text.clone().unwrap_or_default(),
            },
            "image" => MessageContent::Image {
                url: message.id.clone(),
                caption: None,
            },
            "video" => MessageContent::Video {
                url: message.id.clone(),
                duration: message.duration.map(|d| d as u32),
            },
            "audio" => MessageContent::Audio {
                url: message.id.clone(),
                duration: message.duration.map(|d| d as u32),
            },
            "file" => MessageContent::File {
                url: message.id.clone(),
                name: message.file_name.clone().unwrap_or_else(|| "file".to_string()),
            },
            "location" => MessageContent::Location {
                latitude: message.latitude.unwrap_or(0.0),
                longitude: message.longitude.unwrap_or(0.0),
            },
            "sticker" => MessageContent::Sticker {
                url: format!("sticker:{}:{}", message.package_id.as_deref().unwrap_or(""), message.sticker_id.as_deref().unwrap_or("")),
                emoji: None,
            },
            _ => return None,
        };

        Some(IncomingMessage {
            id: message.id.clone(),
            channel: "line".to_string(),
            chat_id,
            user_id,
            user_name: None, // LINE doesn't include user name in webhook
            content,
            timestamp: chrono::DateTime::from_timestamp_millis(event.timestamp)
                .unwrap_or_else(chrono::Utc::now),
            metadata: serde_json::json!({
                "reply_token": event.reply_token,
                "source_type": source.source_type,
            }),
        })
    }
}

#[async_trait]
impl Channel for LineChannel {
    fn id(&self) -> &str {
        "line"
    }

    fn name(&self) -> &str {
        "LINE"
    }

    async fn start(&mut self, event_tx: mpsc::Sender<ChannelEvent>) -> anyhow::Result<()> {
        if self.config.channel_access_token.is_empty() {
            return Err(anyhow::anyhow!("LINE channel access token is required"));
        }

        let connected = self.connected.clone();
        let config = self.config.clone();
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        // Spawn webhook server
        tokio::spawn(async move {
            use axum::{extract::State, routing::post, Json, Router};

            #[derive(Clone)]
            struct WebhookState {
                event_tx: mpsc::Sender<ChannelEvent>,
                config: LineConfig,
            }

            async fn webhook_handler(
                State(state): State<WebhookState>,
                Json(payload): Json<LineWebhookPayload>,
            ) -> &'static str {
                for event in payload.events {
                    // Check permissions
                    if let Some(ref source) = event.source {
                        if let Some(ref user_id) = source.user_id {
                            if !state.config.allowed_users.is_empty()
                                && !state.config.allowed_users.contains(user_id)
                            {
                                continue;
                            }
                        }
                        if let Some(ref group_id) = source.group_id {
                            if !state.config.allowed_groups.is_empty()
                                && !state.config.allowed_groups.contains(group_id)
                            {
                                continue;
                            }
                        }
                    }

                    if let Some(incoming) = LineChannel::convert_event(&event) {
                        let _ = state.event_tx.send(ChannelEvent::Message(incoming)).await;
                    }
                }
                "OK"
            }

            let state = WebhookState {
                event_tx: event_tx.clone(),
                config,
            };

            let app = Router::new()
                .route("/webhook/line", post(webhook_handler))
                .with_state(state);

            connected.store(true, Ordering::SeqCst);
            let _ = event_tx
                .send(ChannelEvent::Connected {
                    channel: "line".to_string(),
                })
                .await;

            let addr = format!("0.0.0.0:{}", default_webhook_port());
            tracing::info!("LINE webhook server listening on {}", addr);

            let listener = match tokio::net::TcpListener::bind(&addr).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!("Failed to bind LINE webhook: {}", e);
                    connected.store(false, Ordering::SeqCst);
                    return;
                }
            };

            tokio::select! {
                result = axum::serve(listener, app) => {
                    if let Err(e) = result {
                        tracing::error!("LINE webhook server error: {}", e);
                    }
                }
                _ = &mut shutdown_rx => {
                    tracing::info!("LINE channel shutting down");
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
            _ => return Err(anyhow::anyhow!("LINE only supports text messages via push API")),
        };

        let body = serde_json::json!({
            "to": message.chat_id,
            "messages": [{
                "type": "text",
                "text": text
            }]
        });

        let response = self
            .client
            .post("https://api.line.me/v2/bot/message/push")
            .header("Authorization", format!("Bearer {}", self.config.channel_access_token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("LINE API error: {}", error_text));
        }

        // LINE push API doesn't return message ID
        Ok(uuid::Uuid::new_v4().to_string())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn supports_edit(&self) -> bool {
        false // LINE doesn't support message editing
    }

    fn status(&self) -> ChannelStatus {
        ChannelStatus {
            id: self.id().to_string(),
            name: self.name().to_string(),
            connected: self.is_connected(),
            error: if self.config.channel_access_token.is_empty() {
                Some("Channel access token not configured".to_string())
            } else {
                None
            },
        }
    }
}

// LINE Webhook types
#[derive(Debug, Deserialize)]
struct LineWebhookPayload {
    events: Vec<LineWebhookEvent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LineWebhookEvent {
    #[serde(rename = "type")]
    event_type: String,
    timestamp: i64,
    source: Option<LineSource>,
    reply_token: Option<String>,
    message: Option<LineMessage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LineSource {
    #[serde(rename = "type")]
    source_type: String,
    user_id: Option<String>,
    group_id: Option<String>,
    room_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LineMessage {
    id: String,
    #[serde(rename = "type")]
    message_type: String,
    text: Option<String>,
    file_name: Option<String>,
    duration: Option<i64>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    package_id: Option<String>,
    sticker_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_config_default() {
        let config = LineConfig::default();
        assert!(config.channel_access_token.is_empty());
        assert!(config.channel_secret.is_empty());
        assert_eq!(config.webhook_port, 8080);
        assert!(config.allowed_users.is_empty());
        assert!(config.allowed_groups.is_empty());
    }

    #[test]
    fn test_line_channel_creation() {
        let config = LineConfig {
            channel_access_token: "test_token".to_string(),
            channel_secret: "test_secret".to_string(),
            ..Default::default()
        };

        let channel = LineChannel::new(config);
        assert_eq!(channel.id(), "line");
        assert_eq!(channel.name(), "LINE");
        assert!(!channel.is_connected());
    }

    #[test]
    fn test_user_allowed_empty_list() {
        let config = LineConfig::default();
        let channel = LineChannel::new(config);

        assert!(channel.is_user_allowed("U12345"));
        assert!(channel.is_user_allowed("U67890"));
    }

    #[test]
    fn test_user_allowed_with_list() {
        let config = LineConfig {
            allowed_users: vec!["U12345".to_string(), "U67890".to_string()],
            ..Default::default()
        };
        let channel = LineChannel::new(config);

        assert!(channel.is_user_allowed("U12345"));
        assert!(channel.is_user_allowed("U67890"));
        assert!(!channel.is_user_allowed("U11111"));
    }

    #[test]
    fn test_supports_edit() {
        let config = LineConfig::default();
        let channel = LineChannel::new(config);
        assert!(!channel.supports_edit());
    }

    #[test]
    fn test_channel_status_no_token() {
        let config = LineConfig::default();
        let channel = LineChannel::new(config);

        let status = channel.status();
        assert!(!status.connected);
        assert!(status.error.is_some());
    }
}
