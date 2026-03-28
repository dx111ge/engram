//! Tool result card renderers: rich HTML cards for chat display.

use super::markdown::html_escape;

/// Render a styled HTML card for a tool result, based on the tool name.
pub fn render_tool_card(tool_name: &str, raw_json: &str) -> String {
    let parsed: serde_json::Value = match serde_json::from_str(raw_json) {
        Ok(v) => v,
        Err(_) => return fallback_card(tool_name, raw_json),
    };

    match tool_name {
        "engram_query" => query_card(&parsed),
        "engram_search" | "engram_similar" => search_card(tool_name, &parsed),
        "engram_explain" => explain_card(&parsed),
        "engram_compare" => compare_card(&parsed),
        "engram_timeline" | "engram_entity_timeline" => timeline_card(&parsed),
        "engram_gaps" => gaps_card(&parsed),
        "engram_most_connected" => bar_chart_card(&parsed),
        "engram_briefing" => briefing_card(&parsed),
        "engram_shortest_path" => path_card(&parsed),
        "engram_assess_list" | "engram_assess_get" | "engram_assess_create"
        | "engram_assess_evaluate" => assessment_card(&parsed),
        "engram_what_if" => whatif_card(&parsed),
        "engram_influence_path" => influence_card(&parsed),
        "engram_isolated" => isolated_card(&parsed),
        "engram_provenance" => provenance_card(&parsed),
        "engram_documents" => documents_card(&parsed),
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

// ── Card renderers ──

fn confidence_bar(value: f64) -> String {
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

fn type_badge(node_type: &str) -> String {
    format!("<span class=\"chat-type-badge\">{}</span>", html_escape(node_type))
}

fn entity_link(name: &str) -> String {
    let escaped = html_escape(name);
    format!(
        "<span class=\"chat-entity-name\" data-entity=\"{}\" onclick=\"window.dispatchEvent(new CustomEvent('engram-navigate',{{detail:'{}'}}))\">{}</span>",
        escaped,
        escaped.replace('\'', "\\'"),
        escaped,
    )
}

fn query_card(data: &serde_json::Value) -> String {
    let nodes = data.get("nodes").and_then(|v| v.as_array());
    let edges = data.get("edges").and_then(|v| v.as_array());
    let node_count = nodes.map(|n| n.len()).unwrap_or(0);
    let edge_count = edges.map(|e| e.len()).unwrap_or(0);

    if node_count == 0 {
        return "<div class=\"chat-card chat-card-empty\"><i class=\"fa-solid fa-circle-info\"></i> No results found</div>".to_string();
    }

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-diagram-project\"></i> {} entities, {} connections</div>\
            <div class=\"chat-card-body\">",
        node_count, edge_count,
    );

    if let Some(nodes) = nodes {
        for (i, n) in nodes.iter().enumerate() {
            if i >= 8 {
                html.push_str(&format!("<div class=\"chat-card-more\">...and {} more</div>", node_count - 8));
                break;
            }
            let label = n.get("label").and_then(|v| v.as_str()).unwrap_or("?");
            let ntype = n.get("node_type").and_then(|v| v.as_str()).unwrap_or("Entity");
            let conf = n.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.5);
            html.push_str(&format!(
                "<div class=\"chat-entity-row\">{} {} {}</div>",
                entity_link(label), type_badge(ntype), confidence_bar(conf),
            ));
        }
    }

    html.push_str("</div></div>");
    html
}

fn search_card(tool_name: &str, data: &serde_json::Value) -> String {
    let results = data.get("results").and_then(|v| v.as_array());
    let count = results.map(|r| r.len()).unwrap_or(0);
    let icon = if tool_name == "engram_similar" { "fa-solid fa-arrows-spin" } else { "fa-solid fa-magnifying-glass" };
    let title = if tool_name == "engram_similar" { "Similar Entities" } else { "Search Results" };

    if count == 0 {
        return "<div class=\"chat-card chat-card-empty\"><i class=\"fa-solid fa-circle-info\"></i> No matches found</div>".to_string();
    }

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"{}\"></i> {} ({})</div>\
            <div class=\"chat-card-body\">",
        icon, title, count,
    );

    if let Some(results) = results {
        for (i, r) in results.iter().enumerate() {
            if i >= 10 {
                html.push_str(&format!("<div class=\"chat-card-more\">...and {} more</div>", count - 10));
                break;
            }
            let label = r.get("label").and_then(|v| v.as_str()).unwrap_or("?");
            let ntype = r.get("node_type").and_then(|v| v.as_str()).unwrap_or("Entity");
            let conf = r.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.5);
            html.push_str(&format!(
                "<div class=\"chat-entity-row\">{} {} {}</div>",
                entity_link(label), type_badge(ntype), confidence_bar(conf),
            ));
        }
    }

    html.push_str("</div></div>");
    html
}

