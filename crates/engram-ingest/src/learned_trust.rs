/// Learned trust: creates and maintains Source/Author trust nodes in the graph.
///
/// On first encounter, creates:
/// - `Source:{source_name}` node with initial confidence = global_baseline
/// - `Author:{author_id}` node with initial confidence = global_baseline
/// - `from_source` edge from entity to source
/// - `authored_by` edge from entity to author
///
/// Trust adjustment via corroboration:
/// - When multiple independent sources confirm a fact, trust increases
/// - When a source provides contradicted information, trust decreases
/// - Per-source author scoping: author trust is scoped to their source

use std::sync::{Arc, RwLock};

use engram_core::graph::Graph;

use crate::types::ProcessedFact;

/// Learned trust configuration.
#[derive(Debug, Clone)]
pub struct LearnedTrustConfig {
    /// Initial trust for new sources (fallback when type not in per-type map).
    pub initial_source_trust: f32,
    /// Initial trust for new authors.
    pub initial_author_trust: f32,
    /// Trust boost per corroboration event.
    pub corroboration_boost: f32,
    /// Trust penalty per contradiction event.
    pub contradiction_penalty: f32,
    /// Maximum trust score.
    pub max_trust: f32,
    /// Minimum trust score (never goes below this).
    pub min_trust: f32,
    /// Per-type initial trust: "web" -> 0.30, "x" -> 0.10, etc.
    pub initial_trust_by_type: std::collections::HashMap<String, f32>,
}

impl Default for LearnedTrustConfig {
    fn default() -> Self {
        let mut by_type = std::collections::HashMap::new();
        by_type.insert("web".into(), 0.30);
        by_type.insert("x".into(), 0.10);
        by_type.insert("tg".into(), 0.10);
        by_type.insert("reddit".into(), 0.10);
        by_type.insert("doc".into(), 0.40);
        by_type.insert("person".into(), 0.50);
        by_type.insert("mesh".into(), 0.25);
        by_type.insert("llm".into(), 0.08);
        Self {
            initial_source_trust: 0.30,
            initial_author_trust: 0.30,
            corroboration_boost: 0.05,
            contradiction_penalty: 0.10,
            max_trust: 0.95,
            min_trust: 0.05,
            initial_trust_by_type: by_type,
        }
    }
}

impl LearnedTrustConfig {
    /// Resolve initial trust for a source type prefix (e.g. "web", "x", "tg").
    /// Falls back to `initial_source_trust` if the type is not in the map.
    pub fn trust_for_type(&self, source_type: &str) -> f32 {
        self.initial_trust_by_type
            .get(source_type)
            .copied()
            .unwrap_or(self.initial_source_trust)
    }
}

/// Extract source type and identifier from a URL.
/// Returns (source_type, identifier) for known platforms.
pub fn extract_source_from_url(url: &str) -> (String, String) {
    // Strip scheme
    let after_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);

    // Split host and path
    let (host_port, path) = match after_scheme.find('/') {
        Some(i) => (&after_scheme[..i], &after_scheme[i..]),
        None => (after_scheme, ""),
    };
    let host = host_port.split(':').next().unwrap_or(host_port);
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    // X (Twitter)
    if host == "x.com" || host == "twitter.com" {
        if let Some(handle) = segments.first() {
            if *handle != "status" && *handle != "search" && *handle != "i" {
                return ("x".into(), format!("@{handle}"));
            }
        }
    }

    // Telegram
    if host == "t.me" || host == "telegram.me" {
        if let Some(channel) = segments.first() {
            return ("tg".into(), format!("@{channel}"));
        }
    }

    // Reddit
    if host == "reddit.com" || host == "www.reddit.com" || host == "old.reddit.com" {
        if segments.len() >= 2 && (segments[0] == "u" || segments[0] == "user") {
            return ("reddit".into(), format!("u/{}", segments[1]));
        }
        if segments.len() >= 2 && segments[0] == "r" {
            return ("reddit".into(), format!("r/{}", segments[1]));
        }
    }

    // Default: web domain
    let domain = host.strip_prefix("www.").unwrap_or(host);
    if !domain.is_empty() {
        return ("web".into(), domain.to_string());
    }

    ("web".into(), url.to_string())
}

/// Manages source and author trust nodes in the graph.
pub struct TrustManager {
    config: LearnedTrustConfig,
    graph: Arc<RwLock<Graph>>,
}

