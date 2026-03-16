/// Chat tool integration tests — exercises graph operations that back the chat tool endpoints.
///
/// Each test validates a specific chat tool capability: temporal queries, entity comparison,
/// path finding, connectivity analysis, change tracking, confidence cascades, and briefings.

use engram_core::graph::{Graph, Provenance};
use std::collections::HashSet;
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

// ============================================================================
// 1. Temporal query: edges with date ranges, filter by date window
// ============================================================================

#[test]
fn temporal_query_filters_by_date_range() {
    let (_dir, mut g) = setup();
    let p = prov("analyst");

    g.store_with_confidence("Alice", 0.9, &p).unwrap();
    g.store_with_confidence("Org-Alpha", 0.9, &p).unwrap();
    g.store_with_confidence("Org-Beta", 0.9, &p).unwrap();

    // Alice worked at Alpha from 2020-2023
    g.relate_with_temporal("Alice", "Org-Alpha", "employed_at", 0.9,
        Some("2020-01-01"), Some("2023-06-30"), &p).unwrap();
    // Alice works at Beta from 2023 onward (no end date)
    g.relate_with_temporal("Alice", "Org-Beta", "employed_at", 0.9,
        Some("2023-07-01"), None, &p).unwrap();

    let edges = g.edges_from("Alice").unwrap();
    assert_eq!(edges.len(), 2);

    // Filter: edges active in 2024 (only Beta should match)
    let active_2024: Vec<_> = edges.iter().filter(|e| {
        let started = e.valid_from.as_deref().map_or(true, |d| d <= "2024-06-01");
        let not_ended = e.valid_to.as_deref().map_or(true, |d| d >= "2024-06-01");
        started && not_ended
    }).collect();
    assert_eq!(active_2024.len(), 1);
    assert_eq!(active_2024[0].to, "Org-Beta");

    // Filter: edges active in 2021 (only Alpha should match)
    let active_2021: Vec<_> = edges.iter().filter(|e| {
        let started = e.valid_from.as_deref().map_or(true, |d| d <= "2021-06-01");
        let not_ended = e.valid_to.as_deref().map_or(true, |d| d >= "2021-06-01");
        started && not_ended
    }).collect();
    assert_eq!(active_2021.len(), 1);
    assert_eq!(active_2021[0].to, "Org-Alpha");
}

// ============================================================================
// 2. Timeline: edges returned sorted chronologically by valid_from
// ============================================================================

#[test]
fn timeline_returns_chronological_order() {
    let (_dir, mut g) = setup();
    let p = prov("analyst");

    g.store_with_confidence("Project-X", 0.9, &p).unwrap();
    g.store_with_confidence("Phase-Design", 0.9, &p).unwrap();
    g.store_with_confidence("Phase-Build", 0.9, &p).unwrap();
    g.store_with_confidence("Phase-Test", 0.9, &p).unwrap();
    g.store_with_confidence("Phase-Deploy", 0.9, &p).unwrap();

    // Create edges in non-chronological order
    g.relate_with_temporal("Project-X", "Phase-Test", "has_phase", 0.9,
        Some("2025-07-01"), Some("2025-09-30"), &p).unwrap();
    g.relate_with_temporal("Project-X", "Phase-Design", "has_phase", 0.9,
        Some("2025-01-01"), Some("2025-03-31"), &p).unwrap();
    g.relate_with_temporal("Project-X", "Phase-Deploy", "has_phase", 0.9,
        Some("2025-10-01"), Some("2025-12-31"), &p).unwrap();
    g.relate_with_temporal("Project-X", "Phase-Build", "has_phase", 0.9,
        Some("2025-04-01"), Some("2025-06-30"), &p).unwrap();

    let mut edges = g.edges_from("Project-X").unwrap();

    // Sort chronologically by valid_from (simulating timeline tool behavior)
    edges.sort_by(|a, b| {
        a.valid_from.as_deref().unwrap_or("").cmp(b.valid_from.as_deref().unwrap_or(""))
    });

    let timeline_order: Vec<&str> = edges.iter().map(|e| e.to.as_str()).collect();
    assert_eq!(timeline_order, vec!["Phase-Design", "Phase-Build", "Phase-Test", "Phase-Deploy"]);
}

