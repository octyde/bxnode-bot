//! Unit tests for the config module

use super::*;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_default_config() {
    let config = Config::default();

    assert_eq!(config.server.host, "0.0.0.0");
    assert_eq!(config.server.port, 3000);
    assert!(config.server.cors);
    assert!(config.channels.telegram.is_none());
    assert!(config.channels.discord.is_none());
    assert!(config.providers.anthropic.is_none());
}

#[test]
fn test_server_config_defaults() {
    let server = ServerConfig::default();

    assert_eq!(server.host, "0.0.0.0");
    assert_eq!(server.port, 3000);
    assert!(server.cors);
}

#[test]
fn test_load_yaml_config() {
    let yaml_content = r#"
server:
  host: "127.0.0.1"
  port: 8080
  cors: false

agents:
  defaults:
    model: "anthropic/claude-3-opus"

providers:
  anthropic:
    api_key: "test-key"
    base_url: "https://custom.api.anthropic.com"
"#;

    let mut temp_file = NamedTempFile::with_suffix(".yaml").unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let config = Config::load(temp_file.path()).unwrap();

    assert_eq!(config.server.host, "127.0.0.1");
    assert_eq!(config.server.port, 8080);
    assert!(!config.server.cors);
    assert_eq!(config.agents.defaults.model, Some("anthropic/claude-3-opus".to_string()));

    let anthropic = config.providers.anthropic.unwrap();
    assert_eq!(anthropic.api_key, "test-key");
    assert_eq!(anthropic.base_url, Some("https://custom.api.anthropic.com".to_string()));
}

#[test]
fn test_load_json5_config() {
    let json5_content = r#"{
  // This is a JSON5 config with comments
  server: {
    host: "localhost",
    port: 9000,
  },
  providers: {
    openai: {
      api_key: "sk-test",
      organization: "org-123",
    },
  },
}"#;

    let mut temp_file = NamedTempFile::with_suffix(".json5").unwrap();
    temp_file.write_all(json5_content.as_bytes()).unwrap();

    let config = Config::load(temp_file.path()).unwrap();

    assert_eq!(config.server.host, "localhost");
    assert_eq!(config.server.port, 9000);

    let openai = config.providers.openai.unwrap();
    assert_eq!(openai.api_key, "sk-test");
    assert_eq!(openai.organization, Some("org-123".to_string()));
}

#[test]
fn test_load_config_with_channels() {
    let yaml_content = r#"
channels:
  telegram:
    token: "bot123:token"
    allowed_users: [123, 456, 789]
  discord:
    token: "discord-token"
    allowed_guilds: [111, 222]
  slack:
    bot_token: "xoxb-test"
    app_token: "xapp-test"
"#;

    let mut temp_file = NamedTempFile::with_suffix(".yaml").unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let config = Config::load(temp_file.path()).unwrap();

    let telegram = config.channels.telegram.unwrap();
    assert_eq!(telegram.token, "bot123:token");
    assert_eq!(telegram.allowed_users, vec![123, 456, 789]);

    let discord = config.channels.discord.unwrap();
    assert_eq!(discord.token, "discord-token");
    assert_eq!(discord.allowed_guilds, vec![111, 222]);

    let slack = config.channels.slack.unwrap();
    assert_eq!(slack.bot_token, "xoxb-test");
    assert_eq!(slack.app_token, "xapp-test");
}

#[test]
fn test_load_config_with_plugins() {
    let yaml_content = r#"
plugins:
  enabled:
    - "weather"
    - "search"
  settings:
    weather:
      api_key: "weather-key"
    search:
      engine: "google"
"#;

    let mut temp_file = NamedTempFile::with_suffix(".yaml").unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let config = Config::load(temp_file.path()).unwrap();

    assert_eq!(config.plugins.enabled, vec!["weather", "search"]);
    assert!(config.plugins.settings.contains_key("weather"));
    assert!(config.plugins.settings.contains_key("search"));
}

#[test]
fn test_load_nonexistent_config() {
    let result = Config::load("/nonexistent/path/config.yaml");
    assert!(result.is_err());
}

#[test]
fn test_load_or_default_with_none() {
    let config = Config::load_or_default::<&str>(None);

    // Should return default config
    assert_eq!(config.server.port, 3000);
}

#[test]
fn test_load_or_default_with_invalid_path() {
    let config = Config::load_or_default(Some("/nonexistent/config.yaml"));

    // Should return default config on error
    assert_eq!(config.server.port, 3000);
}

#[test]
fn test_ollama_config_default_url() {
    let yaml_content = r#"
providers:
  ollama: {}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".yaml").unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let config = Config::load(temp_file.path()).unwrap();
    let ollama = config.providers.ollama.unwrap();

    assert_eq!(ollama.base_url, "http://localhost:11434");
}

#[test]
fn test_config_serialization_roundtrip() {
    let config = Config {
        server: ServerConfig {
            host: "192.168.1.1".to_string(),
            port: 5000,
            cors: false,
        },
        agents: AgentsConfig {
            defaults: AgentDefaults {
                model: Some("test-model".to_string()),
                image_model: Some("test-image-model".to_string()),
            },
        },
        channels: ChannelsConfig::default(),
        providers: ProvidersConfig::default(),
        plugins: PluginsConfig::default(),
    };

    // Serialize to YAML
    let yaml = serde_yaml::to_string(&config).unwrap();

    // Deserialize back
    let restored: Config = serde_yaml::from_str(&yaml).unwrap();

    assert_eq!(restored.server.host, "192.168.1.1");
    assert_eq!(restored.server.port, 5000);
    assert!(!restored.server.cors);
    assert_eq!(restored.agents.defaults.model, Some("test-model".to_string()));
}

#[test]
fn test_partial_config_uses_defaults() {
    let yaml_content = r#"
server:
  port: 4000
"#;

    let mut temp_file = NamedTempFile::with_suffix(".yaml").unwrap();
    temp_file.write_all(yaml_content.as_bytes()).unwrap();

    let config = Config::load(temp_file.path()).unwrap();

    // Port should be overridden
    assert_eq!(config.server.port, 4000);
    // Host should use default
    assert_eq!(config.server.host, "0.0.0.0");
    // CORS should use default
    assert!(config.server.cors);
}
