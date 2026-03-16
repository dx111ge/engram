use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{StoreRequest, StoreResponse, RelateRequest};

#[component]
pub fn CrudModal(
    #[prop(into)] open: ReadSignal<bool>,
    #[prop(into)] on_close: Callback<()>,
    #[prop(optional, into)] on_created: Option<Callback<()>>,
) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (tab, set_tab) = signal(CrudTab::Create);
    let (result_msg, set_result_msg) = signal(Option::<String>::None);

    // Create tab state
    let (create_label, set_create_label) = signal(String::new());
    let (create_type, set_create_type) = signal(String::new());
    let (create_content, set_create_content) = signal(String::new());
    let (create_confidence, set_create_confidence) = signal(0.5f32);

    // Relate tab state
    let (conn_from, set_conn_from) = signal(String::new());
    let (conn_to, set_conn_to) = signal(String::new());
    let (conn_rel, set_conn_rel) = signal(String::new());
    let (conn_conf, set_conn_conf) = signal(0.5f32);

    let overlay_class = move || {
        if open.get() { "modal-overlay active" } else { "modal-overlay" }
    };
    let close = move |_| on_close.run(());

    let tab_class = move |t: CrudTab| {
        if tab.get() == t { "btn btn-primary btn-sm" } else { "btn btn-secondary btn-sm" }
    };

    // Create entity
    let api_create = api.clone();
    let do_create = Action::new_local(move |_: &()| {
        let api = api_create.clone();
        let label = create_label.get_untracked();
        let ntype = create_type.get_untracked();
        let content = create_content.get_untracked();
        let conf = create_confidence.get_untracked();
        async move {
            set_result_msg.set(None);
            let body = StoreRequest {
                entity: label,
                confidence: Some(conf),
                node_type: if ntype.is_empty() { None } else { Some(ntype) },
                content: if content.is_empty() { None } else { Some(content) },
                source: Some("ui".into()),
                properties: None,
            };
            match api.post::<_, StoreResponse>("/store", &body).await {
                Ok(_) => {
                    set_result_msg.set(Some("Entity created".into()));
                    set_create_label.set(String::new());
                    set_create_content.set(String::new());
                    if let Some(cb) = on_created { cb.run(()); }
                }
                Err(e) => set_result_msg.set(Some(format!("Error: {e}"))),
            }
        }
    });

    // Relate entities
    let api_conn = api.clone();
    let do_connect = Action::new_local(move |_: &()| {
        let api = api_conn.clone();
        let from = conn_from.get_untracked();
        let to = conn_to.get_untracked();
        let rel = conn_rel.get_untracked();
        let conf = conn_conf.get_untracked();
        async move {
            set_result_msg.set(None);
            let body = RelateRequest { from, to, relationship: rel, confidence: Some(conf) };
            match api.post_text("/relate", &body).await {
                Ok(_) => {
                    set_result_msg.set(Some("Relationship created".into()));
                    if let Some(cb) = on_created { cb.run(()); }
                }
                Err(e) => set_result_msg.set(Some(format!("Error: {e}"))),
            }
        }
    });

    view! {
        <div class=overlay_class on:click=close>
            <div class="modal" style="max-width: 600px;" on:click=|e| e.stop_propagation()>
                <div class="modal-header">
                    <h3><i class="fa-solid fa-plus"></i>" Create New"</h3>
                    <button class="btn-icon modal-close" on:click=close>
                        <i class="fa-solid fa-xmark"></i>
                    </button>
                </div>
                <div class="modal-body">
                    // Tab buttons
                    <div class="flex gap-sm mb-2">
                        <button class=move || tab_class(CrudTab::Create) on:click=move |_| set_tab.set(CrudTab::Create)>
                            <i class="fa-solid fa-plus"></i>" Create"
                        </button>
                        <button class=move || tab_class(CrudTab::Relate) on:click=move |_| set_tab.set(CrudTab::Relate)>
                            <i class="fa-solid fa-link"></i>" Relate"
                        </button>
                    </div>

                    {move || result_msg.get().map(|m| view! {
                        <div class="card" style="padding: 0.5rem; margin-bottom: 0.75rem; font-size: 0.85rem;">
                            <i class="fa-solid fa-info-circle" style="color: var(--accent-bright);"></i>
                            " " {m}
                        </div>
                    })}

                    {move || match tab.get() {
                        CrudTab::Create => view! {
                            <div>
                                <div class="form-group">
                                    <label>"Entity Label"</label>
                                    <input type="text" placeholder="e.g. PostgreSQL"
                                        prop:value=create_label
                                        on:input=move |ev| set_create_label.set(event_target_value(&ev)) />
                                </div>
                                <div class="form-row">
                                    <div class="form-group">
                                        <label>"Type"</label>
                                        <input type="text" placeholder="e.g. database"
                                            prop:value=create_type
                                            on:input=move |ev| set_create_type.set(event_target_value(&ev)) />
                                    </div>
                                    <div class="form-group">
                                        <label>"Confidence"</label>
                                        <input type="range" min="0" max="1" step="0.05"
                                            prop:value=move || create_confidence.get().to_string()
                                            on:input=move |ev| {
                                                if let Ok(v) = event_target_value(&ev).parse() {
                                                    set_create_confidence.set(v);
                                                }
                                            } />
                                        <span class="text-secondary">{move || format!("{:.0}%", create_confidence.get() * 100.0)}</span>
                                    </div>
                                </div>
                                <div class="form-group">
                                    <label>"Content (optional)"</label>
                                    <textarea placeholder="Additional text content..."
                                        prop:value=create_content
                                        on:input=move |ev| set_create_content.set(event_target_value(&ev)) />
                                </div>
                                <button class="btn btn-primary" on:click=move |_| { do_create.dispatch(()); }>
                                    <i class="fa-solid fa-plus"></i>" Create Entity"
                                </button>
                            </div>
                        }.into_any(),
                        CrudTab::Relate => view! {
                            <div>
                                <div class="form-row">
                                    <div class="form-group">
                                        <label>"From Entity"</label>
                                        <input type="text" placeholder="source entity"
                                            prop:value=conn_from
                                            on:input=move |ev| set_conn_from.set(event_target_value(&ev)) />
                                    </div>
                                    <div class="form-group">
                                        <label>"To Entity"</label>
                                        <input type="text" placeholder="target entity"
                                            prop:value=conn_to
                                            on:input=move |ev| set_conn_to.set(event_target_value(&ev)) />
                                    </div>
                                </div>
                                <div class="form-row">
                                    <div class="form-group">
                                        <label>"Relationship"</label>
                                        <input type="text" placeholder="e.g. uses, related_to"
                                            prop:value=conn_rel
                                            on:input=move |ev| set_conn_rel.set(event_target_value(&ev)) />
                                    </div>
                                    <div class="form-group">
                                        <label>"Confidence"</label>
                                        <input type="range" min="0" max="1" step="0.05"
                                            prop:value=move || conn_conf.get().to_string()
                                            on:input=move |ev| {
                                                if let Ok(v) = event_target_value(&ev).parse() {
                                                    set_conn_conf.set(v);
                                                }
                                            } />
                                        <span class="text-secondary">{move || format!("{:.0}%", conn_conf.get() * 100.0)}</span>
                                    </div>
                                </div>
                                <button class="btn btn-primary" on:click=move |_| { do_connect.dispatch(()); }>
                                    <i class="fa-solid fa-link"></i>" Create Relationship"
                                </button>
                            </div>
                        }.into_any(),
                    }}
                </div>
            </div>
        </div>
    }
}

#[derive(Clone, Copy, PartialEq)]
enum CrudTab {
    Create,
    Relate,
}
