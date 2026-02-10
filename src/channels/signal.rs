//! Signal channel implementation using signal-cli REST API
//!
//! This implementation uses signal-cli-rest-api as a bridge:
//! https://github.com/bbernhard/signal-cli-rest-api
//!
//! You need to run signal-cli-rest-api separately and point this channel to it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::{
    Channel, ChannelEvent, ChannelStatus, IncomingMessage, MessageContent, OutgoingMessage,
    
};

/// Signal channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalConfig {
    /// signal-cli-rest-api base URL
    #[serde(default = "default_api_url")]
    pub api_url: String,

    /// Registered phone number (e.g., "+1234567890")
    pub phone_number: String,

    /// Allowed phone numbers (if empty, allows all)
    #[serde(default)]
    pub allowed_numbers: Vec<String>,

    /// Allowed group IDs (if empty, allows all)
    #[serde(default)]
    pub allowed_groups: Vec<String>,

    /// Polling interval in seconds
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
}

fn default_api_url() -> String {
    "http://localhost:8080".to_string()
}

fn default_poll_interval() -> u64 {
    2
}

impl Default for SignalConfig {
    fn default() -> Self {
        Self {
            api_url: default_api_url(),
            phone_number: String::new(),
            allowed_numbers: vec![],
            allowed_groups: vec![],
            poll_interval_secs: default_poll_interval(),
        }
    }
}

/// Signal channel
pub struct SignalChannel {
    config: SignalConfig,
    client: reqwest::Client,
    connected: Arc<AtomicBool>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl SignalChannel {
    /// Create a new Signal channel
    pub fn new(config: SignalConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            connected: Arc::new(AtomicBool::new(false)),
            shutdown_tx: None,
        }
    }

    /// Check if a phone number is allowed
    fn is_number_allowed(&self, number: &str) -> bool {
        self.config.allowed_numbers.is_empty()
            || self.config.allowed_numbers.contains(&number.to_string())
    }

    /// Check if a group is allowed
    fn is_group_allowed(&self, group_id: &str) -> bool {
        self.config.allowed_groups.is_empty()
            || self.config.allowed_groups.contains(&group_id.to_string())
    }

    /// Convert signal-cli message to IncomingMessage
    fn convert_message(msg: &SignalMessage, phone_number: &str) -> Option<IncomingMessage> {
        let envelope = &msg.envelope;

        // Determine chat_id and user_id
        let (chat_id, user_id) = if let Some(ref group) = envelope.data_message.as_ref().and_then(|d| d.group_info.as_ref()) {
            (group.group_id.clone(), envelope.source.clone())
        } else {
            (envelope.source.clone(), envelope.source.clone())
        };

        // Get message content
        let data_message = envelope.data_message.as_ref()?;

        let content = if let Some(ref text) = data_message.message {
            MessageContent::Text { text: text.clone() }
        } else if let Some(ref attachments) = data_message.attachments {
            if let Some(attachment) = attachments.first() {
                match attachment.content_type.as_str() {
                    t if t.starts_with("image/") => MessageContent::Image {
                        url: attachment.id.clone().unwrap_or_default(),
                        caption: data_message.message.clone(),
                    },
                    t if t.starts_with("video/") => MessageContent::Video {
                        url: attachment.id.clone().unwrap_or_default(),
                        duration: None,
                    },
                    t if t.starts_with("audio/") => MessageContent::Audio {
                        url: attachment.id.clone().unwrap_or_default(),
                        duration: None,
                    },
                    _ => MessageContent::File {
                        url: attachment.id.clone().unwrap_or_default(),
                        name: attachment.filename.clone().unwrap_or_else(|| "file".to_string()),
                    },
                }
            } else {
                return None;
            }
        } else {
            return None;
        };

        Some(IncomingMessage {
            id: uuid::Uuid::new_v4().to_string(),
            channel: "signal".to_string(),
            chat_id,
            user_id: user_id.clone(),
            user_name: envelope.source_name.clone(),
            content,
            timestamp: chrono::DateTime::from_timestamp_millis(envelope.timestamp)
                .unwrap_or_else(chrono::Utc::now),
            metadata: serde_json::json!({
                "source": envelope.source,
                "source_device": envelope.source_device,
            }),
        })
    }
}

#[async_trait]
impl Channel for SignalChannel {
    fn id(&self) -> &str {
        "signal"
    }

    fn name(&self) -> &str {
        "Signal"
    }

