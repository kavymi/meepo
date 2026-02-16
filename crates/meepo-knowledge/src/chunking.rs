//! Document chunking and ingestion pipeline
//!
//! Splits documents into overlapping chunks for indexing in the knowledge
//! graph. Supports recursive character splitting with configurable chunk
//! size and overlap.

use serde::{Deserialize, Serialize};
use tracing::debug;

/// Configuration for document chunking
#[derive(Debug, Clone)]
pub struct ChunkingConfig {
    /// Target chunk size in characters
    pub chunk_size: usize,
    /// Overlap between consecutive chunks in characters
    pub chunk_overlap: usize,
    /// Separators to split on, in priority order
    pub separators: Vec<String>,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            chunk_size: 1000,
            chunk_overlap: 200,
            separators: vec![
                "\n\n".to_string(),
                "\n".to_string(),
                ". ".to_string(),
                "! ".to_string(),
                "? ".to_string(),
                "; ".to_string(),
                ", ".to_string(),
                " ".to_string(),
            ],
        }
    }
}

/// A chunk of a document with position metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunk {
    /// The chunk text content
    pub content: String,
    /// Index of this chunk within the document (0-based)
    pub chunk_index: usize,
    /// Character offset of the start of this chunk in the original document
    pub start_offset: usize,
    /// Character offset of the end of this chunk in the original document
    pub end_offset: usize,
    /// Total number of chunks in the document
    pub total_chunks: usize,
}

/// Metadata about an ingested document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// Original file path (if from file)
    pub source_path: Option<String>,
    /// Document title (extracted or provided)
    pub title: Option<String>,
    /// MIME type or format
    pub content_type: String,
    /// Total character count of the original document
    pub total_chars: usize,
    /// Number of chunks produced
    pub chunk_count: usize,
}

/// Split text into chunks using recursive character splitting.
///
/// Tries to split on the highest-priority separator that produces chunks
/// within the target size. Falls back to lower-priority separators, and
/// ultimately to character-level splitting.
pub fn chunk_text(text: &str, config: &ChunkingConfig) -> Vec<DocumentChunk> {
    if text.is_empty() {
        return Vec::new();
    }

    // If text fits in one chunk, return it directly
    if text.len() <= config.chunk_size {
        return vec![DocumentChunk {
            content: text.to_string(),
            chunk_index: 0,
            start_offset: 0,
            end_offset: text.len(),
            total_chunks: 1,
        }];
    }

    let raw_chunks = recursive_split(text, &config.separators, config.chunk_size);

    // Merge small chunks and apply overlap
    let merged = merge_with_overlap(&raw_chunks, config.chunk_size, config.chunk_overlap);

    // Build DocumentChunk structs with offsets
    let total = merged.len();
    let mut chunks = Vec::with_capacity(total);
    let mut offset = 0;

    for (i, chunk_text) in merged.iter().enumerate() {
        // Find the actual position in the original text
        let start = if i == 0 {
            0
        } else {
            text[offset..]
                .find(chunk_text.split_at(chunk_text.len().min(50)).0)
                .map(|pos| offset + pos)
                .unwrap_or(offset)
        };

        let end = start + chunk_text.len();

        chunks.push(DocumentChunk {
            content: chunk_text.clone(),
            chunk_index: i,
            start_offset: start,
            end_offset: end.min(text.len()),
            total_chunks: total,
        });

        offset = start + chunk_text.len().saturating_sub(config.chunk_overlap);
    }

    debug!("Split {} chars into {} chunks", text.len(), chunks.len());
    chunks
}

