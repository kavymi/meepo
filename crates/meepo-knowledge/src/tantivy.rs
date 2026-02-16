//! Tantivy full-text search index

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tantivy::{
    Index, IndexWriter, ReloadPolicy, TantivyDocument, collector::TopDocs, query::QueryParser,
    schema::*,
};
use tracing::{debug, info};

use crate::sqlite::Entity;

/// Search result with score and snippet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub content: String,
    pub entity_type: String,
    pub score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

/// Tantivy search index wrapper
pub struct TantivyIndex {
    index: Index,
    id_field: Field,
    content_field: Field,
    entity_type_field: Field,
    created_at_field: Field,
}

impl TantivyIndex {
    /// Create or open a Tantivy index
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        info!("Initializing Tantivy index at {:?}", path.as_ref());

        // Create directory if it doesn't exist
        std::fs::create_dir_all(path.as_ref())?;

        // Define schema
        let mut schema_builder = Schema::builder();
        let id_field = schema_builder.add_text_field("id", STRING | STORED);
        let content_field = schema_builder.add_text_field("content", TEXT | STORED);
        let entity_type_field = schema_builder.add_text_field("entity_type", STRING | STORED);
        let created_at_field = schema_builder.add_text_field("created_at", STRING | STORED);
        let schema = schema_builder.build();

        // Open or create index
        let index = if path.as_ref().join("meta.json").exists() {
            Index::open_in_dir(path.as_ref())?
        } else {
            Index::create_in_dir(path.as_ref(), schema.clone())?
        };

        debug!("Tantivy index initialized successfully");

