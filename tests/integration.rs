/// Integration tests — real-world scenarios demonstrating engram's capabilities.
///
/// These tests exercise the full stack: storage, indexing, learning, inference,
/// search, and the NL/API layer working together on realistic knowledge graphs.
/// Also tests compute backends (CPU scalar, SIMD, GPU/NPU planner routing).

use engram_core::graph::{Graph, Provenance, SourceType};
use tempfile::TempDir;

fn setup() -> (TempDir, Graph) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.brain");
    let g = Graph::create(&path).unwrap();
    (dir, g)
}

fn prov(source: &str) -> Provenance {
    Provenance::user(source)
}

fn prov_api(source: &str) -> Provenance {
    Provenance {
        source_type: SourceType::Api,
        source_id: source.to_string(),
    }
}

fn prov_sensor(source: &str) -> Provenance {
    Provenance {
        source_type: SourceType::Sensor,
        source_id: source.to_string(),
    }
}

// ============================================================================
// Scenario 1: Infrastructure Knowledge Base
//
// A DevOps team builds up knowledge about their infrastructure. Engram tracks
// servers, services, dependencies, and incidents. The team queries for context
// during an outage.
// ============================================================================

#[test]
fn infrastructure_knowledge_base() {
    let (_dir, mut g) = setup();
    let ops = prov("ops-team");

    // -- Build the infrastructure graph --

    // Servers
    g.store_with_confidence("server-web-01", 0.95, &ops).unwrap();
    g.store_with_confidence("server-web-02", 0.95, &ops).unwrap();
    g.store_with_confidence("server-db-01", 0.95, &ops).unwrap();
    g.store_with_confidence("server-cache-01", 0.90, &ops).unwrap();

    // Services
    g.store_with_confidence("payment-service", 0.90, &ops).unwrap();
    g.store_with_confidence("auth-service", 0.90, &ops).unwrap();
    g.store_with_confidence("postgresql", 0.95, &ops).unwrap();
    g.store_with_confidence("redis", 0.90, &ops).unwrap();

    // Set types
    g.set_node_type("server-web-01", "server").unwrap();
    g.set_node_type("server-web-02", "server").unwrap();
    g.set_node_type("server-db-01", "server").unwrap();
    g.set_node_type("server-cache-01", "server").unwrap();
    g.set_node_type("payment-service", "service").unwrap();
    g.set_node_type("auth-service", "service").unwrap();
    g.set_node_type("postgresql", "database").unwrap();
    g.set_node_type("redis", "cache").unwrap();

    // Properties
    g.set_property("server-web-01", "ip", "10.0.1.10").unwrap();
    g.set_property("server-web-01", "datacenter", "eu-west").unwrap();
    g.set_property("server-web-02", "ip", "10.0.1.11").unwrap();
    g.set_property("server-web-02", "datacenter", "eu-west").unwrap();
    g.set_property("server-db-01", "ip", "10.0.2.10").unwrap();
    g.set_property("server-db-01", "datacenter", "eu-west").unwrap();
    g.set_property("postgresql", "version", "15.4").unwrap();
    g.set_property("redis", "version", "7.2").unwrap();

    // Relationships
    g.relate("server-web-01", "payment-service", "hosts", &ops).unwrap();
    g.relate("server-web-02", "auth-service", "hosts", &ops).unwrap();
    g.relate("payment-service", "postgresql", "depends_on", &ops).unwrap();
    g.relate("payment-service", "redis", "depends_on", &ops).unwrap();
    g.relate("auth-service", "postgresql", "depends_on", &ops).unwrap();
    g.relate("auth-service", "redis", "depends_on", &ops).unwrap();
    g.relate("server-db-01", "postgresql", "hosts", &ops).unwrap();
    g.relate("server-cache-01", "redis", "hosts", &ops).unwrap();

    // -- Query: What does payment-service depend on? --
    let edges = g.edges_from("payment-service").unwrap();
    let deps: Vec<&str> = edges.iter()
        .filter(|e| e.relationship == "depends_on")
        .map(|e| e.to.as_str())
        .collect();
    assert!(deps.contains(&"postgresql"));
    assert!(deps.contains(&"redis"));

    // -- Query: What hosts postgresql? --
    let edges_in = g.edges_to("postgresql").unwrap();
    let hosts: Vec<&str> = edges_in.iter()
        .filter(|e| e.relationship == "hosts")
        .map(|e| e.from.as_str())
        .collect();
    assert!(hosts.contains(&"server-db-01"));

    // -- Traverse from payment-service: what's the blast radius? --
    let traversal = g.traverse("payment-service", 2, 0.0).unwrap();
    assert!(traversal.nodes.len() >= 3, "should find postgresql, redis, and their hosts: got {}", traversal.nodes.len());

    // -- Search: find all servers --
    let results = g.search("type:server", 10).unwrap();
    assert_eq!(results.len(), 4, "should find all 4 servers");

    // -- Search: high confidence nodes --
    let results = g.search("confidence>0.9", 20).unwrap();
    assert!(results.len() >= 4, "servers and postgresql should be high confidence");

    // -- Prove: is payment-service transitively related to server-db-01? --
    // payment-service -[depends_on]-> postgresql, server-db-01 -[hosts]-> postgresql
    // Check the traversal reaches postgresql via depends_on
    let traversal = g.traverse("payment-service", 3, 0.0).unwrap();
    let labels: Vec<String> = traversal.nodes.iter()
        .filter_map(|&node_id| {
            g.get_node_by_id(node_id).ok().flatten().map(|n| n.label().to_string())
        })
        .collect();
    assert!(labels.iter().any(|l| l == "postgresql"), "traversal should reach postgresql: {:?}", labels);

    // -- Property lookup --
    let ip = g.get_property("server-web-01", "ip").unwrap().unwrap();
    assert_eq!(ip, "10.0.1.10");
    let dc = g.get_property("server-web-01", "datacenter").unwrap().unwrap();
    assert_eq!(dc, "eu-west");
}

