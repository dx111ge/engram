//! Assessment card renderers: assessment, what-if, influence.

use super::{confidence_bar, entity_link, html_escape};

pub(super) fn assessment_card(data: &serde_json::Value) -> String {
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

pub(super) fn whatif_card(data: &serde_json::Value) -> String {
    let entity = data.get("entity").and_then(|v| v.as_str()).unwrap_or("?");
    let current = data.get("current_confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let simulated = data.get("simulated_confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let conf_delta = data.get("confidence_delta").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let affected = data.get("affected").and_then(|v| v.as_array());
    let affected_count = data.get("affected_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let max_depth = data.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(2);

    let delta_color = if conf_delta > 0.0 { "#66bb6a" } else if conf_delta < 0.0 { "#ef5350" } else { "var(--text-muted)" };

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-flask\"></i> What-If: {} <span style=\"font-size:0.75rem;color:var(--text-muted)\">({} hops, {} affected)</span></div>\
            <div class=\"chat-card-body\">\
                <div style=\"display:flex;gap:16px;margin-bottom:8px;font-size:0.85rem\">\
                    <div>Current: {}</div>\
                    <div><i class=\"fa-solid fa-arrow-right\" style=\"font-size:0.6rem\"></i></div>\
                    <div>Simulated: {}</div>\
                    <div style=\"color:{}\">{:+.0}%</div>\
                </div>",
        entity_link(entity), max_depth, affected_count,
        confidence_bar(current), confidence_bar(simulated),
        delta_color, conf_delta * 100.0,
    );

    if let Some(affected) = affected {
        // Group by hop
        let mut hop1: Vec<&serde_json::Value> = Vec::new();
        let mut hop2: Vec<&serde_json::Value> = Vec::new();
        for a in affected {
            match a.get("hop").and_then(|v| v.as_u64()).unwrap_or(1) {
                1 => hop1.push(a),
                _ => hop2.push(a),
            }
        }

        if !hop1.is_empty() {
            html.push_str(&format!("<div style=\"margin:6px 0 2px\"><strong style=\"color:var(--accent);font-size:0.8rem\">Hop 1 ({} entities)</strong></div>", hop1.len()));
            for (i, a) in hop1.iter().enumerate() {
                if i >= 10 { html.push_str("<div style=\"color:var(--text-muted);font-size:0.75rem\">...</div>"); break; }
                let label = a.get("label").and_then(|v| v.as_str()).unwrap_or("?");
                let delta = a.get("delta").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let rel = a.get("relationship").and_then(|v| v.as_str()).unwrap_or("");
                let dc = if delta > 0.0 { "#66bb6a" } else if delta < 0.0 { "#ef5350" } else { "var(--text-muted)" };
                html.push_str(&format!(
                    "<div style=\"font-size:0.8rem;padding:1px 0;display:flex;justify-content:space-between\">\
                        <span>{} <span style=\"color:var(--text-muted);font-size:0.7rem\">[{}]</span></span>\
                        <span style=\"color:{}\">{:+.1}%</span>\
                    </div>",
                    entity_link(label), html_escape(rel), dc, delta * 100.0,
                ));
            }
        }

        if !hop2.is_empty() {
            html.push_str(&format!("<div style=\"margin:6px 0 2px\"><strong style=\"color:var(--text-muted);font-size:0.8rem\">Hop 2+ ({} entities)</strong></div>", hop2.len()));
            for (i, a) in hop2.iter().enumerate() {
                if i >= 8 { html.push_str("<div style=\"color:var(--text-muted);font-size:0.75rem\">...</div>"); break; }
                let label = a.get("label").and_then(|v| v.as_str()).unwrap_or("?");
                let delta = a.get("delta").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let dc = if delta > 0.0 { "#66bb6a" } else if delta < 0.0 { "#ef5350" } else { "var(--text-muted)" };
                html.push_str(&format!(
                    "<div style=\"font-size:0.75rem;padding:1px 0;display:flex;justify-content:space-between;opacity:0.8\">\
                        <span>{}</span><span style=\"color:{}\">{:+.1}%</span>\
                    </div>",
                    entity_link(label), dc, delta * 100.0,
                ));
            }
        }
    }

    html.push_str("</div></div>");
    html
}

pub(super) fn influence_multi_card(data: &serde_json::Value) -> String {
    let from = data.get("from").and_then(|v| v.as_str()).unwrap_or("?");
    let to = data.get("to").and_then(|v| v.as_str()).unwrap_or("?");
    let found = data.get("found").and_then(|v| v.as_bool()).unwrap_or(false);
    let path_count = data.get("path_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let paths = data.get("paths").and_then(|v| v.as_array());

    if !found {
        return format!(
            "<div class=\"chat-card chat-card-empty\"><i class=\"fa-solid fa-diagram-successor\"></i> No influence paths found between {} and {}</div>",
            entity_link(from), entity_link(to),
        );
    }

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-diagram-successor\"></i> Influence: {} <i class=\"fa-solid fa-arrow-right\" style=\"font-size:0.6rem\"></i> {} <span style=\"font-size:0.75rem;color:var(--text-muted)\">({} path{})</span></div>\
            <div class=\"chat-card-body\">",
        entity_link(from), entity_link(to), path_count, if path_count != 1 { "s" } else { "" },
    );

    if let Some(paths) = paths {
        for (i, path) in paths.iter().enumerate() {
            if i >= 5 { break; }
            let hops = path.get("hops").and_then(|v| v.as_u64()).unwrap_or(0);
            let steps = path.get("steps").and_then(|v| v.as_array());

            html.push_str(&format!(
                "<div style=\"margin:4px 0;padding:6px;border:1px solid rgba(79,195,247,0.2);border-radius:4px\">\
                    <div style=\"font-size:0.7rem;color:var(--text-muted);margin-bottom:4px\">Path {} ({} hops)</div>\
                    <div style=\"display:flex;flex-wrap:wrap;align-items:center;gap:4px;font-size:0.8rem\">",
                i + 1, hops,
            ));

            if let Some(steps) = steps {
                for (j, step) in steps.iter().enumerate() {
                    let entity = step.get("entity").and_then(|v| v.as_str()).unwrap_or("?");
                    let rel = step.get("relationship").and_then(|v| v.as_str()).unwrap_or("");
                    if j > 0 && !rel.is_empty() {
                        html.push_str(&format!(
                            "<span style=\"color:var(--accent);font-size:0.65rem\">[{}]</span> <i class=\"fa-solid fa-arrow-right\" style=\"font-size:0.5rem;color:var(--text-muted)\"></i> ",
                            html_escape(rel),
                        ));
                    }
                    html.push_str(&entity_link(entity));
                }
            }

            html.push_str("</div></div>");
        }
    }

    html.push_str("</div></div>");
    html
}

pub(super) fn black_areas_card(data: &serde_json::Value) -> String {
    let gaps = data.get("gaps").or_else(|| data.as_array().map(|_| data)).and_then(|v| v.as_array());

    if gaps.is_none() || gaps.unwrap().is_empty() {
        return "<div class=\"chat-card\"><div class=\"chat-card-header\"><i class=\"fa-solid fa-circle-check\" style=\"color:#66bb6a\"></i> No Knowledge Gaps</div>\
            <div class=\"chat-card-body\" style=\"font-size:0.85rem;color:#66bb6a\">Your knowledge graph has no detected blind spots.</div></div>".to_string();
    }
    let gaps = gaps.unwrap();

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-binoculars\"></i> Knowledge Gaps ({} detected)</div>\
            <div class=\"chat-card-body\">",
        gaps.len(),
    );

    for (i, gap) in gaps.iter().enumerate() {
        if i >= 15 { html.push_str("<div style=\"color:var(--text-muted);font-size:0.75rem\">...</div>"); break; }
        let kind = gap.get("kind").and_then(|v| v.as_str()).unwrap_or("unknown");
        let severity = gap.get("severity").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let entities = gap.get("entities").and_then(|v| v.as_array());
        let domain = gap.get("domain").and_then(|v| v.as_str());
        let suggestions = gap.get("suggested_queries").and_then(|v| v.as_array());

        let (icon, color) = match kind {
            "frontier_node" => ("fa-solid fa-circle-dot", "#ffa726"),
            "structural_hole" => ("fa-solid fa-link-slash", "#ef5350"),
            "asymmetric_cluster" => ("fa-solid fa-scale-unbalanced", "#ce93d8"),
            "temporal_gap" => ("fa-solid fa-clock", "#4fc3f7"),
            "confidence_desert" => ("fa-solid fa-temperature-low", "#90a4ae"),
            "coordinated_cluster" => ("fa-solid fa-users-viewfinder", "#ff7043"),
            _ => ("fa-solid fa-question", "var(--text-muted)"),
        };

        let severity_pct = (severity * 100.0).round() as u32;
        let kind_label = kind.replace('_', " ");

        html.push_str(&format!(
            "<div style=\"border-left:3px solid {};padding:6px 10px;margin-bottom:6px\">\
                <div style=\"display:flex;justify-content:space-between;align-items:center\">\
                    <span style=\"font-size:0.8rem;font-weight:600\"><i class=\"{}\" style=\"color:{}\"></i> {}</span>\
                    <span style=\"font-size:0.7rem;color:{}\">{severity_pct}% severity</span>\
                </div>",
            color, icon, color, kind_label, color,
        ));

        if let Some(domain) = domain {
            html.push_str(&format!("<div style=\"font-size:0.7rem;color:var(--text-muted)\">Domain: {}</div>", html_escape(domain)));
        }

        if let Some(entities) = entities {
            let labels: Vec<String> = entities.iter().take(5)
                .filter_map(|e| e.as_str())
                .map(|l| entity_link(l))
                .collect();
            html.push_str(&format!("<div style=\"font-size:0.8rem;margin-top:2px\">{}</div>", labels.join(", ")));
        }

        if let Some(suggestions) = suggestions {
            for s in suggestions.iter().take(2) {
                if let Some(q) = s.as_str() {
                    html.push_str(&format!(
                        "<div style=\"font-size:0.7rem;color:var(--accent);margin-top:2px\"><i class=\"fa-solid fa-lightbulb\" style=\"font-size:0.6rem\"></i> {}</div>",
                        html_escape(q),
                    ));
                }
            }
        }

        html.push_str("</div>");
    }

    html.push_str("</div></div>");
    html
}

pub(super) fn assess_create_card(data: &serde_json::Value) -> String {
    let label = data.get("label").and_then(|v| v.as_str()).unwrap_or("?");
    let probability = data.get("probability").and_then(|v| v.as_f64()).unwrap_or(0.5);
    let status = data.get("status").and_then(|v| v.as_str()).unwrap_or("active");

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-scale-balanced\"></i> Assessment Created</div>\
            <div class=\"chat-card-body\">\
                <div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Label</span><span>{}</span></div>\
                <div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Probability</span>{}</div>\
                <div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Status</span><span>{}</span></div>",
        html_escape(label), confidence_bar(probability), html_escape(status),
    );

    if let Some(criteria) = data.get("success_criteria").and_then(|v| v.as_array()) {
        if !criteria.is_empty() {
            html.push_str("<div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Success Criteria</span></div>");
            for c in criteria {
                if let Some(s) = c.as_str() {
                    html.push_str(&format!("<div class=\"chat-evidence-row\"><i class=\"fa-solid fa-bullseye\" style=\"color:var(--accent)\"></i> {}</div>", html_escape(s)));
                }
            }
        }
    }

    html.push_str("</div></div>");
    html
}

