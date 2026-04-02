//! Reporting card renderers: briefing, export, dossier, topic map, graph stats, provenance, documents.

use super::{confidence_bar, type_badge, entity_link, html_escape};

pub(super) fn briefing_card(data: &serde_json::Value) -> String {
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

pub(super) fn export_card(data: &serde_json::Value) -> String {
    let center = data.get("center").and_then(|v| v.as_str()).unwrap_or("?");
    let depth = data.get("depth").and_then(|v| v.as_u64()).unwrap_or(0);
    let nodes = data.get("nodes").and_then(|v| v.as_array());
    let edges = data.get("edges").and_then(|v| v.as_array());
    let nc = nodes.map(|a| a.len()).unwrap_or(0);
    let ec = edges.map(|a| a.len()).unwrap_or(0);

    let json_preview = serde_json::to_string_pretty(data)
        .unwrap_or_default();
    let preview_truncated = if json_preview.len() > 500 { format!("{}...", &json_preview[..500]) } else { json_preview.clone() };

    format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-file-export\"></i> Export: {} (depth {})</div>\
            <div class=\"chat-card-body\">\
                <div style=\"display:flex;gap:16px;font-size:0.85rem;margin-bottom:8px\">\
                    <div><strong>{}</strong> nodes</div>\
                    <div><strong>{}</strong> edges</div>\
                </div>\
                <pre style=\"font-size:0.7rem;max-height:200px;overflow:auto;background:rgba(0,0,0,0.2);padding:8px;border-radius:4px\">{}</pre>\
                <button onclick=\"navigator.clipboard.writeText(JSON.stringify({}));this.innerHTML='<i class=\\'fa-solid fa-check\\'></i> Copied!'\" \
                    style=\"margin-top:6px;padding:4px 12px;background:var(--accent);color:white;border:none;border-radius:4px;cursor:pointer;font-size:0.8rem\">\
                    <i class=\"fa-solid fa-copy\"></i> Copy JSON\
                </button>\
            </div></div>",
        entity_link(center), depth, nc, ec, html_escape(&preview_truncated),
        html_escape(&serde_json::to_string(data).unwrap_or_default()),
    )
}

pub(super) fn dossier_card(data: &serde_json::Value) -> String {
    let entity = data.get("entity").and_then(|v| v.as_str()).unwrap_or("?");
    let nt = data.get("node_type").and_then(|v| v.as_str());
    let conf = data.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let total_conn = data.get("total_connections").and_then(|v| v.as_u64()).unwrap_or(0);
    let facts = data.get("facts_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let connections = data.get("connections").and_then(|v| v.as_array());
    let conf_dist = data.get("confidence_distribution");
    let temporal = data.get("temporal_range");

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-address-card\"></i> Dossier: {}</div>\
            <div class=\"chat-card-body\">\
                <div style=\"display:flex;gap:8px;align-items:center;margin-bottom:8px\">{}{}\
                    <span style=\"color:var(--text-muted);font-size:0.8rem\">{} connections | {} facts</span>\
                </div>",
        entity_link(entity),
        nt.map(|t| type_badge(t)).unwrap_or_default(),
        confidence_bar(conf),
        total_conn, facts,
    );

    // Confidence distribution
    if let Some(cd) = conf_dist {
        let h = cd.get("high").and_then(|v| v.as_u64()).unwrap_or(0);
        let m = cd.get("medium").and_then(|v| v.as_u64()).unwrap_or(0);
        let l = cd.get("low").and_then(|v| v.as_u64()).unwrap_or(0);
        html.push_str(&format!(
            "<div style=\"font-size:0.75rem;color:var(--text-muted);margin-bottom:6px\">\
                <span style=\"color:#66bb6a\">{} high</span> | \
                <span style=\"color:#ffa726\">{} medium</span> | \
                <span style=\"color:#ef5350\">{} low</span> confidence\
            </div>",
            h, m, l,
        ));
    }

    // Temporal range
    if let Some(tr) = temporal {
        let earliest = tr.get("earliest").and_then(|v| v.as_str()).unwrap_or("?");
        let latest = tr.get("latest").and_then(|v| v.as_str()).unwrap_or("?");
        if earliest != "unknown" {
            html.push_str(&format!(
                "<div style=\"font-size:0.75rem;color:var(--text-muted);margin-bottom:6px\">\
                    <i class=\"fa-solid fa-calendar\"></i> {} to {}</div>",
                earliest, latest,
            ));
        }
    }

    // Key connections
    if let Some(conns) = connections {
        html.push_str("<div style=\"margin:6px 0 2px\"><strong style=\"font-size:0.8rem\"><i class=\"fa-solid fa-link\"></i> Key Connections</strong></div>");
        for (i, c) in conns.iter().enumerate() {
            if i >= 15 { html.push_str("<div style=\"color:var(--text-muted);font-size:0.75rem\">...</div>"); break; }
            let target = c.get("target").and_then(|v| v.as_str()).unwrap_or("?");
            let rel = c.get("relationship").and_then(|v| v.as_str()).unwrap_or("?");
            let dir = c.get("direction").and_then(|v| v.as_str()).unwrap_or("out");
            let arrow = if dir == "in" { "<i class=\"fa-solid fa-arrow-left\" style=\"font-size:0.5rem;color:var(--text-muted)\"></i>" } else { "" };
            html.push_str(&format!(
                "<div style=\"font-size:0.8rem;padding:1px 0\">{} <span style=\"color:var(--accent);font-size:0.7rem\">[{}]</span> {}</div>",
                arrow, html_escape(rel), entity_link(target),
            ));
        }
    }

    html.push_str("</div></div>");
    html
}

