#![cfg(feature = "ingest")]
/// Integration tests for the document provenance chain.
///
/// Verifies: Entity -> Fact -> Document -> Publisher (Source) nodes and edges,
/// document content caching in DocStore, dedup, and graph traversal helpers.

use engram_core::graph::Graph;
use engram_core::storage::doc_store::DocStore;
use engram_ingest::*;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tempfile::TempDir;

fn setup() -> (TempDir, Arc<RwLock<Graph>>, Arc<RwLock<DocStore>>) {
    let dir = TempDir::new().unwrap();
    let brain_path = dir.path().join("test.brain");
    let g = Graph::create(&brain_path).unwrap();
    let ds = DocStore::open(&brain_path).unwrap();
    (dir, Arc::new(RwLock::new(g)), Arc::new(RwLock::new(ds)))
}

fn make_raw_with_url(text: &str, source: &str, url: &str) -> RawItem {
    RawItem {
        content: Content::Text(text.to_string()),
        source_url: Some(url.to_string()),
        source_name: source.to_string(),
        fetched_at: 1710000000,
        metadata: HashMap::new(),
    }
}

fn make_raw(text: &str, source: &str) -> RawItem {
    RawItem {
        content: Content::Text(text.to_string()),
        source_url: None,
        source_name: source.to_string(),
        fetched_at: 1710000000,
        metadata: HashMap::new(),
    }
}