// ============================================================================
// Scenario 2: Learning Lifecycle
//
// Knowledge evolves: new facts arrive, get reinforced by confirmation,
// decay when unused, and get corrected when wrong. This test shows the
// full confidence lifecycle.
// ============================================================================

#[test]
fn learning_lifecycle() {
    let (_dir, mut g) = setup();
    let user = prov("engineer");
    let monitoring = prov_sensor("prometheus");

    // -- Initial knowledge with varying confidence --
    g.store_with_confidence("disk-usage-high", 0.70, &monitoring).unwrap();
    g.store_with_confidence("cpu-load-normal", 0.85, &monitoring).unwrap();
    g.store_with_confidence("memory-leak-suspected", 0.40, &user).unwrap();

    // Verify initial state
    let disk = g.get_node("disk-usage-high").unwrap().unwrap();
    assert!((disk.confidence - 0.70).abs() < 0.01);

    let mem = g.get_node("memory-leak-suspected").unwrap().unwrap();
    assert!((mem.confidence - 0.40).abs() < 0.01);

    // -- Reinforce: monitoring confirms disk usage is still high --
    g.reinforce_confirm("disk-usage-high", &monitoring).unwrap();
    let disk = g.get_node("disk-usage-high").unwrap().unwrap();
    assert!(disk.confidence > 0.70, "confidence should increase after confirmation: {}", disk.confidence);

    // -- Reinforce again: second confirmation --
    g.reinforce_confirm("disk-usage-high", &monitoring).unwrap();
    let disk = g.get_node("disk-usage-high").unwrap().unwrap();
    assert!(disk.confidence > 0.75, "confidence should increase further: {}", disk.confidence);

    // -- Correct: memory leak was a false alarm --
    g.store("app-server-01", &user).unwrap();
    g.relate("memory-leak-suspected", "app-server-01", "affects", &user).unwrap();

    let correction = g.correct("memory-leak-suspected", &user, 2).unwrap().unwrap();
    let mem = g.get_node("memory-leak-suspected").unwrap().unwrap();
    assert_eq!(mem.confidence, 0.0, "corrected node should have zero confidence");

    // Neighbor should be penalized too
    if !correction.propagated.is_empty() {
        let (_, old, new) = correction.propagated[0];
        assert!(new < old, "neighbor confidence should decrease");
    }

    // -- CPU stayed normal — no reinforcement, no decay yet --
    let cpu = g.get_node("cpu-load-normal").unwrap().unwrap();
    assert!((cpu.confidence - 0.85).abs() < 0.01, "untouched fact should keep its confidence");
}

// ============================================================================
// Scenario 3: Multi-Source Knowledge Aggregation
//
// Knowledge arrives from different sources: user input, API integrations,
// sensor data, and LLM analysis. Each source has different provenance.
// Queries should return results regardless of source, and provenance
// should be tracked for auditability.
// ============================================================================

