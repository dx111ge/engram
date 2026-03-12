use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::SourceCreate;

#[component]
pub fn SourceWizard(
    #[prop(into)] open: ReadSignal<bool>,
    #[prop(into)] on_close: Callback<()>,
    #[prop(optional, into)] on_created: Option<Callback<()>>,
) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (step, set_step) = signal(1u32);
    let (source_type, set_source_type) = signal(String::new());
    let (name, set_name) = signal(String::new());
    let (url, set_url) = signal(String::new());
    let (refresh_interval, set_refresh_interval) = signal(String::new());
    let (auth_type, set_auth_type) = signal("none".to_string());
    let (auth_key, set_auth_key) = signal(String::new());
    let (result_msg, set_result_msg) = signal(Option::<String>::None);

    let overlay_class = move || {
        if open.get() { "modal-overlay active" } else { "modal-overlay" }
    };
    let close = move |_| {
        set_step.set(1);
        set_source_type.set(String::new());
        on_close.run(());
    };

    let source_types = vec![
        ("rss", "RSS Feed", "fa-solid fa-rss"),
        ("web", "Web Page", "fa-solid fa-globe"),
        ("paste", "Paste Text", "fa-solid fa-paste"),
        ("file", "File Upload", "fa-solid fa-file"),
        ("folder", "Folder Watch", "fa-solid fa-folder-open"),
        ("api", "REST API", "fa-solid fa-code"),
        ("sparql", "SPARQL Endpoint", "fa-solid fa-database"),
    ];

    let select_type = move |t: String| {
        set_source_type.set(t);
        set_step.set(2);
    };

    // Create source
    let api_create = api.clone();
    let do_create = Action::new_local(move |_: &()| {
        let api = api_create.clone();
        let body = SourceCreate {
            name: name.get_untracked(),
            source_type: source_type.get_untracked(),
            url: {
                let u = url.get_untracked();
                if u.is_empty() { None } else { Some(u) }
            },
            refresh_interval: refresh_interval.get_untracked().parse().ok(),
            auth_type: {
                let a = auth_type.get_untracked();
                if a == "none" { None } else { Some(a) }
            },
            auth_secret_key: {
                let k = auth_key.get_untracked();
                if k.is_empty() { None } else { Some(k) }
            },
        };
        async move {
            set_result_msg.set(None);
            match api.post_text("/sources", &body).await {
                Ok(_) => {
                    set_result_msg.set(Some("Source created".into()));
                    set_step.set(1);
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
                    <h3><i class="fa-solid fa-plus"></i>" Add Source"</h3>
                    <button class="btn-icon modal-close" on:click=close>
                        <i class="fa-solid fa-xmark"></i>
                    </button>
                </div>
                <div class="modal-body">
                    // Step indicator
                    <div class="text-secondary mb-2" style="font-size: 0.85rem;">
                        {move || format!("Step {} of 3", step.get())}
                    </div>

                    {move || result_msg.get().map(|m| view! {
                        <div class="card" style="padding: 0.5rem; margin-bottom: 0.75rem;">
                            <i class="fa-solid fa-info-circle" style="color: var(--accent-bright);"></i>
                            " " {m}
                        </div>
                    })}

                    {move || match step.get() {
                        1 => view! {
                            <div class="quick-actions">
                                {source_types.iter().map(|(id, label, icon)| {
                                    let id = id.to_string();
                                    let id2 = id.clone();
                                    view! {
                                        <button class="quick-action-btn" on:click=move |_| select_type(id2.clone())>
                                            <i class=*icon></i>
                                            {*label}
                                        </button>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any(),
                        2 => view! {
                            <div>
                                <div class="form-group">
                                    <label>"Source Name"</label>
                                    <input type="text" placeholder="e.g. reuters-rss"
                                        prop:value=name
                                        on:input=move |ev| set_name.set(event_target_value(&ev)) />
                                </div>
                                <div class="form-group">
                                    <label>"URL"</label>
                                    <input type="text" placeholder="https://..."
                                        prop:value=url
                                        on:input=move |ev| set_url.set(event_target_value(&ev)) />
                                </div>
                                <div class="form-row">
                                    <div class="form-group">
                                        <label>"Refresh Interval (seconds)"</label>
                                        <input type="number" placeholder="3600"
                                            prop:value=refresh_interval
                                            on:input=move |ev| set_refresh_interval.set(event_target_value(&ev)) />
                                    </div>
                                    <div class="form-group">
                                        <label>"Auth Type"</label>
                                        <select
                                            prop:value=auth_type
                                            on:change=move |ev| set_auth_type.set(event_target_value(&ev))>
                                            <option value="none">"None"</option>
                                            <option value="bearer">"Bearer Token"</option>
                                            <option value="basic">"Basic Auth"</option>
                                            <option value="api_key">"API Key"</option>
                                        </select>
                                    </div>
                                </div>
                                {move || if auth_type.get() != "none" {
                                    Some(view! {
                                        <div class="form-group">
                                            <label>"Auth Secret Key (from Secrets store)"</label>
                                            <input type="text" placeholder="secret key name"
                                                prop:value=auth_key
                                                on:input=move |ev| set_auth_key.set(event_target_value(&ev)) />
                                        </div>
                                    })
                                } else { None }}
                                <div class="flex gap-sm">
                                    <button class="btn btn-secondary" on:click=move |_| set_step.set(1)>
                                        <i class="fa-solid fa-arrow-left"></i>" Back"
                                    </button>
                                    <button class="btn btn-primary" on:click=move |_| set_step.set(3)>
                                        " Next" <i class="fa-solid fa-arrow-right"></i>
                                    </button>
                                </div>
                            </div>
                        }.into_any(),
                        _ => view! {
                            <div>
                                <h4 class="mb-1">"Confirm Source"</h4>
                                <div class="card" style="margin-bottom: 1rem;">
                                    <div class="prop-row"><span class="text-secondary">"Type:"</span><span>{source_type}</span></div>
                                    <div class="prop-row"><span class="text-secondary">"Name:"</span><span>{name}</span></div>
                                    <div class="prop-row"><span class="text-secondary">"URL:"</span><span>{url}</span></div>
                                </div>
                                <div class="flex gap-sm">
                                    <button class="btn btn-secondary" on:click=move |_| set_step.set(2)>
                                        <i class="fa-solid fa-arrow-left"></i>" Back"
                                    </button>
                                    <button class="btn btn-success" on:click=move |_| { do_create.dispatch(()); }>
                                        <i class="fa-solid fa-check"></i>" Create Source"
                                    </button>
                                </div>
                            </div>
                        }.into_any(),
                    }}
                </div>
            </div>
        </div>
    }
}
