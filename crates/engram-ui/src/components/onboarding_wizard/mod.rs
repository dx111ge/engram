use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{AnalyzeRequest, AnalyzeResponse, IngestRequest, IngestItem, IngestResponse};

mod presets;
mod steps_pipeline;
mod steps_config;

use presets::*;

// ── Step constants ──

const STEP_WELCOME: u32 = 1;
const STEP_EMBEDDER: u32 = 2;
const STEP_NER: u32 = 3;
const STEP_REL: u32 = 4;
const STEP_LLM: u32 = 5;       // Mandatory — required for AoI detection + entity disambiguation
const STEP_QUANTIZATION: u32 = 6;
const STEP_KB_SOURCES: u32 = 7;
const STEP_WEB_SEARCH: u32 = 8; // NEW: web search provider config
const STEP_SEED: u32 = 9;
const STEP_READY: u32 = 10;
const TOTAL_STEPS: u32 = 10;

// ── Quality score weights ──

fn quality_score(
    has_embedder: bool,
    has_ner: bool,
    has_llm: bool,
    has_quantization: bool,
    has_kb: bool,
    has_seed: bool,
) -> u32 {
    let mut score = 0u32;
    if has_embedder { score += 20; }
    if has_ner { score += 20; }
    if has_kb { score += 20; }
    if has_llm { score += 20; }    // Mandatory: area-of-interest detection + entity disambiguation
    if has_quantization { score += 5; }
    if has_seed { score += 15; }
    score
}

