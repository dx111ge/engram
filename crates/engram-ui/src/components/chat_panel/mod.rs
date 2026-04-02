//! Chat panel: intelligence analyst workbench.
//!
//! Split into sub-modules:
//! - `context` -- entity extraction & context retrieval
//! - `tools` -- tool name -> API endpoint mapping
//! - `view` -- message rendering & follow-up extraction
//! - `markdown` -- markdown-to-HTML converter
//! - `cards` -- tool result card HTML generators

pub mod context;
pub mod tools;
pub mod view;
pub mod markdown;
pub mod cards;
pub mod intent;
pub mod tool_cards;

use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api::ApiClient;
use crate::api::types::{LlmMessage, LlmProxyRequest, LlmProxyResponse};
use crate::components::chat_types::*;

mod help;

// ── Helper: get current pathname ──

fn current_pathname() -> String {
    web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .unwrap_or_default()
}

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

    let llm_result: Result<LlmProxyResponse, _> = api.post("/proxy/llm", &llm_req).await;

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

// ── Convert chat messages to LLM wire format ──

fn build_llm_messages(messages: &[ChatMessage]) -> Vec<LlmMessage> {
    messages
        .iter()
        .filter(|m| {
            // Skip context/write-confirmation messages in LLM conversation
            m.role != ChatRole::Context && m.role != ChatRole::WriteConfirmation
        })
        .map(|m| {
            let role = match &m.role {
                ChatRole::User => "user",
                ChatRole::Assistant => "assistant",
                ChatRole::System => "system",
                ChatRole::ToolResult => "tool",
                _ => "system",
            };
            LlmMessage {
                role: role.to_string(),
                content: serde_json::Value::String(m.content.clone()),
            }
        })
        .collect()
}

// ── Main component ──

