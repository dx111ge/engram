use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{
    ComputeResponse, ConfigResponse, ConfigStatusResponse, HealthResponse,
    MeshAuditEntry, PeerInfo, ResetResponse, SecretListItem,
    StatsResponse,
};
use crate::components::collapsible_section::CollapsibleSection;

// ── Provider presets ──

struct ProviderPreset {
    id: &'static str,
    name: &'static str,
    endpoint: &'static str,
    needs_key: bool,
}

const EMBED_PROVIDERS: &[ProviderPreset] = &[
    ProviderPreset { id: "onnx", name: "ONNX (Local)", endpoint: "onnx://local", needs_key: false },
    ProviderPreset { id: "ollama", name: "Ollama", endpoint: "http://localhost:11434/api/embed", needs_key: false },
    ProviderPreset { id: "openai", name: "OpenAI", endpoint: "https://api.openai.com/v1/embeddings", needs_key: true },
    ProviderPreset { id: "vllm", name: "vLLM", endpoint: "http://localhost:8000/v1/embeddings", needs_key: false },
    ProviderPreset { id: "lmstudio", name: "LM Studio", endpoint: "http://localhost:1234/v1/embeddings", needs_key: false },
    ProviderPreset { id: "custom", name: "Custom", endpoint: "", needs_key: false },
];