// ============================================================================
// 3. Current state: filter out expired edges, keep only currently valid
// ============================================================================

#[test]
fn current_state_filters_expired_edges() {
    let (_dir, mut g) = setup();
    let p = prov("analyst");

    g.store_with_confidence("Server-01", 0.9, &p).unwrap();
    g.store_with_confidence("OS-Ubuntu-20", 0.9, &p).unwrap();
    g.store_with_confidence("OS-Ubuntu-24", 0.9, &p).unwrap();
    g.store_with_confidence("App-Legacy", 0.9, &p).unwrap();
    g.store_with_confidence("App-Current", 0.9, &p).unwrap();

    // Expired edges (ended before today 2026-03-16)
    g.relate_with_temporal("Server-01", "OS-Ubuntu-20", "runs", 0.9,
        Some("2020-01-01"), Some("2024-12-31"), &p).unwrap();
    g.relate_with_temporal("Server-01", "App-Legacy", "runs", 0.9,
        Some("2021-01-01"), Some("2025-06-30"), &p).unwrap();

    // Current edges (no end date or end date in the future)
    g.relate_with_temporal("Server-01", "OS-Ubuntu-24", "runs", 0.9,
        Some("2025-01-01"), None, &p).unwrap();
    g.relate_with_temporal("Server-01", "App-Current", "runs", 0.9,
        Some("2025-07-01"), Some("2027-12-31"), &p).unwrap();

    let edges = g.edges_from("Server-01").unwrap();
    assert_eq!(edges.len(), 4);

    // Filter to current edges only (valid_to is None or >= today)
    let today = "2026-03-16";
    let current: Vec<_> = edges.iter().filter(|e| {
        e.valid_to.as_deref().map_or(true, |d| d >= today)
    }).collect();

    assert_eq!(current.len(), 2);
    let current_targets: HashSet<&str> = current.iter().map(|e| e.to.as_str()).collect();
    assert!(current_targets.contains("OS-Ubuntu-24"));
    assert!(current_targets.contains("App-Current"));
}

// ============================================================================
// 4. Compare entities: find shared and unique neighbors
// ============================================================================

#[test]
fn compare_entities_finds_shared_neighbors() {
    let (_dir, mut g) = setup();
    let p = prov("analyst");

    g.store_with_confidence("Company-A", 0.9, &p).unwrap();
    g.store_with_confidence("Company-B", 0.9, &p).unwrap();
    g.store_with_confidence("Shared-Tech-Rust", 0.9, &p).unwrap();
    g.store_with_confidence("Shared-Tech-Docker", 0.9, &p).unwrap();
    g.store_with_confidence("Only-A-Python", 0.9, &p).unwrap();
    g.store_with_confidence("Only-B-Java", 0.9, &p).unwrap();

    g.relate_with_confidence("Company-A", "Shared-Tech-Rust", "uses", 0.9, &p).unwrap();
    g.relate_with_confidence("Company-A", "Shared-Tech-Docker", "uses", 0.9, &p).unwrap();
    g.relate_with_confidence("Company-A", "Only-A-Python", "uses", 0.9, &p).unwrap();

    g.relate_with_confidence("Company-B", "Shared-Tech-Rust", "uses", 0.9, &p).unwrap();
    g.relate_with_confidence("Company-B", "Shared-Tech-Docker", "uses", 0.9, &p).unwrap();
    g.relate_with_confidence("Company-B", "Only-B-Java", "uses", 0.9, &p).unwrap();

    let edges_a: HashSet<String> = g.edges_from("Company-A").unwrap()
        .iter().map(|e| e.to.clone()).collect();
    let edges_b: HashSet<String> = g.edges_from("Company-B").unwrap()
        .iter().map(|e| e.to.clone()).collect();

    let shared: HashSet<_> = edges_a.intersection(&edges_b).collect();
    let only_a: HashSet<_> = edges_a.difference(&edges_b).collect();
    let only_b: HashSet<_> = edges_b.difference(&edges_a).collect();

    assert_eq!(shared.len(), 2);
    assert!(shared.contains(&"Shared-Tech-Rust".to_string()));
    assert!(shared.contains(&"Shared-Tech-Docker".to_string()));
    assert_eq!(only_a.len(), 1);
    assert!(only_a.contains(&"Only-A-Python".to_string()));
    assert_eq!(only_b.len(), 1);
    assert!(only_b.contains(&"Only-B-Java".to_string()));
}

