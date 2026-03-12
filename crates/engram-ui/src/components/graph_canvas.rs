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
) -> impl IntoView {
    let container_ref = NodeRef::<leptos::html::Div>::new();

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
            serde_json::json!({
                "source": e.get("from").and_then(|v| v.as_str()).unwrap_or(""),
                "target": e.get("to").and_then(|v| v.as_str()).unwrap_or(""),
                "label": e.get("label").and_then(|v| v.as_str()).unwrap_or(""),
            })
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

        // Call the JS bridge
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

                let _ = func.call5(
                    &bridge,
                    &el.into(),
                    &nodes_json.into(),
                    &links_json.into(),
                    &click_js,
                    &dbl_js,
                );
            }
        }

        // Prevent closures from being dropped
        if let Some(c) = click_cb { c.forget(); }
        if let Some(c) = dbl_cb { c.forget(); }
    });

    view! {
        <div node_ref=container_ref class="graph-container-canvas" style="width: 100%; height: 100%; min-height: 500px;"></div>
    }
}
