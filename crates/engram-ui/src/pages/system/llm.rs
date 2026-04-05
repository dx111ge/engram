use leptos::prelude::*;

use crate::api::ApiClient;
use super::presets::{event_target_checked, LLM_PRESETS, THINKING_MODELS};

pub(crate) fn render_llm_modal(
    api: ApiClient,
    llm_provider: ReadSignal<String>,
    set_llm_provider: WriteSignal<String>,
    llm_endpoint: ReadSignal<String>,
    set_llm_endpoint: WriteSignal<String>,
    llm_api_key: ReadSignal<String>,
    set_llm_api_key: WriteSignal<String>,
    llm_model: ReadSignal<String>,
    set_llm_model: WriteSignal<String>,
    llm_system_prompt: ReadSignal<String>,
    set_llm_system_prompt: WriteSignal<String>,
    llm_temperature: ReadSignal<String>,
    set_llm_temperature: WriteSignal<String>,
    llm_thinking: ReadSignal<bool>,
    set_llm_thinking: WriteSignal<bool>,
    llm_test_status: ReadSignal<String>,
    set_llm_test_status: WriteSignal<String>,
    llm_has_key: ReadSignal<bool>,
    llm_fetched_models: ReadSignal<Vec<String>>,
    set_llm_fetched_models: WriteSignal<Vec<String>>,
    set_status_msg: WriteSignal<String>,
    set_modal_open: WriteSignal<String>,
) -> impl IntoView {
    let api_llm_save = api.clone();
    let save_llm = Action::new_local(move |_: &()| {
        let api = api_llm_save.clone();
        let endpoint = llm_endpoint.get_untracked();
        let model = llm_model.get_untracked();
        let key = llm_api_key.get_untracked();
        let prompt = llm_system_prompt.get_untracked();
        let temp = llm_temperature.get_untracked();
        let thinking = llm_thinking.get_untracked();
        async move {
            let body = serde_json::json!({
                "llm_endpoint": endpoint,
                "llm_model": model,
                "llm_api_key": key,
                "llm_system_prompt": prompt,
                "llm_temperature": temp.parse::<f64>().unwrap_or(0.7),
                "llm_thinking": thinking,
            });
            match api.post_text("/config", &body).await {
                Ok(_) => { set_status_msg.set("LLM settings saved.".to_string()); set_modal_open.set(String::new()); }
                Err(e) => set_status_msg.set(format!("Error saving LLM config: {e}")),
            }
        }
    });

    // Test LLM connection
    let api_llm_test = api.clone();
    let test_llm = Action::new_local(move |_: &()| {
        let api = api_llm_test.clone();
        async move {
            set_llm_test_status.set("Testing...".into());
            let body = serde_json::json!({
                "model": llm_model.get_untracked(),
                "messages": [{"role": "user", "content": "Say hello in one word."}],
                "temperature": 0.1
            });
            match api.post_text("/proxy/llm", &body).await {
                Ok(_) => set_llm_test_status.set("Connection successful".into()),
                Err(e) => set_llm_test_status.set(format!("Failed: {e}")),
            }
        }
    });

    // Fetch models from provider
    let api_fetch_models = api.clone();
    let fetch_models = Action::new_local(move |_: &()| {
        let api = api_fetch_models.clone();
        let endpoint = llm_endpoint.get_untracked();
        async move {
            set_llm_test_status.set("Fetching models...".into());
            match api.post_text("/proxy/fetch-models", &serde_json::json!({ "endpoint": endpoint })).await {
                Ok(text) => {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        let mut models = Vec::new();
                        if let Some(arr) = json.get("models").and_then(|v| v.as_array()) {
                            for m in arr {
                                if let Some(name) = m.get("name").and_then(|v| v.as_str()) {
                                    models.push(name.to_string());
                                }
                            }
                        }
                        // Also try OpenAI-style /v1/models
                        if let Some(arr) = json.get("data").and_then(|v| v.as_array()) {
                            for m in arr {
                                if let Some(id) = m.get("id").and_then(|v| v.as_str()) {
                                    models.push(id.to_string());
                                }
                            }
                        }
                        if models.is_empty() {
                            set_llm_test_status.set("No models found at endpoint".into());
                        } else {
                            set_llm_test_status.set(format!("Found {} models", models.len()));
                            set_llm_fetched_models.set(models);
                        }
                    } else {
                        set_llm_test_status.set("Could not parse model list".into());
                    }
                }
                Err(e) => set_llm_test_status.set(format!("Fetch models failed: {e}")),
            }
        }
    });

    view! {
        // ── Wizard-style LLM provider cards ──
        <p class="wizard-desc">"A language model is required for area-of-interest detection, entity disambiguation, and intelligent seed enrichment."</p>
        <div class="wizard-cards">
            {LLM_PRESETS.iter().map(|p| {
                let id = p.id.to_string();
                let id2 = id.clone();
                let endpoint = p.endpoint.to_string();
                let default_model = p.default_model.to_string();
                view! {
                    <div
                        class=move || if llm_provider.get() == id { "wizard-card wizard-card-selected" } else { "wizard-card" }
                        on:click=move |_| {
                            set_llm_provider.set(id2.clone());
                            set_llm_endpoint.set(endpoint.clone());
                            set_llm_fetched_models.set(Vec::new());
                            if !default_model.is_empty() && llm_model.get_untracked().is_empty() {
                                set_llm_model.set(default_model.clone());
                            }
                        }
                    >
                        <h4>{p.name}</h4>
                        <div class="wizard-card-grid">
                            <span class="wc-label">"Quality"</span><span>{p.quality}</span>
                            <span class="wc-label">"Privacy"</span><span>{p.privacy}</span>
                            <span class="wc-label">"Cost"</span><span>{p.cost}</span>
                        </div>
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>

        // ── Model selection (when provider selected) ──
        {move || {
            let choice = llm_provider.get();
            let preset = LLM_PRESETS.iter().find(|p| p.id == choice.as_str());
            preset.map(|p| {
                let show_key = p.needs_key;
                let models: Vec<(&str, &str)> = p.models.to_vec();
                let show_custom = p.models.is_empty();
                view! {
                    // API key input
                    {show_key.then(|| view! {
                        <div class="form-group mt-1">
                            <label><i class="fa-solid fa-key"></i>" API Key"
                                {move || {
                                    llm_has_key.get().then(|| view! {
                                        <span class="badge badge-core" style="margin-left: 0.5rem; font-size: 0.65rem;">"key stored"</span>
                                    })
                                }}
                            </label>
                            <input type="password" class="form-control" placeholder="sk-..."
                                prop:value=llm_api_key
                                on:input=move |ev| set_llm_api_key.set(event_target_value(&ev))
                            />
                        </div>
                    })}
                    <div class="form-group mt-1">
                        <label><i class="fa-solid fa-cube"></i>" Model"</label>
                        // Preset model chips
                        {(!models.is_empty()).then(|| {
                            let models2 = models.clone();
                            view! {
                                <div class="wizard-model-chips">
                                    {models2.into_iter().map(|(name, desc)| {
                                        let n = name.to_string();
                                        let n2 = n.clone();
                                        view! {
                                            <button
                                                class=move || if llm_model.get() == n { "wizard-model-chip active" } else { "wizard-model-chip" }
                                                on:click=move |_| set_llm_model.set(n2.clone())
                                            >
                                                <strong>{name}</strong>
                                                <small>{desc}</small>
                                            </button>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            }
                        })}
                        // Fetched models as chips
                        {move || {
                            let fetched = llm_fetched_models.get();
                            if fetched.is_empty() { return None; }
                            let chips = fetched.iter().map(|m| {
                                let model_name = m.clone();
                                let model_name2 = model_name.clone();
                                view! {
                                    <button
                                        class=move || if llm_model.get() == model_name { "wizard-model-chip active" } else { "wizard-model-chip" }
                                        on:click=move |_| set_llm_model.set(model_name2.clone())
                                    >
                                        <i class="fa-solid fa-server" style="margin-right: 0.25rem; font-size: 0.7rem;"></i>
                                        {m.clone()}
                                    </button>
                                }
                            }).collect::<Vec<_>>();
                            Some(view! {
                                <div style="margin-top: 0.5rem;">
                                    <small class="text-secondary"><i class="fa-solid fa-server"></i>" Installed models:"</small>
                                    <div class="wizard-model-chips" style="margin-top: 4px;">
                                        {chips}
                                    </div>
                                </div>
                            })
                        }}
                        // Custom model text input
                        {show_custom.then(|| view! {
                            <input type="text" class="form-control" placeholder="Enter model name..."
                                prop:value=llm_model
                                on:input=move |ev| set_llm_model.set(event_target_value(&ev))
                            />
                        })}
                    </div>
                }
            })
        }}

        // ── System extras ──

        // Endpoint URL
        <div class="form-row" style="margin-top: 0.75rem;">
            <label>"Endpoint URL"</label>
            <input
                type="text"
                prop:value=llm_endpoint
                on:input=move |ev| set_llm_endpoint.set(event_target_value(&ev))
            />
        </div>

        // Temperature slider
        <div class="form-row">
            <label>"Temperature: " {move || {
                let v = llm_temperature.get().parse::<f64>().unwrap_or(0.7);
                format!("{:.1}", v)
            }}</label>
            <input
                type="range"
                min="0" max="2" step="0.1"
                prop:value=llm_temperature
                on:input=move |ev| set_llm_temperature.set(event_target_value(&ev))
                style="width: 100%;"
            />
        </div>

        // Performance tip
        <div class="alert alert-info" style="font-size: 0.85rem; margin: 0.75rem 0; padding: 0.75rem; border-radius: 6px; background: var(--bg-tertiary); border-left: 3px solid var(--accent);">
            <p style="margin: 0 0 0.5rem 0;"><i class="fa-solid fa-lightbulb"></i><strong>" Model Performance Tips"</strong></p>
            <ul style="margin: 0; padding-left: 1.2rem; line-height: 1.6;">
                <li><strong>"Thinking models"</strong>" (deepseek-r1, qwq, qwen3) use extra tokens for reasoning. Great for debate analysis, slower for structured extraction. engram toggles thinking on/off per task automatically."</li>
                <li><strong>"Model size"</strong>" matters: 7-8B models are fast but may struggle with JSON output. 14B+ recommended for debate panel. For Ollama: check VRAM with "<code>"ollama ps"</code>"."</li>
                <li><strong>"Context window"</strong>" is auto-detected when you change models. For Ollama, engram sends "<code>"num_ctx"</code>" to use the full window. If you see truncated output, verify with "<code>"ollama show &lt;model&gt;"</code>" and adjust via "<code>"POST /config {\"llm_context_window\": N}"</code>"."</li>
                <li><strong>"Some models think by default"</strong>" (e.g. gemma4). If JSON output looks garbled or slow, ensure engram detects the model correctly -- thinking is suppressed for extraction tasks."</li>
            </ul>
        </div>

        // Thinking model toggle
        {move || {
            let model = llm_model.get();
            let is_thinking = THINKING_MODELS.iter().any(|t| model.to_lowercase().contains(t));
            is_thinking.then(|| view! {
                <div class="form-row" style="display: flex; align-items: center; gap: 0.5rem;">
                    <input
                        type="checkbox"
                        prop:checked=llm_thinking
                        on:change=move |ev| set_llm_thinking.set(event_target_checked(&ev))
                    />
                    <label style="margin: 0;">"Enable Thinking/Reasoning mode (extended chain-of-thought)"</label>
                </div>
            })
        }}

        // System prompt
        <div class="form-row">
            <label>"System Prompt"</label>
            <textarea
                class="code-area"
                rows="4"
                placeholder="Optional system prompt for the LLM..."
                prop:value=llm_system_prompt
                on:input=move |ev| set_llm_system_prompt.set(event_target_value(&ev))
            />
        </div>

        // Save / Test / Fetch Models buttons
        <div class="button-group">
            <button class="btn btn-success" on:click=move |_| { save_llm.dispatch(()); }>
                <i class="fa-solid fa-floppy-disk"></i>" Save LLM Config"
            </button>
            <button class="btn btn-secondary" on:click=move |_| { test_llm.dispatch(()); }>
                <i class="fa-solid fa-satellite-dish"></i>" Test Connection"
            </button>
            {move || {
                let prov = llm_provider.get();
                let can_fetch = LLM_PRESETS.iter().find(|p| p.id == prov).map(|p| p.can_fetch_models).unwrap_or(false);
                can_fetch.then(|| view! {
                    <button class="btn btn-secondary" on:click=move |_| { fetch_models.dispatch(()); }>
                        <i class="fa-solid fa-list"></i>" Fetch Models"
                    </button>
                })
            }}
        </div>
        {move || {
            let st = llm_test_status.get();
            (!st.is_empty()).then(|| view! {
                <div class="info-box" style="margin-top: 0.5rem;">
                    <i class="fa-solid fa-circle-info"></i>" "{st}
                </div>
            })
        }}
    }
}