/// Recursively split text on separators
fn recursive_split(text: &str, separators: &[String], chunk_size: usize) -> Vec<String> {
    if text.len() <= chunk_size || separators.is_empty() {
        return vec![text.to_string()];
    }

    let separator = &separators[0];
    let remaining_separators = &separators[1..];

    let splits: Vec<&str> = text.split(separator.as_str()).collect();

    let mut result = Vec::new();
    let mut current = String::new();

    for (i, split) in splits.iter().enumerate() {
        let with_sep = if i < splits.len() - 1 {
            format!("{}{}", split, separator)
        } else {
            split.to_string()
        };

        if current.len() + with_sep.len() > chunk_size && !current.is_empty() {
            // Current chunk is full, try to recursively split if still too large
            if current.len() > chunk_size {
                result.extend(recursive_split(&current, remaining_separators, chunk_size));
            } else {
                result.push(current.clone());
            }
            current.clear();
        }

        current.push_str(&with_sep);
    }

    if !current.is_empty() {
        if current.len() > chunk_size {
            result.extend(recursive_split(&current, remaining_separators, chunk_size));
        } else {
            result.push(current);
        }
    }

    result
}

/// Merge chunks and add overlap between consecutive chunks
fn merge_with_overlap(chunks: &[String], max_size: usize, overlap: usize) -> Vec<String> {
    if chunks.is_empty() {
        return Vec::new();
    }

    if chunks.len() == 1 {
        return chunks.to_vec();
    }

    let mut result = Vec::new();

    for (i, chunk) in chunks.iter().enumerate() {
        if i == 0 {
            result.push(chunk.clone());
        } else {
            // Prepend overlap from previous chunk
            let prev = &chunks[i - 1];
            let overlap_text = if prev.len() > overlap {
                &prev[prev.len() - overlap..]
            } else {
                prev.as_str()
            };

            let merged = format!("{}{}", overlap_text, chunk);
            if merged.len() <= max_size + overlap {
                result.push(merged);
            } else {
                // If merged is too large, just use the chunk with truncated overlap
                let truncated_overlap =
                    &overlap_text[overlap_text.len().saturating_sub(overlap / 2)..];
                result.push(format!("{}{}", truncated_overlap, chunk));
            }
        }
    }

    result
}

