//! Vector embedding generation and similarity search
//!
//! Uses fastembed-rs for local ONNX-based embedding generation.
//! Stores vectors in a simple in-memory HNSW index backed by SQLite persistence.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};

/// Configuration for the embedding system
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    /// Whether embeddings are enabled
    pub enabled: bool,
    /// Embedding model name (fastembed model identifier)
    pub model_name: String,
    /// Number of dimensions in the embedding vectors
    pub dimensions: usize,
    /// Weight for vector similarity in hybrid search (0.0 to 1.0)
    pub vector_weight: f32,
    /// Weight for BM25/keyword score in hybrid search (0.0 to 1.0)
    pub keyword_weight: f32,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model_name: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
            dimensions: 384,
            vector_weight: 0.5,
            keyword_weight: 0.5,
        }
    }
}

/// A stored embedding with its entity ID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEmbedding {
    pub entity_id: String,
    pub vector: Vec<f32>,
}

/// Result from a vector similarity search
#[derive(Debug, Clone)]
pub struct VectorSearchResult {
    pub entity_id: String,
    pub similarity: f32,
}

/// Result from hybrid search combining keyword and vector scores
#[derive(Debug, Clone)]
pub struct HybridSearchResult {
    pub entity_id: String,
    /// Combined score using Reciprocal Rank Fusion
    pub score: f32,
    /// Rank from keyword search (None if not found)
    pub keyword_rank: Option<usize>,
    /// Rank from vector search (None if not found)
    pub vector_rank: Option<usize>,
}

/// Vector index for storing and searching embeddings.
///
/// Uses a simple brute-force cosine similarity search. For production
/// scale (>100k vectors), this should be replaced with an HNSW index
/// (e.g., `usearch` or `hnsw_rs`).
pub struct VectorIndex {
    embeddings: Arc<Mutex<HashMap<String, Vec<f32>>>>,
    dimensions: usize,
}

impl VectorIndex {
    /// Create a new vector index
    pub fn new(dimensions: usize) -> Self {
        Self {
            embeddings: Arc::new(Mutex::new(HashMap::new())),
            dimensions,
        }
    }

    /// Load embeddings from SQLite blob storage
    pub fn load_from_db(db_path: &Path, dimensions: usize) -> Result<Self> {
        let index = Self::new(dimensions);

        let conn = rusqlite::Connection::open(db_path)
            .context("Failed to open database for vector index")?;

        // Create table if it doesn't exist
        conn.execute(
            "CREATE TABLE IF NOT EXISTS embeddings (
                entity_id TEXT PRIMARY KEY,
                vector BLOB NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            [],
        )
        .context("Failed to create embeddings table")?;

        // Load existing embeddings
        let mut stmt = conn
            .prepare("SELECT entity_id, vector FROM embeddings")
            .context("Failed to prepare embeddings query")?;

        let rows = stmt
            .query_map([], |row| {
                let entity_id: String = row.get(0)?;
                let blob: Vec<u8> = row.get(1)?;
                Ok((entity_id, blob))
            })
            .context("Failed to query embeddings")?;

        let mut embeddings = index.embeddings.lock().unwrap();
        let mut count = 0;
        for row in rows {
            if let Ok((entity_id, blob)) = row {
                if let Some(vector) = bytes_to_f32_vec(&blob) {
                    if vector.len() == dimensions {
                        embeddings.insert(entity_id, vector);
                        count += 1;
                    }
                }
            }
        }

        info!("Loaded {} embeddings from database", count);
        Ok(index)
    }

    /// Store an embedding for an entity
    pub fn insert(&self, entity_id: &str, vector: Vec<f32>) -> Result<()> {
        if vector.len() != self.dimensions {
            anyhow::bail!(
                "Vector dimension mismatch: expected {}, got {}",
                self.dimensions,
                vector.len()
            );
        }

        let mut embeddings = self.embeddings.lock().unwrap();
        embeddings.insert(entity_id.to_string(), vector);
        debug!("Stored embedding for entity: {}", entity_id);
        Ok(())
    }

    /// Remove an embedding
    pub fn remove(&self, entity_id: &str) {
        let mut embeddings = self.embeddings.lock().unwrap();
        embeddings.remove(entity_id);
    }

    /// Search for the most similar vectors using cosine similarity
    pub fn search(&self, query_vector: &[f32], limit: usize) -> Vec<VectorSearchResult> {
        let embeddings = self.embeddings.lock().unwrap();

        let mut results: Vec<VectorSearchResult> = embeddings
            .iter()
            .map(|(id, vec)| VectorSearchResult {
                entity_id: id.clone(),
                similarity: cosine_similarity(query_vector, vec),
            })
            .collect();

        // Sort by similarity descending
        results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);

        results
    }

    /// Persist all embeddings to SQLite
    pub fn persist_to_db(&self, db_path: &Path) -> Result<()> {
        let conn = rusqlite::Connection::open(db_path)
            .context("Failed to open database for persistence")?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS embeddings (
                entity_id TEXT PRIMARY KEY,
                vector BLOB NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            [],
        )?;

        let embeddings = self.embeddings.lock().unwrap();

        let tx = conn.unchecked_transaction()?;
        for (entity_id, vector) in embeddings.iter() {
            let blob = f32_vec_to_bytes(vector);
            tx.execute(
                "INSERT OR REPLACE INTO embeddings (entity_id, vector) VALUES (?1, ?2)",
                rusqlite::params![entity_id, blob],
            )?;
        }
        tx.commit()?;

        info!("Persisted {} embeddings to database", embeddings.len());
        Ok(())
    }

    /// Number of stored embeddings
    pub fn len(&self) -> usize {
        self.embeddings.lock().unwrap().len()
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> bool {
        self.embeddings.lock().unwrap().is_empty()
    }
}

