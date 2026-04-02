//! Analysis card renderers: query, search, explain, compare, path, bar chart, gaps, isolated.

use super::{confidence_bar, type_badge, entity_link, html_escape};

pub(super) fn query_card(data: &serde_json::Value) -> String {
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

pub(super) fn search_card(tool_name: &str, data: &serde_json::Value) -> String {
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

pub(super) fn explain_card(data: &serde_json::Value) -> String {
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

pub(super) fn compare_card(data: &serde_json::Value) -> String {
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
        if let Some(ec) = a.get("edge_count").and_then(|v| v.as_u64()) {
            html.push_str(&format!("<div style=\"font-size:0.75rem;color:var(--text-muted)\">{} connections</div>", ec));
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
        if let Some(ec) = b.get("edge_count").and_then(|v| v.as_u64()) {
            html.push_str(&format!("<div style=\"font-size:0.75rem;color:var(--text-muted)\">{} connections</div>", ec));
        }
    }
    html.push_str("</div>");

    // Shared neighbors
    if let Some(shared) = data.get("shared_neighbors").or_else(|| data.get("common_connections")).and_then(|v| v.as_array()) {
        if !shared.is_empty() {
            html.push_str(&format!(
                "<div class=\"chat-compare-section\" style=\"grid-column:1/-1;margin-top:8px\">\
                    <strong><i class=\"fa-solid fa-link\" style=\"font-size:0.7rem\"></i> {} shared connection{}</strong><div style=\"margin-top:4px;display:flex;flex-wrap:wrap;gap:4px\">",
                shared.len(), if shared.len() != 1 { "s" } else { "" },
            ));
            for (i, s) in shared.iter().enumerate() {
                if i >= 10 { html.push_str("<span style=\"color:var(--text-muted);font-size:0.75rem\">...</span>"); break; }
                if let Some(name) = s.as_str() {
                    html.push_str(&format!("<span style=\"font-size:0.8rem\">{}</span>", entity_link(name)));
                }
            }
            html.push_str("</div></div>");
        }
    }

    // Unique to A
    let label_a = entity_a.and_then(|a| a.get("label")).and_then(|v| v.as_str()).unwrap_or("A");
    if let Some(unique) = data.get("unique_to_a").and_then(|v| v.as_array()) {
        if !unique.is_empty() {
            html.push_str(&format!(
                "<div class=\"chat-compare-section\" style=\"grid-column:1/-1;margin-top:4px\">\
                    <strong style=\"font-size:0.8rem\">Only {}: </strong>",
                html_escape(label_a),
            ));
            for (i, s) in unique.iter().enumerate() {
                if i >= 10 { html.push_str("<span style=\"color:var(--text-muted);font-size:0.75rem\">...</span>"); break; }
                if let Some(name) = s.as_str() {
                    if i > 0 { html.push_str(", "); }
                    html.push_str(&format!("<span style=\"font-size:0.8rem\">{}</span>", entity_link(name)));
                }
            }
            html.push_str("</div>");
        }
    }

    // Unique to B
    let label_b = entity_b.and_then(|b| b.get("label")).and_then(|v| v.as_str()).unwrap_or("B");
    if let Some(unique) = data.get("unique_to_b").and_then(|v| v.as_array()) {
        if !unique.is_empty() {
            html.push_str(&format!(
                "<div class=\"chat-compare-section\" style=\"grid-column:1/-1;margin-top:4px\">\
                    <strong style=\"font-size:0.8rem\">Only {}: </strong>",
                html_escape(label_b),
            ));
            for (i, s) in unique.iter().enumerate() {
                if i >= 10 { html.push_str("<span style=\"color:var(--text-muted);font-size:0.75rem\">...</span>"); break; }
                if let Some(name) = s.as_str() {
                    if i > 0 { html.push_str(", "); }
                    html.push_str(&format!("<span style=\"font-size:0.8rem\">{}</span>", entity_link(name)));
                }
            }
            html.push_str("</div>");
        }
    }

    html.push_str("</div></div>");
    html
}

pub(super) fn gaps_card(data: &serde_json::Value) -> String {
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

pub(super) fn bar_chart_card(data: &serde_json::Value) -> String {
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
        if i >= 15 { break; }
        let label = e.get("label").and_then(|v| v.as_str()).unwrap_or("?");
        let ntype = e.get("node_type").and_then(|v| v.as_str());
        let count = e.get("edge_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let pct = if max_count > 0.0 { (count as f64 / max_count * 100.0).round() } else { 0.0 };
        let badge = ntype.map(|t| type_badge(t)).unwrap_or_default();
        html.push_str(&format!(
            "<div class=\"chat-bar-row\">\
                <span class=\"chat-bar-label\">{} {}</span>\
                <div class=\"chat-bar-track\"><div class=\"chat-bar-fill\" style=\"width:{}%\"></div></div>\
                <span class=\"chat-bar-count\">{}</span>\
             </div>",
            entity_link(label), badge, pct, count,
        ));
    }

    html.push_str("</div></div>");
    html
}

pub(super) fn path_card(data: &serde_json::Value) -> String {
    let found = data.get("found").and_then(|v| v.as_bool()).unwrap_or(false);
    if !found {
        return "<div class=\"chat-card chat-card-empty\"><i class=\"fa-solid fa-route\"></i> No path found between these entities</div>".to_string();
    }

    // Single path: { found, path: [PathStep], length }
    let path = data.get("path").and_then(|v| v.as_array());
    // Multi-path fallback: { paths: [[str]] }
    let paths = data.get("paths").and_then(|v| v.as_array());

    if let Some(steps) = path {
        if steps.is_empty() {
            return "<div class=\"chat-card chat-card-empty\"><i class=\"fa-solid fa-route\"></i> No path found</div>".to_string();
        }
        let length = data.get("length").and_then(|v| v.as_u64()).unwrap_or(steps.len() as u64);

        let mut html = format!(
            "<div class=\"chat-card\">\
                <div class=\"chat-card-header\"><i class=\"fa-solid fa-route\"></i> Path Found ({} hop{})</div>\
                <div class=\"chat-card-body\"><div class=\"chat-path-chain\">",
            length, if length != 1 { "s" } else { "" },
        );

        // Build: Entity -[rel]-> Entity -[rel]-> Entity
        // The path starts from the first step; we need to infer the start entity
        // PathStep: { entity, relationship, direction }
        for (i, step) in steps.iter().enumerate() {
            let entity = step.get("entity").and_then(|v| v.as_str()).unwrap_or("?");
            let rel = step.get("relationship").and_then(|v| v.as_str()).unwrap_or("");
            let dir = step.get("direction").and_then(|v| v.as_str()).unwrap_or("->");

            if i > 0 || !rel.is_empty() {
                let arrow = if dir == "<-" {
                    "<i class=\"fa-solid fa-arrow-left\" style=\"font-size:0.6rem;color:var(--text-muted)\"></i>"
                } else {
                    "<i class=\"fa-solid fa-arrow-right\" style=\"font-size:0.6rem;color:var(--text-muted)\"></i>"
                };
                html.push_str(&format!(
                    " <span style=\"font-size:0.7rem;color:var(--accent);padding:0 2px\">[{}]</span> {} ",
                    html_escape(rel), arrow,
                ));
            }
            html.push_str(&entity_link(entity));
        }

        html.push_str("</div></div></div>");
        return html;
    }

    // Fallback: multi-path format (array of string arrays)
    if let Some(paths) = paths {
        if paths.is_empty() {
            return "<div class=\"chat-card chat-card-empty\"><i class=\"fa-solid fa-route\"></i> No paths found</div>".to_string();
        }
        let mut html = format!(
            "<div class=\"chat-card\">\
                <div class=\"chat-card-header\"><i class=\"fa-solid fa-route\"></i> {} Path{} Found</div>\
                <div class=\"chat-card-body\">",
            paths.len(), if paths.len() != 1 { "s" } else { "" },
        );
        for (i, p) in paths.iter().enumerate() {
            if i >= 5 { break; }
            if let Some(nodes) = p.as_array() {
                let labels: Vec<&str> = nodes.iter().filter_map(|n| n.as_str()).collect();
                let path_html: Vec<String> = labels.iter().map(|l| entity_link(l)).collect();
                html.push_str(&format!(
                    "<div class=\"chat-path-row\"><span class=\"chat-path-hops\">{} hops</span> {}</div>",
                    labels.len().saturating_sub(1),
                    path_html.join(" <i class=\"fa-solid fa-arrow-right\" style=\"font-size:0.6rem;color:var(--text-muted)\"></i> "),
                ));
            }
        }
        html.push_str("</div></div>");
        return html;
    }

    "<div class=\"chat-card chat-card-empty\"><i class=\"fa-solid fa-route\"></i> No path data</div>".to_string()
}

pub(super) fn isolated_card(data: &serde_json::Value) -> String {
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
        if i >= 20 { break; }
        let label = e.get("label").and_then(|v| v.as_str()).unwrap_or("?");
        let ntype = e.get("node_type").and_then(|v| v.as_str()).unwrap_or("entity");
        let edge_count = e.get("edge_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let edge_label = if edge_count == 0 { "no connections".to_string() } else { format!("{} connection{}", edge_count, if edge_count != 1 { "s" } else { "" }) };
        html.push_str(&format!(
            "<div class=\"chat-entity-row\">{} {} <span style=\"color:var(--text-muted);font-size:0.75rem;margin-left:auto\">{}</span></div>",
            entity_link(label), type_badge(ntype), edge_label,
        ));
    }

    html.push_str("</div></div>");
    html
}