impl TrustManager {
    pub fn new(graph: Arc<RwLock<Graph>>, config: LearnedTrustConfig) -> Self {
        Self { config, graph }
    }

    /// Ensure Source and Author nodes exist for a fact.
    /// Creates typed source nodes (e.g. `Source:web:reuters.com`) with per-type initial trust.
    /// Creates `from_source` and `authored_by` edges.
    pub fn ensure_trust_nodes(&self, fact: &ProcessedFact) -> Result<(), String> {
        let mut graph = self.graph.write().map_err(|_| "graph lock poisoned".to_string())?;
        let prov = engram_core::graph::Provenance {
            source_type: engram_core::graph::SourceType::Derived,
            source_id: "trust-manager".to_string(),
        };

        // Derive typed source label from URL if available, else use source name
        let (source_type, source_id) = if let Some(ref url) = fact.provenance.source_url {
            extract_source_from_url(url)
        } else {
            ("web".into(), fact.provenance.source.clone())
        };

        let source_label = format!("Source:{}:{}", source_type, source_id);
        let initial_trust = self.config.trust_for_type(&source_type);

        if graph.find_node_id(&source_label).ok().flatten().is_none() {
            let _ = graph.store_with_confidence(
                &source_label,
                initial_trust,
                &prov,
            );
            let _ = graph.set_node_type(&source_label, "Source");
            let _ = graph.set_property(&source_label, "source_type", &source_type);
            if let Some(ref url) = fact.provenance.source_url {
                let _ = graph.set_property(&source_label, "url", url);
            }
            tracing::debug!(source = %source_label, trust = initial_trust, "created typed source trust node");
        }

        // Create from_source edge
        let _ = graph.relate(&fact.entity, &source_label, "from_source", &prov);

        // Ensure Author node (if author is known)
        if let Some(ref author) = fact.provenance.author {
            let author_label = format!("Author:{}", author);
            if graph.find_node_id(&author_label).ok().flatten().is_none() {
                let _ = graph.store_with_confidence(
                    &author_label,
                    self.config.initial_author_trust,
                    &prov,
                );
                let _ = graph.set_node_type(&author_label, "Author");

                // Scope author to source
                let _ = graph.relate(&author_label, &source_label, "writes_for", &prov);
                tracing::debug!(author = %author_label, source = %source_label, "created author trust node");
            }

            // Create authored_by edge
            let _ = graph.relate(&fact.entity, &author_label, "authored_by", &prov);
        }

        Ok(())
    }

    /// Boost trust for a source when its fact is corroborated by another source.
    pub fn corroborate(&self, source_name: &str) -> Result<f32, String> {
        let mut graph = self.graph.write().map_err(|_| "graph lock poisoned".to_string())?;
        let source_label = format!("Source:{}", source_name);

        let current = graph
            .get_node(&source_label)
            .ok()
            .flatten()
            .map(|n| n.confidence)
            .unwrap_or(self.config.initial_source_trust);

        let new_trust = (current + self.config.corroboration_boost).min(self.config.max_trust);

        let prov = engram_core::graph::Provenance {
            source_type: engram_core::graph::SourceType::Derived,
            source_id: "trust-adjustment".to_string(),
        };
        let _ = graph.store_with_confidence(&source_label, new_trust, &prov);
        Ok(new_trust)
    }

    /// Penalize trust for a source when its fact is contradicted.
    pub fn contradict(&self, source_name: &str) -> Result<f32, String> {
        let mut graph = self.graph.write().map_err(|_| "graph lock poisoned".to_string())?;
        let source_label = format!("Source:{}", source_name);

        let current = graph
            .get_node(&source_label)
            .ok()
            .flatten()
            .map(|n| n.confidence)
            .unwrap_or(self.config.initial_source_trust);

        let new_trust = (current - self.config.contradiction_penalty).max(self.config.min_trust);

        let prov = engram_core::graph::Provenance {
            source_type: engram_core::graph::SourceType::Derived,
            source_id: "trust-adjustment".to_string(),
        };
        let _ = graph.store_with_confidence(&source_label, new_trust, &prov);
        Ok(new_trust)
    }

