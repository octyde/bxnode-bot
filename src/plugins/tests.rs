//! Tests for the plugin system

use serde_json::json;

use super::*;
use crate::plugins::builtin::HelloPlugin;

#[test]
fn test_plugin_registry_new() {
    let registry = PluginRegistry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
}

#[test]
fn test_plugin_registry_load() {
    let mut registry = PluginRegistry::new();

    let result = registry.load(Box::new(HelloPlugin::new()), json!({}));
    assert!(result.is_ok());

    assert_eq!(registry.len(), 1);
    assert!(!registry.is_empty());

    // Check the plugin is listed
    let plugins = registry.list();
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].info.id, "hello");
    assert_eq!(plugins[0].status, PluginStatus::Loaded);
}

#[test]
fn test_plugin_registry_get() {
    let mut registry = PluginRegistry::new();
    registry.load(Box::new(HelloPlugin::new()), json!({})).unwrap();

    let record = registry.get("hello");
    assert!(record.is_some());

    let record = record.unwrap();
    assert_eq!(record.info.id, "hello");
    assert_eq!(record.info.name, "Hello Plugin");
    assert_eq!(record.status, PluginStatus::Loaded);
    assert_eq!(record.tool_count, 1);
    assert_eq!(record.hook_count, 1);

    // Non-existent plugin
    assert!(registry.get("nonexistent").is_none());
}

#[test]
fn test_plugin_registry_duplicate_load() {
    let mut registry = PluginRegistry::new();

    registry.load(Box::new(HelloPlugin::new()), json!({})).unwrap();

    // Try to load the same plugin again
    let result = registry.load(Box::new(HelloPlugin::new()), json!({}));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("already loaded"));
}

#[test]
fn test_plugin_registry_unload() {
    let mut registry = PluginRegistry::new();
    registry.load(Box::new(HelloPlugin::new()), json!({})).unwrap();

    assert_eq!(registry.len(), 1);

    let result = registry.unload("hello");
    assert!(result.is_ok());
    assert_eq!(registry.len(), 0);

    // Unloading non-existent plugin
    let result = registry.unload("hello");
    assert!(result.is_err());
}

#[test]
fn test_plugin_registry_disable_enable() {
    let mut registry = PluginRegistry::new();
    registry.load(Box::new(HelloPlugin::new()), json!({})).unwrap();

    // Initially loaded
    let record = registry.get("hello").unwrap();
    assert_eq!(record.status, PluginStatus::Loaded);

    // Disable
    registry.disable("hello").unwrap();
    let record = registry.get("hello").unwrap();
    assert_eq!(record.status, PluginStatus::Disabled);

    // Enable
    registry.enable("hello").unwrap();
    let record = registry.get("hello").unwrap();
    assert_eq!(record.status, PluginStatus::Loaded);
}

#[test]
fn test_plugin_registry_tools() {
    let mut registry = PluginRegistry::new();
    registry.load(Box::new(HelloPlugin::new()), json!({})).unwrap();

    let tools = registry.tools();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name(), "hello");
}

#[test]
fn test_plugin_registry_take_tools() {
    let mut registry = PluginRegistry::new();
    registry.load(Box::new(HelloPlugin::new()), json!({})).unwrap();

    let tools = registry.take_tools();
    assert_eq!(tools.len(), 1);

    // After taking, tools should be empty
    assert!(registry.tools().is_empty());
}

#[test]
fn test_plugin_registry_hooks() {
    let mut registry = PluginRegistry::new();
    registry.load(Box::new(HelloPlugin::new()), json!({})).unwrap();

    // HelloPlugin registers an OnServerStart hook
    assert!(registry.has_hooks(HookEvent::OnServerStart));
    assert!(!registry.has_hooks(HookEvent::BeforeSend));
}

#[test]
fn test_plugin_registry_execute_hooks() {
    let mut registry = PluginRegistry::new();
    registry.load(Box::new(HelloPlugin::new()), json!({})).unwrap();

    let data = HookData::new(HookEvent::OnServerStart);
    let result = registry.execute_hooks(HookEvent::OnServerStart, &data);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), HookAction::Continue);
}

#[test]
fn test_plugin_registry_execute_hooks_disabled() {
    let mut registry = PluginRegistry::new();
    registry.load(Box::new(HelloPlugin::new()), json!({})).unwrap();

    // Disable the plugin
    registry.disable("hello").unwrap();

    // Hooks should still "succeed" but skip the disabled plugin's hooks
    let data = HookData::new(HookEvent::OnServerStart);
    let result = registry.execute_hooks(HookEvent::OnServerStart, &data);
    assert!(result.is_ok());
}

#[test]
fn test_plugin_context_new() {
    let ctx = PluginContext::new("test-plugin", json!({"key": "value"}));
    assert_eq!(ctx.plugin_id(), "test-plugin");
    assert_eq!(ctx.config().get("key").unwrap(), "value");
}

#[test]
fn test_plugin_context_get_config() {
    let ctx = PluginContext::new(
        "test-plugin",
        json!({
            "name": "Alice",
            "count": 42,
            "enabled": true
        }),
    );

    let name: Option<String> = ctx.get_config("name");
    assert_eq!(name, Some("Alice".to_string()));

    let count: Option<i32> = ctx.get_config("count");
    assert_eq!(count, Some(42));

    let enabled: Option<bool> = ctx.get_config("enabled");
    assert_eq!(enabled, Some(true));

    let missing: Option<String> = ctx.get_config("missing");
    assert!(missing.is_none());
}

