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

use tokio::sync::RwLock;

use crate::agent::{AgentConfig, AgentContext, AgentExecutor, ToolRegistry};
use crate::channels::{ChannelEvent, ChannelRegistry, IncomingMessage, MessageContent, OutgoingMessage};
use crate::cli::ServeArgs;
use crate::cron::{CronEvent, CronJob, CronScheduler, CronStore};
use crate::memory::{MemoryScope, MemoryStore};
use crate::providers::ProviderRegistry;
use crate::skills::SkillRegistry;

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
    pub cron: Arc<CronScheduler>,
    pub memory: Arc<RwLock<MemoryStore>>,
    pub skills: Option<Arc<RwLock<SkillRegistry>>>,
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

    // Wrap provider registry in Arc for sharing
    let provider_registry = Arc::new(provider_registry);

    // Initialize memory store
    let memory_store = if config.memory.enabled {
        let store_path = expand_path(&config.memory.store_path);
        // Ensure the parent directory exists
        if let Some(parent) = std::path::Path::new(&store_path).parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        match MemoryStore::open(&store_path) {
            Ok(store) => {
                tracing::info!("Loaded memory store with {} record(s)", store.len());
                Arc::new(RwLock::new(store))
            }
            Err(e) => {
                tracing::error!("Failed to load memory store: {}, using in-memory", e);
                Arc::new(RwLock::new(MemoryStore::in_memory()))
            }
        }
    } else {
        tracing::info!("Memory system disabled");
        Arc::new(RwLock::new(MemoryStore::in_memory()))
    };

    // Start channel event handler in background
    let event_channel_registry = channel_registry.clone();
    let event_provider_registry = provider_registry.clone();
    let event_memory_store = memory_store.clone();
    let default_model = config
        .agents
        .defaults
        .model
        .clone()
        .unwrap_or_else(|| "anthropic/claude-3-opus".to_string());

    if let Some(mut event_rx) = channel_registry.take_event_receiver().await {
        tokio::spawn(async move {
            handle_channel_events(
                &mut event_rx,
                &event_channel_registry,
                &event_provider_registry,
                &event_memory_store,
                &default_model,
            )
            .await;
        });
    }

    // Start all channels
    if let Err(e) = channel_registry.start_all().await {
        tracing::error!("Failed to start channels: {}", e);
    }

    // Initialize cron scheduler
    let cron_scheduler = CronScheduler::new();
    if config.cron.enabled {
        // Load jobs from store and config
        if let Err(e) = load_cron_jobs(&cron_scheduler, &config).await {
            tracing::error!("Failed to load cron jobs: {}", e);
        }

        let job_count = cron_scheduler.list_jobs().await.len();
        tracing::info!("Loaded {} cron job(s)", job_count);
    } else {
        tracing::info!("Cron scheduler disabled");
    }

    let cron_scheduler = Arc::new(cron_scheduler);

    // Start cron event handler in background
    if config.cron.enabled {
        let event_cron = cron_scheduler.clone();
        if let Some(mut cron_rx) = cron_scheduler.take_event_receiver().await {
            tokio::spawn(async move {
                handle_cron_events(&mut cron_rx, &event_cron).await;
            });
        }

        // Start the cron scheduler tick loop
        let tick_cron = cron_scheduler.clone();
        tokio::spawn(async move {
            if let Err(e) = tick_cron.start().await {
                tracing::error!("Cron scheduler error: {}", e);
            }
        });
    }

    // Initialize skills registry
    let skills_registry = if config.skills.enabled {
        match SkillRegistry::from_config(&config.skills) {
            Ok(registry) => {
                let skill_count = registry.count();
                let active_count = registry.active_count();
                tracing::info!(
                    "Loaded {} skill(s), {} active",
                    skill_count,
                    active_count
                );
                Some(Arc::new(RwLock::new(registry)))
            }
            Err(e) => {
                tracing::error!("Failed to initialize skills registry: {}", e);
                None
            }
        }
    } else {
        tracing::info!("Skills system disabled");
        None
    };

    let state = AppState {
        providers: provider_registry.clone(),
        channels: channel_registry.clone(),
        cron: cron_scheduler.clone(),
        memory: memory_store.clone(),
        skills: skills_registry,
    };

    // Build the router
    let app = Router::new()
        // Health check endpoints
        .route("/health", get(http::health))
        .route("/health/detailed", get(http::health_detailed))
        .route("/stats", get(http::stats))
        // OpenAI-compatible API
        .route("/v1/chat/completions", post(http::chat_completions))
        .route("/v1/models", get(http::list_models))
        // Skills API endpoints
        .route("/api/skills", get(http::list_skills))
        .route("/api/skills/config", get(http::skills_config))
        .route("/api/skills/sync", post(http::sync_skills))
        .route("/api/skills/:name", get(http::get_skill))
        .route("/api/skills/:name/enable", post(http::enable_skill))
        .route("/api/skills/:name/disable", post(http::disable_skill))
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

    // Stop cron scheduler
    cron_scheduler.stop().await;

    Ok(())
}

