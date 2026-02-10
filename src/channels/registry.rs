//! Channel registry for managing messaging channels

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};

use super::{Channel, ChannelEvent, ChannelStatus, OutgoingMessage};

/// Channel registry for managing multiple messaging channels
pub struct ChannelRegistry {
    channels: RwLock<HashMap<String, Arc<RwLock<Box<dyn Channel>>>>>,
    event_tx: mpsc::Sender<ChannelEvent>,
    event_rx: RwLock<Option<mpsc::Receiver<ChannelEvent>>>,
}

impl ChannelRegistry {
    /// Create a new channel registry
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::channel(1000);
        Self {
            channels: RwLock::new(HashMap::new()),
            event_tx,
            event_rx: RwLock::new(Some(event_rx)),
        }
    }

    /// Get the event sender for channels to emit events
    pub fn event_sender(&self) -> mpsc::Sender<ChannelEvent> {
        self.event_tx.clone()
    }

    /// Take the event receiver (can only be called once)
    pub async fn take_event_receiver(&self) -> Option<mpsc::Receiver<ChannelEvent>> {
        self.event_rx.write().await.take()
    }

    /// Register a channel
    pub async fn register(&self, channel: Box<dyn Channel>) {
        let id = channel.id().to_string();
        let mut channels = self.channels.write().await;
        channels.insert(id, Arc::new(RwLock::new(channel)));
    }

    /// Unregister a channel
    pub async fn unregister(&self, id: &str) -> Option<Arc<RwLock<Box<dyn Channel>>>> {
        let mut channels = self.channels.write().await;
        channels.remove(id)
    }

    /// Get a channel by ID
    pub async fn get(&self, id: &str) -> Option<Arc<RwLock<Box<dyn Channel>>>> {
        let channels = self.channels.read().await;
        channels.get(id).cloned()
    }

    /// Check if a channel is registered
    pub async fn has_channel(&self, id: &str) -> bool {
        let channels = self.channels.read().await;
        channels.contains_key(id)
    }

    /// Get all channel IDs
    pub async fn channel_ids(&self) -> Vec<String> {
        let channels = self.channels.read().await;
        channels.keys().cloned().collect()
    }

    /// Get status of all channels
    pub async fn all_status(&self) -> Vec<ChannelStatus> {
        let channels = self.channels.read().await;
        let mut statuses = Vec::with_capacity(channels.len());

        for channel in channels.values() {
            let ch = channel.read().await;
            statuses.push(ch.status());
        }

        statuses
    }

    /// Start all registered channels
    pub async fn start_all(&self) -> anyhow::Result<()> {
        let channels = self.channels.read().await;

        for (id, channel) in channels.iter() {
            let mut ch = channel.write().await;
            if let Err(e) = ch.start(self.event_tx.clone()).await {
                tracing::error!("Failed to start channel {}: {}", id, e);
                let _ = self.event_tx.send(ChannelEvent::Error {
                    channel: id.clone(),
                    error: e.to_string(),
                }).await;
            }
        }

        Ok(())
    }

    /// Stop all registered channels
    pub async fn stop_all(&self) -> anyhow::Result<()> {
        let channels = self.channels.read().await;

        for (id, channel) in channels.iter() {
            let mut ch = channel.write().await;
            if let Err(e) = ch.stop().await {
                tracing::error!("Failed to stop channel {}: {}", id, e);
            }
        }

        Ok(())
    }

    /// Send a message to a specific channel
    pub async fn send(&self, channel_id: &str, message: OutgoingMessage) -> anyhow::Result<String> {
        let channels = self.channels.read().await;

        match channels.get(channel_id) {
            Some(channel) => {
                let ch = channel.read().await;
                ch.send(message).await
            }
            None => Err(anyhow::anyhow!("Channel not found: {}", channel_id)),
        }
    }

    /// Check if a channel supports message editing
    pub async fn supports_edit(&self, channel_id: &str) -> bool {
        let channels = self.channels.read().await;
        match channels.get(channel_id) {
            Some(channel) => {
                let ch = channel.read().await;
                ch.supports_edit()
            }
            None => false,
        }
    }

    /// Edit an existing message in a channel
    pub async fn edit_message(
        &self,
        channel_id: &str,
        chat_id: &str,
        message_id: &str,
        new_content: &str,
    ) -> anyhow::Result<()> {
        let channels = self.channels.read().await;

        match channels.get(channel_id) {
            Some(channel) => {
                let ch = channel.read().await;
                ch.edit_message(chat_id, message_id, new_content).await
            }
            None => Err(anyhow::anyhow!("Channel not found: {}", channel_id)),
        }
    }

    /// Get the number of registered channels
    pub async fn channel_count(&self) -> usize {
        let channels = self.channels.read().await;
        channels.len()
    }

    /// Get connected channel count
    pub async fn connected_count(&self) -> usize {
        let channels = self.channels.read().await;
        let mut count = 0;

        for channel in channels.values() {
            let ch = channel.read().await;
            if ch.is_connected() {
                count += 1;
            }
        }

        count
    }
}

