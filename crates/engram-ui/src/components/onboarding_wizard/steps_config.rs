use super::*;
use super::presets::*;

/// Step 6: Quantization
pub(crate) fn render_step_quantization(
    quant_choice: ReadSignal<String>,
    set_quant_choice: WriteSignal<String>,
) -> AnyView {
    view! {
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
    }.into_any()
}

/// Step 7: Knowledge Sources
pub(crate) fn render_step_kb_sources(
    kb_wikidata: ReadSignal<bool>,
    set_kb_wikidata: WriteSignal<bool>,
    kb_dbpedia: ReadSignal<bool>,
    set_kb_dbpedia: WriteSignal<bool>,
) -> AnyView {
    view! {
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
    }.into_any()
}

/// Step 8: Web Search
pub(crate) fn render_step_web_search(
    web_search_provider: ReadSignal<String>,
    set_web_search_provider: WriteSignal<String>,
    web_search_api_key: ReadSignal<String>,
    set_web_search_api_key: WriteSignal<String>,
    web_search_url: ReadSignal<String>,
    set_web_search_url: WriteSignal<String>,
    web_search_test_result: ReadSignal<Option<String>>,
    web_search_testing: ReadSignal<bool>,
    do_web_search_test: Action<(), ()>,
) -> AnyView {
    view! {
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
    }.into_any()
}

/// Step 9: Seed
pub(crate) fn render_step_seed(
    seed_text: ReadSignal<String>,
    set_seed_text: WriteSignal<String>,
    seed_phase: ReadSignal<u32>,
    set_seed_phase: WriteSignal<u32>,
    seed_aoi: ReadSignal<String>,
    set_seed_aoi: WriteSignal<String>,
    set_seed_session_id: WriteSignal<String>,
    seed_entities: ReadSignal<Vec<(String, String, f32, bool)>>,
    set_seed_entities: WriteSignal<Vec<(String, String, f32, bool)>>,
    new_entity_label: ReadSignal<String>,
    set_new_entity_label: WriteSignal<String>,
    new_entity_type: ReadSignal<String>,
    set_new_entity_type: WriteSignal<String>,
    analyzing: ReadSignal<bool>,
    seed_result: ReadSignal<Option<String>>,
    set_seed_result: WriteSignal<Option<String>>,
    do_analyze: Action<(), ()>,
    do_ingest: Action<(), ()>,
    set_step: WriteSignal<u32>,
) -> AnyView {
    view! {
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
    }.into_any()
}

/// Step 10: Ready
pub(crate) fn render_step_ready(
    embed_choice: ReadSignal<String>,
    ner_choice: ReadSignal<String>,
    llm_choice: ReadSignal<String>,
    quant_choice: ReadSignal<String>,
    kb_wikidata: ReadSignal<bool>,
    kb_dbpedia: ReadSignal<bool>,
    do_complete: Action<(), ()>,
) -> AnyView {
    view! {
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
    }.into_any()
}
