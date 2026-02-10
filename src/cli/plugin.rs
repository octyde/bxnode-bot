//! Plugin CLI command implementations

use crate::config::Config;
use crate::plugins::builtin::{builtin_plugin_ids, HelloPlugin};
use crate::plugins::PluginRegistry;

use super::{PluginAction, PluginArgs};

/// Execute a plugin CLI command
pub fn execute(args: PluginArgs) -> anyhow::Result<()> {
    let config = Config::load_or_default(args.config.as_ref());

    match args.action {
        PluginAction::List { all } => list_plugins(&config, all),
        PluginAction::Info { id } => show_plugin_info(&config, &id),
        PluginAction::Enable { id } => enable_plugin(&config, &id),
        PluginAction::Disable { id } => disable_plugin(&config, &id),
    }
}

/// List all plugins
fn list_plugins(config: &Config, show_all: bool) -> anyhow::Result<()> {
    // Create a registry and load built-in plugins
    let mut registry = PluginRegistry::new();
    let plugin_config = serde_json::to_value(&config.plugins.settings)
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    // Load all built-in plugins for listing
    for plugin_id in builtin_plugin_ids() {
        let plugin_settings = plugin_config
            .get(*plugin_id)
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        // Only load if enabled or showing all
        let is_enabled = config.plugins.enabled.is_empty()
            || config.plugins.enabled.contains(&plugin_id.to_string());

        if show_all || is_enabled {
            match *plugin_id {
                "hello" => {
                    let _ = registry.load(Box::new(HelloPlugin::new()), plugin_settings);
                }
                _ => {}
            }
        }
    }

    let plugins = registry.list();

    if plugins.is_empty() {
        println!("No plugins found.");
        if !show_all {
            println!("Use --all to show all available plugins.");
        }
        return Ok(());
    }

    println!(
        "{:<15} {:<20} {:<10} {:<10} {:<10}",
        "ID", "NAME", "VERSION", "STATUS", "TOOLS"
    );
    println!("{}", "-".repeat(70));

    for record in &plugins {
        let enabled = config.plugins.enabled.is_empty()
            || config.plugins.enabled.contains(&record.info.id);
        let status = if enabled {
            format!("{}", record.status)
        } else {
            "disabled".to_string()
        };

        println!(
            "{:<15} {:<20} {:<10} {:<10} {:<10}",
            record.info.id, record.info.name, record.info.version, status, record.tool_count
        );
    }

    println!();
    println!("Total: {} plugins", plugins.len());

    Ok(())
}

/// Show detailed info about a plugin
fn show_plugin_info(config: &Config, id: &str) -> anyhow::Result<()> {
    // Create a registry and load the specific plugin
    let mut registry = PluginRegistry::new();
    let plugin_config = serde_json::to_value(&config.plugins.settings)
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    let plugin_settings = plugin_config
        .get(id)
        .cloned()
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    // Try to load the plugin
    match id {
        "hello" => {
            registry.load(Box::new(HelloPlugin::new()), plugin_settings)?;
        }
        _ => {
            anyhow::bail!("Unknown plugin: {}", id);
        }
    }

    let Some(record) = registry.get(id) else {
        anyhow::bail!("Plugin '{}' not found", id);
    };

    let is_enabled =
        config.plugins.enabled.is_empty() || config.plugins.enabled.contains(&id.to_string());

    println!("Plugin Information");
    println!("{}", "-".repeat(40));
    println!("ID:          {}", record.info.id);
    println!("Name:        {}", record.info.name);
    println!("Version:     {}", record.info.version);
    println!("Description: {}", record.info.description);
    println!();
    println!("Status:");
    println!("  Enabled:   {}", if is_enabled { "yes" } else { "no" });
    println!("  Loaded:    {}", record.status);
    println!("  Tools:     {}", record.tool_count);
    println!("  Hooks:     {}", record.hook_count);

    if let Some(ref error) = record.error {
        println!();
        println!("Error: {}", error);
    }

    // Show tools
    if record.tool_count > 0 {
        println!();
        println!("Registered Tools:");
        for tool in registry.tools() {
            println!("  - {}: {}", tool.name(), tool.description());
        }
    }

    // Show configuration
    let settings = config.plugins.settings.get(id);
    if let Some(settings) = settings {
        if !settings.is_null() {
            println!();
            println!("Configuration:");
            println!(
                "  {}",
                serde_json::to_string_pretty(settings)
                    .unwrap_or_else(|_| "{}".to_string())
                    .replace('\n', "\n  ")
            );
        }
    }

    Ok(())
}

/// Enable a plugin
fn enable_plugin(config: &Config, id: &str) -> anyhow::Result<()> {
    // Verify the plugin exists
    if !builtin_plugin_ids().contains(&id) {
        anyhow::bail!("Unknown plugin: {}", id);
    }

    let is_enabled =
        config.plugins.enabled.is_empty() || config.plugins.enabled.contains(&id.to_string());

    if is_enabled {
        println!("Plugin '{}' is already enabled.", id);
    } else {
        println!("To enable plugin '{}', add it to plugins.enabled in your config file:", id);
        println!();
        println!("plugins:");
        println!("  enabled:");
        for existing in &config.plugins.enabled {
            println!("    - {}", existing);
        }
        println!("    - {}", id);
    }

    Ok(())
}

/// Disable a plugin
fn disable_plugin(config: &Config, id: &str) -> anyhow::Result<()> {
    // Verify the plugin exists
    if !builtin_plugin_ids().contains(&id) {
        anyhow::bail!("Unknown plugin: {}", id);
    }

    let is_enabled =
        config.plugins.enabled.is_empty() || config.plugins.enabled.contains(&id.to_string());

    if !is_enabled {
        println!("Plugin '{}' is already disabled.", id);
    } else if config.plugins.enabled.is_empty() {
        println!("To disable plugin '{}', you need to explicitly enable other plugins in your config file.", id);
        println!();
        println!("plugins:");
        println!("  enabled:");
        for pid in builtin_plugin_ids() {
            if *pid != id {
                println!("    - {}", pid);
            }
        }
        println!("  # '{}' is not listed, so it will be disabled", id);
    } else {
        println!("To disable plugin '{}', remove it from plugins.enabled in your config file.", id);
    }

    Ok(())
}
