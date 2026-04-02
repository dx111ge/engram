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
