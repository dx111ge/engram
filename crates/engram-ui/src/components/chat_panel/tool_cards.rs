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
    // Temporal
    ToolCard { tool: "timeline", title: "Entity Timeline", icon: "fa-solid fa-clock", is_write: false, confirm: false,
        button_label: "Show Timeline",
        fields: &[
            Field { id: "tc-timeline-e", label: "Entity", placeholder: "Entity name...", field_type: FieldType::Text, required: true },
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
        _ => &[],
    };
    let mut result: Vec<(&str, &str)> = entity_fields.iter().map(|id| (*id, "/search")).collect();
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
    // Confirm dialog for write operations
    if def.confirm {
        onclick.push_str(&format!("if(!confirm('Execute {}?'))return;", html_escape(def.title)));
    }
    // Build params object and dispatch
    let param_entries: Vec<String> = def.fields.iter().map(|f| {
        let key = f.id.rsplit('-').next().unwrap_or(f.id);
        format!("{}:{}", key, key)
    }).collect();
    onclick.push_str(&format!(
        "window.dispatchEvent(new CustomEvent('engram-run-tool',{{detail:JSON.stringify({{tool:'{}',params:{{{}}}}})}}))",
        def.tool, param_entries.join(","),
    ));

    html.push_str(&format!(
        "<button onclick=\"{}\" style=\"{}\"><i class=\"{}\"></i> {}</button>",
        onclick, btn_style, def.icon, html_escape(def.button_label),
    ));

    html.push_str("</div></div></div>");
    html
}
