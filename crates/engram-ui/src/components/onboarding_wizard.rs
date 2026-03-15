use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{AnalyzeRequest, AnalyzeResponse, IngestRequest, IngestItem, IngestResponse};

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

// ── Provider presets ──

struct EmbedPreset {
    id: &'static str,
    name: &'static str,
    endpoint: &'static str,
    needs_key: bool,
    quality: &'static str,
    performance: &'static str,
    privacy: &'static str,
    cost: &'static str,
    models: &'static [(&'static str, &'static str, &'static str)],  // (model_name, description, lang_badge)
    default_model: &'static str,
}

const EMBED_PRESETS: &[EmbedPreset] = &[
    EmbedPreset {
        id: "onnx", name: "ONNX (Local)", endpoint: "onnx://local",
        needs_key: false,
        quality: "Good (384D all-MiniLM)", performance: "Fast, uses your GPU/CPU",
        privacy: "Everything stays local", cost: "Free, ~50MB download",
        models: &[
            ("all-MiniLM-L6-v2", "384D, 90MB, best balance", "EN"),
            ("multilingual-e5-small", "384D, 120MB, strong multilingual", "100+ langs"),
            ("bge-small-en-v1.5", "384D, 130MB, high quality", "EN"),
        ],
        default_model: "all-MiniLM-L6-v2",
    },
    EmbedPreset {
        id: "ollama", name: "Ollama", endpoint: "http://localhost:11434/api/embed",
        needs_key: false,
        quality: "Good-Excellent (model dependent)", performance: "Fast if local GPU",
        privacy: "Local", cost: "Free",
        models: &[
            ("nomic-embed-text", "768D, 274MB, strong all-rounder", "EN"),
            ("mxbai-embed-large", "1024D, 670MB, highest quality", "EN"),
            ("all-minilm", "384D, 23MB, fastest", "EN"),
            ("snowflake-arctic-embed", "1024D, 335MB, top benchmark", "Multilingual"),
        ],
        default_model: "nomic-embed-text",
    },
    EmbedPreset {
        id: "openai", name: "OpenAI", endpoint: "https://api.openai.com/v1/embeddings",
        needs_key: true,
        quality: "Excellent (text-embedding-3)", performance: "Network latency per op",
        privacy: "Data sent to OpenAI", cost: "~$0.02/1M tokens",
        models: &[
            ("text-embedding-3-small", "1536D, cheapest, good quality", "Multilingual"),
            ("text-embedding-3-large", "3072D, best quality, 6x cost", "Multilingual"),
        ],
        default_model: "text-embedding-3-small",
    },
    EmbedPreset {
        id: "vllm", name: "vLLM", endpoint: "http://localhost:8000/v1/embeddings",
        needs_key: false,
        quality: "Model dependent", performance: "Self-hosted, you control",
        privacy: "Local", cost: "Free",
        models: &[],
        default_model: "",
    },
    EmbedPreset {
        id: "lmstudio", name: "LM Studio", endpoint: "http://localhost:1234/v1/embeddings",
        needs_key: false,
        quality: "Model dependent", performance: "Fast with GPU",
        privacy: "Local", cost: "Free",
        models: &[],
        default_model: "",
    },
    EmbedPreset {
        id: "custom", name: "Custom Provider", endpoint: "",
        needs_key: true,
        quality: "Provider dependent", performance: "Network latency",
        privacy: "Data sent to provider", cost: "Provider dependent",
        models: &[],
        default_model: "",
    },
];

struct NerPreset {
    id: &'static str,
    name: &'static str,
    quality: &'static str,
    speed: &'static str,
    download: &'static str,
    license: &'static str,
    learning: &'static str,
    models: &'static [(&'static str, &'static str, &'static str, &'static str, &'static str)], // (id, name, desc, hf_repo, lang)
}

const NER_PRESETS: &[NerPreset] = &[
    NerPreset {
        id: "builtin", name: "Builtin Rules",
        quality: "Basic \u{2014} patterns only", speed: "Instant",
        download: "None", license: "N/A",
        learning: "Entity gazetteer grows from graph",
        models: &[],
    },
    NerPreset {
        id: "gliner2", name: "GLiNER2 (Recommended)",
        quality: "High \u{2014} NER + Relation Extraction, zero-shot, multilingual", speed: "~125ms/sentence",
        download: "~530MB\u{2013}1.1GB ONNX model (in-process, no sidecar)", license: "Apache-2.0",
        learning: "Discovers entities + relations in one pass. Feeds gazetteer for instant future recognition.",
        models: &[
            ("gliner2-fp16", "GLiNER2 Multi v1 FP16", "530MB FP16 hybrid, 100+ languages (recommended)", "dx111ge/gliner2-multi-v1-onnx", "Multilingual"),
            ("gliner2-fp32", "GLiNER2 Multi v1 FP32", "1.1GB FP32, 100+ languages (maximum precision)", "dx111ge/gliner2-multi-v1-onnx", "Multilingual"),
        ],
    },
    NerPreset {
        id: "llm", name: "LLM Fallback",
        quality: "Highest for unusual entities", speed: "Slow (~500ms+)",
        download: "None (uses your LLM)", license: "Depends on LLM",
        learning: "Same learning loop as GLiNER option",
        models: &[],
    },
];

struct LlmPreset {
    id: &'static str,
    name: &'static str,
    endpoint: &'static str,
    needs_key: bool,
    quality: &'static str,
    privacy: &'static str,
    cost: &'static str,
    models: &'static [(&'static str, &'static str)],
    default_model: &'static str,
}

