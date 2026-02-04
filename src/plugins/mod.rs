//! Plugins module - Native Rust plugin system
//!
//! Plugins can register:
//! - Tools
//! - Hooks
//! - Channels
//! - Providers
//! - CLI commands
//! - HTTP handlers

use serde::{Deserialize, Serialize};

/// Plugin trait - implement this for each plugin
pub trait Plugin: Send + Sync {
    /// Plugin identifier
    fn id(&self) -> &str;

    /// Plugin name
    fn name(&self) -> &str;

    /// Plugin description
    fn description(&self) -> &str;

    /// Plugin version
    fn version(&self) -> &str;

    /// Register plugin capabilities
    fn register(&self, api: &mut PluginApi);

    /// Called when plugin is shutting down
    fn shutdown(&self) {}
}

/// Plugin API - provided to plugins during registration
pub struct PluginApi {
    /// Registered tools
    pub tools: Vec<Box<dyn crate::agent::tools::Tool>>,

    /// Registered hooks
    pub hooks: Vec<Hook>,

    /// Plugin configuration
    pub config: serde_json::Value,
}

impl PluginApi {
    pub fn new(config: serde_json::Value) -> Self {
        Self {
            tools: vec![],
            hooks: vec![],
            config,
        }
    }

    /// Register a tool
    pub fn register_tool(&mut self, tool: Box<dyn crate::agent::tools::Tool>) {
        self.tools.push(tool);
    }

    /// Register a hook
    pub fn register_hook(&mut self, hook: Hook) {
        self.hooks.push(hook);
    }
}

/// Hook definition
#[derive(Debug, Clone)]
pub struct Hook {
    /// Hook name
    pub name: String,

    /// Hook event
    pub event: HookEvent,
    // Note: In a real implementation, this would include a boxed async fn handler
}

/// Hook events
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    /// Before sending a message
    BeforeSend,

    /// After receiving a message
    AfterReceive,

    /// Before calling LLM
    BeforeCompletion,

    /// After LLM response
    AfterCompletion,

    /// On tool execution
    OnToolExecute,
}

/// Plugin record (loaded plugin info)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub enabled: bool,
    pub status: PluginStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginStatus {
    Loaded,
    Disabled,
    Error,
}

/// Plugin registry
#[derive(Default)]
pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
    records: Vec<PluginRecord>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        let record = PluginRecord {
            id: plugin.id().to_string(),
            name: plugin.name().to_string(),
            description: plugin.description().to_string(),
            version: plugin.version().to_string(),
            enabled: true,
            status: PluginStatus::Loaded,
            error: None,
        };

        self.records.push(record);
        self.plugins.push(plugin);
    }

    pub fn list(&self) -> &[PluginRecord] {
        &self.records
    }
}
