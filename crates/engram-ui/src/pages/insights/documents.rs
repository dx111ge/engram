use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::api::ApiClient;
use crate::api::types::{DocumentsResponse, ReprocessResponse};

#[component]
pub fn DocumentsZone(
    #[prop(into)] set_status_msg: WriteSignal<String>,
) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (processing, set_processing) = signal(false);
    let (result_msg, set_result_msg) = signal(Option::<String>::None);
    let (progress_msg, set_progress_msg) = signal(Option::<String>::None);
    let (refresh_trigger, set_refresh_trigger) = signal(0u32);

    let api_docs = api.clone();
    let docs = LocalResource::new(move || {
        let api = api_docs.clone();
        let _ = refresh_trigger.get(); // re-fetch when trigger changes
        async move {
            let body = serde_json::json!({ "limit": 100 });
            api.post::<_, DocumentsResponse>("/documents", &body).await.ok()
        }
    });

    // SSE subscription for ingest_progress events
    {
        let base_url = api.base_url.clone();
        Effect::new(move |_| {
            if !processing.get() {
                return;
            }

            let url = format!("{}/events/stream?topics=ingest_progress", base_url);
            let source = match web_sys::EventSource::new(&url) {
                Ok(s) => s,
                Err(_) => return,
            };

            let set_prog = set_progress_msg;
            let set_proc = set_processing;
            let set_refresh = set_refresh_trigger;

            let on_event = Closure::wrap(Box::new(move |evt: web_sys::MessageEvent| {
                if let Some(data) = evt.data().as_string() {
                    // Parse the stage from the SSE data
                    let stage = data.clone();
                    set_prog.set(Some(stage.clone()));

                    // Check if processing is complete
                    if data.contains("complete") || data.contains("error") {
                        set_proc.set(false);
                        set_refresh.update(|c| *c += 1);
                    }
                }
            }) as Box<dyn FnMut(web_sys::MessageEvent)>);

            // Listen for the typed "ingest_progress" event
            let _ = source.add_event_listener_with_callback(
                "ingest_progress",
                on_event.as_ref().unchecked_ref(),
            );
            on_event.forget();

            // Also listen for generic messages (fallback)
            let set_prog2 = set_progress_msg;
            let set_proc2 = set_processing;
            let set_refresh2 = set_refresh_trigger;
            let on_msg = Closure::wrap(Box::new(move |evt: web_sys::MessageEvent| {
                if let Some(data) = evt.data().as_string() {
                    if data.contains("IngestProgress") || data.contains("ingest_progress") {
                        set_prog2.set(Some(data.clone()));
                        if data.contains("complete") || data.contains("error") {
                            set_proc2.set(false);
                            set_refresh2.update(|c| *c += 1);
                        }
                    }
                }
            }) as Box<dyn FnMut(web_sys::MessageEvent)>);
            source.set_onmessage(Some(on_msg.as_ref().unchecked_ref()));
            on_msg.forget();

            // Close after 5 minutes max to avoid leaking connections
            let source_clone = source.clone();
            let timeout = Closure::wrap(Box::new(move || {
                source_clone.close();
            }) as Box<dyn FnMut()>);
            let _ = web_sys::window().unwrap().set_timeout_with_callback_and_timeout_and_arguments_0(
                timeout.as_ref().unchecked_ref(), 300_000,
            );
            timeout.forget();
        });
    }

    let api_reprocess = api.clone();
    let do_reprocess = Action::new_local(move |_: &()| {
        let api = api_reprocess.clone();
        set_processing.set(true);
        set_result_msg.set(None);
        set_progress_msg.set(None);
        async move {
            let body = serde_json::json!({ "batch_size": 20 });
            match api.post::<_, ReprocessResponse>("/ingest/reprocess-docs", &body).await {
                Ok(r) => {
                    let msg = format!(
                        "{} documents: {} -- {}",
                        r.documents_found, r.status, r.message
                    );
                    set_result_msg.set(Some(msg));
                    if r.status == "complete" {
                        set_processing.set(false);
                        set_refresh_trigger.update(|c| *c += 1);
                    }
                }
                Err(e) => {
                    set_result_msg.set(Some(format!("Error: {e}")));
                    set_processing.set(false);
                }
            }
        }
    });

    view! {
        <div class="card mt-2">
            <div style="display: flex; justify-content: space-between; align-items: center;">
                <h3><i class="fa-solid fa-file-pdf" style="color: var(--accent-bright);"></i>" Documents"</h3>
                <button
                    class="btn btn-sm btn-primary"
                    disabled=move || processing.get()
                    on:click=move |_| { let _ = do_reprocess.dispatch(()); }
                >
                    {move || if processing.get() {
                        view! { <><i class="fa-solid fa-spinner fa-spin"></i>" Processing..."</> }.into_any()
                    } else {
                        view! { <><i class="fa-solid fa-arrows-rotate"></i>" Process Pending"</> }.into_any()
                    }}
                </button>
            </div>

            {move || result_msg.get().map(|m| view! {
                <div class="card" style="padding: 0.5rem; margin-top: 0.75rem; margin-bottom: 0.5rem; background: var(--bg-tertiary);">
                    <i class="fa-solid fa-info-circle" style="color: var(--accent-bright);"></i>
                    " " {m}
                </div>
            })}

            // Live progress from SSE
            {move || progress_msg.get().map(|m| view! {
                <div style="padding: 0.25rem 0.5rem; font-size: 0.8rem; color: var(--text-secondary);">
                    <i class="fa-solid fa-signal" style="color: var(--accent-bright);"></i>
                    " " {m}
                </div>
            })}

            {move || {
                let data = docs.get().flatten();
                match data {
                    None => view! {
                        <p class="text-secondary mt-1" style="font-size: 0.85rem;">
                            <i class="fa-solid fa-spinner fa-spin"></i>" Loading documents..."
                        </p>
                    }.into_any(),
                    Some(resp) if resp.documents.is_empty() => view! {
                        <p class="text-secondary mt-1" style="font-size: 0.85rem;">
                            "No documents ingested yet. Run a debate or add a source to collect documents."
                        </p>
                    }.into_any(),
                    Some(resp) => {
                        let pending: Vec<_> = resp.documents.iter()
                            .filter(|d| !d.ner_complete)
                            .cloned()
                            .collect();
                        let processed: Vec<_> = resp.documents.iter()
                            .filter(|d| d.ner_complete)
                            .cloned()
                            .collect();
                        let pending_count = pending.len();
                        let processed_count = processed.len();

                        view! {
                            <div class="flex gap-sm mt-1 mb-1" style="flex-wrap: wrap;">
                                <span class="badge" style={
                                    if pending_count > 0 { "background: var(--warning); color: var(--bg-primary);" }
                                    else { "" }
                                }>
                                    {format!("{} pending", pending_count)}
                                </span>
                                <span class="badge badge-active">
                                    {format!("{} processed", processed_count)}
                                </span>
                            </div>

                            {if !pending.is_empty() {
                                Some(view! {
                                    <details open=true style="margin-bottom: 1rem;">
                                        <summary class="text-secondary" style="cursor: pointer; font-size: 0.85rem; margin-bottom: 0.5rem;">
                                            <i class="fa-solid fa-clock" style="color: var(--warning);"></i>
                                            " Pending ("{pending_count}")"
                                        </summary>
                                        <table class="data-table">
                                            <thead><tr>
                                                <th>"Label"</th>
                                                <th>"Title"</th>
                                                <th>"URL"</th>
                                                <th>"Lang"</th>
                                            </tr></thead>
                                            <tbody>
                                                {pending.into_iter().map(|d| {
                                                    let label = d.label.clone();
                                                    let title = if d.title.is_empty() { "-".to_string() } else { d.title.clone() };
                                                    let url = if d.url.is_empty() { "-".to_string() } else { d.url.clone() };
                                                    let lang = if d.original_language.is_empty() { "-".to_string() } else { d.original_language.clone() };
                                                    view! {
                                                        <tr>
                                                            <td><code style="font-size: 0.8rem;">{label}</code></td>
                                                            <td>{title}</td>
                                                            <td style="max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{url}</td>
                                                            <td>{lang}</td>
                                                        </tr>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </tbody>
                                        </table>
                                    </details>
                                })
                            } else { None }}

                            {if !processed.is_empty() {
                                Some(view! {
                                    <details style="margin-bottom: 0.5rem;">
                                        <summary class="text-secondary" style="cursor: pointer; font-size: 0.85rem; margin-bottom: 0.5rem;">
                                            <i class="fa-solid fa-check-circle" style="color: var(--success);"></i>
                                            " Processed ("{processed_count}")"
                                        </summary>
                                        <table class="data-table">
                                            <thead><tr>
                                                <th>"Label"</th>
                                                <th>"Title"</th>
                                                <th>"Facts"</th>
                                                <th>"Lang"</th>
                                            </tr></thead>
                                            <tbody>
                                                {processed.into_iter().map(|d| {
                                                    let label = d.label.clone();
                                                    let title = if d.title.is_empty() { "-".to_string() } else { d.title.clone() };
                                                    let facts = d.fact_count;
                                                    let lang = if d.original_language.is_empty() { "en".to_string() } else { d.original_language.clone() };
                                                    view! {
                                                        <tr>
                                                            <td><code style="font-size: 0.8rem;">{label}</code></td>
                                                            <td>{title}</td>
                                                            <td>{facts}</td>
                                                            <td>{lang}</td>
                                                        </tr>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </tbody>
                                        </table>
                                    </details>
                                })
                            } else { None }}
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}
