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
    #[serde(default)]
    total: usize,
    facts: Vec<FactItem>,
}

const PAGE_SIZE: usize = 25;

#[component]
pub fn FactReviewPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (facts, set_facts) = signal(Vec::<FactItem>::new());
    let (loading, set_loading) = signal(false);
    let (filter, set_filter) = signal("pending".to_string());
    let (message, set_message) = signal(Option::<String>::None);
    let (page, set_page) = signal(0usize);
    let (total, set_total) = signal(0usize);
    let (search, set_search) = signal(String::new());
    let (expanded, set_expanded) = signal(Option::<String>::None);

    // Load facts with pagination
    let api_load = api.clone();
    let load_facts = Action::new_local(move |_: &()| {
        let api = api_load.clone();
        let status = filter.get_untracked();
        let current_page = page.get_untracked();
        let search_term = search.get_untracked();
        async move {
            set_loading.set(true);
            set_message.set(None);
            let body = serde_json::json!({
                "status": if status.is_empty() { None::<String> } else { Some(status) },
                "limit": PAGE_SIZE,
                "offset": current_page * PAGE_SIZE,
                "search": if search_term.is_empty() { None::<String> } else { Some(search_term) },
            });
            match api.post::<_, FactListResponse>("/facts", &body).await {
                Ok(resp) => {
                    set_total.set(resp.total);
                    set_facts.set(resp.facts);
                }
                Err(e) => set_message.set(Some(format!("Error: {e}"))),
            }
            set_loading.set(false);
        }
    });

    // Auto-load on mount (once only via prev guard)
    let load_init = load_facts.clone();
    Effect::new(move |prev: Option<()>| {
        if prev.is_none() {
            load_init.dispatch(());
        }
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

    // Pagination helpers
    let total_pages = move || {
        let t = total.get();
        if t == 0 { 1 } else { (t + PAGE_SIZE - 1) / PAGE_SIZE }
    };

    let load_for_page = load_facts.clone();
    let go_prev = move |_| {
        let p = page.get_untracked();
        if p > 0 {
            set_page.set(p - 1);
            load_for_page.dispatch(());
        }
    };

    let load_for_next = load_facts.clone();
    let go_next = move |_| {
        let p = page.get_untracked();
        if p + 1 < total_pages() {
            set_page.set(p + 1);
            load_for_next.dispatch(());
        }
    };

    // Search: reset to page 0 and reload
    let load_for_search = load_facts.clone();
    let do_search = move |_| {
        set_page.set(0);
        load_for_search.dispatch(());
    };

    view! {
        <div class="page-content">
            <div class="page-header">
                <h2><i class="fa-solid fa-check-double"></i>" Fact Review"</h2>
                <div class="page-actions flex gap-sm" style="align-items: center;">
                    // Search box
                    <div style="position: relative;">
                        <i class="fa-solid fa-search" style="position: absolute; left: 8px; top: 50%; transform: translateY(-50%); color: var(--text-secondary); font-size: 0.8rem;"></i>
                        <input type="text" placeholder="Search facts..."
                            style="padding: 0.4rem 0.6rem 0.4rem 1.8rem; background: var(--bg-tertiary); border: 1px solid var(--border); border-radius: 4px; color: var(--text-primary); width: 200px; font-size: 0.85rem;"
                            on:input=move |ev| {
                                set_search.set(event_target_value(&ev));
                            }
                            on:keydown=move |ev: web_sys::KeyboardEvent| {
                                if ev.key() == "Enter" {
                                    set_page.set(0);
                                    load_facts.dispatch(());
                                }
                            }
                        />
                        <button style="position: absolute; right: 4px; top: 50%; transform: translateY(-50%); background: none; border: none; color: var(--text-secondary); cursor: pointer; font-size: 0.75rem; padding: 2px 4px;"
                            on:click=do_search>
                            <i class="fa-solid fa-arrow-right"></i>
                        </button>
                    </div>
                    <select class="form-group"
                        style="padding: 0.4rem 0.6rem; background: var(--bg-tertiary); border: 1px solid var(--border); border-radius: 4px; color: var(--text-primary);"
                        on:change=move |ev| {
                            set_filter.set(event_target_value(&ev));
                            set_page.set(0);
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

            // Summary + pagination
            <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.5rem; font-size: 0.8rem; color: var(--text-secondary);">
                <span>{move || {
                    let t = total.get();
                    let p = page.get();
                    let start = p * PAGE_SIZE + 1;
                    let end = ((p + 1) * PAGE_SIZE).min(t);
                    if t == 0 { "No facts found".to_string() }
                    else { format!("Showing {}\u{2013}{} of {}", start, end, t) }
                }}</span>
                <div style="display: flex; align-items: center; gap: 0.5rem;">
                    <button style="background: var(--bg-tertiary); border: 1px solid var(--border); color: var(--text-primary); padding: 2px 8px; border-radius: 3px; cursor: pointer; font-size: 0.75rem;"
                        on:click=go_prev
                        disabled=move || page.get() == 0 || loading.get()>
                        <i class="fa-solid fa-chevron-left"></i>
                    </button>
                    <span style="font-size: 0.78rem;">{move || format!("{} / {}", page.get() + 1, total_pages())}</span>
                    {
                        let is_last = move || page.get() + 1 >= total_pages() || loading.get();
                        view! {
                            <button style="background: var(--bg-tertiary); border: 1px solid var(--border); color: var(--text-primary); padding: 2px 8px; border-radius: 3px; cursor: pointer; font-size: 0.75rem;"
                                on:click=go_next
                                disabled=is_last>
                                <i class="fa-solid fa-chevron-right"></i>
                            </button>
                        }
                    }
                </div>
            </div>

            // Message
            {move || message.get().map(|msg| {
                let is_err = msg.starts_with("Error");
                view! {
                    <div style=format!("padding: 0.4rem 0.6rem; margin-bottom: 0.5rem; border-radius: 4px; font-size: 0.8rem; color: {}; background: {};",
                        if is_err { "#ef5350" } else { "#66bb6a" },
                        if is_err { "rgba(239,83,80,0.1)" } else { "rgba(102,187,106,0.1)" })>
                        {msg}
                    </div>
                }
            })}

            // Compact table header
            <div style="display: grid; grid-template-columns: 70px 1fr 50px 110px; gap: 0; border-bottom: 1px solid var(--border); padding: 0.3rem 0.5rem; font-size: 0.68rem; color: var(--text-secondary); text-transform: uppercase; letter-spacing: 0.5px;">
                <span>"Status"</span>
                <span>"Fact"</span>
                <span style="text-align: right;">"Conf."</span>
                <span style="text-align: right;">"Actions"</span>
            </div>

            // Fact rows
            <div style="display: flex; flex-direction: column;">
                {move || {
                    let fact_list = facts.get();
                    if fact_list.is_empty() && !loading.get() {
                        return view! {
                            <div style="text-align: center; padding: 2rem; color: var(--text-secondary);">
                                <i class="fa-solid fa-check-circle" style="font-size: 1.5rem; margin-bottom: 0.4rem; display: block; opacity: 0.3;"></i>
                                <p style="font-size: 0.85rem;">"No facts to review."</p>
                            </div>
                        }.into_any();
                    }
                    let current_expanded = expanded.get();
                    view! {
                        <div>
                            {fact_list.into_iter().map(|fact| {
                                let label = fact.label.clone();
                                let label_confirm = label.clone();
                                let label_debunk = label.clone();
                                let label_delete = label.clone();
                                let label_expand = label.clone();
                                let is_expanded = current_expanded.as_ref() == Some(&label);
                                let has_spo = !fact.subject.is_empty();
                                let has_source = !fact.source_passage.is_empty();
                                let (badge_color, badge_bg) = match fact.status.as_str() {
                                    "confirmed" => ("#66bb6a", "rgba(102,187,106,0.15)"),
                                    "pending" => ("#f0ad4e", "rgba(240,173,78,0.15)"),
                                    "debunked" => ("#ef5350", "rgba(239,83,80,0.15)"),
                                    _ => ("#78909c", "rgba(120,144,156,0.15)"),
                                };
                                let conf_pct = format!("{:.0}%", fact.confidence * 100.0);
                                let conf_color = if fact.confidence >= 0.8 { "#66bb6a" }
                                    else if fact.confidence >= 0.5 { "#f0ad4e" }
                                    else { "#ef5350" };

                                view! {
                                    <div style="border-bottom: 1px solid rgba(255,255,255,0.04);">
                                        // Main row
                                        <div style="display: grid; grid-template-columns: 70px 1fr 50px 110px; gap: 0; padding: 0.35rem 0.5rem; align-items: center; cursor: pointer;"
                                            on:click=move |_| {
                                                let cur = expanded.get_untracked();
                                                if cur.as_ref() == Some(&label_expand) {
                                                    set_expanded.set(None);
                                                } else {
                                                    set_expanded.set(Some(label_expand.clone()));
                                                }
                                            }>
                                            // Status badge
                                            <span style=format!("padding: 1px 6px; border-radius: 8px; font-size: 0.62rem; font-weight: 600; color: {}; background: {}; text-align: center; width: fit-content;", badge_color, badge_bg)>
                                                {fact.status.clone()}
                                            </span>
                                            // Fact content inline
                                            <div style="font-size: 0.8rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; padding-right: 0.5rem;">
                                                {if has_spo {
                                                    view! {
                                                        <span>
                                                            <span style="font-weight: 600; color: var(--accent-bright, #4fc3f7);">{fact.subject.clone()}</span>
                                                            <span style="color: var(--text-secondary); margin: 0 0.25rem; font-size: 0.75rem;">{fact.predicate.clone()}</span>
                                                            <span style="font-weight: 600; color: var(--accent-bright, #4fc3f7);">{fact.object.clone()}</span>
                                                            {if !fact.event_date.is_empty() {
                                                                view! {
                                                                    <span style="color: var(--text-secondary); font-size: 0.68rem; margin-left: 0.4rem;">
                                                                        <i class="fa-solid fa-calendar" style="margin-right: 0.15rem; font-size: 0.6rem;"></i>{fact.event_date.clone()}
                                                                    </span>
                                                                }.into_any()
                                                            } else {
                                                                view! { <span></span> }.into_any()
                                                            }}
                                                        </span>
                                                    }.into_any()
                                                } else {
                                                    view! {
                                                        <span style="color: var(--text-primary);">{fact.claim.clone()}</span>
                                                    }.into_any()
                                                }}
                                            </div>
                                            // Confidence
                                            <span style=format!("font-size: 0.75rem; text-align: right; font-weight: 600; color: {};", conf_color)>
                                                {conf_pct}
                                            </span>
                                            // Action buttons
                                            <div style="display: flex; gap: 0.25rem; justify-content: flex-end;">
                                                <button title="Confirm" style="background: rgba(102,187,106,0.15); color: #66bb6a; border: none; padding: 2px 6px; border-radius: 3px; font-size: 0.68rem; cursor: pointer;"
                                                    on:click=move |ev: web_sys::MouseEvent| { ev.stop_propagation(); confirm_fact.dispatch(label_confirm.clone()); }>
                                                    <i class="fa-solid fa-check"></i>
                                                </button>
                                                <button title="Debunk" style="background: rgba(239,83,80,0.1); color: #ef5350; border: none; padding: 2px 6px; border-radius: 3px; font-size: 0.68rem; cursor: pointer;"
                                                    on:click=move |ev: web_sys::MouseEvent| { ev.stop_propagation(); debunk_fact.dispatch(label_debunk.clone()); }>
                                                    <i class="fa-solid fa-xmark"></i>
                                                </button>
                                                <button title="Delete" style="background: rgba(255,255,255,0.05); color: var(--text-secondary); border: none; padding: 2px 6px; border-radius: 3px; font-size: 0.68rem; cursor: pointer;"
                                                    on:click=move |ev: web_sys::MouseEvent| { ev.stop_propagation(); delete_fact.dispatch(label_delete.clone()); }>
                                                    <i class="fa-solid fa-trash"></i>
                                                </button>
                                            </div>
                                        </div>
                                        // Expanded detail (source passage) -- only when clicked
                                        {if is_expanded && has_source {
                                            view! {
                                                <div style="padding: 0.3rem 0.5rem 0.5rem 70px;">
                                                    <blockquote style="margin: 0; padding: 0.4rem 0.6rem; border-left: 3px solid rgba(74,158,255,0.3); background: rgba(255,255,255,0.02); font-size: 0.72rem; color: rgba(255,255,255,0.5); max-height: 120px; overflow-y: auto; white-space: pre-wrap; border-radius: 0 4px 4px 0;">
                                                        {fact.source_passage.clone()}
                                                    </blockquote>
                                                </div>
                                            }.into_any()
                                        } else if is_expanded {
                                            view! {
                                                <div style="padding: 0.2rem 0.5rem 0.3rem 70px; font-size: 0.72rem; color: var(--text-secondary); font-style: italic;">
                                                    "No source passage available."
                                                </div>
                                            }.into_any()
                                        } else {
                                            view! { <span></span> }.into_any()
                                        }}
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    }.into_any()
                }}
            </div>

            // Pagination bottom
            {move || {
                let t = total.get();
                if t > PAGE_SIZE {
                    view! {
                        <div style="display: flex; justify-content: center; align-items: center; gap: 0.75rem; margin-top: 0.5rem; padding: 0.5rem 0;">
                            <button style="background: var(--bg-tertiary); border: 1px solid var(--border); color: var(--text-primary); padding: 4px 12px; border-radius: 3px; cursor: pointer; font-size: 0.78rem;"
                                on:click=move |_| {
                                    let p = page.get_untracked();
                                    if p > 0 { set_page.set(p - 1); load_facts.dispatch(()); }
                                }
                                disabled=move || page.get() == 0 || loading.get()>
                                <i class="fa-solid fa-chevron-left"></i>" Prev"
                            </button>
                            <span style="font-size: 0.78rem; color: var(--text-secondary);">
                                {move || format!("Page {} of {}", page.get() + 1, total_pages())}
                            </span>
                            {
                                let is_last_bottom = move || page.get() + 1 >= total_pages() || loading.get();
                                view! {
                                    <button style="background: var(--bg-tertiary); border: 1px solid var(--border); color: var(--text-primary); padding: 4px 12px; border-radius: 3px; cursor: pointer; font-size: 0.78rem;"
                                        on:click=move |_| {
                                            let p = page.get_untracked();
                                            if p + 1 < total_pages() { set_page.set(p + 1); load_facts.dispatch(()); }
                                        }
                                        disabled=is_last_bottom>
                                        "Next "<i class="fa-solid fa-chevron-right"></i>
                                    </button>
                                }
                            }
                        </div>
                    }.into_any()
                } else {
                    view! { <span></span> }.into_any()
                }
            }}
        </div>
    }
}
