//! Gateway module - HTTP and WebSocket server

pub mod approvals;
pub mod http;
pub mod protocol;
pub mod ws;

pub mod methods;

#[cfg(test)]
mod protocol_tests;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use anyhow::Result;
use axum::{
    routing::{get, post},
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use tokio::sync::{broadcast, RwLock};

use crate::agent::{AgentConfig, AgentContext, AgentEvent, AgentExecutor, ToolRegistry};
use crate::channels::{ChannelEvent, ChannelRegistry, IncomingMessage, MessageContent, OutgoingMessage};
use crate::cli::ServeArgs;
use crate::cron::{CronEvent, CronJob, CronScheduler, CronStore};
use crate::memory::{MemoryScope, MemoryStore};
use crate::project::ProjectStore;
use crate::providers::ProviderRegistry;
use crate::session::{ActiveSessionInfo, ActiveSessionMap, Session, SessionManager, TranscriptEntry, TranscriptEntryType};
use crate::skills::SkillRegistry;

use self::approvals::ApprovalManager;

/// Cumulative usage statistics tracked across the server lifetime
pub struct UsageStats {
    /// Total input (prompt) tokens across all requests
    pub tokens_in: AtomicU64,
    /// Total output (completion) tokens across all requests
    pub tokens_out: AtomicU64,
    /// Number of context compaction (truncation) events
    pub compactions: AtomicU64,
}

impl UsageStats {
    pub fn new() -> Self {
        Self {
            tokens_in: AtomicU64::new(0),
            tokens_out: AtomicU64::new(0),
            compactions: AtomicU64::new(0),
        }
    }

    pub fn record_usage(&self, prompt_tokens: u32, completion_tokens: u32) {
        self.tokens_in.fetch_add(prompt_tokens as u64, Ordering::Relaxed);
        self.tokens_out.fetch_add(completion_tokens as u64, Ordering::Relaxed);
    }

    pub fn record_compaction(&self) {
        self.compactions.fetch_add(1, Ordering::Relaxed);
    }
}

/// Per-chat fixed-window rate limiter
pub struct RateLimiter {
    /// (window_start, request_count) per chat key
    windows: RwLock<HashMap<String, (Instant, u32)>>,
    /// Maximum requests allowed per window
    max_per_minute: u32,
}

impl RateLimiter {
    pub fn new(max_per_minute: u32) -> Self {
        Self {
            windows: RwLock::new(HashMap::new()),
            max_per_minute,
        }
    }

    /// Check if the given chat key is rate limited. Returns true if allowed.
    pub async fn check_and_increment(&self, key: &str) -> bool {
        if self.max_per_minute == 0 {
            return true; // 0 = no limit
        }

        let now = Instant::now();
        let mut windows = self.windows.write().await;
        let entry = windows.entry(key.to_string()).or_insert((now, 0));

        // Reset window if more than 60s elapsed
        if now.duration_since(entry.0).as_secs() >= 60 {
            *entry = (now, 1);
            return true;
        }

        if entry.1 >= self.max_per_minute {
            return false; // Rate limited
        }

        entry.1 += 1;
        true
    }
}

/// Embedded UI assets
#[derive(rust_embed::RustEmbed)]
#[folder = "ui/dist/"]
struct UiAssets;

// Re-export for external use
pub use http::*;

/// Events broadcast to all connected WebSocket clients
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "event", content = "data")]
pub enum BroadcastEvent {
    #[serde(rename = "message.incoming")]
    MessageIncoming {
        id: String,
        channel: String,
        chat_id: String,
        user_id: String,
        user_name: Option<String>,
        content: String,
        timestamp: String,
        session_id: Option<String>,
    },
    #[serde(rename = "message.outgoing")]
    MessageOutgoing {
        channel: String,
        chat_id: String,
        content: String,
        reply_to: Option<String>,
        timestamp: String,
        session_id: Option<String>,
    },
    #[serde(rename = "message.error")]
    MessageError {
        channel: String,
        chat_id: String,
        error: String,
        timestamp: String,
        session_id: Option<String>,
    },
    #[serde(rename = "approval.request")]
    ApprovalRequest {
        id: String,
        user_id: String,
        user_name: Option<String>,
        channel: String,
        chat_id: String,
        first_message: String,
        timestamp: String,
    },
    #[serde(rename = "approval.resolved")]
    ApprovalResolved {
        id: String,
        user_id: String,
        action: String,
    },
}

