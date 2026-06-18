//! Local channel — in-process direct connection.
//!
//! Unlike network channels (Telegram, Discord, etc.), this channel has no
//! external transport. Incoming messages are injected via [`LocalChannel::inject`]
//! (e.g. from an HTTP endpoint, the desktop UI, or a CLI), and outgoing
//! replies are emitted on a broadcast so any local subscriber can receive them.
//!
//! Typical use: the Tauri/desktop UI talks to the agent pipeline directly
//! without needing a third-party messaging service.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, RwLock};

use super::{
    Channel, ChannelEvent, ChannelStatus, IncomingMessage, MessageContent, OutgoingMessage,
};

/// Channel identifier used throughout the system.
pub const LOCAL_CHANNEL_ID: &str = "local";

/// Human-readable channel name.
pub const LOCAL_CHANNEL_NAME: &str = "Local";

/// Which configured model to pick for a given incoming message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    Main,
    Vision,
    Coding,
    Audio,
}

impl ModelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelKind::Main => "main",
            ModelKind::Vision => "vision",
            ModelKind::Coding => "coding",
            ModelKind::Audio => "audio",
        }
    }

    /// Pick a model kind from an incoming message's content. Text / file /
    /// location / sticker all map to Main — only image and audio force a
    /// different kind.
    pub fn from_content(content: &MessageContent) -> Self {
        match content {
            MessageContent::Image { .. } => ModelKind::Vision,
            MessageContent::Audio { .. } => ModelKind::Audio,
            _ => ModelKind::Main,
        }
    }
}

/// Error returned when the local channel cannot resolve a model for a
/// message. Every branch is a hard stop — we never silently downgrade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelResolveError {
    /// `main` is not configured on this channel. The channel is unusable.
    MainNotConfigured,
    /// A specialized kind (vision / coding / audio) is required by the
    /// incoming message but not configured on this channel.
    KindNotConfigured { kind: ModelKind },
}

impl std::fmt::Display for ModelResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelResolveError::MainNotConfigured => {
                write!(
                    f,
                    "Local channel has no 'main' model configured. Open the \
                     channel settings and pick a main model before sending."
                )
            }
            ModelResolveError::KindNotConfigured { kind } => {
                write!(
                    f,
                    "Local channel has no '{}' model configured, but this \
                     message requires one. Configure a '{}' model in the \
                     channel settings.",
                    kind.as_str(),
                    kind.as_str()
                )
            }
        }
    }
}

impl std::error::Error for ModelResolveError {}

/// Local channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConfig {
    /// Channel display name (defaults to "Local").
    #[serde(default = "default_name")]
    pub name: String,

    /// Capacity of the outgoing broadcast buffer.
    #[serde(default = "default_outgoing_capacity")]
    pub outgoing_capacity: usize,

    /// Per-kind model configuration. `main` is required for the channel
    /// to accept any message; other kinds are only checked when the
    /// incoming message requires them.
    #[serde(default)]
    pub models: crate::config::ChannelModels,
}

fn default_name() -> String {
    LOCAL_CHANNEL_NAME.to_string()
}

fn default_outgoing_capacity() -> usize {
    256
}

impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            name: default_name(),
            outgoing_capacity: default_outgoing_capacity(),
            models: crate::config::ChannelModels::default(),
        }
    }
}

/// An outbound message emitted by the local channel.
#[derive(Debug, Clone)]
pub struct LocalOutgoing {
    pub message_id: String,
    pub message: OutgoingMessage,
}

/// Cheaply-cloneable handle for injecting incoming messages and
/// subscribing to outgoing replies. Shared with HTTP/SSE endpoints and
/// any in-process consumer that doesn't need the full `Channel` trait.
#[derive(Clone)]
pub struct LocalChannelHandle {
    connected: Arc<AtomicBool>,
    event_tx: Arc<RwLock<Option<mpsc::Sender<ChannelEvent>>>>,
    outgoing_tx: broadcast::Sender<LocalOutgoing>,
    /// Snapshot of the channel's model configuration at registration time.
    /// Current callers (gateway dispatch) don't mutate this at runtime;
    /// a config reload re-creates the channel, which re-creates the handle.
    models: Arc<crate::config::ChannelModels>,
}

impl LocalChannelHandle {
    /// Whether the underlying channel is currently started.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Subscribe to outbound messages produced by the agent pipeline.
    pub fn subscribe(&self) -> broadcast::Receiver<LocalOutgoing> {
        self.outgoing_tx.subscribe()
    }

