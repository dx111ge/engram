use leptos::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::api::ApiClient;
use crate::api::types::{NodeHit, QueryRequest, QueryResponse, SearchRequest, SearchResponse, NodeResponse};
use crate::components::graph_canvas::GraphCanvas;
use crate::components::crud_modal::CrudModal;
use crate::components::detail_modal::DetailModal;

/// Generate search variations for smart search (add/remove hyphens, spaces).
fn search_variations(query: &str) -> Vec<String> {
    let mut variations = vec![query.to_string()];
    // Try removing hyphens
    if query.contains('-') {
        variations.push(query.replace('-', ""));
        variations.push(query.replace('-', " "));
    }
    // Try adding hyphens between letter-number boundaries
    let with_hyphen = add_hyphens_at_boundaries(query);
    if with_hyphen != query {
        variations.push(with_hyphen);
    }
    // Deduplicate while preserving order
    let mut seen = HashSet::new();
    variations.retain(|v| seen.insert(v.clone()));
    variations
}

/// Insert hyphens between letter-digit boundaries: "F16" -> "F-16"
fn add_hyphens_at_boundaries(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::with_capacity(s.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        result.push(c);
        if i + 1 < chars.len() {
            let next = chars[i + 1];
            if (c.is_alphabetic() && next.is_ascii_digit())
                || (c.is_ascii_digit() && next.is_alphabetic())
            {
                result.push('-');
            }
        }
    }
    result
}

