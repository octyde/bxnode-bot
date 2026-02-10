//! Memory search engine with inverted index and scoring

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::{MemoryRecord, MemoryScope};

/// Search result with relevance score
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MemorySearchResult {
    /// Memory ID
    pub id: String,

    /// Relevance score (higher = more relevant)
    pub score: f64,

    /// Summary if available
    pub summary: Option<String>,

    /// Content preview (first N characters)
    pub content_preview: String,

    /// Tags
    pub tags: Vec<String>,

    /// Creation timestamp
    pub created_at: i64,

    /// Importance level
    pub importance: u8,
}

impl MemorySearchResult {
    /// Create a search result from a memory record
    pub fn from_record(record: &MemoryRecord, score: f64, preview_len: usize) -> Self {
        Self {
            id: record.id.clone(),
            score,
            summary: record.summary.clone(),
            content_preview: record.content_preview(preview_len),
            tags: record.tags.clone(),
            created_at: record.created_at,
            importance: record.importance,
        }
    }
}

/// Inverted index for memory search
pub struct SearchIndex {
    /// Token -> set of memory IDs containing that token
    index: HashMap<String, HashSet<String>>,

    /// Minimum token length to index
    min_token_len: usize,
}

impl SearchIndex {
    /// Create a new empty index
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
            min_token_len: 3,
        }
    }

    /// Create an index with custom minimum token length
    pub fn with_min_token_len(min_len: usize) -> Self {
        Self {
            index: HashMap::new(),
            min_token_len: min_len,
        }
    }

    /// Add a record to the index
    pub fn add(&mut self, record: &MemoryRecord) {
        let tokens = self.tokenize_record(record);
        for token in tokens {
            self.index
                .entry(token)
                .or_insert_with(HashSet::new)
                .insert(record.id.clone());
        }
    }

    /// Remove a record from the index
    pub fn remove(&mut self, record: &MemoryRecord) {
        let tokens = self.tokenize_record(record);
        for token in tokens {
            if let Some(ids) = self.index.get_mut(&token) {
                ids.remove(&record.id);
                // Clean up empty entries
                if ids.is_empty() {
                    self.index.remove(&token);
                }
            }
        }
    }

    /// Search for records matching a query
    pub fn search(
        &self,
        query: &str,
        scope: &MemoryScope,
        records: &HashMap<String, MemoryRecord>,
        limit: usize,
    ) -> Vec<MemorySearchResult> {
        let query_tokens = self.tokenize(query);

        if query_tokens.is_empty() {
            return Vec::new();
        }

        // Find candidate IDs (must contain at least one query token)
        let mut candidate_ids: HashSet<String> = HashSet::new();
        for token in &query_tokens {
            if let Some(ids) = self.index.get(token) {
                candidate_ids.extend(ids.iter().cloned());
            }
        }

        // Score and filter candidates
        let now = chrono::Utc::now().timestamp_millis();
        let mut results: Vec<(String, f64)> = candidate_ids
            .into_iter()
            .filter_map(|id| {
                let record = records.get(&id)?;

                // Filter out deleted records
                if record.is_deleted() {
                    return None;
                }

                // Check scope
                if !record.scope.matches(scope) {
                    return None;
                }

                // Calculate score
                let score = self.score(record, &query_tokens, now);
                Some((id, score))
            })
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top results and convert to SearchResult
        results
            .into_iter()
            .take(limit)
            .filter_map(|(id, score)| {
                let record = records.get(&id)?;
                Some(MemorySearchResult::from_record(record, score, 200))
            })
            .collect()
    }

    /// Calculate relevance score for a record
    fn score(&self, record: &MemoryRecord, query_tokens: &[String], now_ms: i64) -> f64 {
        let record_tokens: HashSet<String> = self.tokenize_record(record).into_iter().collect();

        // Term frequency: count of matching tokens
        let matching = query_tokens
            .iter()
            .filter(|t| record_tokens.contains(*t))
            .count() as f64;

        let term_freq = if query_tokens.is_empty() {
            0.0
        } else {
            matching / query_tokens.len() as f64
        };

        // Recency: favor newer memories
        let age_days = (now_ms - record.created_at) as f64 / (24.0 * 60.0 * 60.0 * 1000.0);
        let recency = 1.0 / (1.0 + age_days);

        // Importance: normalized to 0-1
        let importance = record.importance as f64 / 10.0;

        // Weighted combination
        // - 50% term frequency (most important)
        // - 30% recency (prefer recent memories)
        // - 20% importance (user-assigned weight)
        term_freq * 0.5 + recency * 0.3 + importance * 0.2
    }

    /// Tokenize text into searchable tokens
    pub fn tokenize(&self, text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() >= self.min_token_len)
            .map(|s| s.to_string())
            .collect()
    }

    /// Tokenize all searchable fields of a record
    fn tokenize_record(&self, record: &MemoryRecord) -> Vec<String> {
        let mut tokens = Vec::new();

        // Content tokens
        tokens.extend(self.tokenize(&record.content));

        // Summary tokens
        if let Some(ref summary) = record.summary {
            tokens.extend(self.tokenize(summary));
        }

        // Tag tokens (tags are already tokenized)
        for tag in &record.tags {
            tokens.extend(self.tokenize(tag));
        }

        // Provenance tokens
        if let Some(ref provenance) = record.provenance {
            tokens.extend(self.tokenize(provenance));
        }

        tokens
    }

    /// Get the number of unique tokens in the index
    pub fn token_count(&self) -> usize {
        self.index.len()
    }
}