impl BroadcastEvent {
    /// Get the event name for the RPC protocol
    pub fn event_name(&self) -> &str {
        match self {
            BroadcastEvent::MessageIncoming { .. } => "message.incoming",
            BroadcastEvent::MessageOutgoing { .. } => "message.outgoing",
            BroadcastEvent::MessageError { .. } => "message.error",
            BroadcastEvent::ApprovalRequest { .. } => "approval.request",
            BroadcastEvent::ApprovalResolved { .. } => "approval.resolved",
        }
    }
}

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub providers: Arc<ProviderRegistry>,
    pub channels: Arc<ChannelRegistry>,
    pub cron: Arc<CronScheduler>,
    pub memory: Arc<RwLock<MemoryStore>>,
    pub skills: Option<Arc<RwLock<SkillRegistry>>>,
    pub event_bus: broadcast::Sender<BroadcastEvent>,
    pub approvals: Arc<ApprovalManager>,
    pub sessions: Arc<SessionManager>,
    pub active_sessions: Arc<RwLock<ActiveSessionMap>>,
    pub project_store: Arc<RwLock<ProjectStore>>,
    pub config: Arc<crate::Config>,
    pub usage_stats: Arc<UsageStats>,
    /// Signal to trigger graceful shutdown (for restart)
    pub shutdown_tx: Arc<tokio::sync::watch::Sender<bool>>,
    /// Conversation contexts for the chat API (keyed by context_key)
    pub api_contexts: Arc<RwLock<HashMap<String, AgentContext>>>,
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

    // Initialize skills registry (before event handler so it can be shared)
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

    // Create broadcast channel for WebSocket event push
    let (event_bus, _) = broadcast::channel::<BroadcastEvent>(256);

    // Initialize approval manager
    let approval_store_path = expand_path("~/.bxnode-bot/approvals.json");
    let approval_manager = Arc::new(ApprovalManager::new(&approval_store_path));

    // Build a map of channel_id -> approval_required from config
    let mut approval_required_map: HashMap<String, bool> = HashMap::new();
    if let Some(ref tc) = config.channels.telegram {
        approval_required_map.insert("telegram".to_string(), tc.approval_required);
    }

    // Initialize session manager
    let sessions_base = expand_path("~/.bxnode-bot/sessions");
    let session_manager = Arc::new(SessionManager::new(std::path::PathBuf::from(&sessions_base)));
    if let Err(e) = session_manager.ensure_base_dir().await {
        tracing::error!("Failed to initialize session storage: {}", e);
    }

    let active_sessions_path = expand_path("~/.bxnode-bot/active_sessions.json");
    let active_sessions = Arc::new(RwLock::new(ActiveSessionMap::load(
        std::path::PathBuf::from(&active_sessions_path),
    )));

    tracing::info!("Session storage initialized at {}", sessions_base);

    // Initialize project store
    let workspace_base = expand_path(&config.workspace.base_dir);
    let projects_meta_dir = expand_path("~/.bxnode-bot/projects");
    let project_store = Arc::new(RwLock::new(ProjectStore::new(
        std::path::PathBuf::from(&projects_meta_dir),
        std::path::PathBuf::from(&workspace_base),
    )));
    {
        let store = project_store.read().await;
        if let Err(e) = store.ensure_base_dirs().await {
            tracing::error!("Failed to initialize project store: {}", e);
        }
    }

    // Auto-migrate existing session projects to ProjectStore
    {
        let session_projects = session_manager.list_projects().await.unwrap_or_default();
        let store = project_store.write().await;
        for name in &session_projects {
            if !store.exists(name).await {
                let project = crate::project::Project {
                    name: name.clone(),
                    workspace_dir: store.resolve_workspace_dir(name),
                    description: None,
                    model: None,
                    system_prompt: None,
                    coding_tools_enabled: config.workspace.coding_tools_enabled,
                    shell_enabled: config.workspace.shell_enabled,
                    metadata: serde_json::json!({}),
                    created_at: chrono::Utc::now(),
                };
                if let Err(e) = store.create(&project).await {
                    tracing::debug!("Project migration for '{}': {}", name, e);
                }
            }
        }
    }
    tracing::info!("Project store initialized (workspace base: {})", workspace_base);

    // Start channel event handler in background
    let event_channel_registry = channel_registry.clone();
    let event_provider_registry = provider_registry.clone();
    let event_memory_store = memory_store.clone();
    let event_bus_clone = event_bus.clone();
    let event_approvals = approval_manager.clone();
    let default_model = config
        .agents
        .defaults
        .model
        .clone()
        .unwrap_or_else(|| "anthropic/claude-3-opus".to_string());

    let event_skills = skills_registry.clone();
    let event_sessions = session_manager.clone();
    let event_active_sessions = active_sessions.clone();
    let event_project_store = project_store.clone();
    let usage_stats = Arc::new(UsageStats::new());
    let event_usage_stats = usage_stats.clone();
    let event_config = Arc::new(config.clone());

    if let Some(mut event_rx) = channel_registry.take_event_receiver().await {
        tokio::spawn(async move {
            handle_channel_events(
                &mut event_rx,
                &event_channel_registry,
                &event_provider_registry,
                &event_memory_store,
                &default_model,
                &event_bus_clone,
                &event_approvals,
                &approval_required_map,
                &event_skills,
                &event_sessions,
                &event_active_sessions,
                &event_project_store,
                &event_usage_stats,
                &event_config,
            )
            .await;
        });
    }

    // Start all channels
    if let Err(e) = channel_registry.start_all().await {
        tracing::error!("Failed to start channels: {}", e);
    }

    // Register Telegram bot commands (the /menu that appears when typing /)
    #[cfg(feature = "channel-telegram")]
    if let Some(ref telegram_config) = config.channels.telegram {
        if !telegram_config.token.is_empty() {
            tokio::spawn({
                let token = telegram_config.token.clone();
                async move {
                    if let Err(e) = register_telegram_commands(&token).await {
                        tracing::warn!("Failed to set Telegram bot commands: {}", e);
                    }
                }
            });
        }
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

    // Shutdown signal for graceful restart
    let (shutdown_tx, _shutdown_rx) = tokio::sync::watch::channel(false);
    let shutdown_tx = Arc::new(shutdown_tx);

    let state = AppState {
        providers: provider_registry.clone(),
        channels: channel_registry.clone(),
        cron: cron_scheduler.clone(),
        memory: memory_store.clone(),
        skills: skills_registry,
        event_bus: event_bus.clone(),
        approvals: approval_manager.clone(),
        sessions: session_manager.clone(),
        active_sessions: active_sessions.clone(),
        project_store: project_store.clone(),
        config: Arc::new(config.clone()),
        usage_stats: usage_stats.clone(),
        shutdown_tx: shutdown_tx.clone(),
        api_contexts: Arc::new(RwLock::new(HashMap::new())),
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
        // Chat API (full pipeline: slash commands + agent)
        .route("/api/chat", post(http::chat))
        // Skills API endpoints
        .route("/api/sessions/stats", get(http::session_stats))
        .route("/api/skills", get(http::list_skills))
        .route("/api/skills/config", get(http::skills_config))
        .route("/api/skills/sync", post(http::sync_skills))
        .route("/api/skills/{name}", get(http::get_skill))
        .route("/api/skills/{name}/enable", post(http::enable_skill))
        .route("/api/skills/{name}/disable", post(http::disable_skill))
        // WebSocket endpoint
        .route("/ws", get(ws::handler))
        // Static UI files
        .fallback(http::serve_ui)
        // Shared state
        .with_state(state)
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    // Bind and serve — prefer config values over CLI defaults
    let host = if args.host == "0.0.0.0" && !config.server.host.is_empty() {
        &config.server.host
    } else {
        &args.host
    };
    let port = if args.port == 3000 { config.server.port } else { args.port };
    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("Gateway listening on http://{}", addr);

    // Graceful shutdown: listen for the shutdown signal from RPC
    let mut shutdown_rx = shutdown_tx.subscribe();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            // Wait for shutdown signal
            let _ = shutdown_rx.changed().await;
            tracing::info!("Shutdown signal received, stopping server...");
        })
        .await?;

    // Stop channels on shutdown
    channel_registry.stop_all().await?;

    // Stop cron scheduler
    cron_scheduler.stop().await;

    // Check if this was a restart request — spawn a new process
    if *shutdown_tx.borrow() {
        tracing::info!("Restarting server...");
        // On Linux, std::env::current_exe() reads /proc/self/exe which may
        // point to "<path> (deleted)" if the binary was replaced by a rebuild.
        // Strip the suffix so we launch the updated binary from disk.
        let raw_exe = std::env::current_exe()?;
        let exe_str = raw_exe.to_string_lossy();
        let exe = if exe_str.ends_with(" (deleted)") {
            std::path::PathBuf::from(exe_str.trim_end_matches(" (deleted)"))
        } else {
            raw_exe
        };
        let args: Vec<String> = std::env::args().skip(1).collect();
        match std::process::Command::new(&exe).args(&args).spawn() {
            Ok(_child) => {
                tracing::info!("New server process spawned: {} {:?}", exe.display(), args);
            }
            Err(e) => {
                tracing::error!("Failed to spawn restart process: {}", e);
            }
        }
    }

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
    event_bus: &broadcast::Sender<BroadcastEvent>,
    approvals: &Arc<ApprovalManager>,
    approval_required_map: &HashMap<String, bool>,
    skills: &Option<Arc<RwLock<SkillRegistry>>>,
    sessions: &Arc<SessionManager>,
    active_sessions: &Arc<RwLock<ActiveSessionMap>>,
    project_store: &Arc<RwLock<ProjectStore>>,
    usage_stats: &Arc<UsageStats>,
    config: &Arc<crate::Config>,
) {
    let contexts: Arc<RwLock<HashMap<String, AgentContext>>> =
        Arc::new(RwLock::new(HashMap::new()));

    // Rate limiter for per-chat message throttling
    let rate_limiter = Arc::new(RateLimiter::new(config.rate_limit.max_turns_per_minute));

    // Session idle cleanup: track last activity per context key
    let last_activity: Arc<RwLock<HashMap<String, Instant>>> =
        Arc::new(RwLock::new(HashMap::new()));

    // Spawn background task to evict idle sessions
    let idle_ttl_minutes = config.session.idle_ttl_minutes;
    if idle_ttl_minutes > 0 {
        let evict_contexts = contexts.clone();
        let evict_activity = last_activity.clone();
        tokio::spawn(async move {
            let ttl = std::time::Duration::from_secs(idle_ttl_minutes as u64 * 60);
            let check_interval = std::time::Duration::from_secs(300); // 5 min
            loop {
                tokio::time::sleep(check_interval).await;
                let now = Instant::now();
                let mut activity = evict_activity.write().await;
                let expired_keys: Vec<String> = activity
                    .iter()
                    .filter(|(_, last)| now.duration_since(**last) > ttl)
                    .map(|(k, _)| k.clone())
                    .collect();

                if !expired_keys.is_empty() {
                    let mut ctxs = evict_contexts.write().await;
                    for key in &expired_keys {
                        ctxs.remove(key);
                        activity.remove(key);
                    }
                    tracing::info!("Evicted {} idle session context(s)", expired_keys.len());
                }
            }
        });
    }

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
                let text_preview = msg.content.as_text().unwrap_or("<non-text>");
                tracing::info!(
                    "Received message from channel '{}' chat '{}': {:?}",
                    msg.channel,
                    msg.chat_id,
                    text_preview
                );

                // Broadcast incoming message event to desktop UI
                let _ = event_bus.send(BroadcastEvent::MessageIncoming {
                    id: msg.id.clone(),
                    channel: msg.channel.clone(),
                    chat_id: msg.chat_id.clone(),
                    user_id: msg.user_id.clone(),
                    user_name: msg.user_name.clone(),
                    content: text_preview.to_string(),
                    timestamp: msg.timestamp.to_rfc3339(),
                    session_id: None, // Will be set by process_channel_message
                });

                // Check approval requirements
                let needs_approval = approval_required_map
                    .get(&msg.channel)
                    .copied()
                    .unwrap_or(false);

                if needs_approval {
                    if approvals.is_rejected(&msg.channel, &msg.user_id).await {
                        tracing::debug!("Rejected user {} sent message, ignoring", msg.user_id);
                        continue;
                    }

                    if !approvals.is_approved(&msg.channel, &msg.user_id).await {
                        if approvals.is_pending(&msg.channel, &msg.user_id).await {
                            let _ = send_reply(
                                registry,
                                &msg,
                                "Your access request is still pending admin approval. Please wait.",
                            )
                            .await;
                            continue;
                        }

                        // New unknown user — add to pending approvals
                        let pending = approvals::PendingApproval {
                            id: uuid::Uuid::new_v4().to_string(),
                            user_id: msg.user_id.clone(),
                            user_name: msg.user_name.clone(),
                            channel: msg.channel.clone(),
                            chat_id: msg.chat_id.clone(),
                            first_message: msg
                                .content
                                .as_text()
                                .unwrap_or("")
                                .to_string(),
                            timestamp: chrono::Utc::now(),
                        };

                        let approval_id = approvals.add_pending(pending.clone()).await;

                        let _ = event_bus.send(BroadcastEvent::ApprovalRequest {
                            id: approval_id,
                            user_id: pending.user_id,
                            user_name: pending.user_name,
                            channel: pending.channel,
                            chat_id: pending.chat_id,
                            first_message: pending.first_message,
                            timestamp: pending.timestamp.to_rfc3339(),
                        });

                        let _ = send_reply(
                            registry,
                            &msg,
                            "Welcome! Your access request has been submitted for admin approval.",
                        )
                        .await;
                        continue;
                    }
                }

                // Rate limiting check
                let rate_key = format!("{}:{}", msg.channel, msg.chat_id);
                if !rate_limiter.check_and_increment(&rate_key).await {
                    tracing::warn!("Rate limited: {}", rate_key);
                    let _ = send_reply(
                        registry,
                        &msg,
                        "You're sending messages too fast. Please wait a moment.",
                    )
                    .await;
                    continue;
                }

                // Update last activity for idle session tracking
                {
                    let mut activity = last_activity.write().await;
                    activity.insert(rate_key, Instant::now());
                }

                // Process message in a separate task
                let registry = registry.clone();
                let providers = providers.clone();
                let memory = memory.clone();
                let contexts = contexts.clone();
                let model = default_model.to_string();
                let event_bus = event_bus.clone();
                let skills = skills.clone();
                let sessions = sessions.clone();
                let active_sessions = active_sessions.clone();
                let project_store = project_store.clone();
                let usage_stats = usage_stats.clone();
                let config = config.clone();

                tokio::spawn(async move {
                    if let Err(e) = process_channel_message(
                        msg,
                        &registry,
                        &providers,
                        &memory,
                        &contexts,
                        &model,
                        &event_bus,
                        &skills,
                        &sessions,
                        &active_sessions,
                        &project_store,
                        &usage_stats,
                        &config,
                    )
                    .await
                    {
                        tracing::error!("Failed to process message: {}", e);
                    }
                });
            }
        }
    }
}

/// Parse @project mention from message text.
/// Returns (project_name, remaining_text, was_mention_found).
fn parse_project_mention(text: &str) -> (String, String, bool) {
    let trimmed = text.trim();
    if trimmed.starts_with('@') {
        if let Some(space_pos) = trimmed.find(char::is_whitespace) {
            let project_name = &trimmed[1..space_pos];
            if crate::project::validate_project_name(project_name) {
                let rest = trimmed[space_pos..].trim().to_string();
                if !rest.is_empty() {
                    return (project_name.to_string(), rest, true);
                }
            }
        }
    }
    ("default".to_string(), trimmed.to_string(), false)
}

