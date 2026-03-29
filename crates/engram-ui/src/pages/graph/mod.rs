mod search;
mod sidebar;

use leptos::prelude::*;
use wasm_bindgen::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{NodeHit, QueryRequest, QueryResponse, SearchRequest, SearchResponse, NodeResponse};
use crate::components::chat_types::ChatSelectedNode;
use crate::components::graph_canvas::GraphCanvas;
use crate::components::crud_modal::CrudModal;
use crate::components::detail_modal::DetailModal;
use crate::components::explore_controls::ExploreControls;
use crate::components::chat_panel::ChatPanel;

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
    let (_node_detail, set_node_detail) = signal(Option::<NodeResponse>::None);
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

    // Progressive disclosure hint
    let (show_hint, set_show_hint) = signal(false);

    // Detail modal signals
    let (detail_modal_open, set_detail_modal_open) = signal(false);
    let (detail_node_id, set_detail_node_id) = signal(Option::<String>::None);


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
                                        // Color by confidence: low=yellow (pending), mid=orange, high=green (confirmed)
                                        let fact_color = if conf < 0.1 { "#ef5350" }      // debunked (red)
                                            else if conf < 0.5 { "#f0ad4e" }               // pending/low (yellow)
                                            else if conf < 0.8 { "#ffa726" }               // moderate (orange)
                                            else { "#66bb6a" };                             // confirmed/high (green)
                                        (base_size * 0.5, Some(fact_color), Some("diamond"), claim)
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

                            // Collect node IDs to filter dangling edges
                            let node_ids: std::collections::HashSet<&str> = vis_nodes.iter()
                                .filter_map(|n| n.get("id").and_then(|v| v.as_str()))
                                .collect();

                            let vis_edges: Vec<serde_json::Value> = result.edges.iter().filter_map(|e| {
                                // Skip edges whose endpoints aren't in the node set
                                if !node_ids.contains(e.from.as_str()) || !node_ids.contains(e.to.as_str()) {
                                    return None;
                                }
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
                                Some(edge)
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

            // Clear filters, highlight on new search
            set_hidden_types.set(Vec::new());
            set_hidden_rels.set(Vec::new());
            set_highlighted_node.set(None);
            set_loading.set(false);
        }
    });

    // Load node details on click + highlight + push detail card to chat
    let api_detail = api.clone();
    let on_select = Callback::new(move |node_id: String| {
        set_selected_node.set(Some(node_id.clone()));
        if let Some(ctx) = chat_selected { ctx.0.set(Some(node_id.clone())); }
        set_highlighted_node.set(Some(node_id.clone()));
        set_detail_node_id.set(Some(node_id.clone()));
        // Do NOT open modal on click -- modal opens from "Open" button
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
                // Collect all node IDs (existing + newly added) to filter dangling edges
                let all_node_ids: std::collections::HashSet<String> = nodes.get_untracked().iter()
                    .filter_map(|n| n.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
                    .collect();
                set_edges.update(|existing| {
                    for e in &result.edges {
                        // Skip edges whose endpoints aren't in the node set
                        if !all_node_ids.contains(e.from.as_str()) || !all_node_ids.contains(e.to.as_str()) {
                            continue;
                        }
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
        let cb = Closure::wrap(Box::new(move |ev: web_sys::CustomEvent| {
            if let Some(detail) = ev.detail().as_string() {
                if let Some(ctx) = chat_selected { ctx.0.set(Some(detail)); }
            }
        }) as Box<dyn FnMut(web_sys::CustomEvent)>);
        let _ = web_sys::window().unwrap().add_event_listener_with_callback(
            "engram-set-path-from",
            cb.as_ref().unchecked_ref(),
        );
        cb.forget();
    }

    // Listen for chat graph data push events (Phase 5)
    {
        let set_n = set_nodes.clone();
        let set_e = set_edges.clone();
        let cb = Closure::wrap(Box::new(move |ev: web_sys::CustomEvent| {
            if let Ok(detail_str) = js_sys::JSON::stringify(&ev.detail()) {
                if let Some(s) = detail_str.as_string() {
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(&s) {
                        if let Some(new_nodes) = data.get("nodes").and_then(|v| v.as_array()) {
                            let vis_nodes: Vec<serde_json::Value> = new_nodes.iter().map(|n| {
                                let label = n.get("label").or_else(|| n.get("id"))
                                    .and_then(|v| v.as_str()).unwrap_or("?").to_string();
                                let ntype = n.get("node_type").and_then(|v| v.as_str()).unwrap_or("Entity");
                                let conf = n.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.5);
                                let base_size = 4.0 + (conf * 6.0);
                                serde_json::json!({
                                    "id": label,
                                    "label": label,
                                    "display_label": label,
                                    "title": format!("{} ({:.0}%)", ntype, conf * 100.0),
                                    "node_type": ntype,
                                    "confidence": conf,
                                    "size": base_size,
                                })
                            }).collect();

                            set_n.update(|existing| {
                                for node in vis_nodes {
                                    if !existing.iter().any(|e| e.get("id") == node.get("id")) {
                                        existing.push(node);
                                    }
                                }
                            });
                        }
                        if let Some(new_edges) = data.get("edges").and_then(|v| v.as_array()) {
                            let vis_edges: Vec<serde_json::Value> = new_edges.iter().filter_map(|e| {
                                let from = e.get("from").and_then(|v| v.as_str())?;
                                let to = e.get("to").and_then(|v| v.as_str())?;
                                let rel = e.get("relationship").or_else(|| e.get("label"))
                                    .and_then(|v| v.as_str()).unwrap_or("related_to");
                                Some(serde_json::json!({
                                    "from": from,
                                    "to": to,
                                    "label": rel,
                                }))
                            }).collect();
                            set_e.update(|existing| {
                                for edge in vis_edges {
                                    existing.push(edge);
                                }
                            });
                        }
                    }
                }
            }
        }) as Box<dyn FnMut(web_sys::CustomEvent)>);
        let _ = web_sys::window().unwrap().add_event_listener_with_callback(
            "engram-chat-graph",
            cb.as_ref().unchecked_ref(),
        );
        cb.forget();
    }

    // Listen for entity navigation from chat cards
    {
        let set_hl = set_highlighted_node.clone();
        let cb = Closure::wrap(Box::new(move |ev: web_sys::CustomEvent| {
            if let Some(entity) = ev.detail().as_string() {
                set_hl.set(Some(entity.clone()));
                // Focus on node in graph
                let code = format!(
                    "window.__engram_graph && window.__engram_graph.focusNode('{}')",
                    entity.replace('\'', "\\'"),
                );
                let _ = js_sys::eval(&code);
            }
        }) as Box<dyn FnMut(web_sys::CustomEvent)>);
        let _ = web_sys::window().unwrap().add_event_listener_with_callback(
            "engram-navigate",
            cb.as_ref().unchecked_ref(),
        );
        cb.forget();
    }

    // Listen for detail modal open from chat cards
    {
        let set_dmo = set_detail_modal_open.clone();
        let set_dni = set_detail_node_id.clone();
        let cb = Closure::wrap(Box::new(move |ev: web_sys::CustomEvent| {
            if let Some(entity) = ev.detail().as_string() {
                set_dni.set(Some(entity));
                set_dmo.set(true);
            }
        }) as Box<dyn FnMut(web_sys::CustomEvent)>);
        let _ = web_sys::window().unwrap().add_event_listener_with_callback(
            "engram-open-detail",
            cb.as_ref().unchecked_ref(),
        );
        cb.forget();
    }

    // Show hint when graph loads with nodes, auto-dismiss after 5s
    Effect::new(move |_| {
        let n = nodes.get();
        if !n.is_empty() {
            set_show_hint.set(true);
            let _ = js_sys::eval("setTimeout(function(){if(window.__engram_hint_dismiss){window.__engram_hint_dismiss()}},5000)");
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

        // New layout: graph canvas (flex:1) + embedded chat panel (420px)
        <div class="explore-layout"
            style="display:flex;gap:0;height:calc(100vh - var(--nav-height, 56px) - 7rem);">
            // Left: graph canvas
            <div class="graph-canvas" style="flex:1;position:relative;background:var(--bg-secondary);\
                        border:1px solid var(--border);border-radius:var(--radius, 8px) 0 0 var(--radius, 8px);\
                        overflow:hidden;">
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

            // Right: integrated chat panel with compact controls
            <div class="explore-chat"
                style="width:420px;display:flex;flex-direction:column;\
                       border-left:1px solid var(--border);border-right:1px solid var(--border);\
                       border-top:1px solid var(--border);border-bottom:1px solid var(--border);\
                       border-radius:0 var(--radius, 8px) var(--radius, 8px) 0;\
                       background:var(--bg-secondary);flex-shrink:0;overflow:hidden;">
                // Compact controls bar
                <ExploreControls
                    depth=depth
                    set_depth=set_depth
                    min_confidence=min_strength
                    set_min_confidence=set_min_strength
                    show_edge_labels=show_edge_labels
                    set_show_edge_labels=set_show_edge_labels
                    temporal_current_only=temporal_current_only
                    set_temporal_current_only=set_temporal_current_only
                    edge_bundling=edge_bundling
                    set_edge_bundling=set_edge_bundling
                />
                // Chat panel (always visible, embedded)
                <div style="flex:1;display:flex;flex-direction:column;overflow:hidden;">
                    <ChatPanel embedded=true />
                </div>
            </div>
        </div>

        <CrudModal open=crud_open on_close=close_crud on_created=on_crud_created />
        <DetailModal open=detail_modal_open_signal node_id=detail_node_id_signal on_close=close_detail />
    }
}