    /// Corroborate an author's trust (scoped to their source).
    pub fn corroborate_author(&self, author: &str) -> Result<f32, String> {
        let mut graph = self.graph.write().map_err(|_| "graph lock poisoned".to_string())?;
        let author_label = format!("Author:{}", author);

        let current = graph
            .get_node(&author_label)
            .ok()
            .flatten()
            .map(|n| n.confidence)
            .unwrap_or(self.config.initial_author_trust);

        let new_trust = (current + self.config.corroboration_boost).min(self.config.max_trust);
        let prov = engram_core::graph::Provenance {
            source_type: engram_core::graph::SourceType::Derived,
            source_id: "trust-adjustment".to_string(),
        };
        let _ = graph.store_with_confidence(&author_label, new_trust, &prov);
        Ok(new_trust)
    }

    /// Penalize an author's trust.
    pub fn contradict_author(&self, author: &str) -> Result<f32, String> {
        let mut graph = self.graph.write().map_err(|_| "graph lock poisoned".to_string())?;
        let author_label = format!("Author:{}", author);

        let current = graph
            .get_node(&author_label)
            .ok()
            .flatten()
            .map(|n| n.confidence)
            .unwrap_or(self.config.initial_author_trust);

        let new_trust = (current - self.config.contradiction_penalty).max(self.config.min_trust);
        let prov = engram_core::graph::Provenance {
            source_type: engram_core::graph::SourceType::Derived,
            source_id: "trust-adjustment".to_string(),
        };
        let _ = graph.store_with_confidence(&author_label, new_trust, &prov);
        Ok(new_trust)
    }

    /// Confirm a fact: boost fact confidence and the source's trust.
    /// Returns (new_fact_confidence, new_source_trust).
    pub fn confirm_fact(&self, fact_label: &str) -> Result<(f32, f32), String> {
        let mut graph = self.graph.write().map_err(|_| "graph lock poisoned".to_string())?;
        let prov = engram_core::graph::Provenance {
            source_type: engram_core::graph::SourceType::Derived,
            source_id: "fact-confirm".to_string(),
        };

        // Boost fact confidence
        let fact_conf = graph.node_confidence(fact_label)
            .ok().flatten().unwrap_or(0.5);
        let new_fact_conf = (fact_conf + self.config.corroboration_boost).min(self.config.max_trust);
        let _ = graph.set_node_confidence(fact_label, new_fact_conf);
        let _ = graph.set_property(fact_label, "status", "confirmed");

        // Follow sourced_from edge to boost source trust
        let mut new_source_trust = 0.0;
        let edges = graph.edges_from(fact_label).unwrap_or_default();
        for edge in &edges {
            if edge.relationship == "sourced_from" {
                let src_conf = graph.node_confidence(&edge.to)
                    .ok().flatten().unwrap_or(self.config.initial_source_trust);
                new_source_trust = (src_conf + self.config.corroboration_boost).min(self.config.max_trust);
                let _ = graph.store_with_confidence(&edge.to, new_source_trust, &prov);
            }
        }

        Ok((new_fact_conf, new_source_trust))
    }

    /// Debunk a fact: lower fact confidence and penalize the source's trust.
    /// Returns (new_fact_confidence, new_source_trust).
    pub fn debunk_fact(&self, fact_label: &str) -> Result<(f32, f32), String> {
        let mut graph = self.graph.write().map_err(|_| "graph lock poisoned".to_string())?;
        let prov = engram_core::graph::Provenance {
            source_type: engram_core::graph::SourceType::Derived,
            source_id: "fact-debunk".to_string(),
        };

        // Lower fact confidence
        let fact_conf = graph.node_confidence(fact_label)
            .ok().flatten().unwrap_or(0.5);
        let new_fact_conf = (fact_conf - self.config.contradiction_penalty).max(self.config.min_trust);
        let _ = graph.set_node_confidence(fact_label, new_fact_conf);
        let _ = graph.set_property(fact_label, "status", "debunked");

        // Follow sourced_from edge to penalize source trust
        let mut new_source_trust = 0.0;
        let edges = graph.edges_from(fact_label).unwrap_or_default();
        for edge in &edges {
            if edge.relationship == "sourced_from" {
                let src_conf = graph.node_confidence(&edge.to)
                    .ok().flatten().unwrap_or(self.config.initial_source_trust);
                new_source_trust = (src_conf - self.config.contradiction_penalty).max(self.config.min_trust);
                let _ = graph.store_with_confidence(&edge.to, new_source_trust, &prov);
            }
        }

        Ok((new_fact_conf, new_source_trust))
    }