    /// Resolve which model to use for the given kind. Strict: returns an
    /// error if the kind isn't configured. Never falls back to `main`.
    pub fn resolve_model(&self, kind: ModelKind) -> Result<String, ModelResolveError> {
        // `main` must always be configured; without it the channel is
        // unusable regardless of what kind the message requires.
        let main = self
            .models
            .main
            .as_ref()
            .ok_or(ModelResolveError::MainNotConfigured)?;
        match kind {
            ModelKind::Main => Ok(main.clone()),
            ModelKind::Vision => self
                .models
                .vision
                .clone()
                .ok_or(ModelResolveError::KindNotConfigured { kind }),
            ModelKind::Coding => self
                .models
                .coding
                .clone()
                .ok_or(ModelResolveError::KindNotConfigured { kind }),
            ModelKind::Audio => self
                .models
                .audio
                .clone()
                .ok_or(ModelResolveError::KindNotConfigured { kind }),
        }
    }

    /// Inject an incoming message into the channel event stream.
    pub async fn inject(&self, message: IncomingMessage) -> anyhow::Result<()> {
        let guard = self.event_tx.read().await;
        let tx = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("local channel is not started"))?;
        tx.send(ChannelEvent::Message(message))
            .await
            .map_err(|e| anyhow::anyhow!("failed to queue local message: {}", e))?;
        Ok(())
    }

    /// Convenience: inject a plain-text message from the given chat/user.
    pub async fn inject_text(
        &self,
        chat_id: impl Into<String>,
        user_id: impl Into<String>,
        text: impl Into<String>,
    ) -> anyhow::Result<()> {
        let msg = IncomingMessage::text(LOCAL_CHANNEL_ID, chat_id, user_id, text);
        self.inject(msg).await
    }
}

/// Local channel — direct in-process messaging.
pub struct LocalChannel {
    name: String,
    connected: Arc<AtomicBool>,
    /// Sender used by [`LocalChannelHandle::inject`] to push an incoming
    /// message into the registered event stream. Populated on `start`.
    event_tx: Arc<RwLock<Option<mpsc::Sender<ChannelEvent>>>>,
    /// Broadcast for outbound replies produced by the agent pipeline.
    outgoing_tx: broadcast::Sender<LocalOutgoing>,
    /// Configured models for this channel, frozen at construction time.
    models: Arc<crate::config::ChannelModels>,
}

impl LocalChannel {
    /// Create a new local channel.
    pub fn new(config: LocalConfig) -> Self {
        let (outgoing_tx, _) = broadcast::channel(config.outgoing_capacity.max(1));
        Self {
            name: if config.name.is_empty() {
                LOCAL_CHANNEL_NAME.to_string()
            } else {
                config.name
            },
            connected: Arc::new(AtomicBool::new(false)),
            event_tx: Arc::new(RwLock::new(None)),
            outgoing_tx,
            models: Arc::new(config.models),
        }
    }

    /// Cheaply-cloneable handle for injection + subscription. Stash this in
    /// shared state (e.g. `AppState`) before the channel is registered —
    /// after registration the `Channel` trait object lives inside the
    /// registry and can't be cheaply re-extracted.
    pub fn handle(&self) -> LocalChannelHandle {
        LocalChannelHandle {
            connected: self.connected.clone(),
            event_tx: self.event_tx.clone(),
            outgoing_tx: self.outgoing_tx.clone(),
            models: self.models.clone(),
        }
    }

    /// Subscribe to outbound messages produced by the agent pipeline.
    pub fn subscribe(&self) -> broadcast::Receiver<LocalOutgoing> {
        self.outgoing_tx.subscribe()
    }

    /// Resolve the configured model for the given kind. See
    /// [`LocalChannelHandle::resolve_model`] for semantics.
    pub fn resolve_model(&self, kind: ModelKind) -> Result<String, ModelResolveError> {
        self.handle().resolve_model(kind)
    }

    /// Inject an incoming message into the channel event stream.
    pub async fn inject(&self, message: IncomingMessage) -> anyhow::Result<()> {
        self.handle().inject(message).await
    }

    /// Convenience: inject a plain-text message from the given chat/user.
    pub async fn inject_text(
        &self,
        chat_id: impl Into<String>,
        user_id: impl Into<String>,
        text: impl Into<String>,
    ) -> anyhow::Result<()> {
        self.handle().inject_text(chat_id, user_id, text).await
    }
}

