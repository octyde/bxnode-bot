//! BXNode Bot Desktop - Tauri Application
//!
//! Native desktop application for managing BXNode Bot skills and server.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::RwLock;

use bxnode_bot::config::Config;
use bxnode_bot::memory::{MemoryRecord, MemoryScope, MemorySearchResult, MemoryStore};
use bxnode_bot::providers::{CompletionRequest, Message, ProviderRegistry, Role};
use bxnode_bot::skills::{SkillInfo, SkillRegistry, SkillSyncer, SyncReport};

/// Application state shared across Tauri commands
pub struct AppState {
    pub skills: Arc<RwLock<Option<SkillRegistry>>>,
    pub config: Arc<RwLock<Config>>,
    pub config_path: Arc<RwLock<Option<String>>>,
    pub memory: Arc<RwLock<Option<MemoryStore>>>,
    pub providers: Arc<RwLock<Option<ProviderRegistry>>>,
    pub server_process: Arc<RwLock<Option<tokio::process::Child>>>,
    pub server_port: Arc<RwLock<u16>>,
    pub server_log: Arc<RwLock<Vec<String>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            skills: Arc::new(RwLock::new(None)),
            config: Arc::new(RwLock::new(Config::default())),
            config_path: Arc::new(RwLock::new(None)),
            memory: Arc::new(RwLock::new(None)),
            providers: Arc::new(RwLock::new(None)),
            server_process: Arc::new(RwLock::new(None)),
            server_port: Arc::new(RwLock::new(3000)),
            server_log: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

/// Expand ~ to home directory in paths
fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") || path == "~" {
        if let Some(home) = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
        {
            let home = home.to_string_lossy();
            return path.replacen('~', &home, 1);
        }
    }
    path.to_string()
}

/// Skill detail response including instructions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDetail {
    #[serde(flatten)]
    pub info: SkillInfo,
    pub instructions: Option<String>,
}

/// Sync report response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponse {
    pub success: bool,
    pub synced: Vec<String>,
    pub skipped: Vec<String>,
    pub errors: Vec<(String, String)>,
    pub total: usize,
}

impl From<SyncReport> for SyncResponse {
    fn from(report: SyncReport) -> Self {
        let total = report.total();
        Self {
            success: report.is_success(),
            synced: report.synced,
            skipped: report.skipped,
            errors: report.errors,
            total,
        }
    }
}

/// Skills configuration summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsConfigSummary {
    pub enabled: bool,
    pub total_skills: usize,
    pub active_skills: usize,
    pub directories: Vec<String>,
}

/// Stats response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResponse {
    pub version: String,
    pub providers: usize,
    pub skills_enabled: bool,
    pub total_skills: usize,
    pub active_skills: usize,
}

// ============================================================================
// Tauri Commands
// ============================================================================

/// Get application status
#[tauri::command]
fn get_status() -> String {
    serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    })
    .to_string()
}

/// Get application stats
#[tauri::command]
async fn get_stats(state: State<'_, AppState>) -> Result<StatsResponse, String> {
    let skills = state.skills.read().await;

    let (total_skills, active_skills) = if let Some(ref registry) = *skills {
        (registry.count(), registry.active_count())
    } else {
        (0, 0)
    };

    Ok(StatsResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        providers: 0, // TODO: Get from provider registry
        skills_enabled: skills.is_some(),
        total_skills,
        active_skills,
    })
}

/// Initialize the skills system
#[tauri::command]
async fn init_skills(state: State<'_, AppState>) -> Result<SkillsConfigSummary, String> {
    let config = state.config.read().await;

    if !config.skills.enabled {
        return Ok(SkillsConfigSummary {
            enabled: false,
            total_skills: 0,
            active_skills: 0,
            directories: vec![],
        });
    }

    let registry = SkillRegistry::from_config(&config.skills).map_err(|e| e.to_string())?;

    let summary = SkillsConfigSummary {
        enabled: true,
        total_skills: registry.count(),
        active_skills: registry.active_count(),
        directories: registry
            .directories()
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
    };

    let mut skills = state.skills.write().await;
    *skills = Some(registry);

    Ok(summary)
}

/// List all skills
#[tauri::command]
async fn list_skills(state: State<'_, AppState>) -> Result<Vec<SkillInfo>, String> {
    let skills = state.skills.read().await;

    match &*skills {
        Some(registry) => Ok(registry.list_info()),
        None => Err("Skills system not initialized".to_string()),
    }
}

/// Get skill details by name
#[tauri::command]
async fn get_skill(name: String, state: State<'_, AppState>) -> Result<SkillDetail, String> {
    let skills = state.skills.read().await;

    match &*skills {
        Some(registry) => match registry.get(&name) {
            Some(skill_ref) => {
                let mut info = SkillInfo::from(skill_ref);
                info.is_active = registry.is_active(&name);

                Ok(SkillDetail {
                    info,
                    instructions: skill_ref.instructions().map(|s| s.to_string()),
                })
            }
            None => Err(format!("Skill '{}' not found", name)),
        },
        None => Err("Skills system not initialized".to_string()),
    }
}

/// Enable a skill
#[tauri::command]
async fn enable_skill(name: String, state: State<'_, AppState>) -> Result<String, String> {
    let mut skills = state.skills.write().await;

    match &mut *skills {
        Some(registry) => {
            registry.enable(&name).map_err(|e| e.to_string())?;
            Ok(format!("Skill '{}' enabled", name))
        }
        None => Err("Skills system not initialized".to_string()),
    }
}

/// Disable a skill
#[tauri::command]
async fn disable_skill(name: String, state: State<'_, AppState>) -> Result<String, String> {
    let mut skills = state.skills.write().await;

    match &mut *skills {
        Some(registry) => {
            registry.disable(&name);
            Ok(format!("Skill '{}' disabled", name))
        }
        None => Err("Skills system not initialized".to_string()),
    }
}

