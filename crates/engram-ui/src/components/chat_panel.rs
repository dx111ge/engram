use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api::ApiClient;
use crate::api::types::{LlmMessage, LlmProxyRequest, LlmProxyResponse};
use crate::components::chat_types::*;

// ── Tool execution: map tool name to engram API call ──

async fn execute_tool(api: &ApiClient, name: &str, args: &str) -> String {
    let parsed: serde_json::Value = serde_json::from_str(args).unwrap_or_default();

    let result = match name {
        "engram_store" => {
            let body = serde_json::json!({
                "entity": parsed.get("entity").and_then(|v| v.as_str()).unwrap_or(""),
                "node_type": parsed.get("type").and_then(|v| v.as_str()),
                "source": parsed.get("source").and_then(|v| v.as_str()),
                "confidence": parsed.get("confidence").and_then(|v| v.as_f64()),
                "properties": parsed.get("properties"),
            });
            api.post_text("/store", &body).await
        }
        "engram_relate" => {
            let body = serde_json::json!({
                "from": parsed.get("from").and_then(|v| v.as_str()).unwrap_or(""),
                "to": parsed.get("to").and_then(|v| v.as_str()).unwrap_or(""),
                "relationship": parsed.get("relationship").and_then(|v| v.as_str()).unwrap_or(""),
                "confidence": parsed.get("confidence").and_then(|v| v.as_f64()),
            });
            api.post_text("/relate", &body).await
        }
        "engram_query" => {
            let body = serde_json::json!({
                "query": parsed.get("start").and_then(|v| v.as_str()).unwrap_or(""),
                "depth": parsed.get("depth").and_then(|v| v.as_u64()),
                "direction": parsed.get("direction").and_then(|v| v.as_str()),
                "min_confidence": parsed.get("min_confidence").and_then(|v| v.as_f64()),
            });
            api.post_text("/query", &body).await
        }
        "engram_search" => {
            let body = serde_json::json!({
                "query": parsed.get("query").and_then(|v| v.as_str()).unwrap_or(""),
                "limit": parsed.get("limit").and_then(|v| v.as_u64()),
            });
            api.post_text("/search", &body).await
        }
        "engram_similar" => {
            let body = serde_json::json!({
                "text": parsed.get("text").and_then(|v| v.as_str()).unwrap_or(""),
                "limit": parsed.get("limit").and_then(|v| v.as_u64()),
            });
            api.post_text("/similar", &body).await
        }
        "engram_explain" => {
            let entity = parsed
                .get("entity")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let encoded = js_sys::encode_uri_component(entity);
            api.get_text(&format!("/explain/{}", encoded.as_string().unwrap_or_default()))
                .await
        }
        "engram_gaps" => api.get_text("/reason/gaps").await,
        "engram_reinforce" => {
            let body = serde_json::json!({
                "entity": parsed.get("entity").and_then(|v| v.as_str()).unwrap_or(""),
                "source": parsed.get("source").and_then(|v| v.as_str()),
            });
            api.post_text("/reinforce", &body).await
        }
        other => {
            // Generic fallback: POST to /<tool_name_without_engram_prefix>
            let endpoint = other.strip_prefix("engram_").unwrap_or(other);
            api.post_text(&format!("/{endpoint}"), &parsed).await
        }
    };

    match result {
        Ok(text) => text,
        Err(e) => format!("{{\"error\": \"{e}\"}}"),
    }
}

// ── Convert chat messages to LLM wire format ──

