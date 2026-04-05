// ── Provider presets (matching wizard) ──

pub(crate) struct EmbedPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub endpoint: &'static str,
    pub needs_key: bool,
    pub quality: &'static str,
    pub performance: &'static str,
    pub privacy: &'static str,
    pub cost: &'static str,
    pub models: &'static [(&'static str, &'static str, &'static str)],  // (model_name, description, lang_badge)
    pub default_model: &'static str,
}

pub(crate) const EMBED_PRESETS: &[EmbedPreset] = &[
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

pub(crate) struct OnnxQuickModel {
    pub name: &'static str,
    pub desc: &'static str,
    pub model_url: &'static str,
    pub tokenizer_url: &'static str,
}

pub(crate) const ONNX_QUICK_MODELS: &[OnnxQuickModel] = &[
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

pub(crate) struct NerPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub quality: &'static str,
    pub speed: &'static str,
    pub download: &'static str,
    pub license: &'static str,
    pub learning: &'static str,
    pub models: &'static [(&'static str, &'static str, &'static str, &'static str, &'static str)], // (id, name, desc, hf_repo, lang)
}

pub(crate) const NER_PRESETS: &[NerPreset] = &[
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

pub(crate) struct LlmPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub endpoint: &'static str,
    pub needs_key: bool,
    pub quality: &'static str,
    pub privacy: &'static str,
    pub cost: &'static str,
    pub models: &'static [(&'static str, &'static str)],
    pub default_model: &'static str,
    pub can_fetch_models: bool,
}

pub(crate) const LLM_PRESETS: &[LlmPreset] = &[
    LlmPreset {
        id: "ollama", name: "Ollama (Recommended)", endpoint: "http://localhost:11434/v1/chat/completions",
        needs_key: false, quality: "Good (local models)", privacy: "Local", cost: "Free",
        models: &[
            ("llama3.2", "3B, fast but limited JSON quality"),
            ("phi4", "14B, excellent reasoning, recommended"),
            ("mistral", "7B, balanced speed/quality"),
            ("gemma3", "4B, efficient, thinks by default"),
            ("qwen3", "8B, multilingual, thinking model"),
            ("gemma4:e4b", "47B, high quality, thinks by default"),
        ],
        default_model: "phi4",
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

pub(crate) const THINKING_MODELS: &[&str] = &["deepseek-r1", "deepseek-reasoner", "qwq", "qwen3", "o3-mini", "gemma4"];

/// Parse ONNX status JSON into human-readable string
pub(crate) fn parse_onnx_status(text: &str) -> String {
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
pub(crate) fn parse_ner_model_status(text: &str) -> String {
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

pub(crate) fn event_target_checked(ev: &web_sys::Event) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|el| el.checked())
        .unwrap_or(false)
}
