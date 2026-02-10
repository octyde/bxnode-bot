//! Plugins module - Native Rust plugin system
//!
//! Plugins can register:
//! - Tools (for agent use)
//! - Hooks (lifecycle callbacks)
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                    Plugin Registry                       │
//! ├─────────────────────────────────────────────────────────┤
//! │  ┌──────────┐  ┌──────────┐  ┌──────────┐              │
//! │  │ Plugin A │  │ Plugin B │  │ Plugin C │  ...         │
//! │  └────┬─────┘  └────┬─────┘  └────┬─────┘              │
//! │       │             │             │                     │
//! │  ┌────▼─────────────▼─────────────▼────┐               │
//! │  │           Plugin Context            │               │
//! │  │  - Registered tools                 │               │
//! │  │  - Registered hooks                 │               │
//! │  │  - Plugin settings                  │               │
//! │  └─────────────────────────────────────┘               │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example Plugin
//!
//! ```rust,ignore
//! use bxnode_bot::plugins::{Plugin, PluginContext};
//!
//! struct MyPlugin;
//!
//! impl Plugin for MyPlugin {
//!     fn id(&self) -> &str { "my-plugin" }
//!     fn name(&self) -> &str { "My Plugin" }
//!     fn description(&self) -> &str { "Does something useful" }
//!     fn version(&self) -> &str { "1.0.0" }
//!
//!     fn on_load(&self, ctx: &mut PluginContext) -> anyhow::Result<()> {
//!         ctx.register_tool(Box::new(MyTool::new()));
//!         Ok(())
//!     }
//! }
//! ```

pub mod builtin;
pub mod registry;

#[cfg(test)]
mod tests;

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::agent::tools::Tool;

// Re-exports
pub use registry::PluginRegistry;

/// Plugin trait - implement this for each plugin
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Unique plugin identifier (e.g., "my-plugin")
    fn id(&self) -> &str;

    /// Human-readable plugin name
    fn name(&self) -> &str;

    /// Plugin description
    fn description(&self) -> &str;

    /// Plugin version (semver format)
    fn version(&self) -> &str;

    /// Called when the plugin is loaded
    /// Use this to register tools, hooks, etc.
    fn on_load(&self, ctx: &mut PluginContext) -> anyhow::Result<()>;

    /// Called when the plugin is being unloaded
    fn on_unload(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Plugin metadata for display
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: self.id().to_string(),
            name: self.name().to_string(),
            description: self.description().to_string(),
            version: self.version().to_string(),
        }
    }
}

/// Plugin context - provided to plugins during load
/// Allows plugins to register capabilities
pub struct PluginContext {
    /// Plugin ID (for namespacing)
    plugin_id: String,

    /// Plugin configuration from config file
    config: serde_json::Value,

    /// Tools registered by this plugin
    tools: Vec<Box<dyn Tool>>,

    /// Hooks registered by this plugin
    hooks: Vec<HookRegistration>,
}

impl PluginContext {
    /// Create a new plugin context
    pub fn new(plugin_id: &str, config: serde_json::Value) -> Self {
        Self {
            plugin_id: plugin_id.to_string(),
            config,
            tools: vec![],
            hooks: vec![],
        }
    }

    /// Get the plugin ID
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    /// Get the plugin configuration
    pub fn config(&self) -> &serde_json::Value {
        &self.config
    }

    /// Get a typed configuration value
    pub fn get_config<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.config.get(key).and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Register a tool
    pub fn register_tool(&mut self, tool: Box<dyn Tool>) {
        tracing::debug!(
            plugin = %self.plugin_id,
            tool = %tool.name(),
            "Registering tool"
        );
        self.tools.push(tool);
    }

    /// Register a hook
    pub fn register_hook(&mut self, event: HookEvent, handler: HookHandler) {
        tracing::debug!(
            plugin = %self.plugin_id,
            event = ?event,
            "Registering hook"
        );
        self.hooks.push(HookRegistration {
            plugin_id: self.plugin_id.clone(),
            event,
            handler,
        });
    }

    /// Take all registered tools (consumes them)
    pub(crate) fn take_tools(&mut self) -> Vec<Box<dyn Tool>> {
        std::mem::take(&mut self.tools)
    }

    /// Take all registered hooks (consumes them)
    pub(crate) fn take_hooks(&mut self) -> Vec<HookRegistration> {
        std::mem::take(&mut self.hooks)
    }
}

/// Plugin information (serializable)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
}

/// Plugin status in the registry
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginStatus {
    /// Plugin is loaded and active
    Loaded,
    /// Plugin is registered but disabled
    Disabled,
    /// Plugin failed to load
    Error,
}

impl fmt::Display for PluginStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PluginStatus::Loaded => write!(f, "loaded"),
            PluginStatus::Disabled => write!(f, "disabled"),
            PluginStatus::Error => write!(f, "error"),
        }
    }
}

/// Plugin record in the registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRecord {
    /// Plugin info
    #[serde(flatten)]
    pub info: PluginInfo,

    /// Current status
    pub status: PluginStatus,

    /// Number of tools registered
    pub tool_count: usize,

    /// Number of hooks registered
    pub hook_count: usize,

    /// Error message if status is Error
    pub error: Option<String>,
}

/// Hook events that plugins can listen to
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    /// Before a message is sent to a channel
    BeforeSend,

    /// After a message is received from a channel
    AfterReceive,

    /// Before calling an LLM provider
    BeforeCompletion,

    /// After receiving an LLM response
    AfterCompletion,

    /// Before a tool is executed
    BeforeToolExecute,

    /// After a tool is executed
    AfterToolExecute,

    /// When the server starts
    OnServerStart,

    /// When the server stops
    OnServerStop,
}

impl fmt::Display for HookEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookEvent::BeforeSend => write!(f, "before_send"),
            HookEvent::AfterReceive => write!(f, "after_receive"),
            HookEvent::BeforeCompletion => write!(f, "before_completion"),
            HookEvent::AfterCompletion => write!(f, "after_completion"),
            HookEvent::BeforeToolExecute => write!(f, "before_tool_execute"),
            HookEvent::AfterToolExecute => write!(f, "after_tool_execute"),
            HookEvent::OnServerStart => write!(f, "on_server_start"),
            HookEvent::OnServerStop => write!(f, "on_server_stop"),
        }
    }
}

/// Hook handler function type
pub type HookHandler = Arc<dyn Fn(&HookData) -> anyhow::Result<HookAction> + Send + Sync>;

/// Data passed to hook handlers
#[derive(Debug, Clone)]
pub struct HookData {
    /// The event that triggered this hook
    pub event: HookEvent,

    /// Event-specific payload
    pub payload: serde_json::Value,
}

impl HookData {
    pub fn new(event: HookEvent) -> Self {
        Self {
            event,
            payload: serde_json::Value::Null,
        }
    }

    pub fn with_payload(event: HookEvent, payload: serde_json::Value) -> Self {
        Self { event, payload }
    }
}

/// Action returned by hook handlers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookAction {
    /// Continue with the operation
    Continue,
    /// Stop processing further hooks
    Stop,
    /// Cancel the operation (if applicable)
    Cancel,
}

/// Internal hook registration
pub(crate) struct HookRegistration {
    pub plugin_id: String,
    pub event: HookEvent,
    pub handler: HookHandler,
}