#[component]
pub fn GraphPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (nodes, set_nodes) = signal(Vec::<serde_json::Value>::new());
    let (edges, set_edges) = signal(Vec::<serde_json::Value>::new());
    let (_selected_node, set_selected_node) = signal(Option::<String>::None);
    let (node_detail, set_node_detail) = signal(Option::<NodeResponse>::None);
    let (search_term, set_search_term) = signal(String::new());
    let (depth, set_depth) = signal(2u32);
    let (min_strength, set_min_strength) = signal(0.0f32);
    let (direction, set_direction) = signal("both".to_string());
    let (crud_open, set_crud_open) = signal(false);
    let (loading, set_loading) = signal(false);

    // Explore enhancements
    let (hidden_types, set_hidden_types) = signal(Vec::<String>::new());
    let (hidden_rels, set_hidden_rels) = signal(Vec::<String>::new());
    let (start_node, set_start_node) = signal(Option::<String>::None);
    let (show_edge_labels, set_show_edge_labels) = signal(true);
    let (temporal_current_only, set_temporal_current_only) = signal(false);
    let (highlighted_node, set_highlighted_node) = signal(Option::<String>::None);
    let (filters_open, set_filters_open) = signal(false);

    // Smart search: results list fallback
    let (search_results, set_search_results) = signal(Vec::<NodeHit>::new());
    let (show_results_list, set_show_results_list) = signal(false);

    // Detail modal signals
    let (detail_modal_open, set_detail_modal_open) = signal(false);
    let (detail_node_id, set_detail_node_id) = signal(Option::<String>::None);

    // Derived: count node types from current graph data
    let type_counts = Signal::derive(move || {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for n in nodes.get().iter() {
            let nt = n.get("node_type")
                .and_then(|v| v.as_str())
                .unwrap_or("Entity")
                .to_string();
            *counts.entry(nt).or_insert(0) += 1;
        }
        let mut sorted: Vec<(String, usize)> = counts.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted
    });

    // Derived: count relationship types from current graph data
    let rel_counts = Signal::derive(move || {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for e in edges.get().iter() {
            let rel = e.get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("related_to")
                .to_string();
            *counts.entry(rel).or_insert(0) += 1;
        }
        let mut sorted: Vec<(String, usize)> = counts.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted
    });

    // Smart search action: try variations, traverse best match, fall back to list
    let api_query = api.clone();
    let do_search = Action::new_local(move |_: &()| {
        let api = api_query.clone();
        let query = search_term.get_untracked();
        let d = depth.get_untracked();
        let min_c = min_strength.get_untracked();
        let dir = direction.get_untracked();
        async move {
            if query.is_empty() { return; }
            set_loading.set(true);
            set_show_results_list.set(false);
            set_search_results.set(Vec::new());

            // Generate search variations
            let variations = search_variations(&query);

            // Try each variation, collect all results
            let mut best_match: Option<NodeHit> = None;
            let mut all_results: Vec<NodeHit> = Vec::new();

            for variation in &variations {
                let search_body = SearchRequest { query: variation.clone(), limit: Some(50) };
                if let Ok(sr) = api.post::<_, SearchResponse>("/search", &search_body).await {
                    for hit in &sr.results {
                        // Check for exact/close match (label matches a variation)
                        if best_match.is_none() {
                            let hit_lower = hit.label.to_lowercase();
                            for v in &variations {
                                if hit_lower == v.to_lowercase() {
                                    best_match = Some(hit.clone());
                                    break;
                                }
                            }
                        }
                    }
                    // Merge results (deduplicate by label)
                    for hit in sr.results {
                        if !all_results.iter().any(|r| r.label == hit.label) {
                            all_results.push(hit);
                        }
                    }
                }
            }

            // If no exact match but we have results, use the first result as best
            if best_match.is_none() && !all_results.is_empty() {
                best_match = Some(all_results[0].clone());
            }

            if let Some(best) = best_match {
                // Traverse from the best match
                let start_label = best.label.clone();
                set_start_node.set(Some(start_label.clone()));

                let body = QueryRequest {
                    query: start_label,
                    limit: Some(100),
                    depth: Some(d),
                    direction: Some(dir),
                    min_confidence: if min_c > 0.0 { Some(min_c) } else { None },
                    node_type: None,
                };
                match api.post::<_, QueryResponse>("/query", &body).await {
                    Ok(result) => {
                        if result.nodes.is_empty() {
                            // Traverse returned nothing -- show as list fallback
                            set_search_results.set(all_results);
                            set_show_results_list.set(true);
                            set_nodes.set(Vec::new());
                            set_edges.set(Vec::new());
                        } else {
                            let vis_nodes: Vec<serde_json::Value> = result.nodes.iter().map(|n| {
                                let conf = n.confidence.unwrap_or(0.5);
                                let ntype = n.node_type.as_deref().unwrap_or("Entity");
                                serde_json::json!({
                                    "id": n.label,
                                    "label": n.label,
                                    "title": format!("{} ({:.0}%)", ntype, conf * 100.0),
                                    "node_type": ntype,
                                    "confidence": conf,
                                    "size": 4.0 + (conf as f64 * 6.0),
                                })
                            }).collect();

                            let vis_edges: Vec<serde_json::Value> = result.edges.iter().map(|e| {
                                let mut edge = serde_json::json!({
                                    "from": e.from,
                                    "to": e.to,
                                    "label": e.relationship,
                                    "title": format!("{:.0}%", e.confidence * 100.0),
                                });
                                if let Some(ref vf) = e.valid_from {
                                    edge.as_object_mut().unwrap().insert("valid_from".into(), serde_json::Value::String(vf.clone()));
                                }
                                if let Some(ref vt) = e.valid_to {
                                    edge.as_object_mut().unwrap().insert("valid_to".into(), serde_json::Value::String(vt.clone()));
                                }
                                edge
                            }).collect();

                            set_nodes.set(vis_nodes);
                            set_edges.set(vis_edges);
                        }
                    }
                    Err(_) => {
                        set_nodes.set(Vec::new());
                        set_edges.set(Vec::new());
                    }
                }
            } else {
                // No results at all
                set_nodes.set(Vec::new());
                set_edges.set(Vec::new());
                set_start_node.set(None);
            }

            // Clear filters and highlight on new search
            set_hidden_types.set(Vec::new());
            set_hidden_rels.set(Vec::new());
            set_highlighted_node.set(None);
            set_loading.set(false);
        }
    });

    // Load node details on click + highlight + open detail modal
    let api_detail = api.clone();
    let on_select = Callback::new(move |node_id: String| {
        set_selected_node.set(Some(node_id.clone()));
        set_highlighted_node.set(Some(node_id.clone()));
        set_detail_node_id.set(Some(node_id.clone()));
        set_detail_modal_open.set(true);
        let api = api_detail.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let encoded = js_sys::encode_uri_component(&node_id);
            if let Ok(detail) = api.get::<NodeResponse>(&format!("/node/{encoded}")).await {
                set_node_detail.set(Some(detail));
            }
        });
    });

    // Double-click expand
    let api_expand = api.clone();
    let on_double_click = Callback::new(move |node_id: String| {
        let api = api_expand.clone();
        let d = depth.get_untracked() + 1;
        wasm_bindgen_futures::spawn_local(async move {
            let body = QueryRequest {
                query: node_id,
                limit: Some(50),
                depth: Some(d),
                direction: Some("both".into()),
                min_confidence: None,
                node_type: None,
            };
            if let Ok(result) = api.post::<_, QueryResponse>("/query", &body).await {
                set_nodes.update(|existing| {
                    for n in &result.nodes {
                        let conf = n.confidence.unwrap_or(0.5);
                        let ntype = n.node_type.as_deref().unwrap_or("Entity");
                        let new_node = serde_json::json!({
                            "id": n.label,
                            "label": n.label,
                            "title": format!("{} ({:.0}%)", ntype, conf * 100.0),
                            "node_type": ntype,
                            "confidence": conf,
                            "size": 4.0 + (conf as f64 * 6.0),
                        });
                        if !existing.iter().any(|e| e.get("id") == new_node.get("id")) {
                            existing.push(new_node);
                        }
                    }
                });
                set_edges.update(|existing| {
                    for e in &result.edges {
                        let mut new_edge = serde_json::json!({
                            "from": e.from,
                            "to": e.to,
                            "label": e.relationship,
                        });
                        if let Some(ref vf) = e.valid_from {
                            new_edge.as_object_mut().unwrap().insert("valid_from".into(), serde_json::Value::String(vf.clone()));
                        }
                        if let Some(ref vt) = e.valid_to {
                            new_edge.as_object_mut().unwrap().insert("valid_to".into(), serde_json::Value::String(vt.clone()));
                        }
                        existing.push(new_edge);
                    }
                });
            }
        });
    });

    let nodes_signal = Signal::derive(move || nodes.get());
    let edges_signal = Signal::derive(move || edges.get());
    let hidden_types_signal = Signal::derive(move || hidden_types.get());
    let hidden_rels_signal = Signal::derive(move || hidden_rels.get());
    let start_node_signal = Signal::derive(move || start_node.get());
    let show_edge_labels_signal = Signal::derive(move || show_edge_labels.get());
    let temporal_signal = Signal::derive(move || temporal_current_only.get());
    let highlighted_signal = Signal::derive(move || highlighted_node.get());

    let close_crud = Callback::new(move |_: ()| set_crud_open.set(false));
    let on_crud_created = Callback::new(move |_: ()| {
        do_search.dispatch(());
    });

    // Detail modal signals as Signal<T> for the component props
    let detail_modal_open_signal = Signal::derive(move || detail_modal_open.get());
    let detail_node_id_signal = Signal::derive(move || detail_node_id.get());
    let close_detail = Callback::new(move |_: ()| set_detail_modal_open.set(false));

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-diagram-project"></i>" Knowledge Graph"</h2>
            <div class="page-actions flex gap-sm">
                <input
                    type="text"
                    placeholder="Search entities..."
                    class="search-input"
                    style="width: 300px;"
                    prop:value=search_term
                    on:input=move |ev| set_search_term.set(event_target_value(&ev))
                    on:keydown=move |ev| {
                        if ev.key() == "Enter" { do_search.dispatch(()); }
                    }
                />
                <button class="btn btn-primary" on:click=move |_| { do_search.dispatch(()); }
                    disabled=loading>
                    {move || if loading.get() {
                        view! { <span class="spinner"></span> }.into_any()
                    } else {
                        view! { <><i class="fa-solid fa-search"></i>" Go"</> }.into_any()
                    }}
                </button>
                <button class="btn btn-secondary" on:click=move |_| set_crud_open.set(true)>
                    <i class="fa-solid fa-pencil"></i>" CRUD"
                </button>
            </div>
        </div>

        <div class="graph-container">
            <div class="graph-canvas" style="position: relative;">
                <button
                    class="btn btn-secondary"
                    style="position: absolute; top: 12px; right: 12px; z-index: 10; opacity: 0.85;"
                    title="Recenter graph"
                    on:click=move |_| {
                        let _ = js_sys::eval("window.__engram_graph.recenter()");
                    }
                >
                    <i class="fa-solid fa-crosshairs"></i>
                </button>
                {move || if nodes.get().is_empty() {
                    view! {
                        <div class="empty-state">
                            <i class="fa-solid fa-diagram-project"></i>
                            <p>"Search for a fact to explore its connections"</p>
                            <p class="text-secondary" style="font-size: 0.85rem;">"Click a node to see details. Right-click for context menu."</p>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <GraphCanvas
                            nodes=nodes_signal
                            edges=edges_signal
                            on_select_node=on_select
                            on_double_click=on_double_click
                            hidden_types=hidden_types_signal
                            hidden_rels=hidden_rels_signal
                            start_node_id=start_node_signal
                            show_edge_labels=show_edge_labels_signal
                            highlighted_node=highlighted_signal
                            temporal_current_only=temporal_signal
                        />
                    }.into_any()
                }}
            </div>

            <div class="graph-sidebar">
                // Search results list (shown when smart search finds no traverse match)
                {move || {
                    if !show_results_list.get() { return None; }
                    let results = search_results.get();
                    if results.is_empty() { return None; }
                    Some(view! {
                        <div class="card">
                            <h3><i class="fa-solid fa-list"></i>" Search Results"</h3>
                            <p class="text-secondary" style="font-size: 0.8rem; margin-bottom: 0.5rem;">
                                {format!("{} matches found. Click to explore.", results.len())}
                            </p>
                            <div style="display: grid; gap: 0.25rem;">
                                {results.iter().map(|hit| {
                                    let label = hit.label.clone();
                                    let ntype = hit.node_type.clone().unwrap_or_else(|| "Entity".to_string());
                                    let conf = hit.confidence.unwrap_or(0.5);
                                    let label_click = label.clone();
                                    view! {
                                        <div class="prop-row" style="cursor: pointer; padding: 0.4rem 0.5rem; border-radius: 4px;"
                                            title="Click to traverse from this entity"
                                            on:click=move |_| {
                                                set_search_term.set(label_click.clone());
                                                set_show_results_list.set(false);
                                                do_search.dispatch(());
                                            }>
                                            <span style="display: flex; align-items: center; gap: 0.5rem;">
                                                <strong style="font-size: 0.85rem;">{label}</strong>
                                                <span class="badge badge-active" style="font-size: 0.65rem;">{ntype}</span>
                                            </span>
                                            <span class="text-secondary" style="font-size: 0.75rem;">
                                                {format!("{:.0}%", conf * 100.0)}
                                            </span>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        </div>
                    })
                }}

                // Controls
                <div class="card">
                    <h3><i class="fa-solid fa-sliders"></i>" Controls"</h3>
                    <div class="slider-group mt-1">
                        <label>
                            <span>"Depth"</span>
                            <span>{move || depth.get().to_string()}</span>
                        </label>
                        <input type="range" min="1" max="5" step="1"
                            prop:value=move || depth.get().to_string()
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse() {
                                    set_depth.set(v);
                                }
                            }
                        />
                    </div>
                    <div class="slider-group">
                        <label>
                            <span>"Min Strength"</span>
                            <span>{move || format!("{:.0}%", min_strength.get() * 100.0)}</span>
                        </label>
                        <input type="range" min="0" max="1" step="0.05"
                            prop:value=move || min_strength.get().to_string()
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse() {
                                    set_min_strength.set(v);
                                }
                            }
                        />
                    </div>
                    <div class="form-group">
                        <label>"Direction"</label>
                        <select prop:value=direction
                            on:change=move |ev| set_direction.set(event_target_value(&ev))>
                            <option value="both">"Both directions"</option>
                            <option value="out">"Outgoing"</option>
                            <option value="in">"Incoming"</option>
                        </select>
                    </div>
                    <div class="form-group">
                        <label>"Layout"</label>
                        <select>
                            <option value="forceAtlas2">"Force Atlas"</option>
                            <option value="barnesHut">"Barnes Hut"</option>
                            <option value="repulsion">"Repulsion"</option>
                        </select>
                    </div>
                    // Edge labels toggle
                    <div class="form-group" style="margin-top: 0.5rem;">
                        <label class="checkbox-label">
                            <input type="checkbox"
                                prop:checked=show_edge_labels
                                on:change=move |ev| {
                                    let checked = event_target_checked(&ev);
                                    set_show_edge_labels.set(checked);
                                }
                            />
                            " Show edge labels"
                        </label>
                    </div>
                    // Temporal mode toggle
                    <div class="form-group">
                        <label class="checkbox-label">
                            <input type="checkbox"
                                prop:checked=temporal_current_only
                                on:change=move |ev| {
                                    let checked = event_target_checked(&ev);
                                    set_temporal_current_only.set(checked);
                                }
                            />
                            " Current relations only"
                        </label>
                    </div>
                </div>

                // Collapsible Filters section
                {move || {
                    let tc = type_counts.get();
                    let rc = rel_counts.get();
                    if tc.is_empty() && rc.is_empty() { return None; }
                    let hidden_t_count = hidden_types.get().len();
                    let hidden_r_count = hidden_rels.get().len();
                    let filter_summary = if hidden_t_count > 0 || hidden_r_count > 0 {
                        format!(" ({} hidden)", hidden_t_count + hidden_r_count)
                    } else { String::new() };
                    Some(view! {
                        <div class="card">
                            <h3 style="cursor: pointer; user-select: none;" on:click=move |_| set_filters_open.set(!filters_open.get_untracked())>
                                <i class=move || if filters_open.get() { "fa-solid fa-chevron-down" } else { "fa-solid fa-chevron-right" }></i>
                                " Filters"
                                <span class="text-secondary" style="font-size: 0.75rem; font-weight: 400;">{filter_summary}</span>
                            </h3>
                            <div style=move || if filters_open.get() { "" } else { "display:none" }>
                                // Node types
                                {(!tc.is_empty()).then(|| view! {
                                    <div style="margin-bottom: 0.5rem;">
                                        <span class="text-secondary" style="font-size: 0.75rem; font-weight: 600; text-transform: uppercase;">"Node Types"</span>
                                        <div class="filter-chips">
                                            {tc.iter().map(|(t, count)| {
                                                let t_lower = t.to_lowercase();
                                                let t_clone = t_lower.clone();
                                                let t_display = t.clone();
                                                let count = *count;
                                                let is_hidden = {
                                                    let t_check = t_lower.clone();
                                                    move || hidden_types.get().contains(&t_check)
                                                };
                                                view! {
                                                    <button
                                                        class=move || if is_hidden() { "filter-chip filter-chip-hidden" } else { "filter-chip filter-chip-active" }
                                                        on:click=move |_| {
                                                            let t_val = t_clone.clone();
                                                            set_hidden_types.update(|v| {
                                                                if let Some(pos) = v.iter().position(|x| *x == t_val) {
                                                                    v.remove(pos);
                                                                } else {
                                                                    v.push(t_val);
                                                                }
                                                            });
                                                        }
                                                    >
                                                        {t_display} " " <span class="chip-count">{count.to_string()}</span>
                                                    </button>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </div>
                                    </div>
                                })}
                                // Relationship types
                                {(!rc.is_empty()).then(|| view! {
                                    <div>
                                        <span class="text-secondary" style="font-size: 0.75rem; font-weight: 600; text-transform: uppercase;">"Relationships"</span>
                                        <div class="filter-chips">
                                            {rc.iter().map(|(r, count)| {
                                                let r_lower = r.to_lowercase();
                                                let r_clone = r_lower.clone();
                                                let r_display = r.clone();
                                                let count = *count;
                                                let is_hidden = {
                                                    let r_check = r_lower.clone();
                                                    move || hidden_rels.get().contains(&r_check)
                                                };
                                                view! {
                                                    <button
                                                        class=move || if is_hidden() { "filter-chip filter-chip-hidden" } else { "filter-chip filter-chip-active" }
                                                        on:click=move |_| {
                                                            let r_val = r_clone.clone();
                                                            set_hidden_rels.update(|v| {
                                                                if let Some(pos) = v.iter().position(|x| *x == r_val) {
                                                                    v.remove(pos);
                                                                } else {
                                                                    v.push(r_val);
                                                                }
                                                            });
                                                        }
                                                    >
                                                        {r_display} " " <span class="chip-count">{count.to_string()}</span>
                                                    </button>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </div>
                                    </div>
                                })}
                            </div>
                        </div>
                    })
                }}

                // Compact node preview panel (replaces the old detail panel with action buttons)
                {move || if node_detail.get().is_none() {
                    Some(view! {
                        <div class="card">
                            <h3><i class="fa-solid fa-circle-info"></i>" DETAILS"</h3>
                            <p class="text-secondary">"Click a node in the graph to see its details here."</p>
                        </div>
                    })
                } else { None }}
                {move || node_detail.get().map(|detail| {
                    let label = detail.label.clone();
                    let label_for_start = label.clone();
                    let label_for_open = label.clone();
                    view! {
                        <div class="card node-info-panel">
                            <h3>{detail.label.clone()}</h3>
                            {detail.node_type.clone().map(|t| view! {
                                <span class="badge badge-active">{t}</span>
                            })}
                            <div class="prop-row mt-1">
                                <span class="prop-key">"Confidence"</span>
                                <span>{format!("{:.0}%", detail.confidence * 100.0)}</span>
                            </div>
                            <div class="prop-row">
                                <span class="prop-key">"Connections"</span>
                                <span>{format!("{} out / {} in", detail.edges_from.len(), detail.edges_to.len())}</span>
                            </div>
                            <div class="node-actions mt-1">
                                <button class="btn btn-sm btn-primary" title="Open full detail modal"
                                    on:click=move |_| {
                                        set_detail_node_id.set(Some(label_for_open.clone()));
                                        set_detail_modal_open.set(true);
                                    }>
                                    <i class="fa-solid fa-expand"></i>" Open"
                                </button>
                                <button class="btn btn-sm btn-secondary" title="Set as start node"
                                    on:click=move |_| {
                                        set_start_node.set(Some(label_for_start.clone()));
                                    }>
                                    <i class="fa-solid fa-bullseye"></i>" Set as Start"
                                </button>
                            </div>
                        </div>
                    }
                })}
            </div>
        </div>

        <CrudModal open=crud_open on_close=close_crud on_created=on_crud_created />
        <DetailModal open=detail_modal_open_signal node_id=detail_node_id_signal on_close=close_detail />
    }
}

fn event_target_checked(ev: &web_sys::Event) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|el| el.checked())
        .unwrap_or(false)
}
