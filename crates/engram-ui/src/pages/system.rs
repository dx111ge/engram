use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{
    ComputeResponse, ConfigResponse, ConfigStatusResponse, HealthResponse,
    MeshAuditEntry, PeerInfo, ResetResponse, SecretListItem,
    StatsResponse,
};
// Card grid + modal layout (no more CollapsibleSection)

// ── Provider presets (matching wizard) ──

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

struct OnnxQuickModel {
    name: &'static str,
    desc: &'static str,
    model_url: &'static str,
    tokenizer_url: &'static str,
}

const ONNX_QUICK_MODELS: &[OnnxQuickModel] = &[
    OnnxQuickModel {
        name: "multilingual-e5-small",
        desc: "384d, 120MB, 100+ langs",
        model_url: "https://huggingface.co/intfloat/multilingual-e5-small/resolve/main/onnx/model.onnx",
        tokenizer_url: "https://huggingface.co/intfloat/multilingual-e5-small/resolve/main/tokenizer.json",
    },
    OnnxQuickModel {
        name: "all-MiniLM-L6-v2",
        desc: "384d, 90MB, English",
        model_url: "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx",
        tokenizer_url: "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json",
    },
    OnnxQuickModel {
        name: "bge-small-en-v1.5",
        desc: "384d, 130MB, English",
        model_url: "https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/onnx/model.onnx",
        tokenizer_url: "https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/tokenizer.json",
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

/// Parse ONNX status JSON into human-readable string
fn parse_onnx_status(text: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
        let ready = v.get("ready").and_then(|v| v.as_bool()).unwrap_or(false);
        let size_mb = v.get("model_size_mb").and_then(|v| v.as_f64());
        if ready {
            if let Some(mb) = size_mb {
                format!("ONNX model installed ({:.1} MB). Ready to use.", mb)
            } else {
                "ONNX model installed. Ready to use.".to_string()
            }
        } else {
            "No ONNX model installed".to_string()
        }
    } else {
        text.to_string()
    }
}

/// Parse NER model check JSON into human-readable string
#[allow(dead_code)]
fn parse_ner_model_status(text: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
        let ready = v.get("ready").and_then(|v| v.as_bool()).unwrap_or(false);
        let size_mb = v.get("model_size_mb").and_then(|v| v.as_f64());
        if ready {
            if let Some(mb) = size_mb {
                format!("Model installed ({:.1} MB). Ready to use.", mb)
            } else {
                "Model installed. Ready to use.".to_string()
            }
        } else {
            "Not installed".to_string()
        }
    } else {
        text.to_string()
    }
}

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
    can_fetch_models: bool,
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
        can_fetch_models: true,
    },
    LlmPreset {
        id: "lmstudio", name: "LM Studio", endpoint: "http://localhost:1234/v1/chat/completions",
        needs_key: false, quality: "Good", privacy: "Local", cost: "Free",
        models: &[],
        default_model: "",
        can_fetch_models: true,
    },
    LlmPreset {
        id: "vllm", name: "vLLM", endpoint: "http://localhost:8000/v1/chat/completions",
        needs_key: false, quality: "Model dependent", privacy: "Local", cost: "Free",
        models: &[],
        default_model: "",
        can_fetch_models: true,
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
        can_fetch_models: false,
    },
    LlmPreset {
        id: "google", name: "Google", endpoint: "https://generativelanguage.googleapis.com/v1beta",
        needs_key: true, quality: "Excellent", privacy: "Cloud", cost: "Per-token",
        models: &[
            ("gemini-2.0-flash", "fast, cheap"),
            ("gemini-1.5-pro", "best quality"),
            ("gemini-1.5-flash", "balanced"),
        ],
        default_model: "gemini-2.0-flash",
        can_fetch_models: false,
    },
    LlmPreset {
        id: "deepseek", name: "DeepSeek", endpoint: "https://api.deepseek.com/v1/chat/completions",
        needs_key: true, quality: "Good-Excellent", privacy: "Cloud", cost: "Per-token (cheap)",
        models: &[
            ("deepseek-chat", "fast general model"),
            ("deepseek-reasoner", "R1 reasoning model"),
        ],
        default_model: "deepseek-chat",
        can_fetch_models: false,
    },
    LlmPreset {
        id: "openrouter", name: "OpenRouter", endpoint: "https://openrouter.ai/api/v1/chat/completions",
        needs_key: true, quality: "Depends on model", privacy: "Cloud", cost: "Per-token",
        models: &[
            ("anthropic/claude-3.5-sonnet", "Claude 3.5 Sonnet"),
            ("meta-llama/llama-3.1-70b", "Llama 3.1 70B"),
            ("google/gemini-pro-1.5", "Gemini Pro 1.5"),
        ],
        default_model: "anthropic/claude-3.5-sonnet",
        can_fetch_models: false,
    },
    LlmPreset {
        id: "custom", name: "Custom", endpoint: "",
        needs_key: false, quality: "Provider dependent", privacy: "Depends", cost: "Depends",
        models: &[],
        default_model: "",
        can_fetch_models: false,
    },
];

