//! Configuration module - YAML/JSON5 config loading and management

use serde::{Deserialize, Serialize};
use std::path::Path;

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    /// Bot token
    pub token: String,

    /// Allowed user IDs (empty = allow all)
    #[serde(default)]
    pub allowed_users: Vec<i64>,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvidersConfig {
    /// Anthropic configuration
    pub anthropic: Option<AnthropicConfig>,

    /// OpenAI configuration
    pub openai: Option<OpenAIConfig>,

    /// Ollama configuration
    pub ollama: Option<OllamaConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    /// API key
    pub api_key: String,

    /// Base URL (optional)
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIConfig {
    /// API key
    pub api_key: String,

    /// Base URL (optional)
    pub base_url: Option<String>,

    /// Organization ID (optional)
    pub organization: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Base URL
    #[serde(default = "default_ollama_url")]
    pub base_url: String,
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

    /// Load configuration from a file, or return default if not found
    pub fn load_or_default<P: AsRef<Path>>(path: Option<P>) -> Self {
        path.and_then(|p| Self::load(p).ok()).unwrap_or_default()
    }
}