pub(super) fn assess_list_card(data: &serde_json::Value) -> String {
    let mut html = String::from(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-list-check\"></i> Assessments</div>\
            <div class=\"chat-card-body\">"
    );

    if let Some(assessments) = data.get("assessments").and_then(|v| v.as_array()) {
        if assessments.is_empty() {
            html.push_str("<div class=\"chat-text-muted\">No assessments found</div>");
        } else {
            for a in assessments.iter().take(15) {
                let title = a.get("title").or_else(|| a.get("label")).and_then(|v| v.as_str()).unwrap_or("?");
                let prob = a.get("current_probability").and_then(|v| v.as_f64()).unwrap_or(0.5);
                let shift = a.get("last_shift").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let status = a.get("status").and_then(|v| v.as_str()).unwrap_or("active");
                let ev_count = a.get("evidence_count").and_then(|v| v.as_u64()).unwrap_or(0);

                let trend_icon = if shift > 0.01 {
                    "<i class=\"fa-solid fa-arrow-trend-up\" style=\"color:var(--success);font-size:0.7rem\"></i>"
                } else if shift < -0.01 {
                    "<i class=\"fa-solid fa-arrow-trend-down\" style=\"color:var(--danger);font-size:0.7rem\"></i>"
                } else {
                    "<i class=\"fa-solid fa-minus\" style=\"color:var(--text-muted);font-size:0.7rem\"></i>"
                };

                let status_badge = match status {
                    "resolved" => "<span style=\"color:var(--success);font-size:0.68rem\">resolved</span>",
                    "rejected" => "<span style=\"color:var(--danger);font-size:0.68rem\">rejected</span>",
                    _ => "<span style=\"color:var(--text-muted);font-size:0.68rem\">active</span>",
                };

                html.push_str(&format!(
                    "<div class=\"chat-entity-row\" style=\"display:flex;align-items:center;gap:0.4rem\">\
                        <span style=\"flex:1\">{}</span>\
                        {} {}\
                        <span class=\"chat-text-muted\" style=\"font-size:0.68rem\">{} ev</span>\
                        {}\
                    </div>",
                    html_escape(title), confidence_bar(prob), trend_icon, ev_count, status_badge,
                ));
            }
        }
    } else {
        html.push_str("<div class=\"chat-text-muted\">No assessments data</div>");
    }

    html.push_str("</div></div>");
    html
}

