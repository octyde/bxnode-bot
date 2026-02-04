# BXNode Bot Implementation Plan (Revised)

## Executive Summary

**Goal:** Single binary AI agent gateway with multi-platform messaging support.

### Current Progress (Phase 1-3 Complete)
- CLI framework with `clap`
- HTTP/WebSocket server (`axum` + `tokio-tungstenite`)
- Configuration system (`serde_yaml`, `json5`)
- Provider clients: Anthropic, OpenAI, Ollama with streaming
- Tool calling framework with JSON schemas
- Agent context management and execution loop
- Channel implementations: Telegram, Discord, Slack
- Gateway integration with channel registry

### Nanobot Lessons Applied
Based on analysis of HKUDS/nanobot, we're incorporating:
1. **Clear module boundaries** - Single public interface per module
2. **Cron as first-class primitive** - Scheduling built into core
3. **Minimal, explicit agent loop** - context → tools → execution
4. **Config-first UX** - Single config file controls everything

---

## Revised Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                    bxnode-bot (single binary)                 │
│                        ~12 MB (current)                       │
├──────────────────────────────────────────────────────────────┤
│  CLI Layer (clap)                                            │
│    └─ Commands: serve, config, cron, version                 │
├──────────────────────────────────────────────────────────────┤
│  Gateway Server (axum + tokio)                               │
│    ├─ HTTP endpoints (/v1/chat/completions, webhooks)        │
│    ├─ WebSocket RPC (tokio-tungstenite)                      │
│    └─ Static file server (embedded UI)                       │
├──────────────────────────────────────────────────────────────┤
│  Core Services                                               │
│    ├─ Session Manager (JSONL transcripts)                    │
│    ├─ Channel Router                                         │
│    ├─ Cron Scheduler (NEW - first-class)                     │
│    └─ Config Manager                                         │
├──────────────────────────────────────────────────────────────┤
│  LLM Provider Clients (reqwest + serde)                      │
│    └─ Anthropic, OpenAI, Ollama (✓ complete)                 │
├──────────────────────────────────────────────────────────────┤
│  Agent Runtime                                               │
│    ├─ Execution loop (✓ basic)                               │
│    ├─ Tool registry (✓ complete)                             │
│    └─ Context management (✓ complete)                        │
├──────────────────────────────────────────────────────────────┤
│  Channel Clients                                             │
│    └─ teloxide, serenity, slack (✓ complete)                 │
├──────────────────────────────────────────────────────────────┤
│  Embedded Assets (rust-embed)                                │
│    └─ ui/dist/* (pending)                                    │
└──────────────────────────────────────────────────────────────┘
```

---

## Module Map (Clear Boundaries)

Each module has a single public entrypoint via `mod.rs`:

| Module | Owner | Public Entrypoint |
|--------|-------|-------------------|
| `agent` | Agent loop, context, tool execution | `bxnode_bot::agent` |
| `channels` | Channel adapters and registry | `bxnode_bot::channels` |
| `cli` | CLI surface and subcommands | `bxnode_bot::cli` |
| `config` | Config schema and loading | `bxnode_bot::config` |
| `cron` | **NEW** Cron scheduler and store | `bxnode_bot::cron` |
| `gateway` | HTTP and WS server | `bxnode_bot::gateway` |
| `plugins` | Plugin discovery and lifecycle | `bxnode_bot::plugins` |
| `providers` | LLM client registry | `bxnode_bot::providers` |
| `session` | Session storage and persistence | `bxnode_bot::session` |

---

## Revised Phase Plan

### Phase 1: Foundation ✓ COMPLETE
- [x] CLI framework with `clap`
- [x] HTTP/WebSocket server skeleton
- [x] Configuration system
- [x] Basic session management

### Phase 2: LLM Integration ✓ COMPLETE
- [x] Provider abstraction trait
- [x] Implement: Anthropic, OpenAI, Ollama
- [x] Streaming response support (SSE)
- [x] Tool calling framework

### Phase 3: Channel Migration ✓ COMPLETE (Core)
- [x] Channel abstraction trait
- [x] Implement: Telegram, Discord, Slack
- [x] Unified message routing
- [x] Gateway integration

### Phase 4: Nanobot Adoption (NEW - Current)
**Goal:** Tighten architecture based on nanobot lessons.

#### 4.1 Module Boundaries (0.5 days)
- [ ] Create `docs/module-map.md` with ownership and entrypoints
- [ ] Audit cross-module imports
- [ ] Add `tests/module_boundaries.rs` guardrail

#### 4.2 Cron as First-Class Primitive (3 days)
- [ ] Create `src/cron/mod.rs`, `scheduler.rs`, `store.rs`
- [ ] Add `CronConfig` to config schema
- [ ] Add CLI commands: `cron list`, `cron add`, `cron remove`, `cron run`
- [ ] Wire cron into gateway startup
- [ ] Add tests for cron scheduling

#### 4.3 Config-First UX (1 day)
- [ ] Ensure single config file controls all subsystems
- [ ] Add `config show` CLI command for effective config
- [ ] Create `docs/configuration.md`
- [ ] Create `config.example.yaml` with all options

#### 4.4 Agent Loop Clarity (1 day)
- [ ] Document agent loop flow in `src/agent/mod.rs`
- [ ] Add lifecycle tests for context → tools → execution
- [ ] Update `docs/module-map.md` with agent internals

### Phase 5: Message-to-Agent Routing (2 days)
**Goal:** Complete the channel → agent → response flow.

- [ ] Route channel messages to agent for processing
- [ ] Send agent responses back to channel
- [ ] Handle streaming responses in channels
- [ ] Add conversation context per chat_id

### Phase 6: Remaining Channels (Optional, 5-7 days)
**Goal:** HTTP wrapper channels for completeness.

- [ ] LINE (HTTP wrapper)
- [ ] Signal (HTTP wrapper)
- [ ] iMessage (BlueBubbles proxy)
- [ ] Feishu (HTTP wrapper)

### Phase 7: Plugin System (6-8 weeks)
**Goal:** Native Rust extensibility.

- [ ] Plugin trait and dynamic loading
- [ ] Plugin registry and lifecycle
- [ ] Example plugins

### Phase 8: Polish (2-3 weeks)
**Goal:** Production ready.

- [ ] Comprehensive testing
- [ ] Performance benchmarking
- [ ] Documentation
- [ ] Web UI embedding

---

## Immediate Next Tasks (Prioritized)

Based on nanobot adoption plan:

### Task 1: Module Map (0.5 day)
Create `docs/module-map.md`:
```markdown
# Module Map

## agent
Owner: Agent loop and tool execution
Public entrypoint: `bxnode_bot::agent`

## channels
Owner: Channel adapters and registry
Public entrypoint: `bxnode_bot::channels`

...
```

### Task 2: Cron Module (1.5 days)
Create `src/cron/`:
```rust
// src/cron/mod.rs
pub mod scheduler;
pub mod store;

pub use scheduler::{CronScheduler, CronJob};
pub use store::{CronStore, CronStoreEntry};
```

### Task 3: Cron Config (0.5 day)
Update `src/config/mod.rs`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CronConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default = "default_cron_store")]
    pub store_path: String,
}
```

### Task 4: Cron CLI (1 day)
Add to `src/cli/mod.rs`:
```rust
#[derive(Subcommand, Debug)]
pub enum CronAction {
    List,
    Add { id: String, schedule: String, payload: String },
    Remove { id: String },
    Run { id: String },
}
```

### Task 5: Message Routing (2 days)
Update `src/gateway/mod.rs` event handler:
```rust
ChannelEvent::Message(msg) => {
    // Route to agent, get response, send back to channel
    let response = agent.process(msg).await;
    channels.send(&msg.channel, response).await;
}
```

---

## Definition of Done (Phase 4)

1. Module map doc exists at `docs/module-map.md`
2. No deep cross-module imports outside public interfaces
3. Cron is configurable, manageable by CLI, and tested
4. Config is the single source of truth
5. Agent loop is documented with lifecycle tests
6. Channel messages route to agent and responses return

---

## Current Codebase Stats

| Metric | Value |
|--------|-------|
| Rust LOC | ~5,500 |
| Test count | 222 |
| Binary size (release) | 12 MB |
| Providers | 3 (Anthropic, OpenAI, Ollama) |
| Channels | 3 (Telegram, Discord, Slack) |
| Features | streaming, tool calling, SSE |

---

## Risk Assessment

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Cron storage migration | Low | Low | Use simple JSON store with versioned schema |
| Module boundary refactors | Medium | Low | Add boundary tests first |
| Agent-channel routing complexity | Medium | Medium | Start with simple sync flow |

---

## File Structure (Target)

```
bxnode-bot/
├── Cargo.toml
├── config.example.yaml          # NEW: Complete config example
├── docs/
│   ├── module-map.md           # NEW: Module boundaries
│   ├── configuration.md        # NEW: Config reference
│   └── nanobot-lessons.md      # Existing
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── agent/                  # ✓ Complete
│   ├── channels/               # ✓ Complete (core)
│   ├── cli/
│   │   ├── mod.rs
│   │   ├── config.rs
│   │   └── cron.rs             # NEW
│   ├── config/                 # ✓ Complete
│   ├── cron/                   # NEW
│   │   ├── mod.rs
│   │   ├── scheduler.rs
│   │   ├── store.rs
│   │   └── tests.rs
│   ├── gateway/                # ✓ Complete
│   ├── plugins/                # Stub
│   ├── providers/              # ✓ Complete
│   └── session/                # ✓ Complete
├── tests/
│   ├── http_integration.rs
│   ├── websocket_integration.rs
│   ├── module_boundaries.rs    # NEW
│   └── cron_e2e.rs             # NEW
└── plan/
    ├── 001-nanobot-best-parts.md
    ├── 002-nanobot-adoption-plan.md
    └── 003-nanobot-adoption-tasks.md
```

---

## Conclusion

The revised plan integrates nanobot lessons to create a cleaner, more maintainable architecture:

1. **Module boundaries** make the codebase easier to understand and modify
2. **Cron as first-class** enables scheduling use cases without hacks
3. **Config-first UX** improves onboarding and debugging
4. **Explicit agent loop** makes reasoning flow transparent

Estimated time for Phase 4 (Nanobot Adoption): **~7 days**

The plan prioritizes architectural improvements before adding more features, ensuring the foundation remains solid as complexity grows.