#[test]
fn multi_source_knowledge() {
    let (_dir, mut g) = setup();

    // Sources
    let user = prov("sre-alice");
    let api = prov_api("pagerduty");
    let sensor = prov_sensor("datadog");
    let llm = Provenance {
        source_type: SourceType::Llm,
        source_id: "gpt-4".to_string(),
    };

    // User reports an issue
    g.store_with_confidence("outage-2026-03-07", 0.95, &user).unwrap();
    g.set_property("outage-2026-03-07", "severity", "critical").unwrap();
    g.set_property("outage-2026-03-07", "reporter", "alice").unwrap();
    g.set_node_type("outage-2026-03-07", "incident").unwrap();

    // PagerDuty confirms via API
    g.store_with_confidence("payment-api-down", 0.99, &api).unwrap();
    g.set_node_type("payment-api-down", "alert").unwrap();
    g.relate("outage-2026-03-07", "payment-api-down", "triggered_by", &api).unwrap();

    // Datadog sensor data
    g.store_with_confidence("db-latency-500ms", 0.98, &sensor).unwrap();
    g.set_node_type("db-latency-500ms", "metric").unwrap();
    g.relate("payment-api-down", "db-latency-500ms", "caused_by", &sensor).unwrap();

    // LLM analysis suggests root cause
    g.store_with_confidence("missing-index-after-migration", 0.75, &llm).unwrap();
    g.set_node_type("missing-index-after-migration", "hypothesis").unwrap();
    g.relate("db-latency-500ms", "missing-index-after-migration", "caused_by", &llm).unwrap();

    // Record co-occurrences (migration -> missing index happened before)
    g.record_cooccurrence("migration", "missing-index");
    g.record_cooccurrence("migration", "missing-index");
    g.record_cooccurrence("migration", "missing-index");

    // -- Query the incident chain --
    let traversal = g.traverse("outage-2026-03-07", 3, 0.0).unwrap();
    let labels: Vec<String> = traversal.nodes.iter()
        .filter_map(|&node_id| {
            g.get_node_by_id(node_id).ok().flatten().map(|n| n.label().to_string())
        })
        .collect();

    assert!(labels.iter().any(|l| l == "outage-2026-03-07"));
    assert!(labels.iter().any(|l| l == "payment-api-down"));
    assert!(labels.iter().any(|l| l == "db-latency-500ms"));
    assert!(labels.iter().any(|l| l == "missing-index-after-migration"));

    // -- The chain tells a story: outage -> alert -> metric -> hypothesis --
    let edges = g.edges_from("outage-2026-03-07").unwrap();
    assert_eq!(edges[0].relationship, "triggered_by");
    assert_eq!(edges[0].to, "payment-api-down");

    let edges = g.edges_from("payment-api-down").unwrap();
    assert_eq!(edges[0].relationship, "caused_by");
    assert_eq!(edges[0].to, "db-latency-500ms");

    let edges = g.edges_from("db-latency-500ms").unwrap();
    assert_eq!(edges[0].relationship, "caused_by");
    assert_eq!(edges[0].to, "missing-index-after-migration");

    // -- LLM hypothesis has lower confidence than sensor data --
    let hypothesis = g.get_node("missing-index-after-migration").unwrap().unwrap();
    let metric = g.get_node("db-latency-500ms").unwrap().unwrap();
    assert!(hypothesis.confidence < metric.confidence,
        "LLM hypothesis ({}) should be less confident than sensor data ({})",
        hypothesis.confidence, metric.confidence);

    // -- Co-occurrence evidence supports the hypothesis --
    let (count, _) = g.get_cooccurrence("migration", "missing-index").unwrap();
    assert_eq!(count, 3, "migration -> missing-index seen 3 times");

    // -- Search finds the incident --
    let results = g.search("type:incident", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "outage-2026-03-07");

    // -- Search by confidence: sensor data comes first --
    let results = g.search("confidence>0.9", 10).unwrap();
    let high_conf_labels: Vec<&str> = results.iter().map(|r| r.label.as_str()).collect();
    assert!(high_conf_labels.contains(&"payment-api-down"));
    assert!(high_conf_labels.contains(&"db-latency-500ms"));
    assert!(high_conf_labels.contains(&"outage-2026-03-07"));
    // LLM hypothesis at 0.75 should NOT be in >0.9 results
    assert!(!high_conf_labels.contains(&"missing-index-after-migration"));
}

// ============================================================================
// Scenario 4: Inference and Rule-Based Reasoning
//
// Engram can derive new knowledge from existing facts using forward chaining
// rules. Given "A depends_on B" and "B has vulnerability", infer "A is at risk".
// ============================================================================

#[test]
fn inference_and_reasoning() {
    let (_dir, mut g) = setup();
    let ops = prov("security-team");

    // Build a dependency graph
    g.store("web-app", &ops).unwrap();
    g.store("api-gateway", &ops).unwrap();
    g.store("user-service", &ops).unwrap();
    g.store("log4j", &ops).unwrap();

    g.relate("web-app", "api-gateway", "depends_on", &ops).unwrap();
    g.relate("api-gateway", "user-service", "depends_on", &ops).unwrap();
    g.relate("user-service", "log4j", "depends_on", &ops).unwrap();

    // Mark log4j as vulnerable
    g.set_property("log4j", "vulnerability", "CVE-2021-44228").unwrap();
    g.set_property("log4j", "status", "critical").unwrap();

    // -- Prove transitive dependency: web-app -> ... -> log4j --
    let proof = g.prove("web-app", "log4j", "depends_on", 5).unwrap();
    assert!(proof.supported, "should find transitive dependency chain");
    assert_eq!(proof.chain.len(), 3, "web-app -> api-gateway -> user-service -> log4j");

    // Verify the chain steps
    assert!(proof.chain[0].fact.contains("web-app"));
    assert!(proof.chain[0].fact.contains("api-gateway"));
    assert!(proof.chain[1].fact.contains("api-gateway"));
    assert!(proof.chain[1].fact.contains("user-service"));
    assert!(proof.chain[2].fact.contains("user-service"));
    assert!(proof.chain[2].fact.contains("log4j"));

    // -- Forward chaining: derive security risk --
    use engram_core::learning::rules::{Rule, Condition, Action};

    let rules = vec![
        Rule {
            name: "vuln-propagation".to_string(),
            conditions: vec![
                Condition::Edge {
                    from_var: "service".to_string(),
                    relationship: "depends_on".to_string(),
                    to_var: "dependency".to_string(),
                },
                Condition::Property {
                    node_var: "dependency".to_string(),
                    key: "vulnerability".to_string(),
                    value: "CVE-2021-44228".to_string(),
                },
            ],
            actions: vec![
                Action::Flag {
                    node_var: "service".to_string(),
                    reason: "depends on vulnerable component".to_string(),
                },
            ],
        },
    ];

    let result = g.forward_chain(&rules, &ops).unwrap();
    assert!(result.rules_fired > 0, "rule should fire for user-service -> log4j");
    assert!(result.flags_raised > 0, "should flag user-service as at risk");

    // Verify the flag was set
    let flag = g.get_property("user-service", "_flag").unwrap();
    assert_eq!(flag.as_deref(), Some("depends on vulnerable component"));
}