/// Build a coding-focused system prompt for a project
fn build_coding_system_prompt(project: &crate::project::Project) -> String {
    let shell_section = if project.shell_enabled {
        "\n- shell_exec: Execute shell commands in the workspace (e.g., run tests, build, install)"
    } else {
        ""
    };

    let custom_prompt = project
        .system_prompt
        .as_deref()
        .map(|p| format!("\n\n## Custom Instructions\n{}", p))
        .unwrap_or_default();

    format!(
        r#"You are a coding assistant with access to the project workspace.

## Project
- Name: {name}
- Workspace: {workspace}

## Available Tools
- file_read: Read file contents (supports line ranges)
- file_write: Write or create files
- file_edit: Make targeted edits (search/replace)
- file_delete: Remove files
- list_directory: List directory contents
- file_search: Search file contents with regex
- git_status: Check git status and recent commits{shell}

## Guidelines
1. Read files before editing to understand context
2. Use file_edit for targeted changes, file_write for new files
3. Use list_directory to explore project structure
4. Use file_search to find relevant code
5. Explain what changes you're making and why

To use a tool, respond with:
<tool_call>{{"name": "tool_name", "arguments": {{"key": "value"}}}}</tool_call>{custom}"#,
        name = project.name,
        workspace = project.workspace_dir.display(),
        shell = shell_section,
        custom = custom_prompt,
    )
}