pub(super) fn topic_map_card(data: &serde_json::Value) -> String {
    if data.get("error").and_then(|v| v.as_str()).is_some() {
        let msg = data.get("error").and_then(|v| v.as_str()).unwrap_or("No data");
        return format!("<div class=\"chat-card chat-card-empty\"><i class=\"fa-solid fa-sitemap\"></i> {}</div>", html_escape(msg));
    }

    let topic = data.get("topic").and_then(|v| v.as_str()).unwrap_or("?");
    let total_entities = data.get("total_entities").and_then(|v| v.as_u64()).unwrap_or(0);
    let total_edges = data.get("total_edges").and_then(|v| v.as_u64()).unwrap_or(0);
    let clusters = data.get("clusters").and_then(|v| v.as_array());

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-sitemap\"></i> Topic: {} <span style=\"font-size:0.75rem;color:var(--text-muted)\">({} entities, {} edges)</span></div>\
            <div class=\"chat-card-body\">",
        html_escape(topic), total_entities, total_edges,
    );

    if let Some(clusters) = clusters {
        for cluster in clusters {
            let typ = cluster.get("type").and_then(|v| v.as_str()).unwrap_or("?");
            let count = cluster.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
            let entities = cluster.get("entities").and_then(|v| v.as_array());

            html.push_str(&format!(
                "<div style=\"margin:6px 0 2px\"><strong>{} ({})</strong></div>",
                type_badge(typ), count,
            ));

            if let Some(ents) = entities {
                html.push_str("<div style=\"display:flex;flex-wrap:wrap;gap:4px;margin-bottom:4px\">");
                for (i, e) in ents.iter().enumerate() {
                    if i >= 10 { html.push_str("<span style=\"color:var(--text-muted);font-size:0.75rem\">...</span>"); break; }
                    let label = e.get("label").and_then(|v| v.as_str()).unwrap_or("?");
                    html.push_str(&format!("<span style=\"font-size:0.8rem\">{}</span>", entity_link(label)));
                }
                html.push_str("</div>");
            }
        }
    }

    html.push_str("</div></div>");
    html
}

pub(super) fn graph_stats_card(data: &serde_json::Value) -> String {
    let total_nodes = data.get("total_nodes").and_then(|v| v.as_u64()).unwrap_or(0);
    let total_edges = data.get("total_edges").and_then(|v| v.as_u64()).unwrap_or(0);
    let by_type = data.get("nodes_by_type").and_then(|v| v.as_array());
    let conf_dist = data.get("confidence_distribution");

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-chart-pie\"></i> Knowledge Base Health</div>\
            <div class=\"chat-card-body\">\
                <div style=\"display:flex;gap:24px;font-size:1rem;margin-bottom:12px\">\
                    <div style=\"text-align:center\"><div style=\"font-size:1.5rem;font-weight:700;color:var(--accent)\">{}</div><div style=\"font-size:0.75rem;color:var(--text-muted)\">Entities</div></div>\
                    <div style=\"text-align:center\"><div style=\"font-size:1.5rem;font-weight:700;color:var(--accent)\">{}</div><div style=\"font-size:0.75rem;color:var(--text-muted)\">Connections</div></div>\
                </div>",
        total_nodes, total_edges,
    );

    // Confidence distribution
    if let Some(cd) = conf_dist {
        let h = cd.get("high").and_then(|v| v.as_u64()).unwrap_or(0);
        let m = cd.get("medium").and_then(|v| v.as_u64()).unwrap_or(0);
        let l = cd.get("low").and_then(|v| v.as_u64()).unwrap_or(0);
        let total = (h + m + l).max(1) as f64;
        html.push_str(&format!(
            "<div style=\"margin-bottom:8px\">\
                <div style=\"font-size:0.8rem;font-weight:600;margin-bottom:4px\">Confidence</div>\
                <div style=\"display:flex;height:8px;border-radius:4px;overflow:hidden\">\
                    <div style=\"width:{}%;background:#66bb6a\" title=\"High: {}\"></div>\
                    <div style=\"width:{}%;background:#ffa726\" title=\"Medium: {}\"></div>\
                    <div style=\"width:{}%;background:#ef5350\" title=\"Low: {}\"></div>\
                </div>\
                <div style=\"display:flex;justify-content:space-between;font-size:0.7rem;color:var(--text-muted);margin-top:2px\">\
                    <span style=\"color:#66bb6a\">{} high</span>\
                    <span style=\"color:#ffa726\">{} medium</span>\
                    <span style=\"color:#ef5350\">{} low</span>\
                </div>\
            </div>",
            (h as f64 / total * 100.0).round(), h,
            (m as f64 / total * 100.0).round(), m,
            (l as f64 / total * 100.0).round(), l,
            h, m, l,
        ));
    }

    // Entities by type
    if let Some(types) = by_type {
        html.push_str("<div style=\"font-size:0.8rem;font-weight:600;margin-bottom:4px\">Entity Types</div>");
        for (i, t) in types.iter().enumerate() {
            if i >= 10 { break; }
            let typ = t.get("type").and_then(|v| v.as_str()).unwrap_or("?");
            let count = t.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
            html.push_str(&format!(
                "<div style=\"display:flex;justify-content:space-between;font-size:0.8rem;padding:2px 0\">\
                    <span>{}</span><span style=\"color:var(--text-muted)\">{}</span></div>",
                type_badge(typ), count,
            ));
        }
    }

    html.push_str("</div></div>");
    html
}

pub(super) fn provenance_card(v: &serde_json::Value) -> String {
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

pub(super) fn documents_card(v: &serde_json::Value) -> String {
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
