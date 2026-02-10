//! Memory store implementation with JSONL persistence

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use super::search::{SearchIndex, MemorySearchResult};
use super::{MemoryRecord, MemoryScope};

/// Memory store with JSONL persistence and in-memory indexing
pub struct MemoryStore {
    /// Path to the JSONL file (None for in-memory only)
    path: Option<PathBuf>,

    /// In-memory record storage (id -> record)
    records: HashMap<String, MemoryRecord>,

    /// Search index for fast retrieval
    index: SearchIndex,

    /// Store version for future migrations
    version: u32,
}

impl MemoryStore {
    /// Create a new in-memory store (no persistence)
    pub fn in_memory() -> Self {
        Self {
            path: None,
            records: HashMap::new(),
            index: SearchIndex::new(),
            version: 1,
        }
    }

    /// Open a store from a JSONL file, creating it if it doesn't exist
    pub fn open<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let mut store = Self {
            path: Some(path.clone()),
            records: HashMap::new(),
            index: SearchIndex::new(),
            version: 1,
        };

        // Load existing records if file exists
        if path.exists() {
            store.load_from_file()?;
        }

        Ok(store)
    }

    /// Load records from the JSONL file
    fn load_from_file(&mut self) -> anyhow::Result<()> {
        let path = self.path.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Cannot load from file: no path configured")
        })?;

        let file = File::open(path)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<MemoryRecord>(&line) {
                Ok(record) => {
                    // Apply the record (handles soft deletes)
                    if record.deleted_at.is_some() {
                        // Remove from index and records
                        if let Some(old) = self.records.remove(&record.id) {
                            self.index.remove(&old);
                        }
                    } else {
                        // Add or update
                        if let Some(old) = self.records.get(&record.id) {
                            self.index.remove(old);
                        }
                        self.index.add(&record);
                        self.records.insert(record.id.clone(), record);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to parse memory record: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Append a record to the JSONL file
    fn append_to_file(&self, record: &MemoryRecord) -> anyhow::Result<()> {
        let path = match &self.path {
            Some(p) => p,
            None => return Ok(()), // In-memory only, no persistence
        };

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;

        let json = serde_json::to_string(record)?;
        writeln!(file, "{}", json)?;
        file.flush()?;

        Ok(())
    }

    /// Store a new memory record
    pub fn store(&mut self, record: MemoryRecord) -> anyhow::Result<String> {
        let id = record.id.clone();

        // Remove old record from index if updating
        if let Some(old) = self.records.get(&id) {
            self.index.remove(old);
        }

        // Add to index
        self.index.add(&record);

        // Store in memory
        self.records.insert(id.clone(), record.clone());

        // Persist to file
        self.append_to_file(&record)?;

        Ok(id)
    }

    /// Get a memory by ID
    pub fn get(&self, id: &str) -> Option<&MemoryRecord> {
        self.records.get(id).filter(|r| !r.is_deleted())
    }

    /// Soft-delete a memory by ID
    pub fn delete(&mut self, id: &str) -> anyhow::Result<bool> {
        let record = match self.records.get(id) {
            Some(r) if !r.is_deleted() => r.clone(),
            _ => return Ok(false),
        };

        // Create deletion record
        let mut deleted = record.clone();
        deleted.deleted_at = Some(chrono::Utc::now().timestamp_millis());
        deleted.updated_at = chrono::Utc::now().timestamp_millis();

        // Remove from index
        self.index.remove(&record);

        // Update in-memory state
        self.records.insert(id.to_string(), deleted.clone());

        // Append deletion record to file
        self.append_to_file(&deleted)?;

        Ok(true)
    }

    /// List all non-deleted memories matching a scope
    pub fn list(&self, scope: &MemoryScope) -> Vec<&MemoryRecord> {
        self.records
            .values()
            .filter(|r| !r.is_deleted() && r.scope.matches(scope))
            .collect()
    }

    /// Search memories by query string
    pub fn search(
        &self,
        query: &str,
        scope: &MemoryScope,
        limit: usize,
    ) -> Vec<MemorySearchResult> {
        self.index.search(query, scope, &self.records, limit)
    }

    /// Search with minimum importance filter
    pub fn search_with_importance(
        &self,
        query: &str,
        scope: &MemoryScope,
        limit: usize,
        min_importance: u8,
    ) -> Vec<MemorySearchResult> {
        self.index
            .search(query, scope, &self.records, limit * 2) // Over-fetch to allow filtering
            .into_iter()
            .filter(|r| {
                self.records
                    .get(&r.id)
                    .map(|rec| rec.importance >= min_importance)
                    .unwrap_or(false)
            })
            .take(limit)
            .collect()
    }

    /// Get the number of non-deleted records
    pub fn len(&self) -> usize {
        self.records.values().filter(|r| !r.is_deleted()).count()
    }

    /// Check if the store is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the file path if this is a persistent store
    pub fn file_path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Get statistics about the store
    pub fn stats(&self) -> MemoryStoreStats {
        let total = self.records.len();
        let active = self.records.values().filter(|r| !r.is_deleted()).count();
        let deleted = total - active;

        let by_importance: HashMap<u8, usize> = self
            .records
            .values()
            .filter(|r| !r.is_deleted())
            .fold(HashMap::new(), |mut acc, r| {
                *acc.entry(r.importance).or_insert(0) += 1;
                acc
            });

        MemoryStoreStats {
            total_records: total,
            active_records: active,
            deleted_records: deleted,
            index_tokens: self.index.token_count(),
            by_importance,
        }
    }

    /// Compact the store by rewriting only active records
    ///
    /// This removes soft-deleted records from the file permanently.
    pub fn compact(&mut self) -> anyhow::Result<usize> {
        let path = match &self.path {
            Some(p) => p.clone(),
            None => return Ok(0), // In-memory only
        };

        // Collect active records
        let active: Vec<MemoryRecord> = self
            .records
            .values()
            .filter(|r| !r.is_deleted())
            .cloned()
            .collect();

        let removed = self.records.len() - active.len();

        // Write to temporary file
        let temp_path = path.with_extension("jsonl.tmp");
        {
            let mut file = File::create(&temp_path)?;
            for record in &active {
                let json = serde_json::to_string(record)?;
                writeln!(file, "{}", json)?;
            }
            file.flush()?;
        }

        // Replace original file
        std::fs::rename(&temp_path, &path)?;

        // Update in-memory state
        self.records.clear();
        self.index = SearchIndex::new();
        for record in active {
            self.index.add(&record);
            self.records.insert(record.id.clone(), record);
        }

        Ok(removed)
    }
}

/// Statistics about the memory store
#[derive(Debug, Clone)]
pub struct MemoryStoreStats {
    pub total_records: usize,
    pub active_records: usize,
    pub deleted_records: usize,
    pub index_tokens: usize,
    pub by_importance: HashMap<u8, usize>,
}
