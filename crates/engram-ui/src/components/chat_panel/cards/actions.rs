//! Action card renderers: rules, schedules.

use super::html_escape;

/// Card for rule_create result: shows the created rule's condition and action.
pub(super) fn rule_create_card(data: &serde_json::Value) -> String {
    let id = data.get("id").and_then(|v| v.as_str())
        .or_else(|| data.get("rule_id").and_then(|v| v.as_str()))
        .unwrap_or("?");
    let condition = data.get("condition").and_then(|v| v.as_str())
        .or_else(|| data.get("description").and_then(|v| v.as_str()))
        .unwrap_or("(no condition)");
    let action = data.get("action").and_then(|v| v.as_str()).unwrap_or("");

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-gavel\"></i> Rule Created</div>\
            <div class=\"chat-card-body\">\
                <div class=\"chat-prop-row\"><span class=\"chat-prop-key\">ID</span><span>{}</span></div>\
                <div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Condition</span><span>{}</span></div>",
        html_escape(id),
        html_escape(condition),
    );

    if !action.is_empty() {
        html.push_str(&format!(
            "<div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Action</span><span>{}</span></div>",
            html_escape(action),
        ));
    }

    html.push_str("</div></div>");
    html
}

/// Card for rule_list result: list of rules with condition/action pairs.
pub(super) fn rule_list_card(data: &serde_json::Value) -> String {
    let rules = data.get("rules").and_then(|v| v.as_array());

    let mut html = String::from(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-list-check\"></i> Action Rules</div>\
            <div class=\"chat-card-body\">",
    );

    match rules {
        Some(arr) if !arr.is_empty() => {
            html.push_str(&format!(
                "<div style=\"font-size:0.75rem;color:var(--text-muted);margin-bottom:0.4rem\">{} rule{}</div>",
                arr.len(),
                if arr.len() == 1 { "" } else { "s" },
            ));
            for rule in arr.iter().take(20) {
                let id = rule.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                let condition = rule.get("condition").and_then(|v| v.as_str())
                    .or_else(|| rule.get("description").and_then(|v| v.as_str()))
                    .unwrap_or("?");
                let action = rule.get("action").and_then(|v| v.as_str()).unwrap_or("");
                let enabled = rule.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                let status_icon = if enabled {
                    "<i class=\"fa-solid fa-circle\" style=\"color:var(--success);font-size:0.5rem;vertical-align:middle\"></i>"
                } else {
                    "<i class=\"fa-solid fa-circle\" style=\"color:var(--text-muted);font-size:0.5rem;vertical-align:middle\"></i>"
                };
                html.push_str(&format!(
                    "<div class=\"chat-entity-row\" style=\"flex-direction:column;align-items:flex-start;gap:2px\">\
                        <div style=\"display:flex;gap:6px;align-items:center\">{} <span style=\"font-weight:600\">{}</span> <span class=\"chat-text-muted\" style=\"font-size:0.7rem\">{}</span></div>",
                    status_icon, html_escape(condition), html_escape(id),
                ));
                if !action.is_empty() {
                    html.push_str(&format!(
                        "<div style=\"font-size:0.75rem;color:var(--text-muted);padding-left:1rem\"><i class=\"fa-solid fa-arrow-right\" style=\"font-size:0.6rem\"></i> {}</div>",
                        html_escape(action),
                    ));
                }
                html.push_str("</div>");
            }
        }
        _ => {
            html.push_str("<div class=\"chat-empty\"><i class=\"fa-solid fa-gavel\"></i> No rules defined yet. Use <code>create rule</code> to add one.</div>");
        }
    }

    html.push_str("</div></div>");
    html
}

