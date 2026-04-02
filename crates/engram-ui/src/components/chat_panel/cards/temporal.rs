//! Temporal card renderers: timeline, current state, fact provenance, contradictions, situation-at.

use super::{confidence_bar, entity_link, html_escape};

pub(super) fn timeline_card(data: &serde_json::Value) -> String {
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

pub(super) fn current_state_card(data: &serde_json::Value) -> String {
    let entity = data.get("entity").and_then(|v| v.as_str()).unwrap_or("Entity");
    let as_of = data.get("as_of").and_then(|v| v.as_str()).unwrap_or("now");
    let current = data.get("current").and_then(|v| v.as_array());
    let expired = data.get("expired").and_then(|v| v.as_array());
    let cc = data.get("current_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let ec = data.get("expired_count").and_then(|v| v.as_u64()).unwrap_or(0);

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-clock-rotate-left\"></i> Current State: {} <span style=\"font-size:0.75rem;color:var(--text-muted)\">(as of {})</span></div>\
            <div class=\"chat-card-body\">",
        entity_link(entity), html_escape(as_of),
    );

    // Current relations (green)
    html.push_str(&format!("<div style=\"margin-bottom:8px\"><strong style=\"color:#66bb6a\"><i class=\"fa-solid fa-circle-check\"></i> Active ({cc})</strong></div>"));
    if let Some(edges) = current {
        for (i, e) in edges.iter().enumerate() {
            if i >= 15 { html.push_str("<div style=\"color:var(--text-muted);font-size:0.75rem\">...</div>"); break; }
            let from = e.get("from").and_then(|v| v.as_str()).unwrap_or("?");
            let to = e.get("to").and_then(|v| v.as_str()).unwrap_or("?");
            let rel = e.get("relationship").and_then(|v| v.as_str()).unwrap_or("?");
            let vf = e.get("valid_from").and_then(|v| v.as_str()).unwrap_or("");
            let other = if from == entity { to } else { from };
            html.push_str(&format!(
                "<div class=\"chat-entity-row\" style=\"font-size:0.8rem\">{} <span style=\"color:var(--accent);font-size:0.7rem\">[{}]</span> {} {}</div>",
                entity_link(other), html_escape(rel),
                if !vf.is_empty() { format!("<span style=\"color:var(--text-muted);font-size:0.7rem\">since {}</span>", vf) } else { String::new() },
                "",
            ));
        }
    }

    // Expired relations (grey)
    if ec > 0 {
        html.push_str(&format!("<div style=\"margin:8px 0 4px\"><strong style=\"color:#90a4ae\"><i class=\"fa-solid fa-circle-xmark\"></i> Expired ({ec})</strong></div>"));
        if let Some(edges) = expired {
            for (i, e) in edges.iter().enumerate() {
                if i >= 10 { html.push_str("<div style=\"color:var(--text-muted);font-size:0.75rem\">...</div>"); break; }
                let from = e.get("from").and_then(|v| v.as_str()).unwrap_or("?");
                let to = e.get("to").and_then(|v| v.as_str()).unwrap_or("?");
                let rel = e.get("relationship").and_then(|v| v.as_str()).unwrap_or("?");
                let vt = e.get("valid_to").and_then(|v| v.as_str()).unwrap_or("");
                let other = if from == entity { to } else { from };
                html.push_str(&format!(
                    "<div class=\"chat-entity-row\" style=\"font-size:0.8rem;opacity:0.6;text-decoration:line-through\">{} <span style=\"color:var(--text-muted);font-size:0.7rem\">[{}]</span> {}</div>",
                    entity_link(other), html_escape(rel),
                    if !vt.is_empty() { format!("<span style=\"font-size:0.7rem\">ended {}</span>", vt) } else { String::new() },
                ));
            }
        }
    }

    html.push_str("</div></div>");
    html
}

pub(super) fn fact_provenance_card(data: &serde_json::Value) -> String {
    if data.get("error").and_then(|v| v.as_str()).is_some() {
        let msg = data.get("error").and_then(|v| v.as_str()).unwrap_or("No data");
        return format!("<div class=\"chat-card chat-card-empty\"><i class=\"fa-solid fa-magnifying-glass-location\"></i> {}</div>", html_escape(msg));
    }

    let entity = data.get("entity").and_then(|v| v.as_str()).unwrap_or("Entity");
    let total = data.get("total_facts").and_then(|v| v.as_u64()).unwrap_or(0);
    let sources = data.get("sources").and_then(|v| v.as_array());
    let timeline = data.get("timeline").and_then(|v| v.as_array());
    let corroborations = data.get("corroborations").and_then(|v| v.as_array());

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-magnifying-glass-location\"></i> Provenance: {} <span style=\"font-size:0.75rem;color:var(--text-muted)\">({} facts)</span></div>\
            <div class=\"chat-card-body\">",
        entity_link(entity), total,
    );

    // Sources summary
    if let Some(srcs) = sources {
        html.push_str("<div style=\"margin-bottom:8px\"><strong><i class=\"fa-solid fa-database\"></i> Sources</strong></div>");
        for src in srcs {
            let name = src.get("source").and_then(|v| v.as_str()).unwrap_or("?");
            let count = src.get("facts_count").and_then(|v| v.as_u64()).unwrap_or(0);
            html.push_str(&format!(
                "<div style=\"display:flex;justify-content:space-between;font-size:0.8rem;padding:2px 0\">\
                    <span>{}</span><span style=\"color:var(--text-muted)\">{} facts</span></div>",
                html_escape(name), count,
            ));
        }
    }

    // Corroborations
    if let Some(corrs) = corroborations {
        if !corrs.is_empty() {
            html.push_str(&format!(
                "<div style=\"margin:8px 0 4px\"><strong style=\"color:#66bb6a\"><i class=\"fa-solid fa-check-double\"></i> Corroborated ({})</strong></div>",
                corrs.len(),
            ));
            for (i, c) in corrs.iter().enumerate() {
                if i >= 5 { break; }
                let claim = c.get("claim").and_then(|v| v.as_str()).unwrap_or("?");
                let count = c.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
                html.push_str(&format!(
                    "<div style=\"font-size:0.8rem;padding:2px 0\">{} <span style=\"color:#66bb6a\">({} sources)</span></div>",
                    html_escape(claim), count,
                ));
            }
        }
    }

    // Timeline (most recent facts)
    if let Some(events) = timeline {
        if !events.is_empty() {
            html.push_str("<div style=\"margin:8px 0 4px\"><strong><i class=\"fa-solid fa-clock\"></i> Information Timeline</strong></div>");
            for (i, evt) in events.iter().enumerate() {
                if i >= 10 { html.push_str("<div style=\"color:var(--text-muted);font-size:0.75rem\">...</div>"); break; }
                let fact = evt.get("fact").and_then(|v| v.as_str()).unwrap_or("?");
                let source = evt.get("source").and_then(|v| v.as_str()).unwrap_or("");
                let vf = evt.get("valid_from").and_then(|v| v.as_str()).unwrap_or("");
                let conf = evt.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);
                html.push_str(&format!(
                    "<div style=\"font-size:0.8rem;padding:2px 0;display:flex;gap:6px\">\
                        <span style=\"color:var(--text-muted);min-width:70px\">{}</span>\
                        <span style=\"flex:1\">{}</span>\
                        <span style=\"color:var(--text-muted);font-size:0.7rem\">{} {:.0}%</span>\
                    </div>",
                    if vf.is_empty() { "?" } else { vf }, html_escape(fact), html_escape(source), conf * 100.0,
                ));
            }
        }
    }

    html.push_str("</div></div>");
    html
}