fn build_llm_messages(messages: &[ChatMessage]) -> Vec<LlmMessage> {
    messages
        .iter()
        .map(|m| {
            let role = match &m.role {
                ChatRole::User => "user",
                ChatRole::Assistant => "assistant",
                ChatRole::System => "system",
                ChatRole::ToolResult => "tool",
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
pub fn ChatPanel() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let chat_open =
        use_context::<RwSignal<bool>>().expect("chat_open context");

    let (messages, set_messages) = signal(Vec::<ChatMessage>::new());
    let (input_text, set_input_text) = signal(String::new());
    let (sending, set_sending) = signal(false);

    // ── Send message flow ──

    let api_send = api.clone();
    let send_message = move || {
        let text = input_text.get_untracked().trim().to_string();
        if text.is_empty() || sending.get_untracked() {
            return;
        }
        set_input_text.set(String::new());
        set_sending.set(true);

        // Add user message
        set_messages.update(|msgs| {
            msgs.push(ChatMessage {
                role: ChatRole::User,
                content: text.clone(),
            });
        });

        let api = api_send.clone();
        spawn_local(async move {
            // 1. Check LLM configuration
            let config: Result<serde_json::Value, _> = api.get("/config").await;
            let (llm_endpoint, llm_model) = match &config {
                Ok(cfg) => {
                    let endpoint = cfg
                        .get("llm_endpoint")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let model = cfg
                        .get("llm_model")
                        .and_then(|v| v.as_str())
                        .unwrap_or("default")
                        .to_string();
                    (endpoint, model)
                }
                Err(_) => (None, "default".to_string()),
            };

            if llm_endpoint.is_none() || llm_endpoint.as_deref() == Some("") {
                set_messages.update(|msgs| {
                    msgs.push(ChatMessage {
                        role: ChatRole::System,
                        content: "LLM not configured. Go to System > Language Model to set up."
                            .to_string(),
                    });
                });
                set_sending.set(false);
                return;
            }

            // 2. Fetch available tools
            let tools_val: Option<serde_json::Value> =
                api.get::<serde_json::Value>("/tools").await.ok();
            let tools_array = tools_val
                .as_ref()
                .and_then(|v| v.get("tools"))
                .cloned();

            // 3. Build system prompt
            let mut all_messages = vec![ChatMessage {
                role: ChatRole::System,
                content: "You are an AI assistant with access to the engram knowledge graph. \
                    Use the available tools to store, query, search, and reason about knowledge. \
                    Be concise and precise. When you find information, summarize it clearly."
                    .to_string(),
            }];
            all_messages.extend(messages.get_untracked());

            // 4. Call LLM (with tool-call loop)
            let max_tool_rounds = 5;
            for _round in 0..max_tool_rounds {
                let llm_msgs = build_llm_messages(&all_messages);
                let req = LlmProxyRequest {
                    model: llm_model.clone(),
                    messages: llm_msgs,
                    temperature: Some(0.3),
                    tools: tools_array.clone(),
                };

                let resp: Result<LlmProxyResponse, _> =
                    api.post("/proxy/llm", &req).await;

                match resp {
                    Ok(r) => {
                        let choice = r.choices.first().and_then(|c| c.message.as_ref());
                        if let Some(msg) = choice {
                            // If assistant has text content, display it
                            if let Some(content) = &msg.content {
                                if !content.is_empty() {
                                    let assistant_msg = ChatMessage {
                                        role: ChatRole::Assistant,
                                        content: content.clone(),
                                    };
                                    all_messages.push(assistant_msg.clone());
                                    set_messages.update(|msgs| msgs.push(assistant_msg));
                                }
                            }

                            // If tool calls exist, execute them
                            if let Some(tool_calls) = &msg.tool_calls {
                                if tool_calls.is_empty() {
                                    break;
                                }
                                for tc in tool_calls {
                                    let tool_name = &tc.function.name;
                                    let tool_args = &tc.function.arguments;

                                    // Execute tool
                                    let result =
                                        execute_tool(&api, tool_name, tool_args).await;

                                    // Truncate large results for display
                                    let display_result = if result.len() > 500 {
                                        format!("{}...", &result[..500])
                                    } else {
                                        result.clone()
                                    };

                                    let tool_msg = ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: format!(
                                            "[{tool_name}] {display_result}"
                                        ),
                                    };
                                    set_messages.update(|msgs| msgs.push(tool_msg));

                                    // Feed result back to LLM as tool message
                                    all_messages.push(ChatMessage {
                                        role: ChatRole::ToolResult,
                                        content: result,
                                    });
                                }
                                // Continue loop to let LLM process tool results
                                continue;
                            }
                        }
                        // No tool calls -> done
                        break;
                    }
                    Err(e) => {
                        set_messages.update(|msgs| {
                            msgs.push(ChatMessage {
                                role: ChatRole::System,
                                content: format!("LLM request failed: {e}"),
                            });
                        });
                        break;
                    }
                }
            }

            set_sending.set(false);
        });
    };

    // ── Clear messages ──

    let clear_messages = move |_: leptos::ev::MouseEvent| {
        set_messages.set(Vec::new());
    };

    // ── Toggle panel ──

    let toggle_panel = move |_: leptos::ev::MouseEvent| {
        chat_open.update(|v| *v = !*v);
    };

    let close_panel = move |_: leptos::ev::MouseEvent| {
        chat_open.set(false);
    };

    // ── Keyboard handler: Ctrl+Enter to send ──

    let send_clone = send_message.clone();
    let on_keydown = StoredValue::new(Callback::new(move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() == "Enter" && ev.ctrl_key() {
            ev.prevent_default();
            send_clone();
        }
    }));

    // ── Quick actions (stored to move into view) ──

    let send_for_qa1 = send_message.clone();
    let send_for_qa2 = send_message.clone();
    let send_for_qa3 = send_message.clone();

    let qa_know = StoredValue::new(Callback::new(move |_: leptos::ev::MouseEvent| {
        set_input_text.set("What do I know about...".to_string());
        send_for_qa1();
    }));

    let qa_gaps = StoredValue::new(Callback::new(move |_: leptos::ev::MouseEvent| {
        set_input_text.set("Find gaps in my knowledge".to_string());
        send_for_qa2();
    }));

    let qa_whatif = StoredValue::new(Callback::new(move |_: leptos::ev::MouseEvent| {
        set_input_text.set("What-if analysis".to_string());
        send_for_qa3();
    }));

    // ── Send button handler ──

    let send_for_btn = send_message.clone();
    let on_send_click = StoredValue::new(Callback::new(move |_: leptos::ev::MouseEvent| {
        send_for_btn();
    }));

    view! {
        // ── Floating toggle button ──
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

        // ── Sliding panel ──
        {move || {
            let open = chat_open.get();
            let panel_style = if open {
                "position:fixed;right:0;top:56px;bottom:0;width:380px;z-index:1400;\
                 background:var(--bg-secondary, #1a1d23);border-left:1px solid var(--border, #2d3139);\
                 display:flex;flex-direction:column;transform:translateX(0);\
                 transition:transform 0.3s ease;box-shadow:-4px 0 20px rgba(0,0,0,0.3);"
            } else {
                "position:fixed;right:0;top:56px;bottom:0;width:380px;z-index:1400;\
                 background:var(--bg-secondary, #1a1d23);border-left:1px solid var(--border, #2d3139);\
                 display:flex;flex-direction:column;transform:translateX(100%);\
                 transition:transform 0.3s ease;pointer-events:none;"
            };

            view! {
                <div class="chat-panel" style=panel_style>
                    // ── Header ──
                    <div
                        class="chat-header"
                        style="display:flex;align-items:center;justify-content:space-between;\
                               padding:0.75rem 1rem;border-bottom:1px solid var(--border, #2d3139);\
                               flex-shrink:0;"
                    >
                        <div style="display:flex;align-items:center;gap:0.5rem;font-weight:600;">
                            <i class="fa-solid fa-brain" style="color:var(--accent, #4a9eff);"></i>
                            <span>"Knowledge Chat"</span>
                        </div>
                        <div style="display:flex;gap:0.5rem;">
                            <button
                                class="btn btn-sm btn-ghost"
                                on:click=clear_messages
                                title="Clear chat"
                                style="background:none;border:none;color:var(--text-muted, #8b8fa3);\
                                       cursor:pointer;padding:0.25rem 0.5rem;font-size:0.85rem;"
                            >
                                <i class="fa-solid fa-trash-can"></i>
                            </button>
                            <button
                                class="btn btn-sm btn-ghost"
                                on:click=close_panel
                                title="Close"
                                style="background:none;border:none;color:var(--text-muted, #8b8fa3);\
                                       cursor:pointer;padding:0.25rem 0.5rem;font-size:0.85rem;"
                            >
                                <i class="fa-solid fa-xmark"></i>
                            </button>
                        </div>
                    </div>

                    // ── Message list ──
                    <div
                        class="chat-messages"
                        style="flex:1;overflow-y:auto;padding:0.75rem;display:flex;\
                               flex-direction:column;gap:0.5rem;"
                    >
                        {move || {
                            let msgs = messages.get();
                            if msgs.is_empty() {
                                // ── Empty state with quick actions ──
                                view! {
                                    <div
                                        class="chat-empty"
                                        style="display:flex;flex-direction:column;align-items:center;\
                                               justify-content:center;flex:1;gap:1rem;\
                                               color:var(--text-muted, #8b8fa3);text-align:center;\
                                               padding:2rem 1rem;"
                                    >
                                        <i class="fa-solid fa-comments"
                                           style="font-size:2.5rem;opacity:0.3;"></i>
                                        <p style="margin:0;font-size:0.9rem;">
                                            "Ask questions about your knowledge graph, store new facts, or explore connections."
                                        </p>
                                        <div
                                            class="quick-actions"
                                            style="display:flex;flex-direction:column;gap:0.5rem;\
                                                   width:100%;max-width:280px;"
                                        >
                                            <button
                                                class="btn btn-outline btn-sm"
                                                on:click=move |ev| qa_know.with_value(|cb| cb.run(ev))
                                                style="text-align:left;padding:0.5rem 0.75rem;\
                                                       border:1px solid var(--border, #2d3139);\
                                                       background:transparent;\
                                                       color:var(--text, #c9ccd3);border-radius:6px;\
                                                       cursor:pointer;font-size:0.8rem;"
                                            >
                                                <i class="fa-solid fa-search"
                                                   style="margin-right:0.5rem;color:var(--accent, #4a9eff);">
                                                </i>
                                                "What do I know about..."
                                            </button>
                                            <button
                                                class="btn btn-outline btn-sm"
                                                on:click=move |ev| qa_gaps.with_value(|cb| cb.run(ev))
                                                style="text-align:left;padding:0.5rem 0.75rem;\
                                                       border:1px solid var(--border, #2d3139);\
                                                       background:transparent;\
                                                       color:var(--text, #c9ccd3);border-radius:6px;\
                                                       cursor:pointer;font-size:0.8rem;"
                                            >
                                                <i class="fa-solid fa-circle-question"
                                                   style="margin-right:0.5rem;color:var(--warning, #f0ad4e);">
                                                </i>
                                                "Find gaps in my knowledge"
                                            </button>
                                            <button
                                                class="btn btn-outline btn-sm"
                                                on:click=move |ev| qa_whatif.with_value(|cb| cb.run(ev))
                                                style="text-align:left;padding:0.5rem 0.75rem;\
                                                       border:1px solid var(--border, #2d3139);\
                                                       background:transparent;\
                                                       color:var(--text, #c9ccd3);border-radius:6px;\
                                                       cursor:pointer;font-size:0.8rem;"
                                            >
                                                <i class="fa-solid fa-code-branch"
                                                   style="margin-right:0.5rem;color:var(--success, #5cb85c);">
                                                </i>
                                                "What-if analysis"
                                            </button>
                                        </div>
                                    </div>
                                }.into_any()
                            } else {
                                // ── Rendered messages ──
                                view! {
                                    <For
                                        each={move || {
                                            messages.get().into_iter().enumerate().collect::<Vec<_>>()
                                        }}
                                        key={|(i, _)| *i}
                                        children={move |(_, msg)| {
                                            let (bubble_style, icon_class, align) = match &msg.role {
                                                ChatRole::User => (
                                                    "background:var(--accent, #4a9eff);color:#fff;\
                                                     border-radius:12px 12px 2px 12px;padding:0.6rem 0.85rem;\
                                                     max-width:85%;word-wrap:break-word;font-size:0.85rem;",
                                                    "fa-solid fa-user",
                                                    "align-self:flex-end;",
                                                ),
                                                ChatRole::Assistant => (
                                                    "background:var(--bg-tertiary, #232730);color:var(--text, #c9ccd3);\
                                                     border-radius:12px 12px 12px 2px;padding:0.6rem 0.85rem;\
                                                     max-width:85%;word-wrap:break-word;font-size:0.85rem;\
                                                     border:1px solid var(--border, #2d3139);",
                                                    "fa-solid fa-brain",
                                                    "align-self:flex-start;",
                                                ),
                                                ChatRole::System => (
                                                    "background:var(--warning-bg, #3d3520);color:var(--warning, #f0ad4e);\
                                                     border-radius:8px;padding:0.5rem 0.75rem;\
                                                     max-width:90%;word-wrap:break-word;font-size:0.8rem;\
                                                     border:1px solid var(--warning, #f0ad4e);",
                                                    "fa-solid fa-circle-exclamation",
                                                    "align-self:center;",
                                                ),
                                                ChatRole::ToolResult => (
                                                    "background:var(--bg-tertiary, #232730);color:var(--text-muted, #8b8fa3);\
                                                     border-radius:6px;padding:0.4rem 0.65rem;\
                                                     max-width:90%;word-wrap:break-word;font-size:0.75rem;\
                                                     border-left:3px solid var(--accent, #4a9eff);\
                                                     font-family:monospace;white-space:pre-wrap;",
                                                    "fa-solid fa-wrench",
                                                    "align-self:flex-start;",
                                                ),
                                            };

                                            let wrapper_style = format!(
                                                "display:flex;flex-direction:column;{align}gap:0.2rem;"
                                            );

                                            view! {
                                                <div style=wrapper_style>
                                                    <div style="display:flex;align-items:center;gap:0.3rem;\
                                                                font-size:0.7rem;color:var(--text-muted, #8b8fa3);">
                                                        <i class=icon_class style="font-size:0.65rem;"></i>
                                                        <span>{match &msg.role {
                                                            ChatRole::User => "You",
                                                            ChatRole::Assistant => "Engram",
                                                            ChatRole::System => "System",
                                                            ChatRole::ToolResult => "Tool",
                                                        }}</span>
                                                    </div>
                                                    <div style=bubble_style>
                                                        {msg.content.clone()}
                                                    </div>
                                                </div>
                                            }
                                        }}
                                    />
                                    // ── Typing indicator ──
                                    {move || {
                                        if sending.get() {
                                            view! {
                                                <div style="align-self:flex-start;display:flex;\
                                                            align-items:center;gap:0.4rem;\
                                                            color:var(--text-muted, #8b8fa3);\
                                                            font-size:0.8rem;padding:0.4rem;">
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

                    // ── Input area ──
                    <div
                        class="chat-input-area"
                        style="flex-shrink:0;padding:0.75rem;border-top:1px solid var(--border, #2d3139);\
                               display:flex;gap:0.5rem;align-items:flex-end;"
                    >
                        <textarea
                            class="chat-input"
                            placeholder="Ask about your knowledge... (Ctrl+Enter to send)"
                            rows="2"
                            prop:value=input_text
                            on:input=move |ev| set_input_text.set(event_target_value(&ev))
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
                                if sending.get() || input_text.get().trim().is_empty() {
                                    "0.5"
                                } else {
                                    "1"
                                }
                            }
                        >
                            <i class=move || {
                                if sending.get() {
                                    "fa-solid fa-spinner fa-spin"
                                } else {
                                    "fa-solid fa-paper-plane"
                                }
                            }></i>
                        </button>
                    </div>
                </div>
            }
        }}
    }
}
