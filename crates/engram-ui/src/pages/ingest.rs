use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::GraphEvent;
use crate::components::sse_listener::SseListener;

#[component]
pub fn IngestPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (ingest_text, set_ingest_text) = signal(String::new());
    let (result_msg, set_result_msg) = signal(String::new());
    let (loading, set_loading) = signal(false);
    let (sse_event, set_sse_event) = signal(Option::<GraphEvent>::None);
    let (event_log, set_event_log) = signal(Vec::<String>::new());

    // Track SSE events in the log
    Effect::new(move |_| {
        if let Some(evt) = sse_event.get() {
            set_event_log.update(|log| {
                log.push(format!("[{}] {:?}", evt.event_type, evt.data));
                if log.len() > 100 {
                    log.drain(..50);
                }
            });
        }
    });

    let api_c = api.clone();
    let do_ingest = Action::new_local(move |_: &()| {
        let api = api_c.clone();
        let text = ingest_text.get_untracked();
        async move {
            set_loading.set(true);
            let body = serde_json::json!({"text": text});
            match api.post_text("/ingest", &body).await {
                Ok(r) => set_result_msg.set(format!("Ingested: {r}")),
                Err(e) => set_result_msg.set(format!("Error: {e}")),
            }
            set_loading.set(false);
        }
    });

    let sse_endpoint = Signal::derive(|| "/events/stream".to_string());

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-gears"></i>" Ingest Pipeline"</h2>
        </div>

        <SseListener endpoint=sse_endpoint on_event=set_sse_event />

        {move || {
            let msg = result_msg.get();
            (!msg.is_empty()).then(|| view! {
                <div class="alert">{msg}</div>
            })
        }}

        <div class="ingest-grid">
            <div class="card">
                <h3>"Text Ingest"</h3>
                <textarea
                    class="code-area"
                    placeholder="Paste text to ingest..."
                    prop:value=ingest_text
                    on:input=move |ev| set_ingest_text.set(event_target_value(&ev))
                />
                <button
                    class="btn btn-primary"
                    on:click=move |_| { do_ingest.dispatch(()); }
                    disabled=move || loading.get()
                >
                    {move || if loading.get() { "Ingesting..." } else { "Ingest" }}
                </button>
            </div>

            <div class="card">
                <h3><i class="fa-solid fa-list"></i>" Live Event Log"</h3>
                <div class="event-log">
                    <For
                        each={move || event_log.get().into_iter().enumerate().collect::<Vec<_>>()}
                        key={|(i, _)| *i}
                        children={move |(_, entry)| {
                            view! { <div class="log-entry">{entry}</div> }
                        }}
                    />
                </div>
            </div>
        </div>
    }
}
