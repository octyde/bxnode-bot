//! Feishu (Lark) channel implementation using Bot API
//!
//! Feishu Bot API documentation:
//! https://open.feishu.cn/document/ukTMukTMukTM/uMTNz4yM1MjLzUzM

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};

use super::{
    Channel, ChannelEvent, ChannelStatus, IncomingMessage, MessageContent, OutgoingMessage,
    
};

/// Feishu channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuConfig {
    /// App ID
    pub app_id: String,

    /// App Secret
    pub app_secret: String,

    /// Verification token (for webhook verification)
    pub verification_token: String,

    /// Encrypt key (optional, for encrypted events)
    #[serde(default)]
    pub encrypt_key: Option<String>,

    /// Webhook port (for receiving messages)
    #[serde(default = "default_webhook_port")]
    pub webhook_port: u16,

    /// Allowed user IDs (if empty, allows all)
    #[serde(default)]
    pub allowed_users: Vec<String>,

    /// Allowed chat IDs (if empty, allows all)
    #[serde(default)]
    pub allowed_chats: Vec<String>,
}

fn default_webhook_port() -> u16 {
    8081
}

impl Default for FeishuConfig {
    fn default() -> Self {
        Self {
            app_id: String::new(),
            app_secret: String::new(),
            verification_token: String::new(),
            encrypt_key: None,
            webhook_port: default_webhook_port(),
            allowed_users: vec![],
            allowed_chats: vec![],
        }
    }
}

/// Feishu channel
pub struct FeishuChannel {
    config: FeishuConfig,
    client: reqwest::Client,
    connected: Arc<AtomicBool>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    access_token: Arc<RwLock<Option<String>>>,
    token_expires_at: Arc<RwLock<i64>>,
}

