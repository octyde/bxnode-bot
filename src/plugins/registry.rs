//! Plugin Registry - manages plugin lifecycle and capability collection

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::agent::tools::Tool;

use super::{
    HookAction, HookData, HookEvent, HookRegistration, Plugin, PluginContext, PluginRecord,
    PluginStatus,
};

/// Plugin registry - manages all loaded plugins
pub struct PluginRegistry {
    /// Loaded plugins
    plugins: HashMap<String, LoadedPlugin>,

    /// All registered tools from all plugins
    tools: Vec<Box<dyn Tool>>,

    /// All registered hooks, grouped by event
    hooks: HashMap<HookEvent, Vec<HookRegistration>>,
}

/// A loaded plugin with its metadata
struct LoadedPlugin {
    /// The plugin instance
    plugin: Box<dyn Plugin>,

    /// Current status
    status: PluginStatus,

    /// Number of tools registered
    tool_count: usize,

    /// Number of hooks registered
    hook_count: usize,

    /// Error message if status is Error
    error: Option<String>,
}

impl PluginRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            tools: Vec::new(),
            hooks: HashMap::new(),
        }
    }

    /// Load and register a plugin
    pub fn load(&mut self, plugin: Box<dyn Plugin>, config: serde_json::Value) -> anyhow::Result<()> {
        let id = plugin.id().to_string();

        // Check if already loaded
        if self.plugins.contains_key(&id) {
            anyhow::bail!("Plugin '{}' is already loaded", id);
        }

        tracing::info!(plugin = %id, "Loading plugin");

        // Create context for the plugin
        let mut ctx = PluginContext::new(&id, config);

        // Call plugin's on_load
        match plugin.on_load(&mut ctx) {
            Ok(()) => {
                // Collect tools and hooks from context
                let tools = ctx.take_tools();
                let hooks = ctx.take_hooks();

                let tool_count = tools.len();
                let hook_count = hooks.len();

                // Add tools to registry
                for tool in tools {
                    tracing::debug!(plugin = %id, tool = %tool.name(), "Adding tool");
                    self.tools.push(tool);
                }

                // Add hooks to registry
                for hook in hooks {
                    tracing::debug!(plugin = %id, event = ?hook.event, "Adding hook");
                    self.hooks
                        .entry(hook.event)
                        .or_insert_with(Vec::new)
                        .push(hook);
                }

                // Store the plugin
                self.plugins.insert(
                    id.clone(),
                    LoadedPlugin {
                        plugin,
                        status: PluginStatus::Loaded,
                        tool_count,
                        hook_count,
                        error: None,
                    },
                );

                tracing::info!(
                    plugin = %id,
                    tools = tool_count,
                    hooks = hook_count,
                    "Plugin loaded successfully"
                );

                Ok(())
            }
            Err(e) => {
                let error_msg = e.to_string();
                tracing::error!(plugin = %id, error = %error_msg, "Plugin failed to load");

                // Store the plugin in error state
                self.plugins.insert(
                    id.clone(),
                    LoadedPlugin {
                        plugin,
                        status: PluginStatus::Error,
                        tool_count: 0,
                        hook_count: 0,
                        error: Some(error_msg.clone()),
                    },
                );

                anyhow::bail!("Plugin '{}' failed to load: {}", id, error_msg)
            }
        }
    }

    /// Unload a plugin by ID
    pub fn unload(&mut self, id: &str) -> anyhow::Result<()> {
        let loaded = self
            .plugins
            .remove(id)
            .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", id))?;

        tracing::info!(plugin = %id, "Unloading plugin");

        // Call plugin's on_unload
        if let Err(e) = loaded.plugin.on_unload() {
            tracing::warn!(plugin = %id, error = %e, "Plugin on_unload failed");
        }

        // Remove plugin's tools (by filtering out tools that were registered by this plugin)
        // Note: We can't easily do this without tracking tool ownership, so we rebuild
        // For now, we just log a warning - full implementation would track tool ownership
        tracing::warn!(
            plugin = %id,
            "Plugin unloaded but tools may still be registered (full cleanup requires restart)"
        );

        // Remove plugin's hooks
        for hooks in self.hooks.values_mut() {
            hooks.retain(|h| h.plugin_id != id);
        }

        Ok(())
    }

    /// Disable a plugin (keeps it loaded but inactive)
    pub fn disable(&mut self, id: &str) -> anyhow::Result<()> {
        let loaded = self
            .plugins
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", id))?;

        if loaded.status == PluginStatus::Disabled {
            return Ok(());
        }

        tracing::info!(plugin = %id, "Disabling plugin");
        loaded.status = PluginStatus::Disabled;

        Ok(())
    }

    /// Enable a disabled plugin
    pub fn enable(&mut self, id: &str) -> anyhow::Result<()> {
        let loaded = self
            .plugins
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", id))?;

        if loaded.status == PluginStatus::Loaded {
            return Ok(());
        }

        if loaded.status == PluginStatus::Error {
            anyhow::bail!("Cannot enable plugin '{}' that is in error state", id);
        }

        tracing::info!(plugin = %id, "Enabling plugin");
        loaded.status = PluginStatus::Loaded;

        Ok(())
    }

    /// Get plugin info by ID
    pub fn get(&self, id: &str) -> Option<PluginRecord> {
        self.plugins.get(id).map(|loaded| PluginRecord {
            info: loaded.plugin.info(),
            status: loaded.status,
            tool_count: loaded.tool_count,
            hook_count: loaded.hook_count,
            error: loaded.error.clone(),
        })
    }

    /// List all plugins
    pub fn list(&self) -> Vec<PluginRecord> {
        self.plugins
            .values()
            .map(|loaded| PluginRecord {
                info: loaded.plugin.info(),
                status: loaded.status,
                tool_count: loaded.tool_count,
                hook_count: loaded.hook_count,
                error: loaded.error.clone(),
            })
            .collect()
    }

    /// Get all registered tools
    pub fn tools(&self) -> &[Box<dyn Tool>] {
        &self.tools
    }

    /// Take ownership of all tools (for integration with ToolRegistry)
    pub fn take_tools(&mut self) -> Vec<Box<dyn Tool>> {
        std::mem::take(&mut self.tools)
    }

    /// Execute hooks for an event
    pub fn execute_hooks(&self, event: HookEvent, data: &HookData) -> anyhow::Result<HookAction> {
        let Some(hooks) = self.hooks.get(&event) else {
            return Ok(HookAction::Continue);
        };

        for hook in hooks {
            // Skip hooks from disabled plugins
            if let Some(loaded) = self.plugins.get(&hook.plugin_id) {
                if loaded.status != PluginStatus::Loaded {
                    continue;
                }
            }

            match (hook.handler)(data) {
                Ok(HookAction::Continue) => continue,
                Ok(HookAction::Stop) => {
                    tracing::debug!(
                        plugin = %hook.plugin_id,
                        event = ?event,
                        "Hook requested stop"
                    );
                    return Ok(HookAction::Stop);
                }
                Ok(HookAction::Cancel) => {
                    tracing::debug!(
                        plugin = %hook.plugin_id,
                        event = ?event,
                        "Hook requested cancel"
                    );
                    return Ok(HookAction::Cancel);
                }
                Err(e) => {
                    tracing::error!(
                        plugin = %hook.plugin_id,
                        event = ?event,
                        error = %e,
                        "Hook execution failed"
                    );
                    // Continue with other hooks on error
                }
            }
        }

        Ok(HookAction::Continue)
    }

    /// Check if any hooks are registered for an event
    pub fn has_hooks(&self, event: HookEvent) -> bool {
        self.hooks.get(&event).map(|h| !h.is_empty()).unwrap_or(false)
    }

    /// Get count of loaded plugins
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared plugin registry for async contexts
pub type SharedPluginRegistry = Arc<RwLock<PluginRegistry>>;
