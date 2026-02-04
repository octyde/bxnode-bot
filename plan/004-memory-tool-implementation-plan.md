# Memory Tool Implementation Plan (Zero New Dependencies)

Date: 2026-02-04

## Goal

Add long‑term memory support to bxnode-bot using only existing dependencies and pure Rust (std + current crates). No new crates.

## Constraints

- Do not add new dependencies.
- Prefer stdlib + existing `serde`, `serde_json`, `chrono`, `tokio`.
- Keep the initial design simple and reversible.

## Summary Of Approach

Implement an append‑only JSONL memory store with a lightweight in‑memory index built at startup. Provide a minimal tool surface (`memory_store`, `memory_recall`, `memory_forget`) and an optional progressive disclosure flow (`memory_search`, `memory_get`). Use simple scoring (keyword frequency + recency + importance) and strict scoping (agent + channel + user) to prevent cross‑context leaks.

## Phase 1: Core Storage + Tools (MVP)

### 1) Add a `memory` module

Files:
- `src/memory/mod.rs`
- `src/memory/store.rs`
- `src/memory/search.rs`
- `src/memory/tests.rs`

Responsibilities:
- `store.rs`: append‑only JSONL write, soft delete, load on startup
- `search.rs`: in‑memory index + simple scoring

### 2) Memory record schema (JSONL)

File: `src/memory/mod.rs`

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MemoryRecord {
    pub id: String,
    pub scope: MemoryScope,
    pub content: String,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    pub importance: u8,           // 0-10
    pub created_at: i64,          // epoch ms
    pub updated_at: i64,
    pub ttl_days: Option<u32>,
    pub deleted_at: Option<i64>,
    pub provenance: Option<String>
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MemoryScope {
    pub agent_id: String,
    pub channel_id: Option<String>,
    pub user_id: Option<String>,
    pub session_id: Option<String>
}
```

### 3) Storage format and behavior

- Append‑only JSONL file: `./data/memory.jsonl` (configurable)
- Each write is a single JSON line.
- Deletion is soft delete: append a record with `deleted_at` set and same `id`.
- On startup, rebuild latest state by reading the file sequentially.

### 4) In‑memory index (pure Rust)

- Build a `HashMap<String, MemoryRecord>` keyed by `id`.
- Build an inverted index: `HashMap<String, Vec<String>>` mapping token → memory IDs.
- Tokenization: lowercase, split on non‑alphanumeric, drop tokens < 3 chars.
- Simple scoring:
  - term frequency
  - recency: `1 / (1 + age_days)`
  - importance: `importance / 10.0`

### 5) Tool surface (MVP)

Extend `src/agent/tools.rs` with three tools:

- `memory_store` input:
  - `content` (string, required)
  - `summary` (string, optional)
  - `tags` (string[], optional)
  - `importance` (0–10, optional, default 5)
  - `scope` object (agent/channel/user/session)
- `memory_recall` input:
  - `query` (string)
  - `limit` (int, default 5)
  - `scope` (same as above)
  - `min_importance` (optional)
- `memory_forget` input:
  - `id` (string)
  - `scope`

Tool results return JSON strings with IDs, summaries, and snippets.

### 6) Config additions

Update `src/config/mod.rs` and `config.example.yaml`:

```yaml
memory:
  enabled: true
  store_path: ./data/memory.jsonl
  max_results: 5
  min_token_len: 3
  ttl_days: 90
```

### 7) Tests

- `src/memory/tests.rs`: store/load round‑trip, search ranking, soft delete.
- `src/agent/tools_tests.rs`: memory tool end‑to‑end (store → recall → forget).

## Phase 2: Progressive Disclosure (Token Efficiency)

Add optional `memory_search` and `memory_get` tools:

- `memory_search`: returns IDs + score + short summary only.
- `memory_get`: returns full record for selected IDs.

This allows the model to pull details only when needed.

## Phase 3: Session‑aware Archiving

Implement a simple token budget policy using existing session tracking:

- When session size > threshold, summarize last N turns and store as memory.
- When new request arrives, auto‑recall top 3 memories by score.

Implementation notes:
- No external tokenizer: estimate tokens as `chars / 4` or `words * 1.3`.
- Keep behavior behind a config flag (`memory.auto_recall` / `memory.auto_archive`).

## Integration Points

- `src/agent/execution.rs`: hook to inject recalled memories into context.
- `src/session/mod.rs`: optional session metadata to drive auto‑archiving.
- `src/agent/tools.rs`: register memory tools in `ToolRegistry::with_builtins()`.

## Risks And Mitigations

- Memory leaks across users: require `scope` and enforce exact match.
- Token bloat: limit recall count and cap memory snippet length.
- Unbounded file growth: TTL + compaction command (`memory.compact`).

## Deliverables Checklist

1. `src/memory/*` module with JSONL store + index
2. Config schema for memory
3. Tools: store/recall/forget
4. Tests for store/search/tools
5. Documentation in `docs/memory.md`

## Optional Enhancements (No New Dependencies)

- `memory.compact` CLI: rewrite JSONL to keep latest state only.
- `memory.export` / `memory.import` CLI.
- `memory.stats` tool for debugging (counts by tag / scope).

## Out Of Scope For MVP

- Vector embeddings
- SQLite/FTS5
- External long‑term vector DBs

