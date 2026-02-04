//! Cron job persistence store

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::CronJob;

/// Store entry wrapper with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronEntry {
    /// The cron job
    pub job: CronJob,

    /// When the entry was created
    pub created_at: DateTime<Utc>,

    /// When the entry was last updated
    pub updated_at: DateTime<Utc>,
}

impl CronEntry {
    /// Create a new entry from a job
    pub fn new(job: CronJob) -> Self {
        let now = Utc::now();
        Self {
            job,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Store format (versioned for migrations)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoreData {
    /// Schema version
    version: u32,

    /// Job entries
    entries: Vec<CronEntry>,
}

impl Default for StoreData {
    fn default() -> Self {
        Self {
            version: 1,
            entries: Vec::new(),
        }
    }
}

/// Persistent cron job store
pub struct CronStore {
    path: PathBuf,
    data: StoreData,
}

impl CronStore {
    /// Create or load a store from a file path
    pub fn open<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();

        let data = if path.exists() {
            let content = fs::read_to_string(&path)?;
            serde_json::from_str(&content)?
        } else {
            StoreData::default()
        };

        Ok(Self { path, data })
    }

    /// Create an in-memory store (for testing)
    pub fn in_memory() -> Self {
        Self {
            path: PathBuf::new(),
            data: StoreData::default(),
        }
    }

    /// Save the store to disk
    pub fn flush(&self) -> anyhow::Result<()> {
        if self.path.as_os_str().is_empty() {
            return Ok(()); // In-memory store
        }

        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(&self.data)?;
        fs::write(&self.path, content)?;

        Ok(())
    }

    /// Get the store path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Save or update a job
    pub fn save(&mut self, job: &CronJob) -> anyhow::Result<()> {
        let now = Utc::now();

        // Find existing entry or create new one
        if let Some(entry) = self.data.entries.iter_mut().find(|e| e.job.id == job.id) {
            entry.job = job.clone();
            entry.updated_at = now;
        } else {
            self.data.entries.push(CronEntry {
                job: job.clone(),
                created_at: now,
                updated_at: now,
            });
        }

        self.flush()
    }

    /// Remove a job by ID
    pub fn remove(&mut self, id: &str) -> anyhow::Result<Option<CronEntry>> {
        let idx = self.data.entries.iter().position(|e| e.job.id == id);

        let entry = idx.map(|i| self.data.entries.remove(i));

        if entry.is_some() {
            self.flush()?;
        }

        Ok(entry)
    }

    /// Get a job by ID
    pub fn get(&self, id: &str) -> Option<&CronEntry> {
        self.data.entries.iter().find(|e| e.job.id == id)
    }

    /// List all jobs
    pub fn list(&self) -> anyhow::Result<Vec<CronEntry>> {
        Ok(self.data.entries.clone())
    }

    /// Get the number of jobs
    pub fn len(&self) -> usize {
        self.data.entries.len()
    }

    /// Check if store is empty
    pub fn is_empty(&self) -> bool {
        self.data.entries.is_empty()
    }

    /// Clear all jobs
    pub fn clear(&mut self) -> anyhow::Result<()> {
        self.data.entries.clear();
        self.flush()
    }
}
