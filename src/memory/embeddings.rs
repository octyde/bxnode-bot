//! Embedding-based vector search for memory.
//!
//! Provides optional vector similarity search alongside the existing keyword search.
//! Uses OpenAI's embedding API by default, but the `EmbeddingProvider` trait is pluggable.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use async_trait::async_trait;

/// An embedding vector (f32 for space efficiency).
pub type Embedding = Vec<f32>;

/// Trait for embedding providers.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embedding for a single text.
    async fn embed(&self, text: &str) -> Result<Embedding>;

    /// Generate embeddings for multiple texts (batched).
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Embedding>>;

    /// Dimensionality of the embeddings.
    fn dimensions(&self) -> usize;
}

/// OpenAI embeddings provider using text-embedding-3-small.
pub struct OpenAIEmbeddingProvider {
    api_key: String,
    base_url: String,
    model: String,
    client: reqwest::Client,
}

impl OpenAIEmbeddingProvider {
    pub fn new(api_key: String, base_url: Option<String>, model: Option<String>) -> Self {
        Self {
            api_key,
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
            model: model.unwrap_or_else(|| "text-embedding-3-small".to_string()),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAIEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Embedding> {
        let results = self.embed_batch(&[text]).await?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No embedding returned"))
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Embedding>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let url = format!("{}/embeddings", self.base_url);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&serde_json::json!({
                "model": self.model,
                "input": texts,
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Embedding API error ({}): {}", status, body);
        }

        let body: serde_json::Value = resp.json().await?;
        let data = body
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| anyhow::anyhow!("Invalid embedding response"))?;

        let mut embeddings = Vec::with_capacity(data.len());
        for item in data {
            let vec = item
                .get("embedding")
                .and_then(|e| e.as_array())
                .ok_or_else(|| anyhow::anyhow!("Missing embedding vector"))?;
            let embedding: Embedding = vec
                .iter()
                .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                .collect();
            embeddings.push(embedding);
        }

        Ok(embeddings)
    }

    fn dimensions(&self) -> usize {
        1536 // text-embedding-3-small default
    }
}

/// In-memory vector index with binary file persistence.
pub struct VectorIndex {
    vectors: HashMap<String, Embedding>,
    dimensions: usize,
    path: Option<PathBuf>,
}

// Binary file format:
// Header: "BXVEC\x01" (6 bytes magic + version)
//         dimensions: u32 (4 bytes)
// Entries: id_len: u16, id_bytes: [u8; id_len], vector: [f32; dimensions]

const MAGIC: &[u8; 6] = b"BXVEC\x01";

impl VectorIndex {
    /// Create a new empty index.
    pub fn new(dimensions: usize) -> Self {
        Self {
            vectors: HashMap::new(),
            dimensions,
            path: None,
        }
    }

    /// Open or create a persistent vector index.
    pub fn open(path: impl AsRef<Path>, dimensions: usize) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut index = Self {
            vectors: HashMap::new(),
            dimensions,
            path: Some(path.clone()),
        };

        if path.exists() {
            index.load_from_file(&path)?;
        }

        Ok(index)
    }

    /// Add or update a vector for a record.
    pub fn add(&mut self, id: &str, embedding: Embedding) -> Result<()> {
        if embedding.len() != self.dimensions {
            anyhow::bail!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.dimensions,
                embedding.len()
            );
        }
        self.vectors.insert(id.to_string(), embedding);
        Ok(())
    }

    /// Remove a vector.
    pub fn remove(&mut self, id: &str) {
        self.vectors.remove(id);
    }

    /// Check if a record has an embedding.
    pub fn has(&self, id: &str) -> bool {
        self.vectors.contains_key(id)
    }

    /// Number of stored vectors.
    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    /// Search for the most similar vectors to the query.
    /// Returns (record_id, similarity_score) pairs, sorted by descending similarity.
    pub fn search(&self, query_embedding: &Embedding, limit: usize) -> Vec<(String, f64)> {
        let mut scores: Vec<(String, f64)> = self
            .vectors
            .iter()
            .map(|(id, vec)| (id.clone(), cosine_similarity(query_embedding, vec)))
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(limit);
        scores
    }

    /// Persist the index to disk.
    pub fn save(&self) -> Result<()> {
        let path = match &self.path {
            Some(p) => p,
            None => return Ok(()), // In-memory only
        };

        let tmp_path = path.with_extension("vec.tmp");
        let mut file = std::fs::File::create(&tmp_path)?;

        // Write header
        file.write_all(MAGIC)?;
        file.write_all(&(self.dimensions as u32).to_le_bytes())?;

        // Write entries
        for (id, vec) in &self.vectors {
            let id_bytes = id.as_bytes();
            file.write_all(&(id_bytes.len() as u16).to_le_bytes())?;
            file.write_all(id_bytes)?;
            for &val in vec {
                file.write_all(&val.to_le_bytes())?;
            }
        }

        drop(file);
        std::fs::rename(&tmp_path, path)?;
        Ok(())
    }