fn explain_card(data: &serde_json::Value) -> String {
    // API returns "entity" not "label", and node_type may be in properties or cooccurrences
    let label = data.get("entity").or_else(|| data.get("label"))
        .and_then(|v| v.as_str()).unwrap_or("Unknown");
    let ntype = data.get("node_type").and_then(|v| v.as_str())
        .or_else(|| data.get("properties").and_then(|p| p.get("node_type")).and_then(|v| v.as_str()))
        .unwrap_or("Entity");
    let canonical = data.get("properties")
        .and_then(|p| p.get("canonical_name")).and_then(|v| v.as_str());
    let conf = data.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.5);
    let edges_from = data.get("edges_from").and_then(|v| v.as_array());
    let edges_to = data.get("edges_to").and_then(|v| v.as_array());
    let ef_count = edges_from.map(|e| e.len()).unwrap_or(0);
    let et_count = edges_to.map(|e| e.len()).unwrap_or(0);
    // Use canonical name in header if available
    let display_name = canonical.unwrap_or(label);

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-circle-info\"></i> {}</div>\
            <div class=\"chat-card-body\">\
                <div class=\"chat-entity-row\">{} {}</div>\
                <div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Connections</span><span>{} outgoing, {} incoming</span></div>",
        html_escape(display_name), type_badge(ntype), confidence_bar(conf), ef_count, et_count,
    );

    // Show properties if present
    if let Some(props) = data.get("properties").and_then(|v| v.as_object()) {
        for (k, v) in props {
            let val_str = match v {
                serde_json::Value::String(s) => html_escape(s),
                other => html_escape(&other.to_string()),
            };
            html.push_str(&format!(
                "<div class=\"chat-prop-row\"><span class=\"chat-prop-key\">{}</span><span>{}</span></div>",
                html_escape(k), val_str,
            ));
        }
    }

    // Show a few edges
    if let Some(edges) = edges_from {
        for (i, e) in edges.iter().enumerate() {
            if i >= 5 {
                html.push_str(&format!("<div class=\"chat-card-more\">...and {} more outgoing</div>", ef_count - 5));
                break;
            }
            let rel = e.get("relationship").and_then(|v| v.as_str()).unwrap_or("?");
            let to = e.get("to").and_then(|v| v.as_str()).unwrap_or("?");
            html.push_str(&format!(
                "<div class=\"chat-edge-row\"><i class=\"fa-solid fa-arrow-right\"></i> {} {}</div>",
                html_escape(rel), entity_link(to),
            ));
        }
    }

    html.push_str("</div></div>");
    html
}

fn compare_card(data: &serde_json::Value) -> String {
    let entity_a = data.get("entity_a").or_else(|| data.get("a"));
    let entity_b = data.get("entity_b").or_else(|| data.get("b"));

    let mut html = "<div class=\"chat-card\"><div class=\"chat-card-header\"><i class=\"fa-solid fa-code-compare\"></i> Comparison</div><div class=\"chat-compare-grid\">".to_string();

    // Left column
    html.push_str("<div class=\"chat-compare-col\">");
    if let Some(a) = entity_a {
        let label = a.get("label").and_then(|v| v.as_str()).unwrap_or("Entity A");
        html.push_str(&format!("<div class=\"chat-compare-header\">{}</div>", entity_link(label)));
        if let Some(ntype) = a.get("node_type").and_then(|v| v.as_str()) {
            html.push_str(&type_badge(ntype));
        }
        if let Some(conf) = a.get("confidence").and_then(|v| v.as_f64()) {
            html.push_str(&confidence_bar(conf));
        }
    }
    html.push_str("</div>");

    // Right column
    html.push_str("<div class=\"chat-compare-col\">");
    if let Some(b) = entity_b {
        let label = b.get("label").and_then(|v| v.as_str()).unwrap_or("Entity B");
        html.push_str(&format!("<div class=\"chat-compare-header\">{}</div>", entity_link(label)));
        if let Some(ntype) = b.get("node_type").and_then(|v| v.as_str()) {
            html.push_str(&type_badge(ntype));
        }
        if let Some(conf) = b.get("confidence").and_then(|v| v.as_f64()) {
            html.push_str(&confidence_bar(conf));
        }
    }
    html.push_str("</div>");

    // Common connections if present
    if let Some(common) = data.get("common_connections").and_then(|v| v.as_array()) {
        if !common.is_empty() {
            html.push_str(&format!(
                "<div class=\"chat-compare-common\"><strong>{} shared connections</strong></div>",
                common.len(),
            ));
        }
    }

    html.push_str("</div></div>");
    html
}