/// Sync skills from awesome-openclaw-skills
#[tauri::command]
async fn sync_skills(state: State<'_, AppState>) -> Result<SyncResponse, String> {
    let skills = state.skills.read().await;

    let directories = match &*skills {
        Some(registry) => registry.directories().to_vec(),
        None => return Err("Skills system not initialized".to_string()),
    };

    if directories.is_empty() {
        return Err("No skill directories configured".to_string());
    }

    // Use the first directory as target
    let target_dir = directories[0].clone();
    drop(skills); // Release read lock

    // Ensure the target directory exists
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| format!("Failed to create skills directory: {}", e))?;

    let syncer = SkillSyncer::new(target_dir);

    let report = syncer
        .sync_from_awesome_list(false)
        .await
        .map_err(|e| e.to_string())?;

    // Rescan skills after sync
    let mut skills = state.skills.write().await;
    if let Some(ref mut registry) = *skills {
        let _ = registry.scan_all();
    }

    Ok(SyncResponse::from(report))
}

/// Get skills configuration summary
#[tauri::command]
async fn get_skills_config(state: State<'_, AppState>) -> Result<SkillsConfigSummary, String> {
    let skills = state.skills.read().await;

    match &*skills {
        Some(registry) => Ok(SkillsConfigSummary {
            enabled: true,
            total_skills: registry.count(),
            active_skills: registry.active_count(),
            directories: registry
                .directories()
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
        }),
        None => Ok(SkillsConfigSummary {
            enabled: false,
            total_skills: 0,
            active_skills: 0,
            directories: vec![],
        }),
    }
}

/// Refresh/rescan skills
#[tauri::command]
async fn refresh_skills(state: State<'_, AppState>) -> Result<usize, String> {
    let mut skills = state.skills.write().await;

    match &mut *skills {
        Some(registry) => registry.rescan().map_err(|e| e.to_string()),
        None => Err("Skills system not initialized".to_string()),
    }
}

/// Resolve config path — tries the given path, then falls back to
/// `src-tauri/config.yaml` and `~/.bxnode-bot/config.yaml`.
fn resolve_config_path(hint: Option<&str>) -> Option<String> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();

    let candidates: Vec<std::path::PathBuf> = if let Some(p) = hint {
        let pb = std::path::PathBuf::from(p);
        if pb.is_absolute() {
            vec![pb]
        } else {
            // Prefer src-tauri/ (desktop app saves here), then CWD, then home
            vec![
                std::path::PathBuf::from("src-tauri").join(&pb),
                pb.clone(),
                std::path::PathBuf::from(&home)
                    .join(".bxnode-bot")
                    .join(&pb),
            ]
        }
    } else {
        vec![
            std::path::PathBuf::from("src-tauri/config.yaml"),
            std::path::PathBuf::from("config.yaml"),
            std::path::PathBuf::from(&home).join(".bxnode-bot/config.yaml"),
        ]
    };

    for c in &candidates {
        if c.exists() {
            eprintln!("[config] Resolved config path: {}", c.display());
            return Some(c.to_string_lossy().to_string());
        }
    }

    hint.map(|s| s.to_string())
}

/// Load configuration from file
#[tauri::command]
async fn load_config(path: Option<String>, state: State<'_, AppState>) -> Result<String, String> {
    let resolved = resolve_config_path(path.as_deref());
    let config = Config::load_or_default(resolved.as_ref());

    // Save the resolved config path for later persistence
    let mut config_path_lock = state.config_path.write().await;
    *config_path_lock = resolved.clone();
    drop(config_path_lock);

    // Initialize provider registry
    let registry = ProviderRegistry::from_config(&config);
    let mut providers = state.providers.write().await;
    *providers = Some(registry);
    drop(providers);

    // Initialize memory store if enabled
    if config.memory.enabled {
        let store_path = expand_tilde(&config.memory.store_path);
        if let Ok(store) = MemoryStore::open(&store_path) {
            let mut mem = state.memory.write().await;
            *mem = Some(store);
        }
    }

    let mut state_config = state.config.write().await;
    *state_config = config;
    Ok("Configuration loaded".to_string())
}

// ============================================================================
// Provider Commands
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub configured: bool,
    pub has_api_key: bool,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
}

#[tauri::command]
async fn list_providers(state: State<'_, AppState>) -> Result<Vec<ProviderInfo>, String> {
    let config = state.config.read().await;
    let mut providers = Vec::new();

    // Helper to build ProviderInfo for generic providers
    macro_rules! generic_provider {
        ($id:expr, $name:expr, $cfg:expr) => {
            ProviderInfo {
                id: $id.to_string(),
                name: $name.to_string(),
                configured: $cfg.is_some(),
                has_api_key: $cfg.is_some(),
                base_url: $cfg.as_ref().and_then(|p| p.base_url.clone()),
                default_model: $cfg.as_ref().and_then(|p| p.default_model.clone()),
            }
        };
    }

    providers.push(ProviderInfo {
        id: "anthropic".to_string(),
        name: "Anthropic (Claude)".to_string(),
        configured: config.providers.anthropic.is_some(),
        has_api_key: config.providers.anthropic.is_some(),
        base_url: config.providers.anthropic.as_ref().and_then(|p| p.base_url.clone()),
        default_model: config.providers.anthropic.as_ref().and_then(|p| p.default_model.clone()),
    });

    providers.push(ProviderInfo {
        id: "openai".to_string(),
        name: "OpenAI (GPT)".to_string(),
        configured: config.providers.openai.is_some(),
        has_api_key: config.providers.openai.is_some(),
        base_url: config.providers.openai.as_ref().and_then(|p| p.base_url.clone()),
        default_model: config.providers.openai.as_ref().and_then(|p| p.default_model.clone()),
    });

    providers.push(ProviderInfo {
        id: "ollama".to_string(),
        name: "Ollama (Local)".to_string(),
        configured: config.providers.ollama.is_some(),
        has_api_key: false,
        base_url: config.providers.ollama.as_ref().map(|p| p.base_url.clone()),
        default_model: config.providers.ollama.as_ref().and_then(|p| p.default_model.clone()),
    });

    providers.push(generic_provider!("zai", "Z.AI (GLM)", config.providers.zai));
    providers.push(generic_provider!("groq", "Groq", config.providers.groq));
    providers.push(generic_provider!("deepseek", "DeepSeek", config.providers.deepseek));
    providers.push(generic_provider!("mistral", "Mistral", config.providers.mistral));
    providers.push(generic_provider!("venice", "Venice.ai", config.providers.venice));
    providers.push(generic_provider!("qwen", "Qwen (DashScope)", config.providers.qwen));
    providers.push(generic_provider!("gemini", "Google Gemini", config.providers.gemini));

    Ok(providers)
}