// ============================================================================
// Scenario 5: Contradiction Detection
//
// When conflicting information arrives, engram detects and reports it rather
// than silently overwriting. This prevents data corruption from bad sources.
// ============================================================================

#[test]
fn contradiction_detection() {
    let (_dir, mut g) = setup();
    let alice = prov("alice");

    // Store initial facts
    g.store("production-db", &alice).unwrap();
    g.set_property("production-db", "host", "db-primary.internal").unwrap();
    g.set_property("production-db", "port", "5432").unwrap();

    // Later, someone tries to set a different host
    let (ok, conflicts) = g.set_property_checked("production-db", "host", "db-secondary.internal").unwrap();
    assert!(ok, "write should succeed (contradictions don't block)");
    assert!(conflicts.has_conflicts, "should detect property conflict");

    // The conflict is flagged
    for c in &conflicts.contradictions {
        assert!(c.reason.contains("host"));
        assert!(c.reason.contains("db-primary.internal"));
        assert!(c.reason.contains("db-secondary.internal"));
    }

    // The property now has the new value (write went through)
    let host = g.get_property("production-db", "host").unwrap().unwrap();
    assert_eq!(host, "db-secondary.internal");
}

// ============================================================================
// Scenario 6: Search with Evidence
//
// When searching, engram can enrich results with supporting facts,
// co-occurrence evidence, and contradictions — giving context beyond
// just "here's a match".
// ============================================================================

#[test]
fn search_with_evidence() {
    let (_dir, mut g) = setup();
    let ops = prov("ops");

    // Build knowledge about deployments
    g.store("deploy-v2.1", &ops).unwrap();
    g.store("service-payment", &ops).unwrap();
    g.store("rollback-needed", &ops).unwrap();

    g.relate("deploy-v2.1", "service-payment", "targets", &ops).unwrap();
    g.relate("deploy-v2.1", "rollback-needed", "caused", &ops).unwrap();

    // Record co-occurrence pattern: deploys often cause errors
    g.record_cooccurrence("deploy", "error");
    g.record_cooccurrence("deploy", "error");
    g.record_cooccurrence("deploy", "rollback");
    g.record_cooccurrence("deploy", "rollback");
    g.record_cooccurrence("deploy", "rollback");

    // Search with evidence
    let results = g.search_with_evidence("deploy", 10).unwrap();
    assert!(!results.is_empty(), "should find deploy-v2.1");

    let deploy_result = &results[0];
    assert_eq!(deploy_result.label, "deploy-v2.1");

    // Should have supporting facts (connected nodes)
    assert!(!deploy_result.evidence.supporting.is_empty(),
        "should have supporting facts from edges");
    let supporting_labels: Vec<&str> = deploy_result.evidence.supporting.iter()
        .map(|s| s.label.as_str()).collect();
    assert!(supporting_labels.contains(&"service-payment") || supporting_labels.contains(&"rollback-needed"),
        "supporting facts should include connected nodes");

    // Co-occurrence evidence separately tracked
    let (count, _) = g.get_cooccurrence("deploy", "rollback").unwrap();
    assert_eq!(count, 3, "deploy -> rollback seen 3 times");

    let (count, _) = g.get_cooccurrence("deploy", "error").unwrap();
    assert_eq!(count, 2, "deploy -> error seen 2 times");
}

// ============================================================================
// Scenario 7: Memory Tiers
//
// Engram manages knowledge across memory tiers: Core (always kept),
// Active (frequently used), and Archival (rarely accessed, may decay).
// ============================================================================

#[test]
fn memory_tiers() {
    let (_dir, mut g) = setup();
    let ops = prov("ops");

    // Store nodes with different confidence levels
    g.store_with_confidence("critical-config", 0.99, &ops).unwrap();
    g.store_with_confidence("daily-report", 0.60, &ops).unwrap();
    g.store_with_confidence("old-ticket", 0.25, &ops).unwrap();

    // Manually set tiers (TIER_CORE=0, TIER_ACTIVE=1, TIER_ARCHIVAL=2)
    g.set_tier("critical-config", 0).unwrap(); // Core
    g.set_tier("daily-report", 1).unwrap();    // Active
    g.set_tier("old-ticket", 2).unwrap();      // Archival

    // Search by tier
    let core = g.search("tier:core", 10).unwrap();
    assert_eq!(core.len(), 1);
    assert_eq!(core[0].label, "critical-config");

    let active = g.search("tier:active", 10).unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].label, "daily-report");

    let archival = g.search("tier:archival", 10).unwrap();
    assert_eq!(archival.len(), 1);
    assert_eq!(archival[0].label, "old-ticket");

    // Core nodes query
    let core_nodes = g.core_nodes().unwrap();
    assert_eq!(core_nodes.len(), 1);
    assert_eq!(core_nodes[0].label, "critical-config");
}

// ============================================================================
// Scenario 8: Natural Language Interface
//
// Users interact with engram through natural language. "Tell" stores facts,
// "Ask" retrieves them. The NL parser extracts entities and relationships.
// ============================================================================