impl Default for SearchIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize() {
        let index = SearchIndex::new();

        let tokens = index.tokenize("Hello, World! This is a test.");
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
        assert!(tokens.contains(&"this".to_string()));
        assert!(tokens.contains(&"test".to_string()));

        // Short tokens should be filtered
        assert!(!tokens.contains(&"is".to_string()));
        assert!(!tokens.contains(&"a".to_string()));
    }

    #[test]
    fn test_tokenize_with_numbers() {
        let index = SearchIndex::new();

        let tokens = index.tokenize("Version 123 released on 2024-01-15");
        assert!(tokens.contains(&"version".to_string()));
        assert!(tokens.contains(&"123".to_string()));
        assert!(tokens.contains(&"released".to_string()));
        assert!(tokens.contains(&"2024".to_string()));
    }

    #[test]
    fn test_add_and_search() {
        let mut index = SearchIndex::new();
        let mut records = HashMap::new();

        let scope = MemoryScope::agent("test");
        let record = MemoryRecord::new(scope.clone(), "The quick brown fox jumps over the lazy dog", None);
        let id = record.id.clone();

        index.add(&record);
        records.insert(id.clone(), record);

        // Search for existing terms
        let results = index.search("quick fox", &scope, &records, 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, id);

        // Search for non-existing terms
        let results = index.search("elephant tiger", &scope, &records, 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_scope_filtering() {
        let mut index = SearchIndex::new();
        let mut records = HashMap::new();

        let scope1 = MemoryScope::user("agent", "channel1", "user1");
        let scope2 = MemoryScope::user("agent", "channel1", "user2");

        let record1 = MemoryRecord::new(scope1.clone(), "Secret information for user1", None);
        let record2 = MemoryRecord::new(scope2.clone(), "Secret information for user2", None);

        index.add(&record1);
        index.add(&record2);
        records.insert(record1.id.clone(), record1);
        records.insert(record2.id.clone(), record2);

        // User1 should only see their own memory
        let results = index.search("secret", &scope1, &records, 5);
        assert_eq!(results.len(), 1);
        assert!(results[0].content_preview.contains("user1"));

        // User2 should only see their own memory
        let results = index.search("secret", &scope2, &records, 5);
        assert_eq!(results.len(), 1);
        assert!(results[0].content_preview.contains("user2"));
    }

    #[test]
    fn test_remove_from_index() {
        let mut index = SearchIndex::new();
        let mut records = HashMap::new();

        let scope = MemoryScope::agent("test");
        let record = MemoryRecord::new(scope.clone(), "Important information", None);
        let id = record.id.clone();

        index.add(&record);
        records.insert(id.clone(), record.clone());

        // Should find it
        let results = index.search("important", &scope, &records, 5);
        assert_eq!(results.len(), 1);

        // Remove and search again
        index.remove(&record);
        let results = index.search("important", &scope, &records, 5);
        assert!(results.is_empty());
    }
}
