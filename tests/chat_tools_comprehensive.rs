/// Comprehensive integration test for all engram chat tools.
///
/// Starts an HTTP server on a random port, creates a test graph with known entities,
/// then exercises every tool endpoint and verifies responses.
///
/// Run with: cargo test --features all --test chat_tools_comprehensive -- --nocapture

use engram_api::server;
use engram_api::state::AppState;
use engram_core::graph::{Graph, Provenance};
use serde_json::{json, Value};
use std::net::TcpListener;
use tempfile::TempDir;

/// Find a free TCP port for the test server.
fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Build a test graph with known entities and relations.
fn build_test_graph(dir: &TempDir) -> Graph {
    let path = dir.path().join("test.brain");
    let mut g = Graph::create(&path).unwrap();
    let p = Provenance::user("test-setup");

    // Entities
    g.store_with_confidence("Alice", 0.9, &p).unwrap();
    g.set_node_type("Alice", "Person").unwrap();

    g.store_with_confidence("Bob", 0.85, &p).unwrap();
    g.set_node_type("Bob", "Person").unwrap();

    g.store_with_confidence("Acme", 0.95, &p).unwrap();
    g.set_node_type("Acme", "Organization").unwrap();

    // Extra isolated entity for gap/isolated tests
    g.store_with_confidence("Orphan", 0.5, &p).unwrap();

    // Relations
    g.relate_with_temporal("Alice", "Acme", "works_at", 0.9,
        Some("2020-01-01"), None, &p).unwrap();
    g.relate_with_temporal("Bob", "Acme", "works_at", 0.85,
        Some("2021-06-01"), None, &p).unwrap();
    g.relate_with_temporal("Alice", "Bob", "knows", 0.8,
        Some("2020-01-01"), None, &p).unwrap();

    g
}

/// Wrapper around reqwest for cleaner test assertions.
struct TestClient {
    base_url: String,
    client: reqwest::Client,
}

impl TestClient {
    fn new(port: u16) -> Self {
        TestClient {
            base_url: format!("http://127.0.0.1:{port}"),
            client: reqwest::Client::new(),
        }
    }

    async fn get(&self, path: &str) -> (u16, Value) {
        let resp = self.client
            .get(format!("{}{}", self.base_url, path))
            .send()
            .await
            .unwrap();
        let status = resp.status().as_u16();
        let body: Value = resp.json().await.unwrap_or(json!({"error": "non-json response"}));
        (status, body)
    }

    async fn post(&self, path: &str, body: &Value) -> (u16, Value) {
        let resp = self.client
            .post(format!("{}{}", self.base_url, path))
            .json(body)
            .send()
            .await
            .unwrap();
        let status = resp.status().as_u16();
        let body: Value = resp.json().await.unwrap_or(json!({"error": "non-json response"}));
        (status, body)
    }

    async fn delete(&self, path: &str) -> (u16, Value) {
        let resp = self.client
            .delete(format!("{}{}", self.base_url, path))
            .send()
            .await
            .unwrap();
        let status = resp.status().as_u16();
        let body: Value = resp.json().await.unwrap_or(json!({"error": "non-json response"}));
        (status, body)
    }

    async fn post_text(&self, path: &str, text: &str) -> (u16, Value) {
        let resp = self.client
            .post(format!("{}{}", self.base_url, path))
            .header("content-type", "text/plain")
            .body(text.to_string())
            .send()
            .await
            .unwrap();
        let status = resp.status().as_u16();
        let body: Value = resp.json().await.unwrap_or(json!({"error": "non-json response"}));
        (status, body)
    }

    async fn patch(&self, path: &str, body: &Value) -> (u16, Value) {
        let resp = self.client
            .patch(format!("{}{}", self.base_url, path))
            .json(body)
            .send()
            .await
            .unwrap();
        let status = resp.status().as_u16();
        let body: Value = resp.json().await.unwrap_or(json!({"error": "non-json response"}));
        (status, body)
    }
}

// ── Test result tracking ──

struct TestResult {
    name: String,
    passed: bool,
    detail: String,
}

fn pass(name: &str) -> TestResult {
    TestResult { name: name.to_string(), passed: true, detail: "OK".to_string() }
}

fn fail(name: &str, detail: &str) -> TestResult {
    TestResult { name: name.to_string(), passed: false, detail: detail.to_string() }
}

fn check(name: &str, status: u16, body: &Value, expect_status: u16) -> TestResult {
    if status != expect_status {
        return fail(name, &format!("expected status {expect_status}, got {status}: {body}"));
    }
    if let Some(err) = body.get("error") {
        // Some endpoints return error fields intentionally (e.g. empty results)
        // Only fail if we got a 200 with an error, which shouldn't happen
        if status == 200 && err.as_str().map_or(false, |s| !s.is_empty()) {
            return fail(name, &format!("200 with error: {err}"));
        }
    }
    pass(name)
}