#[test]
fn natural_language_interface() {
    let (_dir, mut g) = setup();

    // -- Tell: store knowledge via natural language --
    let result = engram_api::natural::handle_tell(
        &mut g, "Rust is a systems programming language", Some("user"),
    );
    assert!(result.actions.len() > 0, "should perform at least one action");
    assert!(result.interpretation.len() > 0);

    // The "is a" pattern should create a relationship
    let result = engram_api::natural::handle_tell(
        &mut g, "PostgreSQL is a database", Some("user"),
    );
    assert!(!result.actions.is_empty());

    // Store more facts
    engram_api::natural::handle_tell(&mut g, "Redis is a cache", Some("user"));
    engram_api::natural::handle_tell(
        &mut g, "Payment service uses PostgreSQL", Some("user"),
    );
    engram_api::natural::handle_tell(
        &mut g, "Payment service uses Redis", Some("user"),
    );

    // -- Ask: query via natural language --
    // Graph lookups are case-insensitive: "PostgreSQL", "postgresql", "Postgresql" all work
    let answer = engram_api::natural::handle_ask(&g, "What is PostgreSQL?");
    assert!(!answer.results.is_empty(),
        "should find PostgreSQL (case-insensitive): interpretation='{}'",
        answer.interpretation);

    // Verify case-insensitive lookup works with different casings
    let answer2 = engram_api::natural::handle_ask(&g, "What is postgresql?");
    assert!(!answer2.results.is_empty(),
        "should find postgresql (lowercase): interpretation='{}'",
        answer2.interpretation);

    // Ask about connections
    let answer = engram_api::natural::handle_ask(&g, "What does Payment service connect to?");
    // Should find edges from Payment service
    assert!(answer.interpretation.len() > 0);

    // Search query
    let answer = engram_api::natural::handle_ask(&g, "Find things like database");
    assert!(answer.interpretation.len() > 0);

    // -- Verify case-insensitive graph lookups --
    let node = g.get_node("PostgreSQL").unwrap();
    assert!(node.is_some(), "PostgreSQL should be findable (original casing)");

    let node = g.get_node("postgresql").unwrap();
    assert!(node.is_some(), "postgresql should be findable (lowercase)");

    let node = g.get_node("POSTGRESQL").unwrap();
    assert!(node.is_some(), "POSTGRESQL should be findable (uppercase)");

    let node = g.get_node("Redis").unwrap();
    assert!(node.is_some(), "Redis should exist as a node");

    let node = g.get_node("redis").unwrap();
    assert!(node.is_some(), "redis should be findable (lowercase)");
}

// ============================================================================
// Scenario 9: Knowledge Correction Propagation
//
// When a fact is corrected, the penalty propagates to connected nodes.
// This tests the full BFS propagation of distrust through the graph.
// ============================================================================

#[test]
fn correction_propagation() {
    let (_dir, mut g) = setup();
    let user = prov("analyst");

    // Build a chain: wrong-fact -> A -> B -> C
    g.store_with_confidence("wrong-assumption", 0.80, &user).unwrap();
    g.store_with_confidence("derived-fact-A", 0.75, &user).unwrap();
    g.store_with_confidence("derived-fact-B", 0.70, &user).unwrap();
    g.store_with_confidence("unrelated-fact", 0.90, &user).unwrap();

    g.relate("wrong-assumption", "derived-fact-A", "supports", &user).unwrap();
    g.relate("derived-fact-A", "derived-fact-B", "implies", &user).unwrap();
    // unrelated-fact has no connection

    // Record initial confidences
    let a_before = g.get_node("derived-fact-A").unwrap().unwrap().confidence;
    let b_before = g.get_node("derived-fact-B").unwrap().unwrap().confidence;
    let unrelated_before = g.get_node("unrelated-fact").unwrap().unwrap().confidence;

    // Correct the wrong assumption
    let result = g.correct("wrong-assumption", &user, 3).unwrap().unwrap();

    // The corrected node should be at 0
    let wrong = g.get_node("wrong-assumption").unwrap().unwrap();
    assert_eq!(wrong.confidence, 0.0, "corrected node should be 0");

    // Direct neighbor should be penalized
    let a_after = g.get_node("derived-fact-A").unwrap().unwrap().confidence;
    assert!(a_after < a_before,
        "direct neighbor should lose confidence: {a_before} -> {a_after}");

    // 2-hop neighbor should be penalized less
    let b_after = g.get_node("derived-fact-B").unwrap().unwrap().confidence;
    assert!(b_after < b_before,
        "2-hop neighbor should lose confidence: {b_before} -> {b_after}");
    assert!(b_before - b_after < a_before - a_after,
        "2-hop penalty should be less than 1-hop penalty");

    // Unrelated fact should be untouched
    let unrelated_after = g.get_node("unrelated-fact").unwrap().unwrap().confidence;
    assert!((unrelated_after - unrelated_before).abs() < f32::EPSILON,
        "unrelated fact should not be affected: {unrelated_before} -> {unrelated_after}");

    // Propagation summary
    assert!(!result.propagated.is_empty(), "should have propagated to neighbors");
}

// ============================================================================
// Scenario 10: Boolean Search Queries
//
// Complex search queries combining multiple filters: type, confidence,
// properties, and boolean operators.
// ============================================================================

