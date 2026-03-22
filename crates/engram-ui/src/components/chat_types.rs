use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Context signal: currently selected node on the Explore page.
/// Provided at app level, written by GraphPage, read by ChatPanel.
#[derive(Clone, Copy)]
pub struct ChatSelectedNode(pub RwSignal<Option<String>>);

/// Context signal: currently viewed assessment on the Insights page.
#[derive(Clone, Copy)]
pub struct ChatCurrentAssessment(pub RwSignal<Option<String>>);

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    /// Pre-rendered HTML for rich display (markdown, cards, help grids).
    /// When set, view renders via inner_html instead of plain text.
    pub display_html: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChatRole {
    User,
    Assistant,
    System,
    ToolResult,
    /// Visible context retrieval step
    Context,
    /// Write confirmation batch awaiting user approval
    WriteConfirmation,
}

/// A pending write operation awaiting user confirmation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingWrite {
    /// Display label for the operation (e.g. "Store: Putin as Person")
    pub label: String,
    /// Tool name that produced this write
    pub tool_name: String,
    /// Original arguments JSON string
    pub args: String,
    /// Whether user has checked this for execution
    pub selected: bool,
}

/// An entity retrieved as context for the current message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextItem {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
    pub confidence: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_count: Option<u32>,
}

/// A follow-up suggestion shown after each response.
#[derive(Clone, Debug)]
pub struct FollowUpSuggestion {
    pub text: String,
    pub icon: &'static str,
}

/// Which pages should show the chat panel.
pub fn page_allows_chat(path: &str) -> bool {
    matches!(
        path,
        "/graph" | "/insights" | "/search" | "/node" | "/ingest"
            | "/sources" | "/gaps" | "/actions" | "/mesh"
            | "/nl" | "/import" | "/learning"
    ) || path.starts_with("/node/")
}

/// Write tools that need confirmation before execution.
pub fn is_write_tool(name: &str) -> bool {
    matches!(
        name,
        "engram_store"
            | "engram_relate"
            | "engram_reinforce"
            | "engram_correct"
            | "engram_delete"
            | "engram_ingest_text"
            | "engram_assess_create"
            | "engram_assess_evidence"
            | "engram_rule_create"
            | "engram_investigate"
            | "engram_watch"
    )
}

/// Build a human-readable label for a pending write.
pub fn write_label(tool_name: &str, args: &str) -> String {
    let parsed: serde_json::Value = serde_json::from_str(args).unwrap_or_default();
    match tool_name {
        "engram_store" => {
            let entity = parsed.get("entity").and_then(|v| v.as_str()).unwrap_or("?");
            let typ = parsed.get("type").and_then(|v| v.as_str()).unwrap_or("entity");
            let conf = parsed.get("confidence").and_then(|v| v.as_f64());
            match conf {
                Some(c) => format!("Store: \"{}\" as {} (confidence: {:.0}%)", entity, typ, c * 100.0),
                None => format!("Store: \"{}\" as {}", entity, typ),
            }
        }
        "engram_relate" => {
            let from = parsed.get("from").and_then(|v| v.as_str()).unwrap_or("?");
            let to = parsed.get("to").and_then(|v| v.as_str()).unwrap_or("?");
            let rel = parsed.get("relationship").and_then(|v| v.as_str()).unwrap_or("?");
            format!("Relate: {} -> {} -> {}", from, rel, to)
        }
        "engram_reinforce" => {
            let entity = parsed.get("entity").and_then(|v| v.as_str()).unwrap_or("?");
            format!("Reinforce: {} confidence +boost", entity)
        }
        "engram_correct" => {
            let entity = parsed.get("entity").and_then(|v| v.as_str()).unwrap_or("?");
            format!("Correct: mark \"{}\" as wrong", entity)
        }
        "engram_delete" => {
            let entity = parsed.get("entity").and_then(|v| v.as_str()).unwrap_or("?");
            format!("Delete: soft-delete \"{}\"", entity)
        }
        "engram_ingest_text" => {
            let text = parsed.get("text").and_then(|v| v.as_str()).unwrap_or("...");
            let preview = if text.len() > 40 { &text[..40] } else { text };
            format!("Ingest: NER+RE on \"{}...\"", preview)
        }
        "engram_assess_create" => {
            let title = parsed.get("title").and_then(|v| v.as_str()).unwrap_or("?");
            format!("Assessment: create \"{}\"", title)
        }
        "engram_assess_evidence" => {
            let label = parsed.get("label").and_then(|v| v.as_str()).unwrap_or("?");
            let dir = parsed.get("direction").and_then(|v| v.as_str()).unwrap_or("?");
            format!("Evidence: {} {} assessment", dir, label)
        }
        "engram_rule_create" => {
            let name = parsed.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            format!("Rule: create \"{}\"", name)
        }
        "engram_investigate" => {
            let entity = parsed.get("entity").and_then(|v| v.as_str()).unwrap_or("?");
            let depth = parsed.get("depth").and_then(|v| v.as_str()).unwrap_or("shallow");
            format!("Investigate: \"{}\" ({})", entity, depth)
        }
        "engram_watch" => {
            let entity = parsed.get("entity").and_then(|v| v.as_str()).unwrap_or("?");
            format!("Watch: mark \"{}\" for monitoring", entity)
        }
        _ => format!("{}: {}", tool_name, args),
    }
}
