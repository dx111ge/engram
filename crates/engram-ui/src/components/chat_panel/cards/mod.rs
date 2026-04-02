//! Tool result card renderers: rich HTML cards for chat display.

mod analysis;
mod temporal;
mod investigation;
mod reporting;
mod assessment;
mod actions;

use super::markdown::html_escape;

/// Render a styled HTML card for a tool result, based on the tool name.
pub fn render_tool_card(tool_name: &str, raw_json: &str) -> String {
    let parsed: serde_json::Value = match serde_json::from_str(raw_json) {
        Ok(v) => v,
        Err(_) => return fallback_card(tool_name, raw_json),
    };

    match tool_name {
        "engram_query" => analysis::query_card(&parsed),
        "engram_search" | "engram_similar" => analysis::search_card(tool_name, &parsed),
        "engram_explain" => analysis::explain_card(&parsed),
        "engram_compare" => analysis::compare_card(&parsed),
        "engram_timeline" | "engram_entity_timeline" => temporal::timeline_card(&parsed),
        "engram_gaps" => analysis::gaps_card(&parsed),
        "engram_most_connected" => analysis::bar_chart_card(&parsed),
        "engram_briefing" => reporting::briefing_card(&parsed),
        "engram_shortest_path" => analysis::path_card(&parsed),
        "engram_assess_list" => assessment::assess_list_card(&parsed),
        "engram_assess_create" => assessment::assess_create_card(&parsed),
        "engram_assess_evaluate" => assessment::assess_evaluate_card(&parsed),
        "engram_assess_evidence" => assessment::assess_evidence_card(&parsed),
        "engram_assess_detail" | "engram_assess_get" => assessment::assess_detail_card(&parsed),
        "engram_assess_compare" => assessment::assess_compare_card(&parsed),
        "engram_what_if" => assessment::whatif_card(&parsed),
        "engram_influence_path" => assessment::influence_multi_card(&parsed),
        "engram_black_areas" => assessment::black_areas_card(&parsed),
        "engram_isolated" => analysis::isolated_card(&parsed),
        "engram_current_state" => temporal::current_state_card(&parsed),
        "engram_fact_provenance" => temporal::fact_provenance_card(&parsed),
        "engram_contradictions" => temporal::contradiction_card(&parsed),
        "engram_situation_at" => temporal::situation_at_card(&parsed),
        "engram_ingest" => investigation::ingest_card(&parsed),
        "engram_analyze" => investigation::analyze_card(&parsed),
        "engram_investigate_preview" => investigation::investigate_preview_card(&parsed),
        "engram_changes" => investigation::changes_card(&parsed),
        "engram_watch" => investigation::watch_card(&parsed),
        "engram_network_analysis" => investigation::network_analysis_card(&parsed),
        "engram_entity_360" => investigation::entity_360_card(&parsed),
        "engram_entity_gaps" => investigation::entity_gaps_card(&parsed),
        "engram_export" => reporting::export_card(&parsed),
        "engram_dossier" => reporting::dossier_card(&parsed),
        "engram_topic_map" => reporting::topic_map_card(&parsed),
        "engram_graph_stats" => reporting::graph_stats_card(&parsed),
        "engram_provenance" => reporting::provenance_card(&parsed),
        "engram_documents" => reporting::documents_card(&parsed),
        "engram_rule_create" => actions::rule_create_card(&parsed),
        "engram_rule_list" => actions::rule_list_card(&parsed),
        "engram_rule_fire" => actions::rule_fire_card(&parsed),
        "engram_schedule" => actions::schedule_card(&parsed),
        _ => fallback_card(tool_name, raw_json),
    }
}

/// Extract nodes and edges arrays from a tool result for graph integration.
pub fn extract_graph_data(tool_name: &str, raw_json: &str) -> Option<(Vec<serde_json::Value>, Vec<serde_json::Value>)> {
    let parsed: serde_json::Value = serde_json::from_str(raw_json).ok()?;
    match tool_name {
        "engram_query" | "engram_explain" | "engram_what_if" | "engram_shortest_path" => {
            let nodes = parsed.get("nodes")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let edges = parsed.get("edges")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if nodes.is_empty() { None } else { Some((nodes, edges)) }
        }
        "engram_search" | "engram_similar" => {
            let results = parsed.get("results")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if results.is_empty() { return None; }
            let nodes: Vec<serde_json::Value> = results.iter().map(|r| {
                serde_json::json!({
                    "id": r.get("label").and_then(|v| v.as_str()).unwrap_or(""),
                    "label": r.get("label").and_then(|v| v.as_str()).unwrap_or(""),
                    "node_type": r.get("node_type").and_then(|v| v.as_str()).unwrap_or("Entity"),
                    "confidence": r.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.5),
                })
            }).collect();
            Some((nodes, Vec::new()))
        }
        _ => None,
    }
}