fn timeline_card(data: &serde_json::Value) -> String {
    // Timeline API can return edges array or events array
    let events = data.get("events")
        .or_else(|| data.get("timeline"))
        .or_else(|| data.get("edges"))
        .and_then(|v| v.as_array());

    if events.is_none() || events.unwrap().is_empty() {
        return "<div class=\"chat-card chat-card-empty\"><i class=\"fa-solid fa-clock\"></i> No timeline events found</div>".to_string();
    }
    let events = events.unwrap();

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-clock\"></i> Timeline ({} events)</div>\
            <div class=\"chat-card-body\">",
        events.len(),
    );

    for (i, ev) in events.iter().enumerate() {
        if i >= 10 {
            html.push_str(&format!("<div class=\"chat-card-more\">...and {} more events</div>", events.len() - 10));
            break;
        }
        let date = ev.get("date")
            .or_else(|| ev.get("valid_from"))
            .and_then(|v| v.as_str()).unwrap_or("");
        // Try multiple description fields: description, label, or build from edge fields
        let desc = ev.get("description")
            .or_else(|| ev.get("label"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                // Edge format: from -> relationship -> to
                let from = ev.get("from").and_then(|v| v.as_str()).unwrap_or("");
                let rel = ev.get("relationship").and_then(|v| v.as_str()).unwrap_or("");
                let to = ev.get("to").and_then(|v| v.as_str()).unwrap_or("");
                if !from.is_empty() && !to.is_empty() {
                    format!("{} {} {}", from, rel, to)
                } else {
                    String::new()
                }
            });
        html.push_str(&format!(
            "<div class=\"chat-timeline-item\"><span class=\"chat-timeline-date\">{}</span><span>{}</span></div>",
            html_escape(date), html_escape(&desc),
        ));
    }

    html.push_str("</div></div>");
    html
}

fn gaps_card(data: &serde_json::Value) -> String {
    let gaps = data.get("gaps").and_then(|v| v.as_array());
    if gaps.is_none() || gaps.unwrap().is_empty() {
        return "<div class=\"chat-card chat-card-empty\"><i class=\"fa-solid fa-check-circle\"></i> No knowledge gaps detected</div>".to_string();
    }
    let gaps = gaps.unwrap();

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-circle-question\"></i> Knowledge Gaps ({})</div>\
            <div class=\"chat-card-body\">",
        gaps.len(),
    );

    for (i, g) in gaps.iter().enumerate() {
        if i >= 8 {
            html.push_str(&format!("<div class=\"chat-card-more\">...and {} more gaps</div>", gaps.len() - 8));
            break;
        }
        let severity = g.get("severity").and_then(|v| v.as_str()).unwrap_or("medium");
        let desc = g.get("description").and_then(|v| v.as_str()).unwrap_or("?");
        let dot_color = match severity {
            "high" | "critical" => "var(--danger, #d9534f)",
            "medium" => "var(--warning, #f0ad4e)",
            _ => "var(--info, #5bc0de)",
        };
        html.push_str(&format!(
            "<div class=\"chat-gap-row\">\
                <span class=\"chat-gap-dot\" style=\"background:{}\"></span>\
                <span>{}</span>\
             </div>",
            dot_color, html_escape(desc),
        ));
    }

    html.push_str("</div></div>");
    html
}

