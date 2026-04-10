use leptos::prelude::*;
use crate::api::ApiClient;

#[component]
pub fn ConflictsZone(set_status_msg: WriteSignal<String>) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (conflicts, set_conflicts) = signal(Vec::<serde_json::Value>::new());
    let (loading, set_loading) = signal(true);
    let (conf_page, set_conf_page) = signal(0usize);
    let conf_page_size: usize = 20;

    // Fetch conflicts on mount
    let api_fetch = api.clone();
    Effect::new(move || {
        let api = api_fetch.clone();
        wasm_bindgen_futures::spawn_local(async move {
            set_loading.set(true);
            if let Ok(text) = api.get_text("/conflicts").await {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    let list: Vec<serde_json::Value> = json.get("conflicts")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();
                    set_conflicts.set(list);
                }
            }
            set_loading.set(false);
        });
    });

    let api_resolve = api.clone();
    let resolve_action = Action::new_local(move |(edge_slot, resolution): &(u64, String)| {
        let api = api_resolve.clone();
        let slot = *edge_slot;
        let res = resolution.clone();
        async move {
            let body = serde_json::json!({"edge_slot": slot, "resolution": res});
            match api.post_text("/conflicts/resolve", &body).await {
                Ok(_) => {
                    set_status_msg.set(format!("Conflict resolved: {}", res));
                    // Update local state
                    set_conflicts.update(|list| {
                        if let Some(c) = list.iter_mut().find(|c| c.get("edge_slot").and_then(|v| v.as_u64()) == Some(slot)) {
                            c.as_object_mut().map(|o| o.insert("resolution".into(), serde_json::Value::String(res.clone())));
                        }
                    });
                }
                Err(e) => set_status_msg.set(format!("Error: {e}")),
            }
        }
    });

    view! {
        <div class="card mt-2">
            <div style="display: flex; justify-content: space-between; align-items: center;">
                <h3><i class="fa-solid fa-scale-unbalanced" style="color: var(--warning);"></i>" Contradictions & Conflicts"</h3>
                <a href="/facts" class="btn btn-sm btn-secondary">
                    <i class="fa-solid fa-list"></i>" All Facts"
                </a>
            </div>
            <p class="text-secondary mt-1" style="font-size: 0.85rem;">
                "Facts that contradict existing knowledge. Review and resolve to improve accuracy."
            </p>

            {move || {
                let list = conflicts.get();
                let unresolved: Vec<&serde_json::Value> = list.iter()
                    .filter(|c| c.get("resolution").and_then(|v| v.as_str()).unwrap_or("unresolved") == "unresolved")
                    .collect();

                if loading.get() {
                    return view! { <div class="text-muted" style="padding: 0.5rem;"><i class="fa-solid fa-spinner fa-spin"></i>" Loading..."</div> }.into_any();
                }

                if unresolved.is_empty() && list.is_empty() {
                    return view! { <div class="text-muted" style="padding: 0.5rem; font-size: 0.85rem;">"No contradictions detected."</div> }.into_any();
                }

                let total = list.len();
                let unresolved_count = unresolved.len();
                let resolved_count = total - unresolved_count;

                view! {
                    <div>
                        <div style="display: flex; gap: 0.5rem; margin: 0.5rem 0;">
                            <span class="badge" style="font-size: 0.75rem;">{format!("{} total", total)}</span>
                            {(unresolved_count > 0).then(|| view! {
                                <span class="badge" style="font-size: 0.75rem; background: var(--warning); color: #000;">
                                    {format!("{} unresolved", unresolved_count)}
                                </span>
                            })}
                            {(resolved_count > 0).then(|| view! {
                                <span class="badge badge-active" style="font-size: 0.75rem;">
                                    {format!("{} resolved", resolved_count)}
                                </span>
                            })}
                        </div>

                        <div style="display: flex; flex-direction: column; gap: 4px;">
                            {list.iter().skip(conf_page.get() * conf_page_size).take(conf_page_size).map(|c| {
                                let entity = c.get("entity").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let desc = c.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let severity = c.get("severity").and_then(|v| v.as_f64()).unwrap_or(0.5);
                                let resolution = c.get("resolution").and_then(|v| v.as_str()).unwrap_or("unresolved").to_string();
                                let edge_slot = c.get("edge_slot").and_then(|v| v.as_u64()).unwrap_or(0);
                                let is_resolved = resolution != "unresolved";
                                let sev_color = if severity >= 0.7 { "#e74c3c" } else if severity >= 0.4 { "#f1c40f" } else { "#2ecc71" };

                                let slot_accept = edge_slot;
                                let slot_keep = edge_slot;
                                let slot_dispute = edge_slot;

                                view! {
                                    <div style=format!("padding: 0.5rem; background: var(--bg-secondary); border-radius: 4px; border-left: 3px solid {}; {}",
                                        sev_color, if is_resolved { "opacity: 0.5;" } else { "" })>
                                        <div style="display: flex; justify-content: space-between; align-items: start;">
                                            <div>
                                                <strong style="font-size: 0.85rem;">{entity}</strong>
                                                <div style="font-size: 0.8rem; color: var(--text-secondary); margin-top: 2px;">{desc}</div>
                                            </div>
                                            <div style="display: flex; gap: 3px; flex-shrink: 0;">
                                                {is_resolved.then(|| {
                                                    let res = resolution.clone();
                                                    view! { <span class="badge" style="font-size: 0.65rem;">{res}</span> }
                                                })}
                                                {(!is_resolved).then(|| view! {
                                                    <button class="btn btn-sm" style="font-size: 0.65rem; padding: 1px 6px;"
                                                        title="Accept incoming value"
                                                        on:click=move |_| { resolve_action.dispatch((slot_accept, "accepted_new".into())); }>
                                                        <i class="fa-solid fa-check"></i>
                                                    </button>
                                                    <button class="btn btn-sm" style="font-size: 0.65rem; padding: 1px 6px;"
                                                        title="Keep existing value"
                                                        on:click=move |_| { resolve_action.dispatch((slot_keep, "kept_old".into())); }>
                                                        <i class="fa-solid fa-shield"></i>
                                                    </button>
                                                    <button class="btn btn-sm" style="font-size: 0.65rem; padding: 1px 6px;"
                                                        title="Mark as disputed"
                                                        on:click=move |_| { resolve_action.dispatch((slot_dispute, "disputed".into())); }>
                                                        <i class="fa-solid fa-question"></i>
                                                    </button>
                                                })}
                                            </div>
                                        </div>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                        {
                            let total_conf_pages = if total == 0 { 1 } else { (total + conf_page_size - 1) / conf_page_size };
                            (total_conf_pages > 1).then(|| {
                                let tp = total_conf_pages;
                                view! {
                                    <div style="display: flex; align-items: center; gap: 0.5rem; margin-top: 0.5rem; justify-content: center;">
                                        <button class="btn btn-sm btn-secondary"
                                            disabled=move || conf_page.get() == 0
                                            on:click=move |_| set_conf_page.update(|p| *p = p.saturating_sub(1))>
                                            <i class="fa-solid fa-chevron-left"></i>
                                        </button>
                                        <span style="font-size: 0.8rem;">{format!("Page {} of {}", conf_page.get() + 1, tp)}</span>
                                        <button class="btn btn-sm btn-secondary"
                                            disabled=move || conf_page.get() + 1 >= tp
                                            on:click=move |_| set_conf_page.update(|p| *p += 1)>
                                            <i class="fa-solid fa-chevron-right"></i>
                                        </button>
                                    </div>
                                }
                            })
                        }
                    </div>
                }.into_any()
            }}
        </div>
    }
}