// ── Shared helpers ──

pub(crate) fn confidence_bar(value: f64) -> String {
    let pct = (value * 100.0).round() as i32;
    let color = if pct >= 70 {
        "var(--success, #5cb85c)"
    } else if pct >= 40 {
        "var(--warning, #f0ad4e)"
    } else {
        "var(--danger, #d9534f)"
    };
    format!(
        "<div class=\"chat-confidence-bar\">\
            <div class=\"chat-confidence-fill\" style=\"width:{}%;background:{}\"></div>\
            <span class=\"chat-confidence-label\">{}%</span>\
         </div>",
        pct, color, pct,
    )
}

pub(crate) fn type_badge(node_type: &str) -> String {
    format!("<span class=\"chat-type-badge\">{}</span>", html_escape(node_type))
}

pub(crate) fn entity_link(name: &str) -> String {
    let escaped = html_escape(name);
    format!(
        "<span class=\"chat-entity-name\" data-entity=\"{}\" onclick=\"window.dispatchEvent(new CustomEvent('engram-navigate',{{detail:'{}'}}))\">{}</span>",
        escaped,
        escaped.replace('\'', "\\'"),
        escaped,
    )
}

pub(crate) fn fallback_card(tool_name: &str, raw_json: &str) -> String {
    // Try to pretty-format JSON, fall back to raw
    let display = if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw_json) {
        serde_json::to_string_pretty(&v).unwrap_or_else(|_| raw_json.to_string())
    } else {
        raw_json.to_string()
    };

    let truncated = if display.len() > 2000 {
        format!("{}...", &display[..2000])
    } else {
        display
    };

    format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-wrench\"></i> {}</div>\
            <pre class=\"chat-code-block\">{}</pre>\
         </div>",
        html_escape(tool_name),
        html_escape(&truncated),
    )
}

