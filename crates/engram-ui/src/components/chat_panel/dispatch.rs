//! Card dispatch: handles `engram-run-tool` custom events from interactive tool cards.
//!
//! Extracted from `mod.rs` to keep the main component file manageable.

use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api::ApiClient;
use crate::components::chat_types::*;

use super::{cards, help, markdown, tool_cards};

// ── Dispatch graph data from chat tool results to the main graph canvas ──

fn dispatch_graph_data(tool_name: &str, raw_json: &str) {
    if let Some((nodes, edges)) = cards::extract_graph_data(tool_name, raw_json) {
        let payload = serde_json::json!({ "nodes": nodes, "edges": edges });
        if let Ok(json_str) = serde_json::to_string(&payload) {
            let code = format!(
                "window.dispatchEvent(new CustomEvent('engram-chat-graph',{{detail:{}}}));",
                json_str,
            );
            let _ = js_sys::eval(&code);
        }
    }
}

// ── LLM analysis helper for tool augmentation ──

async fn llm_analysis(
    api: &ApiClient,
    set_messages: WriteSignal<Vec<ChatMessage>>,
    analysis_prompt: &str,
    user_content: &str,
) {
    let config: Result<serde_json::Value, _> = api.get("/config").await;
    let cfg = match config {
        Ok(c) => c,
        Err(_) => {
            set_messages.update(|msgs| {
                msgs.push(ChatMessage {
                    role: ChatRole::System,
                    content: "Could not load config for LLM analysis.".into(),
                    display_html: None,
                });
            });
            return;
        }
    };

    let model = cfg.get("llm_model").and_then(|v| v.as_str()).unwrap_or("");
    let endpoint = cfg.get("llm_endpoint").and_then(|v| v.as_str()).unwrap_or("");
    if model.is_empty() || endpoint.is_empty() {
        set_messages.update(|msgs| {
            msgs.push(ChatMessage {
                role: ChatRole::System,
                content: "<i class=\"fa-solid fa-triangle-exclamation\"></i> LLM not configured. Go to <strong>System > Language Model</strong> to set up.".into(),
                display_html: Some("<div style=\"color:#ffa726;font-size:0.85rem\"><i class=\"fa-solid fa-triangle-exclamation\"></i> LLM not configured. Go to <strong>System &gt; Language Model</strong> to set up.</div>".into()),
            });
        });
        return;
    }

    let persona = cfg.get("llm_system_prompt").and_then(|v| v.as_str()).unwrap_or("");

    // Show loading indicator
    set_messages.update(|msgs| {
        msgs.push(ChatMessage {
            role: ChatRole::Context,
            content: "Analyzing...".into(),
            display_html: Some("<div style=\"display:flex;align-items:center;gap:6px;color:var(--text-muted);font-size:0.85rem\"><i class=\"fa-solid fa-spinner fa-spin\"></i> AI Analysis running...</div>".into()),
        });
    });

    let system = format!("{}\n\n{}", persona, analysis_prompt);
    let llm_req = serde_json::json!({
        "model": model,
        "temperature": 0.3,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user_content}
        ]
    });

    let llm_result: Result<crate::api::types::LlmProxyResponse, _> = api.post("/proxy/llm", &llm_req).await;

    // Remove "Analyzing..." indicator
    set_messages.update(|msgs| {
        if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context && m.content == "Analyzing...") {
            msgs.remove(pos);
        }
    });

    match llm_result {
        Ok(resp) => {
            if let Some(text) = resp.choices.first().and_then(|c| c.message.as_ref()).and_then(|m| m.content.clone()) {
                let rendered = markdown::markdown_to_html(&text);
                let html = format!(
                    "<details open class=\"chat-ai-analysis\"><summary style=\"cursor:pointer;font-weight:600;font-size:0.85rem;color:var(--accent);display:flex;align-items:center;gap:6px\">\
                        <i class=\"fa-solid fa-brain\"></i> AI Analysis</summary>\
                        <div style=\"margin-top:6px;font-size:0.85rem;line-height:1.5\">{}</div></details>",
                    rendered,
                );
                set_messages.update(|msgs| {
                    msgs.push(ChatMessage {
                        role: ChatRole::Assistant,
                        content: text,
                        display_html: Some(html),
                    });
                });
            }
        }
        Err(e) => {
            set_messages.update(|msgs| {
                msgs.push(ChatMessage {
                    role: ChatRole::System,
                    content: format!("AI Analysis failed: {}", e),
                    display_html: Some(format!(
                        "<div style=\"color:#ef5350;font-size:0.85rem\"><i class=\"fa-solid fa-triangle-exclamation\"></i> AI Analysis failed: {}</div>",
                        markdown::html_escape(&e.to_string()),
                    )),
                });
            });
        }
    }
}