const EMBED_MODEL_SUGGESTIONS: &[&str] = &[
    "nomic-embed-text",
    "mxbai-embed-large",
    "all-minilm",
    "snowflake-arctic-embed",
    "bge-m3",
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

struct NerModelInfo {
    id: &'static str,
    label: &'static str,
    size: &'static str,
    langs: &'static str,
    license: &'static str,
    license_ok: bool,
    model_url: &'static str,
    tokenizer_url: &'static str,
    hf_url: &'static str,
}

const NER_MODELS: &[NerModelInfo] = &[
    NerModelInfo {
        id: "gliner_small-v2.1",
        label: "GLiNER Small v2.1",
        size: "183 MB",
        langs: "English",
        license: "Apache-2.0",
        license_ok: true,
        model_url: "https://huggingface.co/onnx-community/gliner_small-v2.1/resolve/main/onnx/model_int8.onnx",
        tokenizer_url: "https://huggingface.co/onnx-community/gliner_small-v2.1/resolve/main/tokenizer.json",
        hf_url: "https://huggingface.co/onnx-community/gliner_small-v2.1",
    },
    NerModelInfo {
        id: "gliner_medium-v2.1",
        label: "GLiNER Medium v2.1",
        size: "~350 MB",
        langs: "English",
        license: "Apache-2.0",
        license_ok: true,
        model_url: "https://huggingface.co/onnx-community/gliner_medium-v2.1/resolve/main/onnx/model_int8.onnx",
        tokenizer_url: "https://huggingface.co/onnx-community/gliner_medium-v2.1/resolve/main/tokenizer.json",
        hf_url: "https://huggingface.co/onnx-community/gliner_medium-v2.1",
    },
    NerModelInfo {
        id: "gliner_multi-v2.1",
        label: "GLiNER Multi v2.1",
        size: "~350 MB",
        langs: "100+ languages",
        license: "Apache-2.0",
        license_ok: true,
        model_url: "https://huggingface.co/onnx-community/gliner_multi-v2.1/resolve/main/onnx/model_int8.onnx",
        tokenizer_url: "https://huggingface.co/onnx-community/gliner_multi-v2.1/resolve/main/tokenizer.json",
        hf_url: "https://huggingface.co/onnx-community/gliner_multi-v2.1",
    },
    NerModelInfo {
        id: "gliner_large-v2.1",
        label: "GLiNER Large v2.1",
        size: "~800 MB",
        langs: "English",
        license: "Apache-2.0",
        license_ok: true,
        model_url: "https://huggingface.co/onnx-community/gliner_large-v2.1/resolve/main/onnx/model_int8.onnx",
        tokenizer_url: "https://huggingface.co/onnx-community/gliner_large-v2.1/resolve/main/tokenizer.json",
        hf_url: "https://huggingface.co/onnx-community/gliner_large-v2.1",
    },
    NerModelInfo {
        id: "gliner_small-v1",
        label: "GLiNER Small v1.0",
        size: "~180 MB",
        langs: "English",
        license: "CC-BY-NC-4.0",
        license_ok: false,
        model_url: "https://huggingface.co/onnx-community/gliner_small/resolve/main/onnx/model_int8.onnx",
        tokenizer_url: "https://huggingface.co/onnx-community/gliner_small/resolve/main/tokenizer.json",
        hf_url: "https://huggingface.co/urchade/gliner_small",
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

struct LlmProviderPreset {
    id: &'static str,
    name: &'static str,
    endpoint: &'static str,
    needs_key: bool,
    description: &'static str,
    can_fetch_models: bool,
    model_suggestions: &'static [&'static str],
}

const LLM_PROVIDERS: &[LlmProviderPreset] = &[
    LlmProviderPreset {
        id: "ollama", name: "Ollama",
        endpoint: "http://localhost:11434/v1/chat/completions",
        needs_key: false,
        description: "Local LLM server. Install from ollama.com, pull a model, then connect.",
        can_fetch_models: true,
        model_suggestions: &["llama3", "llama3.1", "mistral", "mixtral", "gemma2", "phi3", "qwen2", "deepseek-r1", "command-r"],
    },
    LlmProviderPreset {
        id: "lmstudio", name: "LM Studio",
        endpoint: "http://localhost:1234/v1/chat/completions",
        needs_key: false,
        description: "Local LLM with GUI. Download a model in LM Studio, start the server.",
        can_fetch_models: true,
        model_suggestions: &[],
    },
    LlmProviderPreset {
        id: "vllm", name: "vLLM",
        endpoint: "http://localhost:8000/v1/chat/completions",
        needs_key: false,
        description: "High-performance inference server. Best for GPU-accelerated local inference.",
        can_fetch_models: true,
        model_suggestions: &[],
    },
    LlmProviderPreset {
        id: "openai", name: "OpenAI",
        endpoint: "https://api.openai.com/v1/chat/completions",
        needs_key: true,
        description: "Cloud API. Requires an API key from platform.openai.com.",
        can_fetch_models: false,
        model_suggestions: &["gpt-4o", "gpt-4o-mini", "gpt-4-turbo", "o3-mini"],
    },
    LlmProviderPreset {
        id: "google", name: "Google",
        endpoint: "https://generativelanguage.googleapis.com/v1beta",
        needs_key: true,
        description: "Google Gemini API. Requires API key from aistudio.google.com.",
        can_fetch_models: false,
        model_suggestions: &["gemini-1.5-pro", "gemini-1.5-flash", "gemini-2.0-flash"],
    },
    LlmProviderPreset {
        id: "deepseek", name: "DeepSeek",
        endpoint: "https://api.deepseek.com/v1/chat/completions",
        needs_key: true,
        description: "DeepSeek API. Known for reasoning models (R1).",
        can_fetch_models: false,
        model_suggestions: &["deepseek-chat", "deepseek-reasoner"],
    },
    LlmProviderPreset {
        id: "openrouter", name: "OpenRouter",
        endpoint: "https://openrouter.ai/api/v1/chat/completions",
        needs_key: true,
        description: "Unified gateway to 100+ models. Also supports Anthropic Claude via OpenAI-compatible API.",
        can_fetch_models: false,
        model_suggestions: &["anthropic/claude-3.5-sonnet", "meta-llama/llama-3.1-70b", "google/gemini-pro-1.5"],
    },
    LlmProviderPreset {
        id: "custom", name: "Custom",
        endpoint: "",
        needs_key: false,
        description: "Any OpenAI-compatible endpoint.",
        can_fetch_models: false,
        model_suggestions: &[],
    },
];

const THINKING_MODELS: &[&str] = &["deepseek-r1", "deepseek-reasoner", "qwq", "o3-mini"];

// ── System page ──

#[component]
pub fn SystemPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let (status_msg, set_status_msg) = signal(String::new());

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
    };

    // ── Section 2: Embedding ──

    let (embed_provider, set_embed_provider) = signal("ollama".to_string());
    let (embed_endpoint, set_embed_endpoint) = signal(String::new());
    let (embed_model, set_embed_model) = signal(String::new());
    let (embed_test_status, set_embed_test_status) = signal(String::new());
    let (onnx_status, set_onnx_status) = signal(String::new());

    // Sync config values once loaded
    Effect::new(move |_| {
        if let Some(cfg) = config.get().flatten() {
            if let Some(ep) = cfg.data.get("embed_endpoint").and_then(|v: &serde_json::Value| v.as_str()) {
                set_embed_endpoint.set(ep.to_string());
                for p in EMBED_PROVIDERS {
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

    let on_embed_provider_change = move |ev: web_sys::Event| {
        let val = event_target_value(&ev);
        set_embed_provider.set(val.clone());
        if let Some(p) = EMBED_PROVIDERS.iter().find(|p| p.id == val) {
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
                Ok(_) => set_status_msg.set("Embedding settings saved.".to_string()),
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
                for p in LLM_PROVIDERS {
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

    let on_llm_provider_change = move |ev: web_sys::Event| {
        let val = event_target_value(&ev);
        set_llm_provider.set(val.clone());
        if let Some(p) = LLM_PROVIDERS.iter().find(|p| p.id == val) {
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
                Ok(_) => set_status_msg.set("LLM settings saved.".to_string()),
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
            match api.post_text("/llm/proxy", &body).await {
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
    let (rel_model_status, set_rel_model_status) = signal(String::new());
    let (rel_download_status, set_rel_download_status) = signal(String::new());

    // Quantization signal declared early so config Effect can set it
    let (quant_enabled, set_quant_enabled) = signal(true);

    Effect::new(move |_| {
        if let Some(cfg) = config.get().flatten() {
            if let Some(v) = cfg.data.get("ner_provider").and_then(|v: &serde_json::Value| v.as_str()) {
                set_ner_provider.set(v.to_string());
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
        }
    });

    let api_ner_save = api.clone();
    let save_ner = Action::new_local(move |_: &()| {
        let api = api_ner_save.clone();
        let provider = ner_provider.get_untracked();
        let endpoint = ner_endpoint.get_untracked();
        let model = ner_model.get_untracked();
        async move {
            let body = serde_json::json!({
                "ner_provider": provider,
                "ner_endpoint": endpoint,
                "ner_model": model,
            });
            match api.post_text("/config", &body).await {
                Ok(_) => set_status_msg.set("NER provider saved.".to_string()),
                Err(e) => set_status_msg.set(format!("Error saving NER config: {e}")),
            }
        }
    });

    let api_ner_dl = api.clone();
    // download_gliner is now a no-op Action kept for compatibility; actual download uses spawn_local below
    let _api_ner_dl_kept = api_ner_dl;

    // ── Section 5: Quantization ──

    let api_quant = api.clone();
    let toggle_quantization = Action::new_local(move |_: &()| {
        let api = api_quant.clone();
        let enabled = quant_enabled.get_untracked();
        async move {
            let body = serde_json::json!({ "enabled": enabled });
            match api.post_text("/quantize", &body).await {
                Ok(r) => set_status_msg.set(format!("Quantization {}: {r}", if enabled { "enabled" } else { "disabled" })),
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
        let provider_name = EMBED_PROVIDERS.iter()
            .find(|p| p.endpoint == ep)
            .map(|p| p.name)
            .unwrap_or("");
        compute.get().flatten()
            .and_then(|c| c.embedder_model)
            .map(|m| {
                if provider_name == "ONNX (Local)" {
                    "ONNX Local".to_string()
                } else {
                    m
                }
            })
            .unwrap_or_else(|| {
                if ep.starts_with("onnx://") { "ONNX Local".into() }
                else if !ep.is_empty() { provider_name.to_string() }
                else { "not configured".into() }
            })
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
            "anno" => "GLiNER (ONNX)".into(),
            other => other.to_string(),
        }
    });

    let quant_status: Signal<String> = Signal::derive(move || {
        if quant_enabled.get() { "Active".into() } else { "Disabled".into() }
    });

    let mesh_status: Signal<String> = Signal::derive(move || {
        let count = peers.get().map(|p| p.len()).unwrap_or(0);
        if count > 0 { format!("Active ({count} peers)") } else { "Not enabled".into() }
    });

    let secrets_status: Signal<String> = Signal::derive(move || {
        let count = secrets.get().map(|v| v.len()).unwrap_or(0);
        if count > 0 { format!("{count} keys") } else { "No secrets".into() }
    });

    let export_status: Signal<String> = Signal::derive(move || {
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
            <p class="text-secondary">"Control panel \u{2014} connection, models, mesh, secrets, data"</p>
        </div>

        {move || {
            let msg = status_msg.get();
            (!msg.is_empty()).then(|| view! {
                <div class="alert">{msg}</div>
            })
        }}

        // ── 1. Connection ──
        <CollapsibleSection title="Connection" icon="fa-solid fa-plug" collapsed=true status=connection_status>
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
        </CollapsibleSection>

        // ── 2. Embedding ──
        <CollapsibleSection title="Embedding Model" icon="fa-solid fa-circle-nodes" collapsed=true status=embed_status>
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
            <div class="form-row">
                <label>"Provider"</label>
                <select prop:value=embed_provider on:change=on_embed_provider_change>
                    {EMBED_PROVIDERS.iter().map(|p| {
                        let id = p.id.to_string();
                        let name = p.name.to_string();
                        view! { <option value={id}>{name}</option> }
                    }).collect::<Vec<_>>()}
                </select>
            </div>

            // ONNX sub-panel (shown when ONNX provider selected)
            {
                let api_onnx_panel = api_for_onnx.clone();
                move || {
                let is_onnx = embed_provider.get() == "onnx";
                let api_check = api_onnx_panel.clone();
                let api_dl = api_check.clone();
                let api_upload = api_check.clone();
                is_onnx.then(|| view! {
                    <div class="card" style="margin: 0.75rem 0; padding: 0.75rem; background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.08);">
                        <h4 style="margin-top: 0;"><i class="fa-solid fa-microchip"></i>" ONNX Local Embedding"</h4>
                        <p class="text-secondary" style="font-size: 0.85rem;">
                            "Run embeddings locally without any external service. "
                            <a href="https://huggingface.co/models?pipeline_tag=feature-extraction&library=onnx" target="_blank" style="color: var(--accent-bright);">
                                <i class="fa-solid fa-arrow-up-right-from-square" style="font-size: 0.7rem;"></i>" Browse Embedding Models on HuggingFace"
                            </a>
                        </p>

                        // ── ONNX status display ──
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

                        // ── Quick Install buttons ──
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
                                                            // Parse the download response JSON
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

                        // ── Custom HuggingFace model ──
                        <details style="margin-top: 0.75rem;">
                            <summary class="text-secondary" style="cursor: pointer; font-size: 0.85rem;"><i class="fa-brands fa-github" style="margin-right: 0.25rem;"></i>"Custom HuggingFace Model"</summary>
                            <div style="margin-top: 0.5rem;">
                                <p class="text-secondary" style="font-size: 0.8rem; margin-bottom: 0.5rem;">
                                    "Enter any sentence-transformer ONNX model from huggingface.co. Must contain onnx/model.onnx and tokenizer.json."
                                </p>
                                <div class="form-row">
                                    <label>"HuggingFace Model ID"</label>
                                    <input type="text" class="form-control" id="onnx-custom-hf-id"
                                        placeholder="e.g. sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2"
                                    />
                                </div>
                                <div class="button-group" style="margin-top: 0.5rem;">
                                    <button class="btn btn-primary" on:click={
                                        let api = api_dl.clone();
                                        move |_| {
                                            let api = api.clone();
                                            wasm_bindgen_futures::spawn_local(async move {
                                                use wasm_bindgen::JsCast;
                                                let doc = web_sys::window().unwrap().document().unwrap();
                                                let input = doc.get_element_by_id("onnx-custom-hf-id")
                                                    .and_then(|el| el.dyn_into::<web_sys::HtmlInputElement>().ok());
                                                let hf_id = input.map(|i| i.value()).unwrap_or_default();
                                                if hf_id.trim().is_empty() {
                                                    set_onnx_status.set("Please enter a HuggingFace model ID.".into());
                                                    return;
                                                }
                                                let hf_id = hf_id.trim();
                                                // If no slash, assume sentence-transformers namespace
                                                let repo = if hf_id.contains('/') { hf_id.to_string() } else { format!("sentence-transformers/{}", hf_id) };
                                                set_onnx_status.set(format!("Downloading {}...", repo));
                                                let body = serde_json::json!({
                                                    "model_url": format!("https://huggingface.co/{}/resolve/main/onnx/model.onnx", repo),
                                                    "tokenizer_url": format!("https://huggingface.co/{}/resolve/main/tokenizer.json", repo),
                                                });
                                                match api.post_text("/config/onnx-download", &body).await {
                                                    Ok(r) => {
                                                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&r) {
                                                            let size = v.get("model_size_mb").and_then(|v| v.as_f64());
                                                            let size_str = size.map(|s| format!(" ({:.1} MB)", s)).unwrap_or_default();
                                                            set_onnx_status.set(format!("{} installed{}.", repo, size_str));
                                                        } else {
                                                            set_onnx_status.set(format!("{} installed.", repo));
                                                        }
                                                    }
                                                    Err(e) => set_onnx_status.set(format!("Download failed: {e}")),
                                                }
                                            });
                                        }
                                    }>
                                        <i class="fa-solid fa-cloud-arrow-down"></i>" Download from HuggingFace"
                                    </button>
                                </div>
                            </div>
                        </details>

                        // ── Manual upload ──
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
                                                            // parse_onnx_status is for GET, this is POST response
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

                        <p class="text-secondary" style="font-size: 0.7rem; margin-top: 0.5rem; margin-bottom: 0;">
                            <i class="fa-solid fa-bolt" style="margin-right: 0.25rem;"></i>"Powered by ONNX Runtime"
                        </p>
                    </div>
                })
            }}

            // API provider fields (shown when NOT ONNX)
            {move || {
                let is_api = embed_provider.get() != "onnx";
                is_api.then(|| view! {
                    <div class="form-row">
                        <label>"Endpoint"</label>
                        <input
                            type="text"
                            prop:value=embed_endpoint
                            on:input=move |ev| set_embed_endpoint.set(event_target_value(&ev))
                        />
                    </div>
                    <div class="form-row">
                        <label>"Model"</label>
                        <input
                            type="text"
                            placeholder="e.g. nomic-embed-text"
                            list="embed-model-suggestions"
                            prop:value=embed_model
                            on:input=move |ev| set_embed_model.set(event_target_value(&ev))
                        />
                        <datalist id="embed-model-suggestions">
                            {EMBED_MODEL_SUGGESTIONS.iter().map(|m| {
                                let val = m.to_string();
                                view! { <option value={val} /> }
                            }).collect::<Vec<_>>()}
                        </datalist>
                    </div>
                })
            }}

            {move || {
                let info = compute.get().flatten();
                info.map(|c| {
                    let dim_str = c.embedder_dim.map(|d| format!("{d}")).unwrap_or_else(|| "N/A".to_string());
                    let model_str = c.embedder_model.clone().unwrap_or_else(|| "not configured".to_string());
                    view! {
                        <div class="info-box" style="margin-top: 0.5rem;">
                            <i class="fa-solid fa-ruler-combined"></i>
                            " Active model: "{model_str}" | Dimensions: "{dim_str}
                        </div>
                    }
                })
            }}
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
        </CollapsibleSection>

        // ── 3. Language Model ──
        <CollapsibleSection title="Language Model" icon="fa-solid fa-robot" collapsed=true status=llm_status>
            <div class="form-row">
                <label>"Provider"</label>
                <select prop:value=llm_provider on:change=on_llm_provider_change>
                    {LLM_PROVIDERS.iter().map(|p| {
                        let id = p.id.to_string();
                        let name = p.name.to_string();
                        view! { <option value={id}>{name}</option> }
                    }).collect::<Vec<_>>()}
                </select>
            </div>

            // Provider description
            {move || {
                let prov = llm_provider.get();
                LLM_PROVIDERS.iter().find(|p| p.id == prov).map(|p| {
                    let desc = p.description.to_string();
                    view! {
                        <div class="text-secondary" style="font-size: 0.85rem; margin: 0.25rem 0 0.5rem 0;">
                            <i class="fa-solid fa-circle-info" style="margin-right: 0.25rem;"></i>{desc}
                        </div>
                    }
                })
            }}

            // OpenRouter/Anthropic note
            {move || {
                let prov = llm_provider.get();
                (prov == "openrouter").then(|| view! {
                    <div class="info-box" style="margin-bottom: 0.5rem;">
                        <i class="fa-solid fa-circle-info"></i>
                        " Anthropic Claude is not OpenAI-compatible directly. Use "
                        <a href="https://openrouter.ai" target="_blank" style="color: var(--accent-bright);">"OpenRouter"</a>
                        " as a gateway to access Claude with an OpenAI-compatible API."
                    </div>
                })
            }}

            <div class="form-row">
                <label>"Endpoint URL"</label>
                <input
                    type="text"
                    prop:value=llm_endpoint
                    on:input=move |ev| set_llm_endpoint.set(event_target_value(&ev))
                />
            </div>
            {move || {
                let provider = llm_provider.get();
                let needs_key = LLM_PROVIDERS.iter().find(|p| p.id == provider).map(|p| p.needs_key).unwrap_or(false);
                needs_key.then(|| view! {
                    <div class="form-row">
                        <label>
                            "API Key"
                            {move || {
                                llm_has_key.get().then(|| view! {
                                    <span class="badge badge-core" style="margin-left: 0.5rem; font-size: 0.65rem;">"key stored"</span>
                                })
                            }}
                        </label>
                        <input
                            type="password"
                            placeholder="sk-..."
                            prop:value=llm_api_key
                            on:input=move |ev| set_llm_api_key.set(event_target_value(&ev))
                        />
                    </div>
                })
            }}
            <div class="form-row">
                <label>"Model"</label>
                <input
                    type="text"
                    placeholder="e.g. llama3, gpt-4o"
                    list="llm-model-suggestions"
                    prop:value=llm_model
                    on:input=move |ev| set_llm_model.set(event_target_value(&ev))
                />
                <datalist id="llm-model-suggestions">
                    // Provider-specific suggestions
                    {move || {
                        let prov = llm_provider.get();
                        let fetched = llm_fetched_models.get();
                        let mut items: Vec<String> = Vec::new();

                        // Add fetched models first
                        for m in &fetched {
                            items.push(m.clone());
                        }

                        // Add preset suggestions
                        if let Some(p) = LLM_PROVIDERS.iter().find(|p| p.id == prov) {
                            for s in p.model_suggestions {
                                if !items.contains(&s.to_string()) {
                                    items.push(s.to_string());
                                }
                            }
                        }

                        items.into_iter().map(|m| {
                            view! { <option value={m} /> }
                        }).collect::<Vec<_>>()
                    }}
                </datalist>
            </div>

            // Temperature slider
            <div class="form-row">
                <label>"Temperature: " {move || llm_temperature.get()}</label>
                <input
                    type="range"
                    min="0" max="2" step="0.1"
                    prop:value=llm_temperature
                    on:input=move |ev| set_llm_temperature.set(event_target_value(&ev))
                    style="width: 100%;"
                />
            </div>

            // Thinking model checkbox
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
            <div class="button-group">
                <button class="btn btn-success" on:click=move |_| { save_llm.dispatch(()); }>
                    <i class="fa-solid fa-floppy-disk"></i>" Save LLM Config"
                </button>
                <button class="btn btn-secondary" on:click=move |_| { test_llm.dispatch(()); }>
                    <i class="fa-solid fa-satellite-dish"></i>" Test Connection"
                </button>
                {move || {
                    let prov = llm_provider.get();
                    let can_fetch = LLM_PROVIDERS.iter().find(|p| p.id == prov).map(|p| p.can_fetch_models).unwrap_or(false);
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
        </CollapsibleSection>

        // ── 4. NER ──
        <CollapsibleSection title="NER / Entity Recognition" icon="fa-solid fa-tags" collapsed=true status=ner_status>
            <p class="text-secondary" style="font-size: 0.85rem; margin-bottom: 0.75rem;">
                "NER Provider"
            </p>
            <div class="ner-card-grid">
                // Built-in (Rule-based)
                <div
                    class=move || if ner_provider.get() == "builtin" { "ner-card ner-card-selected" } else { "ner-card" }
                    on:click=move |_| set_ner_provider.set("builtin".into())
                >
                    <div class="ner-card-header">
                        <input type="radio" name="ner_provider" prop:checked=move || ner_provider.get() == "builtin" />
                        <strong>"Built-in (Rule-based)"</strong>
                        <span class="badge" style="background: var(--text-muted); color: #fff; font-size: 0.65rem; margin-left: auto;">"Basic"</span>
                    </div>
                    <p class="text-secondary" style="font-size: 0.8rem; margin: 0.25rem 0 0;">
                        "Pattern matching and gazetteer lookup. Always available, zero setup. Learns from your graph: known entities are recognized automatically via the built-in gazetteer."
                    </p>
                </div>
                // GLiNER (ONNX)
                <div
                    class=move || if ner_provider.get() == "anno" { "ner-card ner-card-selected" } else { "ner-card" }
                    on:click=move |_| set_ner_provider.set("anno".into())
                >
                    <div class="ner-card-header">
                        <input type="radio" name="ner_provider" prop:checked=move || ner_provider.get() == "anno" />
                        <strong>"GLiNER (ONNX)"</strong>
                        <span class="badge" style="background: var(--success, #2ecc71); color: #000; font-size: 0.65rem; margin-left: auto;">"Excellent"</span>
                    </div>
                    <p class="text-secondary" style="font-size: 0.8rem; margin: 0.25rem 0 0;">
                        "Zero-shot NER via GLiNER models. Runs locally, no external service. Combined with graph-learned entity gazetteer for disambiguation and boosted recall."
                    </p>
                </div>
            </div>

            // GLiNER model selector (shown when anno provider selected)
            {
                let api_ner_panel = api_for_ner.clone();
                move || {
                let is_anno = ner_provider.get() == "anno";
                let api_check_ner = api_ner_panel.clone();
                let api_dl_ner = api_check_ner.clone();
                let api_save_after_dl = api_dl_ner.clone();
                is_anno.then(|| view! {
                    <div class="card" style="margin: 0.75rem 0; padding: 0.75rem; background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.08);">
                        <h4 style="margin-top: 0;"><i class="fa-solid fa-tags"></i>" GLiNER Model"</h4>
                        <p class="text-secondary" style="font-size: 0.85rem; margin-bottom: 0.5rem;">
                            "Select and install a GLiNER ONNX model for entity recognition."
                        </p>

                        // Model selector dropdown
                        <div class="form-row">
                            <label>"Model"</label>
                            <select prop:value=ner_selected_model on:change={
                                let api = api_check_ner.clone();
                                move |ev: web_sys::Event| {
                                    let val = event_target_value(&ev);
                                    set_ner_selected_model.set(val.clone());
                                    set_ner_model_status.set(String::new());
                                    if !val.is_empty() {
                                        let api = api.clone();
                                        let model_id = val.clone();
                                        set_ner_model_status.set("Checking...".into());
                                        wasm_bindgen_futures::spawn_local(async move {
                                            let path = format!("/config/ner-model?id={}", model_id);
                                            match api.get_text(&path).await {
                                                Ok(text) => set_ner_model_status.set(parse_ner_model_status(&text)),
                                                Err(_) => set_ner_model_status.set("Not installed".into()),
                                            }
                                        });
                                    }
                                }
                            }>
                                <option value="">"-- Select a model --"</option>
                                <optgroup label="Commercial-friendly (Apache-2.0)">
                                    {NER_MODELS.iter().filter(|m| m.license_ok).map(|m| {
                                        let id = m.id.to_string();
                                        let label = format!("{} ({})", m.label, m.size);
                                        view! { <option value={id}>{label}</option> }
                                    }).collect::<Vec<_>>()}
                                </optgroup>
                                <optgroup label="Non-commercial (CC-BY-NC-4.0)">
                                    {NER_MODELS.iter().filter(|m| !m.license_ok).map(|m| {
                                        let id = m.id.to_string();
                                        let label = format!("{} ({})", m.label, m.size);
                                        view! { <option value={id}>{label}</option> }
                                    }).collect::<Vec<_>>()}
                                </optgroup>
                            </select>
                        </div>

                        // Model info card
                        {move || {
                            let sel = ner_selected_model.get();
                            NER_MODELS.iter().find(|m| m.id == sel).map(|m| {
                                let license_style = if m.license_ok {
                                    "background: rgba(46,204,113,0.15); color: #2ecc71; border: 1px solid rgba(46,204,113,0.3);"
                                } else {
                                    "background: rgba(231,76,60,0.15); color: #e74c3c; border: 1px solid rgba(231,76,60,0.3);"
                                };
                                let license_icon = if m.license_ok { "fa-solid fa-check" } else { "fa-solid fa-triangle-exclamation" };
                                let size = m.size.to_string();
                                let langs = m.langs.to_string();
                                let license = m.license.to_string();
                                let hf = m.hf_url.to_string();
                                view! {
                                    <div style="margin: 0.5rem 0; padding: 0.5rem; background: rgba(255,255,255,0.02); border-radius: 4px; font-size: 0.8rem;">
                                        <div style="display: flex; gap: 1rem; flex-wrap: wrap; align-items: center;">
                                            <span><i class="fa-solid fa-hard-drive" style="margin-right: 0.25rem;"></i>{size}</span>
                                            <span><i class="fa-solid fa-globe" style="margin-right: 0.25rem;"></i>{langs}</span>
                                            <span style={format!("padding: 0.1rem 0.4rem; border-radius: 3px; font-size: 0.7rem; {license_style}")}>
                                                <i class={license_icon} style="margin-right: 0.2rem;"></i>{license}
                                            </span>
                                            <a href={hf} target="_blank" style="color: var(--accent-bright); font-size: 0.75rem;">
                                                <i class="fa-solid fa-arrow-up-right-from-square" style="margin-right: 0.15rem;"></i>"HuggingFace"
                                            </a>
                                        </div>
                                    </div>
                                }
                            })
                        }}

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

                        // Download & Enable button (preset models)
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
                                    let model_info = match NER_MODELS.iter().find(|m| m.id == sel) {
                                        Some(m) => m,
                                        None => {
                                            set_ner_download_status.set("Unknown model selected.".into());
                                            return;
                                        }
                                    };
                                    let model_id = model_info.id.to_string();
                                    let model_url = model_info.model_url.to_string();
                                    let tokenizer_url = model_info.tokenizer_url.to_string();
                                    let api_dl = api_dl.clone();
                                    let api_save = api_save.clone();
                                    set_ner_download_status.set(format!("Downloading {}...", model_info.label));
                                    wasm_bindgen_futures::spawn_local(async move {
                                        let body = serde_json::json!({
                                            "model_id": model_id,
                                            "model_url": model_url,
                                            "tokenizer_url": tokenizer_url,
                                        });
                                        match api_dl.post_text("/config/ner-download", &body).await {
                                            Ok(r) => {
                                                set_ner_download_status.set(String::new());
                                                // Parse the JSON response
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
                                                // Auto-save config with this model
                                                let cfg_body = serde_json::json!({
                                                    "ner_provider": "anno",
                                                    "ner_model": model_id,
                                                });
                                                let _ = api_save.post_text("/config", &cfg_body).await;
                                                set_status_msg.set("GLiNER model installed and NER config saved.".into());
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

                        // ── Custom HuggingFace NER model ──
                        <details style="margin-top: 0.75rem;">
                            <summary class="text-secondary" style="cursor: pointer; font-size: 0.85rem;"><i class="fa-brands fa-github" style="margin-right: 0.25rem;"></i>"Custom HuggingFace GLiNER Model"</summary>
                            <div style="margin-top: 0.5rem;">
                                <p class="text-secondary" style="font-size: 0.8rem; margin-bottom: 0.5rem;">
                                    "Enter any GLiNER-compatible ONNX model ID from huggingface.co. Must contain onnx/model.onnx and tokenizer.json."
                                </p>
                                <div class="form-row">
                                    <label>"HuggingFace Model ID"</label>
                                    <input type="text" class="form-control" id="ner-custom-hf-id"
                                        placeholder="e.g. onnx-community/gliner_multi_pii-v1"
                                    />
                                </div>
                                <div class="button-group" style="margin-top: 0.5rem;">
                                    <button class="btn btn-primary" on:click={
                                        let api_dl = api_dl_ner.clone();
                                        let api_save = api_save_after_dl.clone();
                                        move |_| {
                                            let api_dl = api_dl.clone();
                                            let api_save = api_save.clone();
                                            wasm_bindgen_futures::spawn_local(async move {
                                                use wasm_bindgen::JsCast;
                                                let doc = web_sys::window().unwrap().document().unwrap();
                                                let input = doc.get_element_by_id("ner-custom-hf-id")
                                                    .and_then(|el| el.dyn_into::<web_sys::HtmlInputElement>().ok());
                                                let hf_id = input.map(|i| i.value()).unwrap_or_default();
                                                if hf_id.trim().is_empty() {
                                                    set_ner_download_status.set("Please enter a HuggingFace model ID.".into());
                                                    return;
                                                }
                                                let hf_id = hf_id.trim();
                                                let repo = if hf_id.contains('/') { hf_id.to_string() } else { format!("onnx-community/{}", hf_id) };
                                                // Extract model_id from repo (last part after /)
                                                let model_id = repo.split('/').last().unwrap_or(&repo).to_string();
                                                set_ner_download_status.set(format!("Downloading {}...", repo));
                                                let body = serde_json::json!({
                                                    "model_id": model_id,
                                                    "model_url": format!("https://huggingface.co/{}/resolve/main/onnx/model.onnx", repo),
                                                    "tokenizer_url": format!("https://huggingface.co/{}/resolve/main/tokenizer.json", repo),
                                                });
                                                match api_dl.post_text("/config/ner-download", &body).await {
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
                                                        // Auto-save config
                                                        let cfg_body = serde_json::json!({
                                                            "ner_provider": "anno",
                                                            "ner_model": model_id,
                                                        });
                                                        let _ = api_save.post_text("/config", &cfg_body).await;
                                                        set_status_msg.set(format!("Custom GLiNER model {} installed and NER config saved.", repo));
                                                    }
                                                    Err(e) => {
                                                        set_ner_download_status.set(String::new());
                                                        set_ner_model_status.set(format!("Download failed: {e}"));
                                                    }
                                                }
                                            });
                                        }
                                    }>
                                        <i class="fa-solid fa-cloud-arrow-down"></i>" Download from HuggingFace"
                                    </button>
                                </div>
                            </div>
                        </details>
                    </div>
                })
            }}
            // ── GLiREL Relation Extraction Model ──
            {
                let api_rel = api.clone();
                move || {
                let is_anno = ner_provider.get() == "anno";
                let api_rel_dl = api_rel.clone();
                let api_rel_check = api_rel.clone();
                let api_rel_custom = api_rel.clone();
                is_anno.then(|| view! {
                    <div class="card" style="margin: 0.75rem 0; padding: 0.75rem; background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.08);">
                        <h4 style="margin-top: 0;"><i class="fa-solid fa-link"></i>" GLiREL Relation Model"</h4>
                        <p class="text-secondary" style="font-size: 0.85rem; margin-bottom: 0.5rem;">
                            "Relation extraction model paired with GLiNER. Discovers relationships between entities."
                        </p>

                        // Quick install buttons for GLiREL models
                        <div style="margin: 0.5rem 0;">
                            <p class="text-secondary" style="font-size: 0.8rem; margin-bottom: 0.5rem;"><strong>"Quick Install"</strong></p>
                            {["small", "medium", "large"].into_iter().map(|size| {
                                let label = format!("GLiREL {} v2.1", match size { "small" => "Small", "medium" => "Medium", _ => "Large" });
                                let model_id = format!("glirel_{}-v2.1", size);
                                let repo = format!("onnx-community/glirel_{}-v2.1", size);
                                let api = api_rel_dl.clone();
                                let mid = model_id.clone();
                                let rp = repo.clone();
                                view! {
                                    <div style="display: flex; align-items: center; gap: 0.5rem; padding: 0.35rem 0; border-bottom: 1px solid rgba(255,255,255,0.05);">
                                        <div style="flex: 1; min-width: 0;">
                                            <code style="font-size: 0.85rem;">{label}</code>
                                        </div>
                                        <button class="btn btn-sm btn-primary" style="white-space: nowrap; padding: 0.2rem 0.5rem; font-size: 0.75rem;" on:click={
                                            let api = api.clone();
                                            let mid = mid.clone();
                                            let rp = rp.clone();
                                            move |_| {
                                                let api = api.clone();
                                                let mid = mid.clone();
                                                let rp = rp.clone();
                                                set_rel_download_status.set(format!("Downloading {}...", rp));
                                                wasm_bindgen_futures::spawn_local(async move {
                                                    let body = serde_json::json!({
                                                        "model_id": mid,
                                                        "model_url": format!("https://huggingface.co/{}/resolve/main/onnx/model.onnx", rp),
                                                        "tokenizer_url": format!("https://huggingface.co/{}/resolve/main/tokenizer.json", rp),
                                                    });
                                                    match api.post_text("/config/rel-download", &body).await {
                                                        Ok(r) => {
                                                            set_rel_download_status.set(String::new());
                                                            let msg = if let Ok(v) = serde_json::from_str::<serde_json::Value>(&r) {
                                                                let size = v.get("model_size_mb").and_then(|v| v.as_f64());
                                                                if let Some(mb) = size {
                                                                    format!("Model installed ({:.1} MB). Ready to use.", mb)
                                                                } else {
                                                                    "Model installed. Ready to use.".to_string()
                                                                }
                                                            } else {
                                                                "Installed.".to_string()
                                                            };
                                                            set_rel_model_status.set(msg);
                                                        }
                                                        Err(e) => {
                                                            set_rel_download_status.set(String::new());
                                                            set_rel_model_status.set(format!("Download failed: {e}"));
                                                        }
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

                        // Status display
                        {move || {
                            let st = rel_model_status.get();
                            if st.is_empty() {
                                None
                            } else if st.contains("installed") || st.contains("Installed") || st.contains("Ready") {
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

                        // Download progress
                        {move || {
                            let st = rel_download_status.get();
                            (!st.is_empty()).then(|| view! {
                                <div class="info-box" style="margin: 0.25rem 0; font-size: 0.8rem;">
                                    <i class="fa-solid fa-spinner fa-spin" style="margin-right: 0.25rem;"></i>{st}
                                </div>
                            })
                        }}

                        // Custom HuggingFace REL model
                        <details style="margin-top: 0.5rem;">
                            <summary class="text-secondary" style="cursor: pointer; font-size: 0.85rem;"><i class="fa-brands fa-github" style="margin-right: 0.25rem;"></i>"Custom HuggingFace GLiREL Model"</summary>
                            <div style="margin-top: 0.5rem;">
                                <div class="form-row">
                                    <label>"HuggingFace Model ID"</label>
                                    <input type="text" class="form-control" id="rel-custom-hf-id"
                                        placeholder="e.g. onnx-community/glirel_multi-v1"
                                    />
                                </div>
                                <div class="button-group" style="margin-top: 0.5rem;">
                                    <button class="btn btn-primary" on:click={
                                        let api = api_rel_custom.clone();
                                        move |_| {
                                            let api = api.clone();
                                            wasm_bindgen_futures::spawn_local(async move {
                                                use wasm_bindgen::JsCast;
                                                let doc = web_sys::window().unwrap().document().unwrap();
                                                let input = doc.get_element_by_id("rel-custom-hf-id")
                                                    .and_then(|el| el.dyn_into::<web_sys::HtmlInputElement>().ok());
                                                let hf_id = input.map(|i| i.value()).unwrap_or_default();
                                                if hf_id.trim().is_empty() {
                                                    set_rel_download_status.set("Please enter a HuggingFace model ID.".into());
                                                    return;
                                                }
                                                let hf_id = hf_id.trim();
                                                let repo = if hf_id.contains('/') { hf_id.to_string() } else { format!("onnx-community/{}", hf_id) };
                                                let model_id = repo.split('/').last().unwrap_or(&repo).to_string();
                                                set_rel_download_status.set(format!("Downloading {}...", repo));
                                                let body = serde_json::json!({
                                                    "model_id": model_id,
                                                    "model_url": format!("https://huggingface.co/{}/resolve/main/onnx/model.onnx", repo),
                                                    "tokenizer_url": format!("https://huggingface.co/{}/resolve/main/tokenizer.json", repo),
                                                });
                                                match api.post_text("/config/rel-download", &body).await {
                                                    Ok(r) => {
                                                        set_rel_download_status.set(String::new());
                                                        let msg = if let Ok(v) = serde_json::from_str::<serde_json::Value>(&r) {
                                                            let size = v.get("model_size_mb").and_then(|v| v.as_f64());
                                                            if let Some(mb) = size {
                                                                format!("Model installed ({:.1} MB). Ready to use.", mb)
                                                            } else {
                                                                "Model installed. Ready to use.".to_string()
                                                            }
                                                        } else {
                                                            "Installed.".to_string()
                                                        };
                                                        set_rel_model_status.set(msg);
                                                    }
                                                    Err(e) => {
                                                        set_rel_download_status.set(String::new());
                                                        set_rel_model_status.set(format!("Download failed: {e}"));
                                                    }
                                                }
                                            });
                                        }
                                    }>
                                        <i class="fa-solid fa-cloud-arrow-down"></i>" Download from HuggingFace"
                                    </button>
                                </div>
                            </div>
                        </details>
                    </div>
                })
            }}

            <div class="button-group" style="margin-top: 0.5rem;">
                <button class="btn btn-success" on:click=move |_| { save_ner.dispatch(()); }>
                    <i class="fa-solid fa-floppy-disk"></i>" Save NER Config"
                </button>
            </div>
        </CollapsibleSection>

        // ── 5. Quantization ──
        <CollapsibleSection title="Quantization" icon="fa-solid fa-compress" collapsed=true status=quant_status>
            <div class="form-row" style="display: flex; align-items: center; gap: 0.5rem;">
                <input
                    type="checkbox"
                    prop:checked=quant_enabled
                    on:change=move |ev| {
                        let checked = event_target_checked(&ev);
                        set_quant_enabled.set(checked);
                    }
                />
                <label style="margin: 0;">"Enable vector quantization (Int8)"</label>
            </div>
            <div class="info-box" style="margin-top: 0.5rem;">
                <i class="fa-solid fa-circle-info"></i>
                " Quantization reduces memory usage by compressing embedding vectors. Slight accuracy trade-off."
            </div>

            // Detailed status from compute info
            {move || {
                compute.get().flatten().map(|c| {
                    let dim = c.embedder_dim.unwrap_or(0);
                    let backend = c.gpu_backend.clone().unwrap_or_else(|| "CPU".into());
                    view! {
                        <table class="data-table" style="margin-top: 0.75rem;">
                            <tbody>
                                <tr>
                                    <td style="width: 50%; font-weight: 500;">"Int8 Quantization"</td>
                                    <td>{move || if quant_enabled.get() {
                                        view! { <span class="badge badge-core">"active"</span> }.into_any()
                                    } else {
                                        view! { <span class="badge badge-archival">"off"</span> }.into_any()
                                    }}</td>
                                </tr>
                                <tr>
                                    <td style="font-weight: 500;">"Vector Dimensions"</td>
                                    <td>{dim.to_string()}</td>
                                </tr>
                                <tr>
                                    <td style="font-weight: 500;">"Compute Backend"</td>
                                    <td>{backend}</td>
                                </tr>
                            </tbody>
                        </table>
                    }
                })
            }}

            // Vector count from stats
            {move || {
                stats.get().flatten().map(|s| {
                    view! {
                        <div class="info-box" style="margin-top: 0.5rem;">
                            <i class="fa-solid fa-chart-bar"></i>
                            " Nodes: "{s.nodes.to_string()}" | Edges: "{s.edges.to_string()}
                        </div>
                    }
                })
            }}

            <div class="button-group" style="margin-top: 0.5rem;">
                <button class="btn btn-success" on:click=move |_| { toggle_quantization.dispatch(()); }>
                    <i class="fa-solid fa-floppy-disk"></i>" Apply"
                </button>
            </div>
        </CollapsibleSection>

        // ── 6. Mesh ──
        <CollapsibleSection title="Mesh Network" icon="fa-solid fa-share-nodes" collapsed=true status=mesh_status>
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
        </CollapsibleSection>

        // ── 7. Secrets ──
        <CollapsibleSection title="Secrets" icon="fa-solid fa-key" collapsed=true status=secrets_status>
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
        </CollapsibleSection>

        // ── 8. Import / Export ──
        <CollapsibleSection title="Import / Export" icon="fa-solid fa-file-export" collapsed=true status=export_status>
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
        </CollapsibleSection>

        // ── 9. Database Management ──
        <DatabaseManagementSection api=api_for_db set_status_msg=set_status_msg />
    }
}

// ── Database Management section ──

#[component]
fn DatabaseManagementSection(
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
        <CollapsibleSection title="Database Management" icon="fa-solid fa-database" collapsed=true>
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
        </CollapsibleSection>
    }
}

fn event_target_checked(ev: &web_sys::Event) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|el| el.checked())
        .unwrap_or(false)
}