// ============================================================================
// 5. Shortest path: BFS from source to target through a chain
// ============================================================================

#[test]
fn shortest_path_finds_connection() {
    let (_dir, mut g) = setup();
    let p = prov("analyst");

    // Create a chain: Start -> A -> B -> C -> End
    for label in &["Start", "Node-A", "Node-B", "Node-C", "End"] {
        g.store_with_confidence(label, 0.9, &p).unwrap();
    }
    g.relate_with_confidence("Start", "Node-A", "links_to", 0.9, &p).unwrap();
    g.relate_with_confidence("Node-A", "Node-B", "links_to", 0.9, &p).unwrap();
    g.relate_with_confidence("Node-B", "Node-C", "links_to", 0.9, &p).unwrap();
    g.relate_with_confidence("Node-C", "End", "links_to", 0.9, &p).unwrap();

    // BFS traversal from Start should reach End at depth 4
    let result = g.traverse("Start", 5, 0.0).unwrap();
    let target_id = g.find_node_id("End").unwrap().unwrap();
    assert!(result.nodes.contains(&target_id), "End should be reachable from Start");

    // Verify the depth is 4 (Start -> A -> B -> C -> End)
    assert_eq!(*result.depths.get(&target_id).unwrap(), 4);

    // BFS with insufficient depth should NOT reach End
    let shallow = g.traverse("Start", 2, 0.0).unwrap();
    assert!(!shallow.nodes.contains(&target_id), "End should NOT be reachable at depth 2");
}

// ============================================================================
// 6. Most connected: entities sorted by edge count descending
// ============================================================================

#[test]
fn most_connected_returns_sorted() {
    let (_dir, mut g) = setup();
    let p = prov("analyst");

    // Hub has 4 connections, Mid has 2, Leaf has 1
    g.store_with_confidence("Hub", 0.9, &p).unwrap();
    g.store_with_confidence("Mid", 0.9, &p).unwrap();
    g.store_with_confidence("Leaf", 0.9, &p).unwrap();
    g.store_with_confidence("Spoke-1", 0.9, &p).unwrap();
    g.store_with_confidence("Spoke-2", 0.9, &p).unwrap();
    g.store_with_confidence("Spoke-3", 0.9, &p).unwrap();
    g.store_with_confidence("Spoke-4", 0.9, &p).unwrap();

    g.relate_with_confidence("Hub", "Spoke-1", "connects", 0.9, &p).unwrap();
    g.relate_with_confidence("Hub", "Spoke-2", "connects", 0.9, &p).unwrap();
    g.relate_with_confidence("Hub", "Spoke-3", "connects", 0.9, &p).unwrap();
    g.relate_with_confidence("Hub", "Spoke-4", "connects", 0.9, &p).unwrap();
    g.relate_with_confidence("Mid", "Spoke-1", "connects", 0.9, &p).unwrap();
    g.relate_with_confidence("Mid", "Spoke-2", "connects", 0.9, &p).unwrap();
    g.relate_with_confidence("Leaf", "Spoke-1", "connects", 0.9, &p).unwrap();

    let mut nodes = g.all_nodes().unwrap();

    // Sort by total edge count (out + in) descending
    nodes.sort_by(|a, b| {
        let total_a = a.edge_out_count + a.edge_in_count;
        let total_b = b.edge_out_count + b.edge_in_count;
        total_b.cmp(&total_a)
    });

    // Hub should be first (4 outgoing edges)
    let top = &nodes[0];
    assert_eq!(top.label, "Hub");
    assert_eq!(top.edge_out_count, 4);

    // Verify sorting is strictly descending
    for pair in nodes.windows(2) {
        let total_a = pair[0].edge_out_count + pair[0].edge_in_count;
        let total_b = pair[1].edge_out_count + pair[1].edge_in_count;
        assert!(total_a >= total_b, "Nodes should be sorted by connectivity descending");
    }
}