const LLM_PRESETS: &[LlmPreset] = &[
    LlmPreset {
        id: "ollama", name: "Ollama (Recommended)", endpoint: "http://localhost:11434/v1/chat/completions",
        needs_key: false, quality: "Good (local models)", privacy: "Local", cost: "Free",
        models: &[
            ("llama3.2", "3B, fast, good quality"),
            ("phi4", "14B, excellent reasoning"),
            ("mistral", "7B, balanced"),
            ("gemma3", "4B, efficient"),
            ("qwen3", "8B, strong multilingual"),
        ],
        default_model: "llama3.2",
    },
    LlmPreset {
        id: "lmstudio", name: "LM Studio", endpoint: "http://localhost:1234/v1/chat/completions",
        needs_key: false, quality: "Good", privacy: "Local", cost: "Free",
        models: &[],
        default_model: "",
    },
    LlmPreset {
        id: "openai", name: "OpenAI", endpoint: "https://api.openai.com/v1/chat/completions",
        needs_key: true, quality: "Excellent", privacy: "Cloud", cost: "Per-token",
        models: &[
            ("gpt-4o-mini", "fast, cheap, good quality"),
            ("gpt-4o", "best quality, higher cost"),
            ("gpt-4.1-mini", "latest mini, improved"),
        ],
        default_model: "gpt-4o-mini",
    },
    LlmPreset {
        id: "vllm", name: "vLLM", endpoint: "http://localhost:8000/v1/chat/completions",
        needs_key: false, quality: "Model dependent", privacy: "Local", cost: "Free",
        models: &[],
        default_model: "",
    },
];