pub(super) fn assess_detail_card(data: &serde_json::Value) -> String {
    let title = data.get("title").or_else(|| data.get("label")).and_then(|v| v.as_str()).unwrap_or("Assessment");
    let probability = data.get("current_probability").and_then(|v| v.as_f64()).unwrap_or(0.5);
    let category = data.get("category").and_then(|v| v.as_str());
    let description = data.get("description").and_then(|v| v.as_str());
    let status = data.get("status").and_then(|v| v.as_str()).unwrap_or("active");

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-magnifying-glass-chart\"></i> {}</div>\
            <div class=\"chat-card-body\">",
        html_escape(title),
    );

    html.push_str(&format!("<div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Probability</span>{}</div>", confidence_bar(probability)));
    html.push_str(&format!("<div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Status</span><span>{}</span></div>", html_escape(status)));

    if let Some(cat) = category {
        if !cat.is_empty() {
            html.push_str(&format!("<div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Category</span><span>{}</span></div>", html_escape(cat)));
        }
    }
    if let Some(desc) = description {
        if !desc.is_empty() {
            html.push_str(&format!("<div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Description</span><span style=\"font-size:0.8rem\">{}</span></div>", html_escape(desc)));
        }
    }

    // Evidence for
    if let Some(ev_for) = data.get("evidence_for").and_then(|v| v.as_array()) {
        if !ev_for.is_empty() {
            html.push_str("<div style=\"margin-top:0.4rem;font-size:0.72rem;color:var(--success);font-weight:600\"><i class=\"fa-solid fa-arrow-up\"></i> Supporting Evidence</div>");
            for ev in ev_for.iter().take(8) {
                let label = ev.get("node_label").and_then(|v| v.as_str()).unwrap_or("?");
                let conf = ev.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.5);
                html.push_str(&format!(
                    "<div class=\"chat-evidence-row\"><i class=\"fa-solid fa-check\" style=\"color:var(--success)\"></i> {} <span class=\"chat-text-muted\">({:.0}%)</span></div>",
                    entity_link(label), conf * 100.0,
                ));
            }
        }
    }

    // Evidence against
    if let Some(ev_against) = data.get("evidence_against").and_then(|v| v.as_array()) {
        if !ev_against.is_empty() {
            html.push_str("<div style=\"margin-top:0.4rem;font-size:0.72rem;color:var(--danger);font-weight:600\"><i class=\"fa-solid fa-arrow-down\"></i> Contradicting Evidence</div>");
            for ev in ev_against.iter().take(8) {
                let label = ev.get("node_label").and_then(|v| v.as_str()).unwrap_or("?");
                let conf = ev.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.5);
                html.push_str(&format!(
                    "<div class=\"chat-evidence-row\"><i class=\"fa-solid fa-xmark\" style=\"color:var(--danger)\"></i> {} <span class=\"chat-text-muted\">({:.0}%)</span></div>",
                    entity_link(label), conf * 100.0,
                ));
            }
        }
    }

    // History
    if let Some(history) = data.get("history").and_then(|v| v.as_array()) {
        if history.len() > 1 {
            html.push_str("<div style=\"margin-top:0.4rem;font-size:0.72rem;color:var(--text-muted);font-weight:600\"><i class=\"fa-solid fa-clock-rotate-left\"></i> History</div>");
            for point in history.iter().rev().take(5) {
                let prob = point.get("probability").and_then(|v| v.as_f64()).unwrap_or(0.5);
                let shift = point.get("shift").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let reason = point.get("reason").and_then(|v| v.as_str()).unwrap_or("");
                let shift_str = if shift.abs() > 0.001 { format!("{:+.0}%", shift * 100.0) } else { String::new() };
                let shift_color = if shift > 0.0 { "var(--success)" } else if shift < 0.0 { "var(--danger)" } else { "var(--text-muted)" };
                html.push_str(&format!(
                    "<div class=\"chat-evidence-row\"><span style=\"font-size:0.72rem\">{:.0}%</span> <span style=\"color:{};font-size:0.68rem\">{}</span> <span class=\"chat-text-muted\" style=\"font-size:0.68rem\">{}</span></div>",
                    prob * 100.0, shift_color, shift_str, html_escape(reason),
                ));
            }
        }
    }

    // Watches
    if let Some(watches) = data.get("watches").and_then(|v| v.as_array()) {
        if !watches.is_empty() {
            html.push_str("<div style=\"margin-top:0.4rem;font-size:0.72rem;color:var(--text-muted);font-weight:600\"><i class=\"fa-solid fa-bell\"></i> Watching</div>");
            let watch_labels: Vec<String> = watches.iter()
                .filter_map(|w| w.as_str().map(|s| entity_link(s)))
                .take(10)
                .collect();
            html.push_str(&format!("<div class=\"chat-evidence-row\">{}</div>", watch_labels.join(", ")));
        }
    }

    html.push_str("</div></div>");
    html
}

