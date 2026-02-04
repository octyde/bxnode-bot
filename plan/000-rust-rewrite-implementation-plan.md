# Rust Rewrite Estimation for OpenClaw

## Executive Summary

**Goal:** Single binary deployment replacing the TypeScript/Node.js codebase.

### Scope Decisions
- **Approach:** Full Rust rewrite (no hybrid)
- **WhatsApp:** Not needed (removes highest-risk integration)
- **Plugins:** Native Rust only (rewrite existing extensions)

| Metric | Value |
|--------|-------|
| **Total TS/JS Files** | ~2,572 |
| **Total Lines of Code** | ~80,250 |
| **Extensions** | 31 plugins → rewrite in Rust |
| **Channel Integrations** | 7 platforms (no WhatsApp) |
| **Estimated Rust LOC** | 20,000-28,000 |
| **Estimated Duration** | **12-18 months (1 FTE)** |

---

## Architecture Overview

### Current Stack (TypeScript/Node.js)

```
┌─────────────────────────────────────────────────────────────────┐
│  openclaw.mjs → src/entry.ts (CLI Bootstrap)                    │
├─────────────────────────────────────────────────────────────────┤
│  Gateway Server (src/gateway/) - 124 files                      │
│  ├── Protocol Layer (typebox schemas, WS frames)                │
│  ├── HTTP Server (Express: webhooks, OpenAI compat API)         │
│  ├── WebSocket Server (ws: RPC, real-time events)               │
│  └── Server Methods (30 RPC handlers)                           │
├─────────────────────────────────────────────────────────────────┤
│  Channel Integrations (src/channels/) - 8+ platforms            │
│  ├── WhatsApp (baileys), Telegram (grammy), Discord             │
│  ├── Slack (bolt), LINE, Signal, iMessage, Feishu               │
│  └── Unified interface: dock.ts (15,208 LOC)                    │
├─────────────────────────────────────────────────────────────────┤
│  Plugin System (src/plugins/) - 6,511 LOC                       │
│  ├── Dynamic discovery & loading (jiti)                         │
│  ├── Registry pattern with hot-reload                           │
│  └── 31 extensions in extensions/                               │
├─────────────────────────────────────────────────────────────────┤
│  AI Agent Runtime (@mariozechner/pi-*)                          │
│  ├── pi-ai (LLM provider abstraction)                           │
│  ├── pi-agent-core (agent execution)                            │
│  └── pi-coding-agent (code-specific features)                   │
├─────────────────────────────────────────────────────────────────┤
│  Web Control UI (ui/) - Lit.js + Vite                           │
│  └── 128 files, 21,459 LOC                                      │
└─────────────────────────────────────────────────────────────────┘
```

---

## Component-by-Component Estimation

### 1. CLI Bootstrap & Entry
| Metric | Value |
|--------|-------|
| Current LOC | ~500 |
| Complexity | Low |
| Effort | **1-2 weeks** |
| Rust Crates | `clap`, `tokio`, `anyhow`, `tracing`, `dotenvy` |

**Tasks:**
- Argument parsing with `clap`
- Configuration loading (YAML/JSON5 via `serde_yaml`, `json5`)
- Process lifecycle management
- Environment setup

---

### 2. Gateway Server
| Metric | Value |
|--------|-------|
| Current Files | 124 |
| Current LOC | ~12,000 |
| Complexity | High |
| Effort | **10-14 weeks** |
| Rust Crates | `axum`/`actix-web`, `tokio`, `tokio-tungstenite`, `serde`, `tower` |

**Subsystems:**

| Subsystem | LOC | Weeks |
|-----------|-----|-------|
| Protocol schemas | 2,000 | 2 |
| HTTP server (REST, webhooks) | 2,500 | 3 |
| WebSocket server (RPC, events) | 1,500 | 2 |
| Server methods (30 handlers) | 7,200 | 5-6 |
| Session management | 700 | 1 |

**Key Challenges:**
- WebSocket challenge/nonce authentication flow
- Hot configuration reloading
- OpenAI-compatible API endpoints (`/v1/chat/completions`)

---

### 3. Channel Integrations
| Metric | Value |
|--------|-------|
| Platforms | 7 (WhatsApp excluded) |
| Core LOC (dock.ts) | 15,208 |
| Complexity | **High** |
| Effort | **10-14 weeks** |

**Per-Channel Breakdown:**