#[tauri::command]
async fn get_provider_config(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let config = state.config.read().await;
    Ok(serde_json::json!({
        "anthropic": { "configured": config.providers.anthropic.is_some() },
        "openai": { "configured": config.providers.openai.is_some() },
        "ollama": {
            "configured": config.providers.ollama.is_some(),
            "base_url": config.providers.ollama.as_ref().map(|p| &p.base_url),
        },
        "zai": { "configured": config.providers.zai.is_some() },
        "groq": { "configured": config.providers.groq.is_some() },
        "deepseek": { "configured": config.providers.deepseek.is_some() },
        "mistral": { "configured": config.providers.mistral.is_some() },
        "venice": { "configured": config.providers.venice.is_some() },
        "qwen": { "configured": config.providers.qwen.is_some() },
        "gemini": { "configured": config.providers.gemini.is_some() },
    }))
}

async fn save_config_to_disk(state: &State<'_, AppState>) -> Result<(), String> {
    let path = {
        let config_path_lock = state.config_path.read().await;
        config_path_lock.clone()
    };

    if let Some(path) = path {
        let config = {
            let config_guard = state.config.read().await;
            config_guard.clone()
        };
        config
            .save(&path)
            .map_err(|e| format!("Failed to save config to {}: {}", path, e))?;
    }
    Ok(())
}

#[tauri::command]
async fn update_provider_config(
    provider_id: String,
    api_key: Option<String>,
    base_url: Option<String>,
    organization: Option<String>,
    default_model: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let mut config = state.config.write().await;

    // Normalize empty string to None
    let default_model = default_model.filter(|s| !s.is_empty());

    // If the API key is the masked placeholder, preserve the existing key
    const MASKED_KEY: &str = "••••••••";
    let is_masked = api_key.as_deref() == Some(MASKED_KEY);

    match provider_id.as_str() {
        "anthropic" => {
            if let Some(key) = api_key {
                let real_key = if is_masked {
                    config.providers.anthropic.as_ref().map(|p| p.api_key.clone()).unwrap_or_default()
                } else {
                    key
                };
                config.providers.anthropic = Some(bxnode_bot::config::AnthropicConfig {
                    api_key: real_key,
                    base_url,
                    default_model,
                });
            } else if api_key.is_none() && base_url.is_none() {
                config.providers.anthropic = None;
            }
        }
        "openai" => {
            if let Some(key) = api_key {
                let real_key = if is_masked {
                    config.providers.openai.as_ref().map(|p| p.api_key.clone()).unwrap_or_default()
                } else {
                    key
                };
                config.providers.openai = Some(bxnode_bot::config::OpenAIConfig {
                    api_key: real_key,
                    base_url,
                    organization,
                    default_model,
                });
            } else if api_key.is_none() && base_url.is_none() {
                config.providers.openai = None;
            }
        }
        "ollama" => {
            if let Some(url) = base_url {
                config.providers.ollama = Some(bxnode_bot::config::OllamaConfig {
                    base_url: url,
                    default_model,
                });
            } else {
                config.providers.ollama = None;
            }
        }
        "zai" | "groq" | "deepseek" | "mistral" | "venice" | "qwen" | "gemini" => {
            if let Some(key) = api_key {
                let existing_key = if is_masked {
                    match provider_id.as_str() {
                        "zai" => config.providers.zai.as_ref().map(|p| p.api_key.clone()),
                        "groq" => config.providers.groq.as_ref().map(|p| p.api_key.clone()),
                        "deepseek" => config.providers.deepseek.as_ref().map(|p| p.api_key.clone()),
                        "mistral" => config.providers.mistral.as_ref().map(|p| p.api_key.clone()),
                        "venice" => config.providers.venice.as_ref().map(|p| p.api_key.clone()),
                        "qwen" => config.providers.qwen.as_ref().map(|p| p.api_key.clone()),
                        "gemini" => config.providers.gemini.as_ref().map(|p| p.api_key.clone()),
                        _ => None,
                    }
                } else {
                    None
                };
                let real_key = existing_key.unwrap_or(key);
                let provider_config = bxnode_bot::config::GenericProviderConfig {
                    api_key: real_key,
                    base_url,
                    default_model,
                };
                match provider_id.as_str() {
                    "zai" => config.providers.zai = Some(provider_config),
                    "groq" => config.providers.groq = Some(provider_config),
                    "deepseek" => config.providers.deepseek = Some(provider_config),
                    "mistral" => config.providers.mistral = Some(provider_config),
                    "venice" => config.providers.venice = Some(provider_config),
                    "qwen" => config.providers.qwen = Some(provider_config),
                    "gemini" => config.providers.gemini = Some(provider_config),
                    _ => {}
                }
            } else if api_key.is_none() && base_url.is_none() {
                match provider_id.as_str() {
                    "zai" => config.providers.zai = None,
                    "groq" => config.providers.groq = None,
                    "deepseek" => config.providers.deepseek = None,
                    "mistral" => config.providers.mistral = None,
                    "venice" => config.providers.venice = None,
                    "qwen" => config.providers.qwen = None,
                    "gemini" => config.providers.gemini = None,
                    _ => {}
                }
            }
        }
        _ => return Err(format!("Unknown provider: {}", provider_id)),
    }

    drop(config);

    // Save to disk
    save_config_to_disk(&state).await?;

    // Refresh provider registry
    let config = state.config.read().await;
    let registry = ProviderRegistry::from_config(&config);
    drop(config);
    let mut providers = state.providers.write().await;
    *providers = Some(registry);

    Ok(format!("Provider {} configuration updated", provider_id))
}