impl FeishuChannel {
    /// Create a new Feishu channel
    pub fn new(config: FeishuConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            connected: Arc::new(AtomicBool::new(false)),
            shutdown_tx: None,
            access_token: Arc::new(RwLock::new(None)),
            token_expires_at: Arc::new(RwLock::new(0)),
        }
    }

    /// Get or refresh tenant access token
    async fn get_access_token(&self) -> anyhow::Result<String> {
        let now = chrono::Utc::now().timestamp();

        // Check if we have a valid token
        {
            let expires_at = *self.token_expires_at.read().await;
            if expires_at > now + 60 {
                // Token still valid for at least 60 seconds
                if let Some(ref token) = *self.access_token.read().await {
                    return Ok(token.clone());
                }
            }
        }

        // Refresh token
        let body = serde_json::json!({
            "app_id": self.config.app_id,
            "app_secret": self.config.app_secret
        });

        let response = self
            .client
            .post("https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal")
            .json(&body)
            .send()
            .await?;

        let data: FeishuTokenResponse = response.json().await?;

        if data.code != 0 {
            return Err(anyhow::anyhow!("Feishu token error: {}", data.msg));
        }

        let token = data.tenant_access_token;
        let expires_at = now + data.expire as i64;

        // Store token
        *self.access_token.write().await = Some(token.clone());
        *self.token_expires_at.write().await = expires_at;

        Ok(token)
    }

    /// Check if a user is allowed
    fn is_user_allowed(&self, user_id: &str) -> bool {
        self.config.allowed_users.is_empty()
            || self.config.allowed_users.contains(&user_id.to_string())
    }

    /// Check if a chat is allowed
    fn is_chat_allowed(&self, chat_id: &str) -> bool {
        self.config.allowed_chats.is_empty()
            || self.config.allowed_chats.contains(&chat_id.to_string())
    }

    /// Convert Feishu event to IncomingMessage
    fn convert_event(event: &FeishuMessageEvent) -> Option<IncomingMessage> {
        let msg = &event.message;
        let sender = &event.sender;

        let content = match msg.message_type.as_str() {
            "text" => {
                let text_content: FeishuTextContent =
                    serde_json::from_str(&msg.content).ok()?;
                MessageContent::Text {
                    text: text_content.text,
                }
            }
            "image" => {
                let image_content: FeishuImageContent =
                    serde_json::from_str(&msg.content).ok()?;
                MessageContent::Image {
                    url: image_content.image_key,
                    caption: None,
                }
            }
            "file" => {
                let file_content: FeishuFileContent =
                    serde_json::from_str(&msg.content).ok()?;
                MessageContent::File {
                    url: file_content.file_key,
                    name: file_content.file_name,
                }
            }
            "audio" => {
                let audio_content: FeishuAudioContent =
                    serde_json::from_str(&msg.content).ok()?;
                MessageContent::Audio {
                    url: audio_content.file_key,
                    duration: audio_content.duration.map(|d| d as u32),
                }
            }
            "media" => {
                let media_content: FeishuMediaContent =
                    serde_json::from_str(&msg.content).ok()?;
                MessageContent::Video {
                    url: media_content.file_key,
                    duration: media_content.duration.map(|d| d as u32),
                }
            }
            "location" => {
                let loc_content: FeishuLocationContent =
                    serde_json::from_str(&msg.content).ok()?;
                MessageContent::Location {
                    latitude: loc_content.latitude.parse().unwrap_or(0.0),
                    longitude: loc_content.longitude.parse().unwrap_or(0.0),
                }
            }
            "sticker" => {
                let sticker_content: FeishuStickerContent =
                    serde_json::from_str(&msg.content).ok()?;
                MessageContent::Sticker {
                    url: sticker_content.file_key,
                    emoji: None,
                }
            }
            _ => return None,
        };

        let timestamp = msg
            .create_time
            .parse::<i64>()
            .ok()
            .and_then(|ts| chrono::DateTime::from_timestamp_millis(ts))
            .unwrap_or_else(chrono::Utc::now);

        Some(IncomingMessage {
            id: msg.message_id.clone(),
            channel: "feishu".to_string(),
            chat_id: msg.chat_id.clone(),
            user_id: sender.sender_id.open_id.clone(),
            user_name: None, // Would need separate API call to get name
            content,
            timestamp,
            metadata: serde_json::json!({
                "chat_type": msg.chat_type,
                "sender_type": sender.sender_type,
                "tenant_key": sender.tenant_key,
            }),
        })
    }
}

#[async_trait]
impl Channel for FeishuChannel {
    fn id(&self) -> &str {
        "feishu"
    }

    fn name(&self) -> &str {
        "Feishu"
    }

    async fn start(&mut self, event_tx: mpsc::Sender<ChannelEvent>) -> anyhow::Result<()> {
        if self.config.app_id.is_empty() || self.config.app_secret.is_empty() {
            return Err(anyhow::anyhow!("Feishu app_id and app_secret are required"));
        }

        // Verify we can get an access token
        self.get_access_token().await?;

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
                config: FeishuConfig,
            }

            async fn webhook_handler(
                State(state): State<WebhookState>,
                Json(payload): Json<serde_json::Value>,
            ) -> Json<serde_json::Value> {
                // Handle URL verification challenge
                if let Some(challenge) = payload.get("challenge").and_then(|v| v.as_str()) {
                    return Json(serde_json::json!({ "challenge": challenge }));
                }

                // Handle event callback
                if let Ok(event_wrapper) = serde_json::from_value::<FeishuEventWrapper>(payload) {
                    // Verify token
                    if event_wrapper.header.token != state.config.verification_token {
                        tracing::warn!("Invalid Feishu verification token");
                        return Json(serde_json::json!({}));
                    }

                    // Only handle im.message.receive_v1 events
                    if event_wrapper.header.event_type == "im.message.receive_v1" {
                        if let Ok(event) = serde_json::from_value::<FeishuMessageEvent>(event_wrapper.event) {
                            // Check permissions
                            if !state.config.allowed_users.is_empty()
                                && !state.config.allowed_users.contains(&event.sender.sender_id.open_id)
                            {
                                return Json(serde_json::json!({}));
                            }

                            if !state.config.allowed_chats.is_empty()
                                && !state.config.allowed_chats.contains(&event.message.chat_id)
                            {
                                return Json(serde_json::json!({}));
                            }

                            if let Some(incoming) = FeishuChannel::convert_event(&event) {
                                let _ = state.event_tx.send(ChannelEvent::Message(incoming)).await;
                            }
                        }
                    }
                }

                Json(serde_json::json!({}))
            }

