//! Configuration module - YAML/JSON5 config loading and management

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::skills::SkillSyncSource;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Server configuration
    #[serde(default)]
    pub server: ServerConfig,

    /// Agent configuration
    #[serde(default)]
    pub agents: AgentsConfig,

    /// Channel configurations
    #[serde(default)]
    pub channels: ChannelsConfig,

    /// Provider configurations
    #[serde(default)]
    pub providers: ProvidersConfig,

    /// Plugin configurations
    #[serde(default)]
    pub plugins: PluginsConfig,

    /// Skills configurations (OpenClaw/Agent Skills)
    #[serde(default)]
    pub skills: SkillsConfig,

    /// Cron scheduler configuration
    #[serde(default)]
    pub cron: CronConfig,

    /// Memory configuration
    #[serde(default)]
    pub memory: MemoryConfig,

    /// Workspace configuration for coding projects
    #[serde(default)]
    pub workspace: WorkspaceConfig,

    /// Tools configuration (web search, TTS, etc.)
    #[serde(default)]
    pub tools: ToolsConfig,

    /// Session configuration
    #[serde(default)]
    pub session: SessionConfig,

    /// Rate limiting configuration
    #[serde(default)]
    pub rate_limit: RateLimitConfig,

    /// Channel bindings for per-chat configuration
    #[serde(default)]
    pub channel_bindings: Vec<ChannelBindingConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Host to bind to
    #[serde(default = "default_host")]
    pub host: String,

    /// Port to listen on
    #[serde(default = "default_port")]
    pub port: u16,

    /// Enable CORS
    #[serde(default = "default_true")]
    pub cors: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            cors: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentsConfig {
    /// Default model configuration
    #[serde(default)]
    pub defaults: AgentDefaults,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentDefaults {
    /// Primary model ID
    pub model: Option<String>,

    /// Image model ID
    pub image_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelsConfig {
    /// Telegram channel configuration
    pub telegram: Option<TelegramConfig>,

    /// Discord channel configuration
    pub discord: Option<DiscordConfig>,

    /// Slack channel configuration
    pub slack: Option<SlackConfig>,

    /// LINE channel configuration
    pub line: Option<LineConfig>,

    /// Signal channel configuration
    pub signal: Option<SignalConfig>,

    /// Feishu channel configuration
    pub feishu: Option<FeishuConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    /// Bot token
    pub token: String,

    /// Allowed user IDs (empty = allow all)
    #[serde(default)]
    pub allowed_users: Vec<i64>,

    /// Require admin approval for unknown users before processing messages
    #[serde(default)]
    pub approval_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    /// Bot token
    pub token: String,

    /// Allowed guild IDs
    #[serde(default)]
    pub allowed_guilds: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackConfig {
    /// Bot token
    pub bot_token: String,

    /// App token
    pub app_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineConfig {
    /// Channel access token
    pub channel_access_token: String,

    /// Channel secret
    pub channel_secret: String,

    /// Allowed user IDs (empty = allow all)
    #[serde(default)]
    pub allowed_users: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalConfig {
    /// signal-cli-rest-api base URL
    #[serde(default = "default_signal_api_url")]
    pub api_url: String,

    /// Registered phone number
    pub phone_number: String,

    /// Allowed phone numbers (empty = allow all)
    #[serde(default)]
    pub allowed_numbers: Vec<String>,
}

fn default_signal_api_url() -> String {
    "http://localhost:8080".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuConfig {
    /// App ID
    pub app_id: String,

    /// App Secret
    pub app_secret: String,

    /// Verification token
    pub verification_token: String,

    /// Allowed user IDs (empty = allow all)
    #[serde(default)]
    pub allowed_users: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvidersConfig {
    /// Anthropic configuration
    pub anthropic: Option<AnthropicConfig>,

    /// OpenAI configuration
    pub openai: Option<OpenAIConfig>,

    /// Ollama configuration
    pub ollama: Option<OllamaConfig>,

    /// Z.AI (Zhipu GLM) configuration
    pub zai: Option<GenericProviderConfig>,

    /// Groq configuration
    pub groq: Option<GenericProviderConfig>,

    /// DeepSeek configuration
    pub deepseek: Option<GenericProviderConfig>,

    /// Mistral configuration
    pub mistral: Option<GenericProviderConfig>,

    /// Venice.ai configuration
    pub venice: Option<GenericProviderConfig>,

    /// Qwen (DashScope) configuration
    pub qwen: Option<GenericProviderConfig>,

    /// Google Gemini configuration
    pub gemini: Option<GenericProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    /// API key
    pub api_key: String,

    /// Base URL (optional)
    pub base_url: Option<String>,

    /// Default model for this provider
    #[serde(default)]
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIConfig {
    /// API key
    pub api_key: String,

    /// Base URL (optional)
    pub base_url: Option<String>,

    /// Organization ID (optional)
    pub organization: Option<String>,

    /// Default model for this provider
    #[serde(default)]
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Base URL
    #[serde(default = "default_ollama_url")]
    pub base_url: String,

    /// Default model for this provider
    #[serde(default)]
    pub default_model: Option<String>,
}

/// Generic provider configuration (for OpenAI-compatible APIs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericProviderConfig {
    /// API key
    pub api_key: String,

    /// Base URL (optional)
    pub base_url: Option<String>,

    /// Default model for this provider
    #[serde(default)]
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginsConfig {
    /// Enabled plugins
    #[serde(default)]
    pub enabled: Vec<String>,

    /// Plugin-specific configurations
    #[serde(default)]
    pub settings: std::collections::HashMap<String, serde_json::Value>,
}

/// Cron scheduler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronConfig {
    /// Enable the cron scheduler
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Path to the cron job store file
    #[serde(default = "default_cron_store")]
    pub store_path: String,

    /// Jobs to register on startup
    #[serde(default)]
    pub jobs: Vec<CronJobConfig>,
}

impl Default for CronConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            store_path: default_cron_store(),
            jobs: Vec::new(),
        }
    }
}

/// A cron job defined in configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJobConfig {
    /// Unique job ID
    pub id: String,

    /// Cron schedule expression (6-field: sec min hour day month weekday)
    pub schedule: String,

    /// Job payload (passed to handler)
    #[serde(default)]
    pub payload: serde_json::Value,

    /// Human-readable description
    pub description: Option<String>,

    /// Whether the job is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Memory system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Enable the memory system
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Path to the memory store file (JSONL format)
    #[serde(default = "default_memory_store")]
    pub store_path: String,

    /// Maximum number of results to return from search
    #[serde(default = "default_max_results")]
    pub max_results: usize,

    /// Default TTL for memories in days (0 = no expiry)
    #[serde(default = "default_ttl_days")]
    pub ttl_days: u32,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            store_path: default_memory_store(),
            max_results: default_max_results(),
            ttl_days: default_ttl_days(),
        }
    }
}

/// Skills system configuration (OpenClaw/Agent Skills)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsConfig {
    /// Enable the skills system
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Directories to scan for skills
    #[serde(default = "default_skill_dirs")]
    pub directories: Vec<String>,

    /// Skills to explicitly enable (empty = all discovered skills)
    #[serde(default)]
    pub enabled_skills: Vec<String>,

    /// Skills to explicitly disable
    #[serde(default)]
    pub disabled_skills: Vec<String>,

    /// Sources to sync skills from
    #[serde(default)]
    pub sync_sources: Vec<SkillSyncSource>,
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            directories: default_skill_dirs(),
            enabled_skills: Vec::new(),
            disabled_skills: Vec::new(),
            sync_sources: Vec::new(),
        }
    }
}

