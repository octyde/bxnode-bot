//! Gateway module - HTTP and WebSocket server

pub mod http;
pub mod protocol;
pub mod ws;

pub mod methods;

#[cfg(test)]
mod protocol_tests;

use std::sync::Arc;

use anyhow::Result;
use axum::{
    routing::{get, post},
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::channels::{ChannelEvent, ChannelRegistry};
use crate::cli::ServeArgs;
use crate::providers::ProviderRegistry;

/// Embedded UI assets
#[derive(rust_embed::RustEmbed)]
#[folder = "ui/dist/"]
struct UiAssets;

// Re-export for external use
pub use http::*;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub providers: Arc<ProviderRegistry>,
    pub channels: Arc<ChannelRegistry>,
}

/// Start the gateway server
pub async fn serve(args: ServeArgs) -> Result<()> {
    let config = crate::Config::load_or_default(args.config.as_ref());

    // Create provider registry from config
    let provider_registry = ProviderRegistry::from_config(&config);
    let provider_count = provider_registry.provider_count();
    let provider_ids = provider_registry.provider_ids();

    tracing::info!(
        "Loaded {} provider(s): {:?}",
        provider_count,
        provider_ids
    );

    // Create channel registry and register channels from config
    let channel_registry = ChannelRegistry::new();
    register_channels_from_config(&channel_registry, &config).await;

    let channel_count = channel_registry.channel_count().await;
    let channel_ids = channel_registry.channel_ids().await;

    tracing::info!(
        "Registered {} channel(s): {:?}",
        channel_count,
        channel_ids
    );

    let channel_registry = Arc::new(channel_registry);

    // Start channel event handler in background
    let event_channel_registry = channel_registry.clone();
    if let Some(mut event_rx) = channel_registry.take_event_receiver().await {
        tokio::spawn(async move {
            handle_channel_events(&mut event_rx, &event_channel_registry).await;
        });
    }

    // Start all channels
    if let Err(e) = channel_registry.start_all().await {
        tracing::error!("Failed to start channels: {}", e);
    }

    let state = AppState {
        providers: Arc::new(provider_registry),
        channels: channel_registry.clone(),
    };

    // Build the router
    let app = Router::new()
        // Health check
        .route("/health", get(http::health))
        // OpenAI-compatible API
        .route("/v1/chat/completions", post(http::chat_completions))
        .route("/v1/models", get(http::list_models))
        // WebSocket endpoint
        .route("/ws", get(ws::handler))
        // Static UI files
        .fallback(http::serve_ui)
        // Shared state
        .with_state(state)
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    // Bind and serve
    let addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("Gateway listening on http://{}", addr);

    axum::serve(listener, app).await?;

    // Stop channels on shutdown
    channel_registry.stop_all().await?;

    Ok(())
}

/// Register channels from configuration
async fn register_channels_from_config(registry: &ChannelRegistry, config: &crate::Config) {
    // Register Telegram channel if configured
    #[cfg(feature = "channel-telegram")]
    if let Some(ref telegram_config) = config.channels.telegram {
        use crate::channels::telegram::{TelegramChannel, TelegramConfig};

        let channel_config = TelegramConfig {
            bot_token: telegram_config.token.clone(),
            webhook_url: None,
            allowed_users: telegram_config.allowed_users.clone(),
            allowed_chats: vec![],
        };

        let channel = TelegramChannel::new(channel_config);
        registry.register(Box::new(channel)).await;
        tracing::info!("Registered Telegram channel");
    }

    // Register Discord channel if configured
    #[cfg(feature = "channel-discord")]
    if let Some(ref discord_config) = config.channels.discord {
        use crate::channels::discord::{DiscordChannel, DiscordConfig};

        let channel_config = DiscordConfig {
            bot_token: discord_config.token.clone(),
            application_id: None,
            allowed_guilds: discord_config.allowed_guilds.clone(),
            allowed_channels: vec![],
            prefix: None,
            allow_dms: true,
        };

        let channel = DiscordChannel::new(channel_config);
        registry.register(Box::new(channel)).await;
        tracing::info!("Registered Discord channel");
    }

    // Register Slack channel if configured
    #[cfg(feature = "channel-slack")]
    if let Some(ref slack_config) = config.channels.slack {
        use crate::channels::slack::{SlackChannel, SlackConfig};

        let channel_config = SlackConfig {
            bot_token: slack_config.bot_token.clone(),
            app_token: slack_config.app_token.clone(),
            allowed_channels: vec![],
            allow_dms: true,
            mentions_only: false,
        };

        let channel = SlackChannel::new(channel_config);
        registry.register(Box::new(channel)).await;
        tracing::info!("Registered Slack channel");
    }
}

/// Handle channel events in background
async fn handle_channel_events(
    event_rx: &mut tokio::sync::mpsc::Receiver<ChannelEvent>,
    _registry: &Arc<ChannelRegistry>,
) {
    while let Some(event) = event_rx.recv().await {
        match event {
            ChannelEvent::Connected { channel } => {
                tracing::info!("Channel '{}' connected", channel);
            }
            ChannelEvent::Disconnected { channel, reason } => {
                tracing::warn!(
                    "Channel '{}' disconnected: {}",
                    channel,
                    reason.as_deref().unwrap_or("unknown reason")
                );
            }
            ChannelEvent::Error { channel, error } => {
                tracing::error!("Channel '{}' error: {}", channel, error);
            }
            ChannelEvent::Message(msg) => {
                tracing::debug!(
                    "Received message from channel '{}': {:?}",
                    msg.channel,
                    msg.content.as_text().unwrap_or("<non-text>")
                );
                // TODO: Route message to agent for processing
            }
        }
    }
}