#[tauri::command]
async fn remove_provider_config(
    provider_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let mut config = state.config.write().await;

    match provider_id.as_str() {
        "anthropic" => config.providers.anthropic = None,
        "openai" => config.providers.openai = None,
        "ollama" => config.providers.ollama = None,
        "zai" => config.providers.zai = None,
        "groq" => config.providers.groq = None,
        "deepseek" => config.providers.deepseek = None,
        "mistral" => config.providers.mistral = None,
        "venice" => config.providers.venice = None,
        "qwen" => config.providers.qwen = None,
        "gemini" => config.providers.gemini = None,
        _ => return Err(format!("Unknown provider: {}", provider_id)),
    }

    drop(config);

    // Save to disk
    save_config_to_disk(&state).await?;

    // Refresh provider registry
    let config = state.config.read().await;
    let registry = ProviderRegistry::from_config(&config);
    drop(config);
    let mut providers = state.providers.write().await;
    *providers = Some(registry);

    Ok(format!("Provider {} removed", provider_id))
}

// ============================================================================
// Channel Commands
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelInfo {
    pub id: String,
    pub name: String,
    pub configured: bool,
    pub status: String,
}

#[tauri::command]
async fn list_channels(state: State<'_, AppState>) -> Result<Vec<ChannelInfo>, String> {
    let config = state.config.read().await;

    let channels = vec![
        ChannelInfo {
            id: "telegram".to_string(),
            name: "Telegram".to_string(),
            configured: config.channels.telegram.is_some(),
            status: if config.channels.telegram.is_some() {
                "stopped"
            } else {
                "not_configured"
            }
            .to_string(),
        },
        ChannelInfo {
            id: "discord".to_string(),
            name: "Discord".to_string(),
            configured: config.channels.discord.is_some(),
            status: if config.channels.discord.is_some() {
                "stopped"
            } else {
                "not_configured"
            }
            .to_string(),
        },
        ChannelInfo {
            id: "slack".to_string(),
            name: "Slack".to_string(),
            configured: config.channels.slack.is_some(),
            status: if config.channels.slack.is_some() {
                "stopped"
            } else {
                "not_configured"
            }
            .to_string(),
        },
        ChannelInfo {
            id: "line".to_string(),
            name: "LINE".to_string(),
            configured: config.channels.line.is_some(),
            status: if config.channels.line.is_some() {
                "stopped"
            } else {
                "not_configured"
            }
            .to_string(),
        },
        ChannelInfo {
            id: "signal".to_string(),
            name: "Signal".to_string(),
            configured: config.channels.signal.is_some(),
            status: if config.channels.signal.is_some() {
                "stopped"
            } else {
                "not_configured"
            }
            .to_string(),
        },
        ChannelInfo {
            id: "feishu".to_string(),
            name: "Feishu".to_string(),
            configured: config.channels.feishu.is_some(),
            status: if config.channels.feishu.is_some() {
                "stopped"
            } else {
                "not_configured"
            }
            .to_string(),
        },
    ];

    Ok(channels)
}

#[tauri::command]
async fn get_channel_status(channel_id: String) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "id": channel_id,
        "status": "stopped",
    }))
}

#[tauri::command]
async fn update_channel_config(
    channel_id: String,
    fields: serde_json::Value,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let mut config = state.config.write().await;

    match channel_id.as_str() {
        "telegram" => {
            let token = fields.get("token").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if token.is_empty() {
                return Err("Token is required for Telegram".to_string());
            }
            let allowed_users: Vec<i64> = fields
                .get("allowed_users")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            let approval_required = fields
                .get("approval_required")
                .and_then(|v| v.as_str())
                .map(|s| s == "true")
                .unwrap_or(false);
            config.channels.telegram = Some(bxnode_bot::config::TelegramConfig {
                token,
                allowed_users,
                approval_required,
            });
        }
        "discord" => {
            let token = fields.get("token").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if token.is_empty() {
                return Err("Token is required for Discord".to_string());
            }
            let allowed_guilds: Vec<u64> = fields
                .get("allowed_guilds")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            config.channels.discord = Some(bxnode_bot::config::DiscordConfig {
                token,
                allowed_guilds,
            });
        }
        "slack" => {
            let bot_token = fields.get("bot_token").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let app_token = fields.get("app_token").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if bot_token.is_empty() || app_token.is_empty() {
                return Err("Both Bot Token and App Token are required for Slack".to_string());
            }
            config.channels.slack = Some(bxnode_bot::config::SlackConfig {
                bot_token,
                app_token,
            });
        }
        "line" => {
            let channel_access_token = fields.get("channel_access_token").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let channel_secret = fields.get("channel_secret").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if channel_access_token.is_empty() || channel_secret.is_empty() {
                return Err("Both Channel Access Token and Channel Secret are required for LINE".to_string());
            }
            let allowed_users: Vec<String> = fields
                .get("allowed_users")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            config.channels.line = Some(bxnode_bot::config::LineConfig {
                channel_access_token,
                channel_secret,
                allowed_users,
            });
        }
        "signal" => {
            let phone_number = fields.get("phone_number").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if phone_number.is_empty() {
                return Err("Phone number is required for Signal".to_string());
            }
            let api_url = fields
                .get("api_url")
                .and_then(|v| v.as_str())
                .unwrap_or("http://localhost:8080")
                .to_string();
            let allowed_numbers: Vec<String> = fields
                .get("allowed_numbers")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            config.channels.signal = Some(bxnode_bot::config::SignalConfig {
                api_url,
                phone_number,
                allowed_numbers,
            });
        }
        "feishu" => {
            let app_id = fields.get("app_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let app_secret = fields.get("app_secret").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let verification_token = fields.get("verification_token").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if app_id.is_empty() || app_secret.is_empty() || verification_token.is_empty() {
                return Err("App ID, App Secret, and Verification Token are required for Feishu".to_string());
            }
            let allowed_users: Vec<String> = fields
                .get("allowed_users")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            config.channels.feishu = Some(bxnode_bot::config::FeishuConfig {
                app_id,
                app_secret,
                verification_token,
                allowed_users,
            });
        }
        _ => return Err(format!("Unknown channel: {}", channel_id)),
    }

    drop(config);
    save_config_to_disk(&state).await?;
    Ok(format!("Channel {} configuration updated", channel_id))
}

