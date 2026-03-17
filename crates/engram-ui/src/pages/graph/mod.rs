mod search;
mod sidebar;

use leptos::prelude::*;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{NodeHit, QueryRequest, QueryResponse, SearchRequest, SearchResponse, NodeResponse};
use crate::components::chat_types::ChatSelectedNode;
use crate::components::graph_canvas::GraphCanvas;
use crate::components::crud_modal::CrudModal;
use crate::components::detail_modal::DetailModal;

use search::search_variations;

/// Humanize a fact slug: strip trailing `-XXXX` hash, replace hyphens with spaces, capitalize.
fn humanize_fact_slug(slug: &str) -> String {
    // Strip trailing hash: last "-" followed by 4 hex chars
    let clean = if slug.len() > 5 {
        let last_dash = slug.rfind('-').unwrap_or(slug.len());
        if slug.len() - last_dash == 5
            && slug[last_dash + 1..].chars().all(|c| c.is_ascii_hexdigit())
        {
            &slug[..last_dash]
        } else {
            slug
        }
    } else {
        slug
    };
    let mut result: String = clean.replace('-', " ");
    if let Some(first) = result.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    if result.chars().count() > 40 {
        format!("{}...", result.chars().take(40).collect::<String>())
    } else {
        result
    }
}

#[component]
pub fn GraphPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (nodes, set_nodes) = signal(Vec::<serde_json::Value>::new());
    let (edges, set_edges) = signal(Vec::<serde_json::Value>::new());
    let (_selected_node, set_selected_node) = signal(Option::<String>::None);
    let chat_selected = use_context::<ChatSelectedNode>();
    let (node_detail, set_node_detail) = signal(Option::<NodeResponse>::None);
    let (search_term, set_search_term) = signal(String::new());
    let (depth, set_depth) = signal(1u32);
    let (min_strength, set_min_strength) = signal(0.0f32);
    let (direction, set_direction) = signal("both".to_string());
    let (crud_open, set_crud_open) = signal(false);
    let (loading, set_loading) = signal(false);

    // Explore enhancements
    let (hidden_types, set_hidden_types) = signal(Vec::<String>::new());
    let (hidden_rels, set_hidden_rels) = signal(Vec::<String>::new());
    let (start_node, set_start_node) = signal(Option::<String>::None);
    let (show_edge_labels, set_show_edge_labels) = signal(true);
    let (edge_bundling, set_edge_bundling) = signal(false);
    let (temporal_current_only, set_temporal_current_only) = signal(false);
    let (highlighted_node, set_highlighted_node) = signal(Option::<String>::None);
    let (filters_open, set_filters_open) = signal(false);

    // Smart search: results list fallback
    let (search_results, set_search_results) = signal(Vec::<NodeHit>::new());
    let (show_results_list, set_show_results_list) = signal(false);

    // Progressive disclosure hint
    let (show_hint, set_show_hint) = signal(false);

    // Find Path sidebar signals
    let (path_autocomplete_open, set_path_autocomplete_open) = signal(true);
    let (path_from, set_path_from) = signal(Option::<String>::None);
    let (path_target_query, set_path_target_query) = signal(String::new());
    let (path_via_query, set_path_via_query) = signal(String::new());
    let (path_min_depth, set_path_min_depth) = signal(1u32);
    let (path_max_depth, set_path_max_depth) = signal(5u32);
    let (path_results, set_path_results) = signal(Vec::<Vec<String>>::new());
    let (path_selected, set_path_selected) = signal(Vec::<bool>::new());

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
                                let ntype_lower = ntype.to_lowercase();
                                let base_size = 4.0 + (conf as f64 * 6.0);
                                let (size, color, shape, display_label) = match ntype_lower.as_str() {
                                    "fact" => {
                                        let claim = if n.label.len() > 5 && n.label[..5].eq_ignore_ascii_case("fact:") {
                                            humanize_fact_slug(n.label[5..].trim())
                                        } else {
                                            let l = &n.label;
                                            if l.chars().count() > 40 { format!("{}...", l.chars().take(40).collect::<String>()) } else { l.to_string() }
                                        };
                                        (base_size * 0.5, Some("#ffa726"), Some("diamond"), claim)
                                    },
                                    "source" => {
                                        let short = if n.label.len() > 7 && n.label[..7].eq_ignore_ascii_case("source:") {
                                            n.label[7..].trim().to_string()
                                        } else {
                                            n.label.clone()
                                        };
                                        (base_size * 0.75, Some("#78909c"), Some("triangle"), short)
                                    },
                                    _ => (base_size, None, None, n.label.clone()),
                                };
                                let mut node = serde_json::json!({
                                    "id": n.label,
                                    "label": n.label,
                                    "display_label": display_label,
                                    "title": format!("{} ({:.0}%)", ntype, conf * 100.0),
                                    "node_type": ntype,
                                    "confidence": conf,
                                    "size": size,
                                });
                                if let Some(c) = color {
                                    node.as_object_mut().unwrap().insert("color".into(), serde_json::Value::String(c.into()));
                                }
                                if let Some(s) = shape {
                                    node.as_object_mut().unwrap().insert("shape".into(), serde_json::Value::String(s.into()));
                                }
                                node
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

            // Clear filters, highlight, and path state on new search
            set_hidden_types.set(Vec::new());
            set_hidden_rels.set(Vec::new());
            set_highlighted_node.set(None);
            set_path_from.set(None);
            set_path_results.set(Vec::new());
            set_path_selected.set(Vec::new());
            set_path_target_query.set(String::new());
            set_path_via_query.set(String::new());
            set_loading.set(false);
        }
    });

    // Load node details on click + highlight (sidebar preview only, no modal)
    let api_detail = api.clone();
    let on_select = Callback::new(move |node_id: String| {
        set_selected_node.set(Some(node_id.clone()));
        if let Some(ctx) = chat_selected { ctx.0.set(Some(node_id.clone())); }
        set_highlighted_node.set(Some(node_id.clone()));
        set_detail_node_id.set(Some(node_id.clone()));
        // Do NOT open modal on click -- modal opens from sidebar "Open" button or context menu
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
                        let ntype_lower = ntype.to_lowercase();
                        let base_size = 4.0 + (conf as f64 * 6.0);
                        let (size, color, shape, display_label) = match ntype_lower.as_str() {
                            "fact" => {
                                let claim = if n.label.len() > 5 && n.label[..5].eq_ignore_ascii_case("fact:") {
                                    humanize_fact_slug(n.label[5..].trim())
                                } else {
                                    let l = &n.label;
                                    if l.chars().count() > 40 { format!("{}...", l.chars().take(40).collect::<String>()) } else { l.to_string() }
                                };
                                (base_size * 0.5, Some("#ffa726"), Some("diamond"), claim)
                            },
                            "source" => {
                                let short = if n.label.len() > 7 && n.label[..7].eq_ignore_ascii_case("source:") {
                                    n.label[7..].trim().to_string()
                                } else {
                                    n.label.clone()
                                };
                                (base_size * 0.75, Some("#78909c"), Some("triangle"), short)
                            },
                            _ => (base_size, None, None, n.label.clone()),
                        };
                        let mut new_node = serde_json::json!({
                            "id": n.label,
                            "label": n.label,
                            "display_label": display_label,
                            "title": format!("{} ({:.0}%)", ntype, conf * 100.0),
                            "node_type": ntype,
                            "confidence": conf,
                            "size": size,
                        });
                        if let Some(c) = color {
                            new_node.as_object_mut().unwrap().insert("color".into(), serde_json::Value::String(c.into()));
                        }
                        if let Some(s) = shape {
                            new_node.as_object_mut().unwrap().insert("shape".into(), serde_json::Value::String(s.into()));
                        }
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

    // Listen for JS context menu "Find Path From..." event
    {
        let set_pf = set_path_from.clone();
        let set_pr = set_path_results.clone();
        let set_ps = set_path_selected.clone();
        let set_ptq = set_path_target_query.clone();
        let set_pvq = set_path_via_query.clone();
        let cb = Closure::wrap(Box::new(move |ev: web_sys::CustomEvent| {
            if let Some(detail) = ev.detail().as_string() {
                set_pf.set(Some(detail));
                set_pr.set(Vec::new());
                set_ps.set(Vec::new());
                set_ptq.set(String::new());
                set_pvq.set(String::new());
            }
        }) as Box<dyn FnMut(web_sys::CustomEvent)>);
        let _ = web_sys::window().unwrap().add_event_listener_with_callback(
            "engram-set-path-from",
            cb.as_ref().unchecked_ref(),
        );
        cb.forget();
    }

    // Show hint when graph loads with nodes, auto-dismiss after 5s
    Effect::new(move |_| {
        let n = nodes.get();
        if !n.is_empty() {
            set_show_hint.set(true);
            // Auto-dismiss after 5s via JS setTimeout
            let _ = js_sys::eval("setTimeout(function(){if(window.__engram_hint_dismiss){window.__engram_hint_dismiss()}},5000)");
            // Store dismiss callback
            let dismiss_cb = Closure::wrap(Box::new(move || {
                set_show_hint.set(false);
            }) as Box<dyn FnMut()>);
            let _ = js_sys::Reflect::set(
                &wasm_bindgen::JsValue::from(web_sys::window().unwrap()),
                &"__engram_hint_dismiss".into(),
                dismiss_cb.as_ref(),
            );
            dismiss_cb.forget();
        }
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
                    <i class="fa-solid fa-plus"></i>" New"
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
                // Progressive disclosure hint overlay
                {move || if show_hint.get() {
                    Some(view! {
                        <div class="engram-hint-overlay"
                            on:click=move |_| set_show_hint.set(false)>
                            <i class="fa-solid fa-hand-pointer" style="margin-right: 0.5rem;"></i>
                            "Double-click a node to explore deeper connections"
                        </div>
                    })
                } else { None }}

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
                {sidebar::search_results_view(
                    search_results,
                    show_results_list,
                    set_show_results_list,
                    set_search_term,
                    Callback::new(move |_: ()| { do_search.dispatch(()); }),
                )}

                // Controls
                {sidebar::controls_view(
                    depth,
                    set_depth,
                    min_strength,
                    set_min_strength,
                    direction,
                    set_direction,
                    show_edge_labels,
                    set_show_edge_labels,
                    temporal_current_only,
                    set_temporal_current_only,
                    edge_bundling,
                    set_edge_bundling,
                )}

                // Collapsible Filters section
                {sidebar::filters_view(
                    type_counts,
                    rel_counts,
                    hidden_types,
                    set_hidden_types,
                    hidden_rels,
                    set_hidden_rels,
                    filters_open,
                    set_filters_open,
                )}

                // Find Path sidebar section
                {sidebar::find_path_view(
                    nodes,
                    path_from,
                    set_path_from,
                    path_target_query,
                    set_path_target_query,
                    path_via_query,
                    set_path_via_query,
                    path_min_depth,
                    set_path_min_depth,
                    path_max_depth,
                    set_path_max_depth,
                    path_autocomplete_open,
                    set_path_autocomplete_open,
                    path_results,
                    set_path_results,
                    path_selected,
                    set_path_selected,
                )}

                // Compact node preview panel
                {sidebar::node_preview_view(
                    node_detail,
                    set_detail_node_id,
                    set_detail_modal_open,
                    set_start_node,
                    set_path_from,
                    set_path_results,
                    set_path_selected,
                    set_path_target_query,
                )}
            </div>
        </div>

        <CrudModal open=crud_open on_close=close_crud on_created=on_crud_created />
        <DetailModal open=detail_modal_open_signal node_id=detail_node_id_signal on_close=close_detail />
    }
}