pub(super) fn contradiction_card(data: &serde_json::Value) -> String {
    if data.get("error").and_then(|v| v.as_str()).is_some() {
        let msg = data.get("error").and_then(|v| v.as_str()).unwrap_or("No data");
        return format!("<div class=\"chat-card chat-card-empty\"><i class=\"fa-solid fa-scale-unbalanced\"></i> {}</div>", html_escape(msg));
    }

    let entity = data.get("entity").and_then(|v| v.as_str()).unwrap_or("Entity");
    let conflicts = data.get("conflicts").and_then(|v| v.as_array());
    let low_conf = data.get("low_confidence_facts").and_then(|v| v.as_array());
    let debunked = data.get("debunked_facts").and_then(|v| v.as_array());

    let successions = data.get("successions").and_then(|v| v.as_array());

    let conflict_count = data.get("conflict_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let succession_count = data.get("succession_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let has_issues = conflict_count > 0
        || succession_count > 0
        || low_conf.map_or(false, |a| !a.is_empty())
        || debunked.map_or(false, |a| !a.is_empty());

    if !has_issues {
        return format!(
            "<div class=\"chat-card\"><div class=\"chat-card-header\"><i class=\"fa-solid fa-circle-check\" style=\"color:#66bb6a\"></i> No Contradictions: {}</div>\
            <div class=\"chat-card-body\"><p style=\"color:#66bb6a;font-size:0.85rem\">No conflicting information found.</p></div></div>",
            entity_link(entity),
        );
    }

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-scale-unbalanced\"></i> Contradictions: {}</div>\
            <div class=\"chat-card-body\">",
        entity_link(entity),
    );

    // Conflicts
    if let Some(confs) = conflicts {
        for (i, c) in confs.iter().enumerate() {
            if i >= 10 { break; }
            let rel = c.get("relationship").and_then(|v| v.as_str()).unwrap_or("?");
            let a_target = c.get("claim_a").and_then(|v| v.get("target")).and_then(|v| v.as_str()).unwrap_or("?");
            let a_conf = c.get("claim_a").and_then(|v| v.get("confidence")).and_then(|v| v.as_f64()).unwrap_or(0.0);
            let b_target = c.get("claim_b").and_then(|v| v.get("target")).and_then(|v| v.as_str()).unwrap_or("?");
            let b_conf = c.get("claim_b").and_then(|v| v.get("confidence")).and_then(|v| v.as_f64()).unwrap_or(0.0);

            html.push_str(&format!(
                "<div style=\"border:1px solid rgba(239,83,80,0.3);border-radius:6px;padding:8px;margin-bottom:6px\">\
                    <div style=\"font-size:0.75rem;color:#ef5350;margin-bottom:4px\"><i class=\"fa-solid fa-triangle-exclamation\"></i> Conflict: {}</div>\
                    <div style=\"display:flex;justify-content:space-between;font-size:0.8rem\">\
                        <div>{} <span style=\"color:var(--text-muted)\">{:.0}%</span></div>\
                        <div style=\"color:var(--text-muted)\">vs</div>\
                        <div>{} <span style=\"color:var(--text-muted)\">{:.0}%</span></div>\
                    </div>\
                </div>",
                html_escape(rel), entity_link(a_target), a_conf * 100.0, entity_link(b_target), b_conf * 100.0,
            ));
        }
    }

    // Temporal successions (not conflicts -- different time periods)
    if let Some(succs) = successions {
        if !succs.is_empty() {
            html.push_str(&format!(
                "<div style=\"margin:8px 0 4px\"><strong style=\"color:#4fc3f7\"><i class=\"fa-solid fa-clock-rotate-left\"></i> Temporal Successions ({})</strong></div>",
                succs.len(),
            ));
            for (i, s) in succs.iter().enumerate() {
                if i >= 5 { break; }
                let rel = s.get("relationship").and_then(|v| v.as_str()).unwrap_or("?");
                let earlier_target = s.get("earlier").and_then(|v| v.get("target")).and_then(|v| v.as_str()).unwrap_or("?");
                let earlier_from = s.get("earlier").and_then(|v| v.get("valid_from")).and_then(|v| v.as_str()).unwrap_or("?");
                let earlier_to = s.get("earlier").and_then(|v| v.get("valid_to")).and_then(|v| v.as_str()).unwrap_or("?");
                let later_target = s.get("later").and_then(|v| v.get("target")).and_then(|v| v.as_str()).unwrap_or("?");
                let later_from = s.get("later").and_then(|v| v.get("valid_from")).and_then(|v| v.as_str()).unwrap_or("now");
                html.push_str(&format!(
                    "<div style=\"border:1px solid rgba(79,195,247,0.3);border-radius:6px;padding:8px;margin-bottom:6px;font-size:0.8rem\">\
                        <div style=\"color:#4fc3f7;font-size:0.75rem;margin-bottom:4px\"><i class=\"fa-solid fa-clock-rotate-left\"></i> {}: changed over time</div>\
                        <div>{} <span style=\"color:var(--text-muted)\">({} to {})</span></div>\
                        <div style=\"color:var(--text-muted);font-size:0.7rem;margin:2px 0\"><i class=\"fa-solid fa-arrow-down\"></i> then</div>\
                        <div>{} <span style=\"color:var(--text-muted)\">(from {})</span></div>\
                    </div>",
                    html_escape(rel),
                    entity_link(earlier_target), earlier_from, earlier_to,
                    entity_link(later_target), later_from,
                ));
            }
        }
    }

    // Low confidence
    if let Some(lc) = low_conf {
        if !lc.is_empty() {
            html.push_str(&format!(
                "<div style=\"margin:8px 0 4px\"><strong style=\"color:#ffa726\"><i class=\"fa-solid fa-circle-exclamation\"></i> Low Confidence ({})</strong></div>",
                lc.len(),
            ));
            for (i, f) in lc.iter().enumerate() {
                if i >= 5 { break; }
                let fact = f.get("fact").and_then(|v| v.as_str()).unwrap_or("?");
                let conf = f.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);
                html.push_str(&format!(
                    "<div style=\"font-size:0.8rem;padding:2px 0\">{} <span style=\"color:#ffa726\">{:.0}%</span></div>",
                    html_escape(fact), conf * 100.0,
                ));
            }
        }
    }

    html.push_str("</div></div>");
    html
}