    /// Process a batch of facts: ensure trust nodes, detect corroboration.
    ///
    /// Corroboration detection: if the same entity is provided by multiple
    /// sources, boost trust for each confirming source.
    pub fn process_batch(&self, facts: &[ProcessedFact]) -> Result<TrustReport, String> {
        let mut report = TrustReport::default();

        // Create trust nodes
        for fact in facts {
            self.ensure_trust_nodes(fact)?;
            report.sources_seen.insert(fact.provenance.source.clone());
        }

        // Detect corroboration: same entity from multiple sources
        let mut entity_sources: std::collections::HashMap<String, std::collections::HashSet<String>> =
            std::collections::HashMap::new();

        for fact in facts {
            entity_sources
                .entry(fact.entity.clone())
                .or_default()
                .insert(fact.provenance.source.clone());
        }

        for (entity, sources) in &entity_sources {
            if sources.len() > 1 {
                report.corroborations += 1;
                tracing::debug!(
                    entity = %entity,
                    sources = sources.len(),
                    "entity corroborated by multiple sources"
                );
                for source in sources {
                    let _ = self.corroborate(source);
                }
            }
        }

        report.trust_nodes_created = report.sources_seen.len() as u32;
        Ok(report)
    }
}

/// Report from trust processing.
#[derive(Debug, Default)]
pub struct TrustReport {
    /// Number of unique sources seen.
    pub sources_seen: std::collections::HashSet<String>,
    /// Trust nodes created or updated.
    pub trust_nodes_created: u32,
    /// Corroboration events detected.
    pub corroborations: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ExtractionMethod, Provenance};

    fn test_graph() -> (tempfile::TempDir, Arc<RwLock<Graph>>) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.brain");
        let graph = Graph::create(&path).unwrap();
        (dir, Arc::new(RwLock::new(graph)))
    }

    fn make_fact(entity: &str, source: &str, author: Option<&str>) -> ProcessedFact {
        ProcessedFact {
            entity: entity.into(),
            entity_type: Some("ORG".into()),
            properties: Default::default(),
            confidence: 0.7,
            provenance: Provenance {
                source: source.into(),
                source_url: None,
                author: author.map(String::from),
                extraction_method: ExtractionMethod::StatisticalModel,
                fetched_at: 0,
                ingested_at: 0,
            },
            extraction_method: ExtractionMethod::StatisticalModel,
            language: "en".into(),
            relations: vec![],
            conflicts: vec![],
            resolution: None,
            source_text: None,
            entity_span: None,
        }
    }

    #[test]
    fn creates_source_trust_node() {
        let (_dir, graph) = test_graph();
        let manager = TrustManager::new(graph.clone(), LearnedTrustConfig::default());

        // First, store the entity so the edge can be created
        {
            let mut g = graph.write().unwrap();
            let prov = engram_core::graph::Provenance::user("test");
            g.store("Apple", &prov).unwrap();
        }

        let fact = make_fact("Apple", "reuters.com", None);
        manager.ensure_trust_nodes(&fact).unwrap();

        let g = graph.read().unwrap();
        // Source label is now typed: Source:web:reuters.com
        let node = g.get_node("Source:web:reuters.com").unwrap();
        assert!(node.is_some());
        assert!((node.unwrap().confidence - 0.30).abs() < 0.001);
    }

    #[test]
    fn creates_author_trust_node() {
        let (_dir, graph) = test_graph();
        let manager = TrustManager::new(graph.clone(), LearnedTrustConfig::default());

        {
            let mut g = graph.write().unwrap();
            let prov = engram_core::graph::Provenance::user("test");
            g.store("Apple", &prov).unwrap();
        }

        let fact = make_fact("Apple", "x.com", Some("@analyst"));
        manager.ensure_trust_nodes(&fact).unwrap();

        let g = graph.read().unwrap();
        // Source label is now typed: Source:web:x.com
        assert!(g.get_node("Source:web:x.com").unwrap().is_some());
        assert!(g.get_node("Author:@analyst").unwrap().is_some());
    }

    #[test]
    fn corroboration_boosts_trust() {
        let (_dir, graph) = test_graph();
        let config = LearnedTrustConfig {
            corroboration_boost: 0.10,
            ..Default::default()
        };
        let manager = TrustManager::new(graph.clone(), config);

        // Create source node first
        {
            let mut g = graph.write().unwrap();
            let prov = engram_core::graph::Provenance::user("test");
            g.store_with_confidence("Source:reuters", 0.30, &prov).unwrap();
        }

        let new_trust = manager.corroborate("reuters").unwrap();
        assert!((new_trust - 0.40).abs() < 0.001);
    }

    #[test]
    fn contradiction_penalizes_trust() {
        let (_dir, graph) = test_graph();
        let config = LearnedTrustConfig {
            contradiction_penalty: 0.15,
            min_trust: 0.05,
            ..Default::default()
        };
        let manager = TrustManager::new(graph.clone(), config);

        {
            let mut g = graph.write().unwrap();
            let prov = engram_core::graph::Provenance::user("test");
            g.store_with_confidence("Source:tabloid", 0.20, &prov).unwrap();
        }

        let new_trust = manager.contradict("tabloid").unwrap();
        assert!((new_trust - 0.05).abs() < 0.001); // clamped to min
    }

    #[test]
    fn trust_clamped_to_max() {
        let (_dir, graph) = test_graph();
        let config = LearnedTrustConfig {
            max_trust: 0.95,
            corroboration_boost: 0.50,
            ..Default::default()
        };
        let manager = TrustManager::new(graph.clone(), config);

        {
            let mut g = graph.write().unwrap();
            let prov = engram_core::graph::Provenance::user("test");
            g.store_with_confidence("Source:trusted", 0.90, &prov).unwrap();
        }

        let new_trust = manager.corroborate("trusted").unwrap();
        assert!((new_trust - 0.95).abs() < 0.001);
    }

    #[test]
    fn batch_detects_corroboration() {
        let (_dir, graph) = test_graph();
        let manager = TrustManager::new(graph.clone(), LearnedTrustConfig::default());

        // Store entities first
        {
            let mut g = graph.write().unwrap();
            let prov = engram_core::graph::Provenance::user("test");
            g.store("Apple", &prov).unwrap();
        }

        let facts = vec![
            make_fact("Apple", "reuters", None),
            make_fact("Apple", "bbc", None), // same entity, different source = corroboration
        ];

        let report = manager.process_batch(&facts).unwrap();
        assert_eq!(report.corroborations, 1);
        assert_eq!(report.sources_seen.len(), 2);
    }

    #[test]
    fn test_extract_source_from_url_reuters() {
        let (stype, ident) = extract_source_from_url("https://www.reuters.com/article/xyz");
        assert_eq!(stype, "web");
        assert_eq!(ident, "reuters.com");
    }

    #[test]
    fn test_extract_source_from_url_x() {
        let (stype, ident) = extract_source_from_url("https://x.com/Reuters/status/123");
        assert_eq!(stype, "x");
        assert_eq!(ident, "@Reuters");
    }

    #[test]
    fn test_extract_source_from_url_telegram() {
        let (stype, ident) = extract_source_from_url("https://t.me/intel_channel/42");
        assert_eq!(stype, "tg");
        assert_eq!(ident, "@intel_channel");
    }

    #[test]
    fn test_extract_source_from_url_reddit() {
        let (stype, ident) = extract_source_from_url("https://reddit.com/u/analyst/comments/abc");
        assert_eq!(stype, "reddit");
        assert_eq!(ident, "u/analyst");
    }

    #[test]
    fn test_trust_for_type() {
        let config = LearnedTrustConfig::default();
        assert!((config.trust_for_type("web") - 0.30).abs() < 0.001);
        assert!((config.trust_for_type("x") - 0.10).abs() < 0.001);
        // Unknown type falls back to initial_source_trust (0.30)
        assert!((config.trust_for_type("unknown") - 0.30).abs() < 0.001);
    }

    #[test]
    fn test_typed_source_node_creation() {
        let (_dir, graph) = test_graph();
        let manager = TrustManager::new(graph.clone(), LearnedTrustConfig::default());

        // Store the entity first
        {
            let mut g = graph.write().unwrap();
            let prov = engram_core::graph::Provenance::user("test");
            g.store("SomeEntity", &prov).unwrap();
        }

        let mut fact = make_fact("SomeEntity", "reuters.com", None);
        fact.provenance.source_url = Some("https://reuters.com/article".into());

        manager.ensure_trust_nodes(&fact).unwrap();

        let g = graph.read().unwrap();
        let node = g.get_node("Source:web:reuters.com").unwrap();
        assert!(node.is_some(), "Source:web:reuters.com node should exist");
    }
}