impl Default for ChannelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    /// Mock channel for testing
    struct MockChannel {
        id: String,
        name: String,
        connected: bool,
    }

    impl MockChannel {
        fn new(id: &str, name: &str) -> Self {
            Self {
                id: id.to_string(),
                name: name.to_string(),
                connected: false,
            }
        }
    }

    #[async_trait]
    impl Channel for MockChannel {
        fn id(&self) -> &str {
            &self.id
        }

        fn name(&self) -> &str {
            &self.name
        }

        async fn start(&mut self, event_tx: mpsc::Sender<ChannelEvent>) -> anyhow::Result<()> {
            self.connected = true;
            let _ = event_tx.send(ChannelEvent::Connected {
                channel: self.id.clone(),
            }).await;
            Ok(())
        }

        async fn stop(&mut self) -> anyhow::Result<()> {
            self.connected = false;
            Ok(())
        }

        async fn send(&self, _message: OutgoingMessage) -> anyhow::Result<String> {
            Ok("mock-message-id".to_string())
        }

        fn is_connected(&self) -> bool {
            self.connected
        }
    }

    #[tokio::test]
    async fn test_registry_new() {
        let registry = ChannelRegistry::new();
        assert_eq!(registry.channel_count().await, 0);
    }

    #[tokio::test]
    async fn test_registry_register() {
        let registry = ChannelRegistry::new();
        let channel = MockChannel::new("test", "Test Channel");

        registry.register(Box::new(channel)).await;

        assert!(registry.has_channel("test").await);
        assert_eq!(registry.channel_count().await, 1);
    }

    #[tokio::test]
    async fn test_registry_unregister() {
        let registry = ChannelRegistry::new();
        let channel = MockChannel::new("test", "Test Channel");

        registry.register(Box::new(channel)).await;
        assert!(registry.has_channel("test").await);

        registry.unregister("test").await;
        assert!(!registry.has_channel("test").await);
    }

    #[tokio::test]
    async fn test_registry_get() {
        let registry = ChannelRegistry::new();
        let channel = MockChannel::new("test", "Test Channel");

        registry.register(Box::new(channel)).await;

        let retrieved = registry.get("test").await;
        assert!(retrieved.is_some());

        let ch = retrieved.unwrap();
        let ch_guard = ch.read().await;
        assert_eq!(ch_guard.id(), "test");
    }

    #[tokio::test]
    async fn test_registry_channel_ids() {
        let registry = ChannelRegistry::new();
        registry.register(Box::new(MockChannel::new("ch1", "Channel 1"))).await;
        registry.register(Box::new(MockChannel::new("ch2", "Channel 2"))).await;

        let ids = registry.channel_ids().await;
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"ch1".to_string()));
        assert!(ids.contains(&"ch2".to_string()));
    }

    #[tokio::test]
    async fn test_registry_all_status() {
        let registry = ChannelRegistry::new();
        registry.register(Box::new(MockChannel::new("ch1", "Channel 1"))).await;
        registry.register(Box::new(MockChannel::new("ch2", "Channel 2"))).await;

        let statuses = registry.all_status().await;
        assert_eq!(statuses.len(), 2);

        for status in &statuses {
            assert!(!status.connected);
        }
    }

    #[tokio::test]
    async fn test_registry_start_all() {
        let registry = ChannelRegistry::new();
        registry.register(Box::new(MockChannel::new("ch1", "Channel 1"))).await;
        registry.register(Box::new(MockChannel::new("ch2", "Channel 2"))).await;

        registry.start_all().await.unwrap();

        assert_eq!(registry.connected_count().await, 2);
    }

    #[tokio::test]
    async fn test_registry_stop_all() {
        let registry = ChannelRegistry::new();
        registry.register(Box::new(MockChannel::new("ch1", "Channel 1"))).await;

        registry.start_all().await.unwrap();
        assert_eq!(registry.connected_count().await, 1);

        registry.stop_all().await.unwrap();
        assert_eq!(registry.connected_count().await, 0);
    }

    #[tokio::test]
    async fn test_registry_send() {
        let registry = ChannelRegistry::new();
        registry.register(Box::new(MockChannel::new("test", "Test"))).await;
        registry.start_all().await.unwrap();

        let message = OutgoingMessage::text("chat-123", "Hello!");
        let result = registry.send("test", message).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "mock-message-id");
    }

    #[tokio::test]
    async fn test_registry_send_unknown_channel() {
        let registry = ChannelRegistry::new();

        let message = OutgoingMessage::text("chat-123", "Hello!");
        let result = registry.send("unknown", message).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_registry_event_receiver() {
        let registry = ChannelRegistry::new();
        registry.register(Box::new(MockChannel::new("test", "Test"))).await;

        let mut rx = registry.take_event_receiver().await.unwrap();

        // Start should emit Connected event
        registry.start_all().await.unwrap();

        // Receive the event
        let event = rx.recv().await;
        assert!(matches!(event, Some(ChannelEvent::Connected { .. })));
    }
}
