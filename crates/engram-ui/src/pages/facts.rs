use leptos::prelude::*;
use crate::api::ApiClient;

#[derive(Clone, Debug, serde::Deserialize)]
struct FactItem {
    label: String,
    confidence: f32,
    status: String,
    subject: String,
    predicate: String,
    object: String,
    claim: String,
    source_passage: String,
    #[allow(dead_code)]
    chunk_index: String,
    #[allow(dead_code)]
    extraction_method: String,
    event_date: String,
}

#[derive(Clone, Debug, serde::Deserialize)]
struct FactListResponse {
    #[allow(dead_code)]
    count: usize,
    facts: Vec<FactItem>,
}

#[component]
pub fn FactReviewPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (facts, set_facts) = signal(Vec::<FactItem>::new());
    let (loading, set_loading) = signal(false);
    let (filter, set_filter) = signal("pending".to_string());
    let (message, set_message) = signal(Option::<String>::None);

    // Load facts
    let api_load = api.clone();
    let load_facts = Action::new_local(move |_: &()| {
        let api = api_load.clone();
        let status = filter.get_untracked();
        async move {
            set_loading.set(true);
            set_message.set(None);
            let body = serde_json::json!({
                "status": if status.is_empty() { None::<String> } else { Some(status) },
                "limit": 100,
            });
            match api.post::<_, FactListResponse>("/facts", &body).await {
                Ok(resp) => set_facts.set(resp.facts),
                Err(e) => set_message.set(Some(format!("Error: {e}"))),
            }
            set_loading.set(false);
        }
    });

    // Auto-load on mount
    let load_init = load_facts.clone();
    Effect::new(move |_| {
        load_init.dispatch(());
    });

    // Confirm fact
    let api_confirm = api.clone();
    let load_after_confirm = load_facts.clone();
    let confirm_fact = Action::new_local(move |label: &String| {
        let api = api_confirm.clone();
        let label = label.clone();
        let reload = load_after_confirm.clone();
        async move {
            let encoded = js_sys::encode_uri_component(&label);
            match api.post_text(&format!("/facts/{}/confirm", encoded.as_string().unwrap_or_default()), &serde_json::json!({})).await {
                Ok(_) => {
                    set_message.set(Some(format!("Confirmed: {}", label)));
                    reload.dispatch(());
                }
                Err(e) => set_message.set(Some(format!("Error: {e}"))),
            }
        }
    });

    // Debunk fact
    let api_debunk = api.clone();
    let load_after_debunk = load_facts.clone();
    let debunk_fact = Action::new_local(move |label: &String| {
        let api = api_debunk.clone();
        let label = label.clone();
        let reload = load_after_debunk.clone();
        async move {
            let encoded = js_sys::encode_uri_component(&label);
            match api.post_text(&format!("/facts/{}/debunk", encoded.as_string().unwrap_or_default()), &serde_json::json!({})).await {
                Ok(_) => {
                    set_message.set(Some(format!("Debunked: {}", label)));
                    reload.dispatch(());
                }
                Err(e) => set_message.set(Some(format!("Error: {e}"))),
            }
        }
    });

    // Delete fact
    let api_delete = api.clone();
    let load_after_delete = load_facts.clone();
    let delete_fact = Action::new_local(move |label: &String| {
        let api = api_delete.clone();
        let label = label.clone();
        let reload = load_after_delete.clone();
        async move {
            let encoded = js_sys::encode_uri_component(&label);
            match api.delete(&format!("/node/{}", encoded.as_string().unwrap_or_default())).await {
                Ok(_) => {
                    set_message.set(Some(format!("Deleted: {}", label)));
                    reload.dispatch(());
                }
                Err(e) => set_message.set(Some(format!("Error: {e}"))),
            }
        }
    });

    view! {
        <div class="page-content">
            <div class="page-header">
                <h2><i class="fa-solid fa-check-double"></i>" Fact Review"</h2>
                <div class="page-actions flex gap-sm">
                    <select class="form-group"
                        style="padding: 0.4rem 0.6rem; background: var(--bg-tertiary); border: 1px solid var(--border); border-radius: 4px; color: var(--text-primary);"
                        on:change=move |ev| {
                            set_filter.set(event_target_value(&ev));
                            load_facts.dispatch(());
                        }>
                        <option value="pending" selected>"Pending"</option>
                        <option value="confirmed">"Confirmed"</option>
                        <option value="debunked">"Debunked"</option>
                        <option value="">"All"</option>
                    </select>
                    <button class="btn btn-primary" on:click=move |_| { load_facts.dispatch(()); } disabled=loading>
                        {move || if loading.get() {
                            view! { <span class="spinner"></span> }.into_any()
                        } else {
                            view! { <><i class="fa-solid fa-refresh"></i>" Reload"</> }.into_any()
                        }}
                    </button>
                </div>
            </div>

            // Message
            {move || message.get().map(|msg| {
                let is_err = msg.starts_with("Error");
                view! {
                    <div style=format!("padding: 0.5rem 0.75rem; margin-bottom: 1rem; border-radius: 4px; font-size: 0.85rem; color: {}; background: {};",
                        if is_err { "#ef5350" } else { "#66bb6a" },
                        if is_err { "rgba(239,83,80,0.1)" } else { "rgba(102,187,106,0.1)" })>
                        {msg}
                    </div>
                }
            })}

            // Fact cards
            <div style="display: flex; flex-direction: column; gap: 0.75rem;">
                {move || {
                    let fact_list = facts.get();
                    if fact_list.is_empty() && !loading.get() {
                        return view! {
                            <div style="text-align: center; padding: 3rem; color: var(--text-secondary);">
                                <i class="fa-solid fa-check-circle" style="font-size: 2rem; margin-bottom: 0.5rem; display: block; opacity: 0.3;"></i>
                                <p>"No facts to review."</p>
                            </div>
                        }.into_any();
                    }
                    view! {
                        <div>
                            {fact_list.into_iter().map(|fact| {
                                let label = fact.label.clone();
                                let label_confirm = label.clone();
                                let label_debunk = label.clone();
                                let label_delete = label.clone();
                                let has_spo = !fact.subject.is_empty();
                                let has_source = !fact.source_passage.is_empty();
                                let (badge_color, badge_bg) = match fact.status.as_str() {
                                    "confirmed" => ("#66bb6a", "rgba(102,187,106,0.15)"),
                                    "pending" => ("#f0ad4e", "rgba(240,173,78,0.15)"),
                                    "debunked" => ("#ef5350", "rgba(239,83,80,0.15)"),
                                    _ => ("#78909c", "rgba(120,144,156,0.15)"),
                                };
                                view! {
                                    <div class="card" style="padding: 0.75rem; border: 1px solid var(--border); border-radius: 6px; background: var(--bg-secondary);">
                                        // Header: status + confidence
                                        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.5rem;">
                                            <span style=format!("padding: 2px 8px; border-radius: 10px; font-size: 0.7rem; font-weight: 600; color: {}; background: {};", badge_color, badge_bg)>
                                                {fact.status.clone()}
                                            </span>
                                            <span style="font-size: 0.75rem; color: var(--text-secondary);">
                                                {format!("{:.0}%", fact.confidence * 100.0)}
                                            </span>
                                        </div>

                                        // SPO triple or claim
                                        {if has_spo {
                                            view! {
                                                <div style="margin-bottom: 0.5rem; font-size: 0.9rem;">
                                                    <span style="font-weight: 600; color: var(--accent-bright, #4fc3f7);">{fact.subject.clone()}</span>
                                                    <span style="color: var(--text-secondary); margin: 0 0.3rem;">{fact.predicate.clone()}</span>
                                                    <span style="font-weight: 600; color: var(--accent-bright, #4fc3f7);">{fact.object.clone()}</span>
                                                </div>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <div style="margin-bottom: 0.5rem; font-size: 0.85rem; color: var(--text-primary);">
                                                    {fact.claim.clone()}
                                                </div>
                                            }.into_any()
                                        }}

                                        // Event date
                                        {if !fact.event_date.is_empty() {
                                            view! {
                                                <div style="font-size: 0.75rem; color: var(--text-secondary); margin-bottom: 0.5rem;">
                                                    <i class="fa-solid fa-calendar" style="margin-right: 0.2rem;"></i>{fact.event_date.clone()}
                                                </div>
                                            }.into_any()
                                        } else {
                                            view! { <span></span> }.into_any()
                                        }}

                                        // Source passage (collapsible)
                                        {if has_source {
                                            view! {
                                                <details style="margin-bottom: 0.5rem;">
                                                    <summary style="font-size: 0.72rem; color: var(--text-secondary); cursor: pointer;">
                                                        <i class="fa-solid fa-file-lines" style="margin-right: 0.2rem;"></i>"Source passage"
                                                    </summary>
                                                    <blockquote style="margin: 0.25rem 0 0 0; padding: 0.4rem 0.6rem; border-left: 3px solid rgba(74,158,255,0.3); background: rgba(255,255,255,0.02); font-size: 0.75rem; color: rgba(255,255,255,0.5); max-height: 150px; overflow-y: auto; white-space: pre-wrap; border-radius: 0 4px 4px 0;">
                                                        {fact.source_passage.clone()}
                                                    </blockquote>
                                                </details>
                                            }.into_any()
                                        } else {
                                            view! { <span></span> }.into_any()
                                        }}

                                        // Action buttons
                                        <div style="display: flex; gap: 0.4rem;">
                                            <button class="btn btn-sm" style="background: #66bb6a; color: #fff; border: none; padding: 3px 10px; border-radius: 3px; font-size: 0.72rem; cursor: pointer;"
                                                on:click=move |_| { confirm_fact.dispatch(label_confirm.clone()); }>
                                                <i class="fa-solid fa-check"></i>" Confirm"
                                            </button>
                                            <button class="btn btn-sm" style="background: rgba(239,83,80,0.2); color: #ef5350; border: 1px solid rgba(239,83,80,0.3); padding: 3px 10px; border-radius: 3px; font-size: 0.72rem; cursor: pointer;"
                                                on:click=move |_| { debunk_fact.dispatch(label_debunk.clone()); }>
                                                <i class="fa-solid fa-xmark"></i>" Debunk"
                                            </button>
                                            <button class="btn btn-sm" style="background: rgba(255,255,255,0.05); color: var(--text-secondary); border: 1px solid var(--border); padding: 3px 10px; border-radius: 3px; font-size: 0.72rem; cursor: pointer; margin-left: auto;"
                                                on:click=move |_| { delete_fact.dispatch(label_delete.clone()); }>
                                                <i class="fa-solid fa-trash"></i>" Delete"
                                            </button>
                                        </div>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    }.into_any()
                }}
            </div>
        </div>
    }
}