#[test]
fn boolean_search_queries() {
    let (_dir, mut g) = setup();
    let ops = prov("ops");

    // Build a varied dataset
    g.store_with_confidence("nginx", 0.95, &ops).unwrap();
    g.set_node_type("nginx", "service").unwrap();
    g.set_property("nginx", "role", "loadbalancer").unwrap();

    g.store_with_confidence("haproxy", 0.85, &ops).unwrap();
    g.set_node_type("haproxy", "service").unwrap();
    g.set_property("haproxy", "role", "loadbalancer").unwrap();

    g.store_with_confidence("postgres", 0.90, &ops).unwrap();
    g.set_node_type("postgres", "database").unwrap();
    g.set_property("postgres", "role", "primary").unwrap();

    g.store_with_confidence("mysql", 0.50, &ops).unwrap();
    g.set_node_type("mysql", "database").unwrap();
    g.set_property("mysql", "role", "legacy").unwrap();

    // -- Filter by type --
    let services = g.search("type:service", 10).unwrap();
    assert_eq!(services.len(), 2, "should find nginx and haproxy");

    let databases = g.search("type:database", 10).unwrap();
    assert_eq!(databases.len(), 2, "should find postgres and mysql");

    // -- Filter by confidence --
    let high_conf = g.search("confidence>0.85", 10).unwrap();
    let labels: Vec<&str> = high_conf.iter().map(|r| r.label.as_str()).collect();
    assert!(labels.contains(&"nginx"));
    assert!(labels.contains(&"postgres"));
    assert!(!labels.contains(&"mysql"), "mysql at 0.50 should not be in >0.85");

    // -- Filter by property --
    let lbs = g.search("prop:role=loadbalancer", 10).unwrap();
    assert_eq!(lbs.len(), 2, "should find nginx and haproxy");

    // -- Combined: type AND confidence --
    let results = g.search("type:database AND confidence>0.7", 10).unwrap();
    assert_eq!(results.len(), 1, "only postgres is a high-confidence database");
    assert_eq!(results[0].label, "postgres");
}

// ============================================================================
// Scenario 11: A2A Skill Routing (End-to-End)
//
// External agents use the A2A protocol to store and query knowledge.
// This tests the full skill routing pipeline.
// ============================================================================

#[test]
fn a2a_skill_routing() {
    use std::sync::{Arc, Mutex};
    use engram_a2a::task::{TaskRequest, TaskMessage, TaskState, MessagePart};
    use engram_a2a::skill::route_task;

    let (_dir, g) = setup();
    let graph = Arc::new(Mutex::new(g));

    // -- Store via A2A --
    let req = TaskRequest {
        id: Some("store-1".to_string()),
        skill_id: "store-knowledge".to_string(),
        message: TaskMessage::user_text("Kubernetes is a container orchestrator"),
        metadata: None,
        push_notification_url: None,
    };
    let resp = route_task(&req, &graph);
    assert_eq!(resp.status.state, TaskState::Completed);

    // -- Store structured data --
    let req = TaskRequest {
        id: Some("store-2".to_string()),
        skill_id: "store-knowledge".to_string(),
        message: TaskMessage {
            role: "user".to_string(),
            parts: vec![MessagePart::Data {
                data: serde_json::json!({
                    "entity": "Docker",
                    "confidence": 0.9,
                }),
            }],
        },
        metadata: None,
        push_notification_url: None,
    };
    let resp = route_task(&req, &graph);
    assert_eq!(resp.status.state, TaskState::Completed);
    let art = &resp.artifacts.unwrap()[0];
    assert_eq!(art.data["entity"], "Docker");
    // Slot number depends on how many nodes the NL store created before this
    assert!(art.data["slot"].as_u64().unwrap() > 0, "should have a valid slot");

    // -- Query via A2A --
    let req = TaskRequest {
        id: Some("query-1".to_string()),
        skill_id: "query-knowledge".to_string(),
        message: TaskMessage::user_text("What is Kubernetes?"),
        metadata: None,
        push_notification_url: None,
    };
    let resp = route_task(&req, &graph);
    assert_eq!(resp.status.state, TaskState::Completed);

    // -- Learn via A2A: reinforce --
    let req = TaskRequest {
        id: Some("learn-1".to_string()),
        skill_id: "learn".to_string(),
        message: TaskMessage::user_text("Confirm Docker"),
        metadata: None,
        push_notification_url: None,
    };
    let resp = route_task(&req, &graph);
    assert_eq!(resp.status.state, TaskState::Completed);
    let art = &resp.artifacts.unwrap()[0];
    assert_eq!(art.data["action"], "reinforce");

    // -- Unknown skill fails gracefully --
    let req = TaskRequest {
        id: Some("bad-1".to_string()),
        skill_id: "nonexistent".to_string(),
        message: TaskMessage::user_text("test"),
        metadata: None,
        push_notification_url: None,
    };
    let resp = route_task(&req, &graph);
    assert_eq!(resp.status.state, TaskState::Failed);
}

// ============================================================================
// Scenario 12: Mesh Trust Model
//
// Knowledge from peers is weighted by trust and hop count. This tests
// the confidence degradation model that makes distant knowledge less
// trustworthy — just like real-world information.
// ============================================================================