#[tauri::command]
async fn remove_channel_config(
    channel_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let mut config = state.config.write().await;

    match channel_id.as_str() {
        "telegram" => config.channels.telegram = None,
        "discord" => config.channels.discord = None,
        "slack" => config.channels.slack = None,
        "line" => config.channels.line = None,
        "signal" => config.channels.signal = None,
        "feishu" => config.channels.feishu = None,
        _ => return Err(format!("Unknown channel: {}", channel_id)),
    }

    drop(config);
    save_config_to_disk(&state).await?;
    Ok(format!("Channel {} removed", channel_id))
}

#[tauri::command]
async fn get_channel_config(
    channel_id: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let config = state.config.read().await;

    let result = match channel_id.as_str() {
        "telegram" => {
            if let Some(ref c) = config.channels.telegram {
                serde_json::json!({
                    "token": c.token,
                    "allowed_users": c.allowed_users.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(", "),
                    "approval_required": if c.approval_required { "true" } else { "false" },
                })
            } else {
                serde_json::json!({})
            }
        }
        "discord" => {
            if let Some(ref c) = config.channels.discord {
                serde_json::json!({
                    "token": c.token,
                    "allowed_guilds": c.allowed_guilds.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(", "),
                })
            } else {
                serde_json::json!({})
            }
        }
        "slack" => {
            if let Some(ref c) = config.channels.slack {
                serde_json::json!({
                    "bot_token": c.bot_token,
                    "app_token": c.app_token,
                })
            } else {
                serde_json::json!({})
            }
        }
        "line" => {
            if let Some(ref c) = config.channels.line {
                serde_json::json!({
                    "channel_access_token": c.channel_access_token,
                    "channel_secret": c.channel_secret,
                    "allowed_users": c.allowed_users.join(", "),
                })
            } else {
                serde_json::json!({})
            }
        }
        "signal" => {
            if let Some(ref c) = config.channels.signal {
                serde_json::json!({
                    "api_url": c.api_url,
                    "phone_number": c.phone_number,
                    "allowed_numbers": c.allowed_numbers.join(", "),
                })
            } else {
                serde_json::json!({})
            }
        }
        "feishu" => {
            if let Some(ref c) = config.channels.feishu {
                serde_json::json!({
                    "app_id": c.app_id,
                    "app_secret": c.app_secret,
                    "verification_token": c.verification_token,
                    "allowed_users": c.allowed_users.join(", "),
                })
            } else {
                serde_json::json!({})
            }
        }
        _ => return Err(format!("Unknown channel: {}", channel_id)),
    };

    Ok(result)
}

