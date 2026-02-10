# Plugin System

BXNode Bot includes a native Rust plugin system that allows extending functionality through plugins. Plugins can register tools (for agent use) and hooks (lifecycle callbacks).

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Plugin Registry                       │
├─────────────────────────────────────────────────────────┤
│  ┌──────────┐  ┌──────────┐  ┌──────────┐              │
│  │ Plugin A │  │ Plugin B │  │ Plugin C │  ...         │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘              │
│       │             │             │                     │
│  ┌────▼─────────────▼─────────────▼────┐               │
│  │           Plugin Context            │               │
│  │  - Registered tools                 │               │
│  │  - Registered hooks                 │               │
│  │  - Plugin settings                  │               │
│  └─────────────────────────────────────┘               │
└─────────────────────────────────────────────────────────┘
```

## Built-in Plugins

### Hello Plugin

A simple example plugin that demonstrates the plugin system.

**Features:**
- Registers a `hello` tool that returns greetings
- Registers an `OnServerStart` hook

**Configuration:**
```yaml
plugins:
  enabled:
    - hello
  settings:
    hello:
      greeting: "Hello"  # Custom greeting (default: "Hello")
```

## CLI Commands

### List Plugins

```bash
# List enabled plugins
bxnode-bot plugin list

# List all plugins (including disabled)
bxnode-bot plugin list --all
```

### Plugin Info

```bash
# Show detailed info about a plugin
bxnode-bot plugin info hello
```

### Enable/Disable

```bash
# Get instructions to enable a plugin
bxnode-bot plugin enable <plugin-id>

# Get instructions to disable a plugin
bxnode-bot plugin disable <plugin-id>
```

Note: Enabling/disabling is done through configuration, not runtime commands.

## Configuration

Plugins are configured in your `config.yaml`:

```yaml
plugins:
  # List of enabled plugin IDs (empty = all built-in plugins enabled)
  enabled:
    - hello

  # Plugin-specific settings
  settings:
    hello:
      greeting: "Hi there"
```

## Writing a Plugin

### Plugin Trait

Implement the `Plugin` trait to create a new plugin:

```rust
use bxnode_bot::plugins::{Plugin, PluginContext};
use async_trait::async_trait;

struct MyPlugin;

#[async_trait]
impl Plugin for MyPlugin {
    fn id(&self) -> &str { "my-plugin" }
    fn name(&self) -> &str { "My Plugin" }
    fn description(&self) -> &str { "Does something useful" }
    fn version(&self) -> &str { "1.0.0" }

    fn on_load(&self, ctx: &mut PluginContext) -> anyhow::Result<()> {
        // Register tools
        ctx.register_tool(Box::new(MyTool::new()));

        // Register hooks
        ctx.register_hook(
            HookEvent::OnServerStart,
            Arc::new(|_data| {
                println!("Server starting!");
                Ok(HookAction::Continue)
            }),
        );

        Ok(())
    }

    fn on_unload(&self) -> anyhow::Result<()> {
        // Cleanup if needed
        Ok(())
    }
}
```

### Registering Tools

Tools provide functionality that agents can use:

```rust
use bxnode_bot::agent::tools::Tool;
use async_trait::async_trait;
use serde_json::{json, Value};

struct MyTool;

#[async_trait]
impl Tool for MyTool {
    fn name(&self) -> &str { "my_tool" }

    fn description(&self) -> &str {
        "Does something useful"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": "The input value"
                }
            },
            "required": ["input"]
        })
    }

    async fn execute(&self, input: Value) -> anyhow::Result<String> {
        let input_value = input.get("input")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing input"))?;

        Ok(format!("Processed: {}", input_value))
    }
}
```

### Hook Events

Hooks allow plugins to intercept various events:

| Event | Description | Use Case |
|-------|-------------|----------|
| `BeforeSend` | Before sending message to channel | Modify outgoing messages |
| `AfterReceive` | After receiving message from channel | Log or filter incoming |
| `BeforeCompletion` | Before calling LLM provider | Add context, rate limit |
| `AfterCompletion` | After LLM response received | Log, analyze responses |
| `BeforeToolExecute` | Before tool execution | Validate, authorize |
| `AfterToolExecute` | After tool execution | Log, modify results |
| `OnServerStart` | When server starts | Initialize resources |
| `OnServerStop` | When server stops | Cleanup resources |

### Hook Actions

Hook handlers return one of three actions:

- `HookAction::Continue` - Continue with the operation
- `HookAction::Stop` - Stop processing further hooks
- `HookAction::Cancel` - Cancel the operation entirely

### Accessing Configuration

Plugins receive configuration through the `PluginContext`:

```rust
fn on_load(&self, ctx: &mut PluginContext) -> anyhow::Result<()> {
    // Get full config
    let config = ctx.config();

    // Get typed config value
    let greeting: String = ctx.get_config("greeting")
        .unwrap_or_else(|| "Hello".to_string());

    Ok(())
}
```

## Plugin Registry

The `PluginRegistry` manages all loaded plugins:

```rust
use bxnode_bot::plugins::PluginRegistry;

let mut registry = PluginRegistry::new();

// Load a plugin
registry.load(Box::new(MyPlugin), config)?;

// Get plugin info
if let Some(record) = registry.get("my-plugin") {
    println!("Status: {}", record.status);
    println!("Tools: {}", record.tool_count);
}

// List all plugins
for plugin in registry.list() {
    println!("{}: {}", plugin.info.id, plugin.status);
}

// Execute hooks
let data = HookData::new(HookEvent::OnServerStart);
registry.execute_hooks(HookEvent::OnServerStart, &data)?;

// Get registered tools
let tools = registry.tools();
```

## Best Practices

1. **Keep plugins focused** - Each plugin should have a single responsibility
2. **Handle errors gracefully** - Return `anyhow::Result` with descriptive errors
3. **Use configuration** - Make behavior configurable through plugin settings
4. **Log important events** - Use `tracing::info!` and `tracing::debug!`
5. **Clean up resources** - Implement `on_unload` if your plugin allocates resources
6. **Document your plugin** - Include description, version, and configuration options