#[component]
pub fn OnboardingWizard(
    #[prop(into)] open: ReadSignal<bool>,
    #[prop(into)] on_complete: Callback<()>,
) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    // Step state
    let (step, set_step) = signal(STEP_WELCOME);

    // Choices
    let (embed_choice, set_embed_choice) = signal(String::new());
    let (embed_key, set_embed_key) = signal(String::new());
    let (embed_model, set_embed_model) = signal(String::new());
    let (embed_endpoint, set_embed_endpoint) = signal(String::new());
    let (ner_choice, set_ner_choice) = signal("gliner2".to_string());
    let (ner_model, set_ner_model) = signal("gliner2-fp16".to_string());
    let (llm_choice, set_llm_choice) = signal(String::new());
    let (llm_key, set_llm_key) = signal(String::new());
    let (llm_model, set_llm_model) = signal(String::new());
    let (rel_threshold, set_rel_threshold) = signal(0.85_f64);
    let (rel_model_choice, set_rel_model_choice) = signal("gliner2".to_string());
    let (rel_download_progress, set_rel_download_progress) = signal(String::new());
    let (rel_custom_templates_json, set_rel_custom_templates_json) = signal(String::new());
    let (rel_templates_mode, set_rel_templates_mode) = signal("general".to_string());
    let (quant_choice, set_quant_choice) = signal("int8".to_string());
    let (kb_wikidata, set_kb_wikidata) = signal(true);
    let (kb_dbpedia, set_kb_dbpedia) = signal(false);
    // Source trust defaults: Vec<(type_key, value_0_100)>
    let source_trust_values = RwSignal::new(Vec::<(String, u32)>::new());
    let (seed_text, set_seed_text) = signal(String::new());
    // Web search step
    let (web_search_provider, set_web_search_provider) = signal("duckduckgo".to_string());
    let (web_search_api_key, set_web_search_api_key) = signal(String::new());
    let (web_search_url, set_web_search_url) = signal(String::new());
    let (web_search_test_result, set_web_search_test_result) = signal(Option::<String>::None);
    let (web_search_testing, set_web_search_testing) = signal(false);
    // Seed enrichment interactive state
    let (seed_phase, set_seed_phase) = signal(0u32);  // 0=input, 1=aoi+entities
    let (seed_aoi, set_seed_aoi) = signal(String::new());
    let (seed_session_id, set_seed_session_id) = signal(String::new());
    // Entity list: Vec<(label, type, confidence, skipped)>
    let (seed_entities, set_seed_entities) = signal(Vec::<(String, String, f32, bool)>::new());
    // New entity input
    let (new_entity_label, set_new_entity_label) = signal(String::new());
    let (new_entity_type, set_new_entity_type) = signal("entity".to_string());
    // Relation review state (seed phase 2)
    let (seed_review_connections, set_seed_review_connections) = signal(
        Vec::<crate::components::relation_review::ReviewConnection>::new()
    );
    let (seed_known_rel_types, set_seed_known_rel_types) = signal(Vec::<String>::new());
    let (seed_review_submitting, set_seed_review_submitting) = signal(false);

    // Ollama model fetching
    let (ollama_embed_models, set_ollama_embed_models) = signal(Vec::<String>::new());
    let (ollama_llm_models, set_ollama_llm_models) = signal(Vec::<String>::new());
    let (ollama_fetching, set_ollama_fetching) = signal(false);

    // Auto-fetch Ollama models when Ollama is selected for embed or LLM
    // Uses backend proxy to reach Ollama (which could be on any host)
    let api_ollama = api.clone();
    Effect::new(move || {
        let embed = embed_choice.get();
        let llm = llm_choice.get();
        if (embed == "ollama" && ollama_embed_models.get_untracked().is_empty())
            || (llm == "ollama" && ollama_llm_models.get_untracked().is_empty())
        {
            let api = api_ollama.clone();
            let needs_embed = embed == "ollama";
            let needs_llm = llm == "ollama";
            // Get the endpoint from the preset
            let endpoint = if needs_embed {
                EMBED_PRESETS.iter().find(|p| p.id == "ollama").map(|p| p.endpoint).unwrap_or("http://localhost:11434/api/embed")
            } else {
                LLM_PRESETS.iter().find(|p| p.id == "ollama").map(|p| p.endpoint).unwrap_or("http://localhost:11434/v1/chat/completions")
            };
            let endpoint = endpoint.to_string();
            set_ollama_fetching.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                match api.post_text("/proxy/fetch-models", &serde_json::json!({ "endpoint": endpoint })).await {
                    Ok(text) => {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                            let mut names = Vec::new();
                            // Ollama-style: { "models": [{ "name": "..." }] }
                            if let Some(models) = json.get("models").and_then(|m| m.as_array()) {
                                for m in models {
                                    if let Some(name) = m.get("name").and_then(|n| n.as_str()) {
                                        names.push(name.to_string());
                                    }
                                }
                            }
                            // OpenAI-style: { "data": [{ "id": "..." }] }
                            if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
                                for m in data {
                                    if let Some(id) = m.get("id").and_then(|n| n.as_str()) {
                                        names.push(id.to_string());
                                    }
                                }
                            }
                            if needs_embed { set_ollama_embed_models.set(names.clone()); }
                            if needs_llm { set_ollama_llm_models.set(names); }
                        }
                    }
                    Err(_) => {} // Silently fail - user can still type manually
                }
                set_ollama_fetching.set(false);
            });
        }
    });

    // Web search test — saves config then hits the proxy to verify results come back
    let api_ws_test = api.clone();
    let do_web_search_test = Action::new_local(move |_: &()| {
        let api = api_ws_test.clone();
        let provider = web_search_provider.get_untracked();
        let url = web_search_url.get_untracked();
        let api_key = web_search_api_key.get_untracked();
        async move {
            set_web_search_testing.set(true);
            set_web_search_test_result.set(None);

            // Validate inputs first
            let validation: Result<(), String> = match provider.as_str() {
                "brave" if api_key.is_empty() => Err("API key is required for Brave Search".into()),
                "brave" if api_key.len() < 20 => Err("API key looks too short".into()),
                "searxng" if url.is_empty() => Err("SearXNG URL is required".into()),
                "searxng" if !url.starts_with("http://") && !url.starts_with("https://") => Err("URL must start with http:// or https://".into()),
                _ => Ok(()),
            };

            let test_result = match validation {
                Err(e) => Err(e),
                Ok(()) => {
                    // Save config so proxy routes correctly
                    let config = serde_json::json!({
                        "web_search_provider": &provider,
                        "web_search_api_key": if api_key.is_empty() { None } else { Some(&api_key) },
                        "web_search_url": if url.is_empty() { None } else { Some(&url) },
                    });
                    let _ = api.post_text("/config", &config).await;

                    if provider == "duckduckgo" {
                        Ok("DuckDuckGo requires no configuration".to_string())
                    } else {
                        // Test via proxy which now routes by provider
                        match api.get_text("/proxy/search?q=test&limit=3").await {
                            Ok(resp) => {
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&resp) {
                                    let count = json.get("results").and_then(|r| r.as_array()).map(|a| a.len()).unwrap_or(0);
                                    if count > 0 {
                                        Ok(format!("Connected ({count} results)"))
                                    } else {
                                        Err(format!("No results from {} -- check configuration", provider))
                                    }
                                } else {
                                    Err("Invalid response from search proxy".into())
                                }
                            }
                            Err(e) => Err(format!("Connection failed: {e}"))
                        }
                    }
                }
            };

            match test_result {
                Ok(msg) => set_web_search_test_result.set(Some(msg)),
                Err(msg) => set_web_search_test_result.set(Some(format!("FAIL: {msg}"))),
            }
            set_web_search_testing.set(false);
        }
    });

    // State
    let (saving, set_saving) = signal(false);
    let (save_error, set_save_error) = signal(Option::<String>::None);
    let (seed_result, set_seed_result) = signal(Option::<String>::None);
    let (analyzing, set_analyzing) = signal(false);

    // Quality meter
    let quality = Signal::derive(move || {
        quality_score(
            !embed_choice.get().is_empty(),
            !ner_choice.get().is_empty(),
            !llm_choice.get().is_empty(),
            !quant_choice.get().is_empty(),
            kb_wikidata.get() || kb_dbpedia.get(),
            !seed_text.get().trim().is_empty(),
        )
    });

    let overlay_class = move || {
        if open.get() { "modal-overlay active" } else { "modal-overlay" }
    };

    // Save current step's config to backend
    let api_save = api.clone();
    let save_step_config = Action::new_local(move |step_num: &u32| {
        let api = api_save.clone();
        let step_num = *step_num;
        let embed = embed_choice.get_untracked();
        let embed_k = embed_key.get_untracked();
        let embed_m = embed_model.get_untracked();
        let embed_ep = embed_endpoint.get_untracked();
        let ner = ner_choice.get_untracked();
        let ner_m = ner_model.get_untracked();
        let rel_thresh = rel_threshold.get_untracked();
        let rel_m = rel_model_choice.get_untracked();
        let rel_tpl_mode = rel_templates_mode.get_untracked();
        let rel_custom_tpl = rel_custom_templates_json.get_untracked();
        let llm = llm_choice.get_untracked();
        let llm_k = llm_key.get_untracked();
        let llm_m = llm_model.get_untracked();
        let quant = quant_choice.get_untracked();
        let wiki = kb_wikidata.get_untracked();
        let dbp = kb_dbpedia.get_untracked();
        async move {
            set_saving.set(true);
            set_save_error.set(None);

            let result = match step_num {
                STEP_EMBEDDER => {
                    if embed.is_empty() { set_save_error.set(Some("Please select an embedding provider".into())); set_saving.set(false); return false; }
                    if embed == "custom" && embed_ep.trim().is_empty() { set_save_error.set(Some("Please enter the provider's endpoint URL".into())); set_saving.set(false); return false; }
                    let preset = EMBED_PRESETS.iter().find(|p| p.id == embed);
                    let endpoint = if embed == "custom" { embed_ep.as_str() } else { preset.map(|p| p.endpoint).unwrap_or("") };
                    let mut config = serde_json::json!({ "embed_endpoint": endpoint });
                    if !embed_k.is_empty() { config["embed_api_key"] = serde_json::json!(embed_k); }
                    if !embed_m.is_empty() { config["embed_model"] = serde_json::json!(embed_m); }
                    if embed == "onnx" && !embed_m.is_empty() {
                        // Map model name to HuggingFace repo (presets or custom)
                        let hf_repo = match embed_m.as_str() {
                            "multilingual-e5-small" => "intfloat/multilingual-e5-small".to_string(),
                            "bge-small-en-v1.5" => "BAAI/bge-small-en-v1.5".to_string(),
                            "all-MiniLM-L6-v2" => "sentence-transformers/all-MiniLM-L6-v2".to_string(),
                            custom => {
                                // Custom HuggingFace model ID (e.g. "sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2")
                                if custom.contains('/') { custom.to_string() } else { format!("sentence-transformers/{}", custom) }
                            }
                        };
                        let onnx_path = "onnx/model.onnx";
                        let dl = api.post_text("/config/onnx-download", &serde_json::json!({
                            "model_url": format!("https://huggingface.co/{}/resolve/main/{}", hf_repo, onnx_path),
                            "tokenizer_url": format!("https://huggingface.co/{}/resolve/main/tokenizer.json", hf_repo),
                        })).await;
                        if let Err(e) = dl {
                            set_save_error.set(Some(format!("ONNX download failed: {e}. You can configure this later in System settings.")));
                        }
                    } else if embed == "ollama" && !embed_m.is_empty() {
                        // Only pull if not already installed
                        let installed = ollama_embed_models.get_untracked();
                        let already_have = installed.iter().any(|m| m == &embed_m || m.starts_with(&format!("{}:", embed_m)));
                        if !already_have {
                            let dl = api.post_text("/config/ollama-pull", &serde_json::json!({ "model": embed_m })).await;
                            if let Err(e) = dl {
                                set_save_error.set(Some(format!("Ollama pull failed: {e}. Make sure Ollama is running and pull '{}' manually.", embed_m)));
                            }
                        }
                    }
                    api.post_text("/config", &config).await
                }
                STEP_NER => {
                    let provider = if ner == "gliner2" { "gliner2" } else { &ner };
                    let mut config = serde_json::json!({
                        "ner_provider": provider,
                    });
                    if ner == "gliner2" && !ner_m.is_empty() {
                        config["ner_model"] = serde_json::json!(&ner_m);

                        // Download GLiNER2 ONNX model (unified NER+RE)
                        let variant = if ner_m.contains("fp32") { "fp32" } else { "fp16" };
                        let ner_dl = api.post_text("/config/gliner2-download", &serde_json::json!({
                            "repo_id": "dx111ge/gliner2-multi-v1-onnx",
                            "variant": variant,
                        })).await;
                        if let Err(e) = &ner_dl {
                            set_save_error.set(Some(format!("GLiNER2 model download note: {e}. Model will be downloaded on first use.")));
                        }
                    }
                    api.post_text("/config", &config).await
                }
                STEP_REL => {
                    // GLiNER2 handles both NER and RE -- NLI step just saves threshold config
                    let config = serde_json::json!({
                        "rel_threshold": rel_thresh,
                    });

                    // Import custom templates if provided
                    if rel_tpl_mode == "custom" && !rel_custom_tpl.trim().is_empty() {
                        if let Ok(templates) = serde_json::from_str::<serde_json::Value>(&rel_custom_tpl) {
                            let import_body = serde_json::json!({
                                "templates": templates,
                                "threshold": rel_thresh,
                            });
                            let _ = api.post_text("/config/relation-templates/import", &import_body).await;
                        }
                    }

                    api.post_text("/config", &config).await
                }
                STEP_LLM => {
                    if llm.is_empty() {
                        set_save_error.set(Some("LLM is required for area-of-interest detection and entity disambiguation. Please select a provider.".into()));
                        set_saving.set(false);
                        return false;
                    }
                    let preset = LLM_PRESETS.iter().find(|p| p.id == llm);
                    let endpoint = preset.map(|p| p.endpoint).unwrap_or("");
                    let mut config = serde_json::json!({ "llm_endpoint": endpoint });
                    if !llm_k.is_empty() { config["llm_api_key"] = serde_json::json!(llm_k); }
                    if !llm_m.is_empty() { config["llm_model"] = serde_json::json!(llm_m); }
                    // Pull model in Ollama if selected and not already installed
                    if llm == "ollama" && !llm_m.is_empty() {
                        let installed = ollama_llm_models.get_untracked();
                        let already_have = installed.iter().any(|m| m == &llm_m || m.starts_with(&format!("{}:", llm_m)));
                        if !already_have {
                            let dl = api.post_text("/config/ollama-pull", &serde_json::json!({ "model": llm_m })).await;
                            if let Err(e) = dl {
                                set_save_error.set(Some(format!("Ollama pull failed: {e}. Make sure Ollama is running and pull '{}' manually.", llm_m)));
                            }
                        }
                    }
                    api.post_text("/config", &config).await
                }
                STEP_QUANTIZATION => {
                    let enabled = !quant.is_empty() && quant != "off";
                    api.post_text("/quantize", &serde_json::json!({ "enabled": enabled })).await
                }
                STEP_KB_SOURCES => {
                    if !wiki && !dbp {
                        set_save_error.set(Some("Enable at least one knowledge source for best results. Without a KB, relation extraction on an empty graph produces no edges.".into()));
                        set_saving.set(false);
                        return false;
                    }
                    let mut ok = true;
                    if wiki {
                        let r = api.post_text("/config/kb", &serde_json::json!({
                            "name": "wikidata",
                            "url": "https://query.wikidata.org/sparql",
                            "auth_type": "none",
                            "enabled": true
                        })).await;
                        if r.is_err() { ok = false; }
                    }
                    if dbp {
                        let r = api.post_text("/config/kb", &serde_json::json!({
                            "name": "dbpedia",
                            "url": "https://dbpedia.org/sparql",
                            "auth_type": "none",
                            "enabled": true
                        })).await;
                        if r.is_err() { ok = false; }
                    }
                    // Save source trust defaults if any were customized
                    let trust_vals = source_trust_values.get_untracked();
                    if !trust_vals.is_empty() {
                        let mut trust_map = serde_json::Map::new();
                        for (key, val) in &trust_vals {
                            trust_map.insert(key.clone(), serde_json::json!((*val as f64) / 100.0));
                        }
                        let _ = api.post_text("/config", &serde_json::json!({
                            "source_trust_defaults": trust_map,
                        })).await;
                    }
                    if ok { Ok("ok".into()) } else { Err(crate::api::ApiError::Network("KB config failed".into())) }
                }
                STEP_WEB_SEARCH => {
                    let provider = web_search_provider.get_untracked();
                    let mut config = serde_json::json!({ "web_search_provider": provider });
                    let api_key = web_search_api_key.get_untracked();
                    let url = web_search_url.get_untracked();
                    if provider == "brave" && !api_key.is_empty() {
                        config["web_search_api_key"] = serde_json::json!(api_key);
                    }
                    if provider == "searxng" && !url.is_empty() {
                        config["web_search_url"] = serde_json::json!(url);
                    }
                    api.post_text("/config", &config).await
                }
                _ => Ok("ok".into()),
            };

            set_saving.set(false);
            match result {
                Ok(_) => true,
                Err(e) => {
                    let msg = format!("{e}");
                    let friendly = if msg.contains("probe failed") || msg.contains("unsupported URL scheme") {
                        format!("Configuration saved, but the provider is not reachable yet. You can fix this later in System settings. (Details: {msg})")
                    } else if msg.contains("timeout") || msg.contains("Timeout") {
                        format!("The download timed out. Check your internet connection and try again, or configure this later in System settings.")
                    } else if msg.contains("connection refused") || msg.contains("Connection refused") {
                        format!("Could not connect to the provider. Make sure it is running and try again.")
                    } else {
                        format!("Configuration failed: {msg}")
                    };
                    set_save_error.set(Some(friendly));
                    false
                }
            }
        }
    });

    // Analyze seed text — uses seed/start for AoI detection + NER
    let api_analyze = api.clone();
    let do_analyze = Action::new_local(move |_: &()| {
        let api = api_analyze.clone();
        let text = seed_text.get_untracked();
        async move {
            if text.trim().is_empty() { return; }
            set_analyzing.set(true);
            set_seed_result.set(None);
            set_seed_entities.set(Vec::new());
            set_seed_phase.set(0);

            // Try the new seed/start endpoint first
            let body = serde_json::json!({ "text": text });
            match api.post_text("/ingest/seed/start", &body).await {
                Ok(resp_text) => {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&resp_text) {
                        let session_id = json.get("session_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let aoi = json.get("area_of_interest").and_then(|v| v.as_str()).unwrap_or("").to_string();

                        // Parse entities into reactive list
                        let mut ents = Vec::new();
                        if let Some(arr) = json.get("entities").and_then(|v| v.as_array()) {
                            for e in arr {
                                let label = e.get("label").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let etype = e.get("entity_type").and_then(|v| v.as_str()).unwrap_or("entity").to_string();
                                let conf = e.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                                if !label.is_empty() {
                                    ents.push((label, etype, conf, false));
                                }
                            }
                        }
                        set_seed_entities.set(ents);
                        set_seed_session_id.set(session_id);
                        set_seed_aoi.set(aoi.clone());
                        set_seed_phase.set(1);

                        // Auto-trigger enrichment (confirm AoI and start entity linking)
                        let confirm_body = serde_json::json!({
                            "session_id": json.get("session_id").and_then(|v| v.as_str()).unwrap_or(""),
                            "area_of_interest": aoi
                        });
                        let _ = api.post_text("/ingest/seed/confirm-aoi", &confirm_body).await;
                    }
                }
                Err(_) => {
                    // Fallback to old analyze endpoint
                    let body = AnalyzeRequest { text };
                    match api.post::<_, AnalyzeResponse>("/ingest/analyze", &body).await {
                        Ok(resp) => {
                            let ents: Vec<(String, String, f32, bool)> = resp.entities.iter()
                                .map(|e| (e.text.clone(), e.entity_type.clone(), e.confidence, false))
                                .collect();
                            set_seed_entities.set(ents);
                            set_seed_phase.set(1);
                        }
                        Err(e) => {
                            set_seed_result.set(Some(format!("Analysis failed: {e}")));
                        }
                    }
                }
            }
            set_analyzing.set(false);
        }
    });

    // Ingest seed text — commit via seed session or fall back to regular ingest
    let api_ingest = api.clone();
    let do_ingest = Action::new_local(move |_: &()| {
        let api = api_ingest.clone();
        let text = seed_text.get_untracked();
        let session = seed_session_id.get_untracked();
        async move {
            if text.trim().is_empty() { return; }
            set_analyzing.set(true);

            // Use regular ingest — the confirm-aoi already triggered enrichment
            // which writes directly to the graph. We ingest the seed text to ensure
            // all NER entities are stored with proper provenance.
            let body = IngestRequest {
                items: vec![IngestItem { content: text, source_url: None }],
                source: Some("seed-enrichment".into()),
                skip: None,
            };
            match api.post::<_, IngestResponse>("/ingest", &body).await {
                Ok(resp) => {
                    set_seed_result.set(Some(format!(
                        "Seeded! {} facts, {} relations ({}ms)",
                        resp.facts_stored, resp.relations_created, resp.duration_ms,
                    )));
                    // Clean up seed session if we had one
                    if !session.is_empty() {
                        let _ = api.post_text("/ingest/seed/commit", &serde_json::json!({ "session_id": session })).await;
                    }
                    set_step.set(TOTAL_STEPS);
                }
                Err(e) => {
                    set_seed_result.set(Some(format!("Ingest failed: {e}")));
                }
            }
            set_analyzing.set(false);
        }
    });

    // Complete wizard
    let api_complete = api.clone();
    let do_complete = Action::new_local(move |_: &()| {
        let api = api_complete.clone();
        async move {
            let _ = api.post_text("/config/wizard-complete", &serde_json::json!({})).await;
            on_complete.run(());
            // Force page reload so dashboard stats refresh
            if let Some(window) = web_sys::window() {
                let _ = window.location().reload();
            }
        }
    });

    // Next/back navigation
    let go_next = move |_| {
        let current = step.get_untracked();
        // For steps that need saving, dispatch save and advance on success
        if matches!(current, STEP_EMBEDDER | STEP_NER | STEP_REL | STEP_LLM | STEP_QUANTIZATION | STEP_KB_SOURCES | STEP_WEB_SEARCH) {
            save_step_config.dispatch(current);
        } else {
            set_step.set((current + 1).min(TOTAL_STEPS));
        }
    };

    // Watch save results to advance step
    Effect::new(move || {
        if let Some(success) = save_step_config.value().get() {
            if success {
                let current = step.get_untracked();
                set_step.set((current + 1).min(TOTAL_STEPS));
            }
        }
    });

    let go_back = move |_| {
        let current = step.get_untracked();
        if current > 1 {
            set_step.set(current - 1);
            set_save_error.set(None);
        }
    };

    view! {
        <div class=overlay_class>
            <div class="wizard-modal">
                // Progress bar
                <div class="wizard-progress">
                    <div class="wizard-progress-bar" style=move || format!("width: {}%", (step.get() as f32 / TOTAL_STEPS as f32 * 100.0) as u32)></div>
                    <span class="wizard-step-label">{move || format!("Step {} of {}", step.get(), TOTAL_STEPS)}</span>
                </div>

                // Quality meter
                <div class="wizard-quality">
                    <div class="wizard-quality-label">
                        <span>"Knowledge Quality"</span>
                        <span>{move || format!("{}%", quality.get())}</span>
                    </div>
                    <div class="wizard-quality-bar">
                        <div class="wizard-quality-fill" style=move || {
                            let q = quality.get();
                            let color = if q >= 80 { "#66bb6a" } else if q >= 50 { "#4fc3f7" } else if q >= 30 { "#ffa726" } else { "#78909c" };
                            format!("width: {}%; background: {}", q, color)
                        }></div>
                    </div>
                    <div class="wizard-quality-items">
                        <span class=move || if !embed_choice.get().is_empty() { "wq-item wq-on" } else { "wq-item" }>
                            <i class=move || if !embed_choice.get().is_empty() { "fa-solid fa-check" } else { "fa-regular fa-circle" }></i>
                            " Embeddings"
                        </span>
                        <span class=move || if !ner_choice.get().is_empty() { "wq-item wq-on" } else { "wq-item" }>
                            <i class=move || if !ner_choice.get().is_empty() { "fa-solid fa-check" } else { "fa-regular fa-circle" }></i>
                            " NER"
                        </span>
                        <span class=move || if kb_wikidata.get() || kb_dbpedia.get() { "wq-item wq-on" } else { "wq-item" }>
                            <i class=move || if kb_wikidata.get() || kb_dbpedia.get() { "fa-solid fa-check" } else { "fa-regular fa-circle" }></i>
                            " Knowledge Sources"
                        </span>
                        <span class=move || if !llm_choice.get().is_empty() { "wq-item wq-on" } else { "wq-item" }>
                            <i class=move || if !llm_choice.get().is_empty() { "fa-solid fa-check" } else { "fa-regular fa-circle" }></i>
                            " LLM"
                        </span>
                        <span class=move || if !quant_choice.get().is_empty() { "wq-item wq-on" } else { "wq-item" }>
                            <i class=move || if !quant_choice.get().is_empty() { "fa-solid fa-check" } else { "fa-regular fa-circle" }></i>
                            " Quantization"
                        </span>
                        <span class=move || if !seed_text.get().trim().is_empty() { "wq-item wq-on" } else { "wq-item" }>
                            <i class=move || if !seed_text.get().trim().is_empty() { "fa-solid fa-check" } else { "fa-regular fa-circle" }></i>
                            " Seeded"
                        </span>
                    </div>
                </div>

                // Step content
                <div class="wizard-content">
                    {move || match step.get() {
                        STEP_WELCOME => steps_pipeline::render_step_welcome(),
                        STEP_EMBEDDER => steps_pipeline::render_step_embedder(
                            embed_choice, set_embed_choice,
                            embed_key, set_embed_key,
                            embed_model, set_embed_model,
                            embed_endpoint, set_embed_endpoint,
                            ollama_embed_models, ollama_fetching,
                        ),
                        STEP_NER => steps_pipeline::render_step_ner(
                            ner_choice, set_ner_choice,
                            ner_model, set_ner_model,
                        ),
                        STEP_REL => steps_pipeline::render_step_rel(
                            rel_threshold, set_rel_threshold,
                            rel_templates_mode, set_rel_templates_mode,
                            rel_custom_templates_json, set_rel_custom_templates_json,
                        ),
                        STEP_LLM => steps_pipeline::render_step_llm(
                            llm_choice, set_llm_choice,
                            llm_key, set_llm_key,
                            llm_model, set_llm_model,
                            ollama_llm_models, ollama_fetching,
                        ),
                        STEP_QUANTIZATION => steps_config::render_step_quantization(
                            quant_choice, set_quant_choice,
                        ),
                        STEP_KB_SOURCES => steps_config::render_step_kb_sources(
                            kb_wikidata, set_kb_wikidata,
                            kb_dbpedia, set_kb_dbpedia,
                            source_trust_values,
                        ),
                        STEP_WEB_SEARCH => steps_config::render_step_web_search(
                            web_search_provider, set_web_search_provider,
                            web_search_api_key, set_web_search_api_key,
                            web_search_url, set_web_search_url,
                            web_search_test_result, web_search_testing,
                            do_web_search_test,
                            source_trust_values,
                        ),
                        STEP_SEED => steps_config::render_step_seed(
                            seed_text, set_seed_text,
                            seed_phase, set_seed_phase,
                            seed_aoi, set_seed_aoi,
                            seed_session_id, set_seed_session_id,
                            seed_entities, set_seed_entities,
                            new_entity_label, set_new_entity_label,
                            new_entity_type, set_new_entity_type,
                            analyzing, seed_result, set_seed_result,
                            do_analyze, do_ingest,
                            set_step,
                            seed_review_connections, set_seed_review_connections,
                            seed_known_rel_types, set_seed_known_rel_types,
                            seed_review_submitting, set_seed_review_submitting,
                        ),
                        STEP_READY => steps_config::render_step_ready(
                            embed_choice, ner_choice, llm_choice, quant_choice,
                            kb_wikidata, kb_dbpedia,
                            do_complete,
                        ),
                        _ => view! { <div></div> }.into_any(),
                    }}
                </div>

                // Error display
                {move || save_error.get().map(|e| view! {
                    <div class="wizard-error">
                        <i class="fa-solid fa-triangle-exclamation"></i>" "{e}
                    </div>
                })}

                // Navigation buttons
                <div class="wizard-nav">
                    <button class="btn btn-secondary" on:click=go_back
                        disabled=move || step.get() <= STEP_WELCOME>
                        <i class="fa-solid fa-arrow-left"></i>" Back"
                    </button>
                    {move || if step.get() == STEP_READY {
                        view! { <span></span> }.into_any()
                    } else if step.get() == STEP_SEED {
                        // Seed step has its own navigation
                        view! { <span></span> }.into_any()
                    } else {
                        view! {
                            <button class="btn btn-primary" on:click=go_next
                                disabled=saving>
                                {move || if saving.get() {
                                    let msg = match step.get() {
                                        STEP_EMBEDDER => {
                                            let e = embed_choice.get();
                                            if e == "onnx" { " Downloading ONNX model... this may take a minute" }
                                            else if e == "ollama" { " Pulling model from Ollama..." }
                                            else { " Saving configuration..." }
                                        }
                                        STEP_NER => {
                                            let n = ner_choice.get();
                                            if n == "gliner" { " Downloading GLiNER model..." }
                                            else { " Saving configuration..." }
                                        }
                                        STEP_REL => " Saving relation config...",
                                        STEP_LLM => {
                                            let l = llm_choice.get();
                                            if l == "ollama" { " Pulling model from Ollama..." }
                                            else { " Saving configuration..." }
                                        }
                                        _ => " Saving configuration...",
                                    };
                                    view! { <span class="spinner"></span>{msg} }.into_any()
                                } else {
                                    view! { <>"Next "<i class="fa-solid fa-arrow-right"></i></> }.into_any()
                                }}
                            </button>
                        }.into_any()
                    }}
                </div>
            </div>
        </div>
    }
}
