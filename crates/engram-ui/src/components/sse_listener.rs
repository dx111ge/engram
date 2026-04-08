use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::api::types::GraphEvent;

/// Invisible component that connects to SSE and pushes events into a signal.
#[component]
pub fn SseListener(
    /// SSE endpoint URL (e.g., "/events/stream?filter=store")
    #[prop(into)] endpoint: Signal<String>,
    /// Signal to push events into
    on_event: WriteSignal<Option<GraphEvent>>,
) -> impl IntoView {
    // Use Rc<RefCell> since EventSource is not Send (WASM is single-threaded)
    let event_source = std::rc::Rc::new(std::cell::RefCell::new(Option::<web_sys::EventSource>::None));

    let es_clone = event_source.clone();
    Effect::new(move |_| {
        let event_source = &es_clone;
        let url = endpoint.get();

        // Close existing connection
        if let Some(old) = event_source.borrow_mut().take() {
            old.close();
        }

        if url.is_empty() {
            return;
        }

        let api = use_context::<crate::api::ApiClient>();
        let full_url = match api {
            Some(client) => format!("{}{}", client.base_url, url),
            None => url,
        };
        // EventSource can't send Authorization headers; pass token as query param
        let full_url = {
            let sep = if full_url.contains('?') { "&" } else { "?" };
            match crate::api::ApiClient::auth_token() {
                Some(t) => format!("{}{}token={}", full_url, sep, js_sys::encode_uri_component(&t)),
                None => full_url,
            }
        };

        let source = match web_sys::EventSource::new(&full_url) {
            Ok(s) => s,
            Err(_) => return,
        };

        let on_message = Closure::wrap(Box::new(move |evt: web_sys::MessageEvent| {
            if let Some(data) = evt.data().as_string() {
                if let Ok(event) = serde_json::from_str::<GraphEvent>(&data) {
                    on_event.set(Some(event));
                }
            }
        }) as Box<dyn FnMut(web_sys::MessageEvent)>);

        source.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();

        *event_source.borrow_mut() = Some(source);
    });

    // In WASM, page lifetime is managed by the browser; no on_cleanup needed.

    view! {}
}
