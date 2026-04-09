use leptos::prelude::*;

use crate::api::ApiClient;
use super::presets::NER_PRESETS;

/// Component: learned relation types as clickable badge chips.
#[component]
fn LearnedTypesChips(
    api: ApiClient,
    relation_templates_json: ReadSignal<String>,
    set_relation_templates_json: WriteSignal<String>,
) -> impl IntoView {
    let (learned_types, set_learned_types) = signal(Vec::<String>::new());

    // Fetch learned types on mount
    let api_fetch = api.clone();
    Effect::new(move || {
        let api = api_fetch.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(text) = api.get_text("/config/relation-templates/export").await {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    let types: Vec<String> = json.get("learned_relation_types")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                        .unwrap_or_default();
                    set_learned_types.set(types);
                }
            }
        });
    });

    view! {
        {move || {
            let types = learned_types.get();
            (!types.is_empty()).then(|| view! {
                <div style="margin-top: 0.75rem;">
                    <label style="font-size: 0.85rem;"><i class="fa-solid fa-graduation-cap"></i>" Learned from Graph"</label>
                    <p style="font-size: 0.75rem; color: rgba(255,255,255,0.4); margin: 2px 0 6px;">
                        "Click to add to your custom templates."
                    </p>
                    <div style="display: flex; flex-wrap: wrap; gap: 6px;">
                        {types.into_iter().map(|t| {
                            let t_click = t.clone();
                            let t_display = t.clone();
                            view! {
                                <span
                                    style="display: inline-flex; align-items: center; gap: 4px; padding: 3px 10px; background: rgba(79,195,247,0.1); border: 1px solid rgba(79,195,247,0.25); border-radius: 12px; font-size: 0.75rem; color: #4fc3f7; cursor: pointer;"
                                    title="Click to add to custom templates"
                                    on:click=move |_| {
                                        let cur = relation_templates_json.get_untracked();
                                        let mut obj: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&cur).unwrap_or_default();
                                        if !obj.contains_key(&t_click) {
                                            let template = format!("{{head}} {} {{tail}}", t_click);
                                            obj.insert(t_click.clone(), serde_json::json!(template));
                                            set_relation_templates_json.set(serde_json::to_string_pretty(&obj).unwrap_or_default());
                                        }
                                    }
                                >
                                    <i class="fa-solid fa-plus" style="font-size: 0.6rem;"></i>
                                    {t_display}
                                </span>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                </div>
            })
        }}
    }
}

pub(crate) fn render_ner_modal(
    api: ApiClient,
    ner_provider: ReadSignal<String>,
    set_ner_provider: WriteSignal<String>,
    ner_model: ReadSignal<String>,
    set_ner_model: WriteSignal<String>,
    ner_selected_model: ReadSignal<String>,
    set_ner_selected_model: WriteSignal<String>,
    ner_model_status: ReadSignal<String>,
    set_ner_model_status: WriteSignal<String>,
    ner_download_status: ReadSignal<String>,
    set_ner_download_status: WriteSignal<String>,
    coref_enabled: ReadSignal<bool>,
    rel_threshold: ReadSignal<f64>,
    set_rel_threshold: WriteSignal<f64>,
    rel_templates_mode: ReadSignal<String>,
    set_rel_templates_mode: WriteSignal<String>,
    relation_templates_json: ReadSignal<String>,
    set_relation_templates_json: WriteSignal<String>,
    ner_endpoint: ReadSignal<String>,
    set_status_msg: WriteSignal<String>,
    set_modal_open: WriteSignal<String>,
) -> impl IntoView {
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

    let api_for_ner = api.clone();

    view! {
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
        {
            let api_rel_section = api.clone();
            move || {
            let is_gliner = ner_provider.get() == "gliner";
            let api_sec = api_rel_section.clone();
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
                        (rel_templates_mode.get() == "custom").then(|| {
                            // Parse JSON into table rows
                            let json_str = relation_templates_json.get();
                            let rows: Vec<(String, String)> = serde_json::from_str::<serde_json::Value>(&json_str)
                                .ok()
                                .and_then(|v| v.as_object().map(|o| {
                                    o.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect()
                                }))
                                .unwrap_or_default();

                            view! {
                            <div class="form-group" style="margin-top: 0.75rem;">
                                <label><i class="fa-solid fa-table"></i>" Relation Types"</label>
                                <table style="width: 100%; border-collapse: collapse; font-size: 0.85rem; margin-top: 0.5rem;">
                                    <thead>
                                        <tr style="border-bottom: 1px solid rgba(255,255,255,0.15);">
                                            <th style="text-align: left; padding: 6px;">"Relation Type"</th>
                                            <th style="text-align: left; padding: 6px;">"Description Template"</th>
                                            <th style="width: 40px;"></th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {rows.into_iter().map(|(rel_type, desc)| {
                                            let rt_for_rename = rel_type.clone();
                                            let rt_for_desc = rel_type.clone();
                                            let rt_for_delete = rel_type.clone();
                                            let desc_display = desc.clone();
                                            view! {
                                                <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                                                    <td style="padding: 4px 6px;">
                                                        <input type="text" class="form-control"
                                                            style="font-size:0.8rem; padding:2px 6px; background:rgba(255,255,255,0.05);"
                                                            prop:value=rt_for_rename.clone()
                                                            on:change=move |ev| {
                                                                let val = event_target_value(&ev);
                                                                let cur = relation_templates_json.get_untracked();
                                                                if let Ok(mut obj) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&cur) {
                                                                    if let Some(old_val) = obj.remove(&rt_for_rename) {
                                                                        let new_key = val.trim().to_lowercase().replace(' ', "_");
                                                                        if !new_key.is_empty() { obj.insert(new_key, old_val); }
                                                                    }
                                                                    set_relation_templates_json.set(serde_json::to_string_pretty(&obj).unwrap_or_default());
                                                                }
                                                            }
                                                        />
                                                    </td>
                                                    <td style="padding: 4px 6px;">
                                                        <input type="text" class="form-control"
                                                            style="font-size:0.8rem; padding:2px 6px; background:rgba(255,255,255,0.05);"
                                                            prop:value=desc_display
                                                            on:change=move |ev| {
                                                                let val = event_target_value(&ev);
                                                                let cur = relation_templates_json.get_untracked();
                                                                if let Ok(mut obj) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&cur) {
                                                                    obj.insert(rt_for_desc.clone(), serde_json::json!(val));
                                                                    set_relation_templates_json.set(serde_json::to_string_pretty(&obj).unwrap_or_default());
                                                                }
                                                            }
                                                        />
                                                    </td>
                                                    <td style="padding: 4px; text-align: center;">
                                                        <button class="btn btn-xs" style="color:rgba(239,83,80,0.7); background:none; border:none; cursor:pointer;"
                                                            on:click=move |_| {
                                                                let cur = relation_templates_json.get_untracked();
                                                                if let Ok(mut obj) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&cur) {
                                                                    obj.remove(&rt_for_delete);
                                                                    set_relation_templates_json.set(serde_json::to_string_pretty(&obj).unwrap_or_default());
                                                                }
                                                            }
                                                        >
                                                            <i class="fa-solid fa-trash" style="font-size:0.7rem;"></i>
                                                        </button>
                                                    </td>
                                                </tr>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </tbody>
                                </table>
                                <button class="btn btn-sm btn-secondary" style="margin-top: 0.5rem;"
                                    on:click=move |_| {
                                        let cur = relation_templates_json.get_untracked();
                                        let mut obj: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&cur).unwrap_or_default();
                                        obj.insert("new_relation".to_string(), serde_json::json!("{head} new_relation {tail}"));
                                        set_relation_templates_json.set(serde_json::to_string_pretty(&obj).unwrap_or_default());
                                    }
                                >
                                    <i class="fa-solid fa-plus"></i>" Add Relation"
                                </button>

                                // E2: Learned relation types (from graph gazetteer)
                                // Note: Learned types are fetched inline from the exported JSON.
                                // The rows variable already parsed the current templates. Learned
                                // types show from the /export endpoint but that requires an async
                                // fetch. We render a placeholder that fetches on mount.
                                <LearnedTypesChips
                                    api=api_sec.clone()
                                    set_relation_templates_json=set_relation_templates_json
                                    relation_templates_json=relation_templates_json
                                />

                                <div class="wizard-info-box" style="margin-top: 0.5rem; font-size: 0.8rem;">
                                    <i class="fa-solid fa-circle-info" style="margin-right: 0.25rem;"></i>
                                    " GLiNER2 uses the relation name as a zero-shot label. Custom types are merged with defaults."
                                </div>
                            </div>
                        }})
                    }}
                </div>
            })
        }}

        // ── Entity Categories section ──
        {
            let api_labels = api.clone();
            move || {
            let is_gliner = ner_provider.get() == "gliner";
            let api_lbl = api_labels.clone();
            is_gliner.then(|| {
                let (core_labels, set_core_labels) = signal(Vec::<String>::new());
                let (user_labels, set_user_labels) = signal(Vec::<String>::new());
                let (auto_labels, set_auto_labels) = signal(Vec::<(String, usize)>::new());
                let (effective_count, set_effective_count) = signal(0usize);
                let (new_label_input, set_new_label_input) = signal(String::new());
                let (auto_threshold, set_auto_threshold) = signal(3u32);

                // Fetch entity labels on mount
                let api_fetch = api_lbl.clone();
                Effect::new(move || {
                    let api = api_fetch.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        if let Ok(text) = api.get_text("/config/entity-labels").await {
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                                let core: Vec<String> = json.get("core").and_then(|v| v.as_array())
                                    .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                                    .unwrap_or_default();
                                set_core_labels.set(core);

                                let user: Vec<String> = json.get("user_defined").and_then(|v| v.as_array())
                                    .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                                    .unwrap_or_default();
                                set_user_labels.set(user);

                                let auto: Vec<(String, usize)> = json.get("auto_discovered").and_then(|v| v.as_array())
                                    .map(|a| a.iter().filter_map(|v| {
                                        let label = v.get("label")?.as_str()?.to_string();
                                        let count = v.get("count")?.as_u64()? as usize;
                                        Some((label, count))
                                    }).collect())
                                    .unwrap_or_default();
                                set_auto_labels.set(auto);

                                let eff = json.get("effective").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
                                set_effective_count.set(eff);

                                let thresh = json.get("auto_threshold").and_then(|v| v.as_u64()).unwrap_or(3) as u32;
                                set_auto_threshold.set(thresh);
                            }
                        }
                    });
                });

                let api_save = api_lbl.clone();
                let save_labels = Action::new_local(move |_: &()| {
                    let api = api_save.clone();
                    let labels = user_labels.get_untracked();
                    let thresh = auto_threshold.get_untracked();
                    async move {
                        let body = serde_json::json!({
                            "labels": labels,
                            "auto_threshold": thresh,
                        });
                        match api.post_text("/config/entity-labels", &body).await {
                            Ok(_) => set_status_msg.set("Entity labels saved.".to_string()),
                            Err(e) => set_status_msg.set(format!("Error: {e}")),
                        }
                    }
                });

                view! {
                    <div style="margin-top: 1rem;">
                        <h4><i class="fa-solid fa-tags"></i>" Entity Categories"</h4>
                        <p class="wizard-desc">"GLiNER2 detects entities using these labels. Core types are always active. Add domain-specific categories or let engram auto-discover them from your graph."</p>

                        // Budget indicator
                        <div style="margin: 0.5rem 0; font-size: 0.8rem; color: var(--text-secondary);">
                            <i class="fa-solid fa-gauge"></i>
                            {move || format!(" Using {} / 25 label slots", effective_count.get())}
                        </div>

                        // Core labels (non-removable)
                        <div style="margin-bottom: 0.5rem;">
                            <label style="font-size: 0.8rem;"><i class="fa-solid fa-lock" style="opacity: 0.5;"></i>" Core"</label>
                            <div style="display: flex; flex-wrap: wrap; gap: 4px; margin-top: 4px;">
                                {move || core_labels.get().iter().map(|l| {
                                    let l = l.clone();
                                    view! { <span class="badge" style="font-size: 0.75rem; opacity: 0.7;">{l}</span> }
                                }).collect::<Vec<_>>()}
                            </div>
                        </div>

                        // User-defined labels (editable)
                        <div style="margin-bottom: 0.5rem;">
                            <label style="font-size: 0.8rem;"><i class="fa-solid fa-user-pen"></i>" Custom"</label>
                            <div style="display: flex; flex-wrap: wrap; gap: 4px; margin-top: 4px;">
                                {move || user_labels.get().iter().map(|l| {
                                    let l2 = l.clone();
                                    let l3 = l.clone();
                                    view! {
                                        <span class="badge badge-active" style="font-size: 0.75rem;">
                                            {l2}
                                            <button style="background: none; border: none; color: inherit; cursor: pointer; padding: 0 0 0 4px; font-size: 0.65rem;"
                                                on:click=move |_| {
                                                    set_user_labels.update(|labels| labels.retain(|x| x != &l3));
                                                }
                                            ><i class="fa-solid fa-xmark"></i></button>
                                        </span>
                                    }
                                }).collect::<Vec<_>>()}
                                <div style="display: flex; gap: 4px; align-items: center;">
                                    <input type="text" placeholder="new category..."
                                        style="font-size: 0.8rem; width: 140px; padding: 2px 6px;"
                                        prop:value=new_label_input
                                        on:input=move |ev| set_new_label_input.set(event_target_value(&ev))
                                        on:keydown=move |ev| {
                                            if ev.key() == "Enter" {
                                                let v = new_label_input.get_untracked().trim().to_lowercase().replace(' ', "_");
                                                if !v.is_empty() {
                                                    set_user_labels.update(|labels| {
                                                        if !labels.contains(&v) { labels.push(v); }
                                                    });
                                                    set_new_label_input.set(String::new());
                                                }
                                            }
                                        }
                                    />
                                </div>
                            </div>
                        </div>

                        // Auto-discovered labels
                        {move || {
                            let auto = auto_labels.get();
                            (!auto.is_empty()).then(|| view! {
                                <div style="margin-bottom: 0.5rem;">
                                    <label style="font-size: 0.8rem;"><i class="fa-solid fa-graduation-cap"></i>" Auto-discovered"</label>
                                    <div style="display: flex; flex-wrap: wrap; gap: 4px; margin-top: 4px;">
                                        {auto.iter().map(|(l, count)| {
                                            let l2 = l.clone();
                                            let l3 = l.clone();
                                            view! {
                                                <span class="badge" style="font-size: 0.75rem; background: var(--bg-secondary); cursor: pointer;"
                                                    title="Click to pin as custom label"
                                                    on:click=move |_| {
                                                        set_user_labels.update(|labels| {
                                                            if !labels.contains(&l3) { labels.push(l3.clone()); }
                                                        });
                                                    }
                                                >
                                                    {format!("{} ({})", l2, count)}
                                                </span>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </div>
                                </div>
                            })
                        }}

                        // Threshold
                        <div style="display: flex; align-items: center; gap: 0.5rem; margin-top: 0.5rem; font-size: 0.8rem;">
                            <label>"Auto-discovery threshold: "</label>
                            <input type="number" min="0" max="50" style="width: 50px; font-size: 0.8rem; padding: 2px 4px;"
                                prop:value=move || auto_threshold.get().to_string()
                                on:input=move |ev| {
                                    if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                        set_auto_threshold.set(v);
                                    }
                                }
                            />
                            <span style="color: var(--text-secondary);">" nodes needed"</span>
                        </div>

                        // Save button
                        <button class="btn btn-sm btn-primary" style="margin-top: 0.5rem;"
                            on:click=move |_| { let _ = save_labels.dispatch(()); }
                        >
                            <i class="fa-solid fa-floppy-disk"></i>" Save Entity Labels"
                        </button>
                    </div>
                }
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
    }
}
