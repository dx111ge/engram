//! Investigation card renderers: ingest, analyze, investigate preview, changes, watch, network, entity 360, entity gaps.

use super::{confidence_bar, type_badge, entity_link, html_escape};

pub(super) fn ingest_card(data: &serde_json::Value) -> String {
    let facts = data.get("facts_stored").and_then(|v| v.as_u64()).unwrap_or(0);
    let rels = data.get("relations_created").and_then(|v| v.as_u64()).unwrap_or(0);
    let duration = data.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(0);
    let errors = data.get("errors").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);

    let icon = if errors > 0 { "fa-solid fa-triangle-exclamation" } else { "fa-solid fa-circle-check" };
    let color = if errors > 0 { "#ffa726" } else { "#66bb6a" };

    format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-file-import\"></i> Ingest Complete</div>\
            <div class=\"chat-card-body\">\
                <div style=\"display:flex;gap:16px;font-size:0.85rem\">\
                    <div><strong>{}</strong> facts stored</div>\
                    <div><strong>{}</strong> relations</div>\
                    <div style=\"color:var(--text-muted)\">{}ms</div>\
                </div>\
                {}\
            </div></div>",
        facts, rels, duration,
        if errors > 0 {
            format!("<div style=\"color:{};margin-top:6px;font-size:0.8rem\"><i class=\"{}\"></i> {} error(s)</div>", color, icon, errors)
        } else {
            format!("<div style=\"color:{};margin-top:4px;font-size:0.8rem\"><i class=\"{}\"></i> Success</div>", color, icon)
        },
    )
}

pub(super) fn analyze_card(data: &serde_json::Value) -> String {
    let entities = data.get("entities").and_then(|v| v.as_array());
    let relations = data.get("relations").and_then(|v| v.as_array());
    let duration = data.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(0);

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-microscope\"></i> NER Analysis <span style=\"font-size:0.75rem;color:var(--text-muted)\">{}ms</span></div>\
            <div class=\"chat-card-body\">",
        duration,
    );

    if let Some(ents) = entities {
        html.push_str(&format!("<div style=\"margin-bottom:6px\"><strong>Entities ({})</strong></div>", ents.len()));
        for (i, e) in ents.iter().enumerate() {
            if i >= 15 { html.push_str("<div style=\"color:var(--text-muted);font-size:0.75rem\">...</div>"); break; }
            let text = e.get("text").and_then(|v| v.as_str()).unwrap_or("?");
            let etype = e.get("entity_type").and_then(|v| v.as_str()).unwrap_or("entity");
            let conf = e.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);
            html.push_str(&format!(
                "<div style=\"font-size:0.8rem;padding:1px 0\">{} {} <span style=\"color:var(--text-muted)\">{:.0}%</span></div>",
                entity_link(text), type_badge(etype), conf * 100.0,
            ));
        }
    }

    if let Some(rels) = relations {
        if !rels.is_empty() {
            html.push_str(&format!("<div style=\"margin:8px 0 4px\"><strong>Relations ({})</strong></div>", rels.len()));
            for (i, r) in rels.iter().enumerate() {
                if i >= 10 { html.push_str("<div style=\"color:var(--text-muted);font-size:0.75rem\">...</div>"); break; }
                let from = r.get("from").and_then(|v| v.as_str()).unwrap_or("?");
                let to = r.get("to").and_then(|v| v.as_str()).unwrap_or("?");
                let rel = r.get("rel_type").and_then(|v| v.as_str()).unwrap_or("?");
                html.push_str(&format!(
                    "<div style=\"font-size:0.8rem;padding:1px 0\">{} <span style=\"color:var(--accent)\">[{}]</span> {}</div>",
                    entity_link(from), html_escape(rel), entity_link(to),
                ));
            }
        }
    }

    html.push_str("</div></div>");
    html
}

