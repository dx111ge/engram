mod presets;
mod embedding;
mod llm;
mod ner;
mod secrets;
mod database;

use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{
    ComputeResponse, ConfigResponse,
    MeshAuditEntry, PeerInfo, SecretListItem,
    StatsResponse,
};

use presets::*;

// ── System page ──

#[component]
pub fn SystemPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let (status_msg, set_status_msg) = signal(String::new());
    let (active_tab, set_active_tab) = signal("system".to_string());
    let (modal_open, set_modal_open) = signal(String::new());

    // ── Initial data loads ──

    let api_cfg = api.clone();
    let config = LocalResource::new(move || {
        let api = api_cfg.clone();
        async move { api.get::<ConfigResponse>("/config").await.ok() }
    });

    let api_stats = api.clone();
    let stats = LocalResource::new(move || {
        let api = api_stats.clone();
        async move { api.get::<StatsResponse>("/stats").await.ok() }
    });

    let api_compute = api.clone();
    let compute = LocalResource::new(move || {
        let api = api_compute.clone();
        async move { api.get::<ComputeResponse>("/compute").await.ok() }
    });

    let api_peers = api.clone();
    let peers = LocalResource::new(move || {
        let api = api_peers.clone();
        async move { api.get::<Vec<PeerInfo>>("/mesh/peers").await.ok().unwrap_or_default() }
    });

    let api_audit = api.clone();
    let audit_log = LocalResource::new(move || {
        let api = api_audit.clone();
        async move { api.get::<Vec<MeshAuditEntry>>("/mesh/audit").await.ok().unwrap_or_default() }
    });

    let api_secrets = api.clone();
    let secrets = LocalResource::new(move || {
        let api = api_secrets.clone();
        async move { api.get::<Vec<SecretListItem>>("/secrets").await.ok().unwrap_or_default() }
    });

    let api_identity = api.clone();
    let mesh_identity = LocalResource::new(move || {
        let api = api_identity.clone();
        async move { api.get_text("/mesh/identity").await.ok().unwrap_or_default() }
    });

    // ── Section 2: Embedding ──

    let (embed_provider, set_embed_provider) = signal("ollama".to_string());
    let (embed_endpoint, set_embed_endpoint) = signal(String::new());
    let (embed_model, set_embed_model) = signal(String::new());
    let (embed_test_status, set_embed_test_status) = signal(String::new());
    let (onnx_status, set_onnx_status) = signal(String::new());
    // Ollama model fetching for embed modal
    let (ollama_embed_models, set_ollama_embed_models) = signal(Vec::<String>::new());
    let (ollama_fetching, set_ollama_fetching) = signal(false);

    // Sync config values once loaded
    Effect::new(move |_| {
        if let Some(cfg) = config.get().flatten() {
            if let Some(ep) = cfg.data.get("embed_endpoint").and_then(|v: &serde_json::Value| v.as_str()) {
                set_embed_endpoint.set(ep.to_string());
                for p in EMBED_PRESETS {
                    if ep == p.endpoint {
                        set_embed_provider.set(p.id.to_string());
                        break;
                    }
                }
            }
            if let Some(m) = cfg.data.get("embed_model").and_then(|v: &serde_json::Value| v.as_str()) {
                set_embed_model.set(m.to_string());
            }
        }
    });

    // Check ONNX status
    let api_onnx_check = api.clone();
    Effect::new(move |_| {
        let api = api_onnx_check.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match api.get_text("/config/onnx-model").await {
                Ok(text) => set_onnx_status.set(parse_onnx_status(&text)),
                Err(_) => set_onnx_status.set("No ONNX model installed".into()),
            }
        });
    });

    // Auto-fetch Ollama models when Ollama is selected for embed
    let api_ollama_embed = api.clone();
    Effect::new(move || {
        let embed = embed_provider.get();
        if embed == "ollama" && ollama_embed_models.get_untracked().is_empty() {
            let api = api_ollama_embed.clone();
            let endpoint = EMBED_PRESETS.iter().find(|p| p.id == "ollama").map(|p| p.endpoint).unwrap_or("http://localhost:11434/api/embed").to_string();
            set_ollama_fetching.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                match api.post_text("/proxy/fetch-models", &serde_json::json!({ "endpoint": endpoint })).await {
                    Ok(text) => {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                            let mut names = Vec::new();
                            if let Some(models) = json.get("models").and_then(|m| m.as_array()) {
                                for m in models {
                                    if let Some(name) = m.get("name").and_then(|n| n.as_str()) {
                                        names.push(name.to_string());
                                    }
                                }
                            }
                            if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
                                for m in data {
                                    if let Some(id) = m.get("id").and_then(|n| n.as_str()) {
                                        names.push(id.to_string());
                                    }
                                }
                            }
                            set_ollama_embed_models.set(names);
                        }
                    }
                    Err(_) => {}
                }
                set_ollama_fetching.set(false);
            });
        }
    });

    // ── Section 3: Language Model ──

    let (llm_provider, set_llm_provider) = signal("ollama".to_string());
    let (llm_endpoint, set_llm_endpoint) = signal(String::new());
    let (llm_api_key, set_llm_api_key) = signal(String::new());
    let (llm_model, set_llm_model) = signal(String::new());
    let (llm_system_prompt, set_llm_system_prompt) = signal(String::new());
    let (llm_temperature, set_llm_temperature) = signal("0.7".to_string());
    let (llm_thinking, set_llm_thinking) = signal(false);
    let (llm_test_status, set_llm_test_status) = signal(String::new());
    let (llm_has_key, set_llm_has_key) = signal(false);
    let (llm_fetched_models, set_llm_fetched_models) = signal(Vec::<String>::new());

    Effect::new(move |_| {
        if let Some(cfg) = config.get().flatten() {
            if let Some(v) = cfg.data.get("llm_endpoint").and_then(|v: &serde_json::Value| v.as_str()) {
                set_llm_endpoint.set(v.to_string());
                for p in LLM_PRESETS {
                    if v == p.endpoint {
                        set_llm_provider.set(p.id.to_string());
                        break;
                    }
                }
            }
            if let Some(v) = cfg.data.get("llm_model").and_then(|v: &serde_json::Value| v.as_str()) {
                set_llm_model.set(v.to_string());
            }
            if let Some(v) = cfg.data.get("llm_api_key").and_then(|v: &serde_json::Value| v.as_str()) {
                if !v.is_empty() {
                    set_llm_has_key.set(true);
                }
                set_llm_api_key.set(v.to_string());
            }
            if let Some(v) = cfg.data.get("llm_system_prompt").and_then(|v: &serde_json::Value| v.as_str()) {
                set_llm_system_prompt.set(v.to_string());
            }
            if let Some(v) = cfg.data.get("llm_temperature").and_then(|v: &serde_json::Value| v.as_f64()) {
                set_llm_temperature.set(format!("{v}"));
            }
            if let Some(v) = cfg.data.get("llm_thinking").and_then(|v: &serde_json::Value| v.as_bool()) {
                set_llm_thinking.set(v);
            }
        }
    });

    // ── Section 4: NER ──

    let (ner_provider, set_ner_provider) = signal("builtin".to_string());
    let (ner_endpoint, set_ner_endpoint) = signal(String::new());
    let (ner_model, set_ner_model) = signal(String::new());
    let (ner_selected_model, set_ner_selected_model) = signal(String::new());
    let (ner_model_status, set_ner_model_status) = signal(String::new());
    let (ner_download_status, set_ner_download_status) = signal(String::new());
    let (_rel_model_status, _set_rel_model_status) = signal(String::new());
    let (_rel_download_status, _set_rel_download_status) = signal(String::new());
    let (coref_enabled, set_coref_enabled) = signal(true);
    let (rel_threshold, set_rel_threshold) = signal(0.9_f64);
    let (rel_templates_mode, set_rel_templates_mode) = signal("general".to_string());
    let (relation_templates_json, set_relation_templates_json) = signal(String::new());
    let (_import_status, _set_import_status) = signal(String::new());

    // Quantization signal declared early so config Effect can set it
    let (quant_enabled, set_quant_enabled) = signal(true);

    Effect::new(move |_| {
        if let Some(cfg) = config.get().flatten() {
            if let Some(v) = cfg.data.get("ner_provider").and_then(|v: &serde_json::Value| v.as_str()) {
                // Config may store "gliner2", UI uses "gliner"
                let mapped = if v == "gliner2" { "gliner" } else { v };
                set_ner_provider.set(mapped.to_string());
            }
            if let Some(v) = cfg.data.get("ner_endpoint").and_then(|v: &serde_json::Value| v.as_str()) {
                set_ner_endpoint.set(v.to_string());
            }
            if let Some(v) = cfg.data.get("ner_model").and_then(|v: &serde_json::Value| v.as_str()) {
                set_ner_model.set(v.to_string());
                set_ner_selected_model.set(v.to_string());
            }
            if let Some(v) = cfg.data.get("quantization_enabled").and_then(|v: &serde_json::Value| v.as_bool()) {
                set_quant_enabled.set(v);
            }
            if let Some(v) = cfg.data.get("coreference_enabled").and_then(|v: &serde_json::Value| v.as_bool()) {
                set_coref_enabled.set(v);
            }
            if let Some(v) = cfg.data.get("rel_threshold").and_then(|v: &serde_json::Value| v.as_f64()) {
                set_rel_threshold.set(v);
            }
            if let Some(v) = cfg.data.get("relation_templates") {
                if let Ok(json) = serde_json::to_string_pretty(v) {
                    set_relation_templates_json.set(json);
                }
            }
        }
    });

    // ── Section 6: Mesh (peers) ──

    let (peer_key, set_peer_key) = signal(String::new());
    let (peer_endpoint, set_peer_endpoint) = signal(String::new());

    let api_add_peer = api.clone();
    let add_peer = Action::new_local(move |_: &()| {
        let api = api_add_peer.clone();
        let key = peer_key.get_untracked();
        let endpoint = peer_endpoint.get_untracked();
        async move {
            let body = serde_json::json!({ "key": key, "endpoint": endpoint });
            match api.post_text("/mesh/peers", &body).await {
                Ok(_) => {
                    set_status_msg.set("Peer added.".to_string());
                    set_peer_key.set(String::new());
                    set_peer_endpoint.set(String::new());
                }
                Err(e) => set_status_msg.set(format!("Add peer error: {e}")),
            }
        }
    });

    // ── Section 8: Import/Export ──

    let (import_text, set_import_text) = signal(String::new());

    // ── Status indicators (derived from loaded config/compute) ──

    let embed_status: Signal<String> = Signal::derive(move || {
        let ep = embed_endpoint.get();
        let model_cfg = embed_model.get();
        let provider_name = EMBED_PRESETS.iter()
            .find(|p| p.endpoint == ep)
            .map(|p| p.name)
            .unwrap_or("");
        let model_from_compute = compute.get().flatten().and_then(|c| c.embedder_model);
        let model_name = model_from_compute.as_deref()
            .or_else(|| if !model_cfg.is_empty() { Some(model_cfg.as_str()) } else { None });
        if ep.starts_with("onnx://") {
            match model_name {
                Some(m) => format!("ONNX Local | {m}"),
                None => "ONNX Local".into(),
            }
        } else if !ep.is_empty() {
            match model_name {
                Some(m) => format!("{} | {m}", provider_name),
                None => provider_name.to_string(),
            }
        } else {
            "not configured".into()
        }
    });

    let llm_status: Signal<String> = Signal::derive(move || {
        config.get().flatten()
            .and_then(|cfg| {
                let ep = cfg.data.get("llm_endpoint").and_then(|v| v.as_str()).unwrap_or("");
                let model = cfg.data.get("llm_model").and_then(|v| v.as_str()).unwrap_or("");
                if ep.is_empty() { None }
                else if !model.is_empty() { Some(model.to_string()) }
                else { Some("configured".into()) }
            })
            .unwrap_or_else(|| "not configured".into())
    });

    let ner_status: Signal<String> = Signal::derive(move || {
        match ner_provider.get().as_str() {
            "builtin" => "Built-in".into(),
            "gliner" => {
                let m = ner_model.get();
                if m.is_empty() { "GLiNER (ONNX)".into() } else { format!("GLiNER | {m}") }
            },
            other => other.to_string(),
        }
    });

    let secrets_status: Signal<String> = Signal::derive(move || {
        let count = secrets.get().map(|v| v.len()).unwrap_or(0);
        if count > 0 { format!("{count} keys") } else { "No secrets".into() }
    });

    // ── View ──

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-sliders"></i>" System"</h2>
            <p class="text-secondary">"Control panel -- connection, models, mesh, secrets, data"</p>
        </div>

        {move || {
            let msg = status_msg.get();
            (!msg.is_empty()).then(|| view! {
                <div class="alert">{msg}</div>
            })
        }}

        // ── Tab bar ──
        <div class="system-tabs">
            <button class=move || if active_tab.get() == "system" { "system-tab active" } else { "system-tab" }
                on:click=move |_| set_active_tab.set("system".into())>
                <i class="fa-solid fa-sliders"></i>" System"
            </button>
            <button class=move || if active_tab.get() == "mesh" { "system-tab active" } else { "system-tab" }
                on:click=move |_| set_active_tab.set("mesh".into())>
                <i class="fa-solid fa-share-nodes"></i>" Mesh"
            </button>
        </div>

        // ── System tab: 3x2 card grid ──
        <div style=move || if active_tab.get() == "system" { "" } else { "display:none" }>
            <div class="system-grid">
                // ── Card: Embeddings ──
                <div class="system-card" on:click=move |_| set_modal_open.set("embedding".into())>
                    <div class="system-card-header">
                        <span class="system-card-icon"><i class="fa-solid fa-circle-nodes"></i></span>
                        <span class="system-card-title">"Embeddings"</span>
                    </div>
                    <div class="system-card-status">{move || {
                        let st = embed_status.get();
                        let dot = if st != "not configured" { "status-dot green" } else { "status-dot gray" };
                        view! { <span class={dot}></span>{st} }
                    }}</div>
                </div>
                // ── Card: NER & Relations ──
                <div class="system-card" on:click=move |_| set_modal_open.set("ner".into())>
                    <div class="system-card-header">
                        <span class="system-card-icon"><i class="fa-solid fa-tags"></i></span>
                        <span class="system-card-title">"NER & Relations"</span>
                    </div>
                    <div class="system-card-status">{move || {
                        let st = ner_status.get();
                        view! { <span class="status-dot green"></span>{st} }
                    }}</div>
                </div>
                // ── Card: Language Model ──
                <div class="system-card" on:click=move |_| set_modal_open.set("llm".into())>
                    <div class="system-card-header">
                        <span class="system-card-icon"><i class="fa-solid fa-comments"></i></span>
                        <span class="system-card-title">"Language Model"</span>
                    </div>
                    <div class="system-card-status">{move || {
                        let st = llm_status.get();
                        let dot = if st != "not configured" { "status-dot green" } else { "status-dot gray" };
                        view! { <span class={dot}></span>{st} }
                    }}</div>
                </div>
                // ── Card: Secrets ──
                <div class="system-card" on:click=move |_| set_modal_open.set("secrets".into())>
                    <div class="system-card-header">
                        <span class="system-card-icon"><i class="fa-solid fa-key"></i></span>
                        <span class="system-card-title">"Secrets"</span>
                    </div>
                    <div class="system-card-status">{move || {
                        let st = secrets_status.get();
                        let dot = if st == "No secrets" { "status-dot gray" } else { "status-dot green" };
                        view! { <span class={dot}></span>{st} }
                    }}</div>
                </div>
                // ── Card: Database ──
                <div class="system-card" on:click=move |_| set_modal_open.set("database".into())>
                    <div class="system-card-header">
                        <span class="system-card-icon"><i class="fa-solid fa-database"></i></span>
                        <span class="system-card-title">"Database"</span>
                    </div>
                    <div class="system-card-status">{move || {
                        let txt = stats.get().flatten()
                            .map(|s| format!("{} nodes, {} edges", s.nodes, s.edges))
                            .unwrap_or_else(|| "Loading...".into());
                        view! { <span class="status-dot green"></span>{txt} }
                    }}</div>
                </div>
            </div>
        </div>

        // ── Mesh tab ──
        <div style=move || if active_tab.get() == "mesh" { "" } else { "display:none" }>
            <div style="text-align: center; padding: 3rem 0;">
                <i class="fa-solid fa-share-nodes" style="font-size: 3rem; color: var(--text-muted); margin-bottom: 1rem; display: block;"></i>
                <h3>"Mesh Networking"</h3>
                <p class="text-secondary" style="max-width: 500px; margin: 0.5rem auto;">
                    "Peer discovery, federated queries, knowledge profiles, and distributed sync across engram instances. This feature is under active development."
                </p>
                <span class="badge badge-archival" style="margin-top: 1rem;">"Coming Soon"</span>
            </div>
        </div>

        // ══════════════════════════════════════
        //  MODALS
        // ══════════════════════════════════════

        // ── Modal: Embedding ──
        <div class=move || if modal_open.get() == "embedding" { "modal-overlay active" } else { "modal-overlay" }>
            <div class="wizard-modal">
                <div class="wizard-modal-header">
                    <h3><i class="fa-solid fa-circle-nodes"></i>" Embeddings"</h3>
                    <button class="btn btn-secondary btn-sm" on:click=move |_| set_modal_open.set(String::new())>
                        <i class="fa-solid fa-xmark"></i>
                    </button>
                </div>
                <div class="wizard-modal-body">
                    {embedding::render_embedding_modal(
                        api.clone(), stats, compute,
                        embed_provider, set_embed_provider,
                        embed_endpoint, set_embed_endpoint,
                        embed_model, set_embed_model,
                        embed_test_status, set_embed_test_status,
                        onnx_status, set_onnx_status,
                        ollama_embed_models, ollama_fetching,
                        set_status_msg, set_modal_open,
                    )}
                </div>
            </div>
        </div>

        // ── Modal: Language Model ──
        <div class=move || if modal_open.get() == "llm" { "modal-overlay active" } else { "modal-overlay" }>
            <div class="wizard-modal">
                <div class="wizard-modal-header">
                    <h3><i class="fa-solid fa-comments"></i>" Language Model"</h3>
                    <button class="btn btn-secondary btn-sm" on:click=move |_| set_modal_open.set(String::new())>
                        <i class="fa-solid fa-xmark"></i>
                    </button>
                </div>
                <div class="wizard-modal-body">
                    {llm::render_llm_modal(
                        api.clone(),
                        llm_provider, set_llm_provider,
                        llm_endpoint, set_llm_endpoint,
                        llm_api_key, set_llm_api_key,
                        llm_model, set_llm_model,
                        llm_system_prompt, set_llm_system_prompt,
                        llm_temperature, set_llm_temperature,
                        llm_thinking, set_llm_thinking,
                        llm_test_status, set_llm_test_status,
                        llm_has_key,
                        llm_fetched_models, set_llm_fetched_models,
                        set_status_msg, set_modal_open,
                    )}
                </div>
            </div>
        </div>

        // ── Modal: NER & Relations ──
        <div class=move || if modal_open.get() == "ner" { "modal-overlay active" } else { "modal-overlay" }>
            <div class="wizard-modal">
                <div class="wizard-modal-header">
                    <h3><i class="fa-solid fa-tags"></i>" NER & Relations"</h3>
                    <button class="btn btn-secondary btn-sm" on:click=move |_| set_modal_open.set(String::new())>
                        <i class="fa-solid fa-xmark"></i>
                    </button>
                </div>
                <div class="wizard-modal-body">
                    {ner::render_ner_modal(
                        api.clone(),
                        ner_provider, set_ner_provider,
                        ner_model, set_ner_model,
                        ner_selected_model, set_ner_selected_model,
                        ner_model_status, set_ner_model_status,
                        ner_download_status, set_ner_download_status,
                        coref_enabled,
                        rel_threshold, set_rel_threshold,
                        rel_templates_mode, set_rel_templates_mode,
                        relation_templates_json, set_relation_templates_json,
                        ner_endpoint,
                        set_status_msg, set_modal_open,
                    )}
                </div>
            </div>
        </div>

        // ── Mesh section (hidden - moved to Mesh tab) ──
        <div style="display:none">
            // "Not enabled" state with description
            {move || {
                let peer_count = peers.get().map(|p| p.len()).unwrap_or(0);
                (peer_count == 0).then(|| view! {
                    <div style="text-align: center; padding: 1.5rem 0;">
                        <i class="fa-solid fa-diagram-project" style="font-size: 2.5rem; color: var(--text-muted); margin-bottom: 0.75rem; display: block;"></i>
                        <h4 style="margin: 0 0 0.5rem;">"Mesh Networking Not Enabled"</h4>
                        <p class="text-secondary" style="max-width: 400px; margin: 0 auto 1rem;">
                            "Mesh networking allows engram instances to sync knowledge, creating a distributed knowledge graph."
                        </p>
                    </div>
                    <div style="margin-bottom: 1rem;">
                        <div style="display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.5rem;">
                            <i class="fa-solid fa-sync" style="color: var(--accent-bright);"></i>
                            <span class="text-secondary">"Sync facts, relationships, and confidence scores between instances"</span>
                        </div>
                        <div style="display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.5rem;">
                            <i class="fa-solid fa-shield-halved" style="color: var(--accent-bright);"></i>
                            <span class="text-secondary">"Zero-trust security with ed25519 identity and topic-level ACLs"</span>
                        </div>
                        <div style="display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.5rem;">
                            <i class="fa-solid fa-magnifying-glass" style="color: var(--accent-bright);"></i>
                            <span class="text-secondary">"Federated queries across the mesh for distributed knowledge search"</span>
                        </div>
                    </div>
                    <div style="margin-bottom: 0.75rem;">
                        <p style="font-size: 0.8rem; font-weight: 600; margin-bottom: 0.5rem; text-transform: uppercase; color: var(--text-muted);">"Topology"</p>
                        <div style="display: flex; flex-direction: column; gap: 0.4rem;">
                            <label style="display: flex; align-items: center; gap: 0.5rem; cursor: pointer;">
                                <input type="radio" name="mesh_topology" value="star" checked />
                                <span><strong>"Star"</strong>" \u{2014} one hub, many spokes. Simple, centralized sync."</span>
                            </label>
                            <label style="display: flex; align-items: center; gap: 0.5rem; cursor: pointer;">
                                <input type="radio" name="mesh_topology" value="full" />
                                <span><strong>"Full mesh"</strong>" \u{2014} every node connects to every other."</span>
                            </label>
                            <label style="display: flex; align-items: center; gap: 0.5rem; cursor: pointer;">
                                <input type="radio" name="mesh_topology" value="ring" />
                                <span><strong>"Ring"</strong>" \u{2014} each node syncs with two neighbors."</span>
                            </label>
                        </div>
                    </div>
                    <button class="btn btn-primary" on:click=move |_| set_status_msg.set("Mesh enabled. Restart the engram server to activate mesh endpoints.".into())>
                        <i class="fa-solid fa-power-off"></i>" Enable Mesh"
                    </button>
                    <p class="text-secondary" style="font-size: 0.75rem; margin-top: 0.5rem;">
                        <i class="fa-solid fa-circle-info" style="margin-right: 0.25rem;"></i>
                        "After enabling, restart the engram server to activate mesh endpoints."
                    </p>
                })
            }}
            {move || {
                let identity = mesh_identity.get().unwrap_or_default();
                (!identity.is_empty()).then(|| view! {
                    <div class="info-box" style="margin-bottom: 0.75rem;">
                        <i class="fa-solid fa-fingerprint"></i>" Identity: "
                        <code style="word-break: break-all;">{identity}</code>
                    </div>
                })
            }}
            <h4>"Add Peer"</h4>
            <div class="form-row">
                <label>"Public Key"</label>
                <input
                    type="text"
                    placeholder="ed25519 public key..."
                    prop:value=peer_key
                    on:input=move |ev| set_peer_key.set(event_target_value(&ev))
                />
            </div>
            <div class="form-row">
                <label>"Endpoint"</label>
                <input
                    type="text"
                    placeholder="https://peer.example.com:3030"
                    prop:value=peer_endpoint
                    on:input=move |ev| set_peer_endpoint.set(event_target_value(&ev))
                />
            </div>
            <div class="button-group">
                <button class="btn btn-primary" on:click=move |_| { add_peer.dispatch(()); }>
                    <i class="fa-solid fa-plus"></i>" Add Peer"
                </button>
            </div>

            <h4 style="margin-top: 1rem;">"Peers"</h4>
            {move || {
                let list = peers.get().unwrap_or_default();
                if list.is_empty() {
                    view! {
                        <p class="text-muted">"No peers configured."</p>
                    }.into_any()
                } else {
                    view! {
                        <table class="data-table">
                            <thead>
                                <tr>
                                    <th>"Key"</th>
                                    <th>"Endpoint"</th>
                                    <th>"Trust"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {list.into_iter().map(|p| {
                                    let key_short = if p.key.len() > 16 {
                                        format!("{}...", &p.key[..16])
                                    } else {
                                        p.key.clone()
                                    };
                                    let ep = p.endpoint.clone().unwrap_or_default();
                                    let trust = p.trust_level.clone().unwrap_or_else(|| "default".to_string());
                                    view! {
                                        <tr>
                                            <td title={p.key.clone()}><code>{key_short}</code></td>
                                            <td>{ep}</td>
                                            <td>{trust}</td>
                                        </tr>
                                    }
                                }).collect::<Vec<_>>()}
                            </tbody>
                        </table>
                    }.into_any()
                }
            }}

            <h4 style="margin-top: 1rem;">"Audit Log"</h4>
            {move || {
                let entries = audit_log.get().unwrap_or_default();
                if entries.is_empty() {
                    view! {
                        <p class="text-muted">"No audit entries."</p>
                    }.into_any()
                } else {
                    view! {
                        <table class="data-table">
                            <thead>
                                <tr>
                                    <th>"Timestamp"</th>
                                    <th>"Peer"</th>
                                    <th>"Action"</th>
                                    <th>"Details"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {entries.into_iter().map(|e| {
                                    let ts = e.timestamp.clone().unwrap_or_default();
                                    let peer = e.peer.clone().unwrap_or_default();
                                    let action = e.action.clone().unwrap_or_default();
                                    let details = e.details.clone().unwrap_or_default();
                                    view! {
                                        <tr>
                                            <td>{ts}</td>
                                            <td>{peer}</td>
                                            <td>{action}</td>
                                            <td>{details}</td>
                                        </tr>
                                    }
                                }).collect::<Vec<_>>()}
                            </tbody>
                        </table>
                    }.into_any()
                }
            }}
        </div> // hidden mesh section

        // ── Modal: Secrets ──
        <div class=move || if modal_open.get() == "secrets" { "modal-overlay active" } else { "modal-overlay" }>
            <div class="wizard-modal">
                <div class="wizard-modal-header">
                    <h3><i class="fa-solid fa-key"></i>" Secrets"</h3>
                    <button class="btn btn-secondary btn-sm" on:click=move |_| set_modal_open.set(String::new())>
                        <i class="fa-solid fa-xmark"></i>
                    </button>
                </div>
                <div class="wizard-modal-body">
                    {secrets::render_secrets_modal(
                        api.clone(), secrets,
                        set_status_msg,
                    )}
                </div>
            </div>
        </div>

        // ── Modal: Database ──
        <div class=move || if modal_open.get() == "database" { "modal-overlay active" } else { "modal-overlay" }>
            <div class="wizard-modal">
                <div class="wizard-modal-header">
                    <h3><i class="fa-solid fa-database"></i>" Database"</h3>
                    <button class="btn btn-secondary btn-sm" on:click=move |_| set_modal_open.set(String::new())>
                        <i class="fa-solid fa-xmark"></i>
                    </button>
                </div>
                <div class="wizard-modal-body">
                    {database::render_database_modal(
                        api.clone(),
                        import_text, set_import_text,
                        set_status_msg,
                    )}
                </div>
            </div>
        </div>
    }
}
