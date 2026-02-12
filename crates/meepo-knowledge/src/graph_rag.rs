//! GraphRAG — relationship-aware retrieval
//!
//! Enhances standard search by traversing the knowledge graph's entity
//! relationships to pull in contextually connected entities. Combines
//! keyword/vector search results with graph traversal for richer context.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tracing::{debug, info};

use crate::sqlite::{Entity, KnowledgeDb, Relationship};

/// Configuration for GraphRAG retrieval
#[derive(Debug, Clone)]
pub struct GraphRagConfig {
    /// Maximum number of relationship hops to traverse
    pub max_hops: usize,
    /// Maximum total entities to return after expansion
    pub max_expanded_results: usize,
    /// Weight decay per hop (multiplied each hop)
    pub hop_decay: f32,
    /// Whether to include relationship metadata in context
    pub include_relationship_context: bool,
}

impl Default for GraphRagConfig {
    fn default() -> Self {
        Self {
            max_hops: 2,
            max_expanded_results: 20,
            hop_decay: 0.5,
            include_relationship_context: true,
        }
    }
}

/// An entity with its graph-derived relevance score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredEntity {
    pub entity: Entity,
    /// Combined relevance score (search score + graph proximity)
    pub score: f32,
    /// How this entity was found
    pub source: EntitySource,
    /// Relationships connecting this entity to the query results
    pub connecting_relationships: Vec<Relationship>,
}

/// How an entity was discovered during retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EntitySource {
    /// Found directly via keyword/vector search
    DirectMatch { search_score: f32 },
    /// Found via graph traversal from a direct match
    GraphExpansion { hops: usize, from_entity_id: String },
}

/// Expand search results by traversing the knowledge graph.
///
/// Starting from a set of seed entity IDs (from keyword/vector search),
/// traverses relationships up to `max_hops` deep, scoring discovered
/// entities by their proximity to the seeds.
pub async fn graph_expand(
    db: &KnowledgeDb,
    seed_ids: &[(String, f32)], // (entity_id, initial_score) from search
    config: &GraphRagConfig,
) -> Result<Vec<ScoredEntity>> {
    let mut all_entities: HashMap<String, ScoredEntity> = HashMap::new();
    let mut visited: HashSet<String> = HashSet::new();

    // Add seed entities
    for (entity_id, score) in seed_ids {
        if let Some(entity) = db.get_entity(entity_id).await? {
            visited.insert(entity_id.clone());
            all_entities.insert(
                entity_id.clone(),
                ScoredEntity {
                    entity,
                    score: *score,
                    source: EntitySource::DirectMatch {
                        search_score: *score,
                    },
                    connecting_relationships: Vec::new(),
                },
            );
        }
    }

    // BFS expansion through relationships
    let mut frontier: Vec<(String, f32, usize)> = seed_ids
        .iter()
        .map(|(id, score)| (id.clone(), *score, 0))
        .collect();

    for hop in 0..config.max_hops {
        if frontier.is_empty() || all_entities.len() >= config.max_expanded_results {
            break;
        }

        let decay = config.hop_decay.powi((hop + 1) as i32);
        let mut next_frontier = Vec::new();

        for (entity_id, parent_score, _) in &frontier {
            let relationships = db
                .get_relationships_for(entity_id)
                .await
                .unwrap_or_default();

            for rel in relationships {
                // Find the other end of the relationship
                let neighbor_id = if rel.source_id == *entity_id {
                    &rel.target_id
                } else {
                    &rel.source_id
                };

                if visited.contains(neighbor_id) {
                    // If already found, just add the connecting relationship
                    if let Some(existing) = all_entities.get_mut(neighbor_id) {
                        existing.connecting_relationships.push(rel.clone());
                    }
                    continue;
                }

                if all_entities.len() >= config.max_expanded_results {
                    break;
                }

                visited.insert(neighbor_id.clone());

                if let Some(neighbor_entity) = db.get_entity(neighbor_id).await? {
                    let neighbor_score = parent_score * decay;

                    all_entities.insert(
                        neighbor_id.clone(),
                        ScoredEntity {
                            entity: neighbor_entity,
                            score: neighbor_score,
                            source: EntitySource::GraphExpansion {
                                hops: hop + 1,
                                from_entity_id: entity_id.clone(),
                            },
                            connecting_relationships: vec![rel.clone()],
                        },
                    );

                    next_frontier.push((neighbor_id.clone(), neighbor_score, hop + 1));
                }
            }
        }

        frontier = next_frontier;
    }

    // Sort by score descending
    let mut results: Vec<ScoredEntity> = all_entities.into_values().collect();
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(config.max_expanded_results);

    debug!(
        "GraphRAG expanded {} seeds to {} results ({} hops)",
        seed_ids.len(),
        results.len(),
        config.max_hops
    );

    Ok(results)
}

