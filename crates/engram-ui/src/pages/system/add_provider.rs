use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::ConfigResponse;

pub fn render_add_provider_modal(
    api: ApiClient,
    config: LocalResource<Option<ConfigResponse>>,
    set_status_msg: WriteSignal<String>,
    set_modal_open: WriteSignal<String>,
) -> impl IntoView {
    let (provider_type, set_provider_type) = signal("searxng".to_string());
    let (provider_name, set_provider_name) = signal(String::new());
    let (provider_url, set_provider_url) = signal(String::new());
    let (provider_cx_id, set_provider_cx_id) = signal(String::new());
    let (provider_api_key, set_provider_api_key) = signal(String::new());

    // Auto-fill name when type changes
    Effect::new(move |_| {
        let t = provider_type.get();
        let name = match t.as_str() {
            "searxng" => "Local SearxNG",
            "serper" => "Serper.dev",
            "google_cx" => "Google CX",
            "brave" => "Brave Search",
            "duckduckgo" => "DuckDuckGo",
            _ => "",
        };
        set_provider_name.set(name.into());
    });

    let api_save = api.clone();
    let do_save = Action::new_local(move |_: &()| {
        let api = api_save.clone();
        let ptype = provider_type.get_untracked();
        let name = provider_name.get_untracked();
        let url = provider_url.get_untracked();
        let cx_id = provider_cx_id.get_untracked();
        let api_key = provider_api_key.get_untracked();
        async move {
            // Store API key in secrets if provided
            let auth_key = if !api_key.is_empty() {
                let key_name = format!("{}_api_key", &ptype);
                match api.post_text(
                    &format!("/secrets/{}", &key_name),
                    &serde_json::json!({"value": &api_key}),
                ).await {
                    Ok(_) => Some(key_name),
                    Err(e) => {
                        set_status_msg.set(format!("Failed to store API key: {e}. Try logging out and back in."));
                        return;
                    }
                }
            } else { None };

            // Build the new provider entry
            let new_provider = serde_json::json!({
                "name": name,
                "provider": ptype,
                "url": if url.is_empty() { None } else { Some(&url) },
                "cx_id": if cx_id.is_empty() { None } else { Some(&cx_id) },
                "enabled": true,
                "auth_secret_key": auth_key,
            });

            // Get existing providers and append
            let mut providers: Vec<serde_json::Value> = match api.get::<ConfigResponse>("/config").await {
                Ok(cfg) => cfg.data.get("web_search_providers")
                    .and_then(|v| v.as_array().cloned())
                    .unwrap_or_default(),
                Err(_) => Vec::new(),
            };
            providers.push(new_provider);

            let config = serde_json::json!({ "web_search_providers": providers });
            match api.post_text("/config", &config).await {
                Ok(_) => {
                    set_status_msg.set("Provider added".into());
                    set_modal_open.set(String::new());
                    // Force config reload by navigating
                    if let Some(window) = web_sys::window() {
                        let _ = window.location().reload();
                    }
                }
                Err(e) => set_status_msg.set(format!("Error: {e}")),
            }
        }
    });

    view! {
        <div class="form-group">
            <label>"Provider Type"</label>
            <select
                prop:value=provider_type
                on:change=move |ev| set_provider_type.set(event_target_value(&ev))>
                <option value="searxng">"SearxNG (self-hosted)"</option>
                <option value="serper">"Serper.dev (Google API)"</option>
                <option value="google_cx">"Google Custom Search (CX)"</option>
                <option value="brave">"Brave Search"</option>
                <option value="duckduckgo">"DuckDuckGo (no key needed)"</option>
            </select>
        </div>
        <div class="form-group">
            <label>"Display Name"</label>
            <input type="text" placeholder="e.g. Local SearxNG"
                prop:value=provider_name
                on:input=move |ev| set_provider_name.set(event_target_value(&ev)) />
        </div>
        {move || (provider_type.get() == "searxng").then(|| view! {
            <div class="form-group">
                <label>"SearxNG URL"</label>
                <input type="text" placeholder="http://localhost:8090"
                    prop:value=provider_url
                    on:input=move |ev| set_provider_url.set(event_target_value(&ev)) />
            </div>
        })}
        {move || (provider_type.get() == "google_cx").then(|| view! {
            <div class="form-group">
                <label>"CX ID (Search Engine ID)"</label>
                <input type="text" placeholder="e.g. 3151ad185ebdc4368"
                    prop:value=provider_cx_id
                    on:input=move |ev| set_provider_cx_id.set(event_target_value(&ev)) />
            </div>
        })}
        {move || {
            let needs_key = matches!(provider_type.get().as_str(), "serper" | "google_cx" | "brave");
            needs_key.then(|| view! {
                <div class="form-group">
                    <label>"API Key"</label>
                    <input type="password" placeholder="API key (stored encrypted in secrets)"
                        prop:value=provider_api_key
                        on:input=move |ev| set_provider_api_key.set(event_target_value(&ev)) />
                </div>
            })
        }}
        <div class="flex gap-sm" style="margin-top: 1rem;">
            <button class="btn btn-secondary" on:click=move |_| set_modal_open.set(String::new())>
                "Cancel"
            </button>
            <button class="btn btn-primary" on:click=move |_| { do_save.dispatch(()); }>
                <i class="fa-solid fa-plus"></i>" Add Provider"
            </button>
        </div>
    }
}