const THINKING_MODELS: &[&str] = &["deepseek-r1", "deepseek-reasoner", "qwq", "o3-mini"];

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

    // ── Section 1: Connection ──

    let (api_url, set_api_url) = signal(ApiClient::load_base_url());
    let (health_status, set_health_status) = signal(String::new());

    let test_connection = Action::new_local(move |_: &()| {
        let url = api_url.get_untracked();
        let client = ApiClient::new(&url);
        async move {
            match client.get::<HealthResponse>("/health").await {
                Ok(h) => set_health_status.set(format!("Connected - {} ({} nodes, {} edges)", h.status, h.nodes, h.edges)),
                Err(e) => set_health_status.set(format!("Failed: {e}")),
            }
        }
    });

    let save_url = move |_| {
        let url = api_url.get_untracked();
        ApiClient::save_base_url(&url);
        set_status_msg.set("API URL saved. Reload the page to apply.".to_string());
        set_modal_open.set(String::new());
    };

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

    let _on_embed_provider_change = move |ev: web_sys::Event| {
        let val = event_target_value(&ev);
        set_embed_provider.set(val.clone());
        if let Some(p) = EMBED_PRESETS.iter().find(|p| p.id == val) {
            set_embed_endpoint.set(p.endpoint.to_string());
        }
    };

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

    let _on_llm_provider_change = move |ev: web_sys::Event| {
        let val = event_target_value(&ev);
        set_llm_provider.set(val.clone());
        if let Some(p) = LLM_PRESETS.iter().find(|p| p.id == val) {
            set_llm_endpoint.set(p.endpoint.to_string());
        }
        set_llm_fetched_models.set(Vec::new());
    };

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
            // Use backend proxy to fetch models (handles Ollama + OpenAI-compatible)
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

    let api_ner_save = api.clone();
    let save_ner = Action::new_local(move |_: &()| {
        let api = api_ner_save.clone();
        let provider = ner_provider.get_untracked();
        let endpoint = ner_endpoint.get_untracked();
        let model = ner_model.get_untracked();
        let coref = coref_enabled.get_untracked();
        let threshold = rel_threshold.get_untracked();
        let templates_json = relation_templates_json.get_untracked();
        async move {
            let mut body = serde_json::json!({
                "ner_provider": provider,
                "ner_endpoint": endpoint,
                "ner_model": model,
                "coreference_enabled": coref,
                "rel_threshold": threshold,
            });
            // Parse relation templates JSON if provided
            if !templates_json.trim().is_empty() {
                if let Ok(templates) = serde_json::from_str::<serde_json::Value>(&templates_json) {
                    body["relation_templates"] = templates;
                }
            }
            match api.post_text("/config", &body).await {
                Ok(_) => { set_status_msg.set("NER/RE config saved.".to_string()); set_modal_open.set(String::new()); }
                Err(e) => set_status_msg.set(format!("Error saving config: {e}")),
            }
        }
    });

    let api_ner_dl = api.clone();
    // download_gliner is now a no-op Action kept for compatibility; actual download uses spawn_local below
    let _api_ner_dl_kept = api_ner_dl;

    // ── Section 5: Quantization ──

    let api_quant = api.clone();
    let _toggle_quantization = Action::new_local(move |_: &()| {
        let api = api_quant.clone();
        let enabled = quant_enabled.get_untracked();
        async move {
            let body = serde_json::json!({ "enabled": enabled });
            match api.post_text("/quantize", &body).await {
                Ok(r) => { set_status_msg.set(format!("Quantization {}: {r}", if enabled { "enabled" } else { "disabled" })); set_modal_open.set(String::new()); }
                Err(e) => set_status_msg.set(format!("Quantization error: {e}")),
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

    // ── Section 7: Secrets ──

    let (secret_key, set_secret_key) = signal(String::new());
    let (secret_value, set_secret_value) = signal(String::new());

    let api_add_secret = api.clone();
    let add_secret = Action::new_local(move |_: &()| {
        let api = api_add_secret.clone();
        let key = secret_key.get_untracked();
        let value = secret_value.get_untracked();
        async move {
            let path = format!("/secrets/{key}");
            let body = serde_json::json!({ "value": value });
            match api.post_text(&path, &body).await {
                Ok(_) => {
                    set_status_msg.set(format!("Secret '{key}' saved."));
                    set_secret_key.set(String::new());
                    set_secret_value.set(String::new());
                }
                Err(e) => set_status_msg.set(format!("Secret save error: {e}")),
            }
        }
    });

    // Delete secret needs spawn_local since it takes a parameter
    let api_del_secret = api.clone();
    let (del_secret_trigger, set_del_secret_trigger) = signal(Option::<String>::None);

    Effect::new(move |_| {
        if let Some(key) = del_secret_trigger.get() {
            let api = api_del_secret.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let path = format!("/secrets/{key}");
                match api.delete(&path).await {
                    Ok(_) => set_status_msg.set(format!("Secret '{key}' deleted.")),
                    Err(e) => set_status_msg.set(format!("Delete error: {e}")),
                }
            });
            set_del_secret_trigger.set(None);
        }
    });

    // ── Section 8: Import/Export ──

    let (import_text, set_import_text) = signal(String::new());

    let api_export = api.clone();
    let do_export = Action::new_local(move |_: &()| {
        let api = api_export.clone();
        async move {
            match api.get_text("/export/jsonld").await {
                Ok(text) => set_import_text.set(text),
                Err(e) => set_status_msg.set(format!("Export error: {e}")),
            }
        }
    });

    let api_import = api.clone();
    let do_import = Action::new_local(move |_: &()| {
        let api = api_import.clone();
        let text = import_text.get_untracked();
        async move {
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(body) => {
                    match api.post_text("/import/jsonld", &body).await {
                        Ok(r) => set_status_msg.set(format!("Import complete: {r}")),
                        Err(e) => set_status_msg.set(format!("Import error: {e}")),
                    }
                }
                Err(e) => set_status_msg.set(format!("Invalid JSON: {e}")),
            }
        }
    });

    // ── Status indicators (derived from loaded config/compute) ──

    let connection_status: Signal<String> = Signal::derive(move || {
        let h = health_status.get();
        if h.starts_with("Connected") { "Connected".into() }
        else if h.starts_with("Failed") { "Error".into() }
        else { String::new() }
    });

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

    let _quant_status: Signal<String> = Signal::derive(move || {
        if quant_enabled.get() { "Active".into() } else { "Disabled".into() }
    });

    let _mesh_status: Signal<String> = Signal::derive(move || {
        let count = peers.get().map(|p| p.len()).unwrap_or(0);
        if count > 0 { format!("Active ({count} peers)") } else { "Not enabled".into() }
    });

    let secrets_status: Signal<String> = Signal::derive(move || {
        let count = secrets.get().map(|v| v.len()).unwrap_or(0);
        if count > 0 { format!("{count} keys") } else { "No secrets".into() }
    });

    let _export_status: Signal<String> = Signal::derive(move || {
        "Available".to_string()
    });

    // ── View ──

    let api_for_onnx = api.clone();
    let api_for_ner = api.clone();
    let _api_for_kb = api.clone(); // KB Endpoints removed from UI
    let api_for_db = api.clone();

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
                // ── Card: Connection ──
                <div class="system-card" on:click=move |_| set_modal_open.set("connection".into())>
                    <div class="system-card-header">
                        <span class="system-card-icon"><i class="fa-solid fa-plug"></i></span>
                        <span class="system-card-title">"Connection"</span>
                    </div>
                    <div class="system-card-status">{move || {
                        let url = api_url.get();
                        let st = connection_status.get();
                        let txt = if st.is_empty() { url } else { format!("{url} | {st}") };
                        let dot = if st == "Connected" { "status-dot green" } else { "status-dot amber" };
                        view! { <span class={dot}></span>{txt} }
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

        // ── Modal: Connection ──
        <div class=move || if modal_open.get() == "connection" { "modal-overlay active" } else { "modal-overlay" }>
            <div class="wizard-modal">
                <div class="wizard-modal-header">
                    <h3><i class="fa-solid fa-plug"></i>" Connection"</h3>
                    <button class="btn btn-secondary btn-sm" on:click=move |_| set_modal_open.set(String::new())>
                        <i class="fa-solid fa-xmark"></i>
                    </button>
                </div>
                <div class="wizard-modal-body">
            <div class="form-row">
                <label>"API URL"</label>
                <input
                    type="text"
                    placeholder="http://localhost:3030"
                    prop:value=api_url
                    on:input=move |ev| set_api_url.set(event_target_value(&ev))
                />
            </div>
            <div class="button-group">
                <button class="btn btn-primary" on:click=move |_| { test_connection.dispatch(()); }>
                    <i class="fa-solid fa-satellite-dish"></i>" Test"
                </button>
                <button class="btn btn-success" on:click=save_url>
                    <i class="fa-solid fa-floppy-disk"></i>" Save"
                </button>
            </div>
            {move || {
                let st = health_status.get();
                (!st.is_empty()).then(|| view! {
                    <div class="info-box" style="margin-top: 0.5rem;">
                        <i class="fa-solid fa-circle-info"></i>" "{st}
                    </div>
                })
            }}
                </div> // wizard-modal-body
            </div> // wizard-modal
        </div> // modal-overlay connection

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
                </div> // wizard-modal-body
            </div> // wizard-modal
        </div> // modal-overlay embedding

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
                </div> // wizard-modal-body
            </div> // wizard-modal
        </div> // modal-overlay llm

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

            // ── Wizard-style NER provider cards ──
            <p class="wizard-desc">"NER finds people, places, organizations, and concepts in your text. This is how engram knows what you\u{2019}re talking about."</p>
            <div class="wizard-info-box">
                <h4><i class="fa-solid fa-graduation-cap"></i>" Self-improving pipeline"</h4>
                <p>"engram learns from every entity found:"</p>
                <ul>
                    <li>"NER discovers new entities \u{2192} stored in graph \u{2192} gazetteer indexes them for instant future recognition"</li>
                    <li>"GLiNER2 relation extraction: zero-shot, multilingual, in single model pass"</li>
                    <li>"Relation gazetteer learns every edge you store \u{2192} instant recall next time"</li>
                </ul>
                <p><em>"The more you use engram, the faster and more accurate it becomes."</em></p>
            </div>
            <div class="wizard-cards">
                {NER_PRESETS.iter().map(|p| {
                    let id = p.id.to_string();
                    // Map wizard "gliner2" to system "gliner" for the signal
                    let signal_id = if id == "gliner2" { "gliner".to_string() } else { id.clone() };
                    let signal_id2 = signal_id.clone();
                    view! {
                        <div
                            class=move || if ner_provider.get() == signal_id { "wizard-card wizard-card-selected" } else { "wizard-card" }
                            on:click=move |_| set_ner_provider.set(signal_id2.clone())
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

            // ── GLiNER2 model chips (when gliner selected) ──
            {move || {
                let choice = ner_provider.get();
                // Map "gliner" -> find "gliner2" preset
                let preset = if choice == "gliner" {
                    NER_PRESETS.iter().find(|p| p.id == "gliner2")
                } else {
                    NER_PRESETS.iter().find(|p| p.id == choice.as_str())
                };
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
                                            class=move || if ner_selected_model.get() == mid { "wizard-model-chip active" } else { "wizard-model-chip" }
                                            on:click=move |_| {
                                                set_ner_selected_model.set(mid2.clone());
                                                set_ner_model.set(mid2.clone());
                                            }
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
                                            let m = ner_selected_model.get();
                                            let is_preset = NER_PRESETS.iter()
                                                .flat_map(|p| p.models.iter())
                                                .any(|(id, _, _, _, _)| *id == m.as_str());
                                            if is_preset { String::new() } else { m }
                                        }
                                        on:input=move |ev| {
                                            let v = event_target_value(&ev);
                                            if !v.trim().is_empty() {
                                                set_ner_selected_model.set(v.clone());
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

            // ── Relation Extraction section (like wizard STEP_REL) ──
            {move || {
                let is_gliner = ner_provider.get() == "gliner";
                is_gliner.then(|| view! {
                    <div style="margin-top: 1rem;">
                        <h4><i class="fa-solid fa-link"></i>" Relation Extraction"</h4>
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
                                    <label>"Custom relation types JSON"</label>
                                    <textarea
                                        class="form-control"
                                        style="width: 100%; min-height: 120px; font-family: monospace; font-size: 0.8rem; background: rgba(0,0,0,0.2); color: inherit; border: 1px solid rgba(255,255,255,0.1);"
                                        prop:value=relation_templates_json
                                        on:input=move |ev| {
                                            set_relation_templates_json.set(event_target_value(&ev));
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
                })
            }}

            // ── System extras ──

            // NER model download status
            {
                let api_ner_panel = api_for_ner.clone();
                move || {
                let is_gliner = ner_provider.get() == "gliner";
                let api_dl_ner = api_ner_panel.clone();
                let api_save_after_dl = api_dl_ner.clone();
                is_gliner.then(|| view! {
                    <div class="card" style="margin: 0.75rem 0; padding: 0.75rem; background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.08);">
                        <h4 style="margin-top: 0;"><i class="fa-solid fa-cloud-arrow-down"></i>" Model Download"</h4>

                        // Install status
                        {move || {
                            let st = ner_model_status.get();
                            if st.is_empty() {
                                None
                            } else if st.contains("Installed") || st.contains("installed") {
                                Some(view! {
                                    <div style="margin: 0.25rem 0; padding: 0.35rem 0.5rem; background: rgba(46,204,113,0.1); border: 1px solid rgba(46,204,113,0.3); border-radius: 4px; font-size: 0.8rem;">
                                        <i class="fa-solid fa-circle-check" style="color: #2ecc71; margin-right: 0.25rem;"></i>{st.clone()}
                                    </div>
                                }.into_any())
                            } else {
                                Some(view! {
                                    <div style="margin: 0.25rem 0; padding: 0.35rem 0.5rem; background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.08); border-radius: 4px; font-size: 0.8rem;">
                                        <i class="fa-solid fa-circle-info" style="margin-right: 0.25rem;"></i>{st.clone()}
                                    </div>
                                }.into_any())
                            }
                        }}

                        // Download status
                        {move || {
                            let st = ner_download_status.get();
                            (!st.is_empty()).then(|| view! {
                                <div class="info-box" style="margin: 0.25rem 0; font-size: 0.8rem;">
                                    <i class="fa-solid fa-spinner fa-spin" style="margin-right: 0.25rem;"></i>{st}
                                </div>
                            })
                        }}

                        // Download & Enable button
                        <div class="button-group" style="margin-top: 0.5rem;">
                            <button class="btn btn-primary" on:click={
                                let api_dl = api_dl_ner.clone();
                                let api_save = api_save_after_dl.clone();
                                move |_| {
                                    let sel = ner_selected_model.get_untracked();
                                    if sel.is_empty() {
                                        set_ner_download_status.set("Please select a model first.".into());
                                        return;
                                    }
                                    // Check if it's a preset model with known repo
                                    let preset_model = NER_PRESETS.iter()
                                        .flat_map(|p| p.models.iter())
                                        .find(|(id, _, _, _, _)| *id == sel.as_str());

                                    let (model_id, repo) = if let Some((id, _name, _desc, hf_repo, _lang)) = preset_model {
                                        (id.to_string(), hf_repo.to_string())
                                    } else {
                                        // Custom model ID
                                        let repo = if sel.contains('/') { sel.clone() } else { format!("onnx-community/{}", sel) };
                                        let mid = repo.split('/').last().unwrap_or(&repo).to_string();
                                        (mid, repo)
                                    };

                                    let variant = if model_id.contains("fp32") { "fp32" } else { "fp16" };
                                    let api_dl = api_dl.clone();
                                    let api_save = api_save.clone();
                                    set_ner_download_status.set(format!("Downloading {}...", model_id));
                                    wasm_bindgen_futures::spawn_local(async move {
                                        let body = serde_json::json!({
                                            "repo_id": repo,
                                            "variant": variant,
                                        });
                                        match api_dl.post_text("/config/gliner2-download", &body).await {
                                            Ok(r) => {
                                                set_ner_download_status.set(String::new());
                                                let status_msg = if let Ok(v) = serde_json::from_str::<serde_json::Value>(&r) {
                                                    let size = v.get("model_size_mb").and_then(|v| v.as_f64());
                                                    if let Some(mb) = size {
                                                        format!("Model installed ({:.1} MB). Ready to use.", mb)
                                                    } else {
                                                        "Model installed. Ready to use.".to_string()
                                                    }
                                                } else {
                                                    format!("Installed: {r}")
                                                };
                                                set_ner_model_status.set(status_msg);
                                                let cfg_body = serde_json::json!({
                                                    "ner_provider": "gliner2",
                                                    "ner_model": model_id,
                                                });
                                                let _ = api_save.post_text("/config", &cfg_body).await;
                                                set_status_msg.set("GLiNER2 model installed and NER config saved.".into());
                                            }
                                            Err(e) => {
                                                set_ner_download_status.set(String::new());
                                                set_ner_model_status.set(format!("Download failed: {e}"));
                                            }
                                        }
                                    });
                                }
                            }>
                                <i class="fa-solid fa-cloud-arrow-down"></i>" Download & Enable"
                            </button>
                        </div>
                    </div>
                })
            }}

            // Coreference Resolution toggle (Coming Soon)
            <div class="card" style="margin: 0.75rem 0; padding: 0.75rem; background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.08); opacity: 0.5;">
                <h4 style="margin-top: 0;">
                    <i class="fa-solid fa-users"></i>" Coreference Resolution"
                    <span class="badge badge-archival" style="margin-left: 0.5rem; font-size: 0.65rem;">"Coming Soon"</span>
                </h4>
                <p class="text-secondary" style="font-size: 0.85rem; margin-bottom: 0.5rem;">
                    "Resolves pronouns and noun phrases to canonical entity names. E.g. \"He\" -> \"John Smith\". (planned feature)"
                </p>
                <div style="display: flex; align-items: center; gap: 0.5rem;">
                    <input type="checkbox" disabled=true prop:checked=coref_enabled />
                    <label style="margin: 0; color: var(--text-muted);">"Enable coreference resolution"</label>
                </div>
            </div>

            // Save NER/RE Config button
            <div class="button-group" style="margin-top: 0.5rem;">
                <button class="btn btn-success" on:click=move |_| { save_ner.dispatch(()); }>
                    <i class="fa-solid fa-floppy-disk"></i>" Save NER/RE Config"
                </button>
            </div>
                </div> // wizard-modal-body
            </div> // wizard-modal
        </div> // modal-overlay ner

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
            <h4>"Stored Secrets"</h4>
            {move || {
                let list = secrets.get().unwrap_or_default();
                if list.is_empty() {
                    view! {
                        <p class="text-muted">"No secrets stored."</p>
                    }.into_any()
                } else {
                    view! {
                        <table class="data-table">
                            <thead>
                                <tr>
                                    <th>"Key"</th>
                                    <th>"Actions"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {list.into_iter().map(|s| {
                                    let k = s.key.clone();
                                    let k2 = s.key.clone();
                                    view! {
                                        <tr>
                                            <td><code>{k}</code></td>
                                            <td>
                                                <button
                                                    class="btn btn-danger btn-sm"
                                                    on:click=move |_| {
                                                        set_del_secret_trigger.set(Some(k2.clone()));
                                                    }
                                                >
                                                    <i class="fa-solid fa-trash"></i>
                                                </button>
                                            </td>
                                        </tr>
                                    }
                                }).collect::<Vec<_>>()}
                            </tbody>
                        </table>
                    }.into_any()
                }
            }}
            <h4 style="margin-top: 1rem;">"Add Secret"</h4>
            <div class="form-row">
                <label>"Key"</label>
                <input
                    type="text"
                    placeholder="SECRET_NAME"
                    prop:value=secret_key
                    on:input=move |ev| set_secret_key.set(event_target_value(&ev))
                />
            </div>
            <div class="form-row">
                <label>"Value"</label>
                <input
                    type="password"
                    placeholder="secret value..."
                    prop:value=secret_value
                    on:input=move |ev| set_secret_value.set(event_target_value(&ev))
                />
            </div>
            <div class="button-group">
                <button class="btn btn-success" on:click=move |_| { add_secret.dispatch(()); }>
                    <i class="fa-solid fa-plus"></i>" Save Secret"
                </button>
            </div>
                </div> // wizard-modal-body
            </div> // wizard-modal
        </div> // modal-overlay secrets

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
                    // ── Import / Export ──
            <h4><i class="fa-solid fa-download" style="margin-right: 0.25rem;"></i>" Export"</h4>
            <p class="text-secondary" style="font-size: 0.85rem; margin-bottom: 0.5rem;">
                "Export your knowledge base as JSON-LD for backup or sharing."
            </p>
            <button class="btn btn-primary" on:click=move |_| { do_export.dispatch(()); }>
                <i class="fa-solid fa-file-export"></i>" Export as JSON-LD"
            </button>

            <h4 style="margin-top: 1.25rem;"><i class="fa-solid fa-upload" style="margin-right: 0.25rem;"></i>" Import"</h4>
            <p class="text-secondary" style="font-size: 0.85rem; margin-bottom: 0.5rem;">
                "Import knowledge from a JSON-LD file."
            </p>
            <div class="form-row">
                <input type="file" accept=".json,.jsonld" id="import-file" />
            </div>
            <div class="form-row">
                <textarea
                    class="code-area"
                    rows="6"
                    placeholder="Or paste JSON-LD data here..."
                    prop:value=import_text
                    on:input=move |ev| set_import_text.set(event_target_value(&ev))
                />
            </div>
            <button class="btn btn-success" on:click=move |_| { do_import.dispatch(()); }>
                <i class="fa-solid fa-file-import"></i>" Import"
            </button>

                    // ── Database Management (inlined) ──
                    <DatabaseManagementInline api=api_for_db set_status_msg=set_status_msg />
                </div> // wizard-modal-body
            </div> // wizard-modal
        </div> // modal-overlay database
    }
}

// ── Database Management section ──

#[component]
fn DatabaseManagementInline(
    api: ApiClient,
    set_status_msg: WriteSignal<String>,
) -> impl IntoView {
    let (config_status, set_config_status) = signal(Option::<ConfigStatusResponse>::None);
    let (reset_confirm, set_reset_confirm) = signal(String::new());
    let (reset_result, set_reset_result) = signal(Option::<String>::None);

    // Load config status
    let api_status = api.clone();
    Effect::new(move |_| {
        let api = api_status.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(status) = api.get::<ConfigStatusResponse>("/config/status").await {
                set_config_status.set(Some(status));
            }
        });
    });

    // Reset action
    let api_reset = api.clone();
    let do_reset = Action::new_local(move |_: &()| {
        let api = api_reset.clone();
        async move {
            if reset_confirm.get_untracked() != "yes" {
                set_reset_result.set(Some("Type 'yes' to confirm reset.".into()));
                return;
            }
            set_reset_result.set(Some("Resetting...".into()));
            match api.post::<_, ResetResponse>("/admin/reset", &serde_json::json!({})).await {
                Ok(r) => {
                    if r.success {
                        let cleaned = if r.sidecars_cleaned.is_empty() {
                            "none".into()
                        } else {
                            r.sidecars_cleaned.join(", ")
                        };
                        set_reset_result.set(Some(format!("Reset complete. Sidecars cleaned: {cleaned}")));
                        set_reset_confirm.set(String::new());
                        // Refresh status
                        if let Ok(status) = api.get::<ConfigStatusResponse>("/config/status").await {
                            set_config_status.set(Some(status));
                        }
                    } else {
                        set_reset_result.set(Some("Reset failed.".into()));
                    }
                }
                Err(e) => set_reset_result.set(Some(format!("Error: {e}"))),
            }
        }
    });

    view! {
        <div>
            <h4 style="margin-top: 1.5rem;"><i class="fa-solid fa-database"></i>" Database Management"</h4>
            // Stats
            {move || config_status.get().map(|status| view! {
                <div class="stat-grid" style="margin-bottom: 1rem;">
                    <div class="stat-card">
                        <div class="stat-value">{status.node_count.to_string()}</div>
                        <div class="stat-label">"Nodes"</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-value">{status.edge_count.to_string()}</div>
                        <div class="stat-label">"Edges"</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-value">{if status.ready { "Ready" } else { "Not Ready" }}</div>
                        <div class="stat-label">"Status"</div>
                    </div>
                </div>

                {if !status.warnings.is_empty() {
                    Some(view! {
                        <div style="color: var(--warning); font-size: 0.85rem; margin-bottom: 0.75rem;">
                            {status.warnings.iter().map(|w| view! {
                                <p><i class="fa-solid fa-triangle-exclamation"></i>" " {w.clone()}</p>
                            }).collect::<Vec<_>>()}
                        </div>
                    })
                } else { None }}

                <div class="text-secondary" style="font-size: 0.85rem; margin-bottom: 0.5rem;">
                    <strong>"Configured: "</strong>{status.configured.join(", ")}
                </div>
                {if !status.missing.is_empty() {
                    Some(view! {
                        <div class="text-secondary" style="font-size: 0.85rem; margin-bottom: 0.75rem;">
                            <strong>"Missing: "</strong>{status.missing.join(", ")}
                        </div>
                    })
                } else { None }}
            })}

            // Reset section
            <h4 style="margin-top: 1rem; color: var(--danger, #e74c3c);">
                <i class="fa-solid fa-triangle-exclamation"></i>" Reset Database"
            </h4>
            <p class="text-secondary" style="font-size: 0.85rem; margin-bottom: 0.75rem;">
                "This will delete all nodes, edges, and learned data. Configuration, users, and secrets are preserved."
            </p>

            {move || reset_result.get().map(|msg| view! {
                <div class="card" style="padding: 0.5rem; margin-bottom: 0.75rem;">
                    <i class="fa-solid fa-info-circle" style="color: var(--accent-bright);"></i>
                    " " {msg}
                </div>
            })}

            <div class="form-row">
                <label>"Type 'yes' to confirm"</label>
                <input type="text" placeholder="yes"
                    prop:value=reset_confirm
                    on:input=move |ev| set_reset_confirm.set(event_target_value(&ev))
                />
            </div>
            <div class="button-group">
                <button class="btn btn-danger"
                    on:click=move |_| { do_reset.dispatch(()); }
                    disabled=move || reset_confirm.get() != "yes">
                    <i class="fa-solid fa-trash"></i>" Reset Database"
                </button>
            </div>

            // ── Rerun Onboarding Wizard ──
            <h4 style="margin-top: 1.5rem;"><i class="fa-solid fa-hat-wizard"></i>" Onboarding Wizard"</h4>
            <p class="text-secondary" style="font-size: 0.85rem; margin-bottom: 0.75rem;">
                "Rerun the setup wizard to reconfigure settings or seed a new topic into your knowledge graph."
            </p>
            <div class="button-group">
                <button class="btn btn-primary" on:click={
                    move |_| {
                        // Open wizard directly via shared context signal
                        if let Some(set_open) = use_context::<WriteSignal<bool>>() {
                            set_open.set(true);
                        }
                    }
                }>
                    <i class="fa-solid fa-hat-wizard"></i>" Run Setup Wizard"
                </button>
            </div>
        </div>
    }
}

fn event_target_checked(ev: &web_sys::Event) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|el| el.checked())
        .unwrap_or(false)
}