pub(super) fn assess_evidence_card(data: &serde_json::Value) -> String {
    let direction = data.get("direction").and_then(|v| v.as_str()).unwrap_or("supports");
    let new_prob = data.get("new_probability").and_then(|v| v.as_f64()).unwrap_or(0.5);
    let shift = data.get("shift").and_then(|v| v.as_f64()).unwrap_or(0.0);

    let dir_icon = if direction == "supports" {
        "<i class=\"fa-solid fa-arrow-up\" style=\"color:var(--success)\"></i>"
    } else {
        "<i class=\"fa-solid fa-arrow-down\" style=\"color:var(--danger)\"></i>"
    };

    let shift_str = format!("{:+.1}%", shift * 100.0);
    let shift_color = if shift > 0.0 { "var(--success)" } else { "var(--danger)" };

    format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-plus-circle\"></i> Evidence Added</div>\
            <div class=\"chat-card-body\">\
                <div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Direction</span><span>{} {}</span></div>\
                <div class=\"chat-prop-row\"><span class=\"chat-prop-key\">New Probability</span>{}</div>\
                <div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Shift</span><span style=\"color:{};font-weight:600\">{}</span></div>\
            </div>\
        </div>",
        dir_icon, html_escape(direction), confidence_bar(new_prob), shift_color, shift_str,
    )
}