    async fn start(&mut self, event_tx: mpsc::Sender<ChannelEvent>) -> anyhow::Result<()> {
        if self.config.phone_number.is_empty() {
            return Err(anyhow::anyhow!("Signal phone number is required"));
        }

        let connected = self.connected.clone();
        let config = self.config.clone();
        let client = self.client.clone();
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        // Spawn message polling task
        tokio::spawn(async move {
            connected.store(true, Ordering::SeqCst);
            let _ = event_tx
                .send(ChannelEvent::Connected {
                    channel: "signal".to_string(),
                })
                .await;

            let poll_url = format!(
                "{}/v1/receive/{}",
                config.api_url,
                urlencoding::encode(&config.phone_number)
            );

            let poll_interval = tokio::time::Duration::from_secs(config.poll_interval_secs);

            loop {
                tokio::select! {
                    _ = tokio::time::sleep(poll_interval) => {
                        // Poll for new messages
                        match client.get(&poll_url).send().await {
                            Ok(response) => {
                                if response.status().is_success() {
                                    if let Ok(messages) = response.json::<Vec<SignalMessage>>().await {
                                        for msg in messages {
                                            // Check permissions
                                            if !config.allowed_numbers.is_empty() {
                                                if !config.allowed_numbers.contains(&msg.envelope.source) {
                                                    continue;
                                                }
                                            }

                                            if let Some(ref data_msg) = msg.envelope.data_message {
                                                if let Some(ref group) = data_msg.group_info {
                                                    if !config.allowed_groups.is_empty()
                                                        && !config.allowed_groups.contains(&group.group_id)
                                                    {
                                                        continue;
                                                    }
                                                }
                                            }

                                            if let Some(incoming) = SignalChannel::convert_message(&msg, &config.phone_number) {
                                                let _ = event_tx.send(ChannelEvent::Message(incoming)).await;
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to poll Signal messages: {}", e);
                            }
                        }
                    }
                    _ = &mut shutdown_rx => {
                        tracing::info!("Signal channel shutting down");
                        break;
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
        Ok(())
    }

    async fn send(&self, message: OutgoingMessage) -> anyhow::Result<String> {
        let text = match &message.content {
            MessageContent::Text { text } => text.clone(),
            _ => return Err(anyhow::anyhow!("Signal only supports text messages")),
        };

        // Determine if this is a group or direct message
        let is_group = message.chat_id.starts_with("group.");

        let body = if is_group {
            serde_json::json!({
                "message": text,
                "number": self.config.phone_number,
                "recipients": [message.chat_id]
            })
        } else {
            serde_json::json!({
                "message": text,
                "number": self.config.phone_number,
                "recipients": [message.chat_id]
            })
        };

        let url = format!(
            "{}/v2/send",
            self.config.api_url
        );

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Signal API error: {}", error_text));
        }

        Ok(uuid::Uuid::new_v4().to_string())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn supports_edit(&self) -> bool {
        false // Signal doesn't support message editing via API
    }

    fn status(&self) -> ChannelStatus {
        ChannelStatus {
            id: self.id().to_string(),
            name: self.name().to_string(),
            connected: self.is_connected(),
            error: if self.config.phone_number.is_empty() {
                Some("Phone number not configured".to_string())
            } else {
                None
            },
        }
    }
}

// signal-cli-rest-api types
#[derive(Debug, Deserialize)]
struct SignalMessage {
    envelope: SignalEnvelope,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SignalEnvelope {
    source: String,
    source_name: Option<String>,
    source_device: Option<i32>,
    timestamp: i64,
    data_message: Option<SignalDataMessage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SignalDataMessage {
    message: Option<String>,
    timestamp: Option<i64>,
    group_info: Option<SignalGroupInfo>,
    attachments: Option<Vec<SignalAttachment>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SignalGroupInfo {
    group_id: String,
    #[serde(rename = "type")]
    group_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SignalAttachment {
    content_type: String,
    filename: Option<String>,
    id: Option<String>,
    size: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_config_default() {
        let config = SignalConfig::default();
        assert_eq!(config.api_url, "http://localhost:8080");
        assert!(config.phone_number.is_empty());
        assert!(config.allowed_numbers.is_empty());
        assert!(config.allowed_groups.is_empty());
        assert_eq!(config.poll_interval_secs, 2);
    }

    #[test]
    fn test_signal_channel_creation() {
        let config = SignalConfig {
            phone_number: "+1234567890".to_string(),
            ..Default::default()
        };

        let channel = SignalChannel::new(config);
        assert_eq!(channel.id(), "signal");
        assert_eq!(channel.name(), "Signal");
        assert!(!channel.is_connected());
    }

    #[test]
    fn test_number_allowed_empty_list() {
        let config = SignalConfig::default();
        let channel = SignalChannel::new(config);

        assert!(channel.is_number_allowed("+1234567890"));
        assert!(channel.is_number_allowed("+0987654321"));
    }

    #[test]
    fn test_number_allowed_with_list() {
        let config = SignalConfig {
            allowed_numbers: vec!["+1234567890".to_string(), "+0987654321".to_string()],
            ..Default::default()
        };
        let channel = SignalChannel::new(config);

        assert!(channel.is_number_allowed("+1234567890"));
        assert!(channel.is_number_allowed("+0987654321"));
        assert!(!channel.is_number_allowed("+1111111111"));
    }

    #[test]
    fn test_supports_edit() {
        let config = SignalConfig::default();
        let channel = SignalChannel::new(config);
        assert!(!channel.supports_edit());
    }

    #[test]
    fn test_channel_status_no_number() {
        let config = SignalConfig::default();
        let channel = SignalChannel::new(config);

        let status = channel.status();
        assert!(!status.connected);
        assert!(status.error.is_some());
    }
}
