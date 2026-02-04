//! Session module - Conversation session management
//!
//! Sessions are stored as JSONL transcript files.

#[cfg(test)]
mod tests;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Session ID
    pub id: String,

    /// Channel ID (e.g., "telegram", "discord")
    pub channel: String,

    /// Chat/conversation ID within the channel
    pub chat_id: String,

    /// Agent ID handling this session
    pub agent_id: String,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last activity timestamp
    pub updated_at: DateTime<Utc>,

    /// Session title (optional)
    pub title: Option<String>,

    /// Session metadata
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Transcript entry (one line in JSONL)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptEntry {
    /// Entry timestamp
    pub timestamp: DateTime<Utc>,

    /// Entry type
    #[serde(flatten)]
    pub entry_type: TranscriptEntryType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TranscriptEntryType {
    #[serde(rename = "user")]
    User {
        user_id: String,
        content: String,
    },

    #[serde(rename = "assistant")]
    Assistant {
        content: String,
        model: String,
    },

    #[serde(rename = "tool_use")]
    ToolUse {
        tool: String,
        input: serde_json::Value,
    },

    #[serde(rename = "tool_result")]
    ToolResult {
        tool: String,
        success: bool,
        output: String,
    },

    #[serde(rename = "system")]
    System {
        message: String,
    },
}

/// Session manager
pub struct SessionManager {
    /// Base directory for session storage
    base_dir: PathBuf,
}

impl SessionManager {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Get session directory path
    fn session_dir(&self, session_id: &str) -> PathBuf {
        self.base_dir.join(session_id)
    }

    /// Get session metadata file path
    fn metadata_path(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id).join("session.json")
    }

    /// Get transcript file path
    fn transcript_path(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id).join("transcript.jsonl")
    }

    /// Create a new session
    pub async fn create(&self, session: Session) -> anyhow::Result<()> {
        let dir = self.session_dir(&session.id);
        tokio::fs::create_dir_all(&dir).await?;

        let metadata = serde_json::to_string_pretty(&session)?;
        tokio::fs::write(self.metadata_path(&session.id), metadata).await?;

        Ok(())
    }

    /// Load session metadata
    pub async fn load(&self, session_id: &str) -> anyhow::Result<Option<Session>> {
        let path = self.metadata_path(session_id);
        if !path.exists() {
            return Ok(None);
        }

        let content = tokio::fs::read_to_string(&path).await?;
        let session: Session = serde_json::from_str(&content)?;
        Ok(Some(session))
    }

    /// Append entry to transcript
    pub async fn append(&self, session_id: &str, entry: TranscriptEntry) -> anyhow::Result<()> {
        use tokio::io::AsyncWriteExt;

        let path = self.transcript_path(session_id);
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;

        let line = serde_json::to_string(&entry)?;
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;

        Ok(())
    }

    /// List all sessions
    pub async fn list(&self) -> anyhow::Result<Vec<Session>> {
        let mut sessions = vec![];

        let mut entries = tokio::fs::read_dir(&self.base_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                if let Some(session) = self.load(entry.file_name().to_str().unwrap()).await? {
                    sessions.push(session);
                }
            }
        }

        Ok(sessions)
    }
}
