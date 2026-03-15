/// Relation gazetteer: graph-derived relation lookup.
///
/// Mirrors the entity gazetteer pattern: builds a lookup table from existing
/// graph edges. When two known entities appear in text, checks if a relation
/// already exists between them in the graph.
///
/// Self-improving: as new edges are added to the graph, the gazetteer grows.
/// Persisted as `.brain.relgaz` sidecar (tab-separated).

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::rel_traits::{CandidateRelation, RelationExtractionInput, RelationExtractor};
use crate::types::ExtractionMethod;

/// A single relation gazetteer entry.
#[derive(Debug, Clone)]
pub struct RelGazetteerEntry {
    /// Lowercase head entity label.
    pub head: String,
    /// Lowercase tail entity label.
    pub tail: String,
    /// Relation type.
    pub rel_type: String,
    /// Confidence of the edge in the graph.
    pub confidence: f32,
}

/// Graph-derived relation gazetteer.
pub struct RelationGazetteer {
    /// Map from (head_lower, tail_lower) -> list of known relations.
    entries: HashMap<(String, String), Vec<RelGazetteerEntry>>,
    /// All known relation types (fed to GLiREL as candidate labels).
    known_rel_types: HashSet<String>,
    /// Path to the .brain.relgaz sidecar file.
    path: PathBuf,
}

impl RelationGazetteer {
    /// Create a new empty relation gazetteer.
    pub fn new(brain_path: &Path) -> Self {
        let path = brain_path.with_extension("relgaz");
        Self {
            entries: HashMap::new(),
            known_rel_types: HashSet::new(),
            path,
        }
    }

    /// Build from current graph state — scans all edges.
    pub fn build_from_graph(&mut self, graph: &engram_core::graph::Graph) {
        let start = std::time::Instant::now();
        self.entries.clear();
        self.known_rel_types.clear();

        let nodes = match graph.all_nodes() {
            Ok(n) => n,
            Err(e) => {
                tracing::error!("relation gazetteer build failed: {}", e);
                return;
            }
        };

        let mut total_entries = 0usize;

        for node in &nodes {
            let edges = match graph.edges_from(&node.label) {
                Ok(e) => e,
                Err(_) => continue,
            };

            for edge in edges {
                let head = node.label.to_lowercase();
                let tail = edge.to.to_lowercase();
                let rel_type = edge.relationship.clone();

                self.known_rel_types.insert(rel_type.clone());

                let entry = RelGazetteerEntry {
                    head: head.clone(),
                    tail: tail.clone(),
                    rel_type,
                    confidence: edge.confidence,
                };

                self.entries
                    .entry((head, tail))
                    .or_default()
                    .push(entry);
                total_entries += 1;
            }
        }

        tracing::info!(
            entries = total_entries,
            rel_types = self.known_rel_types.len(),
            duration_ms = start.elapsed().as_millis(),
            "relation gazetteer built from graph"
        );
    }

    /// Look up known relations between two entities (checks both directions).
    pub fn lookup(&self, head: &str, tail: &str) -> Vec<&RelGazetteerEntry> {
        let h = head.to_lowercase();
        let t = tail.to_lowercase();

        let mut results = Vec::new();

        if let Some(entries) = self.entries.get(&(h.clone(), t.clone())) {
            results.extend(entries.iter());
        }
        if let Some(entries) = self.entries.get(&(t, h)) {
            results.extend(entries.iter());
        }

        results
    }

    /// Get all known relation type labels.
    pub fn known_relation_types(&self) -> &HashSet<String> {
        &self.known_rel_types
    }

    /// Number of entity-pair entries.
    pub fn len(&self) -> usize {
        self.entries.values().map(|v| v.len()).sum()
    }

    /// Whether the gazetteer is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Add a single entry.
    pub fn insert(&mut self, entry: RelGazetteerEntry) {
        self.known_rel_types.insert(entry.rel_type.clone());
        self.entries
            .entry((entry.head.clone(), entry.tail.clone()))
            .or_default()
            .push(entry);
    }

    /// Persist to disk as tab-separated file.
    pub fn save(&self) -> Result<(), crate::IngestError> {
        let file = std::fs::File::create(&self.path).map_err(crate::IngestError::Io)?;
        let mut writer = BufWriter::new(file);

        for entries in self.entries.values() {
            for entry in entries {
                writeln!(
                    writer,
                    "{}\t{}\t{}\t{:.4}",
                    entry.head, entry.tail, entry.rel_type, entry.confidence,
                )
                .map_err(crate::IngestError::Io)?;
            }
        }

        writer.flush().map_err(crate::IngestError::Io)?;
        tracing::debug!(
            entries = self.len(),
            path = %self.path.display(),
            "relation gazetteer saved"
        );
        Ok(())
    }

    /// Load from disk.
    pub fn load(brain_path: &Path) -> Result<Self, crate::IngestError> {
        let path = brain_path.with_extension("relgaz");
        let mut gaz = Self {
            entries: HashMap::new(),
            known_rel_types: HashSet::new(),
            path: path.clone(),
        };

        if !path.exists() {
            return Ok(gaz);
        }

        let file = std::fs::File::open(&path).map_err(crate::IngestError::Io)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line.map_err(crate::IngestError::Io)?;
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 4 {
                continue;
            }

            let confidence: f32 = parts[3].parse().unwrap_or(0.0);
            let rel_type = parts[2].to_string();

            gaz.known_rel_types.insert(rel_type.clone());
            gaz.entries
                .entry((parts[0].to_string(), parts[1].to_string()))
                .or_default()
                .push(RelGazetteerEntry {
                    head: parts[0].to_string(),
                    tail: parts[1].to_string(),
                    rel_type,
                    confidence,
                });
        }