#[tauri::command]
async fn test_channel_connection(
    channel_id: String,
    fields: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    match channel_id.as_str() {
        "telegram" => {
            let token = fields
                .get("token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if token.is_empty() {
                return Ok(serde_json::json!({
                    "success": false,
                    "message": "Bot token is required"
                }));
            }
            let url = format!("https://api.telegram.org/bot{}/getMe", token);
            match client.get(&url).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let body: serde_json::Value =
                            resp.json().await.map_err(|e| e.to_string())?;
                        let result = &body["result"];
                        let first_name = result["first_name"].as_str().unwrap_or("Unknown");
                        let username = result["username"].as_str().unwrap_or("");
                        let display = if username.is_empty() {
                            first_name.to_string()
                        } else {
                            format!("{} (@{})", first_name, username)
                        };
                        Ok(serde_json::json!({
                            "success": true,
                            "name": first_name,
                            "username": username,
                            "message": format!("Connected as {}", display)
                        }))
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        let msg = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body)
                        {
                            json["description"]
                                .as_str()
                                .unwrap_or(&format!("HTTP {}", status))
                                .to_string()
                        } else {
                            format!("HTTP {}", status)
                        };
                        Ok(serde_json::json!({
                            "success": false,
                            "message": msg
                        }))
                    }
                }
                Err(e) => Ok(serde_json::json!({
                    "success": false,
                    "message": format!("Connection failed: {}", e)
                })),
            }
        }
        "discord" => {
            let token = fields
                .get("token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if token.is_empty() {
                return Ok(serde_json::json!({
                    "success": false,
                    "message": "Bot token is required"
                }));
            }
            let url = "https://discord.com/api/v10/users/@me";
            match client
                .get(url)
                .header("Authorization", format!("Bot {}", token))
                .send()
                .await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let body: serde_json::Value =
                            resp.json().await.map_err(|e| e.to_string())?;
                        let username = body["username"].as_str().unwrap_or("Unknown");
                        let global_name = body["global_name"].as_str().unwrap_or(username);
                        Ok(serde_json::json!({
                            "success": true,
                            "name": global_name,
                            "username": username,
                            "message": format!("Connected as {} ({})", global_name, username)
                        }))
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        let msg = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body)
                        {
                            json["message"]
                                .as_str()
                                .unwrap_or(&format!("HTTP {}", status))
                                .to_string()
                        } else {
                            format!("HTTP {}", status)
                        };
                        Ok(serde_json::json!({
                            "success": false,
                            "message": msg
                        }))
                    }
                }
                Err(e) => Ok(serde_json::json!({
                    "success": false,
                    "message": format!("Connection failed: {}", e)
                })),
            }
        }
        "slack" => {
            let bot_token = fields
                .get("bot_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if bot_token.is_empty() {
                return Ok(serde_json::json!({
                    "success": false,
                    "message": "Bot token is required"
                }));
            }
            let url = "https://slack.com/api/auth.test";
            match client
                .post(url)
                .header("Authorization", format!("Bearer {}", bot_token))
                .send()
                .await
            {
                Ok(resp) => {
                    let body: serde_json::Value =
                        resp.json().await.map_err(|e| e.to_string())?;
                    if body["ok"].as_bool().unwrap_or(false) {
                        let user = body["user"].as_str().unwrap_or("Unknown");
                        let team = body["team"].as_str().unwrap_or("Unknown");
                        Ok(serde_json::json!({
                            "success": true,
                            "name": user,
                            "username": team,
                            "message": format!("Connected as {} in workspace {}", user, team)
                        }))
                    } else {
                        let error = body["error"].as_str().unwrap_or("Unknown error");
                        Ok(serde_json::json!({
                            "success": false,
                            "message": format!("Slack API error: {}", error)
                        }))
                    }
                }
                Err(e) => Ok(serde_json::json!({
                    "success": false,
                    "message": format!("Connection failed: {}", e)
                })),
            }
        }
        "line" => {
            let token = fields
                .get("channel_access_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if token.is_empty() {
                return Ok(serde_json::json!({
                    "success": false,
                    "message": "Channel access token is required"
                }));
            }
            let url = "https://api.line.me/v2/bot/info";
            match client
                .get(url)
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let body: serde_json::Value =
                            resp.json().await.map_err(|e| e.to_string())?;
                        let display_name = body["displayName"].as_str().unwrap_or("Unknown");
                        Ok(serde_json::json!({
                            "success": true,
                            "name": display_name,
                            "message": format!("Connected as {}", display_name)
                        }))
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        let msg = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body)
                        {
                            json["message"]
                                .as_str()
                                .unwrap_or(&format!("HTTP {}", status))
                                .to_string()
                        } else {
                            format!("HTTP {}", status)
                        };
                        Ok(serde_json::json!({
                            "success": false,
                            "message": msg
                        }))
                    }
                }
                Err(e) => Ok(serde_json::json!({
                    "success": false,
                    "message": format!("Connection failed: {}", e)
                })),
            }
        }
        "signal" => {
            let api_url = fields
                .get("api_url")
                .and_then(|v| v.as_str())
                .unwrap_or("http://localhost:8080")
                .to_string();
            let api_url = if api_url.is_empty() {
                "http://localhost:8080".to_string()
            } else {
                api_url
            };
            let url = format!("{}/v1/about", api_url.trim_end_matches('/'));
            match client.get(&url).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        Ok(serde_json::json!({
                            "success": true,
                            "name": "Signal CLI",
                            "message": format!("Signal CLI API reachable at {}", api_url)
                        }))
                    } else {
                        Ok(serde_json::json!({
                            "success": false,
                            "message": format!("Signal CLI API returned HTTP {}", resp.status())
                        }))
                    }
                }
                Err(e) => Ok(serde_json::json!({
                    "success": false,
                    "message": format!("Cannot reach Signal CLI API at {}: {}", api_url, e)
                })),
            }
        }
        "feishu" => {
            let app_id = fields
                .get("app_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let app_secret = fields
                .get("app_secret")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if app_id.is_empty() || app_secret.is_empty() {
                return Ok(serde_json::json!({
                    "success": false,
                    "message": "App ID and App Secret are required"
                }));
            }
            let url =
                "https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal";
            match client
                .post(url)
                .json(&serde_json::json!({
                    "app_id": app_id,
                    "app_secret": app_secret,
                }))
                .send()
                .await
            {
                Ok(resp) => {
                    let body: serde_json::Value =
                        resp.json().await.map_err(|e| e.to_string())?;
                    let code = body["code"].as_i64().unwrap_or(-1);
                    if code == 0 {
                        Ok(serde_json::json!({
                            "success": true,
                            "name": "Feishu App",
                            "message": "Feishu app credentials verified successfully"
                        }))
                    } else {
                        let msg = body["msg"].as_str().unwrap_or("Unknown error");
                        Ok(serde_json::json!({
                            "success": false,
                            "message": format!("Feishu API error: {}", msg)
                        }))
                    }
                }
                Err(e) => Ok(serde_json::json!({
                    "success": false,
                    "message": format!("Connection failed: {}", e)
                })),
            }
        }
        _ => Ok(serde_json::json!({
            "success": false,
            "message": format!("Test connection not supported for channel: {}", channel_id)
        })),
    }
}

// ============================================================================
// Cron Commands
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJobInfo {
    pub id: String,
    pub schedule: String,
    pub payload: serde_json::Value,
    pub enabled: bool,
    pub description: Option<String>,
}

#[tauri::command]
async fn list_cron_jobs(state: State<'_, AppState>) -> Result<Vec<CronJobInfo>, String> {
    let config = state.config.read().await;
    let jobs: Vec<CronJobInfo> = config
        .cron
        .jobs
        .iter()
        .map(|j| CronJobInfo {
            id: j.id.clone(),
            schedule: j.schedule.clone(),
            payload: j.payload.clone(),
            enabled: j.enabled,
            description: j.description.clone(),
        })
        .collect();
    Ok(jobs)
}

