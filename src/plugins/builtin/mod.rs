//! Built-in plugins that ship with bxnode-bot
//!
//! These plugins demonstrate the plugin system and provide useful functionality.

mod hello;

pub use hello::HelloPlugin;

use super::PluginRegistry;

/// Register all built-in plugins with the registry
pub fn register_builtins(registry: &mut PluginRegistry, config: &serde_json::Value) {
    // Hello plugin (example/demo)
    let hello_config = config
        .get("hello")
        .cloned()
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    if let Err(e) = registry.load(Box::new(HelloPlugin::new()), hello_config) {
        tracing::warn!(error = %e, "Failed to load hello plugin");
    }
}

/// Get a list of all built-in plugin IDs
pub fn builtin_plugin_ids() -> &'static [&'static str] {
    &["hello"]
}