fn bar_chart_card(data: &serde_json::Value) -> String {
    let entities = data.get("entities")
        .or_else(|| data.get("results"))
        .and_then(|v| v.as_array());
    if entities.is_none() || entities.unwrap().is_empty() {
        return "<div class=\"chat-card chat-card-empty\"><i class=\"fa-solid fa-chart-bar\"></i> No results</div>".to_string();
    }
    let entities = entities.unwrap();
    let max_count = entities.iter()
        .filter_map(|e| e.get("edge_count").and_then(|v| v.as_u64()))
        .max()
        .unwrap_or(1) as f64;

    let mut html = "<div class=\"chat-card\"><div class=\"chat-card-header\"><i class=\"fa-solid fa-chart-bar\"></i> Most Connected</div><div class=\"chat-card-body\">".to_string();

    for (i, e) in entities.iter().enumerate() {
        if i >= 10 { break; }
        let label = e.get("label").and_then(|v| v.as_str()).unwrap_or("?");
        let count = e.get("edge_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let pct = if max_count > 0.0 { (count as f64 / max_count * 100.0).round() } else { 0.0 };
        html.push_str(&format!(
            "<div class=\"chat-bar-row\">\
                <span class=\"chat-bar-label\">{}</span>\
                <div class=\"chat-bar-track\"><div class=\"chat-bar-fill\" style=\"width:{}%\"></div></div>\
                <span class=\"chat-bar-count\">{}</span>\
             </div>",
            entity_link(label), pct, count,
        ));
    }

    html.push_str("</div></div>");
    html
}

fn briefing_card(data: &serde_json::Value) -> String {
    let topic = data.get("topic").and_then(|v| v.as_str()).unwrap_or("Briefing");
    let sections = data.get("sections").and_then(|v| v.as_array());

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-file-lines\"></i> {}</div>\
            <div class=\"chat-card-body\">",
        html_escape(topic),
    );

    if let Some(sections) = sections {
        for s in sections {
            let title = s.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let content = s.get("content").and_then(|v| v.as_str()).unwrap_or("");
            html.push_str(&format!(
                "<div class=\"chat-briefing-section\"><strong>{}</strong><p>{}</p></div>",
                html_escape(title), html_escape(content),
            ));
        }
    } else {
        // Fallback: render as formatted JSON summary
        let summary = data.get("summary").and_then(|v| v.as_str())
            .or_else(|| data.get("content").and_then(|v| v.as_str()));
        if let Some(text) = summary {
            html.push_str(&format!("<p>{}</p>", html_escape(text)));
        }
    }

    html.push_str("</div></div>");
    html
}

fn path_card(data: &serde_json::Value) -> String {
    let paths = data.get("paths").and_then(|v| v.as_array());
    if paths.is_none() || paths.unwrap().is_empty() {
        return "<div class=\"chat-card chat-card-empty\"><i class=\"fa-solid fa-route\"></i> No paths found</div>".to_string();
    }
    let paths = paths.unwrap();

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-route\"></i> {} Path{} Found</div>\
            <div class=\"chat-card-body\">",
        paths.len(), if paths.len() != 1 { "s" } else { "" },
    );

    for (i, p) in paths.iter().enumerate() {
        if i >= 5 { break; }
        if let Some(nodes) = p.as_array() {
            let labels: Vec<&str> = nodes.iter()
                .filter_map(|n| n.as_str())
                .collect();
            let path_html: Vec<String> = labels.iter()
                .map(|l| entity_link(l))
                .collect();
            html.push_str(&format!(
                "<div class=\"chat-path-row\"><span class=\"chat-path-hops\">{} hops</span> {}</div>",
                labels.len().saturating_sub(1),
                path_html.join(" <i class=\"fa-solid fa-arrow-right\" style=\"font-size:0.6rem;color:var(--text-muted)\"></i> "),
            ));
        }
    }

    html.push_str("</div></div>");
    html
}