// ── Set up the engram-run-tool event listener ──

pub fn setup_card_dispatch(
    api: ApiClient,
    set_messages: WriteSignal<Vec<ChatMessage>>,
) {
    let api_run = api.clone();
    let cb = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web_sys::CustomEvent| {
        if let Some(detail) = ev.detail().as_string() {
            let parsed: serde_json::Value = match serde_json::from_str(&detail) {
                Ok(v) => v,
                Err(_) => return,
            };
            let tool = parsed.get("tool").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let params = parsed.get("params").cloned().unwrap_or(serde_json::json!({}));

            // Show thinking indicator
            set_messages.update(|msgs| {
                msgs.push(ChatMessage {
                    role: ChatRole::Context,
                    content: format!("Running {}...", tool),
                    display_html: None,
                });
            });

            let api = api_run.clone();
            spawn_local(async move {
                let p = |key: &str| params.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string();

                let result: Result<(String, String), String> = match tool.as_str() {
                    "query" => {
                        let body = serde_json::json!({"start": p("entity"), "depth": params.get("depth").and_then(|v| v.as_str()).and_then(|s| s.parse::<u32>().ok()).unwrap_or(1), "direction": p("dir")});
                        api.post_text("/query", &body).await.map(|r| ("engram_query".into(), r)).map_err(|e| e.to_string())
                    }
                    "search" => {
                        let body = serde_json::json!({"query": p("q"), "limit": 20});
                        api.post_text("/search", &body).await.map(|r| ("engram_search".into(), r)).map_err(|e| e.to_string())
                    }
                    "explain" => {
                        let entity = p("e");
                        let encoded = js_sys::encode_uri_component(&entity);
                        let result = api.get_text(&format!("/explain/{}", encoded.as_string().unwrap_or_default())).await;
                        match result {
                            Ok(json_str) => {
                                // Show data card first
                                let card_html = cards::render_tool_card("engram_explain", &json_str);
                                dispatch_graph_data("engram_explain", &json_str);
                                set_messages.update(|msgs| {
                                    // Remove "Running..." message
                                    if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) {
                                        msgs.remove(pos);
                                    }
                                    msgs.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: json_str.chars().take(500).collect(),
                                        display_html: Some(card_html),
                                    });
                                });

                                // Now async LLM summary (non-blocking)
                                let config: Result<serde_json::Value, _> = api.get("/config").await;
                                if let Ok(cfg) = config {
                                    let model = cfg.get("llm_model").and_then(|v| v.as_str()).unwrap_or("");
                                    let endpoint = cfg.get("llm_endpoint").and_then(|v| v.as_str()).unwrap_or("");
                                    if model.is_empty() || endpoint.is_empty() {
                                        set_messages.update(|msgs| {
                                            msgs.push(ChatMessage {
                                                role: ChatRole::System,
                                                content: "LLM not configured. Go to System > Language Model to set up.".into(),
                                                display_html: None,
                                            });
                                        });
                                    } else {
                                        let persona = cfg.get("llm_system_prompt").and_then(|v| v.as_str()).unwrap_or("");
                                        set_messages.update(|msgs| {
                                            msgs.push(ChatMessage {
                                                role: ChatRole::Context,
                                                content: "Summarizing...".into(),
                                                display_html: None,
                                            });
                                        });
                                        let llm_req = serde_json::json!({
                                            "model": model,
                                            "temperature": 0.3,
                                            "messages": [
                                                {"role": "system", "content": format!("{}\n\nSummarize the following entity data in 2-3 concise sentences. Mention confidence level, key relationships, and notable properties. Do not repeat raw JSON data.", persona)},
                                                {"role": "user", "content": format!("Summarize this entity:\n{}", json_str)}
                                            ]
                                        });
                                        let llm_result: Result<crate::api::types::LlmProxyResponse, _> = api.post("/proxy/llm", &llm_req).await;
                                        set_messages.update(|msgs| {
                                            // Remove "Summarizing..." message
                                            if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context && m.content == "Summarizing...") {
                                                msgs.remove(pos);
                                            }
                                        });
                                        match llm_result {
                                            Ok(resp) => {
                                                if let Some(text) = resp.choices.first().and_then(|c| c.message.as_ref()).and_then(|m| m.content.clone()) {
                                                    let rendered = markdown::markdown_to_html(&text);
                                                    set_messages.update(|msgs| {
                                                        msgs.push(ChatMessage {
                                                            role: ChatRole::Assistant,
                                                            content: text,
                                                            display_html: Some(rendered),
                                                        });
                                                    });
                                                }
                                            }
                                            Err(e) => {
                                                set_messages.update(|msgs| {
                                                    msgs.push(ChatMessage {
                                                        role: ChatRole::System,
                                                        content: format!("LLM summary failed: {}", e),
                                                        display_html: None,
                                                    });
                                                });
                                            }
                                        }
                                    }
                                }
                                return; // already handled
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    "similar" => {
                        let entity = p("t");
                        // Two-step: resolve entity first for better semantic search
                        let encoded = js_sys::encode_uri_component(&entity);
                        let explain_text = api.get_text(&format!("/explain/{}", encoded.as_string().unwrap_or_default())).await.unwrap_or_default();
                        let search_text = if !explain_text.is_empty() {
                            let ed: serde_json::Value = serde_json::from_str(&explain_text).unwrap_or_default();
                            let cn = ed.get("properties").and_then(|p| p.get("canonical_name")).and_then(|v| v.as_str()).unwrap_or(&entity);
                            let nt = ed.get("properties").and_then(|p| p.get("node_type")).and_then(|v| v.as_str()).unwrap_or("");
                            format!("{} {}", cn, nt)
                        } else {
                            entity
                        };
                        let body = serde_json::json!({"text": search_text, "limit": 10});
                        api.post_text("/similar", &body).await.map(|r| ("engram_similar".into(), r)).map_err(|e| e.to_string())
                    }
                    "prove" | "shortest_path" => {
                        let body = serde_json::json!({"from": p("from"), "to": p("to"), "max_depth": 6});
                        match api.post_text("/chat/shortest_path", &body).await {
                            Ok(json_str) => {
                                let card_html = cards::render_tool_card("engram_shortest_path", &json_str);
                                dispatch_graph_data("engram_shortest_path", &json_str);
                                set_messages.update(|msgs| {
                                    if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) { msgs.remove(pos); }
                                    msgs.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: json_str.chars().take(500).collect(),
                                        display_html: Some(card_html),
                                    });
                                });
                                // LLM analysis of the path
                                let found = serde_json::from_str::<serde_json::Value>(&json_str).ok()
                                    .and_then(|v| v.get("found").and_then(|f| f.as_bool())).unwrap_or(false);
                                if found {
                                    llm_analysis(&api, set_messages,
                                        "Explain the significance of this connection path between entities in a knowledge graph. Describe what each hop reveals about the relationship. Be concise (2-3 sentences).",
                                        &format!("Explain this path:\n{}", json_str),
                                    ).await;
                                }
                                return;
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    "compare" => {
                        let body = serde_json::json!({"entity_a": p("a"), "entity_b": p("b")});
                        match api.post_text("/chat/compare", &body).await {
                            Ok(json_str) => {
                                let card_html = cards::render_tool_card("engram_compare", &json_str);
                                dispatch_graph_data("engram_compare", &json_str);
                                set_messages.update(|msgs| {
                                    if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) { msgs.remove(pos); }
                                    msgs.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: json_str.chars().take(500).collect(),
                                        display_html: Some(card_html),
                                    });
                                });
                                // LLM analysis of the comparison
                                llm_analysis(&api, set_messages,
                                    "Compare these two entities based on the knowledge graph data. Highlight key similarities and differences, shared connections, and what makes each unique. Be concise (2-3 sentences).",
                                    &format!("Compare these entities:\n{}", json_str),
                                ).await;
                                return;
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    "most_connected" => {
                        let limit = params.get("limit").and_then(|v| v.as_str()).and_then(|s| s.parse::<u32>().ok()).unwrap_or(10);
                        let body = serde_json::json!({"limit": limit});
                        api.post_text("/chat/most_connected", &body).await.map(|r| ("engram_most_connected".into(), r)).map_err(|e| e.to_string())
                    }
                    "timeline" => {
                        let body = serde_json::json!({"entity": p("e"), "from_date": p("from"), "to_date": p("to")});
                        api.post_text("/chat/entity_timeline", &body).await.map(|r| ("engram_timeline".into(), r)).map_err(|e| e.to_string())
                    }
                    "date_query" => {
                        let body = serde_json::json!({"entity": p("entity"), "from_date": p("from"), "to_date": p("to")});
                        api.post_text("/chat/temporal_query", &body).await.map(|r| ("engram_timeline".into(), r)).map_err(|e| e.to_string())
                    }
                    "current_state" => {
                        let body = serde_json::json!({"entity": p("entity")});
                        api.post_text("/chat/current_state", &body).await.map(|r| ("engram_current_state".into(), r)).map_err(|e| e.to_string())
                    }
                    "fact_provenance" => {
                        let body = serde_json::json!({"entity": p("entity")});
                        match api.post_text("/chat/fact_provenance", &body).await {
                            Ok(json_str) => {
                                let card_html = cards::render_tool_card("engram_fact_provenance", &json_str);
                                dispatch_graph_data("engram_fact_provenance", &json_str);
                                set_messages.update(|msgs| {
                                    if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) { msgs.remove(pos); }
                                    msgs.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: json_str.chars().take(500).collect(),
                                        display_html: Some(card_html),
                                    });
                                });
                                llm_analysis(&api, set_messages,
                                    "Analyze how information about this entity arrived in the knowledge graph. Describe the information lifecycle: when it first appeared, which sources contributed, whether facts were corroborated. Be concise (2-3 sentences).",
                                    &format!("Analyze provenance:\n{}", json_str),
                                ).await;
                                return;
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    "contradictions" => {
                        let body = serde_json::json!({"entity": p("entity")});
                        match api.post_text("/chat/contradictions", &body).await {
                            Ok(json_str) => {
                                let card_html = cards::render_tool_card("engram_contradictions", &json_str);
                                set_messages.update(|msgs| {
                                    if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) { msgs.remove(pos); }
                                    msgs.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: json_str.chars().take(500).collect(),
                                        display_html: Some(card_html),
                                    });
                                });
                                llm_analysis(&api, set_messages,
                                    "Analyze the contradictions found for this entity. Which claims are stronger based on confidence? What do the conflicting sources suggest? Recommend how to resolve. Be concise (2-3 sentences).",
                                    &format!("Analyze contradictions:\n{}", json_str),
                                ).await;
                                return;
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    "situation_at" => {
                        let body = serde_json::json!({"entity": p("entity"), "date": p("date")});
                        api.post_text("/chat/situation_at", &body).await.map(|r| ("engram_situation_at".into(), r)).map_err(|e| e.to_string())
                    }
                    "provenance" => {
                        let body = serde_json::json!({"entity": p("e")});
                        api.post_text("/provenance", &body).await.map(|r| ("engram_provenance".into(), r)).map_err(|e| e.to_string())
                    }
                    "documents" => {
                        let limit = params.get("limit").and_then(|v| v.as_str()).and_then(|s| s.parse::<u32>().ok()).unwrap_or(20);
                        let body = serde_json::json!({"limit": limit});
                        api.post_text("/documents", &body).await.map(|r| ("engram_documents".into(), r)).map_err(|e| e.to_string())
                    }
                    // Write operations
                    "store" => {
                        let body = serde_json::json!({"entity": p("entity"), "type": p("type"), "confidence": params.get("conf").and_then(|v| v.as_str()).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.7), "source": p("src")});
                        api.post_text("/store", &body).await.map(|r| ("engram_store".into(), r)).map_err(|e| e.to_string())
                    }
                    "relate" => {
                        let body = serde_json::json!({"from": p("from"), "to": p("to"), "relationship": p("rel"), "confidence": params.get("conf").and_then(|v| v.as_str()).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.7), "valid_from": p("vf")});
                        api.post_text("/relate", &body).await.map(|r| ("engram_relate".into(), r)).map_err(|e| e.to_string())
                    }
                    "reinforce" => {
                        // Two-step: first fetch current, then show in chat for user to adjust
                        let entity = p("e");
                        let encoded = js_sys::encode_uri_component(&entity);
                        match api.get_text(&format!("/explain/{}", encoded.as_string().unwrap_or_default())).await {
                            Ok(json_str) => {
                                let card_html = cards::render_tool_card("engram_explain", &json_str);
                                set_messages.update(|msgs| {
                                    if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) { msgs.remove(pos); }
                                    msgs.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: format!("Current state of {}", entity),
                                        display_html: Some(card_html),
                                    });
                                    msgs.push(ChatMessage {
                                        role: ChatRole::System,
                                        content: "To adjust confidence, use: store <entity> with the desired confidence value.".into(),
                                        display_html: None,
                                    });
                                });
                                return;
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    "correct" => {
                        let body = serde_json::json!({"entity": p("e"), "reason": p("reason")});
                        api.post_text("/learn/correct", &body).await.map(|r| ("engram_correct".into(), r)).map_err(|e| e.to_string())
                    }
                    "delete" => {
                        let entity = p("e");
                        let encoded = js_sys::encode_uri_component(&entity);
                        api.delete(&format!("/node/{}", encoded.as_string().unwrap_or_default())).await.map(|r| ("engram_delete".into(), r)).map_err(|e| e.to_string())
                    }
                    "isolated" => {
                        let max_edges = params.get("max").and_then(|v| v.as_str()).and_then(|s| s.parse::<u32>().ok()).unwrap_or(1);
                        let body = serde_json::json!({"max_edges": max_edges});
                        api.post_text("/chat/isolated", &body).await.map(|r| ("engram_isolated".into(), r)).map_err(|e| e.to_string())
                    }
                    // Investigation tools
                    "ingest" => {
                        let text = p("text");
                        let body = serde_json::json!({"items": [text], "source": "chat-ingest"});
                        api.post_text("/ingest", &body).await.map(|r| ("engram_ingest".into(), r)).map_err(|e| e.to_string())
                    }
                    "analyze" => {
                        let text = p("text");
                        match api.post_text("/ingest/analyze", &serde_json::json!({"text": text})).await {
                            Ok(json_str) => {
                                let card_html = cards::render_tool_card("engram_analyze", &json_str);
                                set_messages.update(|msgs| {
                                    if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) { msgs.remove(pos); }
                                    msgs.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: json_str.chars().take(500).collect(),
                                        display_html: Some(card_html),
                                    });
                                });
                                llm_analysis(&api, set_messages,
                                    "Summarize the NER analysis results. Highlight the most significant entities and relationships detected, their types, and confidence levels. Mention anything surprising or noteworthy. Be concise (2-3 sentences).",
                                    &format!("NER analysis results:\n{}", json_str),
                                ).await;
                                return;
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    "investigate" => {
                        let entity = p("entity");
                        // Step 1: Web search
                        let encoded = js_sys::encode_uri_component(&entity);
                        let search_url = format!("/proxy/search?q={}", encoded.as_string().unwrap_or_default());
                        match api.get_text(&search_url).await {
                            Ok(search_json) => {
                                let card_html = cards::render_tool_card("engram_investigate_preview", &search_json);
                                set_messages.update(|msgs| {
                                    if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) { msgs.remove(pos); }
                                    msgs.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: format!("Web search results for: {}", entity),
                                        display_html: Some(card_html),
                                    });
                                });
                                // LLM summary of search findings
                                llm_analysis(&api, set_messages,
                                    "Summarize what was found from the web search about this entity. What new information could be added to the knowledge graph? Be concise (2-3 sentences).",
                                    &format!("Web search results for '{}':\n{}", entity, &search_json[..search_json.len().min(2000)]),
                                ).await;
                                return;
                            }
                            Err(e) => Err(format!("Web search failed: {}", e)),
                        }
                    }
                    "changes" => {
                        let since = p("since");
                        let since_val = if since.is_empty() {
                            // Default: 24 hours ago
                            let _secs = js_sys::Date::now() as u64 / 1000 - 86400;
                            // Simple date calc
                            format!("2026-01-01") // fallback
                        } else { since };
                        let body = serde_json::json!({"since": since_val});
                        match api.post_text("/chat/changes", &body).await {
                            Ok(json_str) => {
                                let card_html = cards::render_tool_card("engram_changes", &json_str);
                                set_messages.update(|msgs| {
                                    if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) { msgs.remove(pos); }
                                    msgs.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: json_str.chars().take(500).collect(),
                                        display_html: Some(card_html),
                                    });
                                });
                                llm_analysis(&api, set_messages,
                                    "Summarize the recent changes to the knowledge graph. What entities were added or modified? What does this suggest about evolving knowledge? Be concise (2-3 sentences).",
                                    &format!("Recent graph changes:\n{}", json_str),
                                ).await;
                                return;
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    "watch" => {
                        let entity = p("entity");
                        let body = serde_json::json!({"entity": entity});
                        api.post_text("/chat/watch", &body).await.map(|r| ("engram_watch".into(), r)).map_err(|e| e.to_string())
                    }
                    "network_analysis" => {
                        let entity = p("entity");
                        let depth = params.get("depth").and_then(|v| v.as_str()).and_then(|s| s.parse::<u32>().ok()).unwrap_or(2);
                        let body = serde_json::json!({"entity": entity, "depth": depth});
                        match api.post_text("/chat/network_analysis", &body).await {
                            Ok(json_str) => {
                                let card_html = cards::render_tool_card("engram_network_analysis", &json_str);
                                dispatch_graph_data("engram_network_analysis", &json_str);
                                set_messages.update(|msgs| {
                                    if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) { msgs.remove(pos); }
                                    msgs.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: json_str.chars().take(500).collect(),
                                        display_html: Some(card_html),
                                    });
                                });
                                llm_analysis(&api, set_messages,
                                    "Analyze this entity's network. Describe the key connections at each hop level, identify important intermediaries, and explain the entity's position in the broader knowledge graph. Be concise (2-3 sentences).",
                                    &format!("Network analysis:\n{}", &json_str[..json_str.len().min(3000)]),
                                ).await;
                                return;
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    "entity_360" => {
                        let entity = p("entity");
                        let body = serde_json::json!({"entity": entity});
                        api.post_text("/chat/entity_360", &body).await.map(|r| ("engram_entity_360".into(), r)).map_err(|e| e.to_string())
                    }
                    "entity_gaps" => {
                        let entity = p("entity");
                        let body = serde_json::json!({"entity": entity});
                        api.post_text("/chat/entity_gaps", &body).await.map(|r| ("engram_entity_gaps".into(), r)).map_err(|e| e.to_string())
                    }
                    // Reasoning tools
                    "what_if" => {
                        let entity = p("entity");
                        let conf = params.get("conf").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.20);
                        let body = serde_json::json!({"entity": entity, "new_confidence": conf, "depth": 2});
                        match api.post_text("/chat/what_if", &body).await {
                            Ok(json_str) => {
                                let card_html = cards::render_tool_card("engram_what_if", &json_str);
                                set_messages.update(|msgs| {
                                    if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) { msgs.remove(pos); }
                                    msgs.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: json_str.chars().take(500).collect(),
                                        display_html: Some(card_html),
                                    });
                                });
                                llm_analysis(&api, set_messages,
                                    "Analyze this what-if simulation. Explain the strategic implications of the confidence cascade. Which entities are most affected and why? What does this mean for the broader knowledge landscape? Be concise (2-3 sentences).",
                                    &format!("What-if simulation results:\n{}", &json_str[..json_str.len().min(3000)]),
                                ).await;
                                return;
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    "influence" => {
                        let from = p("from");
                        let to = p("to");
                        let body = serde_json::json!({"from": from, "to": to, "max_depth": 4});
                        match api.post_text("/chat/influence_path", &body).await {
                            Ok(json_str) => {
                                let card_html = cards::render_tool_card("engram_influence_path", &json_str);
                                dispatch_graph_data("engram_influence_path", &json_str);
                                set_messages.update(|msgs| {
                                    if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) { msgs.remove(pos); }
                                    msgs.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: json_str.chars().take(500).collect(),
                                        display_html: Some(card_html),
                                    });
                                });
                                llm_analysis(&api, set_messages,
                                    "Analyze these influence paths between two entities. Explain the different influence channels/mechanisms. Which path is most significant and why? Be concise (2-3 sentences).",
                                    &format!("Influence paths:\n{}", &json_str[..json_str.len().min(3000)]),
                                ).await;
                                return;
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    "black_areas" => {
                        match api.get_text("/reason/gaps").await {
                            Ok(json_str) => {
                                let card_html = cards::render_tool_card("engram_black_areas", &json_str);
                                set_messages.update(|msgs| {
                                    if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) { msgs.remove(pos); }
                                    msgs.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: json_str.chars().take(500).collect(),
                                        display_html: Some(card_html),
                                    });
                                });
                                llm_analysis(&api, set_messages,
                                    "Analyze these knowledge gaps and blind spots. Which gaps are most critical? What investigations would fill them? Prioritize by severity. Be concise (2-3 sentences).",
                                    &format!("Knowledge gaps detected:\n{}", &json_str[..json_str.len().min(3000)]),
                                ).await;
                                return;
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    // Reporting tools
                    "briefing" => {
                        let topic = p("topic");
                        let body = serde_json::json!({"topic": topic, "depth": "standard"});
                        match api.post_text("/chat/briefing", &body).await {
                            Ok(json_str) => {
                                let card_html = cards::render_tool_card("engram_briefing", &json_str);
                                dispatch_graph_data("engram_briefing", &json_str);
                                set_messages.update(|msgs| {
                                    if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) { msgs.remove(pos); }
                                    msgs.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: json_str.chars().take(500).collect(),
                                        display_html: Some(card_html),
                                    });
                                });
                                llm_analysis(&api, set_messages,
                                    "Generate a structured briefing from this knowledge graph data. Include: key entities and their roles, major relationships and dynamics, temporal context where available, and confidence assessment. Format with clear sections.",
                                    &format!("Generate briefing on '{}':\n{}", topic, &json_str[..json_str.len().min(4000)]),
                                ).await;
                                return;
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    "export" => {
                        let entity = p("entity");
                        let depth = params.get("depth").and_then(|v| v.as_str()).and_then(|s| s.parse::<u32>().ok()).unwrap_or(2);
                        let body = serde_json::json!({"entity": entity, "depth": depth});
                        api.post_text("/chat/export_subgraph", &body).await.map(|r| ("engram_export".into(), r)).map_err(|e| e.to_string())
                    }
                    "dossier" => {
                        let entity = p("entity");
                        let body = serde_json::json!({"entity": entity});
                        match api.post_text("/chat/dossier", &body).await {
                            Ok(json_str) => {
                                let card_html = cards::render_tool_card("engram_dossier", &json_str);
                                dispatch_graph_data("engram_dossier", &json_str);
                                set_messages.update(|msgs| {
                                    if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) { msgs.remove(pos); }
                                    msgs.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: json_str.chars().take(500).collect(),
                                        display_html: Some(card_html),
                                    });
                                });
                                llm_analysis(&api, set_messages,
                                    "Generate a comprehensive dossier report on this entity based on the knowledge graph data. Include an executive summary, key relationships, notable properties, temporal context, and any gaps or areas needing further investigation.",
                                    &format!("Generate dossier on '{}':\n{}", entity, &json_str[..json_str.len().min(4000)]),
                                ).await;
                                return;
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    "topic_map" => {
                        let topic = p("topic");
                        let body = serde_json::json!({"topic": topic});
                        api.post_text("/chat/topic_map", &body).await.map(|r| ("engram_topic_map".into(), r)).map_err(|e| e.to_string())
                    }
                    "graph_stats" => {
                        let body = serde_json::json!({});
                        api.post_text("/chat/graph_stats", &body).await.map(|r| ("engram_graph_stats".into(), r)).map_err(|e| e.to_string())
                    }
                    // Assessment tools
                    "assess_create" => {
                        let body = serde_json::json!({
                            "title": p("title"),
                            "description": p("description"),
                            "initial_probability": params.get("probability").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.50),
                            "success_criteria": vec![p("criteria")].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>(),
                            "watches": Vec::<String>::new(),
                            "tags": Vec::<String>::new(),
                        });
                        match api.post_text("/assessments", &body).await {
                            Ok(json_str) => {
                                let card_html = cards::render_tool_card("engram_assess_create", &json_str);
                                set_messages.update(|msgs| {
                                    if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) { msgs.remove(pos); }
                                    msgs.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: json_str.chars().take(500).collect(),
                                        display_html: Some(card_html),
                                    });
                                });
                                llm_analysis(&api, set_messages,
                                    "An assessment/hypothesis has been created. Briefly describe what this assessment tracks, what the initial probability suggests, and recommend what evidence to look for next. Be concise (2-3 sentences).",
                                    &format!("Assessment created:\n{}", json_str),
                                ).await;
                                return;
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    "assess_evidence" => {
                        let assessment = p("assessment");
                        let encoded = js_sys::encode_uri_component(&assessment);
                        let body = serde_json::json!({
                            "node_label": p("text"),
                            "direction": p("direction"),
                        });
                        match api.post_text(&format!("/assessments/{}/evidence", encoded.as_string().unwrap_or_default()), &body).await {
                            Ok(json_str) => {
                                let card_html = cards::render_tool_card("engram_assess_evidence", &json_str);
                                set_messages.update(|msgs| {
                                    if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) { msgs.remove(pos); }
                                    msgs.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: json_str.chars().take(500).collect(),
                                        display_html: Some(card_html),
                                    });
                                });
                                llm_analysis(&api, set_messages,
                                    "Evidence has been added to an assessment. Explain how this evidence shifts the probability, whether the direction is significant, and what this means for the hypothesis. Be concise (2-3 sentences).",
                                    &format!("Evidence added:\n{}", json_str),
                                ).await;
                                return;
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    "assess_evaluate" => {
                        let assessment = p("assessment");
                        let encoded = js_sys::encode_uri_component(&assessment);
                        match api.post_text(&format!("/assessments/{}/evaluate", encoded.as_string().unwrap_or_default()), &serde_json::json!({})).await {
                            Ok(json_str) => {
                                let card_html = cards::render_tool_card("engram_assess_evaluate", &json_str);
                                set_messages.update(|msgs| {
                                    if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) { msgs.remove(pos); }
                                    msgs.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: json_str.chars().take(500).collect(),
                                        display_html: Some(card_html),
                                    });
                                });
                                llm_analysis(&api, set_messages,
                                    "An assessment has been re-evaluated. Analyze the probability shift, whether it moved toward confirmation or rejection, and what the current probability implies. Be concise (2-3 sentences).",
                                    &format!("Assessment evaluated:\n{}", json_str),
                                ).await;
                                return;
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    "assess_list" => {
                        api.get_text("/assessments").await.map(|r| ("engram_assess_list".into(), r)).map_err(|e| e.to_string())
                    }
                    "assess_detail" => {
                        let assessment = p("assessment");
                        let encoded = js_sys::encode_uri_component(&assessment);
                        match api.get_text(&format!("/assessments/{}", encoded.as_string().unwrap_or_default())).await {
                            Ok(json_str) => {
                                let card_html = cards::render_tool_card("engram_assess_detail", &json_str);
                                set_messages.update(|msgs| {
                                    if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) { msgs.remove(pos); }
                                    msgs.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: json_str.chars().take(500).collect(),
                                        display_html: Some(card_html),
                                    });
                                });
                                llm_analysis(&api, set_messages,
                                    "Analyze this assessment in detail. Describe the balance of evidence for and against, the probability trend from history, what the watched entities suggest, and recommend next steps. Be concise (3-4 sentences).",
                                    &format!("Assessment detail:\n{}", json_str),
                                ).await;
                                return;
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    "assess_compare" => {
                        let a = p("a");
                        let b = p("b");
                        let enc_a = js_sys::encode_uri_component(&a);
                        let enc_b = js_sys::encode_uri_component(&b);
                        match api.get_text(&format!("/assessments/compare/{}/{}", enc_a.as_string().unwrap_or_default(), enc_b.as_string().unwrap_or_default())).await {
                            Ok(json_str) => {
                                let card_html = cards::render_tool_card("engram_assess_compare", &json_str);
                                set_messages.update(|msgs| {
                                    if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) { msgs.remove(pos); }
                                    msgs.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: json_str.chars().take(500).collect(),
                                        display_html: Some(card_html),
                                    });
                                });
                                llm_analysis(&api, set_messages,
                                    "Compare these two assessments/hypotheses. Which has stronger evidence? Which is more likely? Are they mutually exclusive or could both be true? Recommend which to investigate further. Be concise (3-4 sentences).",
                                    &format!("Assessment comparison:\n{}", json_str),
                                ).await;
                                return;
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    _ => Err(format!("Unknown tool: {}", tool)),
                };

                // Update inline result feedback on the card
                let result_id = format!("tc-{}-result", tool);
                let btn_id = format!("tc-{}-btn", tool);
                match result {
                    Ok((card_tool, json_str)) => {
                        // Show inline success on the card
                        let _ = js_sys::eval(&format!(
                            "var r=document.getElementById('{}');if(r){{r.style.display='block';\
                             r.style.background='rgba(102,187,106,0.15)';r.style.color='#66bb6a';\
                             r.innerHTML='<i class=\"fa-solid fa-check\"></i> Done';}}\
                             var b=document.getElementById('{}');if(b){{b.disabled=true;b.style.opacity='0.5';}}",
                            result_id, btn_id,
                        ));
                        let card_html = cards::render_tool_card(&card_tool, &json_str);
                        dispatch_graph_data(&card_tool, &json_str);
                        set_messages.update(|msgs| {
                            // Remove "Running..." message
                            if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) {
                                msgs.remove(pos);
                            }
                            msgs.push(ChatMessage {
                                role: ChatRole::ToolResult,
                                content: if json_str.len() > 500 { format!("{}...", &json_str[..500]) } else { json_str.clone() },
                                display_html: Some(card_html),
                            });
                        });

                        // After successful store: offer a relate card to connect the new entity
                        if tool == "store" {
                            let entity_name = params.get("entity").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            if !entity_name.is_empty() {
                                if let Some(relate_html) = tool_cards::generate_tool_card("relate") {
                                    set_messages.update(|msgs| {
                                        msgs.push(ChatMessage {
                                            role: ChatRole::System,
                                            content: format!("Connect {} to make it visible in the graph:", entity_name),
                                            display_html: None,
                                        });
                                        msgs.push(ChatMessage {
                                            role: ChatRole::Assistant,
                                            content: "Tool: relate".to_string(),
                                            display_html: Some(relate_html),
                                        });
                                    });
                                    // Pre-fill "from" field and wire up autocomplete
                                    help::ensure_card_helpers();
                                    let ac_fields = tool_cards::autocomplete_fields("relate");
                                    let escaped = entity_name.replace('\'', "\\'");
                                    let ac_js: String = ac_fields.iter()
                                        .map(|(id, endpoint)| format!("__ec_suggest('{}','{}','label');", id, endpoint))
                                        .collect();
                                    let code = format!(
                                        "setTimeout(function(){{\
                                         var el=document.getElementById('tc-relate-from');if(el)el.value='{}';\
                                         {}}},100);",
                                        escaped, ac_js,
                                    );
                                    let _ = js_sys::eval(&code);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        // Show inline error on the card
                        let escaped = e.replace('\'', "\\'").replace('<', "&lt;");
                        let _ = js_sys::eval(&format!(
                            "var r=document.getElementById('{}');if(r){{r.style.display='block';\
                             r.style.background='rgba(239,83,80,0.15)';r.style.color='#ef5350';\
                             r.innerHTML='<i class=\"fa-solid fa-xmark\"></i> {}'}}",
                            result_id, escaped,
                        ));
                        set_messages.update(|msgs| {
                            if let Some(pos) = msgs.iter().rposition(|m| m.role == ChatRole::Context) {
                                msgs.remove(pos);
                            }
                            msgs.push(ChatMessage {
                                role: ChatRole::System,
                                content: format!("Error: {}", e),
                                display_html: None,
                            });
                        });
                    }
                }
            });
        }
    }) as Box<dyn FnMut(web_sys::CustomEvent)>);
    let _ = web_sys::window().unwrap().add_event_listener_with_callback(
        "engram-run-tool",
        cb.as_ref().unchecked_ref(),
    );
    cb.forget();
}
