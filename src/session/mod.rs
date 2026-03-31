//! Session module - Conversation session management
//!
//! Sessions are stored as JSONL transcript files, organized by project.
//! Storage layout: `base_dir/project/session_id/session.json + transcript.jsonl`

#[cfg(test)]
mod tests;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

    /// Project this session belongs to
    #[serde(default = "default_project")]
    pub project: String,

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

fn default_project() -> String {
    "default".to_string()
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

/// Metrics returned from transcript truncation
#[derive(Debug, Clone)]
pub struct TruncationMetrics {
    /// Number of entries removed
    pub entries_removed: usize,
    /// Transcript file size before truncation
    pub bytes_before: u64,
    /// Transcript file size after truncation
    pub bytes_after: u64,
}

/// Session manager — handles persistent session storage organized by project
pub struct SessionManager {
    /// Base directory for session storage
    base_dir: PathBuf,
}

impl SessionManager {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Get session directory path (project-aware)
    fn session_dir(&self, project: &str, session_id: &str) -> PathBuf {
        self.base_dir.join(project).join(session_id)
    }

    /// Get session metadata file path
    fn metadata_path(&self, project: &str, session_id: &str) -> PathBuf {
        self.session_dir(project, session_id).join("session.json")
    }

    /// Get transcript file path
    fn transcript_path(&self, project: &str, session_id: &str) -> PathBuf {
        self.session_dir(project, session_id).join("transcript.jsonl")
    }

    /// Ensure base directory exists
    pub async fn ensure_base_dir(&self) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(&self.base_dir).await?;
        // Ensure default project directory exists
        tokio::fs::create_dir_all(self.base_dir.join("default")).await?;
        Ok(())
    }

    /// Create a new session
    pub async fn create(&self, session: &Session) -> anyhow::Result<()> {
        let dir = self.session_dir(&session.project, &session.id);
        tokio::fs::create_dir_all(&dir).await?;

        let metadata = serde_json::to_string_pretty(session)?;
        tokio::fs::write(self.metadata_path(&session.project, &session.id), metadata).await?;

        Ok(())
    }

    /// Update session metadata
    pub async fn update(&self, session: &Session) -> anyhow::Result<()> {
        let metadata = serde_json::to_string_pretty(session)?;
        tokio::fs::write(self.metadata_path(&session.project, &session.id), metadata).await?;
        Ok(())
    }

    /// Load session metadata by scanning all projects
    pub async fn load(&self, session_id: &str) -> anyhow::Result<Option<Session>> {
        // Scan all project directories to find the session
        let projects = self.list_projects().await.unwrap_or_default();
        for project in projects {
            let path = self.metadata_path(&project, session_id);
            if path.exists() {
                let content = tokio::fs::read_to_string(&path).await?;
                let session: Session = serde_json::from_str(&content)?;
                return Ok(Some(session));
            }
        }
        Ok(None)
    }

    /// Load session metadata from a known project
    pub async fn load_from_project(&self, project: &str, session_id: &str) -> anyhow::Result<Option<Session>> {
        let path = self.metadata_path(project, session_id);
        if !path.exists() {
            return Ok(None);
        }
        let content = tokio::fs::read_to_string(&path).await?;
        let session: Session = serde_json::from_str(&content)?;
        Ok(Some(session))
    }

    /// Delete a session
    pub async fn delete(&self, project: &str, session_id: &str) -> anyhow::Result<()> {
        let dir = self.session_dir(project, session_id);
        if dir.exists() {
            tokio::fs::remove_dir_all(&dir).await?;
        }
        Ok(())
    }

    /// Append entry to transcript
    pub async fn append(&self, project: &str, session_id: &str, entry: &TranscriptEntry) -> anyhow::Result<()> {
        use tokio::io::AsyncWriteExt;

        let path = self.transcript_path(project, session_id);
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;

        let line = serde_json::to_string(entry)?;
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;

        Ok(())
    }

    /// Load all transcript entries for a session
    pub async fn load_transcript(&self, project: &str, session_id: &str) -> anyhow::Result<Vec<TranscriptEntry>> {
        let path = self.transcript_path(project, session_id);
        if !path.exists() {
            return Ok(vec![]);
        }

        let content = tokio::fs::read_to_string(&path).await?;
        let mut entries = vec![];
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<TranscriptEntry>(line) {
                Ok(entry) => entries.push(entry),
                Err(e) => {
                    tracing::warn!("Skipping malformed transcript line: {}", e);
                }
            }
        }
        Ok(entries)
    }

    /// Truncate transcript by removing entries older than `keep_after`.
    /// System entries are always preserved. Writes atomically via temp file + rename.
    pub async fn truncate_transcript(
        &self,
        project: &str,
        session_id: &str,
        keep_after: DateTime<Utc>,
    ) -> anyhow::Result<TruncationMetrics> {
        use tokio::io::AsyncWriteExt;

        let path = self.transcript_path(project, session_id);
        if !path.exists() {
            return Ok(TruncationMetrics {
                entries_removed: 0,
                bytes_before: 0,
                bytes_after: 0,
            });
        }

        let bytes_before = tokio::fs::metadata(&path).await?.len();
        let entries = self.load_transcript(project, session_id).await?;
        let total = entries.len();

        // Keep entries that are:
        // - Newer than or equal to keep_after, OR
        // - System entries (always preserved)
        let kept: Vec<&TranscriptEntry> = entries
            .iter()
            .filter(|e| {
                e.timestamp >= keep_after
                    || matches!(e.entry_type, TranscriptEntryType::System { .. })
            })
            .collect();

        let entries_removed = total - kept.len();
        if entries_removed == 0 {
            return Ok(TruncationMetrics {
                entries_removed: 0,
                bytes_before,
                bytes_after: bytes_before,
            });
        }

        // Write kept entries to temp file, then atomic rename
        let tmp_path = path.with_extension("jsonl.tmp");
        let mut file = tokio::fs::File::create(&tmp_path).await?;

        for entry in &kept {
            let line = serde_json::to_string(entry)?;
            file.write_all(line.as_bytes()).await?;
            file.write_all(b"\n").await?;
        }
        file.flush().await?;
        drop(file);

        tokio::fs::rename(&tmp_path, &path).await?;
        let bytes_after = tokio::fs::metadata(&path).await?.len();

        tracing::info!(
            "Truncated transcript {}/{}: removed {} entries ({} -> {} bytes)",
            project,
            session_id,
            entries_removed,
            bytes_before,
            bytes_after,
        );

        Ok(TruncationMetrics {
            entries_removed,
            bytes_before,
            bytes_after,
        })
    }

    /// List all sessions across all projects
    pub async fn list(&self) -> anyhow::Result<Vec<Session>> {
        let mut sessions = vec![];
        let projects = self.list_projects().await?;

        for project in projects {
            let project_dir = self.base_dir.join(&project);
            let mut entries = match tokio::fs::read_dir(&project_dir).await {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            while let Some(entry) = entries.next_entry().await? {
                if entry.file_type().await?.is_dir() {
                    let session_id = entry.file_name().to_string_lossy().to_string();
                    if let Ok(Some(session)) = self.load_from_project(&project, &session_id).await {
                        sessions.push(session);
                    }
                }
            }
        }

        Ok(sessions)
    }

    /// List sessions for a specific channel and chat_id
    pub async fn list_for_chat(&self, channel: &str, chat_id: &str) -> anyhow::Result<Vec<Session>> {
        let all = self.list().await?;
        Ok(all
            .into_iter()
            .filter(|s| s.channel == channel && s.chat_id == chat_id)
            .collect())
    }

    /// List all project names
    pub async fn list_projects(&self) -> anyhow::Result<Vec<String>> {
        let mut projects = vec![];

        if !self.base_dir.exists() {
            return Ok(projects);
        }

        let mut entries = tokio::fs::read_dir(&self.base_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                projects.push(name);
            }
        }

        // Ensure "default" is always first
        projects.sort();
        if let Some(pos) = projects.iter().position(|p| p == "default") {
            projects.remove(pos);
            projects.insert(0, "default".to_string());
        }

        Ok(projects)
    }

    /// Create a new project directory
    pub async fn create_project(&self, name: &str) -> anyhow::Result<()> {
        let dir = self.base_dir.join(name);
        tokio::fs::create_dir_all(&dir).await?;
        Ok(())
    }

    /// Delete a project and all its sessions
    pub async fn delete_project(&self, name: &str) -> anyhow::Result<()> {
        if name == "default" {
            anyhow::bail!("Cannot delete the default project");
        }
        let dir = self.base_dir.join(name);
        if dir.exists() {
            tokio::fs::remove_dir_all(&dir).await?;
        }
        Ok(())
    }

    /// Count sessions in a project
    pub async fn count_sessions_in_project(&self, project: &str) -> anyhow::Result<usize> {
        let project_dir = self.base_dir.join(project);
        if !project_dir.exists() {
            return Ok(0);
        }

        let mut count = 0;
        let mut entries = tokio::fs::read_dir(&project_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                count += 1;
            }
        }
        Ok(count)
    }
}