    fn load_from_file(&mut self, path: &Path) -> Result<()> {
        let mut file = std::fs::File::open(path)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        if buf.len() < 10 || &buf[..6] != MAGIC {
            anyhow::bail!("Invalid vector index file");
        }

        let dims = u32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]) as usize;
        if dims != self.dimensions {
            anyhow::bail!(
                "Vector index dimension mismatch: file has {}, expected {}",
                dims,
                self.dimensions
            );
        }

        let mut pos = 10;
        while pos < buf.len() {
            if pos + 2 > buf.len() {
                break;
            }
            let id_len = u16::from_le_bytes([buf[pos], buf[pos + 1]]) as usize;
            pos += 2;

            if pos + id_len > buf.len() {
                break;
            }
            let id = String::from_utf8_lossy(&buf[pos..pos + id_len]).to_string();
            pos += id_len;

            let vec_bytes = dims * 4;
            if pos + vec_bytes > buf.len() {
                break;
            }
            let mut embedding = Vec::with_capacity(dims);
            for i in 0..dims {
                let offset = pos + i * 4;
                let val = f32::from_le_bytes([
                    buf[offset],
                    buf[offset + 1],
                    buf[offset + 2],
                    buf[offset + 3],
                ]);
                embedding.push(val);
            }
            pos += vec_bytes;

            self.vectors.insert(id, embedding);
        }

        Ok(())
    }
}

/// Cosine similarity between two vectors. Returns value in [-1, 1].
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;

    for (&x, &y) in a.iter().zip(b.iter()) {
        let x = x as f64;
        let y = y as f64;
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

/// Merge keyword search results with vector search results using reciprocal rank fusion.
/// Returns merged (id, score) pairs sorted by descending score.
pub fn reciprocal_rank_fusion(
    keyword_results: &[(String, f64)],
    vector_results: &[(String, f64)],
    k: f64, // RRF constant (typically 60)
) -> Vec<(String, f64)> {
    let mut scores: HashMap<String, f64> = HashMap::new();

    for (rank, (id, _)) in keyword_results.iter().enumerate() {
        *scores.entry(id.clone()).or_default() += 1.0 / (k + rank as f64 + 1.0);
    }

    for (rank, (id, _)) in vector_results.iter().enumerate() {
        *scores.entry(id.clone()).or_default() += 1.0 / (k + rank as f64 + 1.0);
    }

    let mut merged: Vec<(String, f64)> = scores.into_iter().collect();
    merged.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let sim = cosine_similarity(&[], &[]);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_vector_index_crud() {
        let mut index = VectorIndex::new(3);
        index.add("a", vec![1.0, 0.0, 0.0]).unwrap();
        index.add("b", vec![0.0, 1.0, 0.0]).unwrap();
        index.add("c", vec![0.7, 0.7, 0.0]).unwrap();

        assert_eq!(index.len(), 3);
        assert!(index.has("a"));

        let results = index.search(&vec![1.0, 0.0, 0.0], 3);
        assert_eq!(results[0].0, "a"); // Most similar
        assert!((results[0].1 - 1.0).abs() < 1e-6);

        index.remove("a");
        assert!(!index.has("a"));
        assert_eq!(index.len(), 2);
    }

    #[test]
    fn test_vector_index_dimension_mismatch() {
        let mut index = VectorIndex::new(3);
        let result = index.add("a", vec![1.0, 0.0]); // Wrong dims
        assert!(result.is_err());
    }

    #[test]
    fn test_vector_index_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.vec");

        // Write
        {
            let mut index = VectorIndex::open(&path, 3).unwrap();
            index.add("x", vec![0.1, 0.2, 0.3]).unwrap();
            index.add("y", vec![0.4, 0.5, 0.6]).unwrap();
            index.save().unwrap();
        }

        // Read back
        {
            let index = VectorIndex::open(&path, 3).unwrap();
            assert_eq!(index.len(), 2);
            assert!(index.has("x"));
            assert!(index.has("y"));

            let results = index.search(&vec![0.1, 0.2, 0.3], 1);
            assert_eq!(results[0].0, "x");
        }
    }

    #[test]
    fn test_reciprocal_rank_fusion() {
        let keyword = vec![
            ("a".to_string(), 0.9),
            ("b".to_string(), 0.7),
            ("c".to_string(), 0.5),
        ];
        let vector = vec![
            ("b".to_string(), 0.95),
            ("d".to_string(), 0.8),
            ("a".to_string(), 0.6),
        ];

        let merged = reciprocal_rank_fusion(&keyword, &vector, 60.0);

        // "a" appears at rank 0 in keyword, rank 2 in vector
        // "b" appears at rank 1 in keyword, rank 0 in vector
        // Both should score higher than single-list items
        let a_score = merged.iter().find(|(id, _)| id == "a").unwrap().1;
        let b_score = merged.iter().find(|(id, _)| id == "b").unwrap().1;
        let d_score = merged.iter().find(|(id, _)| id == "d").unwrap().1;

        // Items in both lists should generally score higher
        assert!(a_score > d_score);
        assert!(b_score > d_score);
    }
}
