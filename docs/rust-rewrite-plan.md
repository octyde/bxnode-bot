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
│    └─ Commands: serve, config, cron, memory, version         │
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
| `cron` | Cron scheduler and store | `bxnode_bot::cron` |
| `gateway` | HTTP and WS server | `bxnode_bot::gateway` |
| `memory` | Long-term memory storage and search | `bxnode_bot::memory` |
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

### Phase 4: Nanobot Adoption ✓ COMPLETE
**Goal:** Tighten architecture based on nanobot lessons.

#### 4.1 Module Boundaries ✓
- [x] Create `docs/module-map.md` with ownership and entrypoints
- [x] Audit cross-module imports
- [x] Add `tests/module_boundaries.rs` guardrail

#### 4.2 Cron as First-Class Primitive ✓
- [x] Create `src/cron/mod.rs`, `scheduler.rs`, `store.rs`
- [x] Add `CronConfig` to config schema
- [x] Add CLI commands: `cron list`, `cron add`, `cron remove`, `cron run`
- [x] Wire cron into gateway startup
- [x] Add tests for cron scheduling

#### 4.3 Config-First UX ✓
- [x] Ensure single config file controls all subsystems
- [x] Add `config show` CLI command for effective config
- [x] Create `docs/configuration.md`
- [x] Create `config.example.yaml` with all options

#### 4.4 Agent Loop Clarity ✓
- [x] Document agent loop flow in `src/agent/mod.rs`
- [x] Add lifecycle tests for context → tools → execution
- [x] Update `docs/module-map.md` with agent internals

### Phase 5: Message-to-Agent Routing ✓ COMPLETE
**Goal:** Complete the channel → agent → response flow.

- [x] Route channel messages to agent for processing
- [x] Send agent responses back to channel
- [x] Handle streaming responses in channels
- [x] Add conversation context per chat_id

### Phase 5.5: Memory System ✓ COMPLETE (NEW)
**Goal:** Add long-term memory support for agents.

- [x] Create `src/memory/mod.rs`, `store.rs`, `search.rs`, `tests.rs`
- [x] Add `MemoryConfig` to config schema
- [x] Implement JSONL append-only storage with soft deletes
- [x] Implement in-memory inverted index for search
- [x] Add memory tools: `memory_store`, `memory_recall`, `memory_forget`
- [x] Integrate memory store with gateway
- [x] Create `docs/memory.md` documentation
- [x] Add memory CLI commands: `list`, `search`, `stats`, `get`, `delete`, `compact`

### Phase 5.6: Streaming Channel Updates ✓ COMPLETE
**Goal:** Enable real-time message editing for streaming responses in channels.

- [x] Add `supports_edit()` and `edit_message()` methods to Channel trait
- [x] Implement message editing for Telegram (teloxide)
- [x] Implement message editing for Discord (serenity)
- [x] Implement message editing for Slack (chat.update API)
- [x] Add registry methods for checking edit support and editing messages

### Phase 6: Additional Channels ✓ COMPLETE
**Goal:** HTTP wrapper channels for completeness.

- [x] LINE (webhook + push API)
- [x] Signal (signal-cli-rest-api integration)
- [x] Feishu (webhook + Bot API)
- [ ] iMessage (BlueBubbles proxy) - deferred

### Phase 7: Plugin System ✓ COMPLETE
**Goal:** Native Rust extensibility.

- [x] Plugin trait with async lifecycle (on_load, on_unload)
- [x] Plugin registry with load/unload/enable/disable
- [x] Hook system with 8 event types
- [x] Example hello plugin with tool and hook
- [x] Plugin CLI commands (list, info, enable, disable)
- [x] Plugin documentation

### Phase 8: Polish ✓ COMPLETE
**Goal:** Production ready.

- [x] Comprehensive testing (300+ tests passing with all features)
- [x] Enhanced health endpoint (`/health/detailed`) with subsystem status
- [x] Stats/metrics endpoint (`/stats`)
- [x] Release build optimization (6.3 MB binary)
- [x] Documentation updates
- [ ] Web UI embedding (deferred - UI assets pending)

---

## Immediate Next Tasks (Prioritized)

All phases (1-8) are complete. The project is production-ready.

### Future Enhancements (Optional)
- Web UI with embedded assets
- Vector embeddings for memory search
- iMessage channel (BlueBubbles proxy)
- Additional provider integrations

---

## Definition of Done (Phases 1-8) ✓ COMPLETE

