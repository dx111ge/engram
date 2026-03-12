use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::ApiClient;
use crate::api::types::{QueryRequest, QueryResponse, SearchRequest, SearchResponse, NodeResponse};
use crate::components::graph_canvas::GraphCanvas;
use crate::components::crud_modal::CrudModal;

#[component]
pub fn GraphPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (nodes, set_nodes) = signal(Vec::<serde_json::Value>::new());
    let (edges, set_edges) = signal(Vec::<serde_json::Value>::new());
    let (selected_node, set_selected_node) = signal(Option::<String>::None);
    let (node_detail, set_node_detail) = signal(Option::<NodeResponse>::None);
    let (search_term, set_search_term) = signal(String::new());
    let (depth, set_depth) = signal(2u32);
    let (min_strength, set_min_strength) = signal(0.0f32);
    let (direction, set_direction) = signal("both".to_string());
    let (crud_open, set_crud_open) = signal(false);
    let (loading, set_loading) = signal(false);

    // Load graph: first search for matching nodes, then traverse from top result
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

            // Step 1: fulltext search to find matching node labels
            let search_body = SearchRequest { query: query.clone(), limit: Some(50) };
            let start_label = match api.post::<_, SearchResponse>("/search", &search_body).await {
                Ok(sr) if !sr.results.is_empty() => sr.results[0].label.clone(),
                _ => {
                    // Fallback: try the query text as an exact label
                    query
                }
            };

            // Step 2: traverse graph from the found node
            let body = QueryRequest {
                query: start_label,
                limit: Some(100),
                depth: Some(d),
                direction: Some(dir),
                min_confidence: if min_c > 0.0 { Some(min_c) } else { None },
            };
            match api.post::<_, QueryResponse>("/query", &body).await {
                Ok(result) => {
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
                        serde_json::json!({
                            "from": e.from,
                            "to": e.to,
                            "label": e.relationship,
                            "title": format!("{:.0}%", e.confidence * 100.0),
                        })
                    }).collect();

                    set_nodes.set(vis_nodes);
                    set_edges.set(vis_edges);
                }
                Err(_) => {
                    set_nodes.set(Vec::new());
                    set_edges.set(Vec::new());
                }
            }
            set_loading.set(false);
        }
    });

    // Load node details on click
    let api_detail = api.clone();
    let on_select = Callback::new(move |node_id: String| {
        set_selected_node.set(Some(node_id.clone()));
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
                        let new_edge = serde_json::json!({
                            "from": e.from,
                            "to": e.to,
                            "label": e.relationship,
                        });
                        existing.push(new_edge);
                    }
                });
            }
        });
    });

    let nodes_signal = Signal::derive(move || nodes.get());
    let edges_signal = Signal::derive(move || edges.get());

    let close_crud = Callback::new(move |_: ()| set_crud_open.set(false));
    let on_crud_created = Callback::new(move |_: ()| {
        do_search.dispatch(());
    });

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-diagram-project"></i>" Knowledge Graph"</h2>
            <div class="page-actions flex gap-sm">
                <input
                    type="text"
                    placeholder="Search or ask a question..."
                    class="search-input"
                    style="width: 250px;"
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
                            <p class="text-secondary" style="font-size: 0.85rem;">"Click a node to see details. Double-click to expand."</p>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <GraphCanvas
                            nodes=nodes_signal
                            edges=edges_signal
                            on_select_node=on_select
                            on_double_click=on_double_click
                        />
                    }.into_any()
                }}
            </div>

            <div class="graph-sidebar">
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
                </div>

                // Node detail panel
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
                    let encoded = js_sys::encode_uri_component(&label).as_string().unwrap_or_default();
                    view! {
                        <div class="card node-info-panel">
                            <div class="flex-between">
                                <h3>{detail.label.clone()}</h3>
                                <a href=format!("/node/{encoded}") class="btn btn-sm btn-secondary">
                                    <i class="fa-solid fa-expand"></i>" Detail"
                                </a>
                            </div>
                            {detail.node_type.clone().map(|t| view! {
                                <span class="badge badge-active">{t}</span>
                            })}
                            <div class="prop-row mt-1">
                                <span class="prop-key">"Confidence"</span>
                                <span>{format!("{:.0}%", detail.confidence * 100.0)}</span>
                            </div>
                            <div class="prop-row">
                                <span class="prop-key">"Outgoing"</span>
                                <span>{detail.edges_from.len().to_string()}</span>
                            </div>
                            <div class="prop-row">
                                <span class="prop-key">"Incoming"</span>
                                <span>{detail.edges_to.len().to_string()}</span>
                            </div>
                            {detail.properties.clone().and_then(|p| {
                                p.as_object().map(|obj| {
                                    obj.iter().take(5).map(|(k, v)| view! {
                                        <div class="prop-row">
                                            <span class="prop-key">{k.clone()}</span>
                                            <span style="font-size: 0.8rem;">{v.to_string()}</span>
                                        </div>
                                    }).collect::<Vec<_>>()
                                })
                            })}
                        </div>
                    }
                })}
            </div>
        </div>

        <CrudModal open=crud_open on_close=close_crud on_created=on_crud_created />
    }
}
