/// Graph gazetteer: fast entity lookup from persisted sorted string table.
///
/// The gazetteer is a reverse index: surface_form -> GazetteerEntry.
/// Built from graph entities + their alias properties, persisted as a
/// sorted string table (SST) for O(log n) exact/prefix lookups.
///
/// Self-updates via GraphEvent subscription.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::traits::Extractor;
use crate::types::{DetectedLanguage, ExtractedEntity, ExtractionMethod};

/// A single gazetteer entry mapping a surface form to a graph node.
#[derive(Debug, Clone)]
pub struct GazetteerEntry {
    /// The canonical surface form (lowercased for matching).
    pub surface: String,
    /// Graph node ID.
    pub node_id: u64,
    /// Entity type (PERSON, ORG, LOC, etc.).
    pub entity_type: String,
    /// Confidence of the source entity in the graph.
    pub confidence: f32,
    /// Whether this is an alias (true) or the primary label (false).
    pub is_alias: bool,
}

/// Persisted, sorted-string-table-backed gazetteer.
///
/// In memory: sorted `Vec<GazetteerEntry>` for binary search.
/// On disk: `.brain.gazetteer` file, one entry per line, sorted.
pub struct GraphGazetteer {
    /// Sorted by lowercase surface form for binary search.
    entries: Vec<GazetteerEntry>,
    /// surface_lower -> index into entries (for O(1) exact match after build).
    exact_index: HashMap<String, Vec<usize>>,
    /// Path to the .brain.gazetteer file.
    path: PathBuf,
    /// Minimum confidence for a graph entity to enter the gazetteer.
    min_confidence: f32,
}

impl GraphGazetteer {
    /// Create a new empty gazetteer.
    pub fn new(brain_path: &Path, min_confidence: f32) -> Self {
        let path = brain_path.with_extension("gazetteer");
        Self {
            entries: Vec::new(),
            exact_index: HashMap::new(),
            path,
            min_confidence,
        }
    }

    /// Build the gazetteer from the current graph state.
    /// Scans all nodes above `min_confidence`, extracts labels + alias properties.
    pub fn build_from_graph(&mut self, graph: &engram_core::graph::Graph) {
        let start = std::time::Instant::now();
        self.entries.clear();
        self.exact_index.clear();

        let nodes = match graph.all_nodes() {
            Ok(n) => n,
            Err(e) => {
                tracing::error!("gazetteer build failed: {}", e);
                return;
            }
        };

        for node in &nodes {
            if node.confidence < self.min_confidence {
                continue;
            }

            let node_id = match graph.find_node_id(&node.label) {
                Ok(Some(id)) => id,
                _ => continue,
            };

            let entity_type = node.node_type.clone().unwrap_or_default();

            // Primary label
            self.entries.push(GazetteerEntry {
                surface: node.label.to_lowercase(),
                node_id,
                entity_type: entity_type.clone(),
                confidence: node.confidence,
                is_alias: false,
            });

            // Alias properties (alias:en, alias:de, alias:zh, etc.)
            for (key, value) in &node.properties {
                if key.starts_with("alias:") || key == "alias" {
                    self.entries.push(GazetteerEntry {
                        surface: value.to_lowercase(),
                        node_id,
                        entity_type: entity_type.clone(),
                        confidence: node.confidence,
                        is_alias: true,
                    });
                }
            }
        }

        // Sort for binary search
        self.entries.sort_by(|a, b| a.surface.cmp(&b.surface));

        // Build exact index
        for (idx, entry) in self.entries.iter().enumerate() {
            self.exact_index
                .entry(entry.surface.clone())
                .or_default()
                .push(idx);
        }

        tracing::info!(
            entries = self.entries.len(),
            duration_ms = start.elapsed().as_millis(),
            "gazetteer built from graph"
        );
    }

    /// Exact match lookup (case-insensitive).
    pub fn lookup_exact(&self, surface: &str) -> Vec<&GazetteerEntry> {
        let key = surface.to_lowercase();
        match self.exact_index.get(&key) {
            Some(indices) => indices.iter().map(|&i| &self.entries[i]).collect(),
            None => Vec::new(),
        }
    }

