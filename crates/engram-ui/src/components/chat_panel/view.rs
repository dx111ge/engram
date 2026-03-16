//! Chat panel view helpers: message rendering and follow-up extraction.

use leptos::prelude::*;
use crate::components::chat_types::*;

/// Render a single chat message bubble with appropriate styling.
pub fn render_message(msg: ChatMessage) -> impl IntoView {
    let (bubble_style, icon_class, align, role_name) = match &msg.role {
        ChatRole::User => (
            "background:var(--accent, #4a9eff);color:#fff;\
             border-radius:12px 12px 2px 12px;padding:0.6rem 0.85rem;\
             max-width:85%;word-wrap:break-word;font-size:0.85rem;",
            "fa-solid fa-user", "align-self:flex-end;", "You",
        ),
        ChatRole::Assistant => (
            "background:var(--bg-tertiary, #232730);color:var(--text, #c9ccd3);\
             border-radius:12px 12px 12px 2px;padding:0.6rem 0.85rem;\
             max-width:85%;word-wrap:break-word;font-size:0.85rem;\
             border:1px solid var(--border, #2d3139);",
            "fa-solid fa-brain", "align-self:flex-start;", "Engram",
        ),
        ChatRole::System => (
            "background:var(--warning-bg, #3d3520);color:var(--warning, #f0ad4e);\
             border-radius:8px;padding:0.5rem 0.75rem;\
             max-width:90%;word-wrap:break-word;font-size:0.8rem;\
             border:1px solid var(--warning, #f0ad4e);",
            "fa-solid fa-circle-exclamation", "align-self:center;", "System",
        ),
        ChatRole::ToolResult => (
            "background:var(--bg-tertiary, #232730);color:var(--text-muted, #8b8fa3);\
             border-radius:6px;padding:0.4rem 0.65rem;\
             max-width:90%;word-wrap:break-word;font-size:0.75rem;\
             border-left:3px solid var(--accent, #4a9eff);\
             font-family:monospace;white-space:pre-wrap;",
            "fa-solid fa-wrench", "align-self:flex-start;", "Tool",
        ),
        ChatRole::Context => (
            "background:var(--bg-tertiary, #232730);color:var(--info, #5bc0de);\
             border-radius:6px;padding:0.4rem 0.65rem;\
             max-width:90%;word-wrap:break-word;font-size:0.75rem;\
             border-left:3px solid var(--info, #5bc0de);",
            "fa-solid fa-magnifying-glass", "align-self:flex-start;", "Context",
        ),
        ChatRole::WriteConfirmation => (
            "background:var(--warning-bg, #3d3520);color:var(--warning, #f0ad4e);\
             border-radius:6px;padding:0.4rem 0.65rem;\
             max-width:90%;word-wrap:break-word;font-size:0.75rem;\
             border-left:3px solid var(--warning, #f0ad4e);",
            "fa-solid fa-pen-to-square", "align-self:flex-start;", "Writes",
        ),
    };

    let wrapper_style = format!("display:flex;flex-direction:column;{align}gap:0.2rem;");

    view! {
        <div style=wrapper_style>
            <div style="display:flex;align-items:center;gap:0.3rem;\
                        font-size:0.7rem;color:var(--text-muted, #8b8fa3);">
                <i class=icon_class style="font-size:0.65rem;"></i>
                <span>{role_name}</span>
            </div>
            <div style=bubble_style>
                {msg.content.clone()}
            </div>
        </div>
    }
}

/// Extract follow-up suggestions from the last assistant message.
pub fn extract_follow_ups(messages: &[ChatMessage]) -> Vec<FollowUpSuggestion> {
    let last_assistant = messages.iter().rev().find(|m| m.role == ChatRole::Assistant);
    let content = match last_assistant {
        Some(msg) => &msg.content,
        None => return Vec::new(),
    };

    let mut suggestions = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        let is_suggestion = (trimmed.starts_with("1.") || trimmed.starts_with("2.") || trimmed.starts_with("3."))
            || (trimmed.starts_with("- ") && trimmed.contains('?'))
            || (trimmed.starts_with("* ") && trimmed.contains('?'));

        if is_suggestion {
            let text = trimmed
                .trim_start_matches(|c: char| c.is_numeric() || c == '.' || c == '-' || c == '*')
                .trim()
                .to_string();
            if !text.is_empty() && text.len() < 100 {
                let icon = if text.to_lowercase().contains("compare") {
                    "fa-solid fa-code-compare"
                } else if text.to_lowercase().contains("gap") || text.to_lowercase().contains("missing") {
                    "fa-solid fa-circle-question"
                } else if text.to_lowercase().contains("timeline") || text.to_lowercase().contains("when") {
                    "fa-solid fa-clock"
                } else {
                    "fa-solid fa-arrow-right"
                };
                suggestions.push(FollowUpSuggestion { text, icon });
            }
        }

        if suggestions.len() >= 3 {
            break;
        }
    }

    suggestions
}