            let state = WebhookState {
                event_tx: event_tx.clone(),
                config,
            };

            let app = Router::new()
                .route("/webhook/feishu", post(webhook_handler))
                .with_state(state);

            connected.store(true, Ordering::SeqCst);
            let _ = event_tx
                .send(ChannelEvent::Connected {
                    channel: "feishu".to_string(),
                })
                .await;

            let addr = format!("0.0.0.0:{}", default_webhook_port());
            tracing::info!("Feishu webhook server listening on {}", addr);

            let listener = match tokio::net::TcpListener::bind(&addr).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!("Failed to bind Feishu webhook: {}", e);
                    connected.store(false, Ordering::SeqCst);
                    return;
                }
            };

            tokio::select! {
                result = axum::serve(listener, app) => {
                    if let Err(e) = result {
                        tracing::error!("Feishu webhook server error: {}", e);
                    }
                }
                _ = &mut shutdown_rx => {
                    tracing::info!("Feishu channel shutting down");
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
        let token = self.get_access_token().await?;

        let (msg_type, content) = match &message.content {
            MessageContent::Text { text } => {
                ("text", serde_json::json!({ "text": text }))
            }
            MessageContent::Image { url, .. } => {
                ("image", serde_json::json!({ "image_key": url }))
            }
            MessageContent::File { url, name } => {
                ("file", serde_json::json!({ "file_key": url }))
            }
            _ => return Err(anyhow::anyhow!("Unsupported message type for Feishu")),
        };

        let body = serde_json::json!({
            "receive_id": message.chat_id,
            "msg_type": msg_type,
            "content": content.to_string()
        });

        // Determine receive_id_type based on chat_id format
        let receive_id_type = if message.chat_id.starts_with("oc_") {
            "chat_id"
        } else if message.chat_id.starts_with("ou_") {
            "open_id"
        } else if message.chat_id.starts_with("on_") {
            "union_id"
        } else if message.chat_id.contains("@") {
            "email"
        } else {
            "open_id"
        };

        let url = format!(
            "https://open.feishu.cn/open-apis/im/v1/messages?receive_id_type={}",
            receive_id_type
        );

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json; charset=utf-8")
            .json(&body)
            .send()
            .await?;

        let data: FeishuApiResponse = response.json().await?;

        if data.code != 0 {
            return Err(anyhow::anyhow!("Feishu API error: {}", data.msg));
        }

        Ok(data.data.map(|d| d.message_id).unwrap_or_default())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn supports_edit(&self) -> bool {
        true // Feishu supports message editing
    }

    async fn edit_message(
        &self,
        _chat_id: &str,
        message_id: &str,
        new_content: &str,
    ) -> anyhow::Result<()> {
        let token = self.get_access_token().await?;

        let body = serde_json::json!({
            "content": serde_json::json!({ "text": new_content }).to_string()
        });

        let url = format!(
            "https://open.feishu.cn/open-apis/im/v1/messages/{}",
            message_id
        );

        let response = self
            .client
            .patch(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json; charset=utf-8")
            .json(&body)
            .send()
            .await?;

        let data: FeishuApiResponse = response.json().await?;

        if data.code != 0 {
            return Err(anyhow::anyhow!("Feishu API error: {}", data.msg));
        }

        Ok(())
    }

    fn status(&self) -> ChannelStatus {
        ChannelStatus {
            id: self.id().to_string(),
            name: self.name().to_string(),
            connected: self.is_connected(),
            error: if self.config.app_id.is_empty() || self.config.app_secret.is_empty() {
                Some("App ID and secret not configured".to_string())
            } else {
                None
            },
        }
    }
}

// Feishu API types
#[derive(Debug, Deserialize)]
struct FeishuTokenResponse {
    code: i32,
    msg: String,
    tenant_access_token: String,
    expire: i32,
}

#[derive(Debug, Deserialize)]
struct FeishuApiResponse {
    code: i32,
    msg: String,
    data: Option<FeishuMessageData>,
}

#[derive(Debug, Deserialize)]
struct FeishuMessageData {
    message_id: String,
}

#[derive(Debug, Deserialize)]
struct FeishuEventWrapper {
    header: FeishuEventHeader,
    event: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct FeishuEventHeader {
    event_type: String,
    token: String,
}

#[derive(Debug, Deserialize)]
struct FeishuMessageEvent {
    sender: FeishuSender,
    message: FeishuMessage,
}

#[derive(Debug, Deserialize)]
struct FeishuSender {
    sender_id: FeishuSenderId,
    sender_type: String,
    tenant_key: String,
}

#[derive(Debug, Deserialize)]
struct FeishuSenderId {
    open_id: String,
}

#[derive(Debug, Deserialize)]
struct FeishuMessage {
    message_id: String,
    chat_id: String,
    chat_type: String,
    message_type: String,
    content: String,
    create_time: String,
}

// Content types
#[derive(Debug, Deserialize)]
struct FeishuTextContent {
    text: String,
}

#[derive(Debug, Deserialize)]
struct FeishuImageContent {
    image_key: String,
}

#[derive(Debug, Deserialize)]
struct FeishuFileContent {
    file_key: String,
    file_name: String,
}

#[derive(Debug, Deserialize)]
struct FeishuAudioContent {
    file_key: String,
    duration: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct FeishuMediaContent {
    file_key: String,
    duration: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct FeishuLocationContent {
    latitude: String,
    longitude: String,
}

#[derive(Debug, Deserialize)]
struct FeishuStickerContent {
    file_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feishu_config_default() {
        let config = FeishuConfig::default();
        assert!(config.app_id.is_empty());
        assert!(config.app_secret.is_empty());
        assert!(config.verification_token.is_empty());
        assert_eq!(config.webhook_port, 8081);
        assert!(config.allowed_users.is_empty());
        assert!(config.allowed_chats.is_empty());
    }

    #[test]
    fn test_feishu_channel_creation() {
        let config = FeishuConfig {
            app_id: "test_app_id".to_string(),
            app_secret: "test_app_secret".to_string(),
            verification_token: "test_token".to_string(),
            ..Default::default()
        };

        let channel = FeishuChannel::new(config);
        assert_eq!(channel.id(), "feishu");
        assert_eq!(channel.name(), "Feishu");
        assert!(!channel.is_connected());
    }

    #[test]
    fn test_user_allowed_empty_list() {
        let config = FeishuConfig::default();
        let channel = FeishuChannel::new(config);

        assert!(channel.is_user_allowed("ou_12345"));
        assert!(channel.is_user_allowed("ou_67890"));
    }

    #[test]
    fn test_user_allowed_with_list() {
        let config = FeishuConfig {
            allowed_users: vec!["ou_12345".to_string(), "ou_67890".to_string()],
            ..Default::default()
        };
        let channel = FeishuChannel::new(config);

        assert!(channel.is_user_allowed("ou_12345"));
        assert!(channel.is_user_allowed("ou_67890"));
        assert!(!channel.is_user_allowed("ou_11111"));
    }

    #[test]
    fn test_supports_edit() {
        let config = FeishuConfig::default();
        let channel = FeishuChannel::new(config);
        assert!(channel.supports_edit());
    }

    #[test]
    fn test_channel_status_no_config() {
        let config = FeishuConfig::default();
        let channel = FeishuChannel::new(config);

        let status = channel.status();
        assert!(!status.connected);
        assert!(status.error.is_some());
    }
}