        tracing::info!(
            entries = gaz.len(),
            rel_types = gaz.known_rel_types.len(),
            path = %path.display(),
            "relation gazetteer loaded from disk"
        );

        Ok(gaz)
    }
}

/// Relation gazetteer as a `RelationExtractor` backend.
pub struct RelationGazetteerExtractor {
    gazetteer: std::sync::Arc<tokio::sync::RwLock<RelationGazetteer>>,
}

impl RelationGazetteerExtractor {
    pub fn new(gazetteer: std::sync::Arc<tokio::sync::RwLock<RelationGazetteer>>) -> Self {
        Self { gazetteer }
    }
}

impl RelationExtractor for RelationGazetteerExtractor {
    fn extract_relations(&self, input: &RelationExtractionInput) -> Vec<CandidateRelation> {
        let gaz = self.gazetteer.blocking_read();
        let mut relations = Vec::new();

        // Check every entity pair
        for i in 0..input.entities.len() {
            for j in (i + 1)..input.entities.len() {
                let matches = gaz.lookup(&input.entities[i].text, &input.entities[j].text);

                for m in matches {
                    // Determine head/tail based on the original direction in gazetteer
                    let (head_idx, tail_idx) =
                        if m.head == input.entities[i].text.to_lowercase() {
                            (i, j)
                        } else {
                            (j, i)
                        };

                    relations.push(CandidateRelation {
                        head_idx,
                        tail_idx,
                        rel_type: m.rel_type.clone(),
                        confidence: m.confidence * 0.95, // slight discount like entity gazetteer
                        method: ExtractionMethod::Gazetteer,
                    });
                }
            }
        }

        relations
    }

    fn name(&self) -> &str {
        "relation-gazetteer"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ExtractedEntity;
    use tempfile::TempDir;

    fn make_gazetteer(dir: &TempDir) -> RelationGazetteer {
        let path = dir.path().join("test.brain");
        let mut gaz = RelationGazetteer::new(&path);

        gaz.insert(RelGazetteerEntry {
            head: "tim cook".into(),
            tail: "apple inc.".into(),
            rel_type: "ceo_of".into(),
            confidence: 0.9,
        });
        gaz.insert(RelGazetteerEntry {
            head: "apple inc.".into(),
            tail: "cupertino".into(),
            rel_type: "headquartered_in".into(),
            confidence: 0.85,
        });

        gaz
    }

    #[test]
    fn lookup_finds_forward_and_reverse() {
        let dir = TempDir::new().unwrap();
        let gaz = make_gazetteer(&dir);

        let results = gaz.lookup("Tim Cook", "Apple Inc.");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].rel_type, "ceo_of");

        // Reverse direction also found
        let results = gaz.lookup("Apple Inc.", "Tim Cook");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn known_rel_types_populated() {
        let dir = TempDir::new().unwrap();
        let gaz = make_gazetteer(&dir);
        let types = gaz.known_relation_types();
        assert!(types.contains("ceo_of"));
        assert!(types.contains("headquartered_in"));
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let gaz = make_gazetteer(&dir);
        gaz.save().unwrap();

        let loaded = RelationGazetteer::load(&path).unwrap();
        assert_eq!(loaded.len(), gaz.len());
        assert!(loaded.known_relation_types().contains("ceo_of"));

        let results = loaded.lookup("Tim Cook", "Apple Inc.");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn extractor_finds_relations_between_known_entities() {
        let dir = TempDir::new().unwrap();
        let gaz = make_gazetteer(&dir);
        let gaz = std::sync::Arc::new(tokio::sync::RwLock::new(gaz));
        let extractor = RelationGazetteerExtractor::new(gaz);

        let input = RelationExtractionInput {
            text: "Tim Cook announced Apple Inc. results".into(),
            entities: vec![
                ExtractedEntity {
                    text: "Tim Cook".into(),
                    entity_type: "PERSON".into(),
                    span: (0, 8),
                    confidence: 0.9,
                    method: ExtractionMethod::Gazetteer,
                    language: "en".into(),
                    resolved_to: Some(1),
                },
                ExtractedEntity {
                    text: "Apple Inc.".into(),
                    entity_type: "ORG".into(),
                    span: (19, 29),
                    confidence: 0.85,
                    method: ExtractionMethod::Gazetteer,
                    language: "en".into(),
                    resolved_to: Some(2),
                },
            ],
            language: "en".into(),
            area_of_interest: None,
        };

        let relations = extractor.extract_relations(&input);
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].rel_type, "ceo_of");
        assert_eq!(relations[0].head_idx, 0);
        assert_eq!(relations[0].tail_idx, 1);
    }

    #[test]
    fn empty_gazetteer_returns_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let gaz = RelationGazetteer::new(&path);
        assert!(gaz.is_empty());
        assert_eq!(gaz.len(), 0);
    }
}