pub(super) fn investigate_preview_card(data: &serde_json::Value) -> String {
    let results = data.get("results").and_then(|v| v.as_array());
    let query = data.get("query").and_then(|v| v.as_str()).unwrap_or("?");

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-magnifying-glass-chart\"></i> Investigation: {}</div>\
            <div class=\"chat-card-body\">",
        html_escape(query),
    );

    if let Some(results) = results {
        html.push_str(&format!("<div style=\"margin-bottom:4px\"><strong>{} web results</strong></div>", results.len()));
        for (i, r) in results.iter().enumerate() {
            if i >= 8 { break; }
            let title = r.get("title").and_then(|v| v.as_str()).unwrap_or("?");
            let snippet = r.get("snippet").and_then(|v| v.as_str()).unwrap_or("");
            let truncated = if snippet.len() > 120 { format!("{}...", &snippet[..120]) } else { snippet.to_string() };
            html.push_str(&format!(
                "<div style=\"margin-bottom:6px\"><div style=\"font-size:0.85rem;font-weight:500\">{}</div>\
                <div style=\"font-size:0.75rem;color:var(--text-muted)\">{}</div></div>",
                html_escape(title), html_escape(&truncated),
            ));
        }
    } else {
        html.push_str("<div style=\"color:var(--text-muted)\">No search results found.</div>");
    }

    html.push_str("</div></div>");
    html
}

pub(super) fn changes_card(data: &serde_json::Value) -> String {
    let changes = data.get("changes").and_then(|v| v.as_array());
    let total = data.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
    let since = data.get("since").and_then(|v| v.as_str()).unwrap_or("?");

    if changes.is_none() || changes.unwrap().is_empty() {
        return format!(
            "<div class=\"chat-card chat-card-empty\"><i class=\"fa-solid fa-clock-rotate-left\"></i> No changes since {}</div>",
            html_escape(since),
        );
    }
    let changes = changes.unwrap();

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-clock-rotate-left\"></i> Changes since {} ({} total)</div>\
            <div class=\"chat-card-body\">",
        html_escape(since), total,
    );

    for (i, c) in changes.iter().enumerate() {
        if i >= 20 { html.push_str("<div style=\"color:var(--text-muted);font-size:0.75rem\">...</div>"); break; }
        let label = c.get("label").and_then(|v| v.as_str()).unwrap_or("?");
        let ntype = c.get("node_type").and_then(|v| v.as_str());
        let conf = c.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let badge = ntype.map(|t| type_badge(t)).unwrap_or_default();
        html.push_str(&format!(
            "<div class=\"chat-entity-row\" style=\"font-size:0.8rem\">{} {} <span style=\"color:var(--text-muted)\">{:.0}%</span></div>",
            entity_link(label), badge, conf * 100.0,
        ));
    }

    html.push_str("</div></div>");
    html
}

pub(super) fn watch_card(data: &serde_json::Value) -> String {
    let entity = data.get("entity").and_then(|v| v.as_str()).unwrap_or("?");
    let watched = data.get("watched").and_then(|v| v.as_bool()).unwrap_or(false);
    let icon = if watched { "fa-solid fa-bell" } else { "fa-solid fa-bell-slash" };
    let color = if watched { "#66bb6a" } else { "#ef5350" };
    format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"{}\" style=\"color:{}\"></i> Watch: {}</div>\
            <div class=\"chat-card-body\" style=\"font-size:0.85rem\">{}</div></div>",
        icon, color, entity_link(entity),
        if watched { "Now monitoring this entity for changes." } else { "Watch removed." },
    )
}

