# Memory System

The memory system provides long-term memory support for bxnode-bot agents, allowing them to store and recall information across conversations.

## Overview

The memory system uses:
- **Append-only JSONL storage** with soft deletes for durability and auditability
- **In-memory inverted index** for fast keyword search
- **Scope-based access control** to prevent cross-user memory leaks

## Configuration

Add to your `config.yaml`:

```yaml
memory:
  # Enable/disable the memory system
  enabled: true
  # Path to the memory store file (JSONL format)
  store_path: "~/.bxnode-bot/memory.jsonl"
  # Maximum number of results to return from search
  max_results: 5
  # Default TTL for memories in days (0 = no expiry)
  ttl_days: 90
```

## Memory Tools

The agent has access to three memory tools:

### `memory_store`

Store information in long-term memory.

**Input:**
```json
{
  "content": "The user's favorite color is blue",
  "summary": "Color preference",
  "tags": ["preferences", "colors"],
  "importance": 7
}
```

**Fields:**
- `content` (required): The content to remember
- `summary` (optional): A short summary for search results
- `tags` (optional): Tags to help categorize and find this memory
- `importance` (optional, 0-10, default 5): Higher values are prioritized in search results

**Output:**
```json
{
  "stored": true,
  "id": "mem_abc123",
  "message": "Memory stored successfully"
}
```

### `memory_recall`

Search long-term memory for relevant information.

**Input:**
```json
{
  "query": "color preference",
  "limit": 5,
  "min_importance": 5
}
```

**Fields:**
- `query` (required): Search query to find relevant memories
- `limit` (optional, default 5, max 20): Maximum results to return
- `min_importance` (optional, 0-10): Only return memories with at least this importance

**Output:**
```json
{
  "found": true,
  "count": 1,
  "memories": [
    {
      "id": "mem_abc123",
      "score": "0.85",
      "summary": "Color preference",
      "content_preview": "The user's favorite color is blue",
      "tags": ["preferences", "colors"],
      "importance": 7,
      "created_at": 1706985600000
    }
  ]
}
```

### `memory_forget`

Remove a memory by its ID (soft delete).

**Input:**
```json
{
  "id": "mem_abc123"
}
```

**Output:**
```json
{
  "deleted": true,
  "id": "mem_abc123",
  "message": "Memory forgotten"
}
```

## Memory Scoping

Memories are scoped to prevent cross-user access:

- **agent_id**: The agent that owns the memory
- **channel_id**: The messaging channel (telegram, discord, slack)
- **user_id**: The user within the channel

A memory can only be accessed or deleted by requests with a matching scope.

## Search Algorithm

The search uses a scoring formula that combines:
- **Term frequency (50%)**: How many query terms match the content
- **Recency (30%)**: `1 / (1 + age_days)` - newer memories rank higher
- **Importance (20%)**: User-assigned importance level

## Storage Format

Memories are stored in append-only JSONL format:

```jsonl
{"id":"mem_abc123","scope":{"agent_id":"default","channel_id":"telegram","user_id":"user123"},"content":"...","created_at":1706985600000,...}
{"id":"mem_def456","scope":{"agent_id":"default","channel_id":"telegram","user_id":"user123"},"content":"...","created_at":1706985700000,...}
```

Soft deletes append a record with the same ID and a `deleted_at` timestamp.

## Compaction

Over time, the JSONL file may grow with deleted records. Use the `compact()` method to rewrite the file with only active records:

```rust
let removed = store.compact()?;
println!("Compacted {} deleted records", removed);
```

## Best Practices

1. **Use meaningful summaries**: Summaries appear in search results and help the agent decide which memories to use
2. **Tag consistently**: Use consistent tags to help categorize and filter memories
3. **Set appropriate importance**: Reserve high importance (8-10) for critical information
4. **Scope appropriately**: The default scope isolates memories per user per channel

## Architecture

```
┌────────────────────────────────────────────────────────────┐
│                    Memory System                            │
├────────────────────────────────────────────────────────────┤
│  Tools (exposed to LLM)                                    │
│    ├─ memory_store   → store content with metadata         │
│    ├─ memory_recall  → search memories by query            │
│    └─ memory_forget  → soft-delete by ID                   │
├────────────────────────────────────────────────────────────┤
│  MemoryStore                                               │
│    ├─ JSONL file (append-only, soft deletes)               │
│    ├─ In-memory HashMap<id, MemoryRecord>                  │
│    └─ Inverted index HashMap<token, Vec<id>>               │
├────────────────────────────────────────────────────────────┤
│  Scoring (pure Rust)                                       │
│    ├─ Term frequency (keyword matches)                     │
│    ├─ Recency: 1 / (1 + age_days)                          │
│    └─ Importance: importance / 10.0                        │
└────────────────────────────────────────────────────────────┘
```

## Future Enhancements

Planned features (not yet implemented):
- **Progressive disclosure**: `memory_search` for IDs + `memory_get` for full content
- **Auto-archiving**: Archive conversation context when token budget is exceeded
- **TTL enforcement**: Automatic pruning of expired memories
- **Memory CLI**: Commands to inspect, search, and manage memories
