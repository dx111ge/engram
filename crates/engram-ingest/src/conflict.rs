/// Conflict detection: checks incoming facts against existing graph data.
///
/// Detects contradictions (e.g. "CEO of Apple: Tim Cook" vs "CEO of Apple: Steve Jobs")
/// and flags them for resolution.

use crate::types::{ConflictRecord, ProcessedFact};

/// Configuration for conflict detection.
#[derive(Debug, Clone)]
pub struct ConflictConfig {
    /// Properties that are single-valued (conflict if different value exists).
    pub singular_properties: Vec<String>,
    /// Minimum confidence of existing fact to trigger a conflict.
    pub min_existing_confidence: f32,
}

impl Default for ConflictConfig {
    fn default() -> Self {
        Self {
            singular_properties: vec![
                "ceo".into(),
                "president".into(),
                "capital".into(),
                "population".into(),
                "founded".into(),
            ],
            min_existing_confidence: 0.3,
        }
    }
}

/// Detect conflicts between incoming facts and the existing graph.
///
/// Runs under a read lock — reports conflicts but doesn't resolve them.
pub struct ConflictDetector {
    config: ConflictConfig,
}

impl ConflictDetector {
    pub fn new(config: ConflictConfig) -> Self {
        Self { config }
    }

    /// Check a single fact for conflicts against the graph.
    pub fn check(
        &self,
        fact: &ProcessedFact,
        graph: &engram_core::graph::Graph,
    ) -> Vec<ConflictRecord> {
        let mut conflicts = Vec::new();

        // Check if entity already exists with different properties
        let node_id = match graph.find_node_id(&fact.entity) {
            Ok(Some(id)) => id,
            _ => return conflicts, // new entity, no conflicts possible
        };

        // Check singular properties for contradictions
        for (key, new_value) in &fact.properties {
            if !self.config.singular_properties.contains(key) {
                continue;
            }

            // Check if graph has a different value for this property
            if let Ok(Some(existing)) = graph.get_property(&fact.entity, key) {
                if existing != *new_value {
                    conflicts.push(ConflictRecord {
                        existing_node: node_id,
                        description: format!(
                            "property '{}': existing='{}', incoming='{}'",
                            key, existing, new_value
                        ),
                        severity: 0.7,
                    });
                }
            }
        }

        // Check for contradictory relations (same from+rel_type, different to)
        for rel in &fact.relations {
            if let Ok(edges) = graph.edges_from(&rel.from) {
                for edge in &edges {
                    if edge.relationship == rel.rel_type && edge.to != rel.to {
                        // Check if this is a singular relation type
                        if self.config.singular_properties.contains(&rel.rel_type) {
                            conflicts.push(ConflictRecord {
                                existing_node: node_id,
                                description: format!(
                                    "relation '{}': existing target='{}', incoming target='{}'",
                                    rel.rel_type, edge.to, rel.to
                                ),
                                severity: 0.8,
                            });
                        }
                    }
                }
            }
        }

        conflicts
    }

    /// Check a batch of facts for conflicts.
    pub fn check_batch(
        &self,
        facts: &mut [ProcessedFact],
        graph: &engram_core::graph::Graph,
    ) -> u32 {
        let mut total_conflicts = 0u32;

        for fact in facts.iter_mut() {
            let conflicts = self.check(fact, graph);
            total_conflicts += conflicts.len() as u32;
            fact.conflicts.extend(conflicts);
        }

        total_conflicts
    }
}

impl Default for ConflictDetector {
    fn default() -> Self {
        Self::new(ConflictConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ExtractionMethod, Provenance};
    use tempfile::TempDir;

    fn make_fact(entity: &str) -> ProcessedFact {
        ProcessedFact {
            entity: entity.into(),
            entity_type: Some("ORG".into()),
            properties: Default::default(),
            confidence: 0.8,
            provenance: Provenance {
                source: "test".into(),
                source_url: None,
                author: None,
                extraction_method: ExtractionMethod::Manual,
                fetched_at: 0,
                ingested_at: 0,
            },
            extraction_method: ExtractionMethod::Manual,
            language: "en".into(),
            relations: vec![],
            conflicts: vec![],
            resolution: None,
        }
    }

    #[test]
    fn no_conflict_for_new_entity() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let graph = engram_core::graph::Graph::create(&path).unwrap();

        let detector = ConflictDetector::default();
        let fact = make_fact("Unknown Corp");
        let conflicts = detector.check(&fact, &graph);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn detects_property_conflict() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut graph = engram_core::graph::Graph::create(&path).unwrap();

        let prov = engram_core::graph::Provenance::user("test");
        graph.store_with_confidence("Apple Inc.", 0.9, &prov).unwrap();
        graph.set_property("Apple Inc.", "ceo", "Tim Cook").unwrap();

        let detector = ConflictDetector::default();
        let mut fact = make_fact("Apple Inc.");
        fact.properties.insert("ceo".into(), "Steve Jobs".into());

        let conflicts = detector.check(&fact, &graph);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].description.contains("Tim Cook"));
        assert!(conflicts[0].description.contains("Steve Jobs"));
    }

    #[test]
    fn no_conflict_for_matching_property() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut graph = engram_core::graph::Graph::create(&path).unwrap();

        let prov = engram_core::graph::Provenance::user("test");
        graph.store_with_confidence("Apple Inc.", 0.9, &prov).unwrap();
        graph.set_property("Apple Inc.", "ceo", "Tim Cook").unwrap();

        let detector = ConflictDetector::default();
        let mut fact = make_fact("Apple Inc.");
        fact.properties.insert("ceo".into(), "Tim Cook".into());

        let conflicts = detector.check(&fact, &graph);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn non_singular_property_ignored() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut graph = engram_core::graph::Graph::create(&path).unwrap();

        let prov = engram_core::graph::Provenance::user("test");
        graph.store_with_confidence("Apple Inc.", 0.9, &prov).unwrap();
        graph.set_property("Apple Inc.", "industry", "tech").unwrap();

        let detector = ConflictDetector::default();
        let mut fact = make_fact("Apple Inc.");
        fact.properties.insert("industry".into(), "consumer electronics".into());

        // "industry" is not in singular_properties, so no conflict
        let conflicts = detector.check(&fact, &graph);
        assert!(conflicts.is_empty());
    }
}
