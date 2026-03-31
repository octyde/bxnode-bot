//! Memory module - Long-term memory storage and retrieval
//!
//! This module provides persistent memory capabilities for the agent,
//! allowing it to store, recall, and forget information across sessions.
//!
//! # Architecture
//!
//! ```text
//! ┌────────────────────────────────────────────────────────┐
//! │                    Memory System                        │
//! ├────────────────────────────────────────────────────────┤
//! │  MemoryStore                                           │
//! │    ├─ JSONL file (append-only, soft deletes)           │
//! │    ├─ In-memory HashMap<id, MemoryRecord>              │
//! │    └─ Inverted index for search                        │
//! ├────────────────────────────────────────────────────────┤
//! │  Scoring (keyword + recency + importance)              │
//! └────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use bxnode_bot::memory::{MemoryStore, MemoryRecord, MemoryScope};
//!
//! let mut store = MemoryStore::open("./data/memory.jsonl")?;
//!
//! // Store a memory
//! let record = MemoryRecord::new(scope, "Important fact", Some("summary"));
//! store.store(record)?;
//!
//! // Search memories
//! let results = store.search("important", &scope, 5);
//! ```

pub mod embeddings;
pub mod search;
pub mod store;

#[cfg(test)]
mod tests;

pub use search::MemorySearchResult;
pub use store::MemoryStore;

use serde::{Deserialize, Serialize};

/// A memory record stored in the memory system
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MemoryRecord {
    /// Unique identifier for this memory
    pub id: String,

    /// Scope constraints (who can access this memory)
    pub scope: MemoryScope,

    /// The main content of the memory
    pub content: String,

    /// Optional short summary for quick retrieval
    pub summary: Option<String>,

    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,

    /// Importance level (0-10, higher = more important)
    #[serde(default = "default_importance")]
    pub importance: u8,

    /// Creation timestamp (epoch milliseconds)
    pub created_at: i64,

    /// Last update timestamp (epoch milliseconds)
    pub updated_at: i64,

    /// Time-to-live in days (None = forever)
    #[serde(default)]
    pub ttl_days: Option<u32>,

    /// Soft delete marker (epoch milliseconds when deleted)
    #[serde(default)]
    pub deleted_at: Option<i64>,

    /// Source context (where this memory came from)
    #[serde(default)]
    pub provenance: Option<String>,
}

fn default_importance() -> u8 {
    5
}

impl MemoryRecord {
    /// Create a new memory record
    pub fn new(scope: MemoryScope, content: impl Into<String>, summary: Option<String>) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            scope,
            content: content.into(),
            summary,
            tags: Vec::new(),
            importance: default_importance(),
            created_at: now,
            updated_at: now,
            ttl_days: None,
            deleted_at: None,
            provenance: None,
        }
    }

    /// Create a memory record with all fields specified
    pub fn builder() -> MemoryRecordBuilder {
        MemoryRecordBuilder::default()
    }

    /// Check if this memory is deleted
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }

    /// Check if this memory has expired based on TTL
    pub fn is_expired(&self, now_ms: i64) -> bool {
        if let Some(ttl_days) = self.ttl_days {
            let age_ms = now_ms - self.created_at;
            let ttl_ms = ttl_days as i64 * 24 * 60 * 60 * 1000;
            age_ms > ttl_ms
        } else {
            false
        }
    }

    /// Get a preview of the content (first N chars)
    pub fn content_preview(&self, max_len: usize) -> String {
        if self.content.len() <= max_len {
            self.content.clone()
        } else {
            format!("{}...", &self.content[..max_len])
        }
    }
}

/// Builder for MemoryRecord
#[derive(Default)]
pub struct MemoryRecordBuilder {
    scope: Option<MemoryScope>,
    content: Option<String>,
    summary: Option<String>,
    tags: Vec<String>,
    importance: u8,
    ttl_days: Option<u32>,
    provenance: Option<String>,
}

impl MemoryRecordBuilder {
    pub fn scope(mut self, scope: MemoryScope) -> Self {
        self.scope = Some(scope);
        self
    }

    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = Some(content.into());
        self
    }

    pub fn summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    pub fn summary_opt(mut self, summary: Option<String>) -> Self {
        self.summary = summary;
        self
    }

    pub fn tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn importance(mut self, importance: u8) -> Self {
        self.importance = importance.min(10);
        self
    }

    pub fn ttl_days(mut self, days: u32) -> Self {
        self.ttl_days = Some(days);
        self
    }

    pub fn provenance(mut self, provenance: impl Into<String>) -> Self {
        self.provenance = Some(provenance.into());
        self
    }

    pub fn build(self) -> anyhow::Result<MemoryRecord> {
        let scope = self
            .scope
            .ok_or_else(|| anyhow::anyhow!("scope is required"))?;
        let content = self
            .content
            .ok_or_else(|| anyhow::anyhow!("content is required"))?;

        let now = chrono::Utc::now().timestamp_millis();

        Ok(MemoryRecord {
            id: uuid::Uuid::new_v4().to_string(),
            scope,
            content,
            summary: self.summary,
            tags: self.tags,
            importance: self.importance,
            created_at: now,
            updated_at: now,
            ttl_days: self.ttl_days,
            deleted_at: None,
            provenance: self.provenance,
        })
    }
}

/// Scope constraints for memory access
///
/// Memories are scoped to prevent cross-user/cross-channel leaks.
/// All fields must match for a memory to be accessible.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryScope {
    /// Agent identifier (required)
    #[serde(default = "default_agent_id")]
    pub agent_id: String,

    /// Channel identifier (optional, None = global to agent)
    #[serde(default)]
    pub channel_id: Option<String>,

    /// User identifier (optional, None = shared within channel)
    #[serde(default)]
    pub user_id: Option<String>,

    /// Session identifier (optional, None = persistent across sessions)
    #[serde(default)]
    pub session_id: Option<String>,
}

fn default_agent_id() -> String {
    "default".to_string()
}

impl MemoryScope {
    /// Create a new scope with just an agent ID
    pub fn agent(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            ..Default::default()
        }
    }

    /// Create a scope for a specific channel
    pub fn channel(agent_id: impl Into<String>, channel_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            channel_id: Some(channel_id.into()),
            ..Default::default()
        }
    }

    /// Create a scope for a specific user in a channel
    pub fn user(
        agent_id: impl Into<String>,
        channel_id: impl Into<String>,
        user_id: impl Into<String>,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            channel_id: Some(channel_id.into()),
            user_id: Some(user_id.into()),
            session_id: None,
        }
    }

    /// Check if this scope matches another (for access control)
    ///
    /// A memory matches if all non-None fields in the query scope
    /// match the corresponding fields in the memory's scope.
    pub fn matches(&self, query: &MemoryScope) -> bool {
        // Agent ID must always match
        if self.agent_id != query.agent_id {
            return false;
        }

        // If query specifies channel, it must match
        if let Some(ref query_channel) = query.channel_id {
            if self.channel_id.as_ref() != Some(query_channel) {
                return false;
            }
        }

        // If query specifies user, it must match
        if let Some(ref query_user) = query.user_id {
            if self.user_id.as_ref() != Some(query_user) {
                return false;
            }
        }

        // If query specifies session, it must match
        if let Some(ref query_session) = query.session_id {
            if self.session_id.as_ref() != Some(query_session) {
                return false;
            }
        }

        true
    }
}