/// Detect content type from file extension
pub fn detect_content_type(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if lower.ends_with(".md") || lower.ends_with(".markdown") {
        "text/markdown"
    } else if lower.ends_with(".txt") {
        "text/plain"
    } else if lower.ends_with(".rs") {
        "text/x-rust"
    } else if lower.ends_with(".py") {
        "text/x-python"
    } else if lower.ends_with(".js") || lower.ends_with(".ts") {
        "text/javascript"
    } else if lower.ends_with(".json") {
        "application/json"
    } else if lower.ends_with(".toml") {
        "application/toml"
    } else if lower.ends_with(".yaml") || lower.ends_with(".yml") {
        "application/yaml"
    } else if lower.ends_with(".html") || lower.ends_with(".htm") {
        "text/html"
    } else if lower.ends_with(".csv") {
        "text/csv"
    } else {
        "text/plain"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_empty() {
        let config = ChunkingConfig::default();
        let chunks = chunk_text("", &config);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_small_text() {
        let config = ChunkingConfig::default();
        let chunks = chunk_text("Hello, world!", &config);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, "Hello, world!");
        assert_eq!(chunks[0].chunk_index, 0);
        assert_eq!(chunks[0].total_chunks, 1);
    }

    #[test]
    fn test_chunk_large_text() {
        let config = ChunkingConfig {
            chunk_size: 100,
            chunk_overlap: 20,
            ..Default::default()
        };

        // Create a text with clear paragraph boundaries
        let text = (0..10)
            .map(|i| {
                format!(
                    "This is paragraph {}. It contains some text about topic {}.",
                    i, i
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let chunks = chunk_text(&text, &config);
        assert!(chunks.len() > 1);

        // All chunks should have correct total_chunks
        for chunk in &chunks {
            assert_eq!(chunk.total_chunks, chunks.len());
        }

        // Chunks should be within size limits (with some tolerance for overlap)
        for chunk in &chunks {
            assert!(
                chunk.content.len() <= config.chunk_size + config.chunk_overlap + 50,
                "Chunk too large: {} chars",
                chunk.content.len()
            );
        }
    }

    #[test]
    fn test_chunk_preserves_content() {
        let config = ChunkingConfig {
            chunk_size: 50,
            chunk_overlap: 0,
            ..Default::default()
        };

        let text = "First sentence. Second sentence. Third sentence. Fourth sentence.";
        let chunks = chunk_text(text, &config);

        // All original content should appear in at least one chunk
        assert!(chunks.iter().any(|c| c.content.contains("First")));
        assert!(chunks.iter().any(|c| c.content.contains("Fourth")));
    }

    #[test]
    fn test_detect_content_type() {
        assert_eq!(detect_content_type("readme.md"), "text/markdown");
        assert_eq!(detect_content_type("main.rs"), "text/x-rust");
        assert_eq!(detect_content_type("data.json"), "application/json");
        assert_eq!(detect_content_type("unknown.xyz"), "text/plain");
    }

    #[test]
    fn test_detect_content_type_all_extensions() {
        assert_eq!(detect_content_type("doc.markdown"), "text/markdown");
        assert_eq!(detect_content_type("file.txt"), "text/plain");
        assert_eq!(detect_content_type("script.py"), "text/x-python");
        assert_eq!(detect_content_type("app.js"), "text/javascript");
        assert_eq!(detect_content_type("app.ts"), "text/javascript");
        assert_eq!(detect_content_type("config.toml"), "application/toml");
        assert_eq!(detect_content_type("config.yaml"), "application/yaml");
        assert_eq!(detect_content_type("config.yml"), "application/yaml");
        assert_eq!(detect_content_type("page.html"), "text/html");
        assert_eq!(detect_content_type("page.htm"), "text/html");
        assert_eq!(detect_content_type("data.csv"), "text/csv");
    }

    #[test]
    fn test_detect_content_type_case_insensitive() {
        assert_eq!(detect_content_type("README.MD"), "text/markdown");
        assert_eq!(detect_content_type("Main.RS"), "text/x-rust");
        assert_eq!(detect_content_type("DATA.JSON"), "application/json");
    }

    #[test]
    fn test_default_config() {
        let config = ChunkingConfig::default();
        assert_eq!(config.chunk_size, 1000);
        assert_eq!(config.chunk_overlap, 200);
        assert!(!config.separators.is_empty());
        assert_eq!(config.separators[0], "\n\n");
    }

    #[test]
    fn test_chunk_indices_sequential() {
        let config = ChunkingConfig {
            chunk_size: 50,
            chunk_overlap: 10,
            ..Default::default()
        };
        let text = "Word ".repeat(100);
        let chunks = chunk_text(&text, &config);
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.chunk_index, i);
        }
    }

    #[test]
    fn test_document_chunk_serde() {
        let chunk = DocumentChunk {
            content: "test content".to_string(),
            chunk_index: 2,
            start_offset: 100,
            end_offset: 200,
            total_chunks: 5,
        };
        let json = serde_json::to_string(&chunk).unwrap();
        let parsed: DocumentChunk = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.content, "test content");
        assert_eq!(parsed.chunk_index, 2);
        assert_eq!(parsed.total_chunks, 5);
    }

    #[test]
    fn test_document_metadata_serde() {
        let meta = DocumentMetadata {
            source_path: Some("/tmp/test.md".to_string()),
            title: Some("Test Doc".to_string()),
            content_type: "text/markdown".to_string(),
            total_chars: 5000,
            chunk_count: 5,
        };
        let json = serde_json::to_string(&meta).unwrap();
        let parsed: DocumentMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.title.as_deref(), Some("Test Doc"));
        assert_eq!(parsed.chunk_count, 5);
    }

    #[test]
    fn test_chunk_no_overlap() {
        let config = ChunkingConfig {
            chunk_size: 50,
            chunk_overlap: 0,
            ..Default::default()
        };
        let text = "Sentence one. Sentence two. Sentence three. Sentence four. Sentence five. Sentence six.";
        let chunks = chunk_text(text, &config);
        assert!(chunks.len() > 1);
    }
}
