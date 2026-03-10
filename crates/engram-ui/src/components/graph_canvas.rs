use leptos::prelude::*;
use wasm_bindgen::prelude::*;

// vis.js extern bindings -- vis-network loaded via CDN in index.html
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = vis, js_name = Network)]
    type VisNetwork;

    #[wasm_bindgen(constructor, js_namespace = vis, js_name = Network)]
    fn new(container: &web_sys::HtmlElement, data: &JsValue, options: &JsValue) -> VisNetwork;

    #[wasm_bindgen(method, js_name = setData)]
    fn set_data(this: &VisNetwork, data: &JsValue);

    #[wasm_bindgen(method)]
    fn on(this: &VisNetwork, event: &str, callback: &Closure<dyn FnMut(JsValue)>);

    #[wasm_bindgen(method)]
    fn destroy(this: &VisNetwork);

    #[wasm_bindgen(method)]
    fn fit(this: &VisNetwork);
}

/// Graph visualization component wrapping vis.js Network.
#[component]
pub fn GraphCanvas(
    /// JSON array of nodes: [{id, label, ...}]
    #[prop(into)] nodes: Signal<Vec<serde_json::Value>>,
    /// JSON array of edges: [{from, to, label, ...}]
    #[prop(into)] edges: Signal<Vec<serde_json::Value>>,
    /// Callback when a node is clicked
    #[prop(optional)] on_select_node: Option<Callback<String>>,
) -> impl IntoView {
    let container_ref = NodeRef::<leptos::html::Div>::new();

    // Use Rc<RefCell> to hold the vis.js network instance (not Send+Sync, WASM is single-threaded)
    let network = std::rc::Rc::new(std::cell::RefCell::new(Option::<VisNetwork>::None));

    let network_clone = network.clone();
    Effect::new(move |_| {
        let network = &network_clone;
        let Some(el) = container_ref.get() else { return };
        let el: &web_sys::HtmlElement = &el;

        // Build nodes and edges arrays
        let node_data = serde_json::to_string(&nodes.get()).unwrap_or_default();
        let edge_data = serde_json::to_string(&edges.get()).unwrap_or_default();

        let node_js = js_sys::JSON::parse(&node_data).unwrap_or(JsValue::NULL);
        let edge_js = js_sys::JSON::parse(&edge_data).unwrap_or(JsValue::NULL);

        // Build data object: { nodes: [...], edges: [...] }
        let data = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&data, &"nodes".into(), &node_js);
        let _ = js_sys::Reflect::set(&data, &"edges".into(), &edge_js);

        // Options
        let options_str = r#"{
            "physics": { "stabilization": { "iterations": 100 } },
            "nodes": {
                "shape": "dot",
                "size": 16,
                "font": { "size": 14 },
                "borderWidth": 2
            },
            "edges": {
                "arrows": "to",
                "font": { "size": 12, "align": "middle" },
                "smooth": { "type": "curvedCW", "roundness": 0.2 }
            },
            "interaction": { "hover": true, "tooltipDelay": 200 }
        }"#;
        let options = js_sys::JSON::parse(options_str).unwrap_or(JsValue::NULL);

        // Destroy previous instance
        if let Some(old) = network.borrow_mut().take() {
            old.destroy();
        }

        let net = VisNetwork::new(el, &data.into(), &options);

        // Node click handler
        if let Some(cb) = on_select_node {
            let closure = Closure::wrap(Box::new(move |params: JsValue| {
                if let Ok(nodes_val) = js_sys::Reflect::get(&params, &"nodes".into()) {
                    if let Some(arr) = nodes_val.dyn_ref::<js_sys::Array>() {
                        if let Some(first) = arr.get(0).as_string() {
                            cb.run(first);
                        }
                    }
                }
            }) as Box<dyn FnMut(JsValue)>);
            net.on("click", &closure);
            closure.forget();
        }

        *network.borrow_mut() = Some(net);
    });

    // In WASM, page lifetime is managed by the browser; no on_cleanup needed.

    view! {
        <div node_ref=container_ref class="graph-container" style="width: 100%; height: 600px;"></div>
    }
}