pub(super) fn network_analysis_card(data: &serde_json::Value) -> String {
    if data.get("error").and_then(|v| v.as_str()).is_some() {
        let msg = data.get("error").and_then(|v| v.as_str()).unwrap_or("Not found");
        return format!("<div class=\"chat-card chat-card-empty\"><i class=\"fa-solid fa-diagram-project\"></i> {}</div>", html_escape(msg));
    }

    let entity = data.get("entity").and_then(|v| v.as_str()).unwrap_or("?");
    let total_nodes = data.get("total_nodes").and_then(|v| v.as_u64()).unwrap_or(0);
    let total_edges = data.get("total_edges").and_then(|v| v.as_u64()).unwrap_or(0);
    let layers = data.get("layers").and_then(|v| v.as_array());

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-diagram-project\"></i> Network: {} <span style=\"font-size:0.75rem;color:var(--text-muted)\">({} nodes, {} edges)</span></div>\
            <div class=\"chat-card-body\">",
        entity_link(entity), total_nodes, total_edges,
    );

    if let Some(layers) = layers {
        for layer in layers {
            let hop = layer.get("hop").and_then(|v| v.as_u64()).unwrap_or(0);
            let count = layer.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
            let nodes = layer.get("nodes").and_then(|v| v.as_array());

            html.push_str(&format!(
                "<div style=\"margin:6px 0 2px\"><strong style=\"color:var(--accent)\">Hop {} ({} entities)</strong></div>",
                hop, count,
            ));

            if let Some(nodes) = nodes {
                html.push_str("<div style=\"display:flex;flex-wrap:wrap;gap:4px;margin-bottom:4px\">");
                for (i, n) in nodes.iter().enumerate() {
                    if i >= 15 { html.push_str("<span style=\"color:var(--text-muted);font-size:0.75rem\">...</span>"); break; }
                    let label = n.get("label").and_then(|v| v.as_str()).unwrap_or("?");
                    let ntype = n.get("node_type").and_then(|v| v.as_str());
                    let badge = ntype.map(|t| type_badge(t)).unwrap_or_default();
                    html.push_str(&format!(
                        "<span style=\"font-size:0.8rem\">{}{}</span>",
                        entity_link(label), badge,
                    ));
                }
                html.push_str("</div>");
            }
        }
    }

    html.push_str("</div></div>");
    html
}

pub(super) fn entity_360_card(data: &serde_json::Value) -> String {
    let entity = data.get("entity").and_then(|v| v.as_str()).unwrap_or("?");
    let nt = data.get("node_type").and_then(|v| v.as_str());
    let conf = data.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let total_edges = data.get("total_edges").and_then(|v| v.as_u64()).unwrap_or(0);
    let facts_count = data.get("facts_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let props = data.get("properties").and_then(|v| v.as_object());
    let edges_out = data.get("edges_out").and_then(|v| v.as_array());
    let edges_in = data.get("edges_in").and_then(|v| v.as_array());

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-globe\"></i> 360: {}</div>\
            <div class=\"chat-card-body\">\
                <div style=\"display:flex;gap:8px;align-items:center;margin-bottom:8px\">{}{}<span style=\"color:var(--text-muted);font-size:0.8rem\">{} connections | {} facts</span></div>",
        entity_link(entity),
        nt.map(|t| type_badge(t)).unwrap_or_default(),
        confidence_bar(conf),
        total_edges, facts_count,
    );

    // Properties
    if let Some(props) = props {
        if !props.is_empty() {
            html.push_str("<div style=\"margin:6px 0 2px\"><strong style=\"font-size:0.8rem\"><i class=\"fa-solid fa-tags\"></i> Properties</strong></div>");
            for (k, v) in props.iter().take(10) {
                if k.starts_with('_') { continue; }
                let val = v.as_str().unwrap_or("?");
                let truncated = if val.len() > 60 { format!("{}...", &val[..60]) } else { val.to_string() };
                html.push_str(&format!(
                    "<div style=\"font-size:0.75rem;padding:1px 0\"><span style=\"color:var(--accent)\">{}</span>: {}</div>",
                    html_escape(k), html_escape(&truncated),
                ));
            }
        }
    }

    // Outgoing edges
    if let Some(out) = edges_out {
        if !out.is_empty() {
            html.push_str(&format!("<div style=\"margin:6px 0 2px\"><strong style=\"font-size:0.8rem\"><i class=\"fa-solid fa-arrow-right\"></i> Outgoing ({})</strong></div>", data.get("edges_out_count").and_then(|v| v.as_u64()).unwrap_or(0)));
            for (i, e) in out.iter().enumerate() {
                if i >= 10 { html.push_str("<div style=\"color:var(--text-muted);font-size:0.75rem\">...</div>"); break; }
                let to = e.get("to").and_then(|v| v.as_str()).unwrap_or("?");
                let rel = e.get("relationship").and_then(|v| v.as_str()).unwrap_or("?");
                html.push_str(&format!(
                    "<div style=\"font-size:0.8rem;padding:1px 0\"><span style=\"color:var(--accent);font-size:0.7rem\">[{}]</span> {}</div>",
                    html_escape(rel), entity_link(to),
                ));
            }
        }
    }

    // Incoming edges
    if let Some(inc) = edges_in {
        if !inc.is_empty() {
            html.push_str(&format!("<div style=\"margin:6px 0 2px\"><strong style=\"font-size:0.8rem\"><i class=\"fa-solid fa-arrow-left\"></i> Incoming ({})</strong></div>", data.get("edges_in_count").and_then(|v| v.as_u64()).unwrap_or(0)));
            for (i, e) in inc.iter().enumerate() {
                if i >= 10 { html.push_str("<div style=\"color:var(--text-muted);font-size:0.75rem\">...</div>"); break; }
                let from = e.get("from").and_then(|v| v.as_str()).unwrap_or("?");
                let rel = e.get("relationship").and_then(|v| v.as_str()).unwrap_or("?");
                html.push_str(&format!(
                    "<div style=\"font-size:0.8rem;padding:1px 0\">{} <span style=\"color:var(--accent);font-size:0.7rem\">[{}]</span></div>",
                    entity_link(from), html_escape(rel),
                ));
            }
        }
    }

    html.push_str("</div></div>");
    html
}