const SEED_EXAMPLES: &[(&str, &str)] = &[
    ("Geopolitics & Security",
     "I'm a security analyst focused on European cybersecurity and critical infrastructure protection. Key areas include Germany's BSI, France's ANSSI, and NATO's CCDCOE in Tallinn. I track state-sponsored threat actors like APT28 and APT29, and monitor EU regulations including NIS2 and the Cyber Resilience Act. The energy sector, particularly Nord Stream infrastructure and European power grids, is a priority."),
    ("Technology & AI",
     "I'm researching artificial intelligence companies and their key products. Major players include OpenAI with GPT-4, Google DeepMind with Gemini and AlphaFold, Anthropic with Claude, and Meta AI with LLaMA. I'm interested in the researchers behind these systems, their academic backgrounds at Stanford, MIT, and Oxford, and the venture capital firms like Sequoia and Andreessen Horowitz funding this space."),
    ("History & Geography",
     "I study the history and geography of Central Europe, particularly the Holy Roman Empire and its successor states. Key figures include Charlemagne, Frederick the Great, Maria Theresa, and Otto von Bismarck. Important cities include Vienna, Prague, Berlin, and Munich. I'm interested in the rivers Danube, Rhine, and Elbe, and how they shaped trade routes and political boundaries."),
];

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
                        STEP_WELCOME => view! {
                            <div class="wizard-step">
                                <h2><i class="fa-solid fa-brain"></i>" Welcome to engram"</h2>
                                <p class="wizard-desc">"engram is your personal knowledge engine. It stores facts, discovers relationships, and learns from every interaction."</p>
                                <p class="wizard-desc">"This wizard will set up your engine for maximum knowledge quality. Each step configures a layer of engram\u{2019}s intelligence pipeline."</p>
                                <div class="wizard-info-box">
                                    <h4><i class="fa-solid fa-layer-group"></i>" How engram learns"</h4>
                                    <p>"engram uses three layers of relation extraction, each feeding the next:"</p>
                                    <ol>
                                        <li><strong>"Knowledge Base"</strong>" \u{2014} hard facts from the semantic web (Wikidata/DBpedia). Bootstraps your graph."</li>
                                        <li><strong>"Relation Gazetteer"</strong>" \u{2014} remembers every relationship stored. Instant recall, grows automatically."</li>
                                        <li><strong>"KGE / RotatE"</strong>" \u{2014} trains on your graph structure to predict new relationships from patterns."</li>
                                    </ol>
                                    <p><em>"The first facts you add create a snowball effect. Each new fact makes the system smarter."</em></p>
                                </div>
                            </div>
                        }.into_any(),

                        STEP_EMBEDDER => view! {
                            <div class="wizard-step">
                                <h2><i class="fa-solid fa-vector-square"></i>" Embedding Model"</h2>
                                <p class="wizard-desc">"Embeddings convert text into numbers that capture meaning. This is how engram understands similarity between concepts. "</p>
                                <p class="wizard-required"><i class="fa-solid fa-asterisk"></i>" Required \u{2014} nothing works without embeddings."</p>
                                <div class="wizard-cards">
                                    {EMBED_PRESETS.iter().map(|p| {
                                        let id = p.id.to_string();
                                        let id2 = id.clone();
                                        view! {
                                            <div
                                                class=move || if embed_choice.get() == id { "wizard-card wizard-card-selected" } else { "wizard-card" }
                                                on:click=move |_| set_embed_choice.set(id2.clone())
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
                                {move || {
                                    let choice = embed_choice.get();
                                    let preset = EMBED_PRESETS.iter().find(|p| p.id == choice.as_str());
                                    preset.map(|p| {
                                        // Set default model for this provider (or clear if none)
                                        set_embed_model.set(p.default_model.to_string());
                                        let show_key = p.needs_key;
                                        let models: Vec<(&str, &str, &str)> = p.models.to_vec();
                                        let is_custom_provider = choice == "custom";
                                        let show_custom = p.models.is_empty() && !is_custom_provider;
                                        view! {
                                            // Custom provider: endpoint URL input + provider links
                                            {is_custom_provider.then(|| view! {
                                                <div class="form-group mt-1">
                                                    <label><i class="fa-solid fa-link"></i>" Endpoint URL"</label>
                                                    <input type="text" class="form-control" placeholder="https://api.example.com/v1/embeddings"
                                                        prop:value=embed_endpoint
                                                        on:input=move |ev| set_embed_endpoint.set(event_target_value(&ev))
                                                    />
                                                    <small class="text-secondary">"OpenAI-compatible /v1/embeddings endpoint"</small>
                                                </div>
                                                <div class="wizard-info-box" style="margin-top: 0.75rem;">
                                                    <h4><i class="fa-solid fa-cloud"></i>" Popular embedding providers"</h4>
                                                    <div class="wizard-provider-links">
                                                        <a href="https://cohere.com/embed" target="_blank" rel="noopener"><i class="fa-solid fa-arrow-up-right-from-square"></i>" Cohere Embed v3"<small>" \u{2014} multilingual, 1024D"</small></a>
                                                        <a href="https://jina.ai/embeddings/" target="_blank" rel="noopener"><i class="fa-solid fa-arrow-up-right-from-square"></i>" Jina Embeddings"<small>" \u{2014} multilingual, 8K context"</small></a>
                                                        <a href="https://docs.voyageai.com/docs/embeddings" target="_blank" rel="noopener"><i class="fa-solid fa-arrow-up-right-from-square"></i>" Voyage AI"<small>" \u{2014} code + text, domain-tuned"</small></a>
                                                        <a href="https://cloud.google.com/vertex-ai/docs/generative-ai/embeddings/get-text-embeddings" target="_blank" rel="noopener"><i class="fa-solid fa-arrow-up-right-from-square"></i>" Google Vertex AI"<small>" \u{2014} text-embedding-005, multimodal"</small></a>
                                                        <a href="https://docs.mistral.ai/capabilities/embeddings/" target="_blank" rel="noopener"><i class="fa-solid fa-arrow-up-right-from-square"></i>" Mistral Embed"<small>" \u{2014} 1024D, multilingual"</small></a>
                                                        <a href="https://docs.aws.amazon.com/bedrock/latest/userguide/titan-embedding-models.html" target="_blank" rel="noopener"><i class="fa-solid fa-arrow-up-right-from-square"></i>" AWS Titan"<small>" \u{2014} via Bedrock, multimodal"</small></a>
                                                    </div>
                                                    <small class="text-secondary">"Any provider with an OpenAI-compatible embeddings API will work."</small>
                                                </div>
                                            })}
                                            {show_key.then(|| view! {
                                                <div class="form-group mt-1">
                                                    <label><i class="fa-solid fa-key"></i>" API Key"</label>
                                                    <input type="password" class="form-control" placeholder="sk-..."
                                                        prop:value=embed_key
                                                        on:input=move |ev| set_embed_key.set(event_target_value(&ev))
                                                    />
                                                </div>
                                            })}
                                            <div class="form-group mt-1">
                                                <label><i class="fa-solid fa-cube"></i>" Model"</label>
                                                {(!models.is_empty()).then(|| {
                                                    let models2 = models.clone();
                                                    let is_onnx = choice == "onnx";
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
                                                // Fetched Ollama models (shown when Ollama selected)
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
                                                {show_custom.then(|| view! {
                                                    <input type="text" class="form-control" placeholder="Enter model name..."
                                                        prop:value=embed_model
                                                        on:input=move |ev| set_embed_model.set(event_target_value(&ev))
                                                    />
                                                })}
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
                            </div>
                        }.into_any(),

                        STEP_NER => view! {
                            <div class="wizard-step">
                                <h2><i class="fa-solid fa-tags"></i>" Named Entity Recognition"</h2>
                                <p class="wizard-desc">"NER finds people, places, organizations, and concepts in your text. This is how engram knows what you\u{2019}re talking about."</p>
                                <div class="wizard-info-box">
                                    <h4><i class="fa-solid fa-graduation-cap"></i>" Self-improving pipeline"</h4>
                                    <p>"engram learns from every entity found:"</p>
                                    <ul>
                                        <li>"NER discovers new entities \u{2192} stored in graph \u{2192} gazetteer indexes them for instant future recognition"</li>
                                        <li>"Coreference resolution: pronouns like \u{201c}he\u{201d}/\u{201c}she\u{201d} resolve to actual entity names"</li>
                                        <li>"GLiNER2 relation extraction: zero-shot, multilingual, in single model pass \u{2014} configurable relation types"</li>
                                        <li>"Relation gazetteer learns every edge you store \u{2192} instant recall next time"</li>
                                        <li>"KGE trains on your graph structure \u{2192} predicts new relationships from patterns"</li>
                                    </ul>
                                    <p><em>"The more you use engram, the faster and more accurate it becomes."</em></p>
                                </div>
                                <div class="wizard-cards">
                                    {NER_PRESETS.iter().map(|p| {
                                        let id = p.id.to_string();
                                        let id2 = id.clone();
                                        view! {
                                            <div
                                                class=move || if ner_choice.get() == id { "wizard-card wizard-card-selected" } else { "wizard-card" }
                                                on:click=move |_| set_ner_choice.set(id2.clone())
                                            >
                                                <h4>{p.name}</h4>
                                                <div class="wizard-card-grid">
                                                    <span class="wc-label">"Quality"</span><span>{p.quality}</span>
                                                    <span class="wc-label">"Speed"</span><span>{p.speed}</span>
                                                    <span class="wc-label">"Download"</span><span>{p.download}</span>
                                                    <span class="wc-label">"License"</span><span>{p.license}</span>
                                                </div>
                                                <p class="wizard-card-note"><i class="fa-solid fa-rotate"></i>" "{p.learning}</p>
                                            </div>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                                // Show model selection for GLiNER
                                {move || {
                                    let choice = ner_choice.get();
                                    let preset = NER_PRESETS.iter().find(|p| p.id == choice.as_str());
                                    preset.and_then(|p| {
                                        if p.models.is_empty() { return None; }
                                        let models: Vec<(&str, &str, &str, &str, &str)> = p.models.to_vec();
                                        Some(view! {
                                            <div class="form-group mt-1">
                                                <label><i class="fa-solid fa-cube"></i>" NER Model"</label>
                                                <p class="wizard-hint">"Select a recommended model or enter any HuggingFace model ID below."</p>
                                                <div class="wizard-model-chips">
                                                    {models.into_iter().map(|(id, name, desc, _repo, lang)| {
                                                        let mid = id.to_string();
                                                        let mid2 = mid.clone();
                                                        let badge_class = if lang.contains("100+") || lang.contains("ulti") {
                                                            "wizard-lang-badge wizard-lang-multi"
                                                        } else {
                                                            "wizard-lang-badge wizard-lang-en"
                                                        };
                                                        view! {
                                                            <button
                                                                class=move || if ner_model.get() == mid { "wizard-model-chip active" } else { "wizard-model-chip" }
                                                                on:click=move |_| set_ner_model.set(mid2.clone())
                                                            >
                                                                <strong>{name}</strong>
                                                                <span class=badge_class><i class="fa-solid fa-language"></i>" "{lang}</span>
                                                                <small>{desc}</small>
                                                            </button>
                                                        }
                                                    }).collect::<Vec<_>>()}
                                                </div>
                                                <div class="wizard-custom-model mt-1">
                                                    <label><i class="fa-brands fa-github"></i>" Custom HuggingFace Model"</label>
                                                    <div class="wizard-custom-input-row">
                                                        <input type="text" class="form-control"
                                                            placeholder="e.g. onnx-community/gliner_multi_pii-v1"
                                                            prop:value=move || {
                                                                let m = ner_model.get();
                                                                // Only show in text field if it's a custom (non-preset) value
                                                                let is_preset = NER_PRESETS.iter()
                                                                    .flat_map(|p| p.models.iter())
                                                                    .any(|(id, _, _, _, _)| *id == m.as_str());
                                                                if is_preset { String::new() } else { m }
                                                            }
                                                            on:input=move |ev| {
                                                                let v = event_target_value(&ev);
                                                                if !v.trim().is_empty() {
                                                                    set_ner_model.set(v);
                                                                }
                                                            }
                                                        />
                                                    </div>
                                                    <small class="text-secondary">"Enter any GLiNER-compatible ONNX model from huggingface.co. Must contain onnx/model.onnx and tokenizer.json."</small>
                                                </div>
                                            </div>
                                        })
                                    })
                                }}
                            </div>
                        }.into_any(),

                        STEP_REL => view! {
                            <div class="wizard-step">
                                <h2><i class="fa-solid fa-link"></i>" Relation Extraction"</h2>
                                <p class="wizard-desc">"GLiNER2 extracts both entities and relations in a single model pass. Configure which relation types to detect and the confidence threshold."</p>

                                // Confidence Threshold
                                <div class="form-group" style="margin-top: 1rem;">
                                    <label><i class="fa-solid fa-sliders"></i>" Confidence Threshold: "
                                        <strong>{move || format!("{:.2}", rel_threshold.get())}</strong>
                                    </label>
                                    <input type="range"
                                        min="0.50" max="0.95" step="0.05"
                                        style="width: 100%; margin-top: 0.25rem;"
                                        prop:value=move || format!("{:.2}", rel_threshold.get())
                                        on:input=move |ev| {
                                            if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                                set_rel_threshold.set(v);
                                            }
                                        }
                                    />
                                    <div style="display: flex; justify-content: space-between; font-size: 0.7rem; color: rgba(255,255,255,0.4);">
                                        <span>"0.50 (more relations)"</span>
                                        <span>"0.85 (recommended)"</span>
                                        <span>"0.95 (facts only)"</span>
                                    </div>
                                </div>

                                // Relation Types
                                <div class="form-group" style="margin-top: 1rem;">
                                    <label><i class="fa-solid fa-list-check"></i>" Relation Types"</label>
                                    <p class="wizard-hint">"GLiNER2 uses zero-shot relation labels. Select a preset or define custom relation types for your domain."</p>
                                    <div class="wizard-cards" style="margin-top: 0.5rem;">
                                        <div
                                            class=move || if rel_templates_mode.get() == "general" { "wizard-card wizard-card-selected" } else { "wizard-card" }
                                            on:click=move |_| set_rel_templates_mode.set("general".into())
                                            style="min-width: 200px;"
                                        >
                                            <h4>"General (6 types)"</h4>
                                            <p style="font-size: 0.8rem;">"works_at, headquartered_in, located_in, founded, leads, supports. Covers common entity relationships."</p>
                                            <p style="font-size: 0.75rem; color: rgba(255,255,255,0.5);"><i class="fa-solid fa-wifi-slash" style="margin-right: 0.25rem;"></i>"Works offline / air-gapped"</p>
                                        </div>
                                        <div
                                            class=move || if rel_templates_mode.get() == "custom" { "wizard-card wizard-card-selected" } else { "wizard-card" }
                                            on:click=move |_| set_rel_templates_mode.set("custom".into())
                                            style="min-width: 200px;"
                                        >
                                            <h4>"Custom Relations"</h4>
                                            <p style="font-size: 0.8rem;">"Define domain-specific relation types (e.g. treats, manufactures, regulates). Just name them \u{2014} GLiNER2 extracts zero-shot."</p>
                                            <p style="font-size: 0.75rem; color: rgba(255,255,255,0.5);"><i class="fa-solid fa-file-import" style="margin-right: 0.25rem;"></i>"Air-gapped import supported"</p>
                                        </div>
                                    </div>
                                </div>

                                // Custom relation types (shown when "custom" selected)
                                {move || {
                                    (rel_templates_mode.get() == "custom").then(|| view! {
                                        <div class="form-group" style="margin-top: 0.75rem;">
                                            <label>"Paste relation types JSON or configure in System settings after setup"</label>
                                            <textarea
                                                class="form-control"
                                                style="width: 100%; min-height: 120px; font-family: monospace; font-size: 0.8rem; background: rgba(0,0,0,0.2); color: inherit; border: 1px solid rgba(255,255,255,0.1);"
                                                prop:value=rel_custom_templates_json
                                                on:input=move |ev| {
                                                    set_rel_custom_templates_json.set(event_target_value(&ev));
                                                }
                                                placeholder=r#"{"treats": "{head} treats {tail}", "manufactures": "{head} manufactures {tail}", "regulates": "{head} regulates {tail}"}"#
                                            ></textarea>
                                            <div class="wizard-info-box" style="margin-top: 0.5rem; font-size: 0.8rem;">
                                                <i class="fa-solid fa-circle-info" style="margin-right: 0.25rem;"></i>
                                                " Format: {\"relation_type\": \"description\"}. GLiNER2 uses the relation name as a zero-shot label. Custom types are merged with defaults."
                                            </div>
                                        </div>
                                    })
                                }}
                            </div>
                        }.into_any(),

                        STEP_LLM => view! {
                            <div class="wizard-step">
                                <h2><i class="fa-solid fa-comments"></i>" Language Model"</h2>
                                <p class="wizard-desc">"A language model is required for area-of-interest detection, entity disambiguation, and intelligent seed enrichment."</p>
                                <p class="wizard-required"><i class="fa-solid fa-asterisk"></i>" Required \u{2014} LLM powers the seed enrichment pipeline."</p>
                                <div class="wizard-cards">
                                    {LLM_PRESETS.iter().map(|p| {
                                        let id = p.id.to_string();
                                        let id2 = id.clone();
                                        view! {
                                            <div
                                                class=move || if llm_choice.get() == id { "wizard-card wizard-card-selected" } else { "wizard-card" }
                                                on:click=move |_| set_llm_choice.set(id2.clone())
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
                                {move || {
                                    let choice = llm_choice.get();
                                    let preset = LLM_PRESETS.iter().find(|p| p.id == choice.as_str());
                                    preset.map(|p| {
                                        if llm_model.get_untracked().is_empty() && !p.default_model.is_empty() {
                                            set_llm_model.set(p.default_model.to_string());
                                        }
                                        let show_key = p.needs_key;
                                        let models: Vec<(&str, &str)> = p.models.to_vec();
                                        let show_custom = p.models.is_empty();
                                        view! {
                                            {show_key.then(|| view! {
                                                <div class="form-group mt-1">
                                                    <label><i class="fa-solid fa-key"></i>" API Key"</label>
                                                    <input type="password" class="form-control" placeholder="sk-..."
                                                        prop:value=llm_key
                                                        on:input=move |ev| set_llm_key.set(event_target_value(&ev))
                                                    />
                                                </div>
                                            })}
                                            <div class="form-group mt-1">
                                                <label><i class="fa-solid fa-cube"></i>" Model"</label>
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
                                                // Fetched Ollama LLM models
                                                {(choice == "ollama").then(|| view! {
                                                    {move || {
                                                        let fetched = ollama_llm_models.get();
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
                                                                                    class=move || if llm_model.get() == n { "wizard-model-chip active" } else { "wizard-model-chip" }
                                                                                    on:click=move |_| set_llm_model.set(n2.clone())
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
                                // LLM is mandatory — no skip button
                            </div>
                        }.into_any(),

                        STEP_QUANTIZATION => view! {
                            <div class="wizard-step">
                                <h2><i class="fa-solid fa-compress"></i>" Vector Quantization"</h2>
                                <p class="wizard-desc">"Quantization compresses vector embeddings to use less memory. int8 is the recommended default \u{2014} 4x memory savings with virtually no quality loss."</p>
                                <div class="wizard-cards">
                                    <div
                                        class=move || if quant_choice.get() == "off" { "wizard-card wizard-card-selected" } else { "wizard-card" }
                                        on:click=move |_| set_quant_choice.set("off".into())
                                    >
                                        <h4>"Off"</h4>
                                        <div class="wizard-card-grid">
                                            <span class="wc-label">"Memory"</span><span>"Full precision"</span>
                                            <span class="wc-label">"Quality"</span><span>"100% (baseline)"</span>
                                            <span class="wc-label">"Best for"</span><span>"Small graphs (<10K nodes)"</span>
                                        </div>
                                    </div>
                                    <div
                                        class=move || if quant_choice.get() == "int8" { "wizard-card wizard-card-selected" } else { "wizard-card" }
                                        on:click=move |_| set_quant_choice.set("int8".into())
                                    >
                                        <h4>"int8 (Recommended)"</h4>
                                        <div class="wizard-card-grid">
                                            <span class="wc-label">"Memory"</span><span>"4x reduction"</span>
                                            <span class="wc-label">"Quality"</span><span>"~99% (<1% loss)"</span>
                                            <span class="wc-label">"Best for"</span><span>"Most users"</span>
                                        </div>
                                    </div>
                                    <div
                                        class=move || if quant_choice.get() == "int4" { "wizard-card wizard-card-selected" } else { "wizard-card" }
                                        on:click=move |_| set_quant_choice.set("int4".into())
                                    >
                                        <h4>"int4"</h4>
                                        <div class="wizard-card-grid">
                                            <span class="wc-label">"Memory"</span><span>"8x reduction"</span>
                                            <span class="wc-label">"Quality"</span><span>"~97% (~3% loss)"</span>
                                            <span class="wc-label">"Best for"</span><span>"Very large graphs (100K+)"</span>
                                        </div>
                                    </div>
                                </div>
                            </div>
                        }.into_any(),

                        STEP_KB_SOURCES => view! {
                            <div class="wizard-step">
                                <h2><i class="fa-solid fa-database"></i>" Knowledge Sources"</h2>
                                <p class="wizard-desc">"Knowledge sources are external databases that engram consults to verify and enrich what you tell it."</p>
                                <div class="wizard-info-box">
                                    <p>"When you say \u{201c}Berlin is in Germany\u{201d}, Wikidata confirms this AND adds that Berlin is a city, has 3.7M people, is the capital, sits on the Spree river \u{2014} all as hard facts with high confidence."</p>
                                    <p><strong>"Without a knowledge source, an empty graph has no context for building relationships."</strong></p>
                                </div>
                                <div class="wizard-cards">
                                    <div
                                        class=move || if kb_wikidata.get() { "wizard-card wizard-card-selected" } else { "wizard-card" }
                                        on:click=move |_| set_kb_wikidata.set(!kb_wikidata.get_untracked())
                                    >
                                        <h4>"Wikidata (Recommended)"</h4>
                                        <div class="wizard-card-grid">
                                            <span class="wc-label">"Coverage"</span><span>"100M+ entities, universal"</span>
                                            <span class="wc-label">"Quality"</span><span>"Excellent \u{2014} curated, structured"</span>
                                            <span class="wc-label">"License"</span><span>"CC0 (public domain)"</span>
                                            <span class="wc-label">"Auth"</span><span>"None needed"</span>
                                        </div>
                                    </div>
                                    <div
                                        class=move || if kb_dbpedia.get() { "wizard-card wizard-card-selected" } else { "wizard-card" }
                                        on:click=move |_| set_kb_dbpedia.set(!kb_dbpedia.get_untracked())
                                    >
                                        <h4>"DBpedia"</h4>
                                        <div class="wizard-card-grid">
                                            <span class="wc-label">"Coverage"</span><span>"Wikipedia-derived, encyclopedic"</span>
                                            <span class="wc-label">"Quality"</span><span>"Good for well-known entities"</span>
                                            <span class="wc-label">"License"</span><span>"CC-BY-SA"</span>
                                            <span class="wc-label">"Auth"</span><span>"None needed"</span>
                                        </div>
                                    </div>
                                </div>
                            </div>
                        }.into_any(),

                        STEP_WEB_SEARCH => view! {
                            <div class="wizard-step">
                                <h2><i class="fa-solid fa-magnifying-glass"></i>" Web Search"</h2>
                                <p class="wizard-desc">"Configure a web search provider for enriching seed entities with contextual information from the web. Used as fallback when Wikipedia coverage is thin."</p>
                                {move || {
                                    let p = web_search_provider.get();
                                    let name = match p.as_str() {
                                        "brave" => "Brave Search",
                                        "searxng" => "SearXNG (Self-hosted)",
                                        _ => "DuckDuckGo",
                                    };
                                    view! {
                                        <p style="font-size: 0.85rem; margin-bottom: 0.5rem;">
                                            <i class="fa-solid fa-circle-check" style="color: var(--accent);"></i>
                                            " Selected: "<strong>{name}</strong>
                                        </p>
                                    }
                                }}
                                <div class="wizard-cards">
                                    <div
                                        class=move || if web_search_provider.get() == "duckduckgo" { "wizard-card wizard-card-selected" } else { "wizard-card" }
                                        on:click=move |_| set_web_search_provider.set("duckduckgo".into())
                                    >
                                        <h4>"DuckDuckGo (Default)"</h4>
                                        <div class="wizard-card-grid">
                                            <span class="wc-label">"Auth"</span><span>"None needed"</span>
                                            <span class="wc-label">"Privacy"</span><span>"No tracking"</span>
                                            <span class="wc-label">"Cost"</span><span>"Free"</span>
                                        </div>
                                    </div>
                                    <div
                                        class=move || if web_search_provider.get() == "brave" { "wizard-card wizard-card-selected" } else { "wizard-card" }
                                        on:click=move |_| set_web_search_provider.set("brave".into())
                                    >
                                        <h4>"Brave Search"</h4>
                                        <div class="wizard-card-grid">
                                            <span class="wc-label">"Auth"</span><span>"API key required"</span>
                                            <span class="wc-label">"Quality"</span><span>"High, independent index"</span>
                                            <span class="wc-label">"Cost"</span><span>"Free tier: 2000/month"</span>
                                        </div>
                                    </div>
                                    <div
                                        class=move || if web_search_provider.get() == "searxng" { "wizard-card wizard-card-selected" } else { "wizard-card" }
                                        on:click=move |_| set_web_search_provider.set("searxng".into())
                                    >
                                        <h4>"SearXNG (Self-hosted)"</h4>
                                        <div class="wizard-card-grid">
                                            <span class="wc-label">"Auth"</span><span>"Self-hosted URL"</span>
                                            <span class="wc-label">"Privacy"</span><span>"Full control"</span>
                                            <span class="wc-label">"Cost"</span><span>"Free (self-hosted)"</span>
                                        </div>
                                    </div>
                                </div>
                                // Brave API key field
                                {move || (web_search_provider.get() == "brave").then(|| view! {
                                    <div class="form-group mt-1">
                                        <label><i class="fa-solid fa-key"></i>" Brave API Key"</label>
                                        <input type="password" class="form-control" placeholder="BSA..."
                                            prop:value=web_search_api_key
                                            on:input=move |ev| set_web_search_api_key.set(event_target_value(&ev))
                                        />
                                    </div>
                                })}
                                // SearXNG URL field
                                {move || (web_search_provider.get() == "searxng").then(|| view! {
                                    <div class="form-group mt-1">
                                        <label><i class="fa-solid fa-link"></i>" SearXNG URL"</label>
                                        <input type="text" class="form-control" placeholder="http://localhost:8080/search"
                                            prop:value=web_search_url
                                            on:input=move |ev| set_web_search_url.set(event_target_value(&ev))
                                        />
                                    </div>
                                })}
                                // Brave API key hint
                                {move || (web_search_provider.get() == "brave" && web_search_api_key.get().is_empty()).then(|| view! {
                                    <p class="text-secondary" style="font-size: 0.85rem; margin-top: 0.5rem;">
                                        <i class="fa-solid fa-info-circle"></i>" Get a free API key at search.brave.com/api"
                                    </p>
                                })}
                                // SearXNG URL hint
                                {move || (web_search_provider.get() == "searxng" && web_search_url.get().is_empty()).then(|| view! {
                                    <p class="text-secondary" style="font-size: 0.85rem; margin-top: 0.5rem;">
                                        <i class="fa-solid fa-info-circle"></i>" Enter your SearXNG instance URL (e.g. http://192.168.1.100:8080/search)"
                                    </p>
                                })}
                                // Test connection button (for Brave and SearXNG)
                                {move || (web_search_provider.get() != "duckduckgo").then(|| view! {
                                    <div class="flex gap-sm mt-1" style="align-items: center;">
                                        <button class="btn btn-sm btn-secondary"
                                            disabled=move || web_search_testing.get()
                                            on:click=move |_| { do_web_search_test.dispatch(()); }
                                        >
                                            {move || if web_search_testing.get() {
                                                view! { <span class="spinner"></span>" Testing..." }.into_any()
                                            } else {
                                                view! { <><i class="fa-solid fa-plug"></i>" Test Connection"</> }.into_any()
                                            }}
                                        </button>
                                        {move || web_search_test_result.get().map(|r| {
                                            let is_ok = !r.starts_with("FAIL");
                                            let cls = if is_ok { "text-success" } else { "text-danger" };
                                            let icon = if is_ok { "fa-solid fa-circle-check" } else { "fa-solid fa-circle-xmark" };
                                            view! {
                                                <span class=cls style="font-size: 0.85rem;">
                                                    <i class=icon></i>" "{r}
                                                </span>
                                            }
                                        })}
                                    </div>
                                })}
                            </div>
                        }.into_any(),

                        STEP_SEED => view! {
                            <div class="wizard-step">
                                <h2><i class="fa-solid fa-seedling"></i>" Seed Your Knowledge Graph"</h2>
                                <p class="wizard-desc">"Describe your area of interest in a few sentences. Be specific \u{2014} mention names, places, organizations, events. engram will detect your area of interest, extract entities, link them to knowledge bases, and discover connections."</p>
                                <div class="wizard-info-box" style="font-size: 0.85rem; padding: 8px 12px; margin-bottom: 0.75rem;">
                                    <p style="margin: 0 0 4px 0;"><i class="fa-solid fa-circle-info"></i><strong>" This wizard seeds world knowledge "</strong>"(people, places, events, organizations). For other knowledge types, use the dedicated ingest tools after setup:"</p>
                                    <ul style="margin: 4px 0 0 16px; padding: 0; list-style: none;">
                                        <li><i class="fa-solid fa-code" style="width: 16px;"></i>" Codebases \u{2014} AST parser for module/class/function graphs"</li>
                                        <li><i class="fa-solid fa-file-lines" style="width: 16px;"></i>" Documents \u{2014} PDF/Markdown import with NER extraction"</li>
                                        <li><i class="fa-solid fa-rss" style="width: 16px;"></i>" Live feeds \u{2014} RSS, webhooks, streaming ingest"</li>
                                        <li><i class="fa-solid fa-network-wired" style="width: 16px;"></i>" Internal systems \u{2014} structured data via API or batch import"</li>
                                    </ul>
                                </div>

                                // Phase 0: Input + templates
                                {move || (seed_phase.get() == 0).then(|| view! {
                                    <div>
                                        <div class="wizard-seed-examples">
                                            <span class="text-secondary">"Templates: "</span>
                                            {SEED_EXAMPLES.iter().map(|(label, text)| {
                                                let t = text.to_string();
                                                view! {
                                                    <button class="btn btn-sm btn-secondary" on:click=move |_| set_seed_text.set(t.clone())>
                                                        {*label}
                                                    </button>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </div>

                                        <textarea
                                            class="wizard-seed-input"
                                            rows="6"
                                            placeholder="Describe your domain of interest..."
                                            prop:value=seed_text
                                            on:input=move |ev| set_seed_text.set(event_target_value(&ev))
                                        ></textarea>

                                        <div class="flex gap-sm mt-1">
                                            <button class="btn btn-primary" on:click=move |_| { do_analyze.dispatch(()); }
                                                disabled=move || analyzing.get() || seed_text.get().trim().is_empty()>
                                                {move || if analyzing.get() {
                                                    view! { <span class="spinner"></span>" Detecting area of interest..." }.into_any()
                                                } else {
                                                    view! { <><i class="fa-solid fa-magnifying-glass-chart"></i>" Analyze"</> }.into_any()
                                                }}
                                            </button>
                                        </div>
                                    </div>
                                })}

                                // Phase 1: AoI + interactive entity table
                                {move || (seed_phase.get() >= 1).then(|| {
                                    let entities = seed_entities.get();
                                    let active_count = entities.iter().filter(|(_, _, _, skipped)| !skipped).count();
                                    view! {
                                        <div class="wizard-info-box mt-1">
                                            <h4><i class="fa-solid fa-crosshairs"></i>" Area of Interest"</h4>
                                            <div class="flex gap-sm" style="align-items: center;">
                                                <input type="text" class="form-control" style="flex: 1;"
                                                    prop:value=seed_aoi
                                                    on:input=move |ev| set_seed_aoi.set(event_target_value(&ev))
                                                />
                                            </div>
                                        </div>

                                        <div class="mt-1">
                                            <h4><i class="fa-solid fa-tags"></i>{format!(" Entities ({} active, {} total)", active_count, entities.len())}</h4>
                                            <table style="width: 100%; border-collapse: collapse; font-size: 0.9rem;">
                                                <thead>
                                                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.1);">
                                                        <th style="text-align: left; padding: 6px;">"Entity"</th>
                                                        <th style="text-align: left; padding: 6px;">"Type"</th>
                                                        <th style="text-align: right; padding: 6px;">"Conf."</th>
                                                        <th style="text-align: center; padding: 6px; width: 80px;">"Action"</th>
                                                    </tr>
                                                </thead>
                                                <tbody>
                                                    {entities.into_iter().enumerate().map(|(idx, (label, etype, conf, skipped))| {
                                                        let type_icon = match etype.as_str() {
                                                            "person" => "fa-solid fa-user",
                                                            "organization" => "fa-solid fa-building",
                                                            "location" => "fa-solid fa-location-dot",
                                                            "event" => "fa-solid fa-calendar",
                                                            "product" => "fa-solid fa-cube",
                                                            _ => "fa-solid fa-tag",
                                                        };
                                                        let row_style = if skipped {
                                                            "border-bottom: 1px solid rgba(255,255,255,0.05); opacity: 0.4; text-decoration: line-through;"
                                                        } else {
                                                            "border-bottom: 1px solid rgba(255,255,255,0.05);"
                                                        };
                                                        view! {
                                                            <tr style=row_style>
                                                                <td style="padding: 5px 6px;"><strong>{label}</strong></td>
                                                                <td style="padding: 5px 6px;"><i class={type_icon}></i>" "{etype}</td>
                                                                <td style="padding: 5px 6px; text-align: right;">{format!("{:.0}%", conf * 100.0)}</td>
                                                                <td style="padding: 5px 6px; text-align: center;">
                                                                    <button
                                                                        class=move || if skipped { "btn btn-xs btn-secondary" } else { "btn btn-xs btn-primary" }
                                                                        style="font-size: 0.75rem; padding: 2px 8px;"
                                                                        on:click=move |_| {
                                                                            let mut ents = seed_entities.get_untracked();
                                                                            if idx < ents.len() {
                                                                                ents[idx].3 = !ents[idx].3;
                                                                                set_seed_entities.set(ents);
                                                                            }
                                                                        }
                                                                    >
                                                                        {if skipped { "Restore" } else { "Skip" }}
                                                                    </button>
                                                                </td>
                                                            </tr>
                                                        }
                                                    }).collect::<Vec<_>>()}
                                                </tbody>
                                            </table>

                                            // Add entity row
                                            <div class="flex gap-sm mt-1" style="align-items: center;">
                                                <input type="text" class="form-control" style="flex: 1;"
                                                    placeholder="Add entity..."
                                                    prop:value=new_entity_label
                                                    on:input=move |ev| set_new_entity_label.set(event_target_value(&ev))
                                                    on:keydown=move |ev: web_sys::KeyboardEvent| {
                                                        if ev.key() == "Enter" {
                                                            let label = new_entity_label.get_untracked();
                                                            if !label.trim().is_empty() {
                                                                let etype = new_entity_type.get_untracked();
                                                                let mut ents = seed_entities.get_untracked();
                                                                ents.push((label.trim().to_string(), etype, 1.0, false));
                                                                set_seed_entities.set(ents);
                                                                set_new_entity_label.set(String::new());
                                                            }
                                                        }
                                                    }
                                                />
                                                <select class="form-control" style="width: 130px;"
                                                    prop:value=new_entity_type
                                                    on:change=move |ev| set_new_entity_type.set(event_target_value(&ev))
                                                >
                                                    <option value="entity">"entity"</option>
                                                    <option value="person">"person"</option>
                                                    <option value="organization">"org"</option>
                                                    <option value="location">"location"</option>
                                                    <option value="event">"event"</option>
                                                    <option value="product">"product"</option>
                                                </select>
                                                <button class="btn btn-sm btn-secondary"
                                                    on:click=move |_| {
                                                        let label = new_entity_label.get_untracked();
                                                        if !label.trim().is_empty() {
                                                            let etype = new_entity_type.get_untracked();
                                                            let mut ents = seed_entities.get_untracked();
                                                            ents.push((label.trim().to_string(), etype, 1.0, false));
                                                            set_seed_entities.set(ents);
                                                            set_new_entity_label.set(String::new());
                                                        }
                                                    }
                                                >
                                                    <i class="fa-solid fa-plus"></i>
                                                </button>
                                            </div>
                                        </div>
                                    }
                                })}

                                // Enrichment status
                                {move || (seed_phase.get() >= 1 && analyzing.get()).then(|| view! {
                                    <div class="wizard-info-box mt-1">
                                        <p><span class="spinner"></span>" Enriching entities via Wikipedia + SPARQL..."</p>
                                    </div>
                                })}

                                // Commit / Start Over buttons
                                {move || (seed_phase.get() >= 1 && !analyzing.get()).then(|| view! {
                                    <div class="flex gap-sm mt-1">
                                        <button class="btn btn-primary" on:click=move |_| { do_ingest.dispatch(()); }
                                            disabled=move || analyzing.get()>
                                            <i class="fa-solid fa-seedling"></i>" Commit to Graph"
                                        </button>
                                        <button class="btn btn-secondary" on:click=move |_| {
                                            set_seed_phase.set(0);
                                            set_seed_result.set(None);
                                            set_seed_aoi.set(String::new());
                                            set_seed_session_id.set(String::new());
                                            set_seed_entities.set(Vec::new());
                                        }>
                                            <i class="fa-solid fa-rotate-left"></i>" Start Over"
                                        </button>
                                    </div>
                                })}

                                <button class="btn btn-secondary mt-1" on:click=move |_| set_step.set(STEP_READY)>
                                    <i class="fa-solid fa-forward"></i>" Skip seeding for now"
                                </button>
                            </div>
                        }.into_any(),

                        STEP_READY => view! {
                            <div class="wizard-step wizard-ready">
                                <h2><i class="fa-solid fa-circle-check"></i>" You\u{2019}re Ready!"</h2>
                                <div class="wizard-summary">
                                    <div class="wizard-summary-row">
                                        <span class="wizard-summary-label">"Embedder"</span>
                                        <span>{move || { let c = embed_choice.get(); if c.is_empty() { "\u{2014}".to_string() } else { c } }}</span>
                                    </div>
                                    <div class="wizard-summary-row">
                                        <span class="wizard-summary-label">"NER"</span>
                                        <span>{move || { let c = ner_choice.get(); if c.is_empty() { "\u{2014}".to_string() } else { c } }}</span>
                                    </div>
                                    <div class="wizard-summary-row">
                                        <span class="wizard-summary-label">"LLM"</span>
                                        <span>{move || { let c = llm_choice.get(); if c.is_empty() { "Skipped".to_string() } else { c } }}</span>
                                    </div>
                                    <div class="wizard-summary-row">
                                        <span class="wizard-summary-label">"Quantization"</span>
                                        <span>{move || quant_choice.get()}</span>
                                    </div>
                                    <div class="wizard-summary-row">
                                        <span class="wizard-summary-label">"Knowledge Sources"</span>
                                        <span>{move || {
                                            let mut sources = Vec::new();
                                            if kb_wikidata.get() { sources.push("Wikidata"); }
                                            if kb_dbpedia.get() { sources.push("DBpedia"); }
                                            if sources.is_empty() { "None".to_string() } else { sources.join(", ") }
                                        }}</span>
                                    </div>
                                </div>
                                <div class="flex gap-sm mt-1">
                                    <button class="btn btn-primary" on:click=move |_| { do_complete.dispatch(()); }>
                                        <i class="fa-solid fa-compass"></i>" Explore Graph"
                                    </button>
                                    <button class="btn btn-secondary" on:click=move |_| { do_complete.dispatch(()); }>
                                        <i class="fa-solid fa-gear"></i>" Fine-tune Settings"
                                    </button>
                                </div>
                            </div>
                        }.into_any(),

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
