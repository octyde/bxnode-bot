# Module Map

This document defines module boundaries and public entrypoints for bxnode-bot.
Each module has a single responsibility and exposes a minimal public API via `mod.rs`.

## Module Overview

| Module | Responsibility | Public Entrypoint |
|--------|----------------|-------------------|
| `agent` | Agent loop, context, tool execution | `bxnode_bot::agent` |
| `channels` | Channel adapters and registry | `bxnode_bot::channels` |
| `cli` | CLI surface and subcommands | `bxnode_bot::cli` |
| `config` | Config schema and loading | `bxnode_bot::config` |
| `cron` | Cron scheduler and job store | `bxnode_bot::cron` |
| `gateway` | HTTP and WebSocket server | `bxnode_bot::gateway` |
| `plugins` | Plugin discovery and lifecycle | `bxnode_bot::plugins` |
| `providers` | LLM client registry | `bxnode_bot::providers` |
| `session` | Session storage and persistence | `bxnode_bot::session` |

---

## agent

**Owner:** Agent loop and tool execution

**Public entrypoint:** `bxnode_bot::agent`

**Responsibilities:**
- Agent execution loop (context → tools → response)
- Context management (message history, token limits)
- Tool registry and execution
- Streaming response handling

**Public API:**
```rust
pub use context::AgentContext;
pub use execution::{AgentExecutor, ExecutionConfig, ExecutionUsage};
pub use tools::{Tool, ToolCall, ToolDefinition, ToolRegistry, ToolResult};
```

**Internal modules:**
- `context.rs` - Message history and token management
- `execution.rs` - Agent loop implementation
- `tools.rs` - Tool trait and registry

---

## channels

**Owner:** Channel adapters and registry

**Public entrypoint:** `bxnode_bot::channels`

**Responsibilities:**
- Unified message types (IncomingMessage, OutgoingMessage)
- Channel trait for platform adapters
- Channel registry for multi-platform support
- Event-driven architecture (ChannelEvent)

**Public API:**
```rust
pub use registry::ChannelRegistry;
pub trait Channel: Send + Sync { ... }
pub struct IncomingMessage { ... }
pub struct OutgoingMessage { ... }
pub enum ChannelEvent { ... }
pub struct ChannelStatus { ... }
```

**Internal modules:**
- `registry.rs` - Channel management
- `telegram.rs` - Telegram adapter (feature-gated)
- `discord.rs` - Discord adapter (feature-gated)
- `slack.rs` - Slack adapter (feature-gated)

---

## cli

**Owner:** CLI surface and subcommands

**Public entrypoint:** `bxnode_bot::cli`

**Responsibilities:**
- Command-line argument parsing
- Subcommand routing (serve, config, cron, version)
- Process lifecycle

**Public API:**
```rust
pub fn run() -> Result<()>;
pub struct ServeArgs { ... }
pub struct ConfigArgs { ... }
pub struct CronArgs { ... }
```

**Internal modules:**
- `config.rs` - Config subcommands
- `cron.rs` - Cron subcommands

---

## config

**Owner:** Config schema and loading

**Public entrypoint:** `bxnode_bot::config`

**Responsibilities:**
- Config file loading (YAML, JSON5)
- Config schema definition
- Default values

**Public API:**
```rust
pub struct Config { ... }
pub struct ServerConfig { ... }
pub struct ChannelsConfig { ... }
pub struct ProvidersConfig { ... }
pub struct CronConfig { ... }
```

---

## cron

**Owner:** Cron scheduler and job store

**Public entrypoint:** `bxnode_bot::cron`

**Responsibilities:**
- Cron expression parsing and scheduling
- Job persistence (JSON store)
- Job execution lifecycle

**Public API:**
```rust
pub use scheduler::{CronScheduler, CronJob};
pub use store::{CronStore, CronEntry};
```

**Internal modules:**
- `scheduler.rs` - Cron scheduling engine
- `store.rs` - Job persistence

---

## gateway

**Owner:** HTTP and WebSocket server

**Public entrypoint:** `bxnode_bot::gateway`

**Responsibilities:**
- HTTP endpoints (health, chat completions, models)
- WebSocket RPC (method handlers)
- Static file serving (embedded UI)
- Shared application state

**Public API:**
```rust
pub async fn serve(args: ServeArgs) -> Result<()>;
pub struct AppState { ... }
```

**Internal modules:**
- `http.rs` - HTTP handlers
- `ws.rs` - WebSocket handlers
- `protocol.rs` - RPC frame types

---

## plugins

**Owner:** Plugin discovery and lifecycle

**Public entrypoint:** `bxnode_bot::plugins`

**Responsibilities:**
- Plugin trait definition
- Plugin registry
- Dynamic loading (future)

**Public API:**
```rust
pub trait Plugin: Send + Sync { ... }
pub struct PluginRegistry { ... }
```

---

## providers

**Owner:** LLM client registry

**Public entrypoint:** `bxnode_bot::providers`

**Responsibilities:**
- Provider trait for LLM clients
- Provider registry for model routing
- HTTP clients for each provider

**Public API:**
```rust
pub use registry::ProviderRegistry;
pub trait Provider: Send + Sync { ... }
pub struct CompletionRequest { ... }
pub struct CompletionResponse { ... }
pub struct Message { ... }
pub enum Role { ... }
```

**Internal modules:**
- `registry.rs` - Provider management
- `anthropic.rs` - Claude client
- `openai.rs` - GPT client
- `ollama.rs` - Local inference client

---

## session

**Owner:** Session storage and persistence

**Public entrypoint:** `bxnode_bot::session`

**Responsibilities:**
- Session creation and management
- JSONL transcript storage
- Session listing and loading

**Public API:**
```rust
pub struct Session { ... }
pub struct SessionManager { ... }
pub struct TranscriptEntry { ... }
```

---

## Import Guidelines

1. **Always import from module entrypoints:**
   ```rust
   // Good
   use crate::providers::ProviderRegistry;
   use crate::channels::ChannelRegistry;

   // Bad - bypasses module boundary
   use crate::providers::registry::ProviderRegistry;
   ```

2. **Cross-module dependencies should be minimal:**
   - `gateway` depends on `providers`, `channels`, `config`
   - `cli` depends on `gateway`, `config`, `cron`
   - `agent` depends on `providers`, `tools`

3. **Feature-gated modules:**
   - `channels::telegram` requires `channel-telegram` feature
   - `channels::discord` requires `channel-discord` feature
   - `channels::slack` requires `channel-slack` feature

---

## Adding New Modules

When adding a new module:

1. Create `src/<module>/mod.rs` with public API
2. Add to `src/lib.rs`: `pub mod <module>;`
3. Document in this file
4. Keep public API minimal
5. Add tests in `src/<module>/tests.rs`
