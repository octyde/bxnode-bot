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
| `memory` | Long-term memory storage and search | `bxnode_bot::memory` |
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
- Message editing for streaming updates

**Public API:**
```rust
pub use registry::ChannelRegistry;
pub trait Channel: Send + Sync {
    fn supports_edit(&self) -> bool;
    async fn edit_message(&self, chat_id: &str, message_id: &str, new_content: &str) -> Result<()>;
    // ... other methods
}
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
- `line.rs` - LINE adapter (feature-gated)
- `signal.rs` - Signal adapter (feature-gated)
- `feishu.rs` - Feishu/Lark adapter (feature-gated)

---

## cli

**Owner:** CLI surface and subcommands

**Public entrypoint:** `bxnode_bot::cli`

**Responsibilities:**
- Command-line argument parsing
- Subcommand routing (serve, config, cron, memory, version)
- Process lifecycle

**Public API:**
```rust
pub fn run() -> Result<()>;
pub struct ServeArgs { ... }
pub struct ConfigArgs { ... }
pub struct CronArgs { ... }
pub struct MemoryArgs { ... }
```

**Internal modules:**
- `config.rs` - Config subcommands (show, validate, init)
- `cron.rs` - Cron subcommands (list, add, remove, run, runs)
- `memory.rs` - Memory subcommands (list, search, stats, get, delete, compact)

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
pub struct MemoryConfig { ... }
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
- HTTP endpoints (health, chat completions, models, stats)
- WebSocket RPC (method handlers)
- Static file serving (embedded UI)
- Shared application state
- Channel event routing to agents

**HTTP Endpoints:**
| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Simple health check (for load balancers) |
| `/health/detailed` | GET | Detailed status of all subsystems |
| `/stats` | GET | Metrics and counts |
| `/v1/chat/completions` | POST | OpenAI-compatible chat API |
| `/v1/models` | GET | List available models |
| `/ws` | GET | WebSocket RPC endpoint |
| `/*` | GET | Static UI assets (fallback) |

**Public API:**
```rust
pub async fn serve(args: ServeArgs) -> Result<()>;
pub struct AppState {
    pub providers: Arc<ProviderRegistry>,
    pub channels: Arc<ChannelRegistry>,
    pub cron: Arc<CronScheduler>,
    pub memory: Arc<RwLock<MemoryStore>>,
}
```

**Internal modules:**
- `http.rs` - HTTP handlers
- `ws.rs` - WebSocket handlers
- `protocol.rs` - RPC frame types
- `methods.rs` - WebSocket RPC method implementations

---

## memory

**Owner:** Long-term memory storage and search

**Public entrypoint:** `bxnode_bot::memory`

**Responsibilities:**
- JSONL append-only storage with soft deletes
- In-memory inverted index for keyword search
- Memory scoping (agent, channel, user, session)
- Memory tools for agent use

**Public API:**
```rust
pub use store::{MemoryStore, MemoryStoreStats};
pub use search::MemorySearchResult;
pub struct MemoryRecord { ... }
pub struct MemoryScope { ... }
pub struct MemoryRecordBuilder { ... }
```

**Internal modules:**
- `store.rs` - JSONL persistence and state management
- `search.rs` - Tokenization, indexing, scoring

**Memory Tools (exposed to agents):**
- `memory_store` - Store content with metadata
- `memory_recall` - Search memories by query
- `memory_forget` - Soft-delete by ID

---

## plugins

**Owner:** Plugin discovery and lifecycle

**Public entrypoint:** `bxnode_bot::plugins`

**Responsibilities:**
- Plugin trait for extensibility
- Plugin registry for lifecycle management
- Hook system for event interception
- Built-in plugin discovery

**Public API:**
```rust
pub use registry::PluginRegistry;
pub trait Plugin: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn version(&self) -> &str;
    fn on_load(&self, ctx: &mut PluginContext) -> Result<()>;
    fn on_unload(&self) -> Result<()>;
}
pub struct PluginContext { ... }
pub struct PluginRecord { ... }
pub enum PluginStatus { Loaded, Disabled, Error }
pub enum HookEvent { BeforeSend, AfterReceive, BeforeCompletion, ... }
pub type HookHandler = Arc<dyn Fn(&HookData) -> Result<HookAction>>;
```

**Internal modules:**
- `registry.rs` - Plugin registry and lifecycle management
- `builtin/mod.rs` - Built-in plugins
- `builtin/hello.rs` - Example hello plugin

**Hook Events:**
- `BeforeSend` - Before sending message to channel
- `AfterReceive` - After receiving message from channel
- `BeforeCompletion` - Before calling LLM provider
- `AfterCompletion` - After LLM response received
- `BeforeToolExecute` - Before tool execution
- `AfterToolExecute` - After tool execution
- `OnServerStart` - When server starts
- `OnServerStop` - When server stops

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
   - `gateway` depends on `providers`, `channels`, `config`, `memory`
   - `cli` depends on `gateway`, `config`, `cron`, `memory`
   - `agent` depends on `providers`, `tools`, `memory`

3. **Feature-gated modules:**
   - `channels::telegram` requires `channel-telegram` feature
   - `channels::discord` requires `channel-discord` feature
   - `channels::slack` requires `channel-slack` feature
   - `channels::line` requires `channel-line` feature
   - `channels::signal` requires `channel-signal` feature
   - `channels::feishu` requires `channel-feishu` feature

---

## Adding New Modules

When adding a new module:

1. Create `src/<module>/mod.rs` with public API
2. Add to `src/lib.rs`: `pub mod <module>;`
3. Document in this file
4. Keep public API minimal
5. Add tests in `src/<module>/tests.rs`