    /// Prefix match lookup — returns all entries whose surface form starts with `prefix`.
    /// Uses binary search on the sorted entries for O(log n) start point.
    pub fn lookup_prefix(&self, prefix: &str) -> Vec<&GazetteerEntry> {
        let key = prefix.to_lowercase();
        let start = self.entries.partition_point(|e| e.surface.as_str() < key.as_str());

        let mut results = Vec::new();
        for entry in &self.entries[start..] {
            if entry.surface.starts_with(&key) {
                results.push(entry);
            } else {
                break; // sorted, so no more matches
            }
        }
        results
    }

    /// Number of entries in the gazetteer.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the gazetteer is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Add a single entry (e.g. from a GraphEvent). Maintains sort order.
    pub fn insert(&mut self, entry: GazetteerEntry) {
        let key = entry.surface.clone();
        let pos = self.entries.partition_point(|e| e.surface < key);
        self.entries.insert(pos, entry);
        // Insertion shifts all subsequent indices — rebuild
        self.rebuild_exact_index();
    }

    /// Remove all entries for a given node_id (e.g. on FactDeleted).
    pub fn remove_node(&mut self, node_id: u64) {
        self.entries.retain(|e| e.node_id != node_id);
        self.rebuild_exact_index();
    }

    /// Update confidence for all entries of a node.
    pub fn update_confidence(&mut self, node_id: u64, new_confidence: f32) {
        if new_confidence < self.min_confidence {
            // Below threshold — remove entirely
            self.remove_node(node_id);
            return;
        }
        for entry in &mut self.entries {
            if entry.node_id == node_id {
                entry.confidence = new_confidence;
            }
        }
    }

    /// Persist the gazetteer to disk as a sorted text file.
    /// Format: `surface\tnode_id\tentity_type\tconfidence\tis_alias`
    pub fn save(&self) -> Result<(), crate::IngestError> {
        let file = std::fs::File::create(&self.path)
            .map_err(|e| crate::IngestError::Io(e))?;
        let mut writer = BufWriter::new(file);

        for entry in &self.entries {
            writeln!(
                writer,
                "{}\t{}\t{}\t{:.4}\t{}",
                entry.surface,
                entry.node_id,
                entry.entity_type,
                entry.confidence,
                if entry.is_alias { 1 } else { 0 }
            )
            .map_err(|e| crate::IngestError::Io(e))?;
        }

        writer.flush().map_err(|e| crate::IngestError::Io(e))?;
        tracing::debug!(entries = self.entries.len(), path = %self.path.display(), "gazetteer saved");
        Ok(())
    }

    /// Load the gazetteer from disk.
    pub fn load(brain_path: &Path, min_confidence: f32) -> Result<Self, crate::IngestError> {
        let path = brain_path.with_extension("gazetteer");
        let mut gaz = Self {
            entries: Vec::new(),
            exact_index: HashMap::new(),
            path: path.clone(),
            min_confidence,
        };

        if !path.exists() {
            return Ok(gaz);
        }

        let file = std::fs::File::open(&path).map_err(|e| crate::IngestError::Io(e))?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line.map_err(|e| crate::IngestError::Io(e))?;
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 5 {
                continue;
            }

            let confidence: f32 = parts[3].parse().unwrap_or(0.0);
            if confidence < min_confidence {
                continue;
            }

            gaz.entries.push(GazetteerEntry {
                surface: parts[0].to_string(),
                node_id: parts[1].parse().unwrap_or(0),
                entity_type: parts[2].to_string(),
                confidence,
                is_alias: parts[4] == "1",
            });
        }

        // Already sorted on disk, but verify
        gaz.entries.sort_by(|a, b| a.surface.cmp(&b.surface));

        // Build exact index
        for (idx, entry) in gaz.entries.iter().enumerate() {
            gaz.exact_index
                .entry(entry.surface.clone())
                .or_default()
                .push(idx);
        }

        tracing::info!(
            entries = gaz.entries.len(),
            path = %path.display(),
            "gazetteer loaded from disk"
        );

