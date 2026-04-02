//! Tool parameter cards: pure HTML forms that dispatch `engram-run-tool` events.
//! No XHR, no blocking -- all API calls happen async in the WASM handler.

use super::markdown::html_escape;

/// A field in a tool parameter card.
struct Field {
    id: &'static str,
    label: &'static str,
    placeholder: &'static str,
    field_type: FieldType,
    required: bool,
}

enum FieldType {
    Text,
    Number { default: &'static str, min: &'static str, max: &'static str, step: &'static str },
    Select(&'static [(&'static str, &'static str)]),
}

/// A tool card definition.
struct ToolCard {
    tool: &'static str,
    title: &'static str,
    icon: &'static str,
    is_write: bool,
    fields: &'static [Field],
    button_label: &'static str,
    /// If true, shows a confirm() dialog before dispatching
    confirm: bool,
}

const IS: &str = "width:100%;padding:0.3rem 0.5rem;font-size:0.78rem;\
    background:var(--bg-input, #1e2028);color:var(--text);border:1px solid var(--border);\
    border-radius:4px;outline:none;font-family:inherit;box-sizing:border-box;";
const LS: &str = "font-size:0.68rem;color:var(--text-muted);text-transform:uppercase;";
const BS: &str = "padding:0.35rem 0.75rem;font-size:0.78rem;font-family:inherit;\
    background:var(--accent, #4a9eff);color:#fff;border:none;border-radius:4px;\
    cursor:pointer;display:flex;align-items:center;gap:0.3rem;justify-content:center;width:100%;";

// ── Card definitions ──

static CARDS: &[ToolCard] = &[
    // Knowledge: read tools
    ToolCard { tool: "query", title: "Query Graph", icon: "fa-solid fa-diagram-project", is_write: false, confirm: false,
        button_label: "Run Query",
        fields: &[
            Field { id: "tc-query-entity", label: "Entity", placeholder: "Entity name...", field_type: FieldType::Text, required: true },
            Field { id: "tc-query-depth", label: "Depth", placeholder: "", field_type: FieldType::Number { default: "1", min: "1", max: "5", step: "1" }, required: false },
            Field { id: "tc-query-dir", label: "Direction", placeholder: "", field_type: FieldType::Select(&[("both", "Both"), ("out", "Out"), ("in", "In")]), required: false },
        ],
    },
    ToolCard { tool: "search", title: "Search Entities", icon: "fa-solid fa-magnifying-glass", is_write: false, confirm: false,
        button_label: "Search",
        fields: &[
            Field { id: "tc-search-q", label: "Search query", placeholder: "Keywords...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "explain", title: "Explain Entity", icon: "fa-solid fa-circle-info", is_write: false, confirm: false,
        button_label: "Explain",
        fields: &[
            Field { id: "tc-explain-e", label: "Entity", placeholder: "Entity name...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "similar", title: "Similar Entities", icon: "fa-solid fa-arrows-spin", is_write: false, confirm: false,
        button_label: "Find Similar",
        fields: &[
            Field { id: "tc-similar-t", label: "Entity name", placeholder: "Find similar to...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "prove", title: "Find Evidence", icon: "fa-solid fa-scale-balanced", is_write: false, confirm: false,
        button_label: "Find Evidence",
        fields: &[
            Field { id: "tc-prove-from", label: "Entity A", placeholder: "First entity...", field_type: FieldType::Text, required: true },
            Field { id: "tc-prove-to", label: "Entity B", placeholder: "Second entity...", field_type: FieldType::Text, required: true },
        ],
    },
    // Knowledge: write tools
    ToolCard { tool: "store", title: "Store Entity", icon: "fa-solid fa-plus", is_write: true, confirm: true,
        button_label: "Store",
        fields: &[
            Field { id: "tc-store-entity", label: "Entity name", placeholder: "Entity name...", field_type: FieldType::Text, required: true },
            Field { id: "tc-store-type", label: "Type", placeholder: "Person, Organization...", field_type: FieldType::Text, required: false },
            Field { id: "tc-store-conf", label: "Confidence", placeholder: "", field_type: FieldType::Number { default: "0.7", min: "0", max: "1", step: "0.05" }, required: false },
            Field { id: "tc-store-src", label: "Source", placeholder: "Source (optional)", field_type: FieldType::Text, required: false },
        ],
    },
    ToolCard { tool: "relate", title: "Create Relationship", icon: "fa-solid fa-link", is_write: true, confirm: true,
        button_label: "Relate",
        fields: &[
            Field { id: "tc-relate-from", label: "From entity", placeholder: "Source entity...", field_type: FieldType::Text, required: true },
            Field { id: "tc-relate-rel", label: "Relationship", placeholder: "e.g. president_of, located_in...", field_type: FieldType::Text, required: true },
            Field { id: "tc-relate-to", label: "To entity", placeholder: "Target entity...", field_type: FieldType::Text, required: true },
            Field { id: "tc-relate-conf", label: "Confidence", placeholder: "", field_type: FieldType::Number { default: "0.7", min: "0", max: "1", step: "0.05" }, required: false },
            Field { id: "tc-relate-vf", label: "Valid from", placeholder: "YYYY-MM-DD", field_type: FieldType::Text, required: false },
        ],
    },
    ToolCard { tool: "reinforce", title: "Adjust Confidence", icon: "fa-solid fa-sliders", is_write: true, confirm: false,
        button_label: "Load Current",
        fields: &[
            Field { id: "tc-reinforce-e", label: "Entity", placeholder: "Entity name...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "correct", title: "Correct Entity", icon: "fa-solid fa-pen-to-square", is_write: true, confirm: true,
        button_label: "Correct",
        fields: &[
            Field { id: "tc-correct-e", label: "Entity", placeholder: "Entity to correct...", field_type: FieldType::Text, required: true },
            Field { id: "tc-correct-reason", label: "Reason", placeholder: "Why is this wrong?", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "delete", title: "Delete Entity", icon: "fa-solid fa-trash", is_write: true, confirm: true,
        button_label: "Delete",
        fields: &[
            Field { id: "tc-delete-e", label: "Entity", placeholder: "Entity to delete...", field_type: FieldType::Text, required: true },
        ],
    },
    // Analysis
    ToolCard { tool: "compare", title: "Compare Entities", icon: "fa-solid fa-code-compare", is_write: false, confirm: false,
        button_label: "Compare",
        fields: &[
            Field { id: "tc-compare-a", label: "Entity A", placeholder: "First entity...", field_type: FieldType::Text, required: true },
            Field { id: "tc-compare-b", label: "Entity B", placeholder: "Second entity...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "shortest_path", title: "Shortest Path", icon: "fa-solid fa-route", is_write: false, confirm: false,
        button_label: "Find Path",
        fields: &[
            Field { id: "tc-sp-from", label: "From", placeholder: "Start entity...", field_type: FieldType::Text, required: true },
            Field { id: "tc-sp-to", label: "To", placeholder: "Target entity...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "most_connected", title: "Most Connected", icon: "fa-solid fa-chart-bar", is_write: false, confirm: false,
        button_label: "Find",
        fields: &[
            Field { id: "tc-mc-limit", label: "Limit", placeholder: "", field_type: FieldType::Number { default: "10", min: "1", max: "50", step: "1" }, required: false },
        ],
    },
    ToolCard { tool: "isolated", title: "Isolated Entities", icon: "fa-solid fa-circle-dot", is_write: false, confirm: false,
        button_label: "Find",
        fields: &[
            Field { id: "tc-iso-max", label: "Max Edges", placeholder: "", field_type: FieldType::Number { default: "1", min: "0", max: "5", step: "1" }, required: false },
        ],
    },
    // Temporal
    ToolCard { tool: "timeline", title: "Entity Timeline", icon: "fa-solid fa-clock", is_write: false, confirm: false,
        button_label: "Show Timeline",
        fields: &[
            Field { id: "tc-timeline-e", label: "Entity", placeholder: "Entity name...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "date_query", title: "Date Query", icon: "fa-solid fa-calendar-day", is_write: false, confirm: false,
        button_label: "Query",
        fields: &[
            Field { id: "tc-dq-entity", label: "Entity", placeholder: "Entity name...", field_type: FieldType::Text, required: true },
            Field { id: "tc-dq-from", label: "From Date", placeholder: "YYYY-MM-DD", field_type: FieldType::Text, required: false },
            Field { id: "tc-dq-to", label: "To Date", placeholder: "YYYY-MM-DD", field_type: FieldType::Text, required: false },
        ],
    },
    ToolCard { tool: "current_state", title: "Current State", icon: "fa-solid fa-clock-rotate-left", is_write: false, confirm: false,
        button_label: "Show Current",
        fields: &[
            Field { id: "tc-cs-entity", label: "Entity", placeholder: "Entity name...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "fact_provenance", title: "Fact Provenance", icon: "fa-solid fa-magnifying-glass-location", is_write: false, confirm: false,
        button_label: "Trace Sources",
        fields: &[
            Field { id: "tc-fp-entity", label: "Entity", placeholder: "Entity name...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "contradictions", title: "Contradictions", icon: "fa-solid fa-scale-unbalanced", is_write: false, confirm: false,
        button_label: "Find Conflicts",
        fields: &[
            Field { id: "tc-ct-entity", label: "Entity", placeholder: "Entity or topic...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "situation_at", title: "Situation At Date", icon: "fa-solid fa-camera-retro", is_write: false, confirm: false,
        button_label: "Reconstruct",
        fields: &[
            Field { id: "tc-sa-entity", label: "Entity", placeholder: "Entity name...", field_type: FieldType::Text, required: true },
            Field { id: "tc-sa-date", label: "Date", placeholder: "YYYY-MM-DD", field_type: FieldType::Text, required: true },
        ],
    },
    // Document provenance
    ToolCard { tool: "provenance", title: "Entity Provenance", icon: "fa-solid fa-file-lines", is_write: false, confirm: false,
        button_label: "Trace Sources",
        fields: &[
            Field { id: "tc-provenance-e", label: "Entity", placeholder: "Entity name...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "documents", title: "Ingested Documents", icon: "fa-solid fa-folder-open", is_write: false, confirm: false,
        button_label: "List Documents",
        fields: &[
            Field { id: "tc-docs-limit", label: "Limit", placeholder: "", field_type: FieldType::Number { default: "20", min: "1", max: "100", step: "1" }, required: false },
        ],
    },
    // Investigation tools
    ToolCard { tool: "ingest", title: "Ingest Text", icon: "fa-solid fa-file-import", is_write: true, confirm: true,
        button_label: "Ingest",
        fields: &[
            Field { id: "tc-ingest-text", label: "Text", placeholder: "Paste text to extract entities and relations...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "analyze", title: "Analyze Text (NER Preview)", icon: "fa-solid fa-microscope", is_write: false, confirm: false,
        button_label: "Analyze",
        fields: &[
            Field { id: "tc-analyze-text", label: "Text", placeholder: "Paste text to analyze for entities and relations...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "investigate", title: "Investigate Entity", icon: "fa-solid fa-magnifying-glass-chart", is_write: false, confirm: false,
        button_label: "Search & Analyze",
        fields: &[
            Field { id: "tc-investigate-entity", label: "Entity", placeholder: "Entity or topic to investigate...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "changes", title: "Recent Changes", icon: "fa-solid fa-clock-rotate-left", is_write: false, confirm: false,
        button_label: "Show Changes",
        fields: &[
            Field { id: "tc-changes-since", label: "Since", placeholder: "YYYY-MM-DD (default: last 24h)", field_type: FieldType::Text, required: false },
        ],
    },
    ToolCard { tool: "watch", title: "Watch Entity", icon: "fa-solid fa-bell", is_write: true, confirm: false,
        button_label: "Start Watching",
        fields: &[
            Field { id: "tc-watch-entity", label: "Entity", placeholder: "Entity name...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "network_analysis", title: "Network Analysis", icon: "fa-solid fa-diagram-project", is_write: false, confirm: false,
        button_label: "Map Network",
        fields: &[
            Field { id: "tc-net-entity", label: "Entity", placeholder: "Center entity...", field_type: FieldType::Text, required: true },
            Field { id: "tc-net-depth", label: "Depth", placeholder: "", field_type: FieldType::Number { default: "2", min: "1", max: "4", step: "1" }, required: false },
        ],
    },
    ToolCard { tool: "entity_360", title: "Entity 360", icon: "fa-solid fa-globe", is_write: false, confirm: false,
        button_label: "Full View",
        fields: &[
            Field { id: "tc-360-entity", label: "Entity", placeholder: "Entity name...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "entity_gaps", title: "Entity Gaps", icon: "fa-solid fa-puzzle-piece", is_write: false, confirm: false,
        button_label: "Find Gaps",
        fields: &[
            Field { id: "tc-gaps-entity", label: "Entity", placeholder: "Entity name...", field_type: FieldType::Text, required: true },
        ],
    },
    // Reporting tools
    ToolCard { tool: "briefing", title: "Briefing", icon: "fa-solid fa-file-lines", is_write: false, confirm: false,
        button_label: "Generate Briefing",
        fields: &[
            Field { id: "tc-brief-topic", label: "Topic", placeholder: "Topic or entity...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "export", title: "Export Subgraph", icon: "fa-solid fa-file-export", is_write: false, confirm: false,
        button_label: "Export JSON",
        fields: &[
            Field { id: "tc-export-entity", label: "Entity", placeholder: "Center entity...", field_type: FieldType::Text, required: true },
            Field { id: "tc-export-depth", label: "Depth", placeholder: "", field_type: FieldType::Number { default: "2", min: "1", max: "5", step: "1" }, required: false },
        ],
    },
    ToolCard { tool: "dossier", title: "Entity Dossier", icon: "fa-solid fa-address-card", is_write: false, confirm: false,
        button_label: "Generate Dossier",
        fields: &[
            Field { id: "tc-dossier-entity", label: "Entity", placeholder: "Entity name...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "topic_map", title: "Topic Map", icon: "fa-solid fa-sitemap", is_write: false, confirm: false,
        button_label: "Map Topic",
        fields: &[
            Field { id: "tc-topicmap-topic", label: "Topic", placeholder: "Topic or theme...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "graph_stats", title: "Knowledge Health", icon: "fa-solid fa-chart-pie", is_write: false, confirm: false,
        button_label: "Show Stats",
        fields: &[],
    },
    // Reasoning tools
    ToolCard { tool: "what_if", title: "What-If Simulation", icon: "fa-solid fa-flask", is_write: false, confirm: false,
        button_label: "Simulate",
        fields: &[
            Field { id: "tc-wif-entity", label: "Entity", placeholder: "Entity to change...", field_type: FieldType::Text, required: true },
            Field { id: "tc-wif-conf", label: "New Confidence", placeholder: "", field_type: FieldType::Number { default: "0.20", min: "0.00", max: "1.00", step: "0.05" }, required: true },
        ],
    },
    ToolCard { tool: "influence", title: "Influence Paths", icon: "fa-solid fa-diagram-successor", is_write: false, confirm: false,
        button_label: "Find Influence",
        fields: &[
            Field { id: "tc-inf-from", label: "From", placeholder: "Source entity...", field_type: FieldType::Text, required: true },
            Field { id: "tc-inf-to", label: "To", placeholder: "Target entity...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "black_areas", title: "Knowledge Gaps", icon: "fa-solid fa-binoculars", is_write: false, confirm: false,
        button_label: "Scan for Gaps",
        fields: &[],
    },
    // Assessment tools
    ToolCard { tool: "assess_create", title: "Create Assessment", icon: "fa-solid fa-scale-balanced", is_write: true, confirm: true,
        button_label: "Create",
        fields: &[
            Field { id: "tc-ac-title", label: "Title", placeholder: "Assessment title...", field_type: FieldType::Text, required: true },
            Field { id: "tc-ac-description", label: "Description", placeholder: "What are you assessing?", field_type: FieldType::Text, required: false },
            Field { id: "tc-ac-probability", label: "Initial Probability", placeholder: "", field_type: FieldType::Number { default: "0.50", min: "0.05", max: "0.95", step: "0.05" }, required: false },
            Field { id: "tc-ac-criteria", label: "Success Criteria", placeholder: "How to verify this assessment...", field_type: FieldType::Text, required: false },
        ],
    },
    ToolCard { tool: "assess_evidence", title: "Add Evidence", icon: "fa-solid fa-plus-circle", is_write: true, confirm: true,
        button_label: "Add Evidence",
        fields: &[
            Field { id: "tc-ae-assessment", label: "Assessment", placeholder: "Assessment label...", field_type: FieldType::Text, required: true },
            Field { id: "tc-ae-text", label: "Evidence", placeholder: "Evidence entity or text...", field_type: FieldType::Text, required: true },
            Field { id: "tc-ae-direction", label: "Direction", placeholder: "", field_type: FieldType::Select(&[("supports", "Supports"), ("contradicts", "Contradicts")]), required: true },
        ],
    },
    ToolCard { tool: "assess_evaluate", title: "Evaluate Assessment", icon: "fa-solid fa-calculator", is_write: false, confirm: false,
        button_label: "Evaluate",
        fields: &[
            Field { id: "tc-aev-assessment", label: "Assessment", placeholder: "Assessment label...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "assess_list", title: "List Assessments", icon: "fa-solid fa-list-check", is_write: false, confirm: false,
        button_label: "Show All",
        fields: &[],
    },
    ToolCard { tool: "assess_detail", title: "Assessment Detail", icon: "fa-solid fa-magnifying-glass-chart", is_write: false, confirm: false,
        button_label: "Show Detail",
        fields: &[
            Field { id: "tc-ad-assessment", label: "Assessment", placeholder: "Assessment label...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "assess_compare", title: "Compare Assessments", icon: "fa-solid fa-code-compare", is_write: false, confirm: false,
        button_label: "Compare",
        fields: &[
            Field { id: "tc-acmp-a", label: "Assessment A", placeholder: "First assessment label...", field_type: FieldType::Text, required: true },
            Field { id: "tc-acmp-b", label: "Assessment B", placeholder: "Second assessment label...", field_type: FieldType::Text, required: true },
        ],
    },
    // Action tools
    ToolCard { tool: "rule_create", title: "Create Rule", icon: "fa-solid fa-gavel", is_write: true, confirm: true,
        button_label: "Create Rule",
        fields: &[
            Field { id: "tc-rc-description", label: "Rule description", placeholder: "e.g. when confidence drops below 0.3 then flag for review...", field_type: FieldType::Text, required: true },
        ],
    },
    ToolCard { tool: "rule_list", title: "List Rules", icon: "fa-solid fa-list-check", is_write: false, confirm: false,
        button_label: "Show Rules",
        fields: &[],
    },
    ToolCard { tool: "rule_fire", title: "Fire Rules (Dry Run)", icon: "fa-solid fa-bolt", is_write: false, confirm: false,
        button_label: "Run Dry-Run",
        fields: &[],
    },
    ToolCard { tool: "schedule", title: "Schedule", icon: "fa-solid fa-calendar-check", is_write: true, confirm: true,
        button_label: "Create Schedule",
        fields: &[
            Field { id: "tc-sched-name", label: "Name", placeholder: "Schedule name...", field_type: FieldType::Text, required: true },
            Field { id: "tc-sched-interval", label: "Interval", placeholder: "e.g. 6h, daily, 30m", field_type: FieldType::Text, required: true },
            Field { id: "tc-sched-source", label: "Source", placeholder: "e.g. reuters.com", field_type: FieldType::Text, required: false },
        ],
    },
];

/// Generate a tool parameter card. Returns None if tool has no card definition.
pub fn generate_tool_card(tool_name: &str) -> Option<String> {
    let card_def = CARDS.iter().find(|c| c.tool == tool_name)?;
    Some(render_card(card_def))
}

/// Get the list of autocomplete field IDs for a tool (for wiring up after render).
pub fn autocomplete_fields(tool_name: &str) -> Vec<(&'static str, &'static str)> {
    let entity_fields: &[&str] = match tool_name {
        "query" => &["tc-query-entity"],
        "search" => &["tc-search-q"],
        "explain" => &["tc-explain-e"],
        "similar" => &["tc-similar-t"],
        "prove" => &["tc-prove-from", "tc-prove-to"],
        "store" => &["tc-store-entity"],
        "relate" => &["tc-relate-from", "tc-relate-to"],
        "reinforce" => &["tc-reinforce-e"],
        "correct" => &["tc-correct-e"],
        "delete" => &["tc-delete-e"],
        "compare" => &["tc-compare-a", "tc-compare-b"],
        "shortest_path" => &["tc-sp-from", "tc-sp-to"],
        "timeline" => &["tc-timeline-e"],
        "provenance" => &["tc-provenance-e"],
        "assess_evidence" => &["tc-ae-assessment"],
        "assess_evaluate" => &["tc-aev-assessment"],
        "assess_detail" => &["tc-ad-assessment"],
        "assess_compare" => &["tc-acmp-a", "tc-acmp-b"],
        _ => &[],
    };
    let mut result: Vec<(&str, &str)> = entity_fields.iter().map(|id| (*id, "/autocomplete")).collect();
    // Node type autocomplete
    if tool_name == "store" {
        result.push(("tc-store-type", "/config/node-types"));
    }
    // Relationship type autocomplete
    if tool_name == "relate" {
        result.push(("tc-relate-rel", "/config/relation-types"));
    }
    result
}

// ── Rendering ──

fn render_card(def: &ToolCard) -> String {
    let write_badge = if def.is_write {
        " <span class=\"chat-help-write-badge\"><i class=\"fa-solid fa-pen\"></i> write</span>"
    } else {
        ""
    };

    let mut html = format!(
        "<div class=\"chat-card\">\
            <div class=\"chat-card-header\"><i class=\"{}\"></i> {}{}</div>\
            <div class=\"chat-card-body\" style=\"padding:0.5rem 0.6rem;\">\
                <div style=\"display:flex;flex-direction:column;gap:0.4rem;\">",
        def.icon, html_escape(def.title), write_badge,
    );

    // Render fields
    let mut row_open = false;
    for (i, field) in def.fields.iter().enumerate() {
        // Group small fields (number, select) in rows of 2
        let is_small = matches!(field.field_type, FieldType::Number { .. } | FieldType::Select(_));
        let next_is_small = def.fields.get(i + 1)
            .map(|f| matches!(f.field_type, FieldType::Number { .. } | FieldType::Select(_)))
            .unwrap_or(false);

        if is_small && !row_open && next_is_small {
            html.push_str("<div style=\"display:flex;gap:0.5rem;\">");
            row_open = true;
        }
        if is_small && row_open {
            html.push_str("<div style=\"flex:1;\">");
        } else {
            html.push_str("<div>");
        }

        html.push_str(&format!("<label style=\"{}\">{}</label>", LS, html_escape(field.label)));

        match &field.field_type {
            FieldType::Text => {
                html.push_str(&format!(
                    "<input id=\"{}\" type=\"text\" placeholder=\"{}\" style=\"{}\" />",
                    field.id, html_escape(field.placeholder), IS,
                ));
            }
            FieldType::Number { default, min, max, step } => {
                html.push_str(&format!(
                    "<input id=\"{}\" type=\"number\" value=\"{}\" min=\"{}\" max=\"{}\" step=\"{}\" style=\"{}\" />",
                    field.id, default, min, max, step, IS,
                ));
            }
            FieldType::Select(options) => {
                html.push_str(&format!("<select id=\"{}\" style=\"{}\">", field.id, IS));
                for (val, label) in *options {
                    html.push_str(&format!("<option value=\"{}\">{}</option>", val, label));
                }
                html.push_str("</select>");
            }
        }

        html.push_str("</div>");

        if row_open && (!next_is_small || i + 1 >= def.fields.len()) {
            html.push_str("</div>");
            row_open = false;
        }
    }

    // Button: collect values and dispatch event
    let btn_style = if def.tool == "delete" {
        "padding:0.35rem 0.75rem;font-size:0.78rem;font-family:inherit;\
         background:var(--danger, #d9534f);color:#fff;border:none;border-radius:4px;\
         cursor:pointer;display:flex;align-items:center;gap:0.3rem;justify-content:center;width:100%;"
    } else {
        BS
    };

    let mut onclick = String::new();
    // Collect field values
    for field in def.fields {
        let key = field.id.rsplit('-').next().unwrap_or(field.id);
        onclick.push_str(&format!("var {}=document.getElementById('{}').value;", key, field.id));
        if field.required {
            onclick.push_str(&format!(
                "if(!{}){{alert('{} is required');return;}}",
                key, html_escape(field.label),
            ));
        }
    }
    // Build params object and dispatch
    let param_entries: Vec<String> = def.fields.iter().map(|f| {
        let key = f.id.rsplit('-').next().unwrap_or(f.id);
        format!("{}:{}", key, key)
    }).collect();
    let dispatch_js = format!(
        "window.dispatchEvent(new CustomEvent('engram-run-tool',{{detail:JSON.stringify({{tool:'{}',params:{{{}}}}})}}))",
        def.tool, param_entries.join(","),
    );

    // For delete: two-click confirmation via shared helper
    if def.tool == "delete" {
        onclick.push_str(&format!(
            "if(window.__ec_confirm_delete&&!window.__ec_confirm_delete('tc-{}-btn'))return;{}",
            def.tool, dispatch_js,
        ));
    } else {
        onclick.push_str(&dispatch_js);
    }

    // Result feedback area (hidden until action completes)
    let result_id = format!("tc-{}-result", def.tool);
    html.push_str(&format!(
        "<div id=\"{}\" style=\"display:none;padding:0.3rem 0.5rem;font-size:0.75rem;\
         border-radius:4px;margin-bottom:0.3rem;text-align:center;\"></div>",
        result_id,
    ));

    html.push_str(&format!(
        "<button id=\"tc-{}-btn\" onclick=\"{}\" style=\"{}\"><i class=\"{}\"></i> {}</button>",
        def.tool, onclick, btn_style, def.icon, html_escape(def.button_label),
    ));

    html.push_str("</div></div></div>");
    html
}