pub(super) fn assess_evaluate_card(data: &serde_json::Value) -> String {
    let label = data.get("label").and_then(|v| v.as_str()).unwrap_or("?");
    let old_prob = data.get("old_probability").and_then(|v| v.as_f64()).unwrap_or(0.5);
    let new_prob = data.get("new_probability").and_then(|v| v.as_f64()).unwrap_or(0.5);
    let shift = data.get("shift").and_then(|v| v.as_f64()).unwrap_or(0.0);

    let shift_str = format!("{:+.1}%", shift * 100.0);
    let shift_color = if shift > 0.0 { "var(--success)" } else if shift < 0.0 { "var(--danger)" } else { "var(--text-muted)" };

    format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-calculator\"></i> Evaluation: {}</div>\
            <div class=\"chat-card-body\">\
                <div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Previous</span>{}</div>\
                <div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Current</span>{}</div>\
                <div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Shift</span><span style=\"color:{};font-weight:600\">{}</span></div>\
            </div>\
        </div>",
        html_escape(label), confidence_bar(old_prob), confidence_bar(new_prob), shift_color, shift_str,
    )
}

pub(super) fn assess_compare_card(data: &serde_json::Value) -> String {
    let a = data.get("assessment_a").unwrap_or(&serde_json::Value::Null);
    let b = data.get("assessment_b").unwrap_or(&serde_json::Value::Null);
    let shared = data.get("shared_watches").and_then(|v| v.as_array());

    let title_a = a.get("title").and_then(|v| v.as_str()).unwrap_or("A");
    let title_b = b.get("title").and_then(|v| v.as_str()).unwrap_or("B");
    let prob_a = a.get("current_probability").and_then(|v| v.as_f64()).unwrap_or(0.5);
    let prob_b = b.get("current_probability").and_then(|v| v.as_f64()).unwrap_or(0.5);
    let status_a = a.get("status").and_then(|v| v.as_str()).unwrap_or("active");
    let status_b = b.get("status").and_then(|v| v.as_str()).unwrap_or("active");
    let ev_for_a = a.get("evidence_for_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let ev_against_a = a.get("evidence_against_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let ev_for_b = b.get("evidence_for_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let ev_against_b = b.get("evidence_against_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let shift_a = a.get("last_shift").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let shift_b = b.get("last_shift").and_then(|v| v.as_f64()).unwrap_or(0.0);

    let trend = |s: f64| -> &str {
        if s > 0.01 { "<i class=\"fa-solid fa-arrow-trend-up\" style=\"color:var(--success)\"></i>" }
        else if s < -0.01 { "<i class=\"fa-solid fa-arrow-trend-down\" style=\"color:var(--danger)\"></i>" }
        else { "<i class=\"fa-solid fa-minus\" style=\"color:var(--text-muted)\"></i>" }
    };

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-code-compare\"></i> Compare Assessments</div>\
            <div class=\"chat-card-body\">\
                <div class=\"chat-compare-grid\" style=\"display:grid;grid-template-columns:1fr 1fr;gap:0.5rem\">\
                    <div style=\"font-weight:600;font-size:0.8rem\">{}</div>\
                    <div style=\"font-weight:600;font-size:0.8rem\">{}</div>\
                    <div>{}</div>\
                    <div>{}</div>\
                    <div style=\"font-size:0.72rem\">Status: {}</div>\
                    <div style=\"font-size:0.72rem\">Status: {}</div>\
                    <div style=\"font-size:0.72rem\"><i class=\"fa-solid fa-check\" style=\"color:var(--success)\"></i> {} for / <i class=\"fa-solid fa-xmark\" style=\"color:var(--danger)\"></i> {} against {}</div>\
                    <div style=\"font-size:0.72rem\"><i class=\"fa-solid fa-check\" style=\"color:var(--success)\"></i> {} for / <i class=\"fa-solid fa-xmark\" style=\"color:var(--danger)\"></i> {} against {}</div>\
                </div>",
        html_escape(title_a), html_escape(title_b),
        confidence_bar(prob_a), confidence_bar(prob_b),
        html_escape(status_a), html_escape(status_b),
        ev_for_a, ev_against_a, trend(shift_a),
        ev_for_b, ev_against_b, trend(shift_b),
    );

    if let Some(shared) = shared {
        if !shared.is_empty() {
            let labels: Vec<String> = shared.iter()
                .filter_map(|w| w.as_str().map(|s| entity_link(s)))
                .take(8)
                .collect();
            html.push_str(&format!(
                "<div style=\"margin-top:0.4rem;font-size:0.72rem;color:var(--text-muted)\"><i class=\"fa-solid fa-link\"></i> Shared watches: {}</div>",
                labels.join(", "),
            ));
        }
    }

    html.push_str("</div></div>");
    html
}

pub(super) fn influence_card(data: &serde_json::Value) -> String {
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