/// Register channels from configuration
#[allow(unused_variables)]
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
    registry: &Arc<ChannelRegistry>,
    providers: &Arc<ProviderRegistry>,
    memory: &Arc<RwLock<MemoryStore>>,
    default_model: &str,
) {
    // Simple conversation context cache (keyed by chat_id)
    use std::collections::HashMap;

    let contexts: Arc<RwLock<HashMap<String, AgentContext>>> =
        Arc::new(RwLock::new(HashMap::new()));

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
                tracing::info!(
                    "Received message from channel '{}' chat '{}': {:?}",
                    msg.channel,
                    msg.chat_id,
                    msg.content.as_text().unwrap_or("<non-text>")
                );

                // Process message in a separate task
                let registry = registry.clone();
                let providers = providers.clone();
                let memory = memory.clone();
                let contexts = contexts.clone();
                let model = default_model.to_string();

                tokio::spawn(async move {
                    if let Err(e) =
                        process_channel_message(msg, &registry, &providers, &memory, &contexts, &model).await
                    {
                        tracing::error!("Failed to process message: {}", e);
                    }
                });
            }
        }
    }
}

/// Process a channel message through the agent
async fn process_channel_message(
    msg: IncomingMessage,
    registry: &Arc<ChannelRegistry>,
    providers: &Arc<ProviderRegistry>,
    memory: &Arc<RwLock<MemoryStore>>,
    contexts: &Arc<RwLock<std::collections::HashMap<String, AgentContext>>>,
    model: &str,
) -> anyhow::Result<()> {
    // Only process text messages for now
    let text = match msg.content.as_text() {
        Some(t) => t.to_string(),
        None => {
            tracing::debug!("Skipping non-text message");
            return Ok(());
        }
    };

    // Get or create context for this chat
    let context_key = format!("{}:{}", msg.channel, msg.chat_id);
    let mut context = {
        let mut contexts_write = contexts.write().await;
        contexts_write
            .entry(context_key.clone())
            .or_insert_with(AgentContext::default)
            .clone()
    };

    // Get the provider
    let provider = match providers.get_for_model(model) {
        Some(p) => p,
        None => {
            tracing::error!("No provider found for model: {}", model);
            // Send error message back
            send_reply(
                &registry,
                &msg,
                "Sorry, I couldn't find a provider for this model. Please check configuration.",
            )
            .await?;
            return Ok(());
        }
    };

    // Create memory scope for this conversation
    let memory_scope = MemoryScope {
        agent_id: "default".to_string(),
        channel_id: Some(msg.channel.clone()),
        user_id: Some(msg.user_id.clone()),
        session_id: None,
    };

    // Create agent executor with memory tools
    let tools = Arc::new(ToolRegistry::with_memory(memory.clone(), memory_scope));
    let config = AgentConfig {
        model: model.to_string(),
        ..Default::default()
    };
    let executor = AgentExecutor::new(provider, tools, config);

    // Check if channel supports streaming updates via message editing
    let supports_streaming = registry.supports_edit(&msg.channel).await;

    if supports_streaming {
        // Use streaming with message editing for real-time updates
        process_with_streaming(
            &executor,
            &mut context,
            &text,
            registry,
            &msg,
            &context_key,
            contexts,
        )
        .await
    } else {
        // Fall back to non-streaming execution
        match executor.execute(&mut context, &text).await {
            Ok(result) => {
                // Save updated context
                {
                    let mut contexts_write = contexts.write().await;
                    contexts_write.insert(context_key, context);
                }

                // Send response back to channel
                send_reply(&registry, &msg, &result.content).await?;

                tracing::info!(
                    "Responded to chat '{}' with {} tokens",
                    msg.chat_id,
                    result.usage.total_tokens
                );
                Ok(())
            }
            Err(e) => {
                tracing::error!("Agent execution failed: {}", e);
                send_reply(
                    &registry,
                    &msg,
                    &format!("Sorry, I encountered an error: {}", e),
                )
                .await?;
                Ok(())
            }
        }
    }
}

