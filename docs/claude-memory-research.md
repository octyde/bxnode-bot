---
summary: "Research on Claude memory MCP plugins and implications for bxnode-bot"
title: "Claude Memory Research"
date: "2026-02-04"
---

# Claude Memory Research

## Scope

This document summarizes patterns from Claude memory MCP plugins and maps them to a concrete long-memory design for bxnode-bot.

## What The Plugins Show

### 1) Durable memory with small, explicit tools

`claude-memory-mcp` exposes three core tools: `memory_store`, `memory_recall`, and `memory_forget`. It stores data locally with SQLite + FTS5 and emphasizes token-aware recall, auto-summarization, entity extraction, and soft deletes with provenance and hybrid relevance scoring. citeturn2view0

### 2) Progressive disclosure for token efficiency

Claude-mem’s MCP tools use a 3-layer workflow: `search` returns a compact index, `timeline` provides contextual history, and `get_observations` fetches full details only for chosen IDs. This pattern is designed to reduce token usage by filtering before fetching details. citeturn3view7

### 3) Context-window caching policies

`memory-mcp` adds explicit context-window caching tools like `archive-context`, `retrieve-context`, `score-relevance`, `create-summary`, and tag-based search. It also describes automatic thresholds such as archiving at 80% context usage and retrieving when usage drops below 30%. citeturn3view1

### 4) Checkpoints, channels, and compaction helpers

`mcp-memory-keeper` focuses on persistent context for coding sessions and includes checkpoints, channels (topic-based organization), smart compaction helpers, file caching with change detection, full-text search, and SQLite-based storage optimized for Claude. citeturn4view0turn3view5

## Design Implications For bxnode-bot

### A) Minimal memory tool surface (start small)

Adopt a compact tool set similar to `claude-memory-mcp`:

- `memory_store`: store a memory with summary, entities, importance, and provenance.
- `memory_recall`: search with token-aware loading and optional filters.
- `memory_forget`: soft-delete with audit trail.

This keeps the initial footprint small while enabling durable recall. citeturn2view0

### B) Token-efficient retrieval pattern

Adopt the progressive disclosure pattern from claude-mem:

- `memory_search`: compact index of matching items
- `memory_timeline`: lightweight context around selected IDs
- `memory_get`: full details for chosen IDs

This avoids dumping full memories into every prompt and keeps token costs low. citeturn3view7

### C) Context-window caching hooks

Add a policy-driven cache layer similar to `memory-mcp`:

- Archive when a session nears token limits
- Retrieve when short on context
- Summarize archived chunks and score relevance

This requires a session token estimator and a background summarization task. citeturn3view1

### D) Persistent store with FTS5 first

Use a local SQLite database with FTS5 for fast keyword search and minimal dependencies. This is common across the plugins and keeps deployments simple. citeturn2view0turn3view5

## Proposed Architecture For bxnode-bot

### 1) New module: `memory`

- Storage: SQLite + FTS5 (table for records, FTS virtual table for content + summary)
- Schema fields: `id`, `type`, `content`, `summary`, `entities`, `importance`, `tags`, `created_at`, `updated_at`, `ttl_days`, `provenance`, `deleted_at`
- Retrieval: keyword search + hybrid scoring (recency, importance, frequency)

### 2) Tool API

Implement tools in `src/agent/tools.rs` and expose them to LLM providers:

- `memory_store`
- `memory_recall`
- `memory_forget`
- Optional: `memory_search`, `memory_timeline`, `memory_get`

### 3) Session integration

- Track token usage per session
- When >80% threshold: archive summarized chunks into memory
- When <30% threshold: recall relevant memory snippets

### 4) Checkpoints and channels

- Add optional `channel` or `project` tags on memory entries
- Add checkpoint snapshots for long tasks
- Support file caching metadata if needed for code workflows

## Configuration Additions (Draft)

Add to `config.example.yaml` and `Config`:

```yaml
memory:
  enabled: true
  store_path: ./data/memory.db
  max_tokens: 25000
  archive_threshold: 0.8
  recall_threshold: 0.3
  ttl_days: 90
```

## Rollout Plan (Phased)

1. Phase 1: SQLite + FTS5 storage, `memory_store` / `memory_recall` / `memory_forget` tools.
2. Phase 2: Token-budgeted recall and compact indexing (search → get).
3. Phase 3: Context-window caching policies and checkpoint support.

## Risks And Mitigations

- Risk: Memory leaks across users or channels
  - Mitigation: Strict scoping by agent + channel + user
- Risk: Excessive token usage during recall
  - Mitigation: Progressive disclosure (search → timeline → get) and hard token caps
- Risk: Memory bloat
  - Mitigation: TTL, compaction, and soft-delete pruning

## Source Notes

This plan is derived from public READMEs and documentation for `claude-memory-mcp`, `memory-mcp`, `mcp-memory-keeper`, and claude-mem’s search workflow guidance. citeturn2view0turn3view1turn4view0turn3view7