fn assessment_card(data: &serde_json::Value) -> String {
    let title = data.get("title").and_then(|v| v.as_str())
        .or_else(|| data.get("label").and_then(|v| v.as_str()))
        .unwrap_or("Assessment");
    let probability = data.get("probability").and_then(|v| v.as_f64());
    let category = data.get("category").and_then(|v| v.as_str());

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-scale-balanced\"></i> {}</div>\
            <div class=\"chat-card-body\">",
        html_escape(title),
    );

    if let Some(cat) = category {
        html.push_str(&format!("<div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Category</span><span>{}</span></div>", html_escape(cat)));
    }
    if let Some(prob) = probability {
        html.push_str(&format!("<div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Probability</span>{}</div>", confidence_bar(prob)));
    }

    // Evidence if present
    if let Some(evidence) = data.get("evidence").and_then(|v| v.as_array()) {
        for ev in evidence.iter().take(5) {
            let entity = ev.get("entity").and_then(|v| v.as_str()).unwrap_or("?");
            let direction = ev.get("direction").and_then(|v| v.as_str()).unwrap_or("?");
            let icon = if direction == "supports" { "fa-solid fa-arrow-up" } else { "fa-solid fa-arrow-down" };
            let color = if direction == "supports" { "var(--success)" } else { "var(--danger)" };
            html.push_str(&format!(
                "<div class=\"chat-evidence-row\"><i class=\"{}\" style=\"color:{}\"></i> {} <span class=\"chat-text-muted\">({})</span></div>",
                icon, color, entity_link(entity), html_escape(direction),
            ));
        }
    }

    // List of assessments (from assess_list)
    if let Some(assessments) = data.get("assessments").and_then(|v| v.as_array()) {
        for a in assessments.iter().take(8) {
            let t = a.get("title").or_else(|| a.get("label")).and_then(|v| v.as_str()).unwrap_or("?");
            let p = a.get("probability").and_then(|v| v.as_f64()).unwrap_or(0.5);
            html.push_str(&format!(
                "<div class=\"chat-entity-row\"><span>{}</span>{}</div>",
                html_escape(t), confidence_bar(p),
            ));
        }
    }

    html.push_str("</div></div>");
    html
}

fn whatif_card(data: &serde_json::Value) -> String {
    let entity = data.get("entity").and_then(|v| v.as_str()).unwrap_or("?");
    let new_conf = data.get("new_confidence").and_then(|v| v.as_f64());
    let affected = data.get("affected").and_then(|v| v.as_array());

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-code-branch\"></i> What-If: {}</div>\
            <div class=\"chat-card-body\">",
        entity_link(entity),
    );

    if let Some(conf) = new_conf {
        html.push_str(&format!("<div class=\"chat-prop-row\"><span class=\"chat-prop-key\">New confidence</span>{}</div>", confidence_bar(conf)));
    }

    if let Some(affected) = affected {
        html.push_str(&format!("<div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Affected</span><span>{} entities</span></div>", affected.len()));
        for (i, a) in affected.iter().enumerate() {
            if i >= 8 { break; }
            let label = a.get("label").and_then(|v| v.as_str()).unwrap_or("?");
            let delta = a.get("delta").and_then(|v| v.as_f64());
            let delta_str = delta.map(|d| format!("{:+.0}%", d * 100.0)).unwrap_or_default();
            html.push_str(&format!(
                "<div class=\"chat-entity-row\">{} <span class=\"chat-text-muted\">{}</span></div>",
                entity_link(label), delta_str,
            ));
        }
    }

    html.push_str("</div></div>");
    html
}

fn influence_card(data: &serde_json::Value) -> String {
    let from = data.get("from").and_then(|v| v.as_str()).unwrap_or("?");
    let to = data.get("to").and_then(|v| v.as_str()).unwrap_or("?");
    let paths = data.get("paths").and_then(|v| v.as_array());

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-share-nodes\"></i> Influence: {} to {}</div>\
            <div class=\"chat-card-body\">",
        entity_link(from), entity_link(to),
    );

    if let Some(paths) = paths {
        for (i, p) in paths.iter().enumerate() {
            if i >= 5 { break; }
            if let Some(nodes) = p.as_array() {
                let labels: Vec<String> = nodes.iter()
                    .filter_map(|n| n.as_str().map(|s| entity_link(s)))
                    .collect();
                html.push_str(&format!(
                    "<div class=\"chat-path-row\">{}</div>",
                    labels.join(" <i class=\"fa-solid fa-arrow-right\" style=\"font-size:0.6rem;color:var(--text-muted)\"></i> "),
                ));
            }
        }
    }

    html.push_str("</div></div>");
    html
}

fn isolated_card(data: &serde_json::Value) -> String {
    let entities = data.get("entities").and_then(|v| v.as_array());
    if entities.is_none() || entities.unwrap().is_empty() {
        return "<div class=\"chat-card chat-card-empty\"><i class=\"fa-solid fa-circle-check\"></i> No isolated entities</div>".to_string();
    }
    let entities = entities.unwrap();

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-circle-dot\"></i> Isolated Entities ({})</div>\
            <div class=\"chat-card-body\">",
        entities.len(),
    );

    for (i, e) in entities.iter().enumerate() {
        if i >= 10 { break; }
        let label = e.get("label").and_then(|v| v.as_str()).unwrap_or("?");
        let ntype = e.get("node_type").and_then(|v| v.as_str()).unwrap_or("Entity");
        html.push_str(&format!(
            "<div class=\"chat-entity-row\">{} {}</div>",
            entity_link(label), type_badge(ntype),
        ));
    }

    html.push_str("</div></div>");
    html
}