// ============================================================================
// 7. Isolated nodes: detect nodes with zero edges
// ============================================================================

#[test]
fn isolated_nodes_detected() {
    let (_dir, mut g) = setup();
    let p = prov("analyst");

    g.store_with_confidence("Connected-A", 0.9, &p).unwrap();
    g.store_with_confidence("Connected-B", 0.9, &p).unwrap();
    g.store_with_confidence("Isolated-X", 0.9, &p).unwrap();
    g.store_with_confidence("Isolated-Y", 0.9, &p).unwrap();
    g.store_with_confidence("Isolated-Z", 0.9, &p).unwrap();

    g.relate_with_confidence("Connected-A", "Connected-B", "knows", 0.9, &p).unwrap();

    let nodes = g.all_nodes().unwrap();
    let isolated: Vec<_> = nodes.iter()
        .filter(|n| n.edge_out_count == 0 && n.edge_in_count == 0)
        .collect();

    assert_eq!(isolated.len(), 3);
    let isolated_labels: HashSet<&str> = isolated.iter().map(|n| n.label.as_str()).collect();
    assert!(isolated_labels.contains("Isolated-X"));
    assert!(isolated_labels.contains("Isolated-Y"));
    assert!(isolated_labels.contains("Isolated-Z"));

    // Connected nodes should NOT be isolated
    assert!(!isolated_labels.contains("Connected-A"));
    assert!(!isolated_labels.contains("Connected-B"));
}

// ============================================================================
// 8. Changes since timestamp: filter nodes by updated_at
// ============================================================================

#[test]
fn changes_since_timestamp() {
    let (_dir, mut g) = setup();
    let p = prov("analyst");

    // Store initial batch
    g.store_with_confidence("Entity-Old", 0.9, &p).unwrap();
    g.store_with_confidence("Entity-Static", 0.9, &p).unwrap();

    // Record the max created_at before adding new entities
    let nodes_before = g.all_nodes().unwrap();
    let max_created_before = nodes_before.iter().map(|n| n.created_at).max().unwrap();

    // Small sleep to ensure timestamp difference
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Store new entities after the cutoff
    g.store_with_confidence("Entity-New-1", 0.9, &p).unwrap();
    g.store_with_confidence("Entity-New-2", 0.85, &p).unwrap();

    let all_nodes = g.all_nodes().unwrap();
    let created_after: Vec<_> = all_nodes.iter()
        .filter(|n| n.created_at > max_created_before)
        .collect();

    let labels: HashSet<&str> = created_after.iter().map(|n| n.label.as_str()).collect();
    assert!(labels.contains("Entity-New-1"), "New entity should appear in changes");
    assert!(labels.contains("Entity-New-2"), "New entity should appear in changes");
    assert!(!labels.contains("Entity-Old"), "Pre-existing entity should not appear");
    assert!(!labels.contains("Entity-Static"), "Pre-existing entity should not appear");
}

// ============================================================================
// 9. What-if confidence cascade: find entities affected by a confidence change
// ============================================================================

#[test]
fn what_if_confidence_cascade() {
    let (_dir, mut g) = setup();
    let p = prov("analyst");

    // Source -> Fact-A -> Conclusion-1
    // Source -> Fact-B -> Conclusion-2
    // Source is the root; if its confidence drops, downstream should be affected
    g.store_with_confidence("Source-Intel", 0.9, &p).unwrap();
    g.store_with_confidence("Fact-A", 0.85, &p).unwrap();
    g.store_with_confidence("Fact-B", 0.80, &p).unwrap();
    g.store_with_confidence("Conclusion-1", 0.75, &p).unwrap();
    g.store_with_confidence("Conclusion-2", 0.70, &p).unwrap();
    g.store_with_confidence("Unrelated", 0.90, &p).unwrap();

    g.relate_with_confidence("Source-Intel", "Fact-A", "supports", 0.9, &p).unwrap();
    g.relate_with_confidence("Source-Intel", "Fact-B", "supports", 0.9, &p).unwrap();
    g.relate_with_confidence("Fact-A", "Conclusion-1", "implies", 0.85, &p).unwrap();
    g.relate_with_confidence("Fact-B", "Conclusion-2", "implies", 0.80, &p).unwrap();

    // Simulate: "what if Source-Intel confidence drops to 0.3?"
    // Find all downstream entities via directed traversal (outgoing only)
    let result = g.traverse_directed("Source-Intel", 3, 0.0, "out").unwrap();

    // Collect affected labels (exclude the source itself)
    let source_id = g.find_node_id("Source-Intel").unwrap().unwrap();
    let affected_ids: Vec<u64> = result.nodes.iter()
        .filter(|&&id| id != source_id)
        .copied()
        .collect();

    let affected_labels: HashSet<String> = affected_ids.iter()
        .map(|&id| g.label_for_id(id).unwrap())
        .collect();

    assert_eq!(affected_labels.len(), 4);
    assert!(affected_labels.contains("Fact-A"));
    assert!(affected_labels.contains("Fact-B"));
    assert!(affected_labels.contains("Conclusion-1"));
    assert!(affected_labels.contains("Conclusion-2"));
    assert!(!affected_labels.contains("Unrelated"));
}