/// Process a channel message through the agent
async fn process_channel_message(
    msg: IncomingMessage,
    registry: &Arc<ChannelRegistry>,
    providers: &Arc<ProviderRegistry>,
    memory: &Arc<RwLock<MemoryStore>>,
    contexts: &Arc<RwLock<HashMap<String, AgentContext>>>,
    model: &str,
    event_bus: &broadcast::Sender<BroadcastEvent>,
    skills: &Option<Arc<RwLock<SkillRegistry>>>,
    sessions: &Arc<SessionManager>,
    active_sessions: &Arc<RwLock<ActiveSessionMap>>,
    project_store: &Arc<RwLock<ProjectStore>>,
    usage_stats: &Arc<UsageStats>,
    config: &Arc<crate::Config>,
) -> anyhow::Result<()> {
    // Only process text messages for now
    let text = match msg.content.as_text() {
        Some(t) => t.to_string(),
        None => {
            tracing::debug!("Skipping non-text message");
            return Ok(());
        }
    };

    // Check for slash commands — all /commands are handled locally, never sent to the LLM
    if text.trim().starts_with('/') {
        return handle_slash_command(
            &text, &msg, registry, providers, model, skills, event_bus,
            sessions, active_sessions, contexts, project_store, usage_stats, config,
        )
        .await;
    }

    // ── @project mention routing ──────────────────────────────────
    let (target_project, actual_text, is_project_mention) = parse_project_mention(&text);

    // If @project syntax used, validate the project exists
    if is_project_mention {
        let store = project_store.read().await;
        if !store.exists(&target_project).await {
            send_reply(
                registry,
                &msg,
                &format!(
                    "Project '{}' not found. Use /newproject {} to create it.",
                    target_project, target_project
                ),
            )
            .await?;
            return Ok(());
        }
    }

    // Use project-specific context key when @project is used
    let context_key = if is_project_mention {
        format!("{}:{}:{}", msg.channel, msg.chat_id, target_project)
    } else {
        format!("{}:{}", msg.channel, msg.chat_id)
    };

    // ── Session management ─────────────────────────────────────────
    // Resolve or create session
    let (session_id, project) = if is_project_mention {
        // For @project mentions, use a project-specific session
        let base_key = format!("{}:{}", msg.channel, msg.chat_id);
        let project_key = format!("{}:{}", base_key, target_project);

        let active = active_sessions.read().await;
        match active.get(&project_key) {
            Some(info) => (info.session_id.clone(), info.project.clone()),
            None => {
                drop(active);
                let new_id = uuid::Uuid::new_v4().to_string();
                let title = format!("@{}: {}", target_project, actual_text.chars().take(40).collect::<String>());
                let now = chrono::Utc::now();
                let session = Session {
                    id: new_id.clone(),
                    channel: msg.channel.clone(),
                    chat_id: msg.chat_id.clone(),
                    agent_id: "default".to_string(),
                    project: target_project.clone(),
                    created_at: now,
                    updated_at: now,
                    title: Some(title),
                    metadata: serde_json::json!({}),
                };
                if let Err(e) = sessions.create(&session).await {
                    tracing::error!("Failed to create session: {}", e);
                }
                let mut active = active_sessions.write().await;
                active.set(
                    project_key,
                    ActiveSessionInfo {
                        session_id: new_id.clone(),
                        project: target_project.clone(),
                    },
                );
                (new_id, target_project.clone())
            }
        }
    } else {
        let active = active_sessions.read().await;
        match active.get(&context_key) {
            Some(info) => (info.session_id.clone(), info.project.clone()),
            None => {
                drop(active);
                let new_id = uuid::Uuid::new_v4().to_string();
                let title = actual_text.chars().take(50).collect::<String>();
                let now = chrono::Utc::now();
                let session = Session {
                    id: new_id.clone(),
                    channel: msg.channel.clone(),
                    chat_id: msg.chat_id.clone(),
                    agent_id: "default".to_string(),
                    project: "default".to_string(),
                    created_at: now,
                    updated_at: now,
                    title: Some(title),
                    metadata: serde_json::json!({}),
                };
                if let Err(e) = sessions.create(&session).await {
                    tracing::error!("Failed to create session: {}", e);
                }
                let mut active = active_sessions.write().await;
                active.set(
                    context_key.clone(),
                    ActiveSessionInfo {
                        session_id: new_id.clone(),
                        project: "default".to_string(),
                    },
                );
                tracing::info!("Created new session {} for {}", new_id, context_key);
                (new_id, "default".to_string())
            }
        }
    };

    // Get or create context for this chat, rebuilding from transcript if empty
    let mut context = {
        let mut contexts_write = contexts.write().await;
        let ctx = contexts_write
            .entry(context_key.clone())
            .or_insert_with(AgentContext::default);

        // If context is empty (e.g., after server restart), rebuild from transcript
        if ctx.message_count() == 0 {
            if let Ok(entries) = sessions.load_transcript(&project, &session_id).await {
                for entry in &entries {
                    match &entry.entry_type {
                        TranscriptEntryType::User { content, .. } => {
                            ctx.add_user_message(content);
                        }
                        TranscriptEntryType::Assistant { content, .. } => {
                            ctx.add_assistant_message(content);
                        }
                        _ => {}
                    }
                }
                if !entries.is_empty() {
                    tracing::info!(
                        "Rebuilt context from {} transcript entries for session {}",
                        entries.len(),
                        session_id
                    );
                }
            }
        }
        ctx.clone()
    };

    // ── Load project data and configure tools ─────────────────────
    let project_data = {
        let store = project_store.read().await;
        store.load(&project).await.ok().flatten()
    };

    // Determine the effective model (project override or default)
    let effective_model = project_data
        .as_ref()
        .and_then(|p| p.model.as_deref())
        .unwrap_or(model);

    // Get the provider
    let provider = match providers.get_for_model(effective_model) {
        Some(p) => p,
        None => {
            tracing::error!("No provider found for model: {}", effective_model);
            send_reply(
                registry,
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
        session_id: Some(session_id.clone()),
    };

    // Create tool registry: coding tools if project has them enabled, otherwise just memory
    let tools = if let Some(ref proj) = project_data {
        if proj.coding_tools_enabled {
            // Set system prompt for coding context
            let sys_prompt = build_coding_system_prompt(proj);
            context.set_system_prompt(&sys_prompt);

            Arc::new(ToolRegistry::with_coding_tools(
                memory.clone(),
                memory_scope,
                proj.workspace_dir.clone(),
                proj.shell_enabled,
            ))
        } else {
            Arc::new(ToolRegistry::with_memory(memory.clone(), memory_scope))
        }
    } else {
        Arc::new(ToolRegistry::with_memory(memory.clone(), memory_scope))
    };

    let agent_config = AgentConfig {
        model: effective_model.to_string(),
        ..Default::default()
    };
    let executor = AgentExecutor::new(provider, tools, agent_config);

    // Persist the user message to transcript
    let user_entry = TranscriptEntry {
        timestamp: chrono::Utc::now(),
        entry_type: TranscriptEntryType::User {
            user_id: msg.user_id.clone(),
            content: actual_text.clone(),
        },
    };
    if let Err(e) = sessions.append(&project, &session_id, &user_entry).await {
        tracing::error!("Failed to append user transcript: {}", e);
    }

    // Check if channel supports streaming updates via message editing
    let supports_streaming = registry.supports_edit(&msg.channel).await;

    if supports_streaming {
        process_with_streaming(
            &executor,
            &mut context,
            &actual_text,
            registry,
            &msg,
            &context_key,
            contexts,
            event_bus,
            sessions,
            &session_id,
            &project,
            effective_model,
            usage_stats,
            is_project_mention,
            &target_project,
        )
        .await
    } else {
        match executor.execute(&mut context, &actual_text).await {
            Ok(result) => {
                {
                    let mut contexts_write = contexts.write().await;
                    contexts_write.insert(context_key, context);
                }

                // Prefix response with project name when using @project syntax
                let reply_content = if is_project_mention {
                    format!("{}: {}", target_project, result.content)
                } else {
                    result.content.clone()
                };
                send_reply(registry, &msg, &reply_content).await?;

                // Persist assistant response to transcript
                let assistant_entry = TranscriptEntry {
                    timestamp: chrono::Utc::now(),
                    entry_type: TranscriptEntryType::Assistant {
                        content: result.content.clone(),
                        model: effective_model.to_string(),
                    },
                };
                if let Err(e) = sessions.append(&project, &session_id, &assistant_entry).await {
                    tracing::error!("Failed to append assistant transcript: {}", e);
                }

                // Update session timestamp
                if let Ok(Some(mut session)) = sessions.load_from_project(&project, &session_id).await {
                    session.updated_at = chrono::Utc::now();
                    let _ = sessions.update(&session).await;
                }

                // Broadcast outgoing message
                let _ = event_bus.send(BroadcastEvent::MessageOutgoing {
                    channel: msg.channel.clone(),
                    chat_id: msg.chat_id.clone(),
                    content: result.content.clone(),
                    reply_to: Some(msg.id.clone()),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    session_id: Some(session_id),
                });

                // Track token usage
                usage_stats.record_usage(result.usage.prompt_tokens, result.usage.completion_tokens);

                tracing::info!(
                    "Responded to chat '{}' with {} tokens",
                    msg.chat_id,
                    result.usage.total_tokens
                );
                Ok(())
            }
            Err(e) => {
                tracing::error!("Agent execution failed: {}", e);
                let error_msg = format!("Sorry, I encountered an error: {}", e);
                send_reply(registry, &msg, &error_msg).await?;

                let _ = event_bus.send(BroadcastEvent::MessageError {
                    channel: msg.channel.clone(),
                    chat_id: msg.chat_id.clone(),
                    error: e.to_string(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    session_id: Some(session_id),
                });

                Ok(())
            }
        }
    }
}

/// Handle a slash command message (e.g., /help, /skills, /status, /skill-name args)
///
/// All messages starting with `/` are handled locally — they never reach the LLM provider.
/// Telegram sends commands as `/command@botname` in groups, so we strip the `@...` suffix.
async fn handle_slash_command(
    text: &str,
    msg: &IncomingMessage,
    registry: &Arc<ChannelRegistry>,
    providers: &Arc<ProviderRegistry>,
    model: &str,
    skills: &Option<Arc<RwLock<SkillRegistry>>>,
    event_bus: &broadcast::Sender<BroadcastEvent>,
    sessions: &Arc<SessionManager>,
    active_sessions: &Arc<RwLock<ActiveSessionMap>>,
    contexts: &Arc<RwLock<HashMap<String, AgentContext>>>,
    project_store: &Arc<RwLock<ProjectStore>>,
    usage_stats: &Arc<UsageStats>,
    config: &Arc<crate::Config>,
) -> anyhow::Result<()> {
    let trimmed = text.trim();
    let (raw_command, args) = match trimmed.find(' ') {
        Some(pos) => (&trimmed[1..pos], trimmed[pos + 1..].trim()),
        None => (&trimmed[1..], ""),
    };

    // Strip Telegram @botname suffix (e.g., "/help@mybot" → "help")
    let command = raw_command.split('@').next().unwrap_or(raw_command);

    tracing::info!("Slash command: /{} from {} (args: {:?})", command, msg.user_id, args);

    let context_key = format!("{}:{}", msg.channel, msg.chat_id);

    let response = match command {
        "help" | "commands" => generate_help_text(skills).await,
        "skills" => generate_skills_list(skills).await,
        "status" => generate_status_text(
            providers, model, skills, contexts, sessions, active_sessions,
            usage_stats, config, &context_key,
        ).await,
        "model" => format!("Current model: {}", model),
        "start" => "Bot is already running and listening.".to_string(),
        "ping" => "Pong!".to_string(),
        "id" => format!(
            "Chat ID: {}\nUser ID: {}\nChannel: {}",
            msg.chat_id, msg.user_id, msg.channel
        ),
        "version" => format!("BXNode Bot v{}", env!("CARGO_PKG_VERSION")),

        // ── BTW side question ───────────────────────────────────
        "btw" => {
            if args.is_empty() {
                "Usage: /btw <your question>\n\nAsk a quick side question without affecting the conversation history.".to_string()
            } else {
                handle_btw_side_question(providers, model, contexts, &context_key, args).await
            }
        }

        // ── Session commands ────────────────────────────────────
        "sessions" => {
            generate_sessions_list(sessions, active_sessions, &msg.channel, &msg.chat_id).await
        }
        "newsession" => {
            handle_new_session(sessions, active_sessions, contexts, &context_key, &msg.channel, &msg.chat_id).await
        }

        // ── Project commands ────────────────────────────────────
        "projects" => {
            generate_projects_list(sessions, active_sessions, project_store, &context_key).await
        }
        "newproject" => {
            handle_new_project(sessions, project_store, config, args).await
        }
        "deleteproject" => {
            handle_delete_project(sessions, project_store, args).await
        }
        "project" => {
            handle_switch_project(sessions, active_sessions, contexts, project_store, &context_key, &msg.channel, &msg.chat_id, args).await
        }
        "projectinfo" => {
            handle_project_info(sessions, active_sessions, project_store, &context_key, args).await
        }
        "projectconfig" => {
            handle_project_config(project_store, active_sessions, &context_key, args).await
        }

        _ => {
            // Try to find a matching skill
            if let Some(skills_lock) = skills {
                let skills_read = skills_lock.read().await;
                if let Some(info) = skills_read.list_info().iter().find(|s| {
                    s.metadata.name == command
                        || s.metadata.name.replace('-', "_") == command
                        || s.metadata.name.replace('_', "-") == command
                }) {
                    if info.is_active {
                        format!(
                            "Skill '{}': {}\n\nUse this skill by sending a normal message related to it.",
                            info.metadata.name, info.metadata.description
                        )
                    } else {
                        format!(
                            "Skill '{}' exists but is not active. Enable it from the desktop app.",
                            command
                        )
                    }
                } else {
                    generate_unknown_command_text(command, &skills_read)
                }
            } else {
                format!(
                    "Unknown command: /{}\n\nType /help to see available commands.",
                    command
                )
            }
        }
    };

    if let Err(e) = send_reply(registry, msg, &response).await {
        tracing::error!("Failed to send slash command reply ({} chars): {}", response.len(), e);
        // Try sending a truncated version as a last resort
        let truncated = if response.len() > 4000 {
            format!("{}...\n\n(truncated)", &response[..3990])
        } else {
            response.clone()
        };
        let _ = send_reply(registry, msg, &truncated).await;
    }

    // Broadcast the outgoing response
    let _ = event_bus.send(BroadcastEvent::MessageOutgoing {
        channel: msg.channel.clone(),
        chat_id: msg.chat_id.clone(),
        content: response,
        reply_to: Some(msg.id.clone()),
        timestamp: chrono::Utc::now().to_rfc3339(),
        session_id: None,
    });

    Ok(())
}

/// Handle a /btw side question — ephemeral LLM call that doesn't pollute session history.
async fn handle_btw_side_question(
    providers: &Arc<ProviderRegistry>,
    model: &str,
    contexts: &Arc<RwLock<HashMap<String, AgentContext>>>,
    context_key: &str,
    question: &str,
) -> String {
    use crate::providers::{CompletionRequest, Message, Role};

    // 1. Snapshot recent context (read-only, last 5 messages max)
    let recent_messages = {
        let contexts_read = contexts.read().await;
        match contexts_read.get(context_key) {
            Some(ctx) => {
                let all = ctx.messages_for_provider();
                let start = if all.len() > 5 { all.len() - 5 } else { 0 };
                all[start..].to_vec()
            }
            None => vec![],
        }
    };

    // 2. Build ephemeral messages with side-question system prompt
    let mut messages = vec![Message {
        role: Role::System,
        content: "You are answering a brief /btw side question about the current conversation. \
                  Use the conversation only as background context. \
                  Answer only the side question in the last user message. \
                  Do not continue, resume, or complete any unfinished task from the conversation. \
                  Be concise."
            .to_string(),
    }];
    messages.extend(recent_messages);
    messages.push(Message {
        role: Role::User,
        content: format!("[Side question] {}", question),
    });

    // 3. Resolve provider and call LLM without tools
    let provider = match providers.get_for_model(model) {
        Some(p) => p,
        None => return format!("No provider found for model '{}'", model),
    };

    let model_name = providers.extract_model_name(model);
    let request = CompletionRequest {
        model: model_name,
        messages,
        temperature: Some(0.3),
        max_tokens: Some(1024),
        tools: vec![],
        stream: false,
        stop: vec![],
    };

    match provider.complete(request).await {
        Ok(response) => response.content,
        Err(e) => format!("Failed to answer side question: {}", e),
    }
}

/// Generate help text listing available commands
async fn generate_help_text(skills: &Option<Arc<RwLock<SkillRegistry>>>) -> String {
    let mut help = String::from("Available commands:\n\n");
    help.push_str("/help - Show this help message\n");
    help.push_str("/status - Show bot status\n");
    help.push_str("/skills - List active skills\n");
    help.push_str("/model - Show current model\n");
    help.push_str("/id - Show chat and user IDs\n");
    help.push_str("/ping - Check if bot is alive\n");
    help.push_str("/version - Show bot version\n");
    help.push_str("/btw <question> - Quick side question (doesn't affect history)\n");
    help.push_str("\nSession commands:\n");
    help.push_str("/sessions - List sessions for this chat\n");
    help.push_str("/newsession - Start a new session\n");
    help.push_str("\nProject commands:\n");
    help.push_str("/projects - List all projects\n");
    help.push_str("/project <name> - Switch to a project\n");
    help.push_str("/newproject <name> [--shell] [--model provider/model] - Create a new project\n");
    help.push_str("/deleteproject <name> - Delete a project\n");
    help.push_str("/projectinfo [name] - Show project details\n");
    help.push_str("/projectconfig <key> <value> - Update project settings\n");
    help.push_str("\nCoding via @project:\n");
    help.push_str("@projectname <message> - Send message to a specific project\n");

    if let Some(skills_lock) = skills {
        let skills_read = skills_lock.read().await;
        let active: Vec<_> = skills_read
            .list_info()
            .into_iter()
            .filter(|s| s.is_active)
            .collect();
        if !active.is_empty() {
            help.push_str("\nSkill commands:\n");
            for info in active {
                help.push_str(&format!(
                    "/{} - {}\n",
                    info.metadata.name, info.metadata.description
                ));
            }
        }
    }

    help
}

/// Generate status text with detailed usage information
async fn generate_status_text(
    providers: &Arc<ProviderRegistry>,
    model: &str,
    skills: &Option<Arc<RwLock<SkillRegistry>>>,
    contexts: &Arc<RwLock<HashMap<String, AgentContext>>>,
    sessions: &Arc<SessionManager>,
    active_sessions: &Arc<RwLock<ActiveSessionMap>>,
    usage_stats: &Arc<UsageStats>,
    config: &Arc<crate::Config>,
    context_key: &str,
) -> String {
    let version = env!("CARGO_PKG_VERSION");
    let git_hash = env!("GIT_HASH");

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("BXNode Bot {} ({})", version, git_hash));

    // Model & provider info
    let (provider_id, model_name) = model.split_once('/').unwrap_or(("default", model));

    // Try to find context length from model info
    let max_context = providers
        .all_models_prefixed()
        .iter()
        .find(|m| m.id == model)
        .map(|m| m.model.context_length)
        .unwrap_or(0);

    lines.push(format!(
        "\u{1f9e0} Model: {} \u{00b7} Provider: {}",
        model_name, provider_id
    ));

    // Providers summary
    let provider_ids = providers.provider_ids();
    lines.push(format!(
        "\u{1f50c} Providers: {} [{}]",
        provider_ids.len(),
        provider_ids.join(", ")
    ));

    // Resolve active session for this chat (needed for context + session info)
    let active_info = {
        let active = active_sessions.read().await;
        active.get(context_key).cloned()
    };

    // Token usage — combine in-memory counters with transcript-based estimate
    // (in-memory counters reset on server restart, so we also estimate from transcript)
    let mut tokens_in = usage_stats.tokens_in.load(Ordering::Relaxed);
    let mut tokens_out = usage_stats.tokens_out.load(Ordering::Relaxed);

    if tokens_in == 0 && tokens_out == 0 {
        // Server was restarted — estimate from transcript of all sessions
        if let Ok(all_sessions) = sessions.list().await {
            for sess in &all_sessions {
                if let Ok(entries) = sessions.load_transcript(&sess.project, &sess.id).await {
                    for entry in &entries {
                        match &entry.entry_type {
                            TranscriptEntryType::User { content, .. } => {
                                // Rough estimate: ~4 chars per token
                                tokens_in += (content.len() as u64) / 4;
                            }
                            TranscriptEntryType::Assistant { content, .. } => {
                                tokens_out += (content.len() as u64) / 4;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    lines.push(format!(
        "\u{1f4ca} Tokens: {} in / {} out",
        format_token_count(tokens_in),
        format_token_count(tokens_out),
    ));

    // Context window usage — try in-memory context first, fall back to transcript estimate
    let estimated_tokens = {
        let contexts_read = contexts.read().await;
        if let Some(ctx) = contexts_read.get(context_key) {
            ctx.summary().estimated_tokens
        } else {
            // Not in memory — estimate from current session's transcript
            let mut est = 0u32;
            if let Some(ref info) = active_info {
                if let Ok(entries) = sessions.load_transcript(&info.project, &info.session_id).await {
                    for entry in &entries {
                        match &entry.entry_type {
                            TranscriptEntryType::User { content, .. }
                            | TranscriptEntryType::Assistant { content, .. } => {
                                est += (content.len() as f64 / 4.0).ceil() as u32 + 4;
                            }
                            _ => {}
                        }
                    }
                }
            }
            est
        }
    };

    if max_context > 0 {
        let pct = (estimated_tokens as f64 / max_context as f64 * 100.0) as u32;
        lines.push(format!(
            "\u{1f4da} Context: {}k/{}k ({}%)",
            estimated_tokens / 1000,
            max_context / 1000,
            pct,
        ));
    } else if estimated_tokens > 0 {
        lines.push(format!(
            "\u{1f4da} Context: ~{}k tokens",
            estimated_tokens / 1000,
        ));
    } else {
        lines.push("\u{1f4da} Context: idle".to_string());
    }

    // Compactions
    let compactions = usage_stats.compactions.load(Ordering::Relaxed);
    lines.push(format!("\u{1f5dc} Compactions: {}", compactions));

    // Session info
    if let Some(ref info) = active_info {
        let (channel_part, chat_part) = context_key.split_once(':').unwrap_or(("", ""));
        let session_count = sessions
            .list_for_chat(channel_part, chat_part)
            .await
            .map(|s| s.len())
            .unwrap_or(0);
        lines.push(format!(
            "\u{1f4ac} Session: {} ({} total)",
            &info.session_id[..8.min(info.session_id.len())],
            session_count,
        ));
    } else {
        lines.push("\u{1f4ac} Session: none".to_string());
    }

    // Z.AI quota/usage (if Z.AI provider is configured)
    if let Some(ref zai_config) = config.providers.zai {
        match fetch_zai_quota(&zai_config.api_key).await {
            Ok(quota_text) => lines.push(format!("\u{1f4b3} {}", quota_text)),
            Err(e) => {
                tracing::debug!("Failed to fetch Z.AI quota: {}", e);
                lines.push("\u{1f4b3} Usage: unavailable".to_string());
            }
        }
    }

    // Skills
    if let Some(skills_lock) = skills {
        let skills_read = skills_lock.read().await;
        let info_list = skills_read.list_info();
        let active_count = info_list.iter().filter(|s| s.is_active).count();
        let user_invocable = info_list
            .iter()
            .filter(|s| s.is_active && s.metadata.user_invocable)
            .count();
        lines.push(format!(
            "\u{1f9e9} Skills: {} active / {} total \u{00b7} {} invocable",
            active_count,
            info_list.len(),
            user_invocable
        ));
    } else {
        lines.push("\u{1f9e9} Skills: disabled".to_string());
    }

    // Runtime
    lines.push(format!(
        "\u{2699}\u{fe0f} Runtime: direct \u{00b7} Channels: streaming"
    ));

    lines.join("\n")
}

/// Format a token count for display (e.g., 1500000 -> "1.5m", 45000 -> "45k", 800 -> "800")
fn format_token_count(count: u64) -> String {
    if count >= 1_000_000 {
        let m = count as f64 / 1_000_000.0;
        if m >= 10.0 {
            format!("{:.0}m", m)
        } else {
            format!("{:.1}m", m)
        }
    } else if count >= 1_000 {
        format!("{}k", count / 1_000)
    } else {
        count.to_string()
    }
}

/// Fetch Z.AI quota/usage information
async fn fetch_zai_quota(api_key: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.z.ai/api/monitor/usage/quota/limit")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept-Language", "en-US,en")
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("Z.AI quota API returned {}", resp.status()));
    }

    let body: serde_json::Value = resp.json().await?;

    if !body.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
        return Err(anyhow::anyhow!("Z.AI quota API returned non-success"));
    }

    let limits = body
        .get("data")
        .and_then(|d| d.get("limits"))
        .and_then(|l| l.as_array())
        .ok_or_else(|| anyhow::anyhow!("Missing limits in Z.AI response"))?;

    let mut parts: Vec<String> = Vec::new();

    for limit in limits {
        let limit_type = limit.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let unit = limit.get("unit").and_then(|v| v.as_u64()).unwrap_or(0);
        let number = limit.get("number").and_then(|v| v.as_u64()).unwrap_or(0);
        // percentage from Z.AI API is "used" percentage, not "remaining"
        let used_pct = limit.get("percentage").and_then(|v| v.as_f64()).unwrap_or(0.0)
            .clamp(0.0, 100.0);
        let remaining_pct = 100.0 - used_pct;
        let next_reset = limit.get("nextResetTime").and_then(|v| v.as_str()).unwrap_or("");

        // Build window label using unit + number (e.g., unit=3,number=5 -> "5h")
        let window_label = match unit {
            1 => format!("{}d", number),
            3 => format!("{}h", number),
            5 => format!("{}m", number),
            _ => "?".to_string(),
        };

        let label = match limit_type {
            "TIME_LIMIT" => "Monthly".to_string(),
            "TOKENS_LIMIT" => format!("Tokens ({})", window_label),
            _ => continue,
        };

        let reset_str = format_reset_time(next_reset);
        parts.push(format!(
            "{} {:.0}% left{}",
            label,
            remaining_pct,
            if reset_str.is_empty() { String::new() } else { format!(" {}", reset_str) },
        ));
    }

    if parts.is_empty() {
        Ok("Usage: no limits found".to_string())
    } else {
        Ok(format!("Usage: {}", parts.join(" \u{00b7} ")))
    }
}

/// Format a reset time string as relative duration
fn format_reset_time(reset_time: &str) -> String {
    let Ok(reset_dt) = chrono::DateTime::parse_from_rfc3339(reset_time)
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(reset_time, "%Y-%m-%d %H:%M:%S")
            .map(|n| n.and_utc().fixed_offset()))
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(reset_time, "%Y-%m-%dT%H:%M:%S")
            .map(|n| n.and_utc().fixed_offset()))
    else {
        return String::new();
    };

    let now = chrono::Utc::now();
    let diff = reset_dt.signed_duration_since(now);
    let total_seconds = diff.num_seconds();

    if total_seconds <= 0 {
        return "\u{23f1}now".to_string();
    }

    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let days = hours / 24;
    let remaining_hours = hours % 24;

    if days >= 7 {
        format!("\u{23f1}{}", reset_dt.format("%b %d"))
    } else if days >= 1 {
        format!("\u{23f1}{}d{}h", days, remaining_hours)
    } else if hours >= 1 {
        format!("\u{23f1}{}h{}m", hours, minutes)
    } else {
        format!("\u{23f1}{}m", minutes)
    }
}

/// Generate a list of skills
async fn generate_skills_list(skills: &Option<Arc<RwLock<SkillRegistry>>>) -> String {
    if let Some(skills_lock) = skills {
        let skills_read = skills_lock.read().await;
        let info_list = skills_read.list_info();
        if info_list.is_empty() {
            "No skills discovered.".to_string()
        } else {
            let mut text = String::from("Skills:\n\n");
            for info in info_list {
                let status = if info.is_active { "active" } else { "inactive" };
                text.push_str(&format!(
                    "- {} [{}]: {}\n",
                    info.metadata.name, status, info.metadata.description
                ));
            }
            text
        }
    } else {
        "Skills system is not enabled.".to_string()
    }
}

/// Generate unknown command help text
fn generate_unknown_command_text(
    command: &str,
    skills_read: &crate::skills::SkillRegistry,
) -> String {
    let mut reply = format!(
        "Unknown command: /{}\n\nType /help to see available commands.",
        command
    );
    let active: Vec<_> = skills_read
        .list_info()
        .into_iter()
        .filter(|s| s.is_active)
        .collect();
    if !active.is_empty() {
        reply.push_str("\n\nDid you mean one of these?\n");
        for s in active {
            reply.push_str(&format!("/{} - {}\n", s.metadata.name, s.metadata.description));
        }
    }
    reply
}

// ── Session & Project command handlers ──────────────────────────

/// Generate session list for /sessions command
async fn generate_sessions_list(
    sessions: &Arc<SessionManager>,
    active_sessions: &Arc<RwLock<ActiveSessionMap>>,
    channel: &str,
    chat_id: &str,
) -> String {
    let context_key = format!("{}:{}", channel, chat_id);
    let chat_sessions = match sessions.list_for_chat(channel, chat_id).await {
        Ok(mut s) => {
            s.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            s
        }
        Err(e) => {
            return format!("Failed to list sessions: {}", e);
        }
    };

    if chat_sessions.is_empty() {
        return "No sessions yet. Send a message to start one, or use /newsession.".to_string();
    }

    let active = active_sessions.read().await;
    let active_id = active.get(&context_key).map(|i| i.session_id.as_str());

    let mut text = String::from("Sessions for this chat:\n\n");
    for (i, session) in chat_sessions.iter().enumerate() {
        let is_active = active_id == Some(session.id.as_str());
        let marker = if is_active { " [active]" } else { "" };
        let title = session.title.as_deref().unwrap_or("Untitled");
        let ago = format_relative_time(session.updated_at);
        text.push_str(&format!(
            "{}. {}{} \u{2014} {}\n",
            i + 1,
            title,
            marker,
            ago,
        ));
    }
    text.push_str("\nUse /newsession to start a fresh session.");
    text
}

/// Handle /newsession command
async fn handle_new_session(
    sessions: &Arc<SessionManager>,
    active_sessions: &Arc<RwLock<ActiveSessionMap>>,
    contexts: &Arc<RwLock<HashMap<String, AgentContext>>>,
    context_key: &str,
    channel: &str,
    chat_id: &str,
) -> String {
    // Determine the active project for this chat
    let project = {
        let active = active_sessions.read().await;
        active.get(context_key).map(|i| i.project.clone()).unwrap_or_else(|| "default".to_string())
    };

    let new_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    let session = Session {
        id: new_id.clone(),
        channel: channel.to_string(),
        chat_id: chat_id.to_string(),
        agent_id: "default".to_string(),
        project: project.clone(),
        created_at: now,
        updated_at: now,
        title: Some("New Session".to_string()),
        metadata: serde_json::json!({}),
    };

    if let Err(e) = sessions.create(&session).await {
        return format!("Failed to create session: {}", e);
    }

    // Update active session
    {
        let mut active = active_sessions.write().await;
        active.set(
            context_key.to_string(),
            ActiveSessionInfo {
                session_id: new_id.clone(),
                project,
            },
        );
    }

    // Clear in-memory context
    {
        let mut ctx = contexts.write().await;
        ctx.remove(context_key);
    }

    "Started a new session. Your previous session is still available via /sessions.".to_string()
}

/// Generate project list for /projects command
async fn generate_projects_list(
    sessions: &Arc<SessionManager>,
    active_sessions: &Arc<RwLock<ActiveSessionMap>>,
    project_store: &Arc<RwLock<ProjectStore>>,
    context_key: &str,
) -> String {
    let store = project_store.read().await;
    let projects = store.list().await.unwrap_or_default();

    // Also get session-only projects that may not be in store yet
    let session_projects = sessions.list_projects().await.unwrap_or_default();

    if projects.is_empty() && session_projects.is_empty() {
        return "No projects found. Use /newproject <name> to create one.".to_string();
    }

    let active = active_sessions.read().await;
    let active_project = active.get(context_key).map(|i| i.project.as_str());

    let mut text = String::from("Projects:\n\n");
    let mut listed = std::collections::HashSet::new();

    // Show ProjectStore projects first (they have workspace info)
    for (i, proj) in projects.iter().enumerate() {
        listed.insert(proj.name.clone());
        let is_active = active_project == Some(proj.name.as_str())
            || (active_project.is_none() && proj.name == "default");
        let marker = if is_active { " [active]" } else { "" };
        let count = sessions.count_sessions_in_project(&proj.name).await.unwrap_or(0);
        let session_word = if count == 1 { "session" } else { "sessions" };
        let shell = if proj.shell_enabled { " [shell]" } else { "" };
        text.push_str(&format!(
            "{}. {}{} ({} {}){}\n   {}\n",
            i + 1,
            proj.name,
            marker,
            count,
            session_word,
            shell,
            proj.workspace_dir.display(),
        ));
    }

    // Show any session-only projects not in the store
    let mut idx = projects.len();
    for name in &session_projects {
        if !listed.contains(name) {
            idx += 1;
            let is_active = active_project == Some(name.as_str());
            let marker = if is_active { " [active]" } else { "" };
            let count = sessions.count_sessions_in_project(name).await.unwrap_or(0);
            let session_word = if count == 1 { "session" } else { "sessions" };
            text.push_str(&format!(
                "{}. {}{} ({} {}) [no workspace]\n",
                idx, name, marker, count, session_word,
            ));
        }
    }

    text.push_str("\nUse /project <name> to switch, /newproject <name> to create.");
    text.push_str("\nUse @projectname <message> to send to a specific project.");
    text
}

/// Handle /newproject command
/// Usage: /newproject <name> [--shell] [--model provider/model]
async fn handle_new_project(
    sessions: &Arc<SessionManager>,
    project_store: &Arc<RwLock<ProjectStore>>,
    config: &Arc<crate::Config>,
    args: &str,
) -> String {
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.is_empty() {
        return "Usage: /newproject <name> [--shell] [--model provider/model]\n\nProject names can contain letters, numbers, hyphens, and underscores.".to_string();
    }

    let name = parts[0];

    // Validate name
    if !crate::project::validate_project_name(name) {
        return "Invalid project name. Use only letters, numbers, hyphens, and underscores (1-64 chars).".to_string();
    }

    // Parse flags
    let mut shell_enabled = config.workspace.shell_enabled;
    let mut model_override: Option<String> = None;
    let mut i = 1;
    while i < parts.len() {
        match parts[i] {
            "--shell" => {
                shell_enabled = true;
                i += 1;
            }
            "--model" => {
                if i + 1 < parts.len() {
                    model_override = Some(parts[i + 1].to_string());
                    i += 2;
                } else {
                    return "Usage: --model requires a value (e.g., --model anthropic/claude-3-opus)".to_string();
                }
            }
            _ => { i += 1; }
        }
    }

    // Create in ProjectStore
    let workspace_dir = {
        let store = project_store.read().await;
        if store.exists(name).await {
            return format!("Project '{}' already exists. Use @{} to interact with it.", name, name);
        }
        store.resolve_workspace_dir(name)
    };

    let project = crate::project::Project {
        name: name.to_string(),
        workspace_dir: workspace_dir.clone(),
        description: None,
        model: model_override,
        system_prompt: None,
        coding_tools_enabled: config.workspace.coding_tools_enabled,
        shell_enabled,
        metadata: serde_json::json!({}),
        created_at: chrono::Utc::now(),
    };

    {
        let store = project_store.write().await;
        if let Err(e) = store.create(&project).await {
            return format!("Failed to create project: {}", e);
        }
    }

    // Also create in sessions for compatibility
    if let Err(e) = sessions.create_project(name).await {
        tracing::debug!("Session project creation note: {}", e);
    }

    // Create workspace directory
    if let Err(e) = tokio::fs::create_dir_all(&workspace_dir).await {
        tracing::warn!("Failed to create workspace dir: {}", e);
    }

    let shell_note = if shell_enabled { " (shell enabled)" } else { "" };
    format!(
        "Created project '{}'{}.\nWorkspace: {}\n\nUse @{} <message> to start coding, or /project {} to switch.",
        name, shell_note, workspace_dir.display(), name, name
    )
}

/// Handle /deleteproject command
async fn handle_delete_project(
    sessions: &Arc<SessionManager>,
    project_store: &Arc<RwLock<ProjectStore>>,
    args: &str,
) -> String {
    let name = args.trim();
    if name.is_empty() {
        return "Usage: /deleteproject <name>".to_string();
    }

    if name == "default" {
        return "Cannot delete the default project.".to_string();
    }

    // Check if project exists in store
    let project_data = {
        let store = project_store.read().await;
        store.load(name).await.ok().flatten()
    };

    // Delete from ProjectStore
    {
        let store = project_store.write().await;
        if let Err(e) = store.delete(name).await {
            tracing::debug!("ProjectStore delete note: {}", e);
        }
    }

    // Delete sessions
    match sessions.delete_project(name).await {
        Ok(()) => {
            let workspace_note = if let Some(proj) = project_data {
                format!("\nWorkspace directory preserved: {}", proj.workspace_dir.display())
            } else {
                String::new()
            };
            format!("Deleted project '{}' and all its sessions.{}", name, workspace_note)
        }
        Err(e) => format!("Failed to delete project: {}", e),
    }
}

/// Handle /project command (switch active project)
async fn handle_switch_project(
    sessions: &Arc<SessionManager>,
    active_sessions: &Arc<RwLock<ActiveSessionMap>>,
    contexts: &Arc<RwLock<HashMap<String, AgentContext>>>,
    project_store: &Arc<RwLock<ProjectStore>>,
    context_key: &str,
    channel: &str,
    chat_id: &str,
    args: &str,
) -> String {
    let name = args.trim();
    if name.is_empty() {
        return "Usage: /project <name>\n\nUse /projects to see available projects.".to_string();
    }

    // Verify project exists (check both ProjectStore and sessions)
    let store = project_store.read().await;
    let in_store = store.exists(name).await;
    drop(store);

    let projects = sessions.list_projects().await.unwrap_or_default();
    if !in_store && !projects.iter().any(|p| p == name) {
        return format!("Project '{}' not found. Use /newproject {} to create it.", name, name);
    }

    // Find the most recent session in this project for this chat, or create one
    let chat_sessions = sessions.list_for_chat(channel, chat_id).await.unwrap_or_default();
    let project_session = chat_sessions
        .iter()
        .filter(|s| s.project == name)
        .max_by_key(|s| s.updated_at);

    let session_id = if let Some(s) = project_session {
        s.id.clone()
    } else {
        // Create a new session in the target project
        let new_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        let session = Session {
            id: new_id.clone(),
            channel: channel.to_string(),
            chat_id: chat_id.to_string(),
            agent_id: "default".to_string(),
            project: name.to_string(),
            created_at: now,
            updated_at: now,
            title: Some("New Session".to_string()),
            metadata: serde_json::json!({}),
        };
        if let Err(e) = sessions.create(&session).await {
            return format!("Failed to create session in project: {}", e);
        }
        new_id
    };

    // Update active session to point to this project's session
    {
        let mut active = active_sessions.write().await;
        active.set(
            context_key.to_string(),
            ActiveSessionInfo {
                session_id,
                project: name.to_string(),
            },
        );
    }

    // Clear in-memory context (will be rebuilt from transcript on next message)
    {
        let mut ctx = contexts.write().await;
        ctx.remove(context_key);
    }

    // Include workspace info if available
    let store = project_store.read().await;
    let workspace_note = if let Ok(Some(proj)) = store.load(name).await {
        format!("\nWorkspace: {}", proj.workspace_dir.display())
    } else {
        String::new()
    };

    format!("Switched to project '{}'. New sessions will be created here.{}\n\nUse @{} <message> to start coding.", name, workspace_note, name)
}

/// Handle /projectinfo command
async fn handle_project_info(
    sessions: &Arc<SessionManager>,
    active_sessions: &Arc<RwLock<ActiveSessionMap>>,
    project_store: &Arc<RwLock<ProjectStore>>,
    context_key: &str,
    args: &str,
) -> String {
    // If no name given, use the active project
    let name = if args.trim().is_empty() {
        let active = active_sessions.read().await;
        active.get(context_key).map(|i| i.project.clone()).unwrap_or_else(|| "default".to_string())
    } else {
        args.trim().to_string()
    };

    let store = project_store.read().await;
    match store.load(&name).await {
        Ok(Some(proj)) => {
            let session_count = sessions.count_sessions_in_project(&name).await.unwrap_or(0);
            let model_str = proj.model.as_deref().unwrap_or("(default)");
            let sys_prompt = proj.system_prompt.as_deref().unwrap_or("(none)");
            let coding = if proj.coding_tools_enabled { "enabled" } else { "disabled" };
            let shell = if proj.shell_enabled { "enabled" } else { "disabled" };

            format!(
                "Project: {}\nWorkspace: {}\nModel: {}\nCoding tools: {}\nShell: {}\nSessions: {}\nCreated: {}\nSystem prompt: {}",
                proj.name,
                proj.workspace_dir.display(),
                model_str,
                coding,
                shell,
                session_count,
                proj.created_at.format("%Y-%m-%d %H:%M"),
                sys_prompt,
            )
        }
        Ok(None) => format!("Project '{}' not found. Use /newproject {} to create it.", name, name),
        Err(e) => format!("Failed to load project: {}", e),
    }
}

/// Handle /projectconfig command
/// Usage: /projectconfig <key> <value>
/// Keys: model, shell, systemprompt, coding
async fn handle_project_config(
    project_store: &Arc<RwLock<ProjectStore>>,
    active_sessions: &Arc<RwLock<ActiveSessionMap>>,
    context_key: &str,
    args: &str,
) -> String {
    let parts: Vec<&str> = args.splitn(2, char::is_whitespace).collect();
    if parts.is_empty() || parts[0].is_empty() {
        return "Usage: /projectconfig <key> <value>\n\nKeys:\n  model <provider/model> - Set LLM model\n  shell on|off - Enable/disable shell\n  coding on|off - Enable/disable coding tools\n  systemprompt <text> - Set custom system prompt\n  systemprompt clear - Clear system prompt".to_string();
    }

    let key = parts[0];
    let value = if parts.len() > 1 { parts[1].trim() } else { "" };

    // Get active project
    let project_name = {
        let active = active_sessions.read().await;
        active.get(context_key).map(|i| i.project.clone()).unwrap_or_else(|| "default".to_string())
    };

    let mut store = project_store.write().await;
    let project = match store.load(&project_name).await {
        Ok(Some(p)) => p,
        Ok(None) => return format!("Project '{}' not found. Switch to a project first with /project <name>.", project_name),
        Err(e) => return format!("Failed to load project: {}", e),
    };

    let mut updated = project.clone();

    match key {
        "model" => {
            if value.is_empty() {
                return format!("Current model: {}\n\nUsage: /projectconfig model <provider/model>", project.model.as_deref().unwrap_or("(default)"));
            }
            if value == "default" || value == "clear" {
                updated.model = None;
            } else {
                updated.model = Some(value.to_string());
            }
        }
        "shell" => {
            match value.to_lowercase().as_str() {
                "on" | "true" | "enable" | "enabled" | "1" | "yes" => {
                    updated.shell_enabled = true;
                }
                "off" | "false" | "disable" | "disabled" | "0" | "no" => {
                    updated.shell_enabled = false;
                }
                _ => return "Usage: /projectconfig shell on|off".to_string(),
            }
        }
        "coding" => {
            match value.to_lowercase().as_str() {
                "on" | "true" | "enable" | "enabled" | "1" | "yes" => {
                    updated.coding_tools_enabled = true;
                }
                "off" | "false" | "disable" | "disabled" | "0" | "no" => {
                    updated.coding_tools_enabled = false;
                }
                _ => return "Usage: /projectconfig coding on|off".to_string(),
            }
        }
        "systemprompt" | "system_prompt" | "prompt" => {
            if value.is_empty() {
                return format!("Current system prompt: {}", project.system_prompt.as_deref().unwrap_or("(none)"));
            }
            if value == "clear" || value == "none" {
                updated.system_prompt = None;
            } else {
                updated.system_prompt = Some(value.to_string());
            }
        }
        _ => return format!("Unknown config key: '{}'\n\nValid keys: model, shell, coding, systemprompt", key),
    }

    match store.update(&updated).await {
        Ok(()) => format!("Updated project '{}': {} = {}", project_name, key, if value.is_empty() { "(show)" } else { value }),
        Err(e) => format!("Failed to update project: {}", e),
    }
}

/// Format a DateTime as a human-readable relative time string
fn format_relative_time(dt: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let diff = now.signed_duration_since(dt);

    if diff.num_seconds() < 60 {
        "just now".to_string()
    } else if diff.num_minutes() < 60 {
        let m = diff.num_minutes();
        format!("{} min ago", m)
    } else if diff.num_hours() < 24 {
        let h = diff.num_hours();
        format!("{} hour{} ago", h, if h == 1 { "" } else { "s" })
    } else if diff.num_days() < 7 {
        let d = diff.num_days();
        if d == 1 {
            "yesterday".to_string()
        } else {
            format!("{} days ago", d)
        }
    } else {
        dt.format("%Y-%m-%d").to_string()
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
    contexts: &Arc<RwLock<HashMap<String, AgentContext>>>,
    event_bus: &broadcast::Sender<BroadcastEvent>,
    sessions: &Arc<SessionManager>,
    session_id: &str,
    project: &str,
    model: &str,
    usage_stats: &Arc<UsageStats>,
    is_project_mention: bool,
    target_project: &str,
) -> anyhow::Result<()> {
    use crate::agent::AgentEvent;

    // Send initial "thinking" message
    let initial_message = OutgoingMessage {
        chat_id: msg.chat_id.clone(),
        content: MessageContent::Text {
            text: "\u{258c}".to_string(),
        },
        reply_to: Some(msg.id.clone()),
        parse_mode: None,
    };

    let message_id = registry.send(&msg.channel, initial_message).await?;

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
                    tracing::debug!(
                        "Tool result: {}",
                        if result.success { "success" } else { "error" }
                    );
                }
                AgentEvent::Error { message } => {
                    tracing::error!("Streaming error: {}", message);
                }
                _ => {}
            }
        })
        .await;

    match result {
        Ok(exec_result) => {
            {
                let mut contexts_write = contexts.write().await;
                contexts_write.insert(context_key.to_string(), context.clone());
            }

            // Prefix response with project name when using @project syntax
            let reply_content = if is_project_mention {
                format!("{}: {}", target_project, exec_result.content)
            } else {
                exec_result.content.clone()
            };

            // Handle long responses: edit first chunk, send rest as new messages
            let chunks = chunk_message(&reply_content, MAX_MESSAGE_LEN);
            let first_chunk = &chunks[0];

            if let Err(e) = registry
                .edit_message(
                    &msg.channel,
                    &msg.chat_id,
                    &message_id,
                    first_chunk,
                )
                .await
            {
                tracing::warn!("Failed to edit final message, sending new: {}", e);
                send_reply(registry, msg, first_chunk).await?;
            }

            // Send remaining chunks as separate messages
            for chunk in chunks.iter().skip(1) {
                let continuation = OutgoingMessage {
                    chat_id: msg.chat_id.clone(),
                    content: MessageContent::Text {
                        text: chunk.to_string(),
                    },
                    reply_to: None,
                    parse_mode: None,
                };
                if let Err(e) = registry.send(&msg.channel, continuation).await {
                    tracing::warn!("Failed to send continuation chunk: {}", e);
                }
            }

            // Persist assistant response to transcript
            let assistant_entry = TranscriptEntry {
                timestamp: chrono::Utc::now(),
                entry_type: TranscriptEntryType::Assistant {
                    content: exec_result.content.clone(),
                    model: model.to_string(),
                },
            };
            if let Err(e) = sessions.append(project, session_id, &assistant_entry).await {
                tracing::error!("Failed to append assistant transcript: {}", e);
            }

            // Update session timestamp
            if let Ok(Some(mut session)) = sessions.load_from_project(project, session_id).await {
                session.updated_at = chrono::Utc::now();
                let _ = sessions.update(&session).await;
            }

            // Broadcast outgoing message
            let _ = event_bus.send(BroadcastEvent::MessageOutgoing {
                channel: msg.channel.clone(),
                chat_id: msg.chat_id.clone(),
                content: exec_result.content.clone(),
                reply_to: Some(msg.id.clone()),
                timestamp: chrono::Utc::now().to_rfc3339(),
                session_id: Some(session_id.to_string()),
            });

            // Track token usage
            usage_stats.record_usage(exec_result.usage.prompt_tokens, exec_result.usage.completion_tokens);

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

            let _ = event_bus.send(BroadcastEvent::MessageError {
                channel: msg.channel.clone(),
                chat_id: msg.chat_id.clone(),
                error: e.to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                session_id: Some(session_id.to_string()),
            });

            Ok(())
        }
    }
}