/// Workspace configuration for coding projects
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    /// Base directory for project workspaces (default: ~/projects/)
    #[serde(default = "default_workspace_base_dir")]
    pub base_dir: String,

    /// Whether coding tools are enabled by default for new projects
    #[serde(default = "default_true")]
    pub coding_tools_enabled: bool,

    /// Whether shell execution is enabled by default for new projects
    #[serde(default)]
    pub shell_enabled: bool,

    /// Global list of blocked paths (never allow read/write)
    #[serde(default = "default_blocked_paths")]
    pub blocked_paths: Vec<String>,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            base_dir: default_workspace_base_dir(),
            coding_tools_enabled: true,
            shell_enabled: false,
            blocked_paths: default_blocked_paths(),
        }
    }
}

/// Tools configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolsConfig {
    /// Web tools configuration
    #[serde(default)]
    pub web: WebToolsConfig,

    /// TTS tool configuration
    #[serde(default)]
    pub tts: Option<TtsToolConfig>,
}

/// Web tools configuration (search + fetch)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebToolsConfig {
    /// Web search configuration
    #[serde(default)]
    pub search: WebSearchConfig,
}

/// Web search provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebSearchConfig {
    /// Brave Search API key
    pub brave_api_key: Option<String>,

    /// Perplexity API key
    pub perplexity_api_key: Option<String>,
}

/// TTS tool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsToolConfig {
    /// API key for TTS provider
    pub api_key: String,

    /// Base URL (default: OpenAI)
    pub base_url: Option<String>,
}

/// Session management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Idle session TTL in minutes (0 = no eviction)
    #[serde(default = "default_idle_ttl_minutes")]
    pub idle_ttl_minutes: u32,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            idle_ttl_minutes: default_idle_ttl_minutes(),
        }
    }
}

fn default_idle_ttl_minutes() -> u32 {
    30
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Maximum turns per minute per chat
    #[serde(default = "default_max_turns_per_minute")]
    pub max_turns_per_minute: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_turns_per_minute: default_max_turns_per_minute(),
        }
    }
}

fn default_max_turns_per_minute() -> u32 {
    20
}

/// Per-channel binding configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelBindingConfig {
    /// Channel type (telegram, discord, etc.)
    pub channel: String,

    /// Chat ID to match
    pub chat_id: String,

    /// Project to use
    pub project: Option<String>,

    /// Model override
    pub model: Option<String>,

    /// Tool profile override
    pub tool_profile: Option<String>,
}

fn default_workspace_base_dir() -> String {
    "~/projects".to_string()
}

fn default_blocked_paths() -> Vec<String> {
    vec![
        "~/.ssh/*".to_string(),
        "~/.gnupg/*".to_string(),
    ]
}

// Default value functions
fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    3000
}

fn default_true() -> bool {
    true
}

fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_cron_store() -> String {
    "~/.bxnode-bot/cron.json".to_string()
}

fn default_memory_store() -> String {
    "~/.bxnode-bot/memory.jsonl".to_string()
}

fn default_max_results() -> usize {
    5
}

fn default_ttl_days() -> u32 {
    90
}

fn default_skill_dirs() -> Vec<String> {
    vec![
        "~/.bxnode/skills".to_string(),
        "./skills".to_string(),
    ]
}

#[cfg(test)]
mod tests;

impl Config {
    /// Load configuration from a file
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)?;

        let config = if path.extension().map_or(false, |ext| ext == "json5") {
            json5::from_str(&content)?
        } else {
            serde_yaml::from_str(&content)?
        };

        Ok(config)
    }

    /// Save configuration to a file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        let path = path.as_ref();
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_yaml::to_string(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Load configuration from a file, or return default if not found
    pub fn load_or_default<P: AsRef<Path>>(path: Option<P>) -> Self {
        path.and_then(|p| Self::load(p).ok()).unwrap_or_default()
    }
}