// ============================================================================
// 10. Briefing: traverse a topic cluster and return related entities
// ============================================================================

#[test]
fn briefing_traverses_topic() {
    let (_dir, mut g) = setup();
    let p = prov("analyst");

    // Build a topic cluster around "Cyber-Threat-APT29"
    g.store_with_confidence("Cyber-Threat-APT29", 0.95, &p).unwrap();
    g.set_node_type("Cyber-Threat-APT29", "threat_actor").unwrap();

    g.store_with_confidence("Target-Govt-Agency", 0.90, &p).unwrap();
    g.set_node_type("Target-Govt-Agency", "organization").unwrap();

    g.store_with_confidence("Malware-CozyBear", 0.85, &p).unwrap();
    g.set_node_type("Malware-CozyBear", "malware").unwrap();

    g.store_with_confidence("CVE-2025-1234", 0.80, &p).unwrap();
    g.set_node_type("CVE-2025-1234", "vulnerability").unwrap();

    g.store_with_confidence("Country-Russia", 0.90, &p).unwrap();
    g.set_node_type("Country-Russia", "country").unwrap();

    g.store_with_confidence("Unrelated-Weather", 0.90, &p).unwrap();

    // Build relationships
    g.relate_with_confidence("Cyber-Threat-APT29", "Target-Govt-Agency", "targets", 0.9, &p).unwrap();
    g.relate_with_confidence("Cyber-Threat-APT29", "Malware-CozyBear", "uses", 0.9, &p).unwrap();
    g.relate_with_confidence("Malware-CozyBear", "CVE-2025-1234", "exploits", 0.85, &p).unwrap();
    g.relate_with_confidence("Cyber-Threat-APT29", "Country-Russia", "attributed_to", 0.8, &p).unwrap();

    // Briefing: traverse from topic at depth 2, min confidence 0.5
    let result = g.traverse("Cyber-Threat-APT29", 2, 0.5).unwrap();

    // Should include all related entities within 2 hops
    let briefing_labels: HashSet<String> = result.nodes.iter()
        .map(|&id| g.label_for_id(id).unwrap())
        .collect();

    assert!(briefing_labels.contains("Cyber-Threat-APT29"));
    assert!(briefing_labels.contains("Target-Govt-Agency"));
    assert!(briefing_labels.contains("Malware-CozyBear"));
    assert!(briefing_labels.contains("CVE-2025-1234")); // 2 hops away via malware
    assert!(briefing_labels.contains("Country-Russia"));
    assert!(!briefing_labels.contains("Unrelated-Weather"));

    // Verify edges were captured
    assert!(result.edges.len() >= 4, "Should capture at least 4 edges in the cluster");

    // Verify depth information
    let apt_id = g.find_node_id("Cyber-Threat-APT29").unwrap().unwrap();
    let cve_id = g.find_node_id("CVE-2025-1234").unwrap().unwrap();
    assert_eq!(*result.depths.get(&apt_id).unwrap(), 0);
    assert_eq!(*result.depths.get(&cve_id).unwrap(), 2); // 2 hops: APT29 -> Malware -> CVE
}