#[async_trait]
impl Channel for LocalChannel {
    fn id(&self) -> &str {
        LOCAL_CHANNEL_ID
    }

    fn name(&self) -> &str {
        &self.name
    }

    async fn start(&mut self, event_tx: mpsc::Sender<ChannelEvent>) -> anyhow::Result<()> {
        *self.event_tx.write().await = Some(event_tx.clone());
        self.connected.store(true, Ordering::SeqCst);
        let _ = event_tx
            .send(ChannelEvent::Connected {
                channel: LOCAL_CHANNEL_ID.to_string(),
            })
            .await;
        Ok(())
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        self.connected.store(false, Ordering::SeqCst);
        *self.event_tx.write().await = None;
        Ok(())
    }

    async fn send(&self, message: OutgoingMessage) -> anyhow::Result<String> {
        if !self.connected.load(Ordering::SeqCst) {
            return Err(anyhow::anyhow!("local channel is not connected"));
        }

        let message_id = uuid::Uuid::new_v4().to_string();
        let outgoing = LocalOutgoing {
            message_id: message_id.clone(),
            message: message.clone(),
        };

        // A broadcast with no subscribers is not an error — the UI may not
        // be attached yet. Only `SendError` indicates channel closure, which
        // cannot happen while `self` is alive, so we ignore the result.
        let _ = self.outgoing_tx.send(outgoing);

        // Log the text for easy tailing without a subscriber attached.
        if let MessageContent::Text { text } = &message.content {
            tracing::debug!(
                target: "channels::local",
                chat_id = %message.chat_id,
                message_id = %message_id,
                "local outgoing text: {}",
                text
            );
        }

        Ok(message_id)
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn supports_edit(&self) -> bool {
        false
    }

    fn status(&self) -> ChannelStatus {
        ChannelStatus {
            id: LOCAL_CHANNEL_ID.to_string(),
            name: self.name.clone(),
            connected: self.is_connected(),
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = LocalConfig::default();
        assert_eq!(cfg.name, LOCAL_CHANNEL_NAME);
        assert_eq!(cfg.outgoing_capacity, 256);
    }

    #[test]
    fn test_channel_identity() {
        let ch = LocalChannel::new(LocalConfig::default());
        assert_eq!(ch.id(), LOCAL_CHANNEL_ID);
        assert_eq!(ch.name(), LOCAL_CHANNEL_NAME);
        assert!(!ch.is_connected());
        assert!(!ch.supports_edit());
    }

    #[test]
    fn test_custom_name_falls_back_when_empty() {
        let ch = LocalChannel::new(LocalConfig {
            name: String::new(),
            outgoing_capacity: 8,
            models: crate::config::ChannelModels::default(),
        });
        assert_eq!(ch.name(), LOCAL_CHANNEL_NAME);
    }

    fn cfg_with_models(models: crate::config::ChannelModels) -> LocalConfig {
        LocalConfig {
            name: LOCAL_CHANNEL_NAME.to_string(),
            outgoing_capacity: 8,
            models,
        }
    }

    #[test]
    fn test_resolve_model_main_unset() {
        let ch = LocalChannel::new(cfg_with_models(crate::config::ChannelModels::default()));
        assert_eq!(
            ch.resolve_model(ModelKind::Main),
            Err(ModelResolveError::MainNotConfigured)
        );
        // Without main, every other kind also reports MainNotConfigured
        // (that's the primary gate — the channel is unusable at all).
        assert_eq!(
            ch.resolve_model(ModelKind::Vision),
            Err(ModelResolveError::MainNotConfigured)
        );
    }

    #[test]
    fn test_resolve_main_returns_configured_id() {
        let ch = LocalChannel::new(cfg_with_models(crate::config::ChannelModels {
            main: Some("zai/glm-5.1".into()),
            ..Default::default()
        }));
        assert_eq!(ch.resolve_model(ModelKind::Main).unwrap(), "zai/glm-5.1");
    }

    #[test]
    fn test_resolve_vision_without_config_does_not_fall_back_to_main() {
        let ch = LocalChannel::new(cfg_with_models(crate::config::ChannelModels {
            main: Some("zai/glm-5.1".into()),
            ..Default::default()
        }));
        assert_eq!(
            ch.resolve_model(ModelKind::Vision),
            Err(ModelResolveError::KindNotConfigured {
                kind: ModelKind::Vision
            })
        );
    }

    #[test]
    fn test_resolve_vision_when_configured() {
        let ch = LocalChannel::new(cfg_with_models(crate::config::ChannelModels {
            main: Some("zai/glm-5.1".into()),
            vision: Some("zai/glm-4.6v".into()),
            ..Default::default()
        }));
        assert_eq!(ch.resolve_model(ModelKind::Vision).unwrap(), "zai/glm-4.6v");
    }

    #[test]
    fn test_resolve_coding_and_audio_are_independent() {
        let ch = LocalChannel::new(cfg_with_models(crate::config::ChannelModels {
            main: Some("zai/glm-5.1".into()),
            coding: Some("zai/glm-4.7".into()),
            ..Default::default()
        }));
        assert_eq!(ch.resolve_model(ModelKind::Coding).unwrap(), "zai/glm-4.7");
        assert_eq!(
            ch.resolve_model(ModelKind::Audio),
            Err(ModelResolveError::KindNotConfigured {
                kind: ModelKind::Audio
            })
        );
    }

    #[test]
    fn test_model_kind_from_content() {
        assert_eq!(
            ModelKind::from_content(&MessageContent::Text {
                text: "hi".into()
            }),
            ModelKind::Main
        );
        assert_eq!(
            ModelKind::from_content(&MessageContent::Image {
                url: "file://x".into(),
                caption: None
            }),
            ModelKind::Vision
        );
        assert_eq!(
            ModelKind::from_content(&MessageContent::Audio {
                url: "file://x".into(),
                duration: None
            }),
            ModelKind::Audio
        );
        // File content is not image/audio → falls into Main
        assert_eq!(
            ModelKind::from_content(&MessageContent::File {
                url: "file://x".into(),
                name: "x".into()
            }),
            ModelKind::Main
        );
    }

    #[tokio::test]
    async fn test_start_emits_connected_and_enables_inject() {
        let mut ch = LocalChannel::new(LocalConfig::default());
        let (tx, mut rx) = mpsc::channel(8);

        ch.start(tx).await.unwrap();
        assert!(ch.is_connected());

        let first = rx.recv().await.unwrap();
        assert!(matches!(first, ChannelEvent::Connected { .. }));

        ch.inject_text("chat-1", "user-1", "hello").await.unwrap();
        let next = rx.recv().await.unwrap();
        match next {
            ChannelEvent::Message(msg) => {
                assert_eq!(msg.channel, LOCAL_CHANNEL_ID);
                assert_eq!(msg.content.as_text(), Some("hello"));
            }
            other => panic!("expected Message event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_inject_before_start_fails() {
        let ch = LocalChannel::new(LocalConfig::default());
        let err = ch.inject_text("chat-1", "user-1", "hi").await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn test_send_broadcasts_to_subscribers() {
        let mut ch = LocalChannel::new(LocalConfig::default());
        let mut sub = ch.subscribe();
        let (tx, _rx) = mpsc::channel(8);
        ch.start(tx).await.unwrap();

        let id = ch
            .send(OutgoingMessage::text("chat-1", "reply"))
            .await
            .unwrap();

        let received = sub.recv().await.unwrap();
        assert_eq!(received.message_id, id);
        assert_eq!(received.message.chat_id, "chat-1");
        match received.message.content {
            MessageContent::Text { text } => assert_eq!(text, "reply"),
            _ => panic!("expected text content"),
        }
    }

    #[tokio::test]
    async fn test_send_without_subscribers_is_ok() {
        let mut ch = LocalChannel::new(LocalConfig::default());
        let (tx, _rx) = mpsc::channel(8);
        ch.start(tx).await.unwrap();

        let id = ch
            .send(OutgoingMessage::text("chat-1", "nobody listening"))
            .await
            .unwrap();
        assert!(!id.is_empty());
    }

    #[tokio::test]
    async fn test_send_when_not_connected_fails() {
        let ch = LocalChannel::new(LocalConfig::default());
        let err = ch.send(OutgoingMessage::text("c", "t")).await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn test_stop_disables_inject() {
        let mut ch = LocalChannel::new(LocalConfig::default());
        let (tx, _rx) = mpsc::channel(8);
        ch.start(tx).await.unwrap();
        ch.stop().await.unwrap();

        assert!(!ch.is_connected());
        assert!(ch.inject_text("c", "u", "hi").await.is_err());
    }
}
