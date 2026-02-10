//! Hello Plugin - A simple example plugin
//!
//! This plugin demonstrates the plugin system by registering:
//! - A "hello" tool that returns a greeting
//! - A hook that logs when the server starts

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent::tools::Tool;
use crate::plugins::{HookAction, HookData, HookEvent, Plugin, PluginContext};

/// Hello plugin - demonstrates the plugin system
pub struct HelloPlugin;

impl HelloPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for HelloPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for HelloPlugin {
    fn id(&self) -> &str {
        "hello"
    }

    fn name(&self) -> &str {
        "Hello Plugin"
    }

    fn description(&self) -> &str {
        "A simple example plugin that demonstrates the plugin system"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn on_load(&self, ctx: &mut PluginContext) -> anyhow::Result<()> {
        // Get greeting from config, or use default
        let greeting: String = ctx
            .get_config("greeting")
            .unwrap_or_else(|| "Hello".to_string());

        // Register the hello tool
        ctx.register_tool(Box::new(HelloTool::new(greeting)));

        // Register a hook for server start
        ctx.register_hook(
            HookEvent::OnServerStart,
            Arc::new(|_data: &HookData| {
                tracing::info!("Hello plugin: Server is starting!");
                Ok(HookAction::Continue)
            }),
        );

        tracing::info!("Hello plugin loaded successfully");
        Ok(())
    }

    fn on_unload(&self) -> anyhow::Result<()> {
        tracing::info!("Hello plugin unloading");
        Ok(())
    }
}

/// A simple tool that returns a greeting
struct HelloTool {
    greeting: String,
}

impl HelloTool {
    fn new(greeting: String) -> Self {
        Self { greeting }
    }
}

#[async_trait]
impl Tool for HelloTool {
    fn name(&self) -> &str {
        "hello"
    }

    fn description(&self) -> &str {
        "Returns a friendly greeting. Use this to say hello!"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The name to greet (optional)"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, input: Value) -> anyhow::Result<String> {
        let name = input
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("World");

        let message = format!("{}, {}!", self.greeting, name);

        Ok(json!({
            "message": message
        }).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hello_plugin_info() {
        let plugin = HelloPlugin::new();
        assert_eq!(plugin.id(), "hello");
        assert_eq!(plugin.name(), "Hello Plugin");
        assert_eq!(plugin.version(), "1.0.0");
    }

    #[test]
    fn test_hello_plugin_load() {
        let plugin = HelloPlugin::new();
        let mut ctx = PluginContext::new("hello", json!({}));

        plugin.on_load(&mut ctx).unwrap();

        // Check that a tool was registered
        let tools = ctx.take_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "hello");

        // Check that a hook was registered
        let hooks = ctx.take_hooks();
        assert_eq!(hooks.len(), 1);
    }

    #[tokio::test]
    async fn test_hello_tool_execute() {
        let tool = HelloTool::new("Hello".to_string());

        // Test without name
        let result = tool.execute(json!({})).await.unwrap();
        assert!(result.contains("Hello, World!"));

        // Test with name
        let result = tool.execute(json!({"name": "Alice"})).await.unwrap();
        assert!(result.contains("Hello, Alice!"));
    }

    #[tokio::test]
    async fn test_hello_tool_custom_greeting() {
        let tool = HelloTool::new("Bonjour".to_string());

        let result = tool.execute(json!({"name": "Claude"})).await.unwrap();
        assert!(result.contains("Bonjour, Claude!"));
    }
}