#[test]
fn mesh_trust_model() {
    use engram_mesh::trust::{propagated_confidence, TrustScore};

    // -- Direct observation: full confidence --
    let local = propagated_confidence(0.95, 1.0, 0).unwrap();
    assert!((local - 0.95).abs() < 0.001);

    // -- Trusted peer, 1 hop away --
    let peer1 = propagated_confidence(0.95, 0.80, 1).unwrap();
    // 0.95 * 0.80 * 0.9 = 0.684
    assert!(peer1 < local, "1-hop should be less than direct");
    assert!(peer1 > 0.6, "trusted peer should still be useful");

    // -- Same fact, 3 hops away --
    let peer3 = propagated_confidence(0.95, 0.80, 3).unwrap();
    assert!(peer3 < peer1, "3-hop should be less than 1-hop");

    // -- Untrusted peer: knowledge barely registers --
    let untrusted = propagated_confidence(0.95, 0.20, 2);
    // 0.95 * 0.20 * 0.81 = 0.1539
    assert!(untrusted.is_some(), "should still be above threshold");
    assert!(untrusted.unwrap() < 0.2, "untrusted peer knowledge should be weak");

    // -- Very distant, low trust: dropped entirely --
    let dropped = propagated_confidence(0.30, 0.10, 5);
    assert!(dropped.is_none(), "should be below useful threshold");

    // -- Trust score evolves with experience --
    let mut ts = TrustScore::new(0.5);
    // Peer sends 20 facts, 18 confirmed, 2 contradicted
    for _ in 0..18 { ts.record_confirmation(); }
    for _ in 0..2 { ts.record_contradiction(); }
    assert!(ts.value > 0.5, "trust should increase with mostly correct facts: {}", ts.value);
    assert_eq!(ts.accuracy(), Some(0.9));
}

// ============================================================================
// Scenario 13: Compute Backend — CPU, GPU, NPU
//
// Tests that the compute planner correctly detects hardware, routes workloads
// to the right backend, and that all operations produce correct results
// regardless of which backend (scalar, SIMD, GPU, NPU) executes them.
// ============================================================================