#[test]
fn test_plugin_info() {
    let plugin = HelloPlugin::new();
    let info = plugin.info();

    assert_eq!(info.id, "hello");
    assert_eq!(info.name, "Hello Plugin");
    assert!(!info.description.is_empty());
    assert_eq!(info.version, "1.0.0");
}

#[test]
fn test_plugin_status_display() {
    assert_eq!(format!("{}", PluginStatus::Loaded), "loaded");
    assert_eq!(format!("{}", PluginStatus::Disabled), "disabled");
    assert_eq!(format!("{}", PluginStatus::Error), "error");
}

#[test]
fn test_hook_event_display() {
    assert_eq!(format!("{}", HookEvent::BeforeSend), "before_send");
    assert_eq!(format!("{}", HookEvent::AfterReceive), "after_receive");
    assert_eq!(format!("{}", HookEvent::BeforeCompletion), "before_completion");
    assert_eq!(format!("{}", HookEvent::AfterCompletion), "after_completion");
    assert_eq!(format!("{}", HookEvent::BeforeToolExecute), "before_tool_execute");
    assert_eq!(format!("{}", HookEvent::AfterToolExecute), "after_tool_execute");
    assert_eq!(format!("{}", HookEvent::OnServerStart), "on_server_start");
    assert_eq!(format!("{}", HookEvent::OnServerStop), "on_server_stop");
}

#[test]
fn test_hook_data_new() {
    let data = HookData::new(HookEvent::BeforeSend);
    assert_eq!(data.event, HookEvent::BeforeSend);
    assert_eq!(data.payload, serde_json::Value::Null);
}

#[test]
fn test_hook_data_with_payload() {
    let payload = json!({"message": "test"});
    let data = HookData::with_payload(HookEvent::AfterReceive, payload.clone());
    assert_eq!(data.event, HookEvent::AfterReceive);
    assert_eq!(data.payload, payload);
}

#[test]
fn test_plugin_record_serialization() {
    let record = PluginRecord {
        info: PluginInfo {
            id: "test".to_string(),
            name: "Test Plugin".to_string(),
            description: "A test plugin".to_string(),
            version: "1.0.0".to_string(),
        },
        status: PluginStatus::Loaded,
        tool_count: 2,
        hook_count: 1,
        error: None,
    };

    let json = serde_json::to_string(&record).unwrap();
    assert!(json.contains("\"id\":\"test\""));
    assert!(json.contains("\"status\":\"loaded\""));
    assert!(json.contains("\"tool_count\":2"));
}

/// A test plugin that fails to load
struct FailingPlugin;

#[async_trait::async_trait]
impl Plugin for FailingPlugin {
    fn id(&self) -> &str {
        "failing"
    }

    fn name(&self) -> &str {
        "Failing Plugin"
    }

    fn description(&self) -> &str {
        "A plugin that fails to load"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn on_load(&self, _ctx: &mut PluginContext) -> anyhow::Result<()> {
        anyhow::bail!("Intentional failure for testing")
    }
}

#[test]
fn test_plugin_load_failure() {
    let mut registry = PluginRegistry::new();

    let result = registry.load(Box::new(FailingPlugin), json!({}));
    assert!(result.is_err());

    // Plugin should be registered but in error state
    let record = registry.get("failing");
    assert!(record.is_some());

    let record = record.unwrap();
    assert_eq!(record.status, PluginStatus::Error);
    assert!(record.error.is_some());
    assert!(record.error.unwrap().contains("Intentional failure"));
}

/// A test plugin that registers a hook that returns Stop
struct StoppingPlugin;

#[async_trait::async_trait]
impl Plugin for StoppingPlugin {
    fn id(&self) -> &str {
        "stopping"
    }

    fn name(&self) -> &str {
        "Stopping Plugin"
    }

    fn description(&self) -> &str {
        "A plugin that stops hook execution"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn on_load(&self, ctx: &mut PluginContext) -> anyhow::Result<()> {
        ctx.register_hook(
            HookEvent::BeforeSend,
            std::sync::Arc::new(|_| Ok(HookAction::Stop)),
        );
        Ok(())
    }
}

#[test]
fn test_hook_stop_propagation() {
    let mut registry = PluginRegistry::new();
    registry.load(Box::new(StoppingPlugin), json!({})).unwrap();

    let data = HookData::new(HookEvent::BeforeSend);
    let result = registry.execute_hooks(HookEvent::BeforeSend, &data);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), HookAction::Stop);
}

/// A test plugin that registers a hook that returns Cancel
struct CancellingPlugin;

#[async_trait::async_trait]
impl Plugin for CancellingPlugin {
    fn id(&self) -> &str {
        "cancelling"
    }

    fn name(&self) -> &str {
        "Cancelling Plugin"
    }

    fn description(&self) -> &str {
        "A plugin that cancels operations"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn on_load(&self, ctx: &mut PluginContext) -> anyhow::Result<()> {
        ctx.register_hook(
            HookEvent::BeforeCompletion,
            std::sync::Arc::new(|_| Ok(HookAction::Cancel)),
        );
        Ok(())
    }
}

#[test]
fn test_hook_cancel_propagation() {
    let mut registry = PluginRegistry::new();
    registry.load(Box::new(CancellingPlugin), json!({})).unwrap();

    let data = HookData::new(HookEvent::BeforeCompletion);
    let result = registry.execute_hooks(HookEvent::BeforeCompletion, &data);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), HookAction::Cancel);
}