#[component]
pub fn ChatPanel(
    /// When true, renders inline (for Explore page). When false, renders as floating panel.
    #[prop(default = false)]
    embedded: bool,
) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let chat_open =
        use_context::<RwSignal<bool>>().expect("chat_open context");

    let (messages, set_messages) = signal(Vec::<ChatMessage>::new());
    let (input_text, set_input_text) = signal(String::new());
    let (sending, set_sending) = signal(false);
    let (pending_writes, set_pending_writes) = signal(Vec::<PendingWrite>::new());
    let (follow_ups, set_follow_ups) = signal(Vec::<FollowUpSuggestion>::new());
    let (slash_suggestions, set_slash_suggestions) =
        signal(Vec::<(&'static str, &'static str, &'static str)>::new());

    // Page context signals (set by GraphPage / InsightsPage)
    let chat_selected_node = use_context::<ChatSelectedNode>();
    let chat_assessment = use_context::<ChatCurrentAssessment>();

    // Page-aware visibility: on /graph the chat is always embedded, not floating
    let page_visible = move || {
        let path = current_pathname();
        page_allows_chat(&path)
    };

    let is_explore_page = move || current_pathname() == "/graph";

    // ── Send message flow ──

    let api_send = api.clone();
    let send_message = move || {
        let text = input_text.get_untracked().trim().to_string();
        if text.is_empty() || sending.get_untracked() {
            return;
        }
        set_input_text.set(String::new());
        set_sending.set(true);
        set_follow_ups.set(Vec::new());
        set_slash_suggestions.set(Vec::new());

        // Add user message
        set_messages.update(|msgs| {
            msgs.push(ChatMessage {
                role: ChatRole::User,
                content: text.clone(),
                display_html: None,
            });
        });

        // Intercept /help commands -- respond locally, no LLM needed
        if text.starts_with("/help") {
            let help_text = help::generate_help_response(&text);
            let help_html = help::generate_help_html(&text);
            set_messages.update(|msgs| {
                msgs.push(ChatMessage {
                    role: ChatRole::Assistant,
                    content: help_text,
                    display_html: Some(help_html),
                });
            });
            set_sending.set(false);
            return;
        }

        // Intercept /path commands -- render interactive path search card
        if text.starts_with("/path") {
            let arg = text.strip_prefix("/path").unwrap_or("").trim().to_string();
            let path_html = help::generate_path_card(&arg);
            set_messages.update(|msgs| {
                msgs.push(ChatMessage {
                    role: ChatRole::Assistant,
                    content: "Path search card".to_string(),
                    display_html: Some(path_html),
                });
            });
            set_sending.set(false);
            return;
        }

        // ── Intent-based routing: keyword detection → show tool card ──
        // No LLM tool calls. Cards call the API directly.

        let detected = intent::detect_intent(&text);

        // For tools that have interactive cards, show the card with pre-filled values
        if let Some(card_html) = help::generate_tool_card(detected.tool) {
            // For two-entity tools, try to pre-fill both fields via JS after rendering
            let prefill_js = if !detected.prefill.is_empty() {
                let p1 = detected.prefill.replace('\'', "\\'");
                let p2 = detected.prefill2.replace('\'', "\\'");
                let id_prefix = match detected.tool {
                    "query" => "tc-query-entity",
                    "search" => "tc-search-q",
                    "explain" => "tc-explain-e",
                    "similar" => "tc-similar-t",
                    "compare" => "tc-compare-a",
                    "shortest_path" => "tc-sp-from",
                    "provenance" => "tc-provenance-e",
                    "date_query" => "tc-dq-entity",
                    "current_state" => "tc-cs-entity",
                    "fact_provenance" => "tc-fp-entity",
                    "contradictions" => "tc-ct-entity",
                    "situation_at" => "tc-sa-entity",
                    _ => "",
                };
                if !id_prefix.is_empty() {
                    let mut js = format!(
                        "requestAnimationFrame(function(){{var el=document.getElementById('{}');if(el)el.value='{}';",
                        id_prefix, p1,
                    );
                    // Pre-fill second field for compare/path
                    if !p2.is_empty() {
                        let id2 = match detected.tool {
                            "compare" => "tc-compare-b",
                            "shortest_path" => "tc-sp-to",
                            "situation_at" => "tc-sa-date",
                            "date_query" => "tc-dq-from",
                            _ => "",
                        };
                        if !id2.is_empty() {
                            js.push_str(&format!(
                                "var el2=document.getElementById('{}');if(el2)el2.value='{}';",
                                id2, p2,
                            ));
                        }
                    }
                    js.push_str("});");
                    js
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            set_messages.update(|msgs| {
                msgs.push(ChatMessage {
                    role: ChatRole::Assistant,
                    content: format!("Tool: {}", detected.tool),
                    display_html: Some(card_html),
                });
            });

            // Inject shared JS helpers and wire up autocomplete
            help::ensure_card_helpers();
            let ac_fields = tool_cards::autocomplete_fields(detected.tool);
            if !ac_fields.is_empty() || !prefill_js.is_empty() {
                let ac_js: String = ac_fields.iter()
                    .map(|(id, endpoint)| format!("__ec_suggest('{}','{}','label');", id, endpoint))
                    .collect();
                let code = format!("requestAnimationFrame(function(){{{}{}}});", prefill_js, ac_js);
                let _ = js_sys::eval(&code);
            }

            set_sending.set(false);
            return;
        }

        // For tools without cards (gaps, most_connected, isolated, category commands),
        // execute directly via API
        let api = api_send.clone();
        let tool = detected.tool.to_string();
        let prefill = detected.prefill.clone();
        spawn_local(async move {
            let result: Result<String, _> = match tool.as_str() {
                "gaps" => api.get_text("/reason/gaps").await,
                "most_connected" => {
                    let body = serde_json::json!({"limit": 10});
                    api.post_text("/chat/most_connected", &body).await
                }
                "isolated" => {
                    let body = serde_json::json!({"max_edges": 1});
                    api.post_text("/chat/isolated", &body).await
                }
                "analyze" | "knowledge" | "investigate" => {
                    // Category commands: run explain + query for the entity
                    let encoded = js_sys::encode_uri_component(&prefill);
                    let explain_result = api.get_text(
                        &format!("/explain/{}", encoded.as_string().unwrap_or_default())
                    ).await.unwrap_or_default();

                    let explain_card = cards::render_tool_card("engram_explain", &explain_result);
                    dispatch_graph_data("engram_explain", &explain_result);
                    set_messages.update(|msgs| {
                        msgs.push(ChatMessage {
                            role: ChatRole::ToolResult,
                            content: format!("Explain: {}", prefill),
                            display_html: Some(explain_card),
                        });
                    });

                    // Also run query for graph
                    let body = serde_json::json!({"query": prefill, "depth": 2, "direction": "both", "limit": 100});
                    let query_result = api.post_text("/query", &body).await.unwrap_or_default();
                    dispatch_graph_data("engram_query", &query_result);

                    let query_card = cards::render_tool_card("engram_query", &query_result);
                    set_messages.update(|msgs| {
                        msgs.push(ChatMessage {
                            role: ChatRole::ToolResult,
                            content: format!("Query: {}", prefill),
                            display_html: Some(query_card),
                        });
                    });

                    set_sending.set(false);
                    return;
                }
                "briefing" => {
                    let body = serde_json::json!({"topic": prefill, "depth": 2, "format": "structured"});
                    api.post_text("/chat/briefing", &body).await
                }
                "timeline" => {
                    let body = serde_json::json!({"entity": prefill, "limit": 20});
                    api.post_text("/chat/timeline", &body).await
                }
                "what_if" => {
                    // Show parameter card for what-if (needs entity + confidence)
                    set_messages.update(|msgs| {
                        msgs.push(ChatMessage {
                            role: ChatRole::Assistant,
                            content: "What-if analysis requires entity and new confidence. Use the what-if tool card.".to_string(),
                            display_html: None,
                        });
                    });
                    set_sending.set(false);
                    return;
                }
                _ => {
                    // Unknown tool -- show as system message
                    set_messages.update(|msgs| {
                        msgs.push(ChatMessage {
                            role: ChatRole::System,
                            content: format!("Unknown command: {}. Type /help for available commands.", tool),
                            display_html: None,
                        });
                    });
                    set_sending.set(false);
                    return;
                }
            };

            match result {
                Ok(json_str) => {
                    let card_tool = format!("engram_{}", tool);
                    let card_html = cards::render_tool_card(&card_tool, &json_str);
                    dispatch_graph_data(&card_tool, &json_str);
                    set_messages.update(|msgs| {
                        msgs.push(ChatMessage {
                            role: ChatRole::ToolResult,
                            content: if json_str.len() > 500 { format!("{}...", &json_str[..500]) } else { json_str },
                            display_html: Some(card_html),
                        });
                    });
                }
                Err(e) => {
                    set_messages.update(|msgs| {
                        msgs.push(ChatMessage {
                            role: ChatRole::System,
                            content: format!("API call failed: {e}"),
                            display_html: None,
                        });
                    });
                }
            }
            set_sending.set(false);
        });
    };

    // ── Apply pending writes ──

    let api_apply = api.clone();
    let apply_writes = StoredValue::new(Callback::new(move |_: leptos::ev::MouseEvent| {
        let writes = pending_writes.get_untracked();
        let selected: Vec<PendingWrite> = writes.into_iter().filter(|w| w.selected).collect();
        if selected.is_empty() {
            set_pending_writes.set(Vec::new());
            return;
        }

        let api = api_apply.clone();
        spawn_local(async move {
            let mut results = Vec::new();
            for w in &selected {
                let result = tools::execute_tool(&api, &w.tool_name, &w.args).await;
                results.push(format!("{}: {}", w.label, if result.contains("error") { "FAILED" } else { "OK" }));
            }

            set_messages.update(|msgs| {
                msgs.push(ChatMessage {
                    role: ChatRole::System,
                    content: format!("Applied {} changes:\n{}", selected.len(), results.join("\n")),
                    display_html: None,
                });
            });
            set_pending_writes.set(Vec::new());
        });
    }));

    let reject_writes = StoredValue::new(Callback::new(move |_: leptos::ev::MouseEvent| {
        set_pending_writes.set(Vec::new());
        set_messages.update(|msgs| {
            msgs.push(ChatMessage {
                role: ChatRole::System,
                content: "All proposed changes rejected.".to_string(),
                display_html: None,
            });
        });
    }));

    // ── Toggle write checkbox ──

    let toggle_write = move |idx: usize| {
        set_pending_writes.update(|writes| {
            if let Some(w) = writes.get_mut(idx) {
                w.selected = !w.selected;
            }
        });
    };

    // ── Clear messages ──

    let clear_messages = move |_: leptos::ev::MouseEvent| {
        set_messages.set(Vec::new());
        set_pending_writes.set(Vec::new());
        set_follow_ups.set(Vec::new());
    };

    // ── Toggle panel ──

    let toggle_panel = move |_: leptos::ev::MouseEvent| {
        chat_open.update(|v| *v = !*v);
    };

    let close_panel = move |_: leptos::ev::MouseEvent| {
        chat_open.set(false);
    };

    // ── Keyboard handler ──

    let send_clone = send_message.clone();
    let on_keydown = StoredValue::new(Callback::new(move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() == "Enter" && !ev.shift_key() {
            ev.prevent_default();
            send_clone();
        }
    }));

    // ── Quick actions ──

    let send_for_qa1 = send_message.clone();
    let send_for_qa2 = send_message.clone();
    let send_for_qa3 = send_message.clone();
    let send_for_qa4 = send_message.clone();

    let qa_know = StoredValue::new(Callback::new(move |_: leptos::ev::MouseEvent| {
        // If a node is selected on Explore, query about it specifically
        let selected = chat_selected_node.and_then(|s| s.0.get_untracked());
        let text = match selected {
            Some(node) => format!("What do I know about {}?", node),
            None => "What are the most important entities in the graph?".to_string(),
        };
        set_input_text.set(text);
        send_for_qa1();
    }));
    let qa_gaps = StoredValue::new(Callback::new(move |_: leptos::ev::MouseEvent| {
        set_input_text.set("Find the most critical gaps in my knowledge and suggest investigations".to_string());
        send_for_qa2();
    }));
    let qa_whatif = StoredValue::new(Callback::new(move |_: leptos::ev::MouseEvent| {
        let selected = chat_selected_node.and_then(|s| s.0.get_untracked());
        let text = match selected {
            Some(node) => format!("What if {} confidence drops to 20%? Show me the cascade.", node),
            None => "What are the most connected entities and how would their removal affect the graph?".to_string(),
        };
        set_input_text.set(text);
        send_for_qa3();
    }));

    let qa_help = StoredValue::new(Callback::new(move |_: leptos::ev::MouseEvent| {
        set_input_text.set("/help".to_string());
        send_for_qa4();
    }));

    // ── Listen for chat-send events from HTML cards (e.g. help category clicks) ──

    {
        let send_for_event = send_message.clone();
        let cb = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web_sys::CustomEvent| {
            if let Some(text) = ev.detail().as_string() {
                set_input_text.set(text);
                send_for_event();
            }
        }) as Box<dyn FnMut(web_sys::CustomEvent)>);
        let _ = web_sys::window().unwrap().add_event_listener_with_callback(
            "engram-chat-send",
            cb.as_ref().unchecked_ref(),
        );
        cb.forget();
    }

    // ── Listen for tool-card events (show interactive parameter form) ──

    {
        let cb = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web_sys::CustomEvent| {
            if let Some(tool_name) = ev.detail().as_string() {
                if let Some(card_html) = tool_cards::generate_tool_card(&tool_name) {
                    set_messages.update(|msgs| {
                        msgs.push(ChatMessage {
                            role: ChatRole::Assistant,
                            content: format!("Tool: {}", tool_name),
                            display_html: Some(card_html),
                        });
                    });
                    // Inject shared JS helpers and wire up autocomplete
                    help::ensure_card_helpers();
                    let ac_fields = tool_cards::autocomplete_fields(&tool_name);
                    if !ac_fields.is_empty() {
                        let ac_js: String = ac_fields.iter()
                            .map(|(id, endpoint)| format!("__ec_suggest('{}','{}','label');", id, endpoint))
                            .collect();
                        let code = format!("requestAnimationFrame(function(){{{}}});", ac_js);
                        let _ = js_sys::eval(&code);
                    }
                }
            }
        }) as Box<dyn FnMut(web_sys::CustomEvent)>);
        let _ = web_sys::window().unwrap().add_event_listener_with_callback(
            "engram-tool-card",
            cb.as_ref().unchecked_ref(),
        );
        cb.forget();
    }

    // ── Listen for tool-result events (direct API call results) ──

    {
        let cb = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web_sys::CustomEvent| {
            if let Some(json_str) = ev.detail().as_string() {
                // Try to detect which tool produced this result for proper card rendering
                let tool_name = if json_str.contains("\"results\"") {
                    "engram_search"
                } else if json_str.contains("\"edges_from\"") || json_str.contains("\"edges_to\"") {
                    "engram_explain"
                } else if json_str.contains("\"nodes\"") && json_str.contains("\"edges\"") {
                    "engram_query"
                } else if json_str.contains("\"paths\"") {
                    "engram_shortest_path"
                } else if json_str.contains("\"entities\"") {
                    "engram_most_connected"
                } else {
                    "engram_query" // default
                };

                let card_html = cards::render_tool_card(tool_name, &json_str);
                dispatch_graph_data(tool_name, &json_str);

                set_messages.update(|msgs| {
                    msgs.push(ChatMessage {
                        role: ChatRole::ToolResult,
                        content: json_str.chars().take(500).collect(),
                        display_html: Some(card_html),
                    });
                });
            }
        }) as Box<dyn FnMut(web_sys::CustomEvent)>);
        let _ = web_sys::window().unwrap().add_event_listener_with_callback(
            "engram-tool-result",
            cb.as_ref().unchecked_ref(),
        );
        cb.forget();
    }

    // ── Listen for engram-run-tool events (async tool execution from cards) ──

    {
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
                            let body = serde_json::json!({"entity": p("e"), "limit": 20});
                            api.post_text("/chat/timeline", &body).await.map(|r| ("engram_timeline".into(), r)).map_err(|e| e.to_string())
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

    // ── Listen for LLM summary events (explain enrichment) ──

    {
        let cb = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web_sys::CustomEvent| {
            if let Some(text) = ev.detail().as_string() {
                let rendered_html = markdown::markdown_to_html(&text);
                set_messages.update(|msgs| {
                    msgs.push(ChatMessage {
                        role: ChatRole::Assistant,
                        content: text,
                        display_html: Some(rendered_html),
                    });
                });
            }
        }) as Box<dyn FnMut(web_sys::CustomEvent)>);
        let _ = web_sys::window().unwrap().add_event_listener_with_callback(
            "engram-llm-summary",
            cb.as_ref().unchecked_ref(),
        );
        cb.forget();
    }

    // ── Auto-scroll chat to bottom when messages change ──

    Effect::new(move |_| {
        let _count = messages.get().len(); // subscribe to changes
        // Use requestAnimationFrame to scroll after DOM update
        let _ = js_sys::eval(
            "requestAnimationFrame(function(){var el=document.querySelector('.chat-messages');\
             if(el)el.scrollTop=el.scrollHeight;})"
        );
    });

    // ── Follow-up click ──

    let send_for_fu = send_message.clone();
    let on_follow_up = StoredValue::new(Callback::new(move |text: String| {
        set_input_text.set(text);
        send_for_fu();
    }));

    // ── Send button ──

    let send_for_btn = send_message.clone();
    let on_send_click = StoredValue::new(Callback::new(move |_: leptos::ev::MouseEvent| {
        send_for_btn();
    }));

    // ── Render chat panel internals (reused by both floating and embedded modes) ──

    let render_chat_internals = move || {
        view! {
            // Header
            <div class="chat-header"
                style="display:flex;align-items:center;justify-content:space-between;\
                       padding:0.75rem 1rem;border-bottom:1px solid var(--border, #2d3139);flex-shrink:0;"
            >
                <div style="display:flex;align-items:center;gap:0.5rem;font-weight:600;">
                    <i class="fa-solid fa-brain" style="color:var(--accent, #4a9eff);"></i>
                    <span>"Knowledge Chat"</span>
                </div>
                <div style="display:flex;gap:0.5rem;">
                    <button class="btn btn-sm btn-ghost" on:click=clear_messages title="Clear chat"
                        style="background:none;border:none;color:var(--text-muted, #8b8fa3);\
                               cursor:pointer;padding:0.25rem 0.5rem;font-size:0.85rem;">
                        <i class="fa-solid fa-trash-can"></i>
                    </button>
                    // Close button only in floating mode (not on Explore)
                    {move || if !is_explore_page() {
                        Some(view! {
                            <button class="btn btn-sm btn-ghost" on:click=close_panel title="Close"
                                style="background:none;border:none;color:var(--text-muted, #8b8fa3);\
                                       cursor:pointer;padding:0.25rem 0.5rem;font-size:0.85rem;">
                                <i class="fa-solid fa-xmark"></i>
                            </button>
                        })
                    } else { None }}
                </div>
            </div>

            // Message list
            <div class="chat-messages"
                style="flex:1;overflow-y:auto;padding:0.75rem;display:flex;\
                       flex-direction:column;gap:0.5rem;"
            >
                {move || {
                    let msgs = messages.get();
                    if msgs.is_empty() {
                        // Empty state with quick actions
                        view! {
                            <div class="chat-empty"
                                style="display:flex;flex-direction:column;align-items:center;\
                                       justify-content:center;flex:1;gap:1rem;\
                                       color:var(--text-muted, #8b8fa3);text-align:center;padding:2rem 1rem;"
                            >
                                <i class="fa-solid fa-comments" style="font-size:2.5rem;opacity:0.3;"></i>
                                <p style="margin:0;font-size:0.9rem;">
                                    "Ask questions, analyze entities, compare connections, or investigate gaps."
                                </p>
                                <div class="quick-actions"
                                    style="display:flex;flex-direction:column;gap:0.5rem;width:100%;max-width:300px;">
                                    <button class="btn btn-outline btn-sm"
                                        on:click=move |ev| qa_know.with_value(|cb| cb.run(ev))
                                        style="text-align:left;padding:0.5rem 0.75rem;\
                                               border:1px solid var(--border, #2d3139);background:transparent;\
                                               color:var(--text, #c9ccd3);border-radius:6px;cursor:pointer;font-size:0.8rem;">
                                        <i class="fa-solid fa-search" style="margin-right:0.5rem;color:var(--accent, #4a9eff);"></i>
                                        "What do I know about..."
                                    </button>
                                    <button class="btn btn-outline btn-sm"
                                        on:click=move |ev| qa_gaps.with_value(|cb| cb.run(ev))
                                        style="text-align:left;padding:0.5rem 0.75rem;\
                                               border:1px solid var(--border, #2d3139);background:transparent;\
                                               color:var(--text, #c9ccd3);border-radius:6px;cursor:pointer;font-size:0.8rem;">
                                        <i class="fa-solid fa-circle-question" style="margin-right:0.5rem;color:var(--warning, #f0ad4e);"></i>
                                        "Find gaps in my knowledge"
                                    </button>
                                    <button class="btn btn-outline btn-sm"
                                        on:click=move |ev| qa_whatif.with_value(|cb| cb.run(ev))
                                        style="text-align:left;padding:0.5rem 0.75rem;\
                                               border:1px solid var(--border, #2d3139);background:transparent;\
                                               color:var(--text, #c9ccd3);border-radius:6px;cursor:pointer;font-size:0.8rem;">
                                        <i class="fa-solid fa-code-branch" style="margin-right:0.5rem;color:var(--success, #5cb85c);"></i>
                                        "What-if analysis"
                                    </button>
                                    <button class="btn btn-outline btn-sm"
                                        on:click=move |ev| qa_help.with_value(|cb| cb.run(ev))
                                        style="text-align:left;padding:0.5rem 0.75rem;\
                                               border:1px solid var(--border, #2d3139);background:transparent;\
                                               color:var(--text, #c9ccd3);border-radius:6px;cursor:pointer;font-size:0.8rem;">
                                        <i class="fa-solid fa-circle-question" style="margin-right:0.5rem;color:var(--info, #5bc0de);"></i>
                                        "Browse all commands"
                                    </button>
                                </div>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <For
                                each={move || messages.get().into_iter().enumerate().collect::<Vec<_>>()}
                                key={|(i, _)| *i}
                                children={move |(_, msg)| {
                                    view::render_message(msg)
                                }}
                            />
                            // Typing indicator
                            {move || {
                                if sending.get() {
                                    view! {
                                        <div style="align-self:flex-start;display:flex;align-items:center;gap:0.4rem;\
                                                    color:var(--text-muted, #8b8fa3);font-size:0.8rem;padding:0.4rem;">
                                            <i class="fa-solid fa-spinner fa-spin"></i>
                                            <span>"Thinking..."</span>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! { <span></span> }.into_any()
                                }
                            }}
                        }.into_any()
                    }
                }}
            </div>

            // Write confirmation panel
            {move || {
                let writes = pending_writes.get();
                if writes.is_empty() {
                    return view! { <span></span> }.into_any();
                }
                view! {
                    <div class="chat-write-confirm"
                        style="flex-shrink:0;padding:0.75rem;border-top:1px solid var(--border, #2d3139);\
                               background:var(--bg-tertiary, #232730);max-height:200px;overflow-y:auto;">
                        <div style="font-size:0.75rem;font-weight:600;margin-bottom:0.5rem;\
                                    color:var(--warning, #f0ad4e);display:flex;align-items:center;gap:0.4rem;">
                            <i class="fa-solid fa-pen-to-square"></i>
                            <span>"Proposed Changes"</span>
                        </div>
                        {writes.into_iter().enumerate().map(|(idx, w)| {
                            let label = w.label.clone();
                            let checked = w.selected;
                            view! {
                                <label style="display:flex;align-items:center;gap:0.5rem;padding:0.3rem 0;\
                                              font-size:0.8rem;color:var(--text, #c9ccd3);cursor:pointer;">
                                    <input type="checkbox" prop:checked=checked
                                        on:change=move |_| toggle_write(idx)
                                        style="accent-color:var(--accent, #4a9eff);" />
                                    <span>{label}</span>
                                </label>
                            }
                        }).collect::<Vec<_>>()}
                        <div style="display:flex;gap:0.5rem;margin-top:0.5rem;">
                            <button class="btn btn-sm" on:click=move |ev| apply_writes.with_value(|cb| cb.run(ev))
                                style="background:var(--success, #5cb85c);color:#fff;border:none;\
                                       border-radius:4px;padding:0.3rem 0.75rem;cursor:pointer;font-size:0.75rem;">
                                <i class="fa-solid fa-check" style="margin-right:0.3rem;"></i>
                                "Apply Selected"
                            </button>
                            <button class="btn btn-sm" on:click=move |ev| reject_writes.with_value(|cb| cb.run(ev))
                                style="background:var(--danger, #d9534f);color:#fff;border:none;\
                                       border-radius:4px;padding:0.3rem 0.75rem;cursor:pointer;font-size:0.75rem;">
                                <i class="fa-solid fa-xmark" style="margin-right:0.3rem;"></i>
                                "Reject All"
                            </button>
                        </div>
                    </div>
                }.into_any()
            }}

            // Follow-up suggestions
            {move || {
                let suggestions = follow_ups.get();
                if suggestions.is_empty() || sending.get() {
                    return view! { <span></span> }.into_any();
                }
                view! {
                    <div class="chat-follow-ups"
                        style="flex-shrink:0;padding:0.5rem 0.75rem;border-top:1px solid var(--border, #2d3139);\
                               display:flex;flex-wrap:wrap;gap:0.4rem;">
                        {suggestions.into_iter().map(|s| {
                            let text = s.text.clone();
                            let text_for_click = s.text.clone();
                            let icon = s.icon;
                            view! {
                                <button class="chat-follow-up-chip"
                                    on:click=move |_| on_follow_up.with_value(|cb| cb.run(text_for_click.clone()))
                                    style="display:inline-flex;align-items:center;gap:0.3rem;\
                                           padding:0.25rem 0.6rem;border-radius:12px;\
                                           border:1px solid var(--border, #2d3139);background:transparent;\
                                           color:var(--text-muted, #8b8fa3);font-size:0.72rem;\
                                           cursor:pointer;transition:all 0.2s;">
                                    <i class=icon style="font-size:0.65rem;"></i>
                                    <span>{text}</span>
                                </button>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            }}

            // Slash autocomplete
            {move || {
                let suggestions = slash_suggestions.get();
                if suggestions.is_empty() {
                    return view! { <span></span> }.into_any();
                }
                view! {
                    <div class="slash-autocomplete"
                        style="flex-shrink:0;padding:0.5rem 0.75rem;\
                               border-top:1px solid var(--border, #2d3139);\
                               display:flex;flex-direction:column;gap:0.25rem;">
                        {suggestions.into_iter().map(|(cmd, icon, desc)| {
                            let cmd_str = cmd.to_string();
                            let cmd_display = cmd.to_string();
                            let icon_cls = icon.to_string();
                            let desc_str = desc.to_string();
                            view! {
                                <div style="display:flex;align-items:center;gap:0.5rem;\
                                            padding:0.3rem 0.5rem;border-radius:4px;\
                                            cursor:pointer;font-size:0.8rem;\
                                            color:var(--text, #c9ccd3);\
                                            background:var(--bg-tertiary, #232730);"
                                    on:mousedown=move |ev| {
                                        ev.prevent_default();
                                        set_input_text.set(cmd_str.clone());
                                        set_slash_suggestions.set(Vec::new());
                                    }
                                >
                                    <i class=icon_cls
                                        style="width:16px;text-align:center;\
                                               color:var(--accent, #4a9eff);font-size:0.75rem;">
                                    </i>
                                    <span style="font-weight:600;">{cmd_display}</span>
                                    <span style="font-size:0.72rem;\
                                                 color:var(--text-muted, #8b8fa3);">
                                        {desc_str}
                                    </span>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            }}

            // Input area
            <div class="chat-input-area"
                style="flex-shrink:0;padding:0.75rem;border-top:1px solid var(--border, #2d3139);\
                       display:flex;gap:0.5rem;align-items:flex-end;">
                <textarea
                    class="chat-input"
                    placeholder="Ask about your knowledge... (Enter to send)"
                    rows="2"
                    prop:value=input_text
                    on:input=move |ev| {
                        let val = event_target_value(&ev);
                        set_input_text.set(val.clone());
                        if val.starts_with('/') {
                            let query = &val[1..];
                            set_slash_suggestions.set(help::filter_commands(query));
                        } else {
                            set_slash_suggestions.set(Vec::new());
                        }
                    }
                    on:keydown=move |ev| on_keydown.with_value(|cb| cb.run(ev))
                    style="flex:1;resize:none;background:var(--bg-tertiary, #232730);\
                           color:var(--text, #c9ccd3);border:1px solid var(--border, #2d3139);\
                           border-radius:8px;padding:0.5rem 0.75rem;font-size:0.85rem;\
                           font-family:inherit;outline:none;min-height:40px;max-height:120px;"
                ></textarea>
                <button
                    class="chat-send-btn"
                    on:click=move |ev| on_send_click.with_value(|cb| cb.run(ev))
                    disabled=move || sending.get() || input_text.get().trim().is_empty()
                    title="Send message"
                    style="width:36px;height:36px;border-radius:8px;border:none;\
                           background:var(--accent, #4a9eff);color:#fff;cursor:pointer;\
                           display:flex;align-items:center;justify-content:center;\
                           font-size:0.9rem;flex-shrink:0;transition:opacity 0.2s;"
                    style:opacity=move || {
                        if sending.get() || input_text.get().trim().is_empty() { "0.5" } else { "1" }
                    }
                >
                    <i class=move || {
                        if sending.get() { "fa-solid fa-spinner fa-spin" }
                        else { "fa-solid fa-paper-plane" }
                    }></i>
                </button>
            </div>
        }
    };

    if embedded {
        // Embedded mode: render internals directly, no floating wrapper
        view! {
            <div class="chat-panel chat-panel-embedded"
                style="display:flex;flex-direction:column;height:100%;width:100%;\
                       background:var(--bg-secondary, #1a1d23);">
                {render_chat_internals()}
            </div>
        }.into_any()
    } else {
        // Floating mode: toggle button + sliding panel
        view! {
            // Toggle button
            <Show when=move || page_visible() && !is_explore_page() && !chat_open.get()>
                <button
                    class="chat-toggle-btn"
                    on:click=toggle_panel
                    title="Knowledge Chat"
                    style="position:fixed;right:1.5rem;bottom:1.5rem;z-index:1500;\
                           width:52px;height:52px;border-radius:50%;border:none;\
                           background:var(--accent, #4a9eff);color:#fff;font-size:1.3rem;\
                           cursor:pointer;box-shadow:0 4px 12px rgba(0,0,0,0.3);\
                           display:flex;align-items:center;justify-content:center;\
                           transition:transform 0.2s ease;"
                >
                    <i class="fa-solid fa-comments"></i>
                </button>
            </Show>

            // Floating panel
            {move || {
                if !page_visible() || is_explore_page() {
                    return view! { <span></span> }.into_any();
                }
                let open = chat_open.get();
                let panel_style = if open {
                    "position:fixed;right:0;top:56px;bottom:0;width:420px;z-index:1400;\
                     background:var(--bg-secondary, #1a1d23);border-left:1px solid var(--border, #2d3139);\
                     display:flex;flex-direction:column;transform:translateX(0);\
                     transition:transform 0.3s ease;box-shadow:-4px 0 20px rgba(0,0,0,0.3);"
                } else {
                    "position:fixed;right:0;top:56px;bottom:0;width:420px;z-index:1400;\
                     background:var(--bg-secondary, #1a1d23);border-left:1px solid var(--border, #2d3139);\
                     display:flex;flex-direction:column;transform:translateX(100%);\
                     transition:transform 0.3s ease;pointer-events:none;"
                };

                view! {
                    <div class="chat-panel" style=panel_style>
                        {render_chat_internals()}
                    </div>
                }.into_any()
            }}
        }.into_any()
    }
}