#[tauri::command]
async fn add_cron_job(
    _id: String,
    _schedule: String,
    _payload: serde_json::Value,
    _description: Option<String>,
) -> Result<String, String> {
    Err("Cron job management requires the server to be running".to_string())
}

#[tauri::command]
async fn remove_cron_job(_id: String) -> Result<bool, String> {
    Err("Cron job management requires the server to be running".to_string())
}

#[tauri::command]
async fn run_cron_job(_id: String) -> Result<String, String> {
    Err("Cron job execution requires the server to be running".to_string())
}

// ============================================================================
// Memory Commands
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_records: usize,
    pub total_size_bytes: u64,
}

#[tauri::command]
async fn list_memories(
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<MemoryRecord>, String> {
    let mem = state.memory.read().await;
    match &*mem {
        Some(store) => {
            let scope = MemoryScope::default();
            let mut records: Vec<MemoryRecord> =
                store.list(&scope).into_iter().cloned().collect();
            // Sort by created_at descending (newest first)
            records.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            // Apply limit
            let limit = limit.unwrap_or(50);
            records.truncate(limit);
            Ok(records)
        }
        None => Ok(Vec::new()),
    }
}

#[tauri::command]
async fn search_memories(
    query: String,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<MemorySearchResult>, String> {
    let mem = state.memory.read().await;
    match &*mem {
        Some(store) => {
            let scope = MemoryScope::default();
            let results = store.search(&query, &scope, limit.unwrap_or(10));
            Ok(results)
        }
        None => Ok(Vec::new()),
    }
}

#[tauri::command]
async fn delete_memory(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let mut mem = state.memory.write().await;
    match &mut *mem {
        Some(store) => {
            store.delete(&id).map_err(|e| e.to_string())?;
            Ok(true)
        }
        None => Err("Memory system not initialized".to_string()),
    }
}

#[tauri::command]
async fn memory_stats(state: State<'_, AppState>) -> Result<MemoryStats, String> {
    let mem = state.memory.read().await;
    match &*mem {
        Some(store) => {
            let size = store
                .file_path()
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|m| m.len())
                .unwrap_or(0);
            Ok(MemoryStats {
                total_records: store.len(),
                total_size_bytes: size,
            })
        }
        None => Ok(MemoryStats {
            total_records: 0,
            total_size_bytes: 0,
        }),
    }
}

#[tauri::command]
async fn compact_memory(state: State<'_, AppState>) -> Result<String, String> {
    let mut mem = state.memory.write().await;
    match &mut *mem {
        Some(store) => {
            let removed = store.compact().map_err(|e| e.to_string())?;
            Ok(format!("Compacted: removed {} records", removed))
        }
        None => Err("Memory system not initialized".to_string()),
    }
}

// ============================================================================
// Config Commands
// ============================================================================

#[tauri::command]
async fn get_config(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let config = state.config.read().await;
    Ok(serde_json::json!({
        "server": {
            "host": config.server.host,
            "port": config.server.port,
            "cors": config.server.cors,
        },
        "skills": {
            "enabled": config.skills.enabled,
            "directories": config.skills.directories,
        },
        "cron": {
            "enabled": config.cron.enabled,
            "store_path": config.cron.store_path,
        },
        "memory": {
            "enabled": config.memory.enabled,
            "store_path": config.memory.store_path,
            "max_results": config.memory.max_results,
            "ttl_days": config.memory.ttl_days,
        },
    }))
}

// ============================================================================
// Chat Commands
// ============================================================================

#[tauri::command]
async fn send_message(
    message: String,
    model: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Get provider registry
    let providers = state.providers.read().await;
    let registry = providers
        .as_ref()
        .ok_or_else(|| "Provider registry not initialized. Please load a configuration.".to_string())?;

    // Get provider for the model
    let provider = registry
        .get_for_model(&model)
        .ok_or_else(|| format!("No provider found for model: {}", model))?;

    // Extract the actual model name (remove provider prefix if present)
    let model_name = registry.extract_model_name(&model);

    // Create completion request
    let request = CompletionRequest {
        model: model_name,
        messages: vec![Message {
            role: Role::User,
            content: message,
        }],
        temperature: Some(0.7),
        max_tokens: Some(4096),
        stop: Vec::new(),
        stream: false,
        tools: vec![],
    };

    // Call the provider
    let response = provider
        .complete(request)
        .await
        .map_err(|e| format!("Failed to complete request: {}", e))?;

    Ok(response.content)
}

#[tauri::command]
async fn list_models(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let providers = state.providers.read().await;

    if let Some(registry) = providers.as_ref() {
        // Get all models with provider prefix
        let prefixed_models = registry.all_models_prefixed();
        Ok(prefixed_models.into_iter().map(|m| m.id).collect())
    } else {
        // Fallback if registry not initialized
        Ok(Vec::new())
    }
}

// ============================================================================
// Server Management Commands
// ============================================================================

/// Find the bxnode-bot binary
fn find_server_binary() -> Result<std::path::PathBuf, String> {
    // Look relative to the current executable (for packaged app)
    if let Ok(exe) = std::env::current_exe() {
        let exe_dir = exe.parent().unwrap_or(std::path::Path::new("."));
        let candidate = exe_dir.join("bxnode-bot");
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    // Look in workspace target directories
    let cwd = std::env::current_dir().unwrap_or_default();

    // When running via `cargo tauri dev`, cwd is typically the workspace root or src-tauri
    for base in [cwd.as_path(), cwd.parent().unwrap_or(cwd.as_path())] {
        for profile in ["release", "debug"] {
            let candidate = base.join("target").join(profile).join("bxnode-bot");
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    Err(
        "Server binary not found. Please build it first:\n  cargo build --features full"
            .to_string(),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStatus {
    pub running: bool,
    pub port: u16,
    pub url: Option<String>,
    pub log: Vec<String>,
    pub error: Option<String>,
}

#[tauri::command]
async fn start_server(state: State<'_, AppState>) -> Result<ServerStatus, String> {
    // Check if already running
    {
        let proc = state.server_process.read().await;
        if proc.is_some() {
            let port = *state.server_port.read().await;
            return Ok(ServerStatus {
                running: true,
                port,
                url: Some(format!("http://localhost:{}", port)),
                log: vec![],
                error: None,
            });
        }
    }

    let binary = find_server_binary()?;

    // Read config for port
    let port = {
        let config = state.config.read().await;
        config.server.port
    };

    // Check if port is available before spawning
    match tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await {
        Ok(_listener) => {
            // Port is free — drop the listener so the server can bind to it
        }
        Err(_) => {
            return Err(format!(
                "Port {} is already in use. Another server may be running.\nStop it first or change the port in config.",
                port
            ));
        }
    }

    // Get config path
    let config_path = {
        let cp = state.config_path.read().await;
        cp.clone()
    };

    // Build command
    let mut cmd = tokio::process::Command::new(&binary);
    cmd.arg("serve");
    cmd.arg("--port").arg(port.to_string());
    if let Some(ref path) = config_path {
        cmd.arg("--config").arg(path);
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    // Spawn
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start server: {}", e))?;

    // Capture stderr in a background task
    let log = state.server_log.clone();
    {
        let mut log_w = log.write().await;
        log_w.clear();
    }

    if let Some(stderr) = child.stderr.take() {
        let log_ref = log.clone();
        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                eprintln!("[server] {}", line);
                let mut log_w = log_ref.write().await;
                log_w.push(line);
                // Keep last 200 lines
                if log_w.len() > 200 {
                    let excess = log_w.len() - 200;
                    log_w.drain(..excess);
                }
            }
        });
    }

    if let Some(stdout) = child.stdout.take() {
        let log_ref = log.clone();
        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                eprintln!("[server] {}", line);
                let mut log_w = log_ref.write().await;
                log_w.push(line);
                if log_w.len() > 200 {
                    let excess = log_w.len() - 200;
                    log_w.drain(..excess);
                }
            }
        });
    }

    // Wait briefly to see if it crashes immediately
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Check if process is still alive
    match child.try_wait() {
        Ok(Some(status)) => {
            let log_r = log.read().await;
            let last_lines: Vec<String> = log_r.iter().rev().take(10).rev().cloned().collect();
            return Err(format!(
                "Server exited immediately with {}\n{}",
                status,
                last_lines.join("\n")
            ));
        }
        Ok(None) => {} // Still running — good
        Err(e) => return Err(format!("Failed to check server status: {}", e)),
    }

    // Store the process
    *state.server_port.write().await = port;
    *state.server_process.write().await = Some(child);

    Ok(ServerStatus {
        running: true,
        port,
        url: Some(format!("http://localhost:{}", port)),
        log: vec![],
        error: None,
    })
}

#[tauri::command]
async fn stop_server(state: State<'_, AppState>) -> Result<ServerStatus, String> {
    let mut proc = state.server_process.write().await;

    if let Some(ref mut child) = *proc {
        // Try graceful kill first
        if let Err(e) = child.kill().await {
            eprintln!("[server] kill error: {}", e);
        }
        let _ = child.wait().await;
    }

    *proc = None;

    Ok(ServerStatus {
        running: false,
        port: *state.server_port.read().await,
        url: None,
        log: vec![],
        error: None,
    })
}

#[tauri::command]
async fn get_server_status(state: State<'_, AppState>) -> Result<ServerStatus, String> {
    let mut proc = state.server_process.write().await;
    // Use the config port (which reflects the user's actual setting),
    // falling back to the runtime port if a server was started
    let config_port = state.config.read().await.server.port;
    let runtime_port = *state.server_port.read().await;
    let port = if proc.is_some() { runtime_port } else { config_port };

    let running = if let Some(ref mut child) = *proc {
        match child.try_wait() {
            Ok(Some(_status)) => {
                // Process has exited
                *proc = None;
                false
            }
            Ok(None) => true,   // Still running
            Err(_) => {
                *proc = None;
                false
            }
        }
    } else {
        false
    };

    let log = {
        let log_r = state.server_log.read().await;
        log_r.iter().rev().take(50).rev().cloned().collect()
    };

    Ok(ServerStatus {
        running,
        port,
        url: if running {
            Some(format!("http://localhost:{}", port))
        } else {
            None
        },
        log,
        error: None,
    })
}

// ============================================================================
// Tauri Application Entry Point
// ============================================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState::default();

    let server_proc_cleanup = app_state.server_process.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(app_state)
        .on_window_event(move |_window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                // Kill server process when app closes
                let proc = server_proc_cleanup.clone();
                tokio::spawn(async move {
                    let mut guard = proc.write().await;
                    if let Some(ref mut child) = *guard {
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                    }
                    *guard = None;
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_stats,
            init_skills,
            list_skills,
            get_skill,
            enable_skill,
            disable_skill,
            sync_skills,
            get_skills_config,
            refresh_skills,
            load_config,
            // Providers
            list_providers,
            get_provider_config,
            update_provider_config,
            remove_provider_config,
            // Channels
            list_channels,
            get_channel_status,
            update_channel_config,
            remove_channel_config,
            get_channel_config,
            test_channel_connection,
            // Cron
            list_cron_jobs,
            add_cron_job,
            remove_cron_job,
            run_cron_job,
            // Memory
            list_memories,
            search_memories,
            delete_memory,
            memory_stats,
            compact_memory,
            // Config
            get_config,
            // Chat
            send_message,
            list_models,
            // Server
            start_server,
            stop_server,
            get_server_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