fn fallback_card(tool_name: &str, raw_json: &str) -> String {
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

// ── Provenance card ──

fn provenance_card(v: &serde_json::Value) -> String {
    let entity = v.get("entity").and_then(|v| v.as_str()).unwrap_or("?");
    let docs = v.get("documents").and_then(|v| v.as_array());
    let count = v.get("document_count").and_then(|v| v.as_u64()).unwrap_or(0);

    let mut html = format!(
        r#"<div class="tool-card provenance-card">
        <div class="tool-card-header"><i class="fa-solid fa-file-lines"></i> Provenance: {entity}</div>
        <div class="tool-card-sub">{count} source document(s)</div>"#
    );

    if let Some(docs) = docs {
        for doc in docs {
            let title = doc.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled");
            let url = doc.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let date = doc.get("doc_date").and_then(|v| v.as_str()).unwrap_or("");
            let publisher = doc.get("publisher").and_then(|v| v.as_str()).unwrap_or("");
            let facts = doc.get("facts").and_then(|v| v.as_array());

            html.push_str(&format!(
                r#"<div class="provenance-doc">
                <div class="provenance-doc-title"><i class="fa-solid fa-file"></i> {title}</div>"#
            ));
            if !url.is_empty() {
                html.push_str(&format!(r#"<div class="provenance-doc-url">{url}</div>"#));
            }
            if !date.is_empty() || !publisher.is_empty() {
                html.push_str(&format!(
                    r#"<div class="provenance-doc-meta">{date} {publisher}</div>"#
                ));
            }
            if let Some(facts) = facts {
                for fact in facts {
                    let claim = fact.get("claim").and_then(|v| v.as_str()).unwrap_or("");
                    if !claim.is_empty() {
                        html.push_str(&format!(
                            r#"<div class="provenance-claim"><i class="fa-solid fa-quote-left"></i> {claim}</div>"#
                        ));
                    }
                }
            }
            html.push_str("</div>");
        }
    }

    if count == 0 {
        html.push_str(r#"<div class="tool-card-empty"><i class="fa-solid fa-circle-info"></i> No source documents found. Ingest content to build provenance.</div>"#);
    }

    html.push_str("</div>");
    html
}

// ── Documents list card ──

fn documents_card(v: &serde_json::Value) -> String {
    let docs = v.get("documents").and_then(|v| v.as_array());
    let count = v.get("count").and_then(|v| v.as_u64()).unwrap_or(0);

    let mut html = format!(
        r#"<div class="tool-card documents-card">
        <div class="tool-card-header"><i class="fa-solid fa-folder-open"></i> Ingested Documents ({count})</div>"#
    );

    if let Some(docs) = docs {
        html.push_str(r#"<table class="tool-card-table"><thead><tr><th>Title</th><th>Publisher</th><th>Facts</th><th>Size</th></tr></thead><tbody>"#);
        for doc in docs {
            let title = doc.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled");
            let publisher = doc.get("publisher").and_then(|v| v.as_str()).unwrap_or("");
            let fact_count = doc.get("fact_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let content_length = doc.get("content_length").and_then(|v| v.as_str()).unwrap_or("0");
            let bytes: u64 = content_length.parse().unwrap_or(0);
            let size_str = if bytes > 1_000_000 {
                format!("{:.1}MB", bytes as f64 / 1_000_000.0)
            } else if bytes > 1_000 {
                format!("{:.1}KB", bytes as f64 / 1_000.0)
            } else {
                format!("{}B", bytes)
            };
            html.push_str(&format!(
                "<tr><td>{title}</td><td>{publisher}</td><td>{fact_count}</td><td>{size_str}</td></tr>"
            ));
        }
        html.push_str("</tbody></table>");
    }

    if count == 0 {
        html.push_str(r#"<div class="tool-card-empty"><i class="fa-solid fa-circle-info"></i> No documents ingested yet.</div>"#);
    }

    html.push_str("</div>");
    html
}