/// Compute cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

/// Convert f32 vector to bytes for SQLite blob storage
fn f32_vec_to_bytes(vec: &[f32]) -> Vec<u8> {
    vec.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Convert bytes back to f32 vector
fn bytes_to_f32_vec(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() % 4 != 0 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect(),
    )
}

/// Combine keyword search results and vector search results using
/// Reciprocal Rank Fusion (RRF).
///
/// RRF score = sum(1 / (k + rank)) across both result lists.
/// This is the standard method for combining heterogeneous ranked lists.
pub fn hybrid_search_rrf(
    keyword_results: &[String],   // entity IDs ordered by keyword relevance
    vector_results: &[VectorSearchResult],
    k: f32,                       // RRF constant (typically 60.0)
    limit: usize,
) -> Vec<HybridSearchResult> {
    let mut scores: HashMap<String, (f32, Option<usize>, Option<usize>)> = HashMap::new();

    // Add keyword scores
    for (rank, entity_id) in keyword_results.iter().enumerate() {
        let entry = scores.entry(entity_id.clone()).or_insert((0.0, None, None));
        entry.0 += 1.0 / (k + rank as f32 + 1.0);
        entry.1 = Some(rank + 1);
    }

    // Add vector scores
    for (rank, result) in vector_results.iter().enumerate() {
        let entry = scores
            .entry(result.entity_id.clone())
            .or_insert((0.0, None, None));
        entry.0 += 1.0 / (k + rank as f32 + 1.0);
        entry.2 = Some(rank + 1);
    }

    let mut results: Vec<HybridSearchResult> = scores
        .into_iter()
        .map(|(id, (score, kw_rank, vec_rank))| HybridSearchResult {
            entity_id: id,
            score,
            keyword_rank: kw_rank,
            vector_rank: vec_rank,
        })
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);

    results
}

/// Trait for generating embeddings from text.
///
/// This abstraction allows swapping between local (fastembed) and
/// API-based (OpenAI, Voyage) embedding providers.
#[allow(async_fn_in_trait)]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate an embedding vector for a single text
    fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Generate embeddings for multiple texts (batch)
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }

    /// Dimensionality of the output vectors
    fn dimensions(&self) -> usize;
}

/// A no-op embedding provider for when embeddings are disabled.
/// Returns zero vectors so the system degrades gracefully.
pub struct NoOpEmbeddingProvider {
    dims: usize,
}

impl NoOpEmbeddingProvider {
    pub fn new(dims: usize) -> Self {
        Self { dims }
    }
}

impl EmbeddingProvider for NoOpEmbeddingProvider {
    fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![0.0; self.dims])
    }

    fn dimensions(&self) -> usize {
        self.dims
    }
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
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_f32_roundtrip() {
        let original = vec![1.0f32, -2.5, 3.14, 0.0];
        let bytes = f32_vec_to_bytes(&original);
        let recovered = bytes_to_f32_vec(&bytes).unwrap();
        assert_eq!(original, recovered);
    }

    #[test]
    fn test_vector_index_insert_search() {
        let index = VectorIndex::new(3);
        index.insert("a", vec![1.0, 0.0, 0.0]).unwrap();
        index.insert("b", vec![0.0, 1.0, 0.0]).unwrap();
        index.insert("c", vec![0.7, 0.7, 0.0]).unwrap();

        let results = index.search(&[1.0, 0.0, 0.0], 3);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].entity_id, "a");
        assert!((results[0].similarity - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_vector_index_dimension_mismatch() {
        let index = VectorIndex::new(3);
        let result = index.insert("a", vec![1.0, 0.0]);
        assert!(result.is_err());
    }

    #[test]
    fn test_hybrid_search_rrf() {
        let keyword = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let vector = vec![
            VectorSearchResult {
                entity_id: "b".to_string(),
                similarity: 0.9,
            },
            VectorSearchResult {
                entity_id: "d".to_string(),
                similarity: 0.8,
            },
            VectorSearchResult {
                entity_id: "a".to_string(),
                similarity: 0.7,
            },
        ];

        let results = hybrid_search_rrf(&keyword, &vector, 60.0, 10);

        // "a" and "b" appear in both lists, should score highest
        assert!(!results.is_empty());
        let top_ids: Vec<&str> = results.iter().map(|r| r.entity_id.as_str()).collect();
        assert!(top_ids.contains(&"a"));
        assert!(top_ids.contains(&"b"));

        // "a" is rank 1 in keyword and rank 3 in vector
        // "b" is rank 2 in keyword and rank 1 in vector
        // Both should have higher scores than "c" or "d" which appear in only one list
        let a_score = results.iter().find(|r| r.entity_id == "a").unwrap().score;
        let c_score = results.iter().find(|r| r.entity_id == "c").unwrap().score;
        assert!(a_score > c_score);
    }

    #[test]
    fn test_noop_provider() {
        let provider = NoOpEmbeddingProvider::new(384);
        let vec = provider.embed("test").unwrap();
        assert_eq!(vec.len(), 384);
        assert!(vec.iter().all(|&v| v == 0.0));
    }
}