/// Tracks the active session for each "channel:chat_id" key.
/// Persisted to disk so it survives server restarts.
pub struct ActiveSessionMap {
    inner: HashMap<String, ActiveSessionInfo>,
    path: PathBuf,
}

/// Info about the active session for a chat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSessionInfo {
    pub session_id: String,
    pub project: String,
}

impl ActiveSessionMap {
    /// Load from disk, or create empty if file doesn't exist
    pub fn load(path: PathBuf) -> Self {
        let inner = if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(_) => HashMap::new(),
            }
        } else {
            HashMap::new()
        };
        Self { inner, path }
    }

    /// Get active session info for a chat
    pub fn get(&self, key: &str) -> Option<&ActiveSessionInfo> {
        self.inner.get(key)
    }

    /// Set active session for a chat and persist
    pub fn set(&mut self, key: String, info: ActiveSessionInfo) {
        self.inner.insert(key, info);
        self.save();
    }

    /// Number of active sessions
    pub fn count(&self) -> usize {
        self.inner.len()
    }

    /// Remove active session mapping and persist
    pub fn remove(&mut self, key: &str) {
        self.inner.remove(key);
        self.save();
    }

    /// Persist to disk
    fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_string_pretty(&self.inner) {
            let _ = std::fs::write(&self.path, content);
        }
    }
}