        Ok(gaz)
    }

    /// Rebuild the entire exact index from entries.
    fn rebuild_exact_index(&mut self) {
        self.exact_index.clear();
        for (idx, entry) in self.entries.iter().enumerate() {
            self.exact_index
                .entry(entry.surface.clone())
                .or_default()
                .push(idx);
        }
    }
}

/// Gazetteer as an NER Extractor — plugs directly into the NER chain.
///
/// Wraps a shared `GraphGazetteer` behind `Arc<RwLock>` for concurrent
/// pipeline access.
pub struct GazetteerExtractor {
    gazetteer: Arc<RwLock<GraphGazetteer>>,
}

impl GazetteerExtractor {
    pub fn new(gazetteer: Arc<RwLock<GraphGazetteer>>) -> Self {
        Self { gazetteer }
    }
}

impl Extractor for GazetteerExtractor {
    fn extract(&self, text: &str, _lang: &DetectedLanguage) -> Vec<ExtractedEntity> {
        // Block on the async lock — this runs in a sync NER context.
        // Safe because gazetteer reads are fast (microseconds).
        let gaz = self.gazetteer.blocking_read();
        let lower = text.to_lowercase();
        let mut entities = Vec::new();

        // Scan text for known surface forms.
        // Simple approach: check each word and multi-word span against the gazetteer.
        let words: Vec<(usize, &str)> = lower
            .split_whitespace()
            .scan(0usize, |pos, word| {
                let start = lower[*pos..].find(word).unwrap_or(0) + *pos;
                *pos = start + word.len();
                Some((start, word))
            })
            .collect();

        for window_size in (1..=4).rev() {
            // Try 4-word, 3-word, 2-word, 1-word spans (longest match first)
            for window in words.windows(window_size) {
                let span_start = window[0].0;
                let last = window.last().unwrap();
                let span_end = last.0 + last.1.len();
                let span_text = &lower[span_start..span_end];

                let matches = gaz.lookup_exact(span_text);
                if !matches.is_empty() {
                    // Take highest-confidence match
                    let best = matches
                        .iter()
                        .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap())
                        .unwrap();

                    // Get original-case text from input
                    let original = &text[span_start..span_end];

                    entities.push(ExtractedEntity {
                        text: original.to_string(),
                        entity_type: best.entity_type.clone(),
                        span: (span_start, span_end),
                        confidence: best.confidence * 0.95, // slight discount vs graph confidence
                        method: ExtractionMethod::Gazetteer,
                        language: _lang.code.clone(),
                        resolved_to: Some(best.node_id),
                    });
                }
            }
        }

        // Deduplicate: if a longer match covers a shorter one, keep the longer
        entities.sort_by_key(|e| e.span.0);
        let mut deduped: Vec<ExtractedEntity> = Vec::new();
        for entity in entities {
            let dominated = deduped.iter().any(|existing| {
                existing.span.0 <= entity.span.0 && existing.span.1 >= entity.span.1
            });
            if !dominated {
                deduped.push(entity);
            }
        }