1. ✅ Module map doc exists at `docs/module-map.md`
2. ✅ No deep cross-module imports outside public interfaces
3. ✅ Cron is configurable, manageable by CLI, and tested
4. ✅ Config is the single source of truth
5. ✅ Agent loop is documented with lifecycle tests
6. ✅ Channel messages route to agent and responses return
7. ✅ Memory system with store/recall/forget tools
8. ✅ Memory CLI for management (list, search, stats, compact)
9. ✅ Streaming support with message editing in all channels
10. ✅ Plugin system with trait, registry, hooks, and CLI
11. ✅ Additional channels: LINE, Signal, Feishu
12. ✅ Enhanced health/stats endpoints for monitoring
13. ✅ 300+ tests passing with comprehensive coverage
14. ✅ Optimized release build (6.3 MB)

---

## Current Codebase Stats

| Metric | Value |
|--------|-------|
| Rust LOC | ~12,000 |
| Test count | 300+ (with full features) |
| Binary size (release) | 6.3 MB |
| Providers | 3 (Anthropic, OpenAI, Ollama) |
| Channels | 6 (Telegram, Discord, Slack, LINE, Signal, Feishu) |
| Plugins | 1 built-in (hello) |
| HTTP Endpoints | 6 (/health, /health/detailed, /stats, /v1/chat/completions, /v1/models, /ws) |
| Features | streaming, tool calling, SSE, long-term memory, channel editing, plugin system |

---

## Risk Assessment

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Cron storage migration | Low | Low | Use simple JSON store with versioned schema |
| Module boundary refactors | Medium | Low | Add boundary tests first |
| Agent-channel routing complexity | Medium | Medium | Start with simple sync flow |

---

## File Structure (Current)

```
bxnode-bot/
├── Cargo.toml
├── config.example.yaml          # ✓ Complete config example
├── docs/
│   ├── module-map.md           # ✓ Module boundaries
│   ├── memory.md               # ✓ Memory system docs
│   ├── plugins.md              # ✓ Plugin system docs
│   ├── configuration.md        # ✓ Config reference
│   └── nanobot-lessons.md      # Existing
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── agent/                  # ✓ Complete
│   ├── channels/               # ✓ Complete (core)
│   ├── cli/
│   │   ├── mod.rs
│   │   ├── config.rs
│   │   ├── cron.rs             # ✓ Complete
│   │   ├── memory.rs           # ✓ Complete
│   │   └── plugin.rs           # ✓ Complete
│   ├── config/                 # ✓ Complete
│   ├── cron/                   # ✓ Complete
│   │   ├── mod.rs
│   │   ├── scheduler.rs
│   │   ├── store.rs
│   │   └── tests.rs
│   ├── gateway/                # ✓ Complete
│   ├── memory/                 # ✓ Complete
│   │   ├── mod.rs
│   │   ├── store.rs
│   │   ├── search.rs
│   │   └── tests.rs
│   ├── plugins/                # ✓ Complete
│   │   ├── mod.rs
│   │   ├── registry.rs
│   │   ├── tests.rs
│   │   └── builtin/
│   │       ├── mod.rs
│   │       └── hello.rs
│   ├── providers/              # ✓ Complete
│   └── session/                # ✓ Complete
├── tests/
│   ├── http_integration.rs
│   ├── websocket_integration.rs
│   └── module_boundaries.rs    # ✓ Complete
└── plan/
    ├── 001-nanobot-best-parts.md
    ├── 002-nanobot-adoption-plan.md
    ├── 003-nanobot-adoption-tasks.md
    └── 004-memory-tool-implementation-plan.md
```

---

## Conclusion

The revised plan integrates nanobot lessons to create a cleaner, more maintainable architecture:

1. **Module boundaries** make the codebase easier to understand and modify
2. **Cron as first-class** enables scheduling use cases without hacks
3. **Config-first UX** improves onboarding and debugging
4. **Explicit agent loop** makes reasoning flow transparent
5. **Plugin system** enables native Rust extensibility
6. **Production polish** with comprehensive testing and monitoring endpoints

All phases (1-8) are complete. The bxnode-bot is production-ready with:
- 6 messaging channels (Telegram, Discord, Slack, LINE, Signal, Feishu)
- 3 LLM providers (Anthropic, OpenAI, Ollama)
- Long-term memory system with search
- Cron scheduling for automated tasks
- Native plugin extensibility
- OpenAI-compatible HTTP API
- Optimized 6.3 MB release binary