pub(super) fn situation_at_card(data: &serde_json::Value) -> String {
    if data.get("error").and_then(|v| v.as_str()).is_some() {
        let msg = data.get("error").and_then(|v| v.as_str()).unwrap_or("No data");
        return format!("<div class=\"chat-card chat-card-empty\"><i class=\"fa-solid fa-camera-retro\"></i> {}</div>", html_escape(msg));
    }

    let entity = data.get("entity").and_then(|v| v.as_str()).unwrap_or("Entity");
    let date = data.get("date").and_then(|v| v.as_str()).unwrap_or("?");
    let edges = data.get("active_edges").and_then(|v| v.as_array());
    let edge_count = data.get("edge_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let total = data.get("total_edges").and_then(|v| v.as_u64()).unwrap_or(0);

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-camera-retro\"></i> Snapshot: {} on {}</div>\
            <div class=\"chat-card-body\">\
            <div style=\"font-size:0.8rem;color:var(--text-muted);margin-bottom:8px\">{} of {} total relations were active on this date.</div>",
        entity_link(entity), html_escape(date), edge_count, total,
    );

    if let Some(edges) = edges {
        for (i, e) in edges.iter().enumerate() {
            if i >= 20 { html.push_str("<div style=\"color:var(--text-muted);font-size:0.75rem\">...</div>"); break; }
            let from = e.get("from").and_then(|v| v.as_str()).unwrap_or("?");
            let to = e.get("to").and_then(|v| v.as_str()).unwrap_or("?");
            let rel = e.get("relationship").and_then(|v| v.as_str()).unwrap_or("?");
            let conf = e.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let other = if from == entity { to } else { from };
            html.push_str(&format!(
                "<div class=\"chat-entity-row\" style=\"font-size:0.8rem\">{} <span style=\"color:var(--accent);font-size:0.7rem\">[{}]</span> {} <span style=\"color:var(--text-muted);font-size:0.7rem\">{:.0}%</span></div>",
                entity_link(other), html_escape(rel),
                if from != entity { format!("<i class=\"fa-solid fa-arrow-left\" style=\"font-size:0.5rem;color:var(--text-muted)\"></i>") } else { String::new() },
                conf * 100.0,
            ));
        }
    }

    html.push_str("</div></div>");
    html
}
