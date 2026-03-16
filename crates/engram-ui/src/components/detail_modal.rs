use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::api::ApiClient;
use crate::api::types::{NodeResponse, IngestRequest, IngestItem, IngestResponse};

#[component]
pub fn DetailModal(
    #[prop(into)] open: Signal<bool>,
    #[prop(into)] node_id: Signal<Option<String>>,
    #[prop(into)] on_close: Callback<()>,
) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (detail, set_detail) = signal(Option::<NodeResponse>::None);
    let (loading, set_loading) = signal(false);
    let (active_tab, set_active_tab) = signal("info".to_string());
    // Internal node_id for navigation within the modal
    let (current_node_id, set_current_node_id) = signal(Option::<String>::None);

    // Sync external node_id prop into current_node_id
    Effect::new(move || {
        let nid = node_id.get();
        set_current_node_id.set(nid);
    });

    // Fetch node details whenever current_node_id changes and modal is open
    let api_fetch = api.clone();
    Effect::new(move || {
        let is_open = open.get();
        let nid = current_node_id.get();
        if is_open {
            if let Some(id) = nid {
                let api = api_fetch.clone();
                set_loading.set(true);
                set_detail.set(None);
                wasm_bindgen_futures::spawn_local(async move {
                    let encoded = js_sys::encode_uri_component(&id);
                    match api.get::<NodeResponse>(&format!("/node/{encoded}")).await {
                        Ok(d) => set_detail.set(Some(d)),
                        Err(_) => set_detail.set(None),
                    }
                    set_loading.set(false);
                });
            }
        }
    });

    // Reset tab when modal opens
    Effect::new(move || {
        if open.get() {
            set_active_tab.set("info".to_string());
        }
    });

    let on_close_click = move |_| {
        on_close.run(());
    };

    // Overlay click (close when clicking backdrop)
    let on_overlay_click = move |ev: web_sys::MouseEvent| {
        // Only close if clicking the overlay itself, not modal content
        if let Some(target) = ev.target() {
            if let Ok(el) = target.dyn_into::<web_sys::HtmlElement>() {
                if el.class_list().contains("modal-overlay") {
                    on_close.run(());
                }
            }
        }
    };

    view! {
        <div
            class="modal-overlay"
            style=move || if open.get() { "display: flex;" } else { "display: none;" }
            on:click=on_overlay_click
        >
            <div class="wizard-modal" style="width: 900px; max-width: 95vw; max-height: 85vh; display: flex; flex-direction: column;">
                // Header
                {move || {
                    let d = detail.get();
                    let label = d.as_ref().map(|d| d.label.clone()).unwrap_or_default();
                    let ntype = d.as_ref().and_then(|d| d.node_type.clone());
                    let conf = d.as_ref().map(|d| d.confidence).unwrap_or(0.0);
                    view! {
                        <div class="wizard-modal-header" style="justify-content: space-between;">
                            <div style="display: flex; align-items: center; gap: 0.75rem;">
                                <h3>
                                    <i class="fa-solid fa-circle-nodes" style="margin-right: 0.5rem;"></i>
                                    {label}
                                </h3>
                                {ntype.map(|t| view! {
                                    <span class="badge badge-active">{t}</span>
                                })}
                                <span class="text-secondary" style="font-size: 0.85rem;">
                                    {format!("{:.0}%", conf * 100.0)}
                                </span>
                            </div>
                            <button class="btn btn-sm btn-secondary" on:click=on_close_click
                                style="min-width: auto; padding: 0.25rem 0.5rem;">
                                <i class="fa-solid fa-xmark"></i>
                            </button>
                        </div>
                    }
                }}

                // Tab bar
                <div style="display: flex; gap: 0; border-bottom: 1px solid var(--border); background: var(--bg-card);">
                    <button
                        class=move || if active_tab.get() == "info" { "btn btn-sm btn-primary" } else { "btn btn-sm btn-secondary" }
                        style="border-radius: 0; border: none; border-bottom: 2px solid transparent; padding: 0.5rem 1rem;"
                        style:border-bottom-color=move || if active_tab.get() == "info" { "var(--accent-bright)" } else { "transparent" }
                        on:click=move |_| set_active_tab.set("info".to_string())
                    >
                        <i class="fa-solid fa-circle-info"></i>" Info"
                    </button>
                    <button
                        class=move || if active_tab.get() == "connections" { "btn btn-sm btn-primary" } else { "btn btn-sm btn-secondary" }
                        style="border-radius: 0; border: none; border-bottom: 2px solid transparent; padding: 0.5rem 1rem;"
                        style:border-bottom-color=move || if active_tab.get() == "connections" { "var(--accent-bright)" } else { "transparent" }
                        on:click=move |_| set_active_tab.set("connections".to_string())
                    >
                        <i class="fa-solid fa-diagram-project"></i>" Connections"
                    </button>
                    <button
                        class=move || if active_tab.get() == "investigate" { "btn btn-sm btn-primary" } else { "btn btn-sm btn-secondary" }
                        style="border-radius: 0; border: none; border-bottom: 2px solid transparent; padding: 0.5rem 1rem;"
                        style:border-bottom-color=move || if active_tab.get() == "investigate" { "var(--accent-bright)" } else { "transparent" }
                        on:click=move |_| set_active_tab.set("investigate".to_string())
                    >
                        <i class="fa-solid fa-magnifying-glass-chart"></i>" Investigate"
                    </button>
                </div>

                // Body
                <div class="wizard-modal-body" style="flex: 1; overflow-y: auto;">
                    {move || {
                        if loading.get() {
                            return view! {
                                <div style="display: flex; align-items: center; justify-content: center; padding: 2rem;">
                                    <span class="spinner"></span>
                                    <span style="margin-left: 0.75rem;">"Loading..."</span>
                                </div>
                            }.into_any();
                        }
                        let d = detail.get();
                        if d.is_none() {
                            return view! {
                                <div style="padding: 2rem; text-align: center;">
                                    <p class="text-secondary">"No data available."</p>
                                </div>
                            }.into_any();
                        }
                        let d = d.unwrap();
                        let tab = active_tab.get();

                        match tab.as_str() {
                            "info" => {
                                render_info_tab(d)
                            }
                            "connections" => {
                                render_connections_tab(d, set_current_node_id)
                            }
                            "investigate" => {
                                render_investigate_tab(d, api.clone())
                            }
                            _ => view! { <div></div> }.into_any(),
                        }
                    }}
                </div>
            </div>
        </div>
    }
}

