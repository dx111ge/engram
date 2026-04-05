use super::*;
use super::presets::*;

/// Step 1: Welcome
pub(crate) fn render_step_welcome() -> AnyView {
    view! {
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
    }.into_any()
}

/// Step 2: Embedding Model
pub(crate) fn render_step_embedder(
    embed_choice: ReadSignal<String>,
    set_embed_choice: WriteSignal<String>,
    embed_key: ReadSignal<String>,
    set_embed_key: WriteSignal<String>,
    embed_model: ReadSignal<String>,
    set_embed_model: WriteSignal<String>,
    embed_endpoint: ReadSignal<String>,
    set_embed_endpoint: WriteSignal<String>,
    ollama_embed_models: ReadSignal<Vec<String>>,
    ollama_fetching: ReadSignal<bool>,
) -> AnyView {
    view! {
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
    }.into_any()
}

/// Step 3: NER
pub(crate) fn render_step_ner(
    ner_choice: ReadSignal<String>,
    set_ner_choice: WriteSignal<String>,
    ner_model: ReadSignal<String>,
    set_ner_model: WriteSignal<String>,
) -> AnyView {
    view! {
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
    }.into_any()
}

/// Step 4: Relation Extraction
pub(crate) fn render_step_rel(
    rel_threshold: ReadSignal<f64>,
    set_rel_threshold: WriteSignal<f64>,
    rel_templates_mode: ReadSignal<String>,
    set_rel_templates_mode: WriteSignal<String>,
    rel_custom_templates_json: ReadSignal<String>,
    set_rel_custom_templates_json: WriteSignal<String>,
) -> AnyView {
    view! {
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
    }.into_any()
}

/// Step 5: LLM
pub(crate) fn render_step_llm(
    llm_choice: ReadSignal<String>,
    set_llm_choice: WriteSignal<String>,
    llm_key: ReadSignal<String>,
    set_llm_key: WriteSignal<String>,
    llm_model: ReadSignal<String>,
    set_llm_model: WriteSignal<String>,
    ollama_llm_models: ReadSignal<Vec<String>>,
    ollama_fetching: ReadSignal<bool>,
) -> AnyView {
    view! {
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
            <div class="wizard-tip" style="font-size: 0.82rem; margin-top: 1rem; padding: 0.6rem 0.8rem; background: var(--bg-tertiary); border-radius: 6px; border-left: 3px solid var(--accent);">
                <p style="margin: 0 0 0.4rem 0;"><i class="fa-solid fa-circle-info"></i><strong>" Choosing a model"</strong></p>
                <ul style="margin: 0; padding-left: 1.1rem; line-height: 1.5; color: var(--text-secondary);">
                    <li>"For Ollama: "<strong>"14B+ parameters"</strong>" recommended (e.g. gemma4:e4b, qwen3:14b). Smaller models (7-8B) work but may produce unreliable JSON."</li>
                    <li><strong>"Thinking models"</strong>" (deepseek-r1, qwq, qwen3) give deeper analysis but use more tokens. engram toggles thinking per task automatically."</li>
                    <li>"Context window is auto-detected. For Ollama, run "<code>"ollama show &lt;model&gt;"</code>" to check. Larger context = better debate synthesis."</li>
                </ul>
            </div>
        </div>
    }.into_any()
}