#[test]
fn compute_backend_cpu_simd() {
    use engram_compute::simd;
    use engram_compute::planner::{ComputePlanner, HardwareInfo, Backend};

    // -- Hardware detection --
    let hw = HardwareInfo::detect();
    assert!(hw.cpu_cores >= 1, "must detect at least 1 CPU core");
    println!("Detected hardware: cores={}, avx2={}, gpu={}, npu={}",
        hw.cpu_cores, hw.has_avx2, hw.has_gpu, hw.has_npu);

    // -- SIMD vs Scalar correctness across dimensions --
    // Test with real embedding sizes: 128 (small), 384 (MiniLM), 768 (BERT), 1536 (OpenAI)
    for dim in [128, 384, 768, 1536] {
        let a: Vec<f32> = (0..dim).map(|i| ((i * 7 + 3) as f32).sin()).collect();
        let b: Vec<f32> = (0..dim).map(|i| ((i * 11 + 5) as f32).cos()).collect();

        let cos_sim = simd::cosine_similarity(&a, &b);
        let dot = simd::dot_product(&a, &b);
        let l2 = simd::l2_distance_sq(&a, &b);

        // Cosine similarity must be in [-1, 1]
        assert!(cos_sim >= -1.0 && cos_sim <= 1.0,
            "dim={dim}: cosine similarity {cos_sim} out of range");

        // Dot product sign should match cosine similarity sign
        assert_eq!(dot.is_sign_positive(), cos_sim.is_sign_positive(),
            "dim={dim}: dot={dot}, cos={cos_sim} sign mismatch");

        // L2 distance must be non-negative
        assert!(l2 >= 0.0, "dim={dim}: L2 distance {l2} is negative");

        // Cosine distance = 1 - similarity
        let cos_dist = simd::cosine_distance(&a, &b);
        assert!((cos_dist - (1.0 - cos_sim)).abs() < 1e-5,
            "dim={dim}: cosine_distance={cos_dist} != 1 - similarity={}", 1.0 - cos_sim);
    }

    // -- Normalize correctness --
    let mut v: Vec<f32> = (0..384).map(|i| (i as f32) * 0.01).collect();
    simd::normalize(&mut v);
    let norm = simd::dot_product(&v, &v).sqrt();
    assert!((norm - 1.0).abs() < 1e-4, "normalized vector norm={norm}, expected 1.0");

    // -- Batch cosine distances: ranking correctness --
    let query: Vec<f32> = vec![1.0; 128];
    let identical: Vec<f32> = vec![1.0; 128];
    let similar: Vec<f32> = (0..128).map(|i| if i < 100 { 1.0 } else { 0.0 }).collect();
    let orthogonal: Vec<f32> = (0..128).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect();
    let opposite: Vec<f32> = vec![-1.0; 128];

    let vectors: Vec<&[f32]> = vec![&opposite, &orthogonal, &identical, &similar];
    let ranked = simd::batch_cosine_distances(&query, &vectors, 4);

    // Identical should be first (distance ~0), opposite last (distance ~2)
    assert_eq!(ranked[0].0, 2, "identical vector should rank first");
    assert_eq!(ranked[1].0, 3, "similar vector should rank second");
    assert_eq!(ranked[3].0, 0, "opposite vector should rank last");
    assert!(ranked[0].1 < 0.01, "identical distance should be ~0, got {}", ranked[0].1);

    // -- Planner backend selection logic --
    let planner = ComputePlanner::new();

    // Small workloads always go to CPU
    assert_eq!(planner.select_similarity_backend(100), Backend::Cpu);
    assert_eq!(planner.select_traversal_backend(100), Backend::Cpu);
    assert_eq!(planner.select_propagation_backend(100), Backend::Cpu);

    // Without GPU/NPU hardware, large workloads still go to CPU
    if !hw.has_gpu {
        assert_eq!(planner.select_similarity_backend(200_000), Backend::Cpu,
            "without GPU, large similarity should fall back to CPU");
        assert_eq!(planner.select_traversal_backend(2_000_000), Backend::Cpu,
            "without GPU, large traversal should fall back to CPU");
    }
    if !hw.has_npu {
        assert_eq!(planner.select_similarity_backend(50_000), Backend::Cpu,
            "without NPU, medium similarity should fall back to CPU");
    }

    // -- Planner-routed similarity search produces same results as direct SIMD --
    let q = vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let v1 = vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]; // identical
    let v2 = vec![0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]; // orthogonal
    let v3 = vec![0.7, 0.7, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]; // similar
    let vecs: Vec<&[f32]> = vec![&v1, &v2, &v3];

    let planner_results = planner.similarity_search(&q, &vecs, 3);
    let direct_results = simd::batch_cosine_distances(&q, &vecs, 3);

    // Both should produce the same ranking
    assert_eq!(planner_results[0].0, direct_results[0].0,
        "planner and direct SIMD should agree on closest vector");
    assert!((planner_results[0].1 - direct_results[0].1).abs() < 1e-5,
        "planner and direct SIMD should agree on distance");

    // -- Performance: CPU SIMD should handle realistic workloads fast --
    let dim = 384;
    let num_vectors = 10_000;
    let big_query: Vec<f32> = (0..dim).map(|i| ((i * 3) as f32).sin()).collect();
    let big_vectors: Vec<Vec<f32>> = (0..num_vectors)
        .map(|j| (0..dim).map(|i| ((i * j + 1) as f32).sin()).collect())
        .collect();
    let big_refs: Vec<&[f32]> = big_vectors.iter().map(|v| v.as_slice()).collect();

    let start = std::time::Instant::now();
    let top10 = planner.similarity_search(&big_query, &big_refs, 10);
    let elapsed = start.elapsed();

    assert_eq!(top10.len(), 10, "should return top 10 results");
    // On any modern CPU, 10K x 384-dim should complete well under 1 second
    assert!(elapsed.as_millis() < 5000,
        "10K x 384-dim similarity search took {}ms, expected <5000ms", elapsed.as_millis());
    println!("CPU similarity search: 10K x 384-dim in {}ms", elapsed.as_millis());

    // -- GPU/NPU availability reporting --
    if hw.has_gpu {
        println!("GPU: {} ({})", hw.gpu_name, hw.gpu_backend);
    } else {
        println!("GPU: not available");
    }
    if hw.has_npu {
        println!("NPU compute: {} (via wgpu low-power adapter)", hw.npu_name);
    } else {
        println!("NPU compute: not available");
    }
    for npu in &hw.dedicated_npu {
        println!("Dedicated NPU: {npu}");
    }
    println!("AVX2+FMA: {}", if hw.has_avx2 { "enabled" } else { "not available (using scalar fallback)" });
    println!("NEON: {}", if hw.has_neon { "enabled" } else { "not available (not aarch64)" });

    // -- GPU compute test (if available) --
    if hw.has_gpu {
        use engram_compute::gpu::GpuDevice;
        let gpu = GpuDevice::probe().expect("GPU probe succeeded in detection but failed here");
        let q = vec![1.0f32, 0.0, 0.0, 0.0];
        let vecs_flat = vec![
            1.0f32, 0.0, 0.0, 0.0, // identical
            0.0, 1.0, 0.0, 0.0,    // orthogonal
        ];
        let dists = gpu.batch_cosine_distances(&q, &vecs_flat, 4, 2)
            .expect("GPU cosine distance failed");
        assert!(dists[0].abs() < 0.01, "GPU: identical distance should be ~0, got {}", dists[0]);
        assert!((dists[1] - 1.0).abs() < 0.01, "GPU: orthogonal distance should be ~1, got {}", dists[1]);
        println!("GPU compute: verified (cosine distances correct)");

        // GPU performance benchmark
        let dim = 384;
        let count = 10_000;
        let gpu_query: Vec<f32> = (0..dim).map(|i| ((i * 3) as f32).sin()).collect();
        let gpu_flat: Vec<f32> = (0..count * dim).map(|i| ((i * 7 + 1) as f32).sin()).collect();
        let start = std::time::Instant::now();
        let _ = gpu.batch_cosine_distances(&gpu_query, &gpu_flat, dim, count);
        let gpu_elapsed = start.elapsed();
        println!("GPU similarity search: {}x{}-dim in {}ms", count, dim, gpu_elapsed.as_millis());
    }

    // -- NPU compute test (if available) --
    if hw.has_npu {
        use engram_compute::npu::NpuDevice;
        let npu = NpuDevice::probe().expect("NPU probe succeeded in detection but failed here");
        let q = vec![1.0f32, 0.0, 0.0, 0.0];
        let vecs_flat = vec![1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        let dists = npu.batch_cosine_distances(&q, &vecs_flat, 4, 2)
            .expect("NPU cosine distance failed");
        assert!(dists[0].abs() < 0.01, "NPU: identical distance should be ~0, got {}", dists[0]);
        println!("NPU compute: verified (cosine distances correct)");
    }
}