fn render_info_tab(detail: NodeResponse) -> leptos::prelude::AnyView {
    let confidence = detail.confidence;
    let conf_pct = confidence * 100.0;
    let conf_color = if confidence >= 0.7 {
        "#66bb6a"
    } else if confidence >= 0.4 {
        "#ffa726"
    } else {
        "#ef5350"
    };

    // Extract properties
    let props: Vec<(String, String)> = detail
        .properties
        .as_ref()
        .and_then(|p| p.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| {
                    let val = if let Some(s) = v.as_str() {
                        s.to_string()
                    } else {
                        v.to_string()
                    };
                    (k.clone(), val)
                })
                .collect()
        })
        .unwrap_or_default();

    // Extract KB ID if present
    let kb_id = detail
        .properties
        .as_ref()
        .and_then(|p| p.get("kb_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Extract ingest source
    let ingest_source = detail
        .properties
        .as_ref()
        .and_then(|p| p.get("ingest_source"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    view! {
        <div>
            // Confidence bar
            <div style="margin-bottom: 1rem;">
                <div style="display: flex; justify-content: space-between; margin-bottom: 0.25rem;">
                    <span class="text-secondary" style="font-size: 0.8rem;">"Confidence"</span>
                    <span style="font-size: 0.8rem; font-weight: 600;">{format!("{:.0}%", conf_pct)}</span>
                </div>
                <div style="height: 6px; background: var(--bg-tertiary); border-radius: 3px; overflow: hidden;">
                    <div style=format!("height: 100%; width: {:.0}%; background: {}; border-radius: 3px; transition: width 0.3s;", conf_pct, conf_color)></div>
                </div>
            </div>

            // Provenance
            {ingest_source.map(|src| view! {
                <div class="prop-row" style="margin-bottom: 0.5rem;">
                    <span class="prop-key"><i class="fa-solid fa-file-import" style="margin-right: 0.25rem;"></i>"Source"</span>
                    <span style="font-size: 0.85rem;">{src}</span>
                </div>
            })}

            // KB Link
            {kb_id.map(|id| {
                let url = if id.starts_with('Q') {
                    format!("https://www.wikidata.org/wiki/{}", id)
                } else {
                    id.clone()
                };
                view! {
                    <div class="prop-row" style="margin-bottom: 0.5rem;">
                        <span class="prop-key"><i class="fa-solid fa-link" style="margin-right: 0.25rem;"></i>"KB Link"</span>
                        <a href=url.clone() target="_blank" rel="noopener" style="font-size: 0.85rem; color: var(--accent-bright);">
                            {id}
                            " " <i class="fa-solid fa-arrow-up-right-from-square" style="font-size: 0.7rem;"></i>
                        </a>
                    </div>
                }
            })}

            // Properties table
            {if props.is_empty() {
                view! {
                    <p class="text-secondary" style="font-size: 0.85rem;">"No properties stored for this entity."</p>
                }.into_any()
            } else {
                view! {
                    <div>
                        <h4 style="font-size: 0.85rem; color: rgba(255,255,255,0.5); margin-bottom: 0.5rem; text-transform: uppercase;">
                            <i class="fa-solid fa-table-list" style="margin-right: 0.25rem;"></i>"Properties"
                        </h4>
                        <div style="display: grid; gap: 0.25rem;">
                            {props.iter().map(|(k, v)| view! {
                                <div class="prop-row">
                                    <span class="prop-key">{k.clone()}</span>
                                    <span style="font-size: 0.85rem; word-break: break-word;">{v.clone()}</span>
                                </div>
                            }).collect::<Vec<_>>()}
                        </div>
                    </div>
                }.into_any()
            }}
        </div>
    }
    .into_any()
}

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

fn render_investigate_tab(detail: NodeResponse, api: ApiClient) -> leptos::prelude::AnyView {
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

fn render_connections_tab(
    detail: NodeResponse,
    set_current_node_id: WriteSignal<Option<String>>,
) -> leptos::prelude::AnyView {
    let edges_from = detail.edges_from.clone();
    let edges_to = detail.edges_to.clone();

    view! {
        <div>
            // Outgoing edges
            <div style="margin-bottom: 1.5rem;">
                <h4 style="font-size: 0.85rem; color: rgba(255,255,255,0.5); margin-bottom: 0.5rem; text-transform: uppercase;">
                    <i class="fa-solid fa-arrow-right" style="margin-right: 0.25rem;"></i>
                    "Outgoing (" {edges_from.len().to_string()} ")"
                </h4>
                {if edges_from.is_empty() {
                    view! { <p class="text-secondary" style="font-size: 0.85rem;">"No outgoing connections."</p> }.into_any()
                } else {
                    view! {
                        <div style="display: grid; gap: 0.25rem;">
                            {edges_from.iter().map(|e| {
                                let to = e.to.clone();
                                let rel = e.relationship.clone();
                                let conf = e.confidence;
                                let to_click = to.clone();
                                view! {
                                    <div class="prop-row" style="cursor: pointer; padding: 0.35rem 0.5rem; border-radius: 4px;"
                                        title="Click to navigate"
                                        on:click=move |_| {
                                            set_current_node_id.set(Some(to_click.clone()));
                                        }>
                                        <span style="font-size: 0.8rem; color: var(--accent-bright);">
                                            {rel}
                                        </span>
                                        <span style="font-size: 0.85rem; display: flex; align-items: center; gap: 0.5rem;">
                                            <i class="fa-solid fa-arrow-right" style="font-size: 0.65rem; opacity: 0.5;"></i>
                                            <strong>{to}</strong>
                                            <span class="text-secondary" style="font-size: 0.75rem;">{format!("{:.0}%", conf * 100.0)}</span>
                                        </span>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    }.into_any()
                }}
            </div>

            // Incoming edges
            <div>
                <h4 style="font-size: 0.85rem; color: rgba(255,255,255,0.5); margin-bottom: 0.5rem; text-transform: uppercase;">
                    <i class="fa-solid fa-arrow-left" style="margin-right: 0.25rem;"></i>
                    "Incoming (" {edges_to.len().to_string()} ")"
                </h4>
                {if edges_to.is_empty() {
                    view! { <p class="text-secondary" style="font-size: 0.85rem;">"No incoming connections."</p> }.into_any()
                } else {
                    view! {
                        <div style="display: grid; gap: 0.25rem;">
                            {edges_to.iter().map(|e| {
                                let from = e.from.clone();
                                let rel = e.relationship.clone();
                                let conf = e.confidence;
                                let from_click = from.clone();
                                view! {
                                    <div class="prop-row" style="cursor: pointer; padding: 0.35rem 0.5rem; border-radius: 4px;"
                                        title="Click to navigate"
                                        on:click=move |_| {
                                            set_current_node_id.set(Some(from_click.clone()));
                                        }>
                                        <span style="font-size: 0.85rem; display: flex; align-items: center; gap: 0.5rem;">
                                            <strong>{from}</strong>
                                            <i class="fa-solid fa-arrow-right" style="font-size: 0.65rem; opacity: 0.5;"></i>
                                        </span>
                                        <span style="font-size: 0.8rem; display: flex; align-items: center; gap: 0.5rem;">
                                            <span style="color: var(--accent-bright);">{rel}</span>
                                            <span class="text-secondary" style="font-size: 0.75rem;">{format!("{:.0}%", conf * 100.0)}</span>
                                        </span>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    }.into_any()
                }}
            </div>
        </div>
    }
    .into_any()
}
