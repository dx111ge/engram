use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{ComputeResponse, StatsResponse};
use super::presets::{parse_onnx_status, EMBED_PRESETS, ONNX_QUICK_MODELS};

pub(crate) fn render_embedding_modal(
    api: ApiClient,
    stats: LocalResource<Option<StatsResponse>>,
    compute: LocalResource<Option<ComputeResponse>>,
    embed_provider: ReadSignal<String>,
    set_embed_provider: WriteSignal<String>,
    embed_endpoint: ReadSignal<String>,
    set_embed_endpoint: WriteSignal<String>,
    embed_model: ReadSignal<String>,
    set_embed_model: WriteSignal<String>,
    embed_test_status: ReadSignal<String>,
    set_embed_test_status: WriteSignal<String>,
    onnx_status: ReadSignal<String>,
    set_onnx_status: WriteSignal<String>,
    ollama_embed_models: ReadSignal<Vec<String>>,
    ollama_fetching: ReadSignal<bool>,
    set_status_msg: WriteSignal<String>,
    set_modal_open: WriteSignal<String>,
) -> impl IntoView {
    let api_embed_save = api.clone();
    let save_embedding = Action::new_local(move |_: &()| {
        let api = api_embed_save.clone();
        let endpoint = embed_endpoint.get_untracked();
        let model = embed_model.get_untracked();
        async move {
            let body = serde_json::json!({
                "embed_endpoint": endpoint,
                "embed_model": model,
            });
            match api.post_text("/config", &body).await {
                Ok(_) => { set_status_msg.set("Embedding settings saved.".to_string()); set_modal_open.set(String::new()); }
                Err(e) => set_status_msg.set(format!("Error saving embedding config: {e}")),
            }
        }
    });

    // Test embedding connection
    let api_embed_test = api.clone();
    let test_embedding = Action::new_local(move |_: &()| {
        let api = api_embed_test.clone();
        async move {
            set_embed_test_status.set("Testing...".into());
            let body = serde_json::json!({ "text": "test embedding connection" });
            match api.post_text("/similar", &body).await {
                Ok(_) => set_embed_test_status.set("Connection successful".into()),
                Err(e) => set_embed_test_status.set(format!("Failed: {e}")),
            }
        }
    });

    // Reindex
    let api_reindex = api.clone();
    let do_reindex = Action::new_local(move |_: &()| {
        let api = api_reindex.clone();
        async move {
            set_status_msg.set("Reindexing all vectors...".into());
            match api.post_text("/admin/reindex", &serde_json::json!({})).await {
                Ok(r) => set_status_msg.set(format!("Reindex complete: {r}")),
                Err(e) => set_status_msg.set(format!("Reindex error: {e}")),
            }
        }
    });

    let api_for_onnx = api.clone();

    view! {
        {move || {
            let has_data = stats.get().flatten()
                .map(|s| s.nodes > 0)
                .unwrap_or(false);
            has_data.then(|| view! {
                <div class="alert alert-warning">
                    <i class="fa-solid fa-lock"></i>
                    " Graph contains data. Changing the embedding model requires running "
                    <code>"engram reindex"</code>" to rebuild vectors."
                </div>
            })
        }}

        // ── Wizard-style provider cards ──
        <p class="wizard-desc">"Embeddings convert text into numbers that capture meaning. This is how engram understands similarity between concepts."</p>
        <div class="wizard-cards">
            {EMBED_PRESETS.iter().map(|p| {
                let id = p.id.to_string();
                let id2 = id.clone();
                let endpoint = p.endpoint.to_string();
                let default_model = p.default_model.to_string();
                view! {
                    <div
                        class=move || if embed_provider.get() == id { "wizard-card wizard-card-selected" } else { "wizard-card" }
                        on:click=move |_| {
                            set_embed_provider.set(id2.clone());
                            set_embed_endpoint.set(endpoint.clone());
                            if !default_model.is_empty() {
                                set_embed_model.set(default_model.clone());
                            }
                        }
                    >
                        <h4>{p.name}</h4>
                        <div class="wizard-card-grid">
                            <span class="wc-label">"Quality"</span><span>{p.quality}</span>
                            <span class="wc-label">"Speed"</span><span>{p.performance}</span>
                            <span class="wc-label">"Privacy"</span><span>{p.privacy}</span>
                            <span class="wc-label">"Cost"</span><span>{p.cost}</span>
                        </div>
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>

        // ── Model selection (when a provider is selected) ──
        {move || {
            let choice = embed_provider.get();
            let preset = EMBED_PRESETS.iter().find(|p| p.id == choice.as_str());
            preset.map(|p| {
                let show_key = p.needs_key;
                let models: Vec<(&str, &str, &str)> = p.models.to_vec();
                let is_onnx = choice == "onnx";
                let is_custom_provider = choice == "custom";
                let show_custom_input = p.models.is_empty() && !is_custom_provider;
                view! {
                    // Custom provider: endpoint URL input
                    {is_custom_provider.then(|| view! {
                        <div class="form-group mt-1">
                            <label><i class="fa-solid fa-link"></i>" Endpoint URL"</label>
                            <input type="text" class="form-control" placeholder="https://api.example.com/v1/embeddings"
                                prop:value=embed_endpoint
                                on:input=move |ev| set_embed_endpoint.set(event_target_value(&ev))
                            />
                            <small class="text-secondary">"OpenAI-compatible /v1/embeddings endpoint"</small>
                        </div>
                    })}
                    // API key input
                    {show_key.then(|| view! {
                        <div class="form-group mt-1">
                            <label><i class="fa-solid fa-key"></i>" API Key"</label>
                            <input type="password" class="form-control" placeholder="sk-..."
                                on:input=move |ev| {
                                    // Store API key in embed_endpoint for non-ONNX providers that need it
                                    let _ = event_target_value(&ev);
                                }
                            />
                        </div>
                    })}
                    <div class="form-group mt-1">
                        <label><i class="fa-solid fa-cube"></i>" Model"</label>
                        // Model chips from preset
                        {(!models.is_empty()).then(|| {
                            let models2 = models.clone();
                            view! {
                                <div class="wizard-model-chips">
                                    {models2.into_iter().map(|(name, desc, lang)| {
                                        let n = name.to_string();
                                        let n2 = n.clone();
                                        let badge_class = if lang.contains("100+") || lang.contains("ulti") {
                                            "wizard-lang-badge wizard-lang-multi"
                                        } else {
                                            "wizard-lang-badge wizard-lang-en"
                                        };
                                        view! {
                                            <button
                                                class=move || if embed_model.get() == n { "wizard-model-chip active" } else { "wizard-model-chip" }
                                                on:click=move |_| set_embed_model.set(n2.clone())
                                            >
                                                <strong>{name}</strong>
                                                <span class=badge_class><i class="fa-solid fa-language"></i>" "{lang}</span>
                                                <small>{desc}</small>
                                            </button>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                                // Custom HuggingFace model (for ONNX)
                                {is_onnx.then(|| view! {
                                    <div class="wizard-custom-model mt-1">
                                        <label><i class="fa-brands fa-github"></i>" Custom HuggingFace Model"</label>
                                        <div class="wizard-custom-input-row">
                                            <input type="text" class="form-control"
                                                placeholder="e.g. sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2"
                                                prop:value=move || {
                                                    let m = embed_model.get();
                                                    let presets = ["all-MiniLM-L6-v2", "multilingual-e5-small", "bge-small-en-v1.5"];
                                                    if presets.contains(&m.as_str()) { String::new() } else { m }
                                                }
                                                on:input=move |ev| {
                                                    let v = event_target_value(&ev);
                                                    if !v.trim().is_empty() {
                                                        set_embed_model.set(v);
                                                    }
                                                }
                                            />
                                        </div>
                                        <small class="text-secondary">"Enter any sentence-transformer ONNX model from huggingface.co. Must contain onnx/model.onnx and tokenizer.json."</small>
                                    </div>
                                })}
                            }
                        })}
                        // Fetched Ollama models (when Ollama selected)
                        {(choice == "ollama").then(|| view! {
                            {move || {
                                let fetched = ollama_embed_models.get();
                                let is_fetching = ollama_fetching.get();
                                if is_fetching {
                                    view! { <p class="text-secondary" style="font-size: 0.8rem; margin-top: 0.5rem;"><i class="fa-solid fa-spinner fa-spin"></i>" Fetching models from Ollama..."</p> }.into_any()
                                } else if !fetched.is_empty() {
                                    view! {
                                        <div style="margin-top: 0.5rem;">
                                            <small class="text-secondary"><i class="fa-solid fa-server"></i>" Installed on your Ollama:"</small>
                                            <div class="wizard-model-chips" style="margin-top: 4px;">
                                                {fetched.into_iter().map(|name| {
                                                    let n = name.clone();
                                                    let n2 = name.clone();
                                                    view! {
                                                        <button
                                                            class=move || if embed_model.get() == n { "wizard-model-chip active" } else { "wizard-model-chip" }
                                                            on:click=move |_| set_embed_model.set(n2.clone())
                                                        >
                                                            <strong>{name}</strong>
                                                        </button>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </div>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! { <span></span> }.into_any()
                                }
                            }}
                        })}
                        // Custom model text input (vLLM, LM Studio)
                        {show_custom_input.then(|| view! {
                            <input type="text" class="form-control" placeholder="Enter model name..."
                                prop:value=embed_model
                                on:input=move |ev| set_embed_model.set(event_target_value(&ev))
                            />
                        })}
                        // Custom provider model input
                        {is_custom_provider.then(|| view! {
                            <input type="text" class="form-control" placeholder="e.g. embed-english-v3.0"
                                prop:value=embed_model
                                on:input=move |ev| set_embed_model.set(event_target_value(&ev))
                            />
                        })}
                    </div>
                }
            })
        }}

        // ── System extras ──

        // ONNX status and management (when ONNX selected)
        {
            let api_onnx_panel = api_for_onnx.clone();
            move || {
            let is_onnx = embed_provider.get() == "onnx";
            let api_check = api_onnx_panel.clone();
            let api_dl = api_check.clone();
            let api_upload = api_check.clone();
            is_onnx.then(|| view! {
                <div class="card" style="margin: 0.75rem 0; padding: 0.75rem; background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.08);">
                    <h4 style="margin-top: 0;"><i class="fa-solid fa-microchip"></i>" ONNX Status & Install"</h4>

                    // ONNX status display
                    {move || {
                        let st = onnx_status.get();
                        if st.is_empty() {
                            None
                        } else if st.contains("installed") || st.contains("Ready") || st.contains("model.onnx") {
                            Some(view! {
                                <div style="margin: 0.5rem 0; padding: 0.5rem 0.75rem; background: rgba(46,204,113,0.1); border: 1px solid rgba(46,204,113,0.3); border-radius: 4px;">
                                    <i class="fa-solid fa-circle-check" style="color: #2ecc71;"></i>
                                    " "{st.clone()}
                                </div>
                            }.into_any())
                        } else {
                            Some(view! {
                                <div style="margin: 0.5rem 0; padding: 0.5rem 0.75rem; background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.08); border-radius: 4px;">
                                    <i class="fa-solid fa-circle-info"></i>" ONNX: "{st.clone()}
                                </div>
                            }.into_any())
                        }
                    }}

                    // Quick Install buttons
                    <div style="margin: 0.75rem 0;">
                        <p class="text-secondary" style="font-size: 0.8rem; margin-bottom: 0.5rem;"><strong>"Quick Install"</strong>" \u{2014} download directly from HuggingFace:"</p>
                        {ONNX_QUICK_MODELS.iter().map(|m| {
                            let name = m.name.to_string();
                            let desc = m.desc.to_string();
                            let model_url = m.model_url.to_string();
                            let tokenizer_url = m.tokenizer_url.to_string();
                            let api_dl = api_dl.clone();
                            view! {
                                <div style="display: flex; align-items: center; gap: 0.5rem; padding: 0.35rem 0; border-bottom: 1px solid rgba(255,255,255,0.05);">
                                    <div style="flex: 1; min-width: 0;">
                                        <code style="font-size: 0.85rem;">{name.clone()}</code>
                                        <span class="text-secondary" style="font-size: 0.75rem; margin-left: 0.5rem;">{desc}</span>
                                    </div>
                                    <button class="btn btn-sm btn-primary" style="white-space: nowrap; padding: 0.2rem 0.5rem; font-size: 0.75rem;" on:click={
                                        let api = api_dl.clone();
                                        let mu = model_url.clone();
                                        let tu = tokenizer_url.clone();
                                        let nm = name.clone();
                                        move |_| {
                                            let api = api.clone();
                                            let mu = mu.clone();
                                            let tu = tu.clone();
                                            let nm = nm.clone();
                                            set_onnx_status.set(format!("Downloading {nm}..."));
                                            wasm_bindgen_futures::spawn_local(async move {
                                                let body = serde_json::json!({
                                                    "model_url": mu,
                                                    "tokenizer_url": tu,
                                                });
                                                match api.post_text("/config/onnx-download", &body).await {
                                                    Ok(r) => {
                                                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&r) {
                                                            let msg = v.get("message").and_then(|v| v.as_str()).unwrap_or("Installed");
                                                            let size = v.get("model_size_mb").and_then(|v| v.as_f64());
                                                            let size_str = size.map(|s| format!(" ({:.1} MB)", s)).unwrap_or_default();
                                                            set_onnx_status.set(format!("{nm} installed{size_str}. {msg}"));
                                                        } else {
                                                            set_onnx_status.set(format!("{nm} installed."));
                                                        }
                                                    }
                                                    Err(e) => set_onnx_status.set(format!("Download failed: {e}")),
                                                }
                                            });
                                        }
                                    }>
                                        <i class="fa-solid fa-cloud-arrow-down"></i>" Install"
                                    </button>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>

                    // Manual upload
                    <details style="margin-top: 0.75rem;">
                        <summary class="text-secondary" style="cursor: pointer; font-size: 0.85rem;"><i class="fa-solid fa-upload" style="margin-right: 0.25rem;"></i>"Manual Upload"</summary>
                        <div style="margin-top: 0.5rem;">
                            <div class="form-row">
                                <label>"ONNX Model File (.onnx)"</label>
                                <input type="file" accept=".onnx" id="onnx-model-file" />
                            </div>
                            <div class="form-row">
                                <label>"Tokenizer File (.json)"</label>
                                <input type="file" accept=".json" id="onnx-tokenizer-file" />
                            </div>
                            <div class="button-group" style="margin-top: 0.5rem;">
                                <button class="btn btn-primary" on:click={
                                    let api = api_upload.clone();
                                    move |_| {
                                        let api = api.clone();
                                        set_onnx_status.set("Uploading...".into());
                                        wasm_bindgen_futures::spawn_local(async move {
                                            use wasm_bindgen::JsCast;
                                            let doc = web_sys::window().unwrap().document().unwrap();
                                            let model_input = doc.get_element_by_id("onnx-model-file")
                                                .and_then(|el| el.dyn_into::<web_sys::HtmlInputElement>().ok());
                                            let tokenizer_input = doc.get_element_by_id("onnx-tokenizer-file")
                                                .and_then(|el| el.dyn_into::<web_sys::HtmlInputElement>().ok());

                                            let form_data = match web_sys::FormData::new() {
                                                Ok(fd) => fd,
                                                Err(_) => {
                                                    set_onnx_status.set("Failed to create FormData".into());
                                                    return;
                                                }
                                            };

                                            let mut has_model = false;
                                            if let Some(ref input) = model_input {
                                                if let Some(files) = input.files() {
                                                    if let Some(file) = files.get(0) {
                                                        let _ = form_data.append_with_blob_and_filename("model", &file, "model.onnx");
                                                        has_model = true;
                                                    }
                                                }
                                            }
                                            let mut has_tokenizer = false;
                                            if let Some(ref input) = tokenizer_input {
                                                if let Some(files) = input.files() {
                                                    if let Some(file) = files.get(0) {
                                                        let _ = form_data.append_with_blob_and_filename("tokenizer", &file, "tokenizer.json");
                                                        has_tokenizer = true;
                                                    }
                                                }
                                            }

                                            if !has_model || !has_tokenizer {
                                                set_onnx_status.set("Please select both model (.onnx) and tokenizer (.json) files.".into());
                                                return;
                                            }

                                            match api.post_formdata("/config/onnx-model", form_data).await {
                                                Ok(text) => {
                                                    let parsed = parse_onnx_status(&text);
                                                    if parsed.contains("No ONNX") {
                                                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                                                            let msg = v.get("message").and_then(|v| v.as_str()).unwrap_or("Upload complete");
                                                            set_onnx_status.set(msg.to_string());
                                                        } else {
                                                            set_onnx_status.set("Upload complete".into());
                                                        }
                                                    } else {
                                                        set_onnx_status.set(parsed);
                                                    }
                                                }
                                                Err(e) => set_onnx_status.set(format!("Upload failed: {e}")),
                                            }
                                        });
                                    }
                                }>
                                    <i class="fa-solid fa-upload"></i>" Upload & Install"
                                </button>
                                <button class="btn btn-secondary" on:click=move |_| {
                                    set_onnx_status.set("Checking...".into());
                                    let api = api_check.clone();
                                    wasm_bindgen_futures::spawn_local(async move {
                                        match api.get_text("/config/onnx-model").await {
                                            Ok(text) => set_onnx_status.set(parse_onnx_status(&text)),
                                            Err(e) => set_onnx_status.set(format!("Check failed: {e}")),
                                        }
                                    });
                                }>
                                    <i class="fa-solid fa-magnifying-glass"></i>" Check Status"
                                </button>
                            </div>
                        </div>
                    </details>
                </div>
            })
        }}

        // Active model info
        {move || {
            let info = compute.get().flatten();
            let model_from_config = embed_model.get();
            let ep = embed_endpoint.get();
            info.map(|c| {
                let dim_str = c.embedder_dim.map(|d| format!("{d}")).unwrap_or_else(|| "auto-detect".to_string());
                let model_str = c.embedder_model.clone()
                    .or_else(|| if !model_from_config.is_empty() { Some(model_from_config.clone()) } else { None })
                    .or_else(|| if ep.starts_with("onnx://") { Some("ONNX Local".into()) } else { None })
                    .unwrap_or_else(|| "not configured".to_string());
                view! {
                    <div class="info-box" style="margin-top: 0.5rem;">
                        <i class="fa-solid fa-ruler-combined"></i>
                        " Active model: "{model_str}" | Dimensions: "{dim_str}
                    </div>
                }
            })
        }}

        // Save / Test / Reindex buttons
        <div class="button-group" style="margin-top: 0.5rem;">
            <button class="btn btn-success" on:click=move |_| { save_embedding.dispatch(()); }>
                <i class="fa-solid fa-floppy-disk"></i>" Save Embedding Config"
            </button>
            <button class="btn btn-secondary" on:click=move |_| { test_embedding.dispatch(()); }>
                <i class="fa-solid fa-satellite-dish"></i>" Test Connection"
            </button>
            <button class="btn btn-warning" on:click=move |_| { do_reindex.dispatch(()); }>
                <i class="fa-solid fa-rotate"></i>" Reindex"
            </button>
        </div>
        {move || {
            let st = embed_test_status.get();
            (!st.is_empty()).then(|| view! {
                <div class="info-box" style="margin-top: 0.5rem;">
                    <i class="fa-solid fa-circle-info"></i>" "{st}
                </div>
            })
        }}
    }
}