/// Format GraphRAG results into a context string for the LLM.
pub fn format_graph_context(results: &[ScoredEntity], config: &GraphRagConfig) -> String {
    if results.is_empty() {
        return String::new();
    }

    let mut context = String::new();

    // Group by source type
    let direct: Vec<&ScoredEntity> = results
        .iter()
        .filter(|r| matches!(r.source, EntitySource::DirectMatch { .. }))
        .collect();
    let expanded: Vec<&ScoredEntity> = results
        .iter()
        .filter(|r| matches!(r.source, EntitySource::GraphExpansion { .. }))
        .collect();

    if !direct.is_empty() {
        context.push_str("### Direct Matches\n\n");
        for scored in &direct {
            context.push_str(&format!(
                "- **{}** ({})",
                scored.entity.name, scored.entity.entity_type
            ));
            if let Some(metadata) = &scored.entity.metadata {
                context.push_str(&format!(": {}", metadata));
            }
            context.push('\n');
        }
        context.push('\n');
    }

    if !expanded.is_empty() {
        context.push_str("### Related Knowledge\n\n");
        for scored in &expanded {
            let hop_info = match &scored.source {
                EntitySource::GraphExpansion { hops, .. } => format!("{} hop(s) away", hops),
                _ => String::new(),
            };
            context.push_str(&format!(
                "- **{}** ({}) [{}]",
                scored.entity.name, scored.entity.entity_type, hop_info
            ));
            if let Some(metadata) = &scored.entity.metadata {
                context.push_str(&format!(": {}", metadata));
            }
            context.push('\n');

            // Add relationship context
            if config.include_relationship_context {
                for rel in &scored.connecting_relationships {
                    context.push_str(&format!(
                        "  → Relationship: {} ({})\n",
                        rel.relation_type,
                        if rel.source_id == scored.entity.id {
                            "outgoing"
                        } else {
                            "incoming"
                        }
                    ));
                }
            }
        }
        context.push('\n');
    }

    context
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = GraphRagConfig::default();
        assert_eq!(config.max_hops, 2);
        assert_eq!(config.max_expanded_results, 20);
        assert!((config.hop_decay - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_format_empty_results() {
        let config = GraphRagConfig::default();
        let result = format_graph_context(&[], &config);
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_graph_expand_with_db() {
        let temp = tempfile::TempDir::new().unwrap();
        let db = KnowledgeDb::new(&temp.path().join("test.db")).unwrap();

        // Create entities
        let id_a = db
            .insert_entity("Rust", "language", None)
            .await
            .unwrap();
        let id_b = db
            .insert_entity("Systems Programming", "domain", None)
            .await
            .unwrap();
        let id_c = db
            .insert_entity("Memory Safety", "concept", None)
            .await
            .unwrap();

        // Create relationships: Rust -> Systems Programming -> Memory Safety
        db.insert_relationship(&id_a, &id_b, "used_for", None)
            .await
            .unwrap();
        db.insert_relationship(&id_b, &id_c, "enables", None)
            .await
            .unwrap();

        let config = GraphRagConfig {
            max_hops: 2,
            max_expanded_results: 10,
            ..Default::default()
        };

        // Search starting from Rust
        let seeds = vec![(id_a.clone(), 1.0)];
        let results = graph_expand(&db, &seeds, &config).await.unwrap();

        // Should find Rust (direct), Systems Programming (1 hop), Memory Safety (2 hops)
        assert_eq!(results.len(), 3);

        // Verify scores decrease with hops
        let rust_score = results.iter().find(|r| r.entity.id == id_a).unwrap().score;
        let sp_score = results.iter().find(|r| r.entity.id == id_b).unwrap().score;
        let ms_score = results.iter().find(|r| r.entity.id == id_c).unwrap().score;
        assert!(rust_score > sp_score);
        assert!(sp_score > ms_score);
    }
}