/// Process message with streaming updates (for channels that support message editing)
async fn process_with_streaming(
    executor: &AgentExecutor,
    context: &mut AgentContext,
    text: &str,
    registry: &Arc<ChannelRegistry>,
    msg: &IncomingMessage,
    context_key: &str,
    contexts: &Arc<RwLock<std::collections::HashMap<String, AgentContext>>>,
) -> anyhow::Result<()> {
    use crate::agent::AgentEvent;

    // Send initial "thinking" message
    let initial_message = OutgoingMessage {
        chat_id: msg.chat_id.clone(),
        content: MessageContent::Text {
            text: "▌".to_string(), // Typing indicator
        },
        reply_to: Some(msg.id.clone()),
        parse_mode: None,
    };

    let message_id = registry.send(&msg.channel, initial_message).await?;

    // Execute with streaming
    // Note: For now we just accumulate chunks and update at the end.
    // Future enhancement: throttled updates during streaming.
    let mut _accumulated_content = String::new();

    let result = executor
        .execute_stream(context, text, |event| {
            match event {
                AgentEvent::TextDelta { content } => {
                    _accumulated_content.push_str(&content);
                }
                AgentEvent::ToolCall { call } => {
                    tracing::debug!("Tool call during stream: {}", call.name);
                }
                AgentEvent::ToolResult { result } => {
                    tracing::debug!("Tool result: {}", if result.success { "success" } else { "error" });
                }
                AgentEvent::Error { message } => {
                    tracing::error!("Streaming error: {}", message);
                }
                _ => {}
            }
        })
        .await;

    // Final update with complete content
    match result {
        Ok(exec_result) => {
            // Save updated context
            {
                let mut contexts_write = contexts.write().await;
                contexts_write.insert(context_key.to_string(), context.clone());
            }

            // Final edit with complete response
            if let Err(e) = registry
                .edit_message(&msg.channel, &msg.chat_id, &message_id, &exec_result.content)
                .await
            {
                tracing::warn!("Failed to edit final message, sending new: {}", e);
                send_reply(registry, msg, &exec_result.content).await?;
            }

            tracing::info!(
                "Streamed response to chat '{}' with {} tokens",
                msg.chat_id,
                exec_result.usage.total_tokens
            );
            Ok(())
        }
        Err(e) => {
            tracing::error!("Streaming agent execution failed: {}", e);
            let error_msg = format!("Sorry, I encountered an error: {}", e);
            if let Err(_) = registry
                .edit_message(&msg.channel, &msg.chat_id, &message_id, &error_msg)
                .await
            {
                send_reply(registry, msg, &error_msg).await?;
            }
            Ok(())
        }
    }
}

/// Send a reply to a channel message
async fn send_reply(
    registry: &Arc<ChannelRegistry>,
    original: &IncomingMessage,
    content: &str,
) -> anyhow::Result<()> {
    let reply = OutgoingMessage {
        chat_id: original.chat_id.clone(),
        content: MessageContent::Text {
            text: content.to_string(),
        },
        reply_to: Some(original.id.clone()),
        parse_mode: None,
    };

    registry.send(&original.channel, reply).await?;
    Ok(())
}

/// Load cron jobs from config and persistent store
async fn load_cron_jobs(scheduler: &CronScheduler, config: &crate::Config) -> anyhow::Result<()> {
    // Load jobs from store
    let store_path = expand_path(&config.cron.store_path);
    if std::path::Path::new(&store_path).exists() {
        let store = CronStore::open(&store_path)?;
        for entry in store.list()? {
            if let Err(e) = scheduler.add_job(entry.job).await {
                tracing::warn!("Failed to load cron job from store: {}", e);
            }
        }
    }

    // Load jobs from config (additive, won't override store jobs)
    for job_config in &config.cron.jobs {
        // Skip if job already loaded from store
        if scheduler.get_job(&job_config.id).await.is_some() {
            continue;
        }

        let job = CronJob {
            id: job_config.id.clone(),
            schedule: job_config.schedule.clone(),
            payload: job_config.payload.clone(),
            enabled: job_config.enabled,
            description: job_config.description.clone(),
            last_run: None,
            last_result: None,
        };

        if let Err(e) = scheduler.add_job(job).await {
            tracing::warn!("Failed to load cron job '{}' from config: {}", job_config.id, e);
        }
    }

    Ok(())
}

/// Handle cron events in background
async fn handle_cron_events(
    event_rx: &mut tokio::sync::mpsc::Receiver<CronEvent>,
    _scheduler: &Arc<CronScheduler>,
) {
    while let Some(event) = event_rx.recv().await {
        tracing::info!(
            "Cron job '{}' triggered at {}",
            event.job_id,
            event.scheduled_at
        );
        tracing::debug!("Cron payload: {:?}", event.payload);

        // TODO: Execute job based on payload
        // This is where we'd dispatch to an agent or run an action
    }
}

/// Expand ~ to home directory
fn expand_path(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}{}", home.display(), &path[1..]);
        }
    }
    path.to_string()
}