/// Maximum message length for channels like Telegram (4096 chars).
/// We use a slightly lower limit to leave room for splitting at line boundaries.
const MAX_MESSAGE_LEN: usize = 4000;

/// Send a reply to a channel message, automatically chunking if too long.
async fn send_reply(
    registry: &Arc<ChannelRegistry>,
    original: &IncomingMessage,
    content: &str,
) -> anyhow::Result<()> {
    let chunks = chunk_message(content, MAX_MESSAGE_LEN);

    for (i, chunk) in chunks.iter().enumerate() {
        let reply = OutgoingMessage {
            chat_id: original.chat_id.clone(),
            content: MessageContent::Text {
                text: chunk.to_string(),
            },
            // Only reply_to on the first chunk
            reply_to: if i == 0 {
                Some(original.id.clone())
            } else {
                None
            },
            parse_mode: None,
        };

        registry.send(&original.channel, reply).await?;
    }

    Ok(())
}

/// Split a long message into chunks at line boundaries, respecting max_len.
fn chunk_message(text: &str, max_len: usize) -> Vec<String> {
    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        // If adding this line would exceed the limit, finalize current chunk
        if !current.is_empty() && current.len() + 1 + line.len() > max_len {
            chunks.push(current);
            current = String::new();
        }

        // If a single line exceeds max_len, split it by chars
        if line.len() > max_len {
            if !current.is_empty() {
                chunks.push(current);
                current = String::new();
            }
            let mut remaining = line;
            while remaining.len() > max_len {
                // Find a safe split point (don't break in the middle of a multi-byte char)
                let split_at = remaining
                    .char_indices()
                    .take_while(|(i, _)| *i <= max_len)
                    .last()
                    .map(|(i, c)| i + c.len_utf8())
                    .unwrap_or(max_len);
                chunks.push(remaining[..split_at].to_string());
                remaining = &remaining[split_at..];
            }
            if !remaining.is_empty() {
                current = remaining.to_string();
            }
        } else {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

/// Register bot commands with Telegram so they appear in the / menu
#[cfg(feature = "channel-telegram")]
async fn register_telegram_commands(token: &str) -> anyhow::Result<()> {
    let commands = vec![
        serde_json::json!({"command": "help", "description": "Show available commands"}),
        serde_json::json!({"command": "status", "description": "Show bot status"}),
        serde_json::json!({"command": "projects", "description": "List all projects"}),
        serde_json::json!({"command": "newproject", "description": "Create a new project"}),
        serde_json::json!({"command": "deleteproject", "description": "Delete a project"}),
        serde_json::json!({"command": "project", "description": "Switch to a project"}),
        serde_json::json!({"command": "projectinfo", "description": "Show project details"}),
        serde_json::json!({"command": "projectconfig", "description": "Update project settings"}),
        serde_json::json!({"command": "sessions", "description": "List sessions"}),
        serde_json::json!({"command": "newsession", "description": "Start a new session"}),
        serde_json::json!({"command": "skills", "description": "List active skills"}),
        serde_json::json!({"command": "model", "description": "Show current model"}),
        serde_json::json!({"command": "id", "description": "Show chat and user IDs"}),
        serde_json::json!({"command": "version", "description": "Show bot version"}),
        serde_json::json!({"command": "ping", "description": "Check if bot is alive"}),
    ];

    let client = reqwest::Client::new();
    let url = format!("https://api.telegram.org/bot{}/setMyCommands", token);

    // Register for all chat types: default (private), all groups, and all supergroups
    let scopes = vec![
        serde_json::json!({"type": "default"}),
        serde_json::json!({"type": "all_private_chats"}),
        serde_json::json!({"type": "all_group_chats"}),
    ];

    for scope in &scopes {
        let resp = client
            .post(&url)
            .json(&serde_json::json!({"commands": commands, "scope": scope}))
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!("Failed to set Telegram commands for scope {:?}: {}", scope, body);
        }
    }

    tracing::info!("Registered {} Telegram bot commands for all chat scopes", commands.len());

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

// ============================================================================
// Public API for external consumers (Tauri apps, HTTP clients)
// ============================================================================

/// Chat API request — send a message through the full gateway pipeline.
#[derive(Debug, serde::Deserialize)]
pub struct ApiChatRequest {
    /// Message text to send
    pub message: String,
    /// Optional context key for session tracking (default: "api:default")
    #[serde(default = "default_context_key")]
    pub context_key: String,
    /// Whether to stream the response via SSE (default: false)
    #[serde(default)]
    pub stream: bool,
}

fn default_context_key() -> String {
    "api:default".to_string()
}

/// Chat API response for non-streaming requests.
#[derive(Debug, serde::Serialize)]
pub struct ApiChatResponse {
    /// "command" for slash commands, "agent" for agent responses
    #[serde(rename = "type")]
    pub response_type: String,
    /// The response text content
    pub content: String,
    /// Tool calls made during agent execution (empty for commands)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ApiToolCallInfo>,
    /// Usage stats
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ApiUsageInfo>,
}

#[derive(Debug, serde::Serialize)]
pub struct ApiToolCallInfo {
    pub name: String,
    pub success: bool,
    pub content: String,
}

#[derive(Debug, serde::Serialize)]
pub struct ApiUsageInfo {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub tool_calls: usize,
}

/// Process a chat message through the gateway pipeline (slash commands + agent).
///
/// This is the primary API for external consumers (Tauri desktop app, HTTP clients).
/// Slash commands are handled locally; everything else goes through AgentExecutor.
pub async fn process_api_message(
    state: &AppState,
    request: &ApiChatRequest,
) -> anyhow::Result<ApiChatResponse> {
    let text = request.message.trim();

    // ── Slash commands ───────────────────────────────────────────────
    if text.starts_with('/') {
        let response = handle_api_slash_command(text, state, &request.context_key).await;
        return Ok(ApiChatResponse {
            response_type: "command".to_string(),
            content: response,
            tool_calls: vec![],
            usage: None,
        });
    }

    // ── Agent execution ──────────────────────────────────────────────
    let default_model = state.config
        .agents.defaults.model.clone()
        .unwrap_or_else(|| "anthropic/claude-3-opus".to_string());

    let provider = state.providers.get_for_model(&default_model)
        .ok_or_else(|| anyhow::anyhow!("No provider found for model '{}'", default_model))?;

    // Get or create context for this key
    let memory = state.memory.clone();
    let scope = MemoryScope::agent("api");

    // Build tool registry with coding tools
    let workspace_dir = std::path::PathBuf::from(
        expand_path(&state.config.workspace.base_dir),
    );
    let tools = Arc::new(ToolRegistry::with_coding_tools(
        memory,
        scope,
        workspace_dir,
        state.config.workspace.shell_enabled,
    ));

    let agent_config = AgentConfig {
        id: "api".to_string(),
        name: "API Agent".to_string(),
        system_prompt: None,
        model: default_model.clone(),
        tools: tools.names(),
        ..Default::default()
    };

    let executor = AgentExecutor::new(provider, tools, agent_config);

    let mut context = {
        let mut ctx_map = state.api_contexts.write().await;
        ctx_map.remove(&request.context_key)
            .unwrap_or_else(AgentContext::default)
    };

    let mut tool_call_infos = Vec::new();

    let result = executor.execute_stream(
        &mut context,
        text,
        |event| {
            if let AgentEvent::ToolResult { result } = event {
                tool_call_infos.push(ApiToolCallInfo {
                    name: result.tool_call_id.clone(),
                    success: result.success,
                    content: result.content.clone(),
                });
            }
        },
    ).await?;

    // Store context back for session persistence
    {
        let mut ctx_map = state.api_contexts.write().await;
        ctx_map.insert(request.context_key.clone(), context);
    }

    Ok(ApiChatResponse {
        response_type: "agent".to_string(),
        content: result.content,
        tool_calls: tool_call_infos,
        usage: Some(ApiUsageInfo {
            prompt_tokens: result.usage.prompt_tokens,
            completion_tokens: result.usage.completion_tokens,
            total_tokens: result.usage.total_tokens,
            tool_calls: result.usage.tool_calls,
        }),
    })
}

/// Handle slash commands from the API (no channel registry needed).
async fn handle_api_slash_command(
    text: &str,
    state: &AppState,
    context_key: &str,
) -> String {
    let trimmed = text.trim();
    let (raw_command, _args) = match trimmed.find(' ') {
        Some(pos) => (&trimmed[1..pos], trimmed[pos + 1..].trim()),
        None => (&trimmed[1..], ""),
    };
    let command = raw_command.split('@').next().unwrap_or(raw_command);

    let default_model = state.config
        .agents.defaults.model.clone()
        .unwrap_or_else(|| "anthropic/claude-3-opus".to_string());

    match command {
        "help" | "commands" => generate_help_text(&state.skills).await,
        "skills" => generate_skills_list(&state.skills).await,
        "status" => generate_status_text(
            &state.providers,
            &default_model,
            &state.skills,
            &state.api_contexts,
            &state.sessions,
            &state.active_sessions,
            &state.usage_stats,
            &state.config,
            context_key,
        ).await,
        "model" => format!("Current model: {}", default_model),
        "ping" => "Pong!".to_string(),
        "version" => format!("BXNode Bot v{}", env!("CARGO_PKG_VERSION")),
        _ => format!(
            "Unknown command: /{}\n\nType /help to see available commands.",
            command
        ),
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