/// Render a node detail card for display in chat when a node is clicked.
pub fn render_node_detail_card(label: &str, node_type: Option<&str>, confidence: f64, edges_from: usize, edges_to: usize) -> String {
    let ntype = node_type.unwrap_or("Entity");
    format!(
        "<div class=\"chat-card chat-node-detail\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-circle-info\"></i> {label_e}</div>\
            <div class=\"chat-card-body\">\
                <div class=\"chat-entity-row\">{badge} {conf_bar}</div>\
                <div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Connections</span><span>{ef} outgoing, {et} incoming</span></div>\
                <div class=\"chat-node-actions\">\
                    <button class=\"chat-btn chat-btn-primary\" onclick=\"window.dispatchEvent(new CustomEvent('engram-open-detail',{{detail:'{label_js}'}}))\"><i class=\"fa-solid fa-expand\"></i> Open</button>\
                    <button class=\"chat-btn chat-btn-secondary\" onclick=\"window.dispatchEvent(new CustomEvent('engram-set-path-from',{{detail:'{label_js}'}}))\"><i class=\"fa-solid fa-route\"></i> Path</button>\
                </div>\
            </div>\
         </div>",
        label_e = html_escape(label),
        badge = type_badge(ntype),
        conf_bar = confidence_bar(confidence),
        ef = edges_from,
        et = edges_to,
        label_js = html_escape(label).replace('\'', "\\'"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_card_empty() {
        let data = r#"{"nodes":[],"edges":[]}"#;
        let html = render_tool_card("engram_query", data);
        assert!(html.contains("No results found"));
    }

    #[test]
    fn test_query_card_with_nodes() {
        let data = r#"{"nodes":[{"label":"Putin","node_type":"Person","confidence":0.85}],"edges":[]}"#;
        let html = render_tool_card("engram_query", data);
        assert!(html.contains("Putin"));
        assert!(html.contains("chat-type-badge"));
        assert!(html.contains("chat-confidence-bar"));
    }

    #[test]
    fn test_search_card() {
        let data = r#"{"results":[{"label":"NATO","node_type":"Organization","confidence":0.9}]}"#;
        let html = render_tool_card("engram_search", data);
        assert!(html.contains("NATO"));
        assert!(html.contains("Search Results"));
    }

    #[test]
    fn test_gaps_card() {
        let data = r#"{"gaps":[{"severity":"high","description":"Missing source for claim"}]}"#;
        let html = render_tool_card("engram_gaps", data);
        assert!(html.contains("Missing source"));
        assert!(html.contains("chat-gap-dot"));
    }

    #[test]
    fn test_fallback_card() {
        let html = render_tool_card("engram_unknown", r#"{"foo":"bar"}"#);
        assert!(html.contains("engram_unknown"));
        assert!(html.contains("chat-code-block"));
    }

    #[test]
    fn test_malformed_json() {
        let html = render_tool_card("engram_query", "not json at all");
        assert!(html.contains("engram_query"));
    }

    #[test]
    fn test_xss_in_entity_name() {
        let data = r#"{"nodes":[{"label":"<script>alert(1)</script>","node_type":"Entity","confidence":0.5}],"edges":[]}"#;
        let html = render_tool_card("engram_query", data);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn test_node_detail_card() {
        let html = render_node_detail_card("Putin", Some("Person"), 0.85, 10, 5);
        assert!(html.contains("Putin"));
        assert!(html.contains("Person"));
        assert!(html.contains("10 outgoing"));
        assert!(html.contains("5 incoming"));
    }

    #[test]
    fn test_extract_graph_data_query() {
        let data = r#"{"nodes":[{"label":"A"}],"edges":[{"from":"A","to":"B"}]}"#;
        let result = extract_graph_data("engram_query", data);
        assert!(result.is_some());
        let (nodes, edges) = result.unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(edges.len(), 1);
    }

    #[test]
    fn test_extract_graph_data_search() {
        let data = r#"{"results":[{"label":"A","node_type":"Entity","confidence":0.5}]}"#;
        let result = extract_graph_data("engram_search", data);
        assert!(result.is_some());
    }

    #[test]
    fn test_extract_graph_data_none() {
        let result = extract_graph_data("engram_briefing", r#"{"topic":"test"}"#);
        assert!(result.is_none());
    }

    #[test]
    fn test_explain_card() {
        let data = r#"{"label":"Russia","node_type":"Country","confidence":0.92,"edges_from":[{"relationship":"has_capital","to":"Moscow"}],"edges_to":[]}"#;
        let html = render_tool_card("engram_explain", data);
        assert!(html.contains("Russia"));
        assert!(html.contains("Country"));
        assert!(html.contains("chat-confidence-bar"));
        assert!(html.contains("Moscow"));
        assert!(html.contains("has_capital"));
    }

    #[test]
    fn test_compare_card() {
        let data = r#"{"entity_a":{"label":"NATO","node_type":"Organization","confidence":0.9},"entity_b":{"label":"CSTO","node_type":"Organization","confidence":0.7}}"#;
        let html = render_tool_card("engram_compare", data);
        assert!(html.contains("NATO"));
        assert!(html.contains("CSTO"));
        assert!(html.contains("chat-compare-grid"));
    }

    #[test]
    fn test_timeline_card_with_edges() {
        let data = r#"{"edges":[{"from":"Russia","relationship":"invaded","to":"Ukraine","valid_from":"2022-02-24"}]}"#;
        let html = render_tool_card("engram_timeline", data);
        assert!(html.contains("Timeline"));
        assert!(html.contains("2022-02-24"));
        assert!(html.contains("Russia"));
        assert!(html.contains("invaded"));
        assert!(html.contains("Ukraine"));
    }

    #[test]
    fn test_timeline_card_empty() {
        let data = r#"{"events":[]}"#;
        let html = render_tool_card("engram_timeline", data);
        assert!(html.contains("No timeline events found"));
    }

    #[test]
    fn test_path_card() {
        let data = r#"{"paths":[["Putin","Russia","Moscow"]]}"#;
        let html = render_tool_card("engram_shortest_path", data);
        assert!(html.contains("Path"));
        assert!(html.contains("Putin"));
        assert!(html.contains("Moscow"));
        assert!(html.contains("2 hops"));
    }

    #[test]
    fn test_path_card_empty() {
        let data = r#"{"paths":[]}"#;
        let html = render_tool_card("engram_shortest_path", data);
        assert!(html.contains("No paths found"));
    }

    #[test]
    fn test_assessment_card() {
        let data = r#"{"title":"Sanctions Impact","category":"economic","probability":0.65}"#;
        let html = render_tool_card("engram_assess_create", data);
        assert!(html.contains("Sanctions Impact"));
        assert!(html.contains("economic"));
        assert!(html.contains("chat-confidence-bar"));
    }

    #[test]
    fn test_whatif_card() {
        let data = r#"{"entity":"Putin","new_confidence":0.2,"affected":[{"label":"Russia","delta":-0.15}]}"#;
        let html = render_tool_card("engram_what_if", data);
        assert!(html.contains("Putin"));
        assert!(html.contains("Russia"));
        assert!(html.contains("What-If"));
    }

    #[test]
    fn test_most_connected_card() {
        let data = r#"{"entities":[{"label":"Russia","edge_count":42},{"label":"USA","edge_count":38}]}"#;
        let html = render_tool_card("engram_most_connected", data);
        assert!(html.contains("Russia"));
        assert!(html.contains("USA"));
        assert!(html.contains("chat-bar-fill"));
    }

    #[test]
    fn test_isolated_card() {
        let data = r#"{"entities":[{"label":"Obscure Entity","node_type":"Person"}]}"#;
        let html = render_tool_card("engram_isolated", data);
        assert!(html.contains("Obscure Entity"));
        assert!(html.contains("Isolated"));
    }

    #[test]
    fn test_similar_card() {
        let data = r#"{"results":[{"label":"NATO","node_type":"Organization","confidence":0.88}]}"#;
        let html = render_tool_card("engram_similar", data);
        assert!(html.contains("NATO"));
        assert!(html.contains("Similar Entities"));
    }

    #[test]
    fn test_confidence_bar_colors() {
        // High confidence -> green
        let high = render_tool_card("engram_query", r#"{"nodes":[{"label":"A","node_type":"E","confidence":0.85}],"edges":[]}"#);
        assert!(high.contains("var(--success"));
        // Medium confidence -> yellow
        let mid = render_tool_card("engram_query", r#"{"nodes":[{"label":"A","node_type":"E","confidence":0.55}],"edges":[]}"#);
        assert!(mid.contains("var(--warning"));
        // Low confidence -> red
        let low = render_tool_card("engram_query", r#"{"nodes":[{"label":"A","node_type":"E","confidence":0.2}],"edges":[]}"#);
        assert!(low.contains("var(--danger"));
    }

    #[test]
    fn test_entity_link_clickable() {
        let data = r#"{"nodes":[{"label":"Putin","node_type":"Person","confidence":0.9}],"edges":[]}"#;
        let html = render_tool_card("engram_query", data);
        assert!(html.contains("chat-entity-name"));
        assert!(html.contains("engram-navigate"));
        assert!(html.contains("onclick"));
    }

    #[test]
    fn test_tool_routing_search() {
        // engram_search should produce a search card, not a fallback
        let data = r#"{"results":[{"label":"Test","node_type":"Entity","confidence":0.5}]}"#;
        let html = render_tool_card("engram_search", data);
        assert!(html.contains("Search Results"));
        assert!(!html.contains("chat-code-block")); // not fallback
    }

    #[test]
    fn test_tool_routing_explain() {
        // engram_explain should produce an explain card
        let data = r#"{"label":"Test","node_type":"Entity","confidence":0.5,"edges_from":[],"edges_to":[]}"#;
        let html = render_tool_card("engram_explain", data);
        assert!(html.contains("fa-solid fa-circle-info"));
        assert!(!html.contains("chat-code-block"));
    }

    #[test]
    fn test_tool_routing_query() {
        // engram_query with nodes should produce entity cards
        let data = r#"{"nodes":[{"label":"A","node_type":"Entity","confidence":0.5}],"edges":[]}"#;
        let html = render_tool_card("engram_query", data);
        assert!(html.contains("chat-entity-row"));
        assert!(!html.contains("chat-code-block"));
    }

    #[test]
    fn test_graph_data_not_extracted_for_write_tools() {
        let result = extract_graph_data("engram_store", r#"{"label":"test"}"#);
        assert!(result.is_none());
        let result = extract_graph_data("engram_ingest_text", r#"{"text":"test"}"#);
        assert!(result.is_none());
    }
}
