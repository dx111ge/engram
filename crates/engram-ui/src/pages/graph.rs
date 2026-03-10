use leptos::prelude::*;

use crate::api::ApiClient;
use crate::components::graph_canvas::GraphCanvas;

#[component]
pub fn GraphPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (nodes, _set_nodes) = signal(Vec::<serde_json::Value>::new());
    let (edges, _set_edges) = signal(Vec::<serde_json::Value>::new());
    let (selected_node, set_selected_node) = signal(Option::<String>::None);
    let (search_term, set_search_term) = signal(String::new());

    // Load graph data
    let api_c = api.clone();
    let load_graph = Action::new_local(move |_: &()| {
        let api = api_c.clone();
        async move {
            if let Ok(stats) = api.get::<crate::api::types::StatsResponse>("/stats").await {
                let _ = stats;
            }
        }
    });

    load_graph.dispatch(());

    let on_select = Callback::new(move |node_id: String| {
        set_selected_node.set(Some(node_id));
    });

    let nodes_signal = Signal::derive(move || nodes.get());
    let edges_signal = Signal::derive(move || edges.get());

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-diagram-project"></i>" Knowledge Graph"</h2>
            <div class="page-actions">
                <input
                    type="text"
                    placeholder="Search nodes..."
                    class="search-input"
                    prop:value=search_term
                    on:input=move |ev| set_search_term.set(event_target_value(&ev))
                />
                <button class="btn btn-primary" on:click=move |_| { load_graph.dispatch(()); }>
                    <i class="fa-solid fa-refresh"></i>" Reload"
                </button>
            </div>
        </div>

        <div class="graph-wrapper">
            <GraphCanvas nodes=nodes_signal edges=edges_signal on_select_node=on_select />
        </div>

        {move || selected_node.get().map(|node_id| view! {
            <div class="node-inspector">
                <h3><i class="fa-solid fa-circle-info"></i>" Node: " {node_id}</h3>
            </div>
        })}
    }
}