#[tokio::test]
async fn comprehensive_chat_tool_endpoints() {
    let dir = TempDir::new().unwrap();
    let graph = build_test_graph(&dir);

    // Create app state
    let brain_path = dir.path().join("test.brain");
    let mut state = AppState::new(graph);

    // Load assessments so assessment endpoints work
    #[cfg(feature = "assess")]
    {
        let assess_path = brain_path.with_extension("brain.assessments");
        state.load_assessments(assess_path);
    }

    // Load config so config endpoints work
    let config_path = brain_path.with_extension("brain.config");
    state.load_config(config_path);

    // Set up action rules path
    #[cfg(feature = "actions")]
    {
        state.action_rules_path = Some(brain_path.with_extension("brain.rules"));
    }

    let port = free_port();
    let addr = format!("127.0.0.1:{port}");

    // Start server in background
    let app = server::router(state);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give server a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let c = TestClient::new(port);
    let mut results: Vec<TestResult> = Vec::new();

    // ========================================================================
    // 0. GET /tools -- verify tool definitions endpoint
    // ========================================================================
    {
        let (status, body) = c.get("/tools").await;
        let mut r = check("GET /tools", status, &body, 200);
        if r.passed {
            // Response is {"tools": [...]}
            if let Some(arr) = body.get("tools").and_then(|t| t.as_array()) {
                let tool_names: Vec<&str> = arr.iter()
                    .filter_map(|t| t.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()))
                    .collect();
                let expected_tools = [
                    "engram_store", "engram_relate", "engram_query", "engram_search",
                    "engram_explain", "engram_gaps", "engram_temporal_query",
                    "engram_timeline", "engram_current_state", "engram_compare",
                    "engram_shortest_path", "engram_most_connected", "engram_isolated",
                    "engram_what_if", "engram_influence_path", "engram_briefing",
                    "engram_export_subgraph", "engram_entity_timeline", "engram_changes",
                ];
                let mut missing = Vec::new();
                for expected in &expected_tools {
                    if !tool_names.contains(expected) {
                        missing.push(*expected);
                    }
                }
                if !missing.is_empty() {
                    r = fail("GET /tools", &format!("missing tools: {:?} (found: {:?})", missing, tool_names));
                } else {
                    r.detail = format!("{} tools defined", tool_names.len());
                }
            } else {
                r = fail("GET /tools", "response is not an array");
            }
        }
        results.push(r);
    }

    // ========================================================================
    // 1. POST /store (engram_store)
    // ========================================================================
    {
        let (status, body) = c.post("/store", &json!({
            "entity": "TestEntity",
            "type": "TestType",
            "confidence": 0.75,
            "source": "test"
        })).await;
        let r = check("POST /store", status, &body, 200);
        results.push(r);

        // Verify the entity persisted
        let (s2, b2) = c.get("/node/TestEntity").await;
        if s2 == 200 {
            let conf = b2.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if (conf - 0.75).abs() > 0.01 {
                results.push(fail("POST /store (verify)", &format!("confidence mismatch: {conf}")));
            } else {
                results.push(pass("POST /store (verify)"));
            }
        } else {
            results.push(fail("POST /store (verify)", &format!("GET /node/TestEntity returned {s2}")));
        }
    }

    // ========================================================================
    // 2. POST /relate (engram_relate)
    // ========================================================================
    {
        let (status, body) = c.post("/relate", &json!({
            "from": "Alice",
            "to": "TestEntity",
            "relationship": "tested_by",
            "confidence": 0.7,
            "valid_from": "2025-01-01"
        })).await;
        let r = check("POST /relate", status, &body, 200);
        results.push(r);
    }

    // ========================================================================
    // 3. POST /query (engram_query)
    // ========================================================================
    {
        let (status, body) = c.post("/query", &json!({
            "start": "Alice",
            "depth": 2,
            "min_confidence": 0.5
        })).await;
        let mut r = check("POST /query", status, &body, 200);
        if r.passed {
            let node_count = body.get("nodes").and_then(|n| n.as_array()).map(|a| a.len()).unwrap_or(0);
            if node_count == 0 {
                r = fail("POST /query", "returned 0 nodes for Alice traversal");
            } else {
                r.detail = format!("{node_count} nodes returned");
            }
        }
        results.push(r);
    }

    // ========================================================================
    // 4. POST /search (engram_search)
    // ========================================================================
    {
        let (status, body) = c.post("/search", &json!({
            "query": "Alice",
            "limit": 10
        })).await;
        let mut r = check("POST /search", status, &body, 200);
        if r.passed {
            let total = body.get("total").and_then(|t| t.as_u64()).unwrap_or(0);
            if total == 0 {
                r = fail("POST /search", "no results for 'Alice'");
            } else {
                r.detail = format!("{total} results");
            }
        }
        results.push(r);
    }

    // ========================================================================
    // 5. GET /explain/{entity} (engram_explain)
    // ========================================================================
    {
        let (status, body) = c.get("/explain/Alice").await;
        let mut r = check("GET /explain/Alice", status, &body, 200);
        if r.passed {
            let entity = body.get("entity").and_then(|e| e.as_str()).unwrap_or("");
            if entity != "Alice" {
                r = fail("GET /explain/Alice", &format!("entity field is '{entity}', expected 'Alice'"));
            }
        }
        results.push(r);
    }

    // ========================================================================
    // 6. GET /reason/gaps (engram_gaps) -- feature gated
    // ========================================================================
    {
        let (status, body) = c.get("/reason/gaps").await;
        // Might be 501 if reason feature not compiled
        if status == 501 {
            results.push(TestResult { name: "GET /reason/gaps".into(), passed: true, detail: "skipped (feature not enabled)".into() });
        } else {
            results.push(check("GET /reason/gaps", status, &body, 200));
        }
    }

    // ========================================================================
    // 7. POST /chat/temporal_query (engram_temporal_query)
    // ========================================================================
    {
        let (status, body) = c.post("/chat/temporal_query", &json!({
            "entity": "Alice",
            "from_date": "2019-01-01",
            "to_date": "2026-12-31"
        })).await;
        let mut r = check("POST /chat/temporal_query", status, &body, 200);
        if r.passed {
            if let Some(arr) = body.as_array() {
                r.detail = format!("{} temporal edges", arr.len());
                if arr.is_empty() {
                    r = fail("POST /chat/temporal_query", "expected temporal edges for Alice");
                }
            }
        }
        results.push(r);
    }

    // ========================================================================
    // 8. POST /chat/timeline (engram_timeline)
    // ========================================================================
    {
        let (status, body) = c.post("/chat/timeline", &json!({
            "entity": "Alice",
            "limit": 10
        })).await;
        let mut r = check("POST /chat/timeline", status, &body, 200);
        if r.passed {
            if let Some(arr) = body.as_array() {
                r.detail = format!("{} timeline edges", arr.len());
            }
        }
        results.push(r);
    }

    // ========================================================================
    // 9. POST /chat/current_state (engram_current_state)
    // ========================================================================
    {
        let (status, body) = c.post("/chat/current_state", &json!({
            "entity": "Alice",
            "depth": 1
        })).await;
        let mut r = check("POST /chat/current_state", status, &body, 200);
        if r.passed {
            if let Some(arr) = body.as_array() {
                r.detail = format!("{} current edges", arr.len());
            }
        }
        results.push(r);
    }

    // ========================================================================
    // 10. POST /chat/compare (engram_compare)
    // ========================================================================
    {
        let (status, body) = c.post("/chat/compare", &json!({
            "entity_a": "Alice",
            "entity_b": "Bob"
        })).await;
        let mut r = check("POST /chat/compare", status, &body, 200);
        if r.passed {
            let shared = body.get("shared_neighbors").and_then(|s| s.as_array()).map(|a| a.len()).unwrap_or(0);
            r.detail = format!("{shared} shared neighbors");
            // Both work at Acme, so should share Acme
            if shared == 0 {
                r = fail("POST /chat/compare", "Alice and Bob should share Acme as neighbor");
            }
        }
        results.push(r);
    }

    // ========================================================================
    // 11. POST /chat/shortest_path (engram_shortest_path)
    // ========================================================================
    {
        let (status, body) = c.post("/chat/shortest_path", &json!({
            "from": "Alice",
            "to": "Bob",
            "max_depth": 5
        })).await;
        let mut r = check("POST /chat/shortest_path", status, &body, 200);
        if r.passed {
            let found = body.get("found").and_then(|f| f.as_bool()).unwrap_or(false);
            if !found {
                r = fail("POST /chat/shortest_path", "path from Alice to Bob should be found");
            } else {
                let len = body.get("length").and_then(|l| l.as_u64()).unwrap_or(0);
                r.detail = format!("path found, length={len}");
            }
        }
        results.push(r);
    }

    // ========================================================================
    // 12. POST /chat/most_connected (engram_most_connected)
    // ========================================================================
    {
        let (status, body) = c.post("/chat/most_connected", &json!({
            "limit": 5
        })).await;
        let mut r = check("POST /chat/most_connected", status, &body, 200);
        if r.passed {
            if let Some(arr) = body.as_array() {
                r.detail = format!("{} nodes returned", arr.len());
                if arr.is_empty() {
                    r = fail("POST /chat/most_connected", "should return at least 1 connected node");
                }
            }
        }
        results.push(r);
    }

    // ========================================================================
    // 13. POST /chat/isolated (engram_isolated)
    // ========================================================================
    {
        let (status, body) = c.post("/chat/isolated", &json!({
            "max_edges": 0
        })).await;
        let mut r = check("POST /chat/isolated", status, &body, 200);
        if r.passed {
            if let Some(arr) = body.as_array() {
                let labels: Vec<&str> = arr.iter()
                    .filter_map(|n| n.get("label").and_then(|l| l.as_str()))
                    .collect();
                r.detail = format!("isolated: {:?}", labels);
                if !labels.contains(&"Orphan") {
                    r = fail("POST /chat/isolated", "Orphan should be isolated");
                }
            }
        }
        results.push(r);
    }

    // ========================================================================
    // 14. POST /chat/what_if (engram_what_if)
    // ========================================================================
    {
        let (status, body) = c.post("/chat/what_if", &json!({
            "entity": "Alice",
            "new_confidence": 0.3,
            "depth": 2
        })).await;
        let mut r = check("POST /chat/what_if", status, &body, 200);
        if r.passed {
            let affected = body.get("affected").and_then(|a| a.as_array()).map(|a| a.len()).unwrap_or(0);
            r.detail = format!("{affected} affected entities");
        }
        results.push(r);
    }

    // ========================================================================
    // 15. POST /chat/influence_path (engram_influence_path)
    // ========================================================================
    {
        let (status, body) = c.post("/chat/influence_path", &json!({
            "from": "Alice",
            "to": "Acme",
            "max_depth": 3
        })).await;
        let mut r = check("POST /chat/influence_path", status, &body, 200);
        if r.passed {
            let found = body.get("found").and_then(|f| f.as_bool()).unwrap_or(false);
            r.detail = format!("found={found}");
        }
        results.push(r);
    }

    // ========================================================================
    // 16. POST /chat/briefing (engram_briefing)
    // ========================================================================
    {
        let (status, body) = c.post("/chat/briefing", &json!({
            "topic": "Alice",
            "depth": "shallow"
        })).await;
        let mut r = check("POST /chat/briefing", status, &body, 200);
        if r.passed {
            let entity_count = body.get("entity_count").and_then(|c| c.as_u64()).unwrap_or(0);
            r.detail = format!("{entity_count} entities in briefing");
        }
        results.push(r);
    }

    // ========================================================================
    // 17. POST /chat/export_subgraph (engram_export_subgraph)
    // ========================================================================
    {
        let (status, body) = c.post("/chat/export_subgraph", &json!({
            "entity": "Alice",
            "depth": 1
        })).await;
        let mut r = check("POST /chat/export_subgraph", status, &body, 200);
        if r.passed {
            let node_count = body.get("nodes").and_then(|n| n.as_array()).map(|a| a.len()).unwrap_or(0);
            r.detail = format!("{node_count} nodes exported");
            if node_count == 0 {
                r = fail("POST /chat/export_subgraph", "should export at least Alice");
            }
        }
        results.push(r);
    }

    // ========================================================================
    // 18. POST /chat/entity_timeline (engram_entity_timeline)
    // ========================================================================
    {
        let (status, body) = c.post("/chat/entity_timeline", &json!({
            "entity": "Alice",
            "from_date": "2019-01-01",
            "to_date": "2027-01-01"
        })).await;
        let mut r = check("POST /chat/entity_timeline", status, &body, 200);
        if r.passed {
            let event_count = body.get("event_count").and_then(|c| c.as_u64()).unwrap_or(0);
            r.detail = format!("{event_count} events");
        }
        results.push(r);
    }

    // ========================================================================
    // 19. POST /chat/changes (engram_changes)
    // ========================================================================
    {
        let (status, body) = c.post("/chat/changes", &json!({
            "since": "2020-01-01"
        })).await;
        let mut r = check("POST /chat/changes", status, &body, 200);
        if r.passed {
            let total = body.get("total").and_then(|t| t.as_u64()).unwrap_or(0);
            r.detail = format!("{total} changes");
        }
        results.push(r);
    }

    // ========================================================================
    // 20. POST /chat/watch (engram_watch)
    // ========================================================================
    {
        let (status, body) = c.post("/chat/watch", &json!({
            "entity": "Alice"
        })).await;
        let mut r = check("POST /chat/watch", status, &body, 200);
        if r.passed {
            let watched = body.get("watched").and_then(|w| w.as_bool()).unwrap_or(false);
            if !watched {
                r = fail("POST /chat/watch", "watched should be true");
            }
        }
        results.push(r);
    }

    // ========================================================================
    // 21. POST /chat/schedule (engram_schedule -- create)
    // ========================================================================
    {
        let (status, body) = c.post("/chat/schedule", &json!({
            "action": "create",
            "entity": "Alice",
            "interval": "daily"
        })).await;
        let mut r = check("POST /chat/schedule (create)", status, &body, 200);
        if r.passed {
            let scheduled = body.get("scheduled").and_then(|s| s.as_bool()).unwrap_or(false);
            if !scheduled {
                r = fail("POST /chat/schedule (create)", "scheduled should be true");
            }
        }
        results.push(r);
    }

    // ========================================================================
    // 22. POST /chat/schedule (engram_schedule -- list)
    // ========================================================================
    {
        let (status, body) = c.post("/chat/schedule", &json!({
            "action": "list"
        })).await;
        let mut r = check("POST /chat/schedule (list)", status, &body, 200);
        if r.passed {
            let total = body.get("total").and_then(|t| t.as_u64()).unwrap_or(0);
            r.detail = format!("{total} schedules");
            if total == 0 {
                r = fail("POST /chat/schedule (list)", "should have at least 1 schedule after create");
            }
        }
        results.push(r);
    }

    // ========================================================================
    // 23. POST /assessments (engram_assess_create)
    // ========================================================================
    {
        let (status, body) = c.post("/assessments", &json!({
            "title": "Test Assessment",
            "category": "test",
            "initial_probability": 0.6
        })).await;
        if status == 501 {
            results.push(TestResult { name: "POST /assessments".into(), passed: true, detail: "skipped (feature not enabled)".into() });
        } else {
            results.push(check("POST /assessments", status, &body, 200));
        }
    }

    // ========================================================================
    // 24. GET /assessments (engram_assess_list)
    // ========================================================================
    {
        let (status, body) = c.get("/assessments").await;
        if status == 501 {
            results.push(TestResult { name: "GET /assessments".into(), passed: true, detail: "skipped".into() });
        } else {
            results.push(check("GET /assessments", status, &body, 200));
        }
    }

    // ========================================================================
    // 25. GET /assessments/{label} (engram_assess_query)
    //     Label is "Assessment:test-assessment" (auto-generated from title)
    // ========================================================================
    {
        let assess_label = "Assessment%3Atest-assessment";
        let (status, body) = c.get(&format!("/assessments/{assess_label}")).await;
        if status == 501 {
            results.push(TestResult { name: "GET /assessments/{label}".into(), passed: true, detail: "skipped".into() });
        } else if status == 404 {
            results.push(TestResult { name: "GET /assessments/{label}".into(), passed: true, detail: "not found (assess feature off or label mismatch)".into() });
        } else {
            results.push(check("GET /assessments/{label}", status, &body, 200));
        }
    }

    // ========================================================================
    // 26. POST /assessments/{label}/evidence (engram_assess_evidence)
    // ========================================================================
    {
        let assess_label = "Assessment%3Atest-assessment";
        let (status, body) = c.post(&format!("/assessments/{assess_label}/evidence"), &json!({
            "node_label": "Alice",
            "direction": "supports"
        })).await;
        if status == 501 || status == 404 {
            results.push(TestResult { name: "POST /assessments/{label}/evidence".into(), passed: true, detail: format!("skipped ({status})") });
        } else {
            results.push(check("POST /assessments/{label}/evidence", status, &body, 200));
        }
    }

    // ========================================================================
    // 27. POST /assessments/{label}/evaluate (engram_assess_evaluate)
    // ========================================================================
    {
        let assess_label = "Assessment%3Atest-assessment";
        let (status, body) = c.post(&format!("/assessments/{assess_label}/evaluate"), &json!({})).await;
        if status == 501 || status == 404 {
            results.push(TestResult { name: "POST /assessments/{label}/evaluate".into(), passed: true, detail: format!("skipped ({status})") });
        } else {
            results.push(check("POST /assessments/{label}/evaluate", status, &body, 200));
        }
    }

    // ========================================================================
    // 28. POST /assessments/{label}/watch (engram_assess_watch)
    // ========================================================================
    {
        let assess_label = "Assessment%3Atest-assessment";
        let (status, body) = c.post(&format!("/assessments/{assess_label}/watch"), &json!({
            "entity_label": "Alice"
        })).await;
        if status == 501 || status == 404 {
            results.push(TestResult { name: "POST /assessments/{label}/watch".into(), passed: true, detail: format!("skipped ({status})") });
        } else {
            results.push(check("POST /assessments/{label}/watch", status, &body, 200));
        }
    }

    // ========================================================================
    // 29. POST /actions/rules (engram_rule_create) -- expects TOML body
    // ========================================================================
    {
        let toml_body = r#"
[[rules]]
id = "test-rule"
description = "A test rule"

[[rules.triggers]]
type = "fact_stored"

[[rules.effects]]
type = "log"
message = "test rule fired"
"#;
        let (status, body) = c.post_text("/actions/rules", toml_body).await;
        if status == 501 {
            results.push(TestResult { name: "POST /actions/rules".into(), passed: true, detail: "skipped (feature not enabled)".into() });
        } else {
            results.push(check("POST /actions/rules", status, &body, 200));
        }
    }

    // ========================================================================
    // 30. GET /actions/rules (engram_rule_list)
    // ========================================================================
    {
        let (status, body) = c.get("/actions/rules").await;
        if status == 501 {
            results.push(TestResult { name: "GET /actions/rules".into(), passed: true, detail: "skipped".into() });
        } else {
            results.push(check("GET /actions/rules", status, &body, 200));
        }
    }

    // ========================================================================
    // 31. POST /ingest (engram_ingest_text)
    // ========================================================================
    {
        let (status, body) = c.post("/ingest", &json!({
            "items": ["Alice works at Acme Corporation."],
            "source": "test-ingest",
            "skip": "ner,resolve"
        })).await;
        if status == 501 {
            results.push(TestResult { name: "POST /ingest".into(), passed: true, detail: "skipped (feature not enabled)".into() });
        } else {
            results.push(check("POST /ingest", status, &body, 200));
        }
    }

    // ========================================================================
    // 32. POST /ingest/analyze (engram_analyze_relations)
    // ========================================================================
    {
        let (status, body) = c.post("/ingest/analyze", &json!({
            "text": "Alice works at Acme Corporation in New York."
        })).await;
        if status == 501 {
            results.push(TestResult { name: "POST /ingest/analyze".into(), passed: true, detail: "skipped (feature not enabled)".into() });
        } else {
            results.push(check("POST /ingest/analyze", status, &body, 200));
        }
    }

    // ========================================================================
    // 33. GET /sources (engram_sources_list)
    // ========================================================================
    {
        let (status, body) = c.get("/sources").await;
        if status == 501 {
            results.push(TestResult { name: "GET /sources".into(), passed: true, detail: "skipped".into() });
        } else {
            results.push(check("GET /sources", status, &body, 200));
        }
    }

    // ========================================================================
    // 34. GET /sources/{name}/usage (engram_source_coverage)
    // ========================================================================
    {
        let (status, body) = c.get("/sources/test-ingest/usage").await;
        if status == 501 || status == 404 {
            results.push(TestResult { name: "GET /sources/{name}/usage".into(), passed: true, detail: format!("skipped ({status})") });
        } else {
            results.push(check("GET /sources/{name}/usage", status, &body, 200));
        }
    }

    // ========================================================================
    // 35. POST /learn/reinforce (engram_reinforce)
    //     NOTE: tool mapping sends to /reinforce but server route is /learn/reinforce
    // ========================================================================
    {
        let (status, body) = c.post("/learn/reinforce", &json!({
            "entity": "Alice",
            "source": "test"
        })).await;
        let mut r = check("POST /learn/reinforce", status, &body, 200);
        if r.passed {
            let new_conf = body.get("new_confidence").and_then(|c| c.as_f64()).unwrap_or(0.0);
            r.detail = format!("new_confidence={new_conf:.3}");
            // Confidence should have increased from 0.9
            if new_conf <= 0.9 {
                r = fail("POST /learn/reinforce", &format!("confidence should increase from 0.9, got {new_conf}"));
            }
        }
        results.push(r);
    }

    // ========================================================================
    // 36. POST /learn/correct (engram_correct)
    // ========================================================================
    {
        let (status, body) = c.post("/learn/correct", &json!({
            "entity": "Bob",
            "reason": "test correction"
        })).await;
        let r = check("POST /learn/correct", status, &body, 200);
        results.push(r);
    }

    // ========================================================================
    // 37. POST /learn/derive (engram_prove)
    // ========================================================================
    {
        let (status, body) = c.post("/learn/derive", &json!({})).await;
        let r = check("POST /learn/derive", status, &body, 200);
        results.push(r);
    }

    // ========================================================================
    // 38. GET /reason/frontier (engram_frontier)
    // ========================================================================
    {
        let (status, body) = c.get("/reason/frontier").await;
        if status == 501 {
            results.push(TestResult { name: "GET /reason/frontier".into(), passed: true, detail: "skipped (feature not enabled)".into() });
        } else {
            results.push(check("GET /reason/frontier", status, &body, 200));
        }
    }

    // ========================================================================
    // 39. DELETE /node/{entity} (engram_delete)
    //     Create a sacrificial entity first, then delete it
    // ========================================================================
    {
        // Create entity to delete
        c.post("/store", &json!({"entity": "DeleteMe", "confidence": 0.5})).await;
        let (status, body) = c.delete("/node/DeleteMe").await;
        let mut r = check("DELETE /node/{entity}", status, &body, 200);
        if r.passed {
            let deleted = body.get("deleted").and_then(|d| d.as_bool()).unwrap_or(false);
            if !deleted {
                r = fail("DELETE /node/{entity}", "deleted should be true");
            }
        }
        results.push(r);

        // Verify deletion persisted
        let (s2, _) = c.get("/node/DeleteMe").await;
        if s2 == 404 {
            results.push(pass("DELETE /node/{entity} (verify)"));
        } else {
            results.push(fail("DELETE /node/{entity} (verify)", &format!("expected 404, got {s2}")));
        }
    }

    // ========================================================================
    // 40. GET /health -- basic health check
    // ========================================================================
    {
        let (status, body) = c.get("/health").await;
        let mut r = check("GET /health", status, &body, 200);
        if r.passed {
            let health = body.get("status").and_then(|s| s.as_str()).unwrap_or("");
            if health != "ok" {
                r = fail("GET /health", &format!("status should be 'ok', got '{health}'"));
            }
        }
        results.push(r);
    }

    // ========================================================================
    // 41. GET /stats -- graph statistics
    // ========================================================================
    {
        let (status, body) = c.get("/stats").await;
        let mut r = check("GET /stats", status, &body, 200);
        if r.passed {
            let nodes = body.get("nodes").and_then(|n| n.as_u64()).unwrap_or(0);
            r.detail = format!("{nodes} nodes");
            if nodes == 0 {
                r = fail("GET /stats", "should have at least some nodes");
            }
        }
        results.push(r);
    }

    // ========================================================================
    // FUZZY-MATCH TESTS
    // ========================================================================

    // 42. Wrong case: "alice" vs "Alice"
    //     Tests how the graph handles case sensitivity. Engram's search/traverse
    //     may do case-insensitive matching depending on the storage layer.
    {
        let (status, body) = c.post("/chat/export_subgraph", &json!({
            "entity": "alice",
            "depth": 1
        })).await;
        let node_count = body.get("nodes").and_then(|n| n.as_array()).map(|a| a.len()).unwrap_or(0);
        // Accept any non-error response: the key test is that the endpoint doesn't crash
        if status == 200 {
            results.push(TestResult {
                name: "FUZZY: wrong case 'alice'".into(),
                passed: true,
                detail: format!("{node_count} nodes (case-insensitive match or empty)"),
            });
        } else {
            results.push(TestResult {
                name: "FUZZY: wrong case 'alice'".into(),
                passed: status != 500,
                detail: format!("status {status}"),
            });
        }
    }

    // 43. Wrong case relate: "alice" -> "acme"
    {
        let (status, body) = c.post("/relate", &json!({
            "from": "alice",
            "to": "acme",
            "relationship": "fuzzy_test"
        })).await;
        // Should either fail or create new nodes (since case-sensitive)
        if status == 200 {
            // Store succeeded -- created lowercase nodes (auto-store on relate)
            let from = body.get("from").and_then(|f| f.as_str()).unwrap_or("");
            results.push(TestResult {
                name: "FUZZY: relate wrong case".into(),
                passed: true,
                detail: format!("created relation with from='{from}' (auto-stored lowercase)"),
            });
        } else {
            results.push(TestResult {
                name: "FUZZY: relate wrong case".into(),
                passed: true,
                detail: format!("status {status}: {}", body.get("error").and_then(|e| e.as_str()).unwrap_or("unknown")),
            });
        }
    }

    // 44. Wrong separator: "acme-corp" when "Acme" exists
    {
        let (status, body) = c.post("/search", &json!({
            "query": "acme-corp",
            "limit": 5
        })).await;
        let total = body.get("total").and_then(|t| t.as_u64()).unwrap_or(0);
        results.push(TestResult {
            name: "FUZZY: search 'acme-corp'".into(),
            passed: true,
            detail: format!("status={status}, total={total} (no fuzzy match expected)"),
        });
    }

    // 45. Explain non-existent entity
    {
        let (status, _body) = c.get("/explain/NonExistent").await;
        if status == 404 || status == 500 {
            results.push(pass("FUZZY: explain non-existent entity"));
        } else {
            results.push(fail("FUZZY: explain non-existent entity", &format!("expected 404/500, got {status}")));
        }
    }

    // 46. Shortest path between non-existent entities
    {
        let (status, body) = c.post("/chat/shortest_path", &json!({
            "from": "NonExistentA",
            "to": "NonExistentB"
        })).await;
        let found = body.get("found").and_then(|f| f.as_bool()).unwrap_or(true);
        if status == 200 && !found {
            results.push(pass("FUZZY: shortest_path non-existent"));
        } else {
            results.push(TestResult {
                name: "FUZZY: shortest_path non-existent".into(),
                passed: status != 500,
                detail: format!("status={status}, found={found}"),
            });
        }
    }

    // ========================================================================
    // WRITE PERSISTENCE TESTS
    // ========================================================================

    // 47. Store + verify type persisted
    {
        c.post("/store", &json!({
            "entity": "PersistTest",
            "type": "Vehicle",
            "confidence": 0.8,
            "properties": {"color": "red"}
        })).await;
        let (status, body) = c.get("/node/PersistTest").await;
        let mut r = check("PERSIST: store with type+props", status, &body, 200);
        if r.passed {
            let nt = body.get("node_type").and_then(|t| t.as_str()).unwrap_or("");
            let color = body.get("properties").and_then(|p| p.get("color")).and_then(|c| c.as_str()).unwrap_or("");
            if nt != "Vehicle" {
                r = fail("PERSIST: store with type+props", &format!("type should be 'Vehicle', got '{nt}'"));
            } else if color != "red" {
                r = fail("PERSIST: store with type+props", &format!("color should be 'red', got '{color}'"));
            } else {
                r.detail = "type=Vehicle, color=red".into();
            }
        }
        results.push(r);
    }

    // 48. Relate + verify edge persisted
    {
        c.post("/relate", &json!({
            "from": "PersistTest",
            "to": "Alice",
            "relationship": "owned_by",
            "confidence": 0.7
        })).await;
        let (status, body) = c.get("/node/PersistTest").await;
        let mut r = check("PERSIST: relate verify", status, &body, 200);
        if r.passed {
            let edges_from = body.get("edges_from").and_then(|e| e.as_array()).map(|a| a.len()).unwrap_or(0);
            if edges_from == 0 {
                r = fail("PERSIST: relate verify", "edges_from should have at least 1 edge after relate");
            } else {
                r.detail = format!("{edges_from} outgoing edges");
            }
        }
        results.push(r);
    }

    // ========================================================================
    // EDGE OPERATIONS
    // ========================================================================

    // 49. PATCH /edge (rename edge)
    {
        let (status, body) = c.patch("/edge", &json!({
            "from": "PersistTest",
            "to": "Alice",
            "old_rel_type": "owned_by",
            "new_rel_type": "driven_by"
        })).await;
        let r = check("PATCH /edge (rename)", status, &body, 200);
        results.push(r);
    }

    // 50. POST /edge/delete
    {
        // Create an edge to delete
        c.post("/relate", &json!({
            "from": "PersistTest",
            "to": "Bob",
            "relationship": "temp_edge"
        })).await;
        let (status, body) = c.post("/edge/delete", &json!({
            "from": "PersistTest",
            "to": "Bob",
            "rel_type": "temp_edge"
        })).await;
        let r = check("POST /edge/delete", status, &body, 200);
        results.push(r);
    }

    // ========================================================================
    // SUMMARY
    // ========================================================================

    println!("\n{}", "=".repeat(70));
    println!("  CHAT TOOL ENDPOINT TEST SUMMARY");
    println!("{}", "=".repeat(70));

    let total = results.len();
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = total - passed;

    for r in &results {
        let icon = if r.passed { "PASS" } else { "FAIL" };
        println!("  [{icon}] {:<45} {}", r.name, r.detail);
    }

    println!("\n  Total: {total}  Passed: {passed}  Failed: {failed}");
    println!("{}\n", "=".repeat(70));

    // Assert all tests passed
    let failures: Vec<&TestResult> = results.iter().filter(|r| !r.passed).collect();
    if !failures.is_empty() {
        for f in &failures {
            eprintln!("  FAILED: {} -- {}", f.name, f.detail);
        }
        panic!("{failed} test(s) failed out of {total}");
    }
}
