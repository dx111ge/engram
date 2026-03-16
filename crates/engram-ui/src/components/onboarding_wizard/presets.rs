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
}

pub(crate) const LLM_PRESETS: &[LlmPreset] = &[
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

pub(crate) const SEED_EXAMPLES: &[(&str, &str)] = &[
    ("Geopolitics & Security",
     "I'm a security analyst focused on European cybersecurity and critical infrastructure protection. Key areas include Germany's BSI, France's ANSSI, and NATO's CCDCOE in Tallinn. I track state-sponsored threat actors like APT28 and APT29, and monitor EU regulations including NIS2 and the Cyber Resilience Act. The energy sector, particularly Nord Stream infrastructure and European power grids, is a priority."),
    ("Technology & AI",
     "I'm researching artificial intelligence companies and their key products. Major players include OpenAI with GPT-4, Google DeepMind with Gemini and AlphaFold, Anthropic with Claude, and Meta AI with LLaMA. I'm interested in the researchers behind these systems, their academic backgrounds at Stanford, MIT, and Oxford, and the venture capital firms like Sequoia and Andreessen Horowitz funding this space."),
    ("History & Geography",
     "I study the history and geography of Central Europe, particularly the Holy Roman Empire and its successor states. Key figures include Charlemagne, Frederick the Great, Maria Theresa, and Otto von Bismarck. Important cities include Vienna, Prague, Berlin, and Munich. I'm interested in the rivers Danube, Rhine, and Elbe, and how they shaped trade routes and political boundaries."),
];