        deduped
    }

    fn name(&self) -> &str {
        "graph-gazetteer"
    }

    fn method(&self) -> ExtractionMethod {
        ExtractionMethod::Gazetteer
    }

    fn supported_languages(&self) -> Vec<String> {
        vec![] // all languages (aliases handle multilingual)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_gazetteer(dir: &TempDir) -> GraphGazetteer {
        let path = dir.path().join("test.brain");
        let mut gaz = GraphGazetteer::new(&path, 0.3);

        gaz.insert(GazetteerEntry {
            surface: "apple inc.".into(),
            node_id: 1,
            entity_type: "ORG".into(),
            confidence: 0.9,
            is_alias: false,
        });
        gaz.insert(GazetteerEntry {
            surface: "apple".into(),
            node_id: 1,
            entity_type: "ORG".into(),
            confidence: 0.9,
            is_alias: true,
        });
        gaz.insert(GazetteerEntry {
            surface: "tim cook".into(),
            node_id: 2,
            entity_type: "PERSON".into(),
            confidence: 0.85,
            is_alias: false,
        });
        gaz.insert(GazetteerEntry {
            surface: "苹果公司".into(),
            node_id: 1,
            entity_type: "ORG".into(),
            confidence: 0.9,
            is_alias: true,
        });

        gaz
    }

    #[test]
    fn exact_lookup_case_insensitive() {
        let dir = TempDir::new().unwrap();
        let gaz = make_gazetteer(&dir);

        let results = gaz.lookup_exact("Apple Inc.");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].node_id, 1);

        let results = gaz.lookup_exact("TIM COOK");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].node_id, 2);
    }

    #[test]
    fn prefix_lookup() {
        let dir = TempDir::new().unwrap();
        let gaz = make_gazetteer(&dir);

        let results = gaz.lookup_prefix("apple");
        assert_eq!(results.len(), 2); // "apple" and "apple inc."
    }

    #[test]
    fn multilingual_alias_lookup() {
        let dir = TempDir::new().unwrap();
        let gaz = make_gazetteer(&dir);

        let results = gaz.lookup_exact("苹果公司");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].node_id, 1);
        assert!(results[0].is_alias);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let gaz = make_gazetteer(&dir);

        gaz.save().unwrap();

        let loaded = GraphGazetteer::load(&path, 0.3).unwrap();
        assert_eq!(loaded.len(), gaz.len());

        let results = loaded.lookup_exact("tim cook");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].node_id, 2);

        // Multilingual survives roundtrip
        let results = loaded.lookup_exact("苹果公司");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn remove_node_cleans_all_entries() {
        let dir = TempDir::new().unwrap();
        let mut gaz = make_gazetteer(&dir);
        assert_eq!(gaz.len(), 4);

        gaz.remove_node(1); // removes Apple (label + 2 aliases)
        assert_eq!(gaz.len(), 1); // only Tim Cook left
        assert!(gaz.lookup_exact("apple").is_empty());
        assert!(gaz.lookup_exact("苹果公司").is_empty());
        assert_eq!(gaz.lookup_exact("tim cook").len(), 1);
    }

    #[test]
    fn update_confidence_below_threshold_removes() {
        let dir = TempDir::new().unwrap();
        let mut gaz = make_gazetteer(&dir);

        gaz.update_confidence(2, 0.1); // below min_confidence=0.3
        assert!(gaz.lookup_exact("tim cook").is_empty());
    }

    #[test]
    fn extractor_finds_known_entities_in_text() {
        let dir = TempDir::new().unwrap();
        let gaz = make_gazetteer(&dir);
        let gaz = Arc::new(RwLock::new(gaz));
        let extractor = GazetteerExtractor::new(gaz);

        let lang = DetectedLanguage {
            code: "en".into(),
            confidence: 1.0,
        };
        let entities = extractor.extract("Tim Cook announced Apple results", &lang);

        assert!(entities.len() >= 2);
        let names: Vec<&str> = entities.iter().map(|e| e.text.as_str()).collect();
        assert!(names.contains(&"Tim Cook"));
        assert!(names.contains(&"Apple"));
    }

    #[test]
    fn extractor_longest_match_wins() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let mut gaz = GraphGazetteer::new(&path, 0.3);

        gaz.insert(GazetteerEntry {
            surface: "new york".into(),
            node_id: 10,
            entity_type: "LOC".into(),
            confidence: 0.9,
            is_alias: false,
        });
        gaz.insert(GazetteerEntry {
            surface: "new york city".into(),
            node_id: 11,
            entity_type: "LOC".into(),
            confidence: 0.9,
            is_alias: false,
        });

        let gaz = Arc::new(RwLock::new(gaz));
        let extractor = GazetteerExtractor::new(gaz);
        let lang = DetectedLanguage {
            code: "en".into(),
            confidence: 1.0,
        };

        let entities = extractor.extract("I visited New York City yesterday", &lang);

        // Should get "New York City" (longer), not "New York" (shorter, dominated)
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].text, "New York City");
        assert_eq!(entities[0].resolved_to, Some(11));
    }
}