/// Card for rule_fire (dry-run) result: list of rules that would trigger.
pub(super) fn rule_fire_card(data: &serde_json::Value) -> String {
    let results = data.get("results").and_then(|v| v.as_array())
        .or_else(|| data.get("triggered").and_then(|v| v.as_array()))
        .or_else(|| data.get("actions").and_then(|v| v.as_array()));

    let mut html = String::from(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-bolt\"></i> Rules Dry Run</div>\
            <div class=\"chat-card-body\">",
    );

    match results {
        Some(arr) if !arr.is_empty() => {
            html.push_str(&format!(
                "<div style=\"font-size:0.75rem;color:var(--text-muted);margin-bottom:0.4rem\">{} rule{} would trigger</div>",
                arr.len(),
                if arr.len() == 1 { "" } else { "s" },
            ));
            for result in arr.iter().take(20) {
                let rule_id = result.get("rule_id").and_then(|v| v.as_str())
                    .or_else(|| result.get("id").and_then(|v| v.as_str()))
                    .unwrap_or("?");
                let description = result.get("description").and_then(|v| v.as_str())
                    .or_else(|| result.get("condition").and_then(|v| v.as_str()))
                    .unwrap_or("?");
                let action = result.get("action").and_then(|v| v.as_str())
                    .or_else(|| result.get("would_create").and_then(|v| v.as_str()))
                    .unwrap_or("");
                html.push_str(&format!(
                    "<div class=\"chat-entity-row\" style=\"flex-direction:column;align-items:flex-start;gap:2px\">\
                        <div style=\"display:flex;gap:6px;align-items:center\">\
                            <i class=\"fa-solid fa-bolt\" style=\"color:var(--warning);font-size:0.7rem\"></i>\
                            <span style=\"font-weight:600\">{}</span>\
                            <span class=\"chat-text-muted\" style=\"font-size:0.7rem\">{}</span>\
                        </div>",
                    html_escape(description), html_escape(rule_id),
                ));
                if !action.is_empty() {
                    html.push_str(&format!(
                        "<div style=\"font-size:0.75rem;color:var(--accent);padding-left:1rem\"><i class=\"fa-solid fa-arrow-right\" style=\"font-size:0.6rem\"></i> Would create: {}</div>",
                        html_escape(action),
                    ));
                }
                html.push_str("</div>");
            }
        }
        _ => {
            // Check if data itself is an array (some APIs return bare arrays)
            if let Some(arr) = data.as_array() {
                if arr.is_empty() {
                    html.push_str("<div class=\"chat-empty\"><i class=\"fa-solid fa-check-circle\"></i> No rules would trigger right now.</div>");
                } else {
                    html.push_str(&format!(
                        "<div style=\"font-size:0.75rem;color:var(--text-muted);margin-bottom:0.4rem\">{} result{}</div>",
                        arr.len(),
                        if arr.len() == 1 { "" } else { "s" },
                    ));
                    for item in arr.iter().take(20) {
                        html.push_str(&format!(
                            "<div class=\"chat-entity-row\"><pre style=\"font-size:0.72rem;white-space:pre-wrap;margin:0\">{}</pre></div>",
                            html_escape(&serde_json::to_string_pretty(item).unwrap_or_default()),
                        ));
                    }
                }
            } else {
                html.push_str("<div class=\"chat-empty\"><i class=\"fa-solid fa-check-circle\"></i> No rules would trigger right now.</div>");
            }
        }
    }

    html.push_str("</div></div>");
    html
}

/// Card for schedule result: shows created/listed schedules.
pub(super) fn schedule_card(data: &serde_json::Value) -> String {
    let mut html = String::from(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"fa-solid fa-calendar-check\"></i> Schedule</div>\
            <div class=\"chat-card-body\">",
    );

    // Single schedule creation result
    let name = data.get("name").and_then(|v| v.as_str());
    let interval = data.get("interval").and_then(|v| v.as_str());

    if let Some(n) = name {
        html.push_str(&format!(
            "<div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Name</span><span>{}</span></div>",
            html_escape(n),
        ));
        if let Some(iv) = interval {
            html.push_str(&format!(
                "<div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Interval</span><span>{}</span></div>",
                html_escape(iv),
            ));
        }
        if let Some(source) = data.get("source").and_then(|v| v.as_str()) {
            if !source.is_empty() {
                html.push_str(&format!(
                    "<div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Source</span><span>{}</span></div>",
                    html_escape(source),
                ));
            }
        }
        let status = data.get("status").and_then(|v| v.as_str()).unwrap_or("awaiting_executor");
        let status_color = match status {
            "active" | "running" => "var(--success)",
            "paused" | "awaiting_executor" => "var(--warning)",
            _ => "var(--text-muted)",
        };
        html.push_str(&format!(
            "<div class=\"chat-prop-row\"><span class=\"chat-prop-key\">Status</span>\
                <span style=\"color:{}\">{}</span></div>",
            status_color, html_escape(status),
        ));
    }

    // List of schedules
    if let Some(schedules) = data.get("schedules").and_then(|v| v.as_array()) {
        html.push_str(&format!(
            "<div style=\"font-size:0.75rem;color:var(--text-muted);margin-bottom:0.4rem\">{} schedule{}</div>",
            schedules.len(),
            if schedules.len() == 1 { "" } else { "s" },
        ));
        for sched in schedules.iter().take(20) {
            let sn = sched.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let si = sched.get("interval").and_then(|v| v.as_str()).unwrap_or("?");
            let ss = sched.get("status").and_then(|v| v.as_str()).unwrap_or("?");
            let status_color = match ss {
                "active" | "running" => "var(--success)",
                "paused" | "awaiting_executor" => "var(--warning)",
                _ => "var(--text-muted)",
            };
            html.push_str(&format!(
                "<div class=\"chat-entity-row\">\
                    <span style=\"font-weight:600\">{}</span>\
                    <span class=\"chat-text-muted\">{}</span>\
                    <span style=\"color:{};font-size:0.75rem\">{}</span>\
                </div>",
                html_escape(sn), html_escape(si), status_color, html_escape(ss),
            ));
        }
    }

    // If the response is just a message
    if name.is_none() && data.get("schedules").is_none() {
        if let Some(msg) = data.get("message").and_then(|v| v.as_str()) {
            html.push_str(&format!("<div>{}</div>", html_escape(msg)));
        } else if let Some(status) = data.get("status").and_then(|v| v.as_str()) {
            html.push_str(&format!("<div>{}</div>", html_escape(status)));
        }
    }

    html.push_str("</div></div>");
    html
}