        Ok(Self {
            index,
            id_field,
            content_field,
            entity_type_field,
            created_at_field,
        })
    }

    /// Index a document
    pub fn index_document(
        &self,
        id: &str,
        content: &str,
        entity_type: &str,
        created_at: &str,
    ) -> Result<()> {
        let mut writer = self.get_writer()?;

        // Delete existing document with same ID (if any)
        let id_query = tantivy::query::TermQuery::new(
            tantivy::Term::from_field_text(self.id_field, id),
            tantivy::schema::IndexRecordOption::Basic,
        );
        let _ = writer.delete_query(Box::new(id_query));

        // Create document
        let mut doc = TantivyDocument::default();
        doc.add_text(self.id_field, id);
        doc.add_text(self.content_field, content);
        doc.add_text(self.entity_type_field, entity_type);
        doc.add_text(self.created_at_field, created_at);

        writer.add_document(doc)?;
        writer.commit()?;

        debug!("Indexed document: {} ({})", id, entity_type);
        Ok(())
    }

    /// Search the index
    pub fn search(&self, query_str: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        let searcher = reader.searcher();

        // Parse query
        let query_parser = QueryParser::for_index(&self.index, vec![self.content_field]);
        let query = query_parser
            .parse_query(query_str)
            .context("Failed to parse search query")?;

        // Search
        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let retrieved_doc: TantivyDocument = searcher.doc(doc_address)?;

            let id = retrieved_doc
                .get_first(self.id_field)
                .and_then(|v: &tantivy::schema::OwnedValue| v.as_str())
                .unwrap_or("")
                .to_string();

            let content = retrieved_doc
                .get_first(self.content_field)
                .and_then(|v: &tantivy::schema::OwnedValue| v.as_str())
                .unwrap_or("")
                .to_string();

            let entity_type = retrieved_doc
                .get_first(self.entity_type_field)
                .and_then(|v: &tantivy::schema::OwnedValue| v.as_str())
                .unwrap_or("")
                .to_string();

            // Generate snippet (first 200 chars)
            let snippet = if content.len() > 200 {
                Some(format!("{}...", &content[..197]))
            } else {
                Some(content.clone())
            };

            results.push(SearchResult {
                id,
                content,
                entity_type,
                score,
                snippet,
            });
        }

        debug!(
            "Search for '{}' returned {} results",
            query_str,
            results.len()
        );
        Ok(results)
    }

    /// Delete a document by ID
    pub fn delete_document(&self, id: &str) -> Result<()> {
        let mut writer = self.get_writer()?;

        let id_query = tantivy::query::TermQuery::new(
            tantivy::Term::from_field_text(self.id_field, id),
            tantivy::schema::IndexRecordOption::Basic,
        );

        let _ = writer.delete_query(Box::new(id_query));
        writer.commit()?;

        debug!("Deleted document: {}", id);
        Ok(())
    }

    /// Reindex all entities from a pre-fetched entity list
    pub fn reindex_all_from_entities(&self, entities: &[Entity]) -> Result<()> {
        info!("Reindexing all entities");

        let mut writer = self.get_writer()?;

        // Delete all documents
        writer.delete_all_documents()?;

        let entity_count = entities.len();
        // Index all entities
        for entity in entities {
            let content = format!(
                "{} {} {}",
                entity.name,
                entity.entity_type,
                entity
                    .metadata
                    .as_ref()
                    .map(|m: &serde_json::Value| m.to_string())
                    .unwrap_or_default()
            );

            let mut doc = TantivyDocument::default();
            doc.add_text(self.id_field, &entity.id);
            doc.add_text(self.content_field, &content);
            doc.add_text(self.entity_type_field, &entity.entity_type);
            doc.add_text(self.created_at_field, entity.created_at.to_rfc3339());

            writer.add_document(doc)?;
        }

        writer.commit()?;

        info!("Reindexed {} entities", entity_count);
        Ok(())
    }

    /// Get index writer
    fn get_writer(&self) -> Result<IndexWriter> {
        // 50MB heap size for writer
        self.index
            .writer(50_000_000)
            .context("Failed to create index writer")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_index_and_search() -> Result<()> {
        let temp_path =
            env::temp_dir().join(format!("test_tantivy_index_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&temp_path);

        let index = TantivyIndex::new(&temp_path)?;

        // Index a document
        index.index_document(
            "test-id-1",
            "This is a test document about Rust programming",
            "concept",
            &chrono::Utc::now().to_rfc3339(),
        )?;

        index.index_document(
            "test-id-2",
            "Another document about Python programming",
            "concept",
            &chrono::Utc::now().to_rfc3339(),
        )?;

        // Search
        let results = index.search("Rust", 10)?;
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "test-id-1");

        let results = index.search("Python", 10)?;
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "test-id-2");

        // Search for common term
        let results = index.search("programming", 10)?;
        assert_eq!(results.len(), 2);

        let _ = std::fs::remove_dir_all(&temp_path);
        Ok(())
    }

    #[test]
    fn test_delete_document() -> Result<()> {
        let temp_path =
            env::temp_dir().join(format!("test_tantivy_delete_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&temp_path);

        let index = TantivyIndex::new(&temp_path)?;

        // Index a document
        index.index_document(
            "delete-test-id",
            "Document to be deleted",
            "concept",
            &chrono::Utc::now().to_rfc3339(),
        )?;

        // Verify it exists
        let results = index.search("deleted", 10)?;
        assert!(!results.is_empty());

        // Delete it
        index.delete_document("delete-test-id")?;

        // Verify it's gone
        let results = index.search("deleted", 10)?;
        assert!(results.is_empty());

        let _ = std::fs::remove_dir_all(&temp_path);
        Ok(())
    }

    #[test]
    fn test_search_no_results() -> Result<()> {
        let temp_path =
            env::temp_dir().join(format!("test_tantivy_empty_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&temp_path);

        let index = TantivyIndex::new(&temp_path)?;

        index.index_document(
            "doc-1",
            "Rust programming language",
            "concept",
            &chrono::Utc::now().to_rfc3339(),
        )?;

        let results = index.search("javascript", 10)?;
        assert!(results.is_empty());

        let _ = std::fs::remove_dir_all(&temp_path);
        Ok(())
    }

    #[test]
    fn test_search_limit() -> Result<()> {
        let temp_path =
            env::temp_dir().join(format!("test_tantivy_limit_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&temp_path);

        let index = TantivyIndex::new(&temp_path)?;

        for i in 0..5 {
            index.index_document(
                &format!("doc-{}", i),
                &format!("Document about programming topic {}", i),
                "concept",
                &chrono::Utc::now().to_rfc3339(),
            )?;
        }

        let results = index.search("programming", 2)?;
        assert_eq!(results.len(), 2);

        let _ = std::fs::remove_dir_all(&temp_path);
        Ok(())
    }

    #[test]
    fn test_index_document_overwrites() -> Result<()> {
        let temp_path =
            env::temp_dir().join(format!("test_tantivy_overwrite_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&temp_path);

        let index = TantivyIndex::new(&temp_path)?;

        index.index_document(
            "doc-1",
            "Original content about cats",
            "concept",
            &chrono::Utc::now().to_rfc3339(),
        )?;

        // Overwrite with new content
        index.index_document(
            "doc-1",
            "Updated content about dogs",
            "concept",
            &chrono::Utc::now().to_rfc3339(),
        )?;

        // Should find the updated content
        let results = index.search("dogs", 10)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "doc-1");

        // Old content should not be found
        let results = index.search("cats", 10)?;
        assert!(results.is_empty());

        let _ = std::fs::remove_dir_all(&temp_path);
        Ok(())
    }

    #[test]
    fn test_reindex_all_from_entities() -> Result<()> {
        let temp_path =
            env::temp_dir().join(format!("test_tantivy_reindex_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&temp_path);

        let index = TantivyIndex::new(&temp_path)?;

        // Index some initial docs
        index.index_document(
            "old-1",
            "Old document that should be removed",
            "concept",
            &chrono::Utc::now().to_rfc3339(),
        )?;

        // Reindex with new entities
        let entities = vec![
            Entity {
                id: "new-1".to_string(),
                name: "Rust Language".to_string(),
                entity_type: "concept".to_string(),
                metadata: Some(serde_json::json!({"category": "programming"})),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
            Entity {
                id: "new-2".to_string(),
                name: "Python Language".to_string(),
                entity_type: "concept".to_string(),
                metadata: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
        ];

        index.reindex_all_from_entities(&entities)?;

        // Old docs should be gone
        let results = index.search("removed", 10)?;
        assert!(results.is_empty());

        // New docs should be searchable
        let results = index.search("Rust", 10)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "new-1");

        let results = index.search("Python", 10)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "new-2");

        let _ = std::fs::remove_dir_all(&temp_path);
        Ok(())
    }

    #[test]
    fn test_reindex_empty_entities() -> Result<()> {
        let temp_path = env::temp_dir().join(format!(
            "test_tantivy_reindex_empty_{}",
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_dir_all(&temp_path);

        let index = TantivyIndex::new(&temp_path)?;

        index.index_document(
            "doc-1",
            "Some content",
            "concept",
            &chrono::Utc::now().to_rfc3339(),
        )?;

        // Reindex with empty list should clear everything
        index.reindex_all_from_entities(&[])?;

        let results = index.search("content", 10)?;
        assert!(results.is_empty());

        let _ = std::fs::remove_dir_all(&temp_path);
        Ok(())
    }

    #[test]
    fn test_search_result_fields() -> Result<()> {
        let temp_path =
            env::temp_dir().join(format!("test_tantivy_fields_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&temp_path);

        let index = TantivyIndex::new(&temp_path)?;

        index.index_document(
            "field-test",
            "Testing all fields are returned correctly",
            "person",
            "2024-01-01T00:00:00Z",
        )?;

        let results = index.search("fields", 10)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "field-test");
        assert_eq!(results[0].entity_type, "person");
        assert!(results[0].score > 0.0);
        assert!(results[0].snippet.is_some());
        assert!(results[0].content.contains("Testing all fields"));

        let _ = std::fs::remove_dir_all(&temp_path);
        Ok(())
    }

    #[test]
    fn test_search_result_serde() {
        let result = SearchResult {
            id: "test".to_string(),
            content: "content".to_string(),
            entity_type: "concept".to_string(),
            score: 0.95,
            snippet: Some("snip".to_string()),
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: SearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "test");
        assert_eq!(parsed.score, 0.95);
    }

    #[test]
    fn test_search_result_snippet_none_skipped() {
        let result = SearchResult {
            id: "test".to_string(),
            content: "content".to_string(),
            entity_type: "concept".to_string(),
            score: 0.5,
            snippet: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("snippet"));
    }

    #[test]
    fn test_reopen_existing_index() -> Result<()> {
        let temp_path =
            env::temp_dir().join(format!("test_tantivy_reopen_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&temp_path);

        // Create and populate
        {
            let index = TantivyIndex::new(&temp_path)?;
            index.index_document(
                "persist-1",
                "Persistent document about databases",
                "concept",
                &chrono::Utc::now().to_rfc3339(),
            )?;
        }

        // Reopen and verify data persists
        {
            let index = TantivyIndex::new(&temp_path)?;
            let results = index.search("databases", 10)?;
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].id, "persist-1");
        }

        let _ = std::fs::remove_dir_all(&temp_path);
        Ok(())
    }

    #[test]
    fn test_delete_nonexistent_document() -> Result<()> {
        let temp_path =
            env::temp_dir().join(format!("test_tantivy_del_none_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&temp_path);

        let index = TantivyIndex::new(&temp_path)?;
        // Should not error when deleting a non-existent doc
        index.delete_document("nonexistent-id")?;

        let _ = std::fs::remove_dir_all(&temp_path);
        Ok(())
    }
}
