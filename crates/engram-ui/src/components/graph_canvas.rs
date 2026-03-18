use leptos::prelude::*;
use wasm_bindgen::prelude::*;

/// Graph visualization component wrapping 3d-force-graph via JS bridge.
#[component]
pub fn GraphCanvas(
    /// JSON array of nodes: [{id, label, color, size, title}]
    #[prop(into)] nodes: Signal<Vec<serde_json::Value>>,
    /// JSON array of edges: [{from, to, label, title}]
    #[prop(into)] edges: Signal<Vec<serde_json::Value>>,
    /// Callback when a node is clicked
    #[prop(optional)] on_select_node: Option<Callback<String>>,
    /// Callback when a node is right-clicked (expand)
    #[prop(optional)] on_double_click: Option<Callback<String>>,
    /// Set of hidden node types (lowercase)
    #[prop(into, optional)] hidden_types: Option<Signal<Vec<String>>>,
    /// Set of hidden relationship types (lowercase)
    #[prop(into, optional)] hidden_rels: Option<Signal<Vec<String>>>,
    /// ID of the start/root node for golden glow
    #[prop(into, optional)] start_node_id: Option<Signal<Option<String>>>,
    /// Whether to show edge labels
    #[prop(into, optional)] show_edge_labels: Option<Signal<bool>>,
    /// ID of node to highlight
    #[prop(into, optional)] highlighted_node: Option<Signal<Option<String>>>,
    /// Whether to show only current temporal edges
    #[prop(into, optional)] temporal_current_only: Option<Signal<bool>>,
) -> impl IntoView {
    let container_ref = NodeRef::<leptos::html::Div>::new();

    // Main effect: create/update graph when nodes/edges change
    Effect::new(move |_| {
        let Some(el) = container_ref.get() else { return };
        let el: &web_sys::HtmlElement = &el;

        let cur_nodes = nodes.get();
        let cur_edges = edges.get();

        if cur_nodes.is_empty() {
            // Destroy graph if empty
            let _ = js_sys::eval("window.__engram_graph && window.__engram_graph.destroy()");
            return;
        }

        // Transform edges: 3d-force-graph uses "source"/"target" not "from"/"to"
        let links: Vec<serde_json::Value> = cur_edges.iter().map(|e| {
            let mut link = serde_json::json!({
                "source": e.get("from").and_then(|v| v.as_str()).unwrap_or(""),
                "target": e.get("to").and_then(|v| v.as_str()).unwrap_or(""),
                "label": e.get("label").and_then(|v| v.as_str()).unwrap_or("related_to"),
            });
            // Pass through temporal fields
            if let Some(vf) = e.get("valid_from").and_then(|v| v.as_str()) {
                link.as_object_mut().unwrap().insert("valid_from".into(), serde_json::Value::String(vf.to_string()));
            }
            if let Some(vt) = e.get("valid_to").and_then(|v| v.as_str()) {
                link.as_object_mut().unwrap().insert("valid_to".into(), serde_json::Value::String(vt.to_string()));
            }
            link
        }).collect();

        let nodes_json = serde_json::to_string(&cur_nodes).unwrap_or_default();
        let links_json = serde_json::to_string(&links).unwrap_or_default();

        // Create click callback
        let click_cb: Option<Closure<dyn FnMut(String)>> = on_select_node.map(|cb| {
            Closure::wrap(Box::new(move |node_id: String| {
                cb.run(node_id);
            }) as Box<dyn FnMut(String)>)
        });

        // Create right-click (expand) callback
        let dbl_cb: Option<Closure<dyn FnMut(String)>> = on_double_click.map(|cb| {
            Closure::wrap(Box::new(move |node_id: String| {
                cb.run(node_id);
            }) as Box<dyn FnMut(String)>)
        });

        // Get start node ID
        let start_id = start_node_id
            .map(|s| s.get())
            .flatten()
            .unwrap_or_default();

        // Call the JS bridge using Function.apply with an array for 6 args
        let bridge = js_sys::Reflect::get(
            &wasm_bindgen::JsValue::from(web_sys::window().unwrap()),
            &"__engram_graph".into(),
        ).unwrap_or(JsValue::NULL);

        if !bridge.is_null() && !bridge.is_undefined() {
            let create_fn = js_sys::Reflect::get(&bridge, &"create".into())
                .unwrap_or(JsValue::NULL);
            if let Some(func) = create_fn.dyn_ref::<js_sys::Function>() {
                let click_js = click_cb.as_ref()
                    .map(|c| c.as_ref().clone())
                    .unwrap_or(JsValue::NULL);
                let dbl_js = dbl_cb.as_ref()
                    .map(|c| c.as_ref().clone())
                    .unwrap_or(JsValue::NULL);

                let args = js_sys::Array::new();
                args.push(&el.into());
                args.push(&nodes_json.into());
                args.push(&links_json.into());
                args.push(&click_js);
                args.push(&dbl_js);
                args.push(&JsValue::from_str(&start_id));

                let _ = func.apply(&bridge, &args);
            }
        }

        // Prevent closures from being dropped
        if let Some(c) = click_cb { c.forget(); }
        if let Some(c) = dbl_cb { c.forget(); }
    });

    // Effect: filter by hidden types/rels
    Effect::new(move |_| {
        let types = hidden_types.map(|s| s.get()).unwrap_or_default();
        let rels = hidden_rels.map(|s| s.get()).unwrap_or_default();
        let types_json = serde_json::to_string(&types).unwrap_or_default();
        let rels_json = serde_json::to_string(&rels).unwrap_or_default();
        // JSON uses double quotes, so wrapping in single quotes is safe
        let code = format!(
            "window.__engram_graph && window.__engram_graph.filter('{}', '{}')",
            types_json,
            rels_json,
        );
        let _ = js_sys::eval(&code);
    });

    // Effect: toggle edge labels
    Effect::new(move |_| {
        let show = show_edge_labels.map(|s| s.get()).unwrap_or(true);
        let code = format!(
            "window.__engram_graph && window.__engram_graph.toggleEdgeLabels({})",
            show
        );
        let _ = js_sys::eval(&code);
    });

    // Effect: highlight node
    Effect::new(move |_| {
        let node_id = highlighted_node.map(|s| s.get()).flatten().unwrap_or_default();
        let code = format!(
            "window.__engram_graph && window.__engram_graph.highlightNode('{}')",
            node_id.replace('\'', "\\'")
        );
        let _ = js_sys::eval(&code);
    });

    // Effect: temporal mode
    Effect::new(move |_| {
        let current_only = temporal_current_only.map(|s| s.get()).unwrap_or(false);
        let code = format!(
            "window.__engram_graph && window.__engram_graph.setTemporalMode({})",
            current_only
        );
        let _ = js_sys::eval(&code);
    });

    view! {
        <div node_ref=container_ref class="graph-container-canvas" style="width: 100%; height: 100%; min-height: 500px;"></div>
    }
}
