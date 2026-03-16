//! Chat panel: intelligence analyst workbench.
//!
//! Split into sub-modules:
//! - `context` -- entity extraction & context retrieval
//! - `tools` -- tool name -> API endpoint mapping
//! - `view` -- message rendering & follow-up extraction

pub mod context;
pub mod tools;
pub mod view;

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api::ApiClient;
use crate::api::types::{LlmMessage, LlmProxyRequest, LlmProxyResponse};
use crate::components::chat_types::*;

// ── Helper: get current pathname ──

fn current_pathname() -> String {
    web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .unwrap_or_default()
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
pub fn ChatPanel() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let chat_open =
        use_context::<RwSignal<bool>>().expect("chat_open context");

    let (messages, set_messages) = signal(Vec::<ChatMessage>::new());
    let (input_text, set_input_text) = signal(String::new());
    let (sending, set_sending) = signal(false);
    let (pending_writes, set_pending_writes) = signal(Vec::<PendingWrite>::new());
    let (follow_ups, set_follow_ups) = signal(Vec::<FollowUpSuggestion>::new());

    // Page context signals (set by GraphPage / InsightsPage)
    let chat_selected_node = use_context::<ChatSelectedNode>();
    let chat_assessment = use_context::<ChatCurrentAssessment>();

    // Page-aware visibility
    let page_visible = move || {
        let path = current_pathname();
        page_allows_chat(&path)
    };

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
                    let endpoint = cfg.get("llm_endpoint").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let model = cfg.get("llm_model").and_then(|v| v.as_str()).unwrap_or("default").to_string();
                    (endpoint, model)
                }
                Err(_) => (None, "default".to_string()),
            };

            if llm_endpoint.is_none() || llm_endpoint.as_deref() == Some("") {
                set_messages.update(|msgs| {
                    msgs.push(ChatMessage {
                        role: ChatRole::System,
                        content: "LLM not configured. Go to System > Language Model to set up.".to_string(),
                    });
                });
                set_sending.set(false);
                return;
            }

            // 2. Context retrieval (visible step)
            let keywords = context::extract_keywords(&text);
            let context_items = if !keywords.is_empty() {
                set_messages.update(|msgs| {
                    msgs.push(ChatMessage {
                        role: ChatRole::Context,
                        content: format!("Retrieving context for: {}", keywords.join(", ")),
                    });
                });
                context::retrieve_context(&api, &keywords).await
            } else {
                Vec::new()
            };

            // Update context message with results
            if !context_items.is_empty() {
                set_messages.update(|msgs| {
                    // Replace the "Retrieving..." message with results
                    if let Some(last_ctx) = msgs.iter_mut().rev().find(|m| m.role == ChatRole::Context) {
                        let items_str: Vec<String> = context_items.iter().map(|c| {
                            let typ = c.node_type.as_deref().unwrap_or("entity");
                            format!("{} ({}, {:.0}%)", c.label, typ, c.confidence * 100.0)
                        }).collect();
                        last_ctx.content = format!("Context: {}", items_str.join(", "));
                    }
                });
            }

            // 3. Get graph stats for system prompt
            let (node_count, edge_count) = match api.get::<serde_json::Value>("/stats").await {
                Ok(stats) => (
                    stats.get("nodes").and_then(|v| v.as_u64()).unwrap_or(0),
                    stats.get("edges").and_then(|v| v.as_u64()).unwrap_or(0),
                ),
                Err(_) => (0, 0),
            };

            // 4. Build system prompt with context
            let page = current_pathname();
            // Read page context signals
            let selected_node = chat_selected_node.and_then(|s| s.0.get_untracked());
            let current_assessment = chat_assessment.and_then(|s| s.0.get_untracked());
            let context_block = context::format_context_block(
                &context_items, &page, &selected_node, &current_assessment,
                node_count, edge_count,
            );

            // Get persona from config (llm_system_prompt) or use default
            let persona = config.as_ref().ok()
                .and_then(|c| c.get("llm_system_prompt").and_then(|v| v.as_str()))
                .filter(|s| !s.is_empty())
                .unwrap_or(context::DEFAULT_PERSONA);
            let system_prompt = context::build_system_prompt(&context_block, persona);

            // 5. Fetch tools
            let tools_val: Option<serde_json::Value> = api.get::<serde_json::Value>("/tools").await.ok();
            let tools_array = tools_val.as_ref().and_then(|v| v.get("tools")).cloned();

            // 6. Build message history
            let mut all_messages = vec![ChatMessage {
                role: ChatRole::System,
                content: system_prompt,
            }];
            all_messages.extend(messages.get_untracked());

            // 7. LLM call with tool loop
            let mut collected_writes = Vec::<PendingWrite>::new();
            let max_tool_rounds = 5;

            for _round in 0..max_tool_rounds {
                let llm_msgs = build_llm_messages(&all_messages);
                let req = LlmProxyRequest {
                    model: llm_model.clone(),
                    messages: llm_msgs,
                    temperature: Some(0.3),
                    tools: tools_array.clone(),
                };

                let resp: Result<LlmProxyResponse, _> = api.post("/proxy/llm", &req).await;

                match resp {
                    Ok(r) => {
                        let choice = r.choices.first().and_then(|c| c.message.as_ref());
                        if let Some(msg) = choice {
                            // Display assistant text
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

                            // Process tool calls
                            if let Some(tool_calls) = &msg.tool_calls {
                                if tool_calls.is_empty() {
                                    break;
                                }
                                for tc in tool_calls {
                                    let tool_name = &tc.function.name;
                                    let tool_args = &tc.function.arguments;

                                    if is_write_tool(tool_name) {
                                        // Collect write for confirmation
                                        let label = write_label(tool_name, tool_args);
                                        collected_writes.push(PendingWrite {
                                            label,
                                            tool_name: tool_name.clone(),
                                            args: tool_args.clone(),
                                            selected: true,
                                        });

                                        // Tell LLM the write is pending confirmation
                                        let pending_msg = format!(
                                            "{{\"status\": \"pending_confirmation\", \"tool\": \"{tool_name}\"}}"
                                        );
                                        all_messages.push(ChatMessage {
                                            role: ChatRole::ToolResult,
                                            content: pending_msg,
                                        });
                                    } else {
                                        // Execute read tool immediately
                                        let result = tools::execute_tool(&api, tool_name, tool_args).await;

                                        let display_result = if result.len() > 500 {
                                            format!("{}...", &result[..500])
                                        } else {
                                            result.clone()
                                        };

                                        let tool_msg = ChatMessage {
                                            role: ChatRole::ToolResult,
                                            content: format!("[{tool_name}] {display_result}"),
                                        };
                                        set_messages.update(|msgs| msgs.push(tool_msg));

                                        all_messages.push(ChatMessage {
                                            role: ChatRole::ToolResult,
                                            content: result,
                                        });
                                    }
                                }
                                continue;
                            }
                        }
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

            // 8. Show write confirmation if any writes pending
            if !collected_writes.is_empty() {
                set_pending_writes.set(collected_writes);
                set_messages.update(|msgs| {
                    msgs.push(ChatMessage {
                        role: ChatRole::WriteConfirmation,
                        content: "Proposed changes ready for review.".to_string(),
                    });
                });
            }

            // 9. Extract follow-up suggestions from last assistant message
            let suggestions = view::extract_follow_ups(&messages.get_untracked());
            set_follow_ups.set(suggestions);

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
        if ev.key() == "Enter" && ev.ctrl_key() {
            ev.prevent_default();
            send_clone();
        }
    }));

    // ── Quick actions ──

    let send_for_qa1 = send_message.clone();
    let send_for_qa2 = send_message.clone();
    let send_for_qa3 = send_message.clone();

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

    view! {
        // Toggle button -- only visible on allowed pages
        <Show when=page_visible>
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

        // Sliding panel
        {move || {
            if !page_visible() {
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
                            <button class="btn btn-sm btn-ghost" on:click=close_panel title="Close"
                                style="background:none;border:none;color:var(--text-muted, #8b8fa3);\
                                       cursor:pointer;padding:0.25rem 0.5rem;font-size:0.85rem;">
                                <i class="fa-solid fa-xmark"></i>
                            </button>
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

                    // Input area
                    <div class="chat-input-area"
                        style="flex-shrink:0;padding:0.75rem;border-top:1px solid var(--border, #2d3139);\
                               display:flex;gap:0.5rem;align-items:flex-end;">
                        <textarea
                            class="chat-input"
                            placeholder="Ask about your knowledge... (Ctrl+Enter)"
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
                                if sending.get() || input_text.get().trim().is_empty() { "0.5" } else { "1" }
                            }
                        >
                            <i class=move || {
                                if sending.get() { "fa-solid fa-spinner fa-spin" }
                                else { "fa-solid fa-paper-plane" }
                            }></i>
                        </button>
                    </div>
                </div>
            }.into_any()
        }}
    }
}

