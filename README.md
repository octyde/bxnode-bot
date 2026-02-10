# bxnode-bot

**bxnode-bot** is a standalone, zero-dependency AI agent gateway built in Rust. It ships as a
**single static binary** that converts unstructured inputs (messages, webhooks, schedules) into
deterministic, auditable execution flows — no database, no Redis, no sidecars required.

All persistence (sessions, memory, cron state) is file-based (JSONL/JSON), making deployment
as simple as copying a binary and a config file.

---

## What This Is

- A single-binary agent runtime and gateway — zero runtime dependencies
- Multi-channel message ingress (Telegram, Discord, Slack, LINE, Signal, Feishu, HTTP, WebSocket)
- Multi-provider LLM execution engine (Anthropic, OpenAI, Ollama)
- Cron- and memory-aware execution environment
- Plugin-extensible and skill-extensible at runtime
- File-based persistence — no external storage required
- Deterministic and auditable by design

---

## High-Level Architecture

```
┌────────────────────────────────────────────────────────────────┐
│                   bxnode-bot (single binary)                    │
│                       ~7.7 MB (release)                        │
├────────────────────────────────────────────────────────────────┤
│  CLI Layer (clap)                                              │
│    └─ serve | config | cron | memory | plugin | skill | version│
├────────────────────────────────────────────────────────────────┤
│  Gateway Server (axum + tokio)                                 │
│    ├─ HTTP (OpenAI-compatible API)                             │
│    ├─ WebSocket RPC                                            │
│    └─ Health / Stats endpoints                                 │
├────────────────────────────────────────────────────────────────┤
│  Core Services                                                 │
│    ├─ Session Manager (JSONL transcripts)                      │
│    ├─ Channel Router                                           │
│    ├─ Cron Scheduler (first-class primitive)                   │
│    └─ Config Manager                                           │
├────────────────────────────────────────────────────────────────┤
│  Agent Runtime                                                 │
│    ├─ Explicit execution loop                                  │
│    ├─ Tool registry (JSON schema-based)                        │
│    ├─ Context + long-term memory                               │
│    └─ Skills (OpenClaw / Agent Skills)                         │
├────────────────────────────────────────────────────────────────┤
│  LLM Providers                                                 │
│    └─ Anthropic | OpenAI | Ollama                              │
├────────────────────────────────────────────────────────────────┤
│  Channels                                                      │
│    └─ Telegram | Discord | Slack | LINE | Signal | Feishu      │
├────────────────────────────────────────────────────────────────┤
│  Plugin System                                                 │
│    └─ Native Rust plugins with hooks + tools                   │
└────────────────────────────────────────────────────────────────┘
```

---

## Design Principles

bxnode-bot is built around a small set of non-negotiable principles:

1. **Single Binary, Zero Dependencies**
   - No sidecars, no databases, no runtime dependency sprawl
   - File-based persistence (JSONL/JSON) — deploy anywhere

2. **Explicit Agent Loop**
   - Context → tools → execution → response
   - No hidden control flow

3. **Config-First UX**
   - One config file controls all subsystems
   - CLI can render effective runtime config

4. **Cron as a First-Class Primitive**
   - Scheduling is part of the core, not an afterthought

5. **Clear Module Boundaries**
   - Each module exposes a single public interface
   - Guarded by boundary tests

6. **Auditability Over Cleverness**
   - JSONL transcripts
   - Deterministic execution paths
   - Observable internal state

---

## Getting Started

### Prerequisites

- Rust toolchain (1.70+)

### Build

```bash
# Default build (core channels only)
cargo build --release

# Build with specific channels
cargo build --release --features "channel-telegram,channel-discord"

# Build with all channels
cargo build --release --features full
```

Available feature flags: `channel-telegram`, `channel-discord`, `channel-slack`, `channel-line`, `channel-signal`, `channel-feishu`, `full` (all channels).

### Configure

```bash
cp config.example.yaml config.yaml
# Edit config.yaml with your provider API keys and channel tokens
```

See `config.example.yaml` for all options or `docs/configuration.md` for the full reference.

### Run

```bash
bxnode-bot serve

# Or with custom config / host / port
bxnode-bot serve --config /path/to/config.yaml --host 127.0.0.1 --port 8080
```