| Channel | Current SDK | Rust Alternative | Weeks |
|---------|-------------|------------------|-------|
| Telegram | `grammy` | `teloxide` | 2 |
| Discord | native API | `serenity`/`twilight` | 2 |
| Slack | `@slack/bolt` | `slack-morphism` | 2 |
| LINE | `@line/bot-sdk` | HTTP wrapper | 1-2 |
| Signal | signal-utils | `libsignal-protocol` | 2-3 |
| iMessage | BlueBubbles proxy | HTTP wrapper | 1 |
| Feishu | `@larksuiteoapi/node-sdk` | HTTP wrapper | 1 |

**Note:** WhatsApp support excluded per scope decision. This removes the highest-risk integration (no Rust equivalent for baileys protocol).

---

### 4. Plugin System
| Metric | Value |
|--------|-------|
| Current LOC | 6,511 |
| Extensions | 31 → rewrite in Rust |
| Complexity | **Medium** |
| Effort | **6-8 weeks** |

**Chosen Approach: Native Rust Plugins**

Per scope decision, all plugins will be native Rust. This simplifies the architecture significantly:

| Component | Crates | Effort |
|-----------|--------|--------|
| Plugin trait definition | - | 1 week |
| Dynamic loading (`libloading`) | `libloading`, `abi_stable` | 2 weeks |
| Plugin registry & lifecycle | - | 2 weeks |
| Extension rewrites (31 plugins) | Various | 2-3 weeks |

**Plugin Trait Design:**
```rust
pub trait OpenClawPlugin: Send + Sync {
    fn id(&self) -> &str;
    fn register(&self, api: &mut PluginApi);
    fn shutdown(&self) {}
}
```

**Benefits of Native Rust:**
- No JS runtime overhead
- Compile-time type safety
- Smaller binary size
- Simpler debugging

---

### 5. AI Agent Runtime
| Metric | Value |
|--------|-------|
| Dependencies | `@mariozechner/pi-ai`, `pi-agent-core`, `pi-coding-agent` |
| Complexity | **Very High** |
| Effort | **16-24 weeks** |

**This is the highest-risk component.**

These are proprietary Mario Zechner libraries deeply integrated throughout:
- LLM provider abstraction (model selection, message conversion, tool formatting)
- Agent execution runtime (context management, tool calling, streaming)
- Coding-specific features (code analysis, file operations)

**Tasks:**
1. Provider trait abstraction (~3 weeks)
2. HTTP clients for each LLM provider (~4 weeks)
3. Agent execution loop (~6-8 weeks)
4. Tool system (~3 weeks)
5. Streaming response handling (~2 weeks)

**Supported Providers to Implement:**
- Anthropic (Claude)
- OpenAI (GPT, Codex)
- AWS Bedrock
- Ollama (local)
- Z.AI
- Venice.ai
- Qwen Portal

---

### 6. Web Control UI Embedding
| Metric | Value |
|--------|-------|
| Current LOC | 21,459 |
| Complexity | Low |
| Effort | **1-2 weeks** |
| Rust Crates | `rust-embed`, `axum` static files |

**Approach:**
1. Keep Lit.js UI unchanged
2. Build with Vite to `dist/control-ui/`
3. Embed in binary: `rust_embed::RustEmbed`
4. Serve via `axum::Router::nest_service`

```rust
#[derive(RustEmbed)]
#[folder = "ui/dist/"]
struct UiAssets;
```

Binary size increase: ~2-5 MB (acceptable).

---

### 7. Native Dependencies Migration

| Dependency | Purpose | Rust Alternative | Effort |
|------------|---------|------------------|--------|
| `@lydell/node-pty` | Terminal PTY | `portable-pty` | 1 week |
| `sharp` | Image processing | `image` crate | 1 week |
| `pdfjs-dist` | PDF parsing | `pdfium-render` | 2 weeks |
| `playwright-core` | Browser automation | `chromiumoxide` | 3 weeks |
| `@matrix-org/matrix-sdk-crypto` | Matrix E2EE | `matrix-sdk-crypto` | 2 weeks |
| `chokidar` | File watching | `notify` | 1 week |
| `croner` | Cron scheduling | `cron` | 1 week |

---

## Total Effort Estimation (Revised)

With WhatsApp excluded and native Rust plugins:

| Component | Min Weeks | Max Weeks |
|-----------|-----------|-----------|
| CLI Bootstrap | 1 | 2 |
| Gateway Server | 10 | 14 |
| Channel Integrations | 10 | 14 |
| Plugin System | 6 | 8 |
| AI Agent Runtime | 16 | 24 |
| UI Embedding | 1 | 2 |
| Native Dependencies | 8 | 10 |
| Testing & Integration | 6 | 10 |
| Documentation | 2 | 3 |
| **Total** | **60 weeks** | **87 weeks** |

**Duration: 12-18 months** (1 FTE) or **6-9 months** (2 FTE)

### Savings from Scope Decisions
- WhatsApp removed: -6-8 weeks (highest-risk item eliminated)
- Native Rust plugins: -4-6 weeks (no JS runtime embedding)
- **Total savings: 10-14 weeks**

---

## Single Binary Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                    openclaw (single binary)                   │
│                        ~15-25 MB                              │
├──────────────────────────────────────────────────────────────┤
│  CLI Layer (clap)                                            │
│    └─ Commands: serve, chat, config, plugin, ...             │
├──────────────────────────────────────────────────────────────┤
│  Gateway Server (axum + tokio)                               │
│    ├─ HTTP endpoints (/v1/chat/completions, webhooks)        │
│    ├─ WebSocket RPC (tokio-tungstenite)                      │
│    └─ Static file server (embedded UI)                       │
├──────────────────────────────────────────────────────────────┤
│  Core Services                                               │
│    ├─ Session Manager (JSONL transcripts)                    │
│    ├─ Channel Router                                         │
│    ├─ Cron Scheduler                                         │
│    └─ Config Hot-Reload (notify)                             │
├──────────────────────────────────────────────────────────────┤
│  LLM Provider Clients (reqwest + serde)                      │
│    └─ Anthropic, OpenAI, Bedrock, Ollama, ...                │
├──────────────────────────────────────────────────────────────┤
│  Agent Runtime                                               │
│    ├─ Execution loop                                         │
│    ├─ Tool registry                                          │
│    └─ Context management                                     │
├──────────────────────────────────────────────────────────────┤
│  Plugin Runtime (wasmtime)                                   │
│    └─ WASM plugin loader & sandbox                           │
├──────────────────────────────────────────────────────────────┤
│  Channel Clients                                             │
│    └─ teloxide, serenity, slack-morphism, ...                │
├──────────────────────────────────────────────────────────────┤
│  Embedded Assets (rust-embed)                                │
│    └─ ui/dist/* (~2-5 MB)                                    │
└──────────────────────────────────────────────────────────────┘
```

---

## Risk Matrix (Revised)

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Agent runtime complexity | High | Critical | Phased migration, extensive testing |
| Timeline overrun | Medium | Medium | MVP-first approach, cut scope |
| Rust ecosystem gaps | Low | Medium | Fallback to HTTP wrappers |
| Performance regression | Low | Low | Benchmark against Node baseline |

**Eliminated Risks:**
- ~~WhatsApp integration gap~~ (excluded from scope)
- ~~Plugin ecosystem breakage~~ (native Rust, clean break)

---

## Recommended Phased Approach (Revised)

### Phase 1: Foundation (Weeks 1-14)
**Goal:** Single binary serving UI with basic gateway.

- [ ] CLI framework with `clap`
- [ ] HTTP/WebSocket server skeleton (`axum` + `tokio-tungstenite`)
- [ ] Configuration system (`serde_yaml`, `json5`)
- [ ] Embed and serve control UI (`rust-embed`)
- [ ] Basic session management (JSONL transcripts)

**Deliverable:** Binary that serves the web UI and handles config.

### Phase 2: LLM Integration (Weeks 15-26)
**Goal:** Functional chat capabilities.

- [ ] Provider abstraction trait
- [ ] Implement: Anthropic, OpenAI, Ollama (priority providers)
- [ ] Streaming response support (SSE)
- [ ] Tool calling framework

**Deliverable:** Working `/v1/chat/completions` API.

### Phase 3: Channel Migration (Weeks 27-40)
**Goal:** Messaging platform support.

- [ ] Channel abstraction trait
- [ ] Implement: Telegram (`teloxide`), Discord (`serenity`), Slack
- [ ] Unified message routing
- [ ] LINE, Signal, iMessage, Feishu

**Deliverable:** Multi-platform messaging (7 channels).

### Phase 4: Agent Runtime (Weeks 41-60)
**Goal:** Full agent capabilities.

- [ ] Agent execution loop
- [ ] Tool registry and execution
- [ ] Context management
- [ ] Coding-specific features

**Deliverable:** Feature parity with pi-agent-core.

### Phase 5: Plugin System (Weeks 61-68)
**Goal:** Native Rust extensibility.

- [ ] Plugin trait and dynamic loading (`libloading`)
- [ ] Plugin registry and lifecycle
- [ ] Rewrite critical extensions in Rust

**Deliverable:** Native Rust plugin architecture.

### Phase 6: Polish (Weeks 69-80)
**Goal:** Production ready.

- [ ] Comprehensive testing
- [ ] Performance optimization
- [ ] Documentation
- [ ] Migration guide from TypeScript version

**Deliverable:** Production-ready single binary.

---

## Key Files to Reference (in OpenClaw repo)

| Component | Critical Files |
|-----------|----------------|
| Entry | `src/entry.ts` |
| Gateway HTTP | `src/gateway/server-http.ts` |
| Gateway WebSocket | `src/gateway/server/ws-connection.ts` |
| Protocol Schemas | `src/gateway/protocol/` |
| Channel Core | `src/channels/dock.ts` (15,208 LOC) |
| Plugin Loader | `src/plugins/loader.ts` |
| Plugin Registry | `src/plugins/registry.ts` |
| Agent Runtime | `@mariozechner/pi-agent-core` (external) |
| Web UI | `ui/` (Lit.js + Vite) |

---

## Recommended Rust Project Structure

```
openclaw-rs/
├── Cargo.toml
├── src/
│   ├── main.rs                 # CLI entry (clap)
│   ├── lib.rs
│   ├── cli/                    # CLI commands
│   ├── gateway/
│   │   ├── mod.rs
│   │   ├── http.rs             # axum HTTP server
│   │   ├── ws.rs               # WebSocket RPC
│   │   ├── protocol.rs         # Frame types
│   │   └── methods/            # RPC handlers
│   ├── channels/
│   │   ├── mod.rs              # Channel trait
│   │   ├── telegram.rs         # teloxide
│   │   ├── discord.rs          # serenity
│   │   ├── slack.rs
│   │   └── ...
│   ├── agent/
│   │   ├── mod.rs              # Agent runtime
│   │   ├── execution.rs        # Execution loop
│   │   ├── tools.rs            # Tool registry
│   │   └── context.rs
│   ├── providers/
│   │   ├── mod.rs              # Provider trait
│   │   ├── anthropic.rs
│   │   ├── openai.rs
│   │   └── ollama.rs
│   ├── plugins/
│   │   ├── mod.rs              # Plugin trait
│   │   ├── loader.rs           # Dynamic loading
│   │   └── registry.rs
│   ├── config/
│   │   └── mod.rs              # serde_yaml config
│   └── session/
│       └── mod.rs              # JSONL transcripts
├── ui/                         # Lit.js UI (unchanged)
│   └── dist/                   # Embedded at compile
└── plugins/                    # Native Rust plugins
    ├── telegram/
    ├── discord/
    └── ...
```

---

## Verification & Testing Strategy

### Unit Testing
- Use `#[tokio::test]` for async tests
- Mock LLM providers with `wiremock`
- Test protocol serialization with `serde_json`

### Integration Testing
- Spin up gateway, verify WebSocket RPC
- Test each channel with mock servers
- End-to-end session transcript validation

### Benchmarking
- Compare startup time vs Node.js version
- Memory usage under load (100 concurrent sessions)
- Message throughput per channel

### CI/CD
- Cross-compile for Linux (x86_64, aarch64), macOS, Windows
- Binary size tracking
- Automated release builds with `cargo-dist`

---

## Conclusion

A full Rust rewrite for single binary deployment is **feasible**:

| Metric | Value |
|--------|-------|
| **Timeline** | 12-18 months (1 FTE) |
| **Rust LOC** | 20,000-28,000 |
| **Binary Size** | ~15-25 MB |
| **Primary Risk** | Agent runtime complexity |

### Key Benefits
- Single binary deployment (no Node.js required)
- Lower memory footprint (~50-80% reduction)
- Faster startup (sub-second)
- No GC pauses under load
- Easier cross-platform distribution

### Scope Simplifications Applied
- WhatsApp excluded (eliminated highest-risk integration)
- Native Rust plugins only (no JS runtime embedding)
- **Total savings: 10-14 weeks**

The plan is ready for implementation.
