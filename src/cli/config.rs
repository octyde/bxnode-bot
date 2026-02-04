//! Configuration CLI commands

use anyhow::Result;

use crate::config::Config;

use super::{ConfigAction, ConfigArgs};

/// Handle config subcommands
pub fn handle(args: ConfigArgs) -> Result<()> {
    match args.action {
        ConfigAction::Show => show_config(),
        ConfigAction::Validate { path } => validate_config(path),
        ConfigAction::Init { output } => init_config(&output),
    }
}

/// Show the effective configuration
fn show_config() -> Result<()> {
    // Try to load from common paths
    let config = load_config_from_paths();

    // Pretty-print as YAML
    let yaml = serde_yaml::to_string(&config)?;
    println!("{}", yaml);

    Ok(())
}

/// Validate a configuration file
fn validate_config(path: Option<String>) -> Result<()> {
    let path = path.unwrap_or_else(|| "config.yaml".to_string());

    println!("Validating configuration: {}", path);

    match Config::load(&path) {
        Ok(config) => {
            println!("Configuration is valid.");
            println!();

            // Show summary
            let provider_count = [
                config.providers.anthropic.is_some(),
                config.providers.openai.is_some(),
                config.providers.ollama.is_some(),
            ]
            .iter()
            .filter(|&&x| x)
            .count();

            let channel_count = [
                config.channels.telegram.is_some(),
                config.channels.discord.is_some(),
                config.channels.slack.is_some(),
            ]
            .iter()
            .filter(|&&x| x)
            .count();

            println!("Summary:");
            println!("  Server: {}:{}", config.server.host, config.server.port);
            println!("  Providers configured: {}", provider_count);
            println!("  Channels configured: {}", channel_count);
            println!("  Cron enabled: {}", config.cron.enabled);
            println!("  Cron jobs defined: {}", config.cron.jobs.len());

            Ok(())
        }
        Err(e) => {
            eprintln!("Configuration error: {}", e);
            Err(e)
        }
    }
}

/// Initialize a new configuration file
fn init_config(output: &str) -> Result<()> {
    use std::io::Write;

    let example_config = include_str!("../../config.example.yaml");

    // Check if file exists
    if std::path::Path::new(output).exists() {
        anyhow::bail!(
            "File '{}' already exists. Remove it first or choose a different name.",
            output
        );
    }

    let mut file = std::fs::File::create(output)?;
    file.write_all(example_config.as_bytes())?;

    println!("Created configuration file: {}", output);
    println!();
    println!("Edit the file to add your API keys and configure channels.");
    println!("Then run: bxnode-bot serve -c {}", output);

    Ok(())
}

/// Load configuration from common paths
fn load_config_from_paths() -> Config {
    let paths = [
        "config.yaml",
        "config.yml",
        "config.json5",
        ".bxnode-bot/config.yaml",
        "~/.bxnode-bot/config.yaml",
    ];

    for path in paths {
        let expanded = expand_path(path);
        if let Ok(config) = Config::load(&expanded) {
            eprintln!("(Loaded from: {})", expanded);
            return config;
        }
    }

    eprintln!("(Using default configuration - no config file found)");
    Config::default()
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