---

## Module Map

Each module has a single public entrypoint:

| Module       | Responsibility                                   |
|--------------|--------------------------------------------------|
| `agent`      | Execution loop, context, tool invocation          |
| `channels`   | Channel adapters and registry                    |
| `cli`        | CLI surface and subcommands                      |
| `config`     | Config schema and loading                        |
| `cron`       | Cron scheduler and persistence                   |
| `gateway`    | HTTP and WebSocket server                        |
| `memory`     | Long-term memory store and search                |
| `plugins`    | Plugin lifecycle, hooks, and registry            |
| `providers`  | LLM provider abstraction and clients             |
| `session`    | Session storage and persistence                  |
| `skills`     | Agent Skills (OpenClaw) loader and registry      |
| `skillgen`   | Skill code generation and sync                   |

Detailed ownership and boundaries are documented in
`docs/module-map.md`.

---

## Features

### Messaging & Ingress
- Telegram, Discord, Slack
- LINE, Signal, Feishu
- HTTP (OpenAI-compatible `/v1/chat/completions`)
- WebSocket RPC

### LLM Providers
- Anthropic
- OpenAI
- Ollama (local)

Supports streaming (SSE) and tool calling.

### Agent Runtime
- Explicit execution loop
- JSON-schema-based tools
- Conversation-scoped context
- Long-term memory with search

### Memory System
- Append-only JSONL storage
- Soft deletes + compaction
- In-memory inverted index
- Agent-accessible memory tools:
  - `memory_store`
  - `memory_recall`
  - `memory_forget`

### Cron Scheduler
- First-class scheduling primitive
- Configurable via YAML
- Managed via CLI:
  - `cron list`
  - `cron add`
  - `cron remove`
  - `cron run`

### Skills System (OpenClaw)
- Markdown-based agent instructions following the [Agent Skills](https://agentskills.io) spec
- Skill directories: global (`~/.bxnode/skills`) and workspace (`./skills`)
- Sync from GitHub repositories
- CLI management:
  - `skill list`, `skill info`, `skill search`
  - `skill install`, `skill update`, `skill sync`
  - `skill enable`, `skill disable`

### Plugin System
- Native Rust plugins
- Async lifecycle (`on_load`, `on_unload`)
- Hook system (8 event types)
- Plugins can expose tools and hooks
- Runtime enable/disable

### Observability
- `/health`
- `/health/detailed`
- `/stats`
- Subsystem-level status reporting

---

## Configuration

All behavior is controlled via a single config file.

See:
- `config.example.yaml`
- `docs/configuration.md`

You can inspect the effective runtime configuration with:
```bash
bxnode-bot config show
```

---

## CLI Overview

```bash
bxnode-bot serve                                    # Start the server
bxnode-bot config show|validate|init                # Configuration management
bxnode-bot cron list|add|remove|run|runs            # Cron scheduler
bxnode-bot memory list|search|stats|get|delete|compact  # Memory system
bxnode-bot plugin list|info|enable|disable          # Plugin management
bxnode-bot skill list|info|search|install|sync|enable|disable  # Skills
bxnode-bot version                                  # Version info
```

---

## Codebase Stats

| Metric                | Value                 |
| --------------------- | --------------------- |
| Rust LOC              | ~20,000               |
| Tests                 | 300+                  |
| Binary size (release) | ~7.7 MB               |
| Providers             | 3                     |
| Channels              | 6                     |
| Skills                | OpenClaw-compatible   |
| Plugins               | Built-in + extensible |
| HTTP endpoints        | 13                    |

---

## Status

**Production-ready.**

All planned phases (1-8) are complete:

* Module boundaries enforced
* Cron integrated and tested
* Memory system implemented
* Plugin system operational
* Skills system operational
* Streaming supported across channels
* Comprehensive test coverage

---

## Relationship to BXNode

bxnode-bot works as a **standalone** AI agent gateway. It is also used as a core
execution component within [BXNode](https://bxnode.com), where it handles message
ingestion, agent workflow execution, and auditable execution history.

This repository exposes the **engine**, not the business logic.

---

## License

MIT

---

## Maintained by

**Octyde**
[https://octyde.com](https://octyde.com)

BXNode product: [https://bxnode.com](https://bxnode.com)
