use leptos::prelude::*;

use crate::api::ApiClient;

// ── Step constants ──

const STEP_WELCOME: u32 = 1;
const STEP_EMBEDDER: u32 = 2;
const STEP_NER: u32 = 3;
const STEP_LLM: u32 = 4;
const STEP_QUANTIZATION: u32 = 5;
const STEP_KB_SOURCES: u32 = 6;
const STEP_SEED: u32 = 7;
const STEP_READY: u32 = 8;
const TOTAL_STEPS: u32 = 8;

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
    if has_embedder { score += 25; }
    if has_ner { score += 20; }
    if has_kb { score += 25; }
    if has_llm { score += 10; }
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
        id: "gliner", name: "GLiNER (Recommended)",
        quality: "High \u{2014} any entity type", speed: "~50ms/sentence",
        download: "~100MB model", license: "Apache-2.0",
        learning: "Discovers new entities \u{2192} feeds gazetteer for instant future recognition. Relation gazetteer learns every edge. KGE trains on graph structure.",
        models: &[
            ("gliner_small-v2.1", "GLiNER Small v2.1", "~50MB, fast, good quality", "onnx-community/gliner_small-v2.1", "EN"),
            ("gliner_medium-v2.1", "GLiNER Medium v2.1", "~110MB, better accuracy", "onnx-community/gliner_medium-v2.1", "EN"),
            ("gliner_large-v2.1", "GLiNER Large v2.1", "~340MB, best accuracy", "onnx-community/gliner_large-v2.1", "EN"),
            ("gliner_multi-v2.1", "GLiNER Multi v2.1", "~220MB, 12 languages", "onnx-community/gliner_multi-v2.1", "Multilingual"),
            ("gliner_multi_pii-v1", "GLiNER Multi PII v1", "~220MB, PII detection", "onnx-community/gliner_multi_pii-v1", "Multilingual"),
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
    let (ner_choice, set_ner_choice) = signal("gliner".to_string());
    let (ner_model, set_ner_model) = signal("gliner_small-v2.1".to_string());
    let (llm_choice, set_llm_choice) = signal(String::new());
    let (llm_key, set_llm_key) = signal(String::new());
    let (llm_model, set_llm_model) = signal(String::new());
    let (quant_choice, set_quant_choice) = signal("int8".to_string());
    let (kb_wikidata, set_kb_wikidata) = signal(true);
    let (kb_dbpedia, set_kb_dbpedia) = signal(false);
    let (seed_text, set_seed_text) = signal(String::new());

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
                        // Pull the selected model in Ollama
                        let dl = api.post_text("/config/ollama-pull", &serde_json::json!({ "model": embed_m })).await;
                        if let Err(e) = dl {
                            set_save_error.set(Some(format!("Ollama pull failed: {e}. Make sure Ollama is running and pull '{}' manually.", embed_m)));
                        }
                    }
                    api.post_text("/config", &config).await
                }
                STEP_NER => {
                    let provider = if ner == "gliner" { "anno" } else { &ner };
                    let mut config = serde_json::json!({ "ner_provider": provider });
                    if ner == "gliner" && !ner_m.is_empty() {
                        config["ner_model"] = serde_json::json!(&ner_m);
                        // Look up HuggingFace repo for selected model (preset or custom)
                        let ner_preset = NER_PRESETS.iter().find(|p| p.id == "gliner");
                        let hf_repo = ner_preset
                            .and_then(|p| p.models.iter().find(|(id, _, _, _, _)| *id == ner_m.as_str()))
                            .map(|(_, _, _, repo, _)| repo.to_string())
                            .unwrap_or_else(|| {
                                // Custom HuggingFace model ID
                                if ner_m.contains('/') { ner_m.clone() } else { format!("onnx-community/{}", ner_m) }
                            });

                        // Download selected GLiNER NER model
                        let dl = api.post_text("/config/ner-download", &serde_json::json!({
                            "model_id": ner_m,
                            "model_url": format!("https://huggingface.co/{}/resolve/main/onnx/model.onnx", hf_repo),
                            "tokenizer_url": format!("https://huggingface.co/{}/resolve/main/tokenizer.json", hf_repo),
                        })).await;
                        if let Err(e) = &dl {
                            set_save_error.set(Some(format!("GLiNER download failed: {e}. You can download later in System settings.")));
                        }

                        // Also download GLiREL relation extraction model (paired with GLiNER)
                        // Use matching size: small->small, medium->medium, large->large
                        let rel_size = if ner_m.contains("large") { "large" } else if ner_m.contains("medium") { "medium" } else { "small" };
                        let rel_model_id = format!("glirel_{}-v2.1", rel_size);
                        let rel_repo = format!("onnx-community/glirel_{}-v2.1", rel_size);
                        let rel_dl = api.post_text("/config/rel-download", &serde_json::json!({
                            "model_id": rel_model_id,
                            "model_url": format!("https://huggingface.co/{}/resolve/main/onnx/model.onnx", rel_repo),
                            "tokenizer_url": format!("https://huggingface.co/{}/resolve/main/tokenizer.json", rel_repo),
                        })).await;
                        if let Err(e) = &rel_dl {
                            let _ = e; // GLiREL download is non-fatal
                        }
                    }
                    api.post_text("/config", &config).await
                }
                STEP_LLM => {
                    if llm.is_empty() {
                        // Skip is OK for LLM
                        set_saving.set(false);
                        return true;
                    }
                    let preset = LLM_PRESETS.iter().find(|p| p.id == llm);
                    let endpoint = preset.map(|p| p.endpoint).unwrap_or("");
                    let mut config = serde_json::json!({ "llm_endpoint": endpoint });
                    if !llm_k.is_empty() { config["llm_api_key"] = serde_json::json!(llm_k); }
                    if !llm_m.is_empty() { config["llm_model"] = serde_json::json!(llm_m); }
                    // Pull model in Ollama if selected
                    if llm == "ollama" && !llm_m.is_empty() {
                        let dl = api.post_text("/config/ollama-pull", &serde_json::json!({ "model": llm_m })).await;
                        if let Err(e) = dl {
                            set_save_error.set(Some(format!("Ollama pull failed: {e}. Make sure Ollama is running and pull '{}' manually.", llm_m)));
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

    // Analyze seed text
    let api_analyze = api.clone();
    let do_analyze = Action::new_local(move |_: &()| {
        let api = api_analyze.clone();
        let text = seed_text.get_untracked();
        async move {
            if text.trim().is_empty() { return; }
            set_analyzing.set(true);
            set_seed_result.set(None);
            match api.post_text("/ingest/analyze", &serde_json::json!({ "text": text })).await {
                Ok(resp) => {
                    set_seed_result.set(Some(resp));
                }
                Err(e) => {
                    set_seed_result.set(Some(format!("Analysis failed: {e}")));
                }
            }
            set_analyzing.set(false);
        }
    });

    // Ingest seed text
    let api_ingest = api.clone();
    let do_ingest = Action::new_local(move |_: &()| {
        let api = api_ingest.clone();
        let text = seed_text.get_untracked();
        async move {
            if text.trim().is_empty() { return; }
            set_analyzing.set(true);
            match api.post_text("/ingest", &serde_json::json!({ "text": text })).await {
                Ok(resp) => {
                    set_seed_result.set(Some(format!("Seeded! {resp}")));
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
        }
    });

    // Next/back navigation
    let go_next = move |_| {
        let current = step.get_untracked();
        // For steps that need saving, dispatch save and advance on success
        if matches!(current, STEP_EMBEDDER | STEP_NER | STEP_LLM | STEP_QUANTIZATION | STEP_KB_SOURCES) {
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
                                        <li>"Relation gazetteer learns every edge you store \u{2192} instant recall next time"</li>
                                        <li>"GLiREL gets better candidate labels from your growing graph"</li>
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

                        STEP_LLM => view! {
                            <div class="wizard-step">
                                <h2><i class="fa-solid fa-comments"></i>" Language Model"</h2>
                                <p class="wizard-desc">"A language model gives engram conversational intelligence \u{2014} ask questions, get reasoning, run what-if scenarios. Not required for core knowledge storage and search."</p>
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
                                <button class="btn btn-secondary mt-1" on:click=move |_| { set_llm_choice.set(String::new()); }>
                                    <i class="fa-solid fa-forward"></i>" Skip LLM for now"
                                </button>
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

                        STEP_SEED => view! {
                            <div class="wizard-step">
                                <h2><i class="fa-solid fa-seedling"></i>" Seed Your Knowledge Graph"</h2>
                                <p class="wizard-desc">"Describe your area of interest in a few sentences. Be specific \u{2014} mention names, places, organizations, events. engram will extract every entity, look them up in your configured knowledge sources, and build an initial knowledge graph with verified facts and relationships."</p>

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
                                    <button class="btn btn-secondary" on:click=move |_| { do_analyze.dispatch(()); }
                                        disabled=move || analyzing.get() || seed_text.get().trim().is_empty()>
                                        {move || if analyzing.get() {
                                            view! { <span class="spinner"></span>" Analyzing..." }.into_any()
                                        } else {
                                            view! { <><i class="fa-solid fa-magnifying-glass-chart"></i>" Analyze"</> }.into_any()
                                        }}
                                    </button>
                                    <button class="btn btn-primary" on:click=move |_| { do_ingest.dispatch(()); }
                                        disabled=move || analyzing.get() || seed_text.get().trim().is_empty()>
                                        <i class="fa-solid fa-seedling"></i>" Seed Knowledge Graph"
                                    </button>
                                </div>

                                {move || seed_result.get().map(|r| view! {
                                    <pre class="wizard-seed-result">{r}</pre>
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