fn no_ner_config() -> PipelineConfig {
    PipelineConfig {
        stages: StageConfig {
            ner: false,
            entity_resolve: false,
            relation_extract: false,
            language_detect: false,
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn test_ingest_creates_document_node() {
    let (_dir, graph, doc_store) = setup();
    let mut pipeline = Pipeline::new(graph.clone(), no_ner_config());
    pipeline.set_doc_store(doc_store.clone());

    let items = vec![make_raw_with_url(
        "Putin addressed the Russian parliament on security concerns.",
        "reuters",
        "https://reuters.com/world/putin-speech-2024",
    )];

    let result = pipeline.execute(items).unwrap();
    assert!(result.facts_stored > 0, "should store at least one fact");

    let g = graph.read().unwrap();

    // Check Document node exists (filter by Doc: prefix to avoid false positives)
    let all_nodes = g.all_nodes().unwrap();
    let doc_nodes: Vec<_> = all_nodes.iter()
        .filter(|n| n.label.starts_with("Doc:"))
        .collect();
    assert!(!doc_nodes.is_empty(), "should have at least one Doc: node");

    // Check Document has expected properties
    let doc_label = &doc_nodes[0].label;
    let props = g.get_properties(doc_label).unwrap().unwrap();
    assert!(props.contains_key("content_hash"), "Document should have content_hash");
    assert!(props.contains_key("url"), "Document should have url");
    assert_eq!(props.get("url").unwrap(), "https://reuters.com/world/putin-speech-2024");

    // Check Publisher node exists and is linked
    let doc_edges = g.edges_from(doc_label).unwrap();
    let published_by: Vec<_> = doc_edges.iter()
        .filter(|e| e.relationship == "published_by")
        .collect();
    assert!(!published_by.is_empty(), "Document should have published_by edge");
    let publisher = &published_by[0].to;
    assert!(publisher.starts_with("Source:"), "Publisher should be a Source node");

    // Check content is cached in DocStore
    let ds = doc_store.read().unwrap();
    assert!(ds.entry_count() > 0, "DocStore should have cached content");
}

#[test]
fn test_ingest_dedup_same_content() {
    let (_dir, graph, doc_store) = setup();
    let mut pipeline = Pipeline::new(graph.clone(), no_ner_config());
    pipeline.set_doc_store(doc_store.clone());

    let text = "NATO expanded eastward with new member states joining the alliance.";

    // Ingest same text twice
    let items1 = vec![make_raw(text, "source-a")];
    let items2 = vec![make_raw(text, "source-b")];
    pipeline.execute(items1).unwrap();
    pipeline.execute(items2).unwrap();

    let g = graph.read().unwrap();
    let all_nodes = g.all_nodes().unwrap();
    let doc_nodes: Vec<_> = all_nodes.iter()
        .filter(|n| n.label.starts_with("Doc:"))
        .collect();

    // Same content = same hash = one Document node
    assert_eq!(doc_nodes.len(), 1, "identical content should produce one Document node");

    // DocStore should also have just one entry
    let ds = doc_store.read().unwrap();
    assert_eq!(ds.entry_count(), 1);
}

#[test]
fn test_ingest_different_docs_same_source() {
    let (_dir, graph, doc_store) = setup();
    let mut pipeline = Pipeline::new(graph.clone(), no_ner_config());
    pipeline.set_doc_store(doc_store.clone());

    let items = vec![
        make_raw_with_url(
            "First article about sanctions on Russia.",
            "reuters",
            "https://reuters.com/article-1",
        ),
        make_raw_with_url(
            "Second article about diplomatic negotiations.",
            "reuters",
            "https://reuters.com/article-2",
        ),
    ];

    pipeline.execute(items).unwrap();

    let g = graph.read().unwrap();
    let all_nodes = g.all_nodes().unwrap();
    let doc_nodes: Vec<_> = all_nodes.iter()
        .filter(|n| n.label.starts_with("Doc:"))
        .collect();

    // Different content = different hashes = two Document nodes
    assert_eq!(doc_nodes.len(), 2, "different articles should produce two Document nodes");

    // Both should link to the same publisher (reuters)
    let publishers: Vec<String> = doc_nodes.iter().map(|dn| {
        g.edges_from(&dn.label).unwrap()
            .into_iter()
            .find(|e| e.relationship == "published_by")
            .map(|e| e.to)
            .unwrap_or_default()
    }).collect();
    assert_eq!(publishers[0], publishers[1], "both docs should share the same publisher");
}

#[test]
fn test_documents_for_entity_graph_helper() {
    let (_dir, graph, doc_store) = setup();

    // Manually build the provenance chain to test documents_for_entity()
    // This doesn't rely on NER — tests the graph traversal helper directly
    {
        let mut g = graph.write().unwrap();
        let prov = engram_core::graph::Provenance::user("test");

        // Create chain: Entity -> Fact -> Document -> Publisher
        g.store("Putin", &prov).unwrap();
        g.store("Fact:putin-speech-abc1", &prov).unwrap();
        g.set_node_type("Fact:putin-speech-abc1", "Fact").unwrap();
        g.set_property("Fact:putin-speech-abc1", "claim", "Putin addressed parliament").unwrap();
        g.store("Doc:deadbeef", &prov).unwrap();
        g.set_node_type("Doc:deadbeef", "Document").unwrap();
        g.set_property("Doc:deadbeef", "content_hash", "deadbeef01234567").unwrap();
        g.set_property("Doc:deadbeef", "url", "https://reuters.com/speech").unwrap();
        g.store("Source:web:reuters.com", &prov).unwrap();
        g.set_node_type("Source:web:reuters.com", "Source").unwrap();

        g.relate("Putin", "Fact:putin-speech-abc1", "mentioned_in", &prov).unwrap();
        g.relate("Fact:putin-speech-abc1", "Doc:deadbeef", "extracted_from", &prov).unwrap();
        g.relate("Doc:deadbeef", "Source:web:reuters.com", "published_by", &prov).unwrap();
    }

    let g = graph.read().unwrap();
    let docs = g.documents_for_entity("Putin").unwrap();
    assert_eq!(docs.len(), 1, "Putin should have 1 document");
    assert_eq!(docs[0].0, "Doc:deadbeef");
    assert_eq!(docs[0].1.len(), 1, "should have 1 fact");
    assert_eq!(docs[0].1[0].1, "Putin addressed parliament");
}

#[test]
fn test_docstore_content_survives_restart() {
    let dir = TempDir::new().unwrap();
    let brain_path = dir.path().join("test.brain");

    let hash;
    {
        let g = Graph::create(&brain_path).unwrap();
        let ds = DocStore::open(&brain_path).unwrap();
        let graph = Arc::new(RwLock::new(g));
        let doc_store = Arc::new(RwLock::new(ds));

        let mut pipeline = Pipeline::new(graph.clone(), no_ner_config());
        pipeline.set_doc_store(doc_store.clone());

        let items = vec![make_raw("Persistent content for restart test.", "test-source")];
        pipeline.execute(items).unwrap();

        // Get the content hash from the DocStore
        let ds = doc_store.read().unwrap();
        assert_eq!(ds.entry_count(), 1);
        hash = DocStore::hash_content(b"Persistent content for restart test.");
    }

    // Reopen DocStore — should load from persisted index
    let ds2 = DocStore::open(&brain_path).unwrap();
    assert_eq!(ds2.entry_count(), 1);
    let (content, _mime) = ds2.load(&hash).unwrap();
    assert_eq!(String::from_utf8(content).unwrap(), "Persistent content for restart test.");
}
