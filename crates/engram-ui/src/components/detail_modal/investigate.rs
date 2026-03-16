use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{
    NodeResponse, IngestRequest, IngestItem, IngestResponse,
};

/// A web search result item for the investigation tab
#[derive(Clone, Debug)]
struct WebResult {
    title: String,
    snippet: String,
    selected: bool,
}

/// An entity discovered during investigation
#[derive(Clone, Debug)]
struct DiscoveredEntity {
    label: String,
    entity_type: String,
    confidence: f32,
    source: String, // "web" or "kb" or "ner"
    selected: bool,
}

/// A relation discovered during investigation
#[derive(Clone, Debug)]
struct DiscoveredRelation {
    from: String,
    to: String,
    rel_type: String,
    confidence: f32,
    source: String,
    selected: bool,
}

pub(super) fn render_investigate_tab(detail: NodeResponse, api: ApiClient) -> leptos::prelude::AnyView {
    let entity_label = detail.label.clone();
    // Use canonical_name if available for better search results
    let search_query = detail.properties.as_ref()
        .and_then(|p| p.get("canonical_name"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim_matches('"').to_string())
        .unwrap_or_else(|| {
            let ntype = detail.node_type.as_deref().unwrap_or("");
            if ntype.is_empty() || ntype == "Entity" {
                entity_label.clone()
            } else {
                format!("{} {}", entity_label, ntype)
            }
        });

    // Step state: 1=Gather, 2=Review, 3=Commit
    let (step, set_step) = signal(1u32);
    let (gathering, set_gathering) = signal(false);
    let (committing, set_committing) = signal(false);

    // Gathered data
    let (web_results, set_web_results) = signal(Vec::<WebResult>::new());
    let (discovered_entities, set_discovered_entities) = signal(Vec::<DiscoveredEntity>::new());
    let (discovered_relations, set_discovered_relations) = signal(Vec::<DiscoveredRelation>::new());
    let (gather_status, set_gather_status) = signal(String::new());
    let (commit_result, set_commit_result) = signal(Option::<String>::None);

    // Step 1: Gather action
    let api_gather = api.clone();
    let search_q = search_query.clone();
    let entity_for_gather = entity_label.clone();
    let do_gather = Action::new_local(move |_: &()| {
        let api = api_gather.clone();
        let query = search_q.clone();
        let entity = entity_for_gather.clone();
        async move {
            set_gathering.set(true);
            set_gather_status.set("Searching the web...".into());
            set_web_results.set(Vec::new());
            set_discovered_entities.set(Vec::new());
            set_discovered_relations.set(Vec::new());

            // 1. Web search
            let encoded = js_sys::encode_uri_component(&query);
            let mut web_hits = Vec::new();
            if let Ok(val) = api.get::<serde_json::Value>(&format!("/proxy/search?q={encoded}")).await {
                if let Some(arr) = val.get("results").and_then(|r| r.as_array()) {
                    for item in arr.iter().take(8) {
                        let title = item.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string();
                        let snippet = item.get("snippet").and_then(|s| s.as_str())
                            .or_else(|| item.get("description").and_then(|d| d.as_str()))
                            .unwrap_or("").to_string();
                        if !title.is_empty() {
                            web_hits.push(WebResult { title, snippet, selected: true });
                        }
                    }
                }
            }
            set_web_results.set(web_hits.clone());

            // 2. Run NER analysis on the selected web snippets to extract entities+relations
            set_gather_status.set("Analyzing results for entities and relations...".into());
            let combined_text = web_hits.iter()
                .filter(|w| w.selected)
                .map(|w| format!("{}: {}", w.title, w.snippet))
                .collect::<Vec<_>>()
                .join("\n\n");

            if !combined_text.is_empty() {
                let analyze_body = serde_json::json!({ "text": combined_text });
                if let Ok(resp) = api.post::<_, serde_json::Value>("/ingest/analyze", &analyze_body).await {
                    let mut ents = Vec::new();
                    if let Some(arr) = resp.get("entities").and_then(|e| e.as_array()) {
                        for e in arr {
                            let label = e.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string();
                            let etype = e.get("entity_type").and_then(|t| t.as_str()).unwrap_or("entity").to_string();
                            let conf = e.get("confidence").and_then(|c| c.as_f64()).unwrap_or(0.5) as f32;
                            if !label.is_empty() && label.to_lowercase() != entity.to_lowercase() {
                                // Deduplicate
                                if !ents.iter().any(|x: &DiscoveredEntity| x.label.to_lowercase() == label.to_lowercase()) {
                                    ents.push(DiscoveredEntity {
                                        label, entity_type: etype, confidence: conf,
                                        source: "web+NER".into(), selected: true,
                                    });
                                }
                            }
                        }
                    }
                    set_discovered_entities.set(ents);

                    let mut rels = Vec::new();
                    if let Some(arr) = resp.get("relations").and_then(|r| r.as_array()) {
                        for r in arr {
                            let from = r.get("from").and_then(|f| f.as_str()).unwrap_or("").to_string();
                            let to = r.get("to").and_then(|t| t.as_str()).unwrap_or("").to_string();
                            let rel_type = r.get("rel_type").and_then(|t| t.as_str()).unwrap_or("related_to").to_string();
                            let conf = r.get("confidence").and_then(|c| c.as_f64()).unwrap_or(0.5) as f32;
                            if !from.is_empty() && !to.is_empty() {
                                rels.push(DiscoveredRelation {
                                    from, to, rel_type, confidence: conf,
                                    source: "web+NER".into(), selected: true,
                                });
                            }
                        }
                    }
                    set_discovered_relations.set(rels);
                }
            }

            set_gather_status.set(String::new());
            set_gathering.set(false);
            set_step.set(2);
        }
    });

    // Step 3: Commit action -- ingest selected web content through the full pipeline
    let api_commit = api.clone();
    let entity_for_commit = entity_label.clone();
    let do_commit = Action::new_local(move |_: &()| {
        let api = api_commit.clone();
        let entity = entity_for_commit.clone();
        async move {
            set_committing.set(true);
            set_commit_result.set(None);

            // Build content from selected web results
            let webs = web_results.get_untracked();
            let selected_text: Vec<String> = webs.iter()
                .filter(|w| w.selected)
                .map(|w| format!("{}: {}", w.title, w.snippet))
                .collect();

            if selected_text.is_empty() {
                set_commit_result.set(Some("No web results selected.".into()));
                set_committing.set(false);
                return;
            }

            // Ingest through the full NER+RE+KB pipeline
            let content = format!("About {}:\n{}", entity, selected_text.join("\n\n"));
            let body = IngestRequest {
                items: vec![IngestItem { content, source_url: None }],
                source: Some("investigate".into()),
                skip: None,
            };
            match api.post::<_, IngestResponse>("/ingest", &body).await {
                Ok(resp) => {
                    set_commit_result.set(Some(format!(
                        "Ingested: {} facts, {} relations ({}ms)",
                        resp.facts_stored, resp.relations_created, resp.duration_ms,
                    )));
                    set_step.set(3);
                }
                Err(e) => {
                    set_commit_result.set(Some(format!("Error: {e}")));
                }
            }
            set_committing.set(false);
        }
    });

    view! {
        <div>
            // Step indicator
            <div style="display: flex; align-items: center; gap: 0.5rem; margin-bottom: 1.25rem;">
                <span class={move || if step.get() >= 1 { "badge badge-core" } else { "badge badge-archival" }}
                    style="font-size: 0.75rem;">"1. Gather"</span>
                <i class="fa-solid fa-chevron-right" style="font-size: 0.6rem; opacity: 0.4;"></i>
                <span class={move || if step.get() >= 2 { "badge badge-core" } else { "badge badge-archival" }}
                    style="font-size: 0.75rem;">"2. Review"</span>
                <i class="fa-solid fa-chevron-right" style="font-size: 0.6rem; opacity: 0.4;"></i>
                <span class={move || if step.get() >= 3 { "badge badge-core" } else { "badge badge-archival" }}
                    style="font-size: 0.75rem;">"3. Commit"</span>
            </div>

            // Step 1: Gather
            {move || {
                if step.get() != 1 { return None; }
                let sq = search_query.clone();
                Some(view! {
                    <div>
                        <h4><i class="fa-solid fa-magnifying-glass-chart" style="margin-right: 0.5rem;"></i>"Gather Information"</h4>
                        <p class="text-secondary" style="font-size: 0.85rem; margin-bottom: 1rem;">
                            "Search the web and knowledge bases for new information about this entity. Results will be analyzed for entities and relations."
                        </p>
                        <div class="info-box" style="margin-bottom: 1rem;">
                            <i class="fa-solid fa-search" style="margin-right: 0.25rem;"></i>
                            " Search query: "<strong>{sq}</strong>
                        </div>
                        <button class="btn btn-primary" disabled=gathering
                            on:click=move |_| { do_gather.dispatch(()); }>
                            {move || if gathering.get() {
                                view! { <><span class="spinner"></span>" Gathering..."</> }.into_any()
                            } else {
                                view! { <><i class="fa-solid fa-play"></i>" Start Investigation"</> }.into_any()
                            }}
                        </button>
                        {move || {
                            let st = gather_status.get();
                            (!st.is_empty()).then(|| view! {
                                <div class="info-box" style="margin-top: 0.75rem;">
                                    <i class="fa-solid fa-spinner fa-spin" style="margin-right: 0.25rem;"></i>{st}
                                </div>
                            })
                        }}
                    </div>
                })
            }}

            // Step 2: Review
            {move || {
                if step.get() != 2 { return None; }
                let webs = web_results.get();
                let ents = discovered_entities.get();
                let rels = discovered_relations.get();
                let selected_web_count = webs.iter().filter(|w| w.selected).count();
                let selected_ent_count = ents.iter().filter(|e| e.selected).count();
                let selected_rel_count = rels.iter().filter(|r| r.selected).count();

                Some(view! {
                    <div>
                        <h4><i class="fa-solid fa-list-check" style="margin-right: 0.5rem;"></i>"Review Findings"</h4>
                        <p class="text-secondary" style="font-size: 0.85rem; margin-bottom: 1rem;">
                            "Select which web results to ingest. The full NER + Relation Extraction pipeline will process the selected content."
                        </p>

                        // Web results
                        {if webs.is_empty() {
                            view! { <p class="text-secondary">"No web results found."</p> }.into_any()
                        } else {
                            view! {
                                <div style="margin-bottom: 1rem;">
                                    <h4 style="font-size: 0.8rem; color: rgba(255,255,255,0.5); text-transform: uppercase; margin-bottom: 0.5rem;">
                                        <i class="fa-solid fa-globe" style="margin-right: 0.25rem;"></i>
                                        "Web Results ("{selected_web_count.to_string()}" / "{webs.len().to_string()}" selected)"
                                    </h4>
                                    {webs.iter().enumerate().map(|(idx, w)| {
                                        let title = w.title.clone();
                                        let snippet = w.snippet.clone();
                                        let is_selected = w.selected;
                                        view! {
                                            <div style="display: flex; gap: 0.5rem; padding: 0.5rem; border-bottom: 1px solid rgba(255,255,255,0.05); align-items: flex-start;">
                                                <input type="checkbox" prop:checked=is_selected
                                                    style="margin-top: 0.2rem; flex-shrink: 0;"
                                                    on:change=move |ev| {
                                                        let checked = {
                                                            use wasm_bindgen::JsCast;
                                                            ev.target()
                                                                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                                                                .map(|el| el.checked())
                                                                .unwrap_or(false)
                                                        };
                                                        set_web_results.update(|v| {
                                                            if let Some(item) = v.get_mut(idx) {
                                                                item.selected = checked;
                                                            }
                                                        });
                                                    }
                                                />
                                                <div style="flex: 1; min-width: 0;">
                                                    <strong style="font-size: 0.85rem;">{title}</strong>
                                                    <p style="font-size: 0.75rem; color: rgba(255,255,255,0.5); margin: 2px 0 0; line-height: 1.3;">{snippet}</p>
                                                </div>
                                            </div>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            }.into_any()
                        }}

                        // Discovered entities preview
                        {(!ents.is_empty()).then(|| view! {
                            <div style="margin-bottom: 1rem;">
                                <h4 style="font-size: 0.8rem; color: rgba(255,255,255,0.5); text-transform: uppercase; margin-bottom: 0.5rem;">
                                    <i class="fa-solid fa-tags" style="margin-right: 0.25rem;"></i>
                                    "Discovered Entities ("{selected_ent_count.to_string()}")"
                                </h4>
                                <div class="filter-chips">
                                    {ents.iter().enumerate().map(|(idx, e)| {
                                        let label = e.label.clone();
                                        let etype = e.entity_type.clone();
                                        let etype2 = etype.clone();
                                        let source = e.source.clone();
                                        let is_sel = e.selected;
                                        view! {
                                            <button
                                                class=move || if is_sel { "filter-chip filter-chip-active" } else { "filter-chip filter-chip-hidden" }
                                                on:click=move |_| {
                                                    set_discovered_entities.update(|v| {
                                                        if let Some(item) = v.get_mut(idx) {
                                                            item.selected = !item.selected;
                                                        }
                                                    });
                                                }
                                                title=format!("{} ({})", etype, source)
                                            >
                                                {label} <span class="chip-count">{etype2}</span>
                                            </button>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            </div>
                        })}

                        // Discovered relations preview
                        {(!rels.is_empty()).then(|| view! {
                            <div style="margin-bottom: 1rem;">
                                <h4 style="font-size: 0.8rem; color: rgba(255,255,255,0.5); text-transform: uppercase; margin-bottom: 0.5rem;">
                                    <i class="fa-solid fa-link" style="margin-right: 0.25rem;"></i>
                                    "Discovered Relations ("{selected_rel_count.to_string()}")"
                                </h4>
                                {rels.iter().map(|r| {
                                    view! {
                                        <div style="font-size: 0.8rem; padding: 0.2rem 0; color: rgba(255,255,255,0.7);">
                                            <strong>{r.from.clone()}</strong>
                                            " " <span style="color: var(--accent-bright);">{r.rel_type.clone()}</span> " "
                                            <strong>{r.to.clone()}</strong>
                                            <span class="text-secondary" style="font-size: 0.7rem; margin-left: 0.5rem;">{format!("{:.0}%", r.confidence * 100.0)}</span>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        })}

                        // Action buttons
                        <div style="display: flex; gap: 0.5rem; margin-top: 1rem;">
                            <button class="btn btn-secondary" on:click=move |_| set_step.set(1)>
                                <i class="fa-solid fa-arrow-left"></i>" Back"
                            </button>
                            <button class="btn btn-primary" disabled=committing
                                on:click=move |_| { do_commit.dispatch(()); }>
                                {move || if committing.get() {
                                    view! { <><span class="spinner"></span>" Ingesting..."</> }.into_any()
                                } else {
                                    view! { <><i class="fa-solid fa-file-import"></i>" Ingest Selected"</> }.into_any()
                                }}
                            </button>
                        </div>
                    </div>
                })
            }}

            // Step 3: Done
            {move || {
                if step.get() != 3 { return None; }
                Some(view! {
                    <div style="text-align: center; padding: 2rem 0;">
                        <i class="fa-solid fa-circle-check" style="font-size: 3rem; color: #66bb6a; margin-bottom: 1rem; display: block;"></i>
                        <h3>"Investigation Complete"</h3>
                        {move || commit_result.get().map(|msg| view! {
                            <p style="font-size: 0.9rem; margin-top: 0.5rem;">{msg}</p>
                        })}
                        <p class="text-secondary" style="margin-top: 0.75rem;">
                            "New entities and relations have been added to your knowledge graph. Close this modal and search again to see the updated graph."
                        </p>
                        <button class="btn btn-secondary" style="margin-top: 1rem;" on:click=move |_| set_step.set(1)>
                            <i class="fa-solid fa-rotate"></i>" Investigate Again"
                        </button>
                    </div>
                })
            }}

            // Error display (visible from any step)
            {move || {
                if step.get() == 3 { return None; }
                commit_result.get().map(|msg| {
                    if msg.starts_with("Error") {
                        Some(view! {
                            <div class="alert alert-warning" style="margin-top: 0.75rem;">
                                <i class="fa-solid fa-triangle-exclamation" style="margin-right: 0.25rem;"></i>{msg}
                            </div>
                        })
                    } else { None }
                }).flatten()
            }}
        </div>
    }
    .into_any()
}