pub(super) fn entity_gaps_card(data: &serde_json::Value) -> String {
    let entity = data.get("entity").and_then(|v| v.as_str()).unwrap_or("?");
    let nt = data.get("node_type").and_then(|v| v.as_str());
    let completeness = data.get("completeness_pct").and_then(|v| v.as_u64()).unwrap_or(0);
    let present = data.get("present").and_then(|v| v.as_array());
    let missing = data.get("missing").and_then(|v| v.as_array());
    let total_edges = data.get("total_edges").and_then(|v| v.as_u64()).unwrap_or(0);

    let bar_color = if completeness >= 80 { "#66bb6a" } else if completeness >= 50 { "#ffa726" } else { "#ef5350" };

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-puzzle-piece\"></i> Gaps: {} {}</div>\
            <div class=\"chat-card-body\">\
                <div style=\"margin-bottom:8px\">\
                    <div style=\"display:flex;justify-content:space-between;font-size:0.8rem;margin-bottom:2px\">\
                        <span>Completeness</span><span style=\"color:{}\">{completeness}%</span>\
                    </div>\
                    <div style=\"height:6px;background:rgba(255,255,255,0.1);border-radius:3px\">\
                        <div style=\"height:100%;width:{completeness}%;background:{};border-radius:3px\"></div>\
                    </div>\
                    <div style=\"font-size:0.75rem;color:var(--text-muted);margin-top:2px\">{} total edges in graph</div>\
                </div>",
        entity_link(entity), nt.map(|t| type_badge(t)).unwrap_or_default(),
        bar_color, bar_color, total_edges,
    );

    // Present relationships (checkmarks)
    if let Some(present) = present {
        for p in present {
            let rel = p.get("relationship").and_then(|v| v.as_str()).unwrap_or("?");
            let desc = p.get("description").and_then(|v| v.as_str()).unwrap_or("");
            html.push_str(&format!(
                "<div style=\"font-size:0.8rem;padding:2px 0;color:#66bb6a\"><i class=\"fa-solid fa-circle-check\"></i> {} <span style=\"color:var(--text-muted)\">({})</span></div>",
                html_escape(rel), html_escape(desc),
            ));
        }
    }

    // Missing relationships (crosses)
    if let Some(missing) = missing {
        for m in missing {
            let rel = m.get("relationship").and_then(|v| v.as_str()).unwrap_or("?");
            let desc = m.get("description").and_then(|v| v.as_str()).unwrap_or("");
            html.push_str(&format!(
                "<div style=\"font-size:0.8rem;padding:2px 0;color:#ef5350\"><i class=\"fa-solid fa-circle-xmark\"></i> {} <span style=\"color:var(--text-muted)\">({})</span></div>",
                html_escape(rel), html_escape(desc),
            ));
        }
    }

    html.push_str("</div></div>");
    html
}
