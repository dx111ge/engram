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
    let (processing_label, set_processing_label) = signal(Option::<String>::None);
    let (result_msg, set_result_msg) = signal(Option::<String>::None);
    let (progress_msg, set_progress_msg) = signal(Option::<String>::None);
    let (refresh_trigger, set_refresh_trigger) = signal(0u32);
    let (page, set_page) = signal(0usize);
    let (pending_page, set_pending_page) = signal(0usize);
    let page_size: usize = 25;
    // Custom confirm dialog state: holds label to delete
    let (confirm_delete, set_confirm_delete) = signal(Option::<String>::None);

    let api_docs = api.clone();
    let docs = LocalResource::new(move || {
        let api = api_docs.clone();
        let _ = refresh_trigger.get(); // re-fetch when trigger changes
        async move {
            let body = serde_json::json!({ "limit": 500 });
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

            let token_param = crate::api::ApiClient::auth_token()
                .map(|t| format!("&token={}", js_sys::encode_uri_component(&t)))
                .unwrap_or_default();
            let url = format!("{}/events/stream?topics=ingest_progress{}", base_url, token_param);
            let source = match web_sys::EventSource::new(&url) {
                Ok(s) => s,
                Err(_) => return,
            };

            let set_prog = set_progress_msg;
            let set_proc = set_processing;
            let set_proc_label = set_processing_label;
            let set_refresh = set_refresh_trigger;

            let on_event = Closure::wrap(Box::new(move |evt: web_sys::MessageEvent| {
                if let Some(data) = evt.data().as_string() {
                    let stage = data.clone();
                    set_prog.set(Some(stage.clone()));

                    // Check if processing is complete
                    if data.contains("complete") || data.contains("error") {
                        set_proc.set(false);
                        set_proc_label.set(None);
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
            let set_proc_label2 = set_processing_label;
            let set_refresh2 = set_refresh_trigger;
            let on_msg = Closure::wrap(Box::new(move |evt: web_sys::MessageEvent| {
                if let Some(data) = evt.data().as_string() {
                    if data.contains("IngestProgress") || data.contains("ingest_progress") {
                        set_prog2.set(Some(data.clone()));
                        if data.contains("complete") || data.contains("error") {
                            set_proc2.set(false);
                            set_proc_label2.set(None);
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
            let body = serde_json::json!({ "batch_size": 5 });
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

    let api_confirm = api.clone();

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
                                        {
                                            let pending_total_pages = (pending_count + page_size - 1) / page_size;
                                            let pending_cp = pending_page.get().min(pending_total_pages.saturating_sub(1));
                                            let pending_items: Vec<_> = pending.into_iter()
                                                .skip(pending_cp * page_size)
                                                .take(page_size)
                                                .collect();
                                            view! {
                                        <div>
                                        <table class="data-table">
                                            <thead><tr>
                                                <th>"Label"</th>
                                                <th>"Title"</th>
                                                <th>"URL"</th>
                                                <th>"Lang"</th>
                                                <th></th>
                                            </tr></thead>
                                            <tbody>
                                                {pending_items.into_iter().map(|d| {
                                                    let label = d.label.clone();
                                                    let label_for_btn = d.label.clone();
                                                    let label_for_del = d.label.clone();
                                                    let title = if d.title.is_empty() { "-".to_string() } else { d.title.clone() };
                                                    let url = if d.url.is_empty() { "-".to_string() } else { d.url.clone() };
                                                    let lang = if d.original_language.is_empty() { "-".to_string() } else { d.original_language.clone() };
                                                    let api_single = api.clone();
                                                    let do_process_single = Action::new_local(move |_: &()| {
                                                        let api = api_single.clone();
                                                        let lbl = label_for_btn.clone();
                                                        let encoded = js_sys::encode_uri_component(&lbl).as_string().unwrap_or(lbl.clone());
                                                        set_processing.set(true);
                                                        set_processing_label.set(Some(lbl.clone()));
                                                        set_progress_msg.set(Some(format!("Processing {}...", lbl)));
                                                        async move {
                                                            match api.post_text(&format!("/ingest/reprocess-doc/{}", encoded), &()).await {
                                                                Ok(_) => {}
                                                                Err(e) => {
                                                                    set_result_msg.set(Some(format!("Error: {e}")));
                                                                    set_processing.set(false);
                                                                    set_processing_label.set(None);
                                                                }
                                                            }
                                                        }
                                                    });
                                                    let label_check = label.clone();
                                                    view! {
                                                        <tr>
                                                            <td><code style="font-size: 0.8rem;">{label}</code></td>
                                                            <td>{title}</td>
                                                            <td style="max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                                                                {if url == "-" {
                                                                    view! { <span>"-"</span> }.into_any()
                                                                } else {
                                                                    let href = url.clone();
                                                                    view! { <a href=href target="_blank" rel="noopener" style="color: var(--accent-bright);">{url}</a> }.into_any()
                                                                }}
                                                            </td>
                                                            <td>{lang}</td>
                                                            <td style="white-space: nowrap;">
                                                                <button
                                                                    class="btn-icon"
                                                                    title="Process this document"
                                                                    disabled=move || processing.get()
                                                                    on:click=move |_| { let _ = do_process_single.dispatch(()); }
                                                                    style="color: var(--success);"
                                                                >
                                                                    {move || {
                                                                        let is_this = processing_label.get().as_deref() == Some(&label_check);
                                                                        if is_this {
                                                                            view! { <i class="fa-solid fa-spinner fa-spin"></i> }.into_any()
                                                                        } else {
                                                                            view! { <i class="fa-solid fa-play"></i> }.into_any()
                                                                        }
                                                                    }}
                                                                </button>
                                                                {
                                                                    let del_label = label_for_del.clone();
                                                                    view! {
                                                                        <button
                                                                            class="btn-icon icon-danger"
                                                                            title="Delete this document"
                                                                            style="margin-left: 0.25rem;"
                                                                            disabled=move || processing.get()
                                                                            on:click=move |_| {
                                                                                set_confirm_delete.set(Some(del_label.clone()));
                                                                            }
                                                                        >
                                                                            <i class="fa-solid fa-trash"></i>
                                                                        </button>
                                                                    }
                                                                }
                                                            </td>
                                                        </tr>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </tbody>
                                        </table>
                                        // Pending pagination
                                        {(pending_total_pages > 1).then(|| {
                                            let pcp = pending_cp;
                                            let ptp = pending_total_pages;
                                            let is_last_pending = move || pending_page.get() + 1 >= ptp;
                                            view! {
                                                <div style="display: flex; justify-content: center; gap: 0.5rem; margin-top: 0.5rem; align-items: center;">
                                                    <button class="btn btn-sm"
                                                        disabled=move || pending_page.get() == 0
                                                        on:click=move |_| set_pending_page.update(|p| *p = p.saturating_sub(1))>
                                                        <i class="fa-solid fa-chevron-left"></i>
                                                    </button>
                                                    <span style="font-size: 0.8rem;">{format!("Page {} of {}", pcp + 1, ptp)}</span>
                                                    <button class="btn btn-sm"
                                                        disabled=is_last_pending
                                                        on:click=move |_| set_pending_page.update(|p| *p += 1)>
                                                        <i class="fa-solid fa-chevron-right"></i>
                                                    </button>
                                                </div>
                                            }
                                        })}
                                        </div>
                                        }
                                        }
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
                                        {
                                            let total_pages = (processed_count + page_size - 1) / page_size;
                                            let current_page = page.get().min(total_pages.saturating_sub(1));
                                            let page_items: Vec<_> = processed.into_iter()
                                                .skip(current_page * page_size)
                                                .take(page_size)
                                                .collect();
                                            view! {
                                                <table class="data-table">
                                                    <thead><tr>
                                                        <th>"Label"</th>
                                                        <th>"Title"</th>
                                                        <th>"URL"</th>
                                                        <th>"Facts"</th>
                                                        <th>"Lang"</th>
                                                        <th></th>
                                                    </tr></thead>
                                                    <tbody>
                                                        {page_items.into_iter().map(|d| {
                                                            let label = d.label.clone();
                                                            let title = if d.title.is_empty() { "-".to_string() } else { d.title.clone() };
                                                            let url = if d.url.is_empty() { "-".to_string() } else { d.url.clone() };
                                                            let facts = d.fact_count;
                                                            let lang = if d.original_language.is_empty() { "en".to_string() } else { d.original_language.clone() };
                                                            let del_label = label.clone();
                                                            view! {
                                                                <tr>
                                                                    <td><code style="font-size: 0.8rem;">{label}</code></td>
                                                                    <td>{title}</td>
                                                                    <td style="max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                                                                        {if url == "-" {
                                                                            view! { <span>"-"</span> }.into_any()
                                                                        } else {
                                                                            let href = url.clone();
                                                                            view! { <a href=href target="_blank" rel="noopener" style="color: var(--accent-bright);">{url}</a> }.into_any()
                                                                        }}
                                                                    </td>
                                                                    <td>{facts}</td>
                                                                    <td>{lang}</td>
                                                                    <td>
                                                                        <button class="btn-icon icon-danger" title="Delete"
                                                                            on:click=move |_| {
                                                                                set_confirm_delete.set(Some(del_label.clone()));
                                                                            }
                                                                        >
                                                                            <i class="fa-solid fa-trash"></i>
                                                                        </button>
                                                                    </td>
                                                                </tr>
                                                            }
                                                        }).collect::<Vec<_>>()}
                                                    </tbody>
                                                </table>
                                                // Pagination controls
                                                {(total_pages > 1).then(|| {
                                                    let cp = current_page;
                                                    let tp = total_pages;
                                                    let is_last_proc = move || page.get() + 1 >= tp;
                                                    view! {
                                                        <div style="display: flex; justify-content: center; gap: 0.5rem; margin-top: 0.5rem; align-items: center;">
                                                            <button class="btn btn-sm"
                                                                disabled=move || page.get() == 0
                                                                on:click=move |_| set_page.update(|p| *p = p.saturating_sub(1))>
                                                                <i class="fa-solid fa-chevron-left"></i>
                                                            </button>
                                                            <span style="font-size: 0.8rem;">{format!("Page {} of {}", cp + 1, tp)}</span>
                                                            <button class="btn btn-sm"
                                                                disabled=is_last_proc
                                                                on:click=move |_| set_page.update(|p| *p += 1)>
                                                                <i class="fa-solid fa-chevron-right"></i>
                                                            </button>
                                                        </div>
                                                    }
                                                })}
                                            }
                                        }
                                    </details>
                                })
                            } else { None }}
                        }.into_any()
                    }
                }
            }}

            // Custom styled confirm dialog for delete
            {
                let api_confirm = api_confirm.clone();
                move || confirm_delete.get().map(|label| {
                let label_for_action = label.clone();
                let api_del = api_confirm.clone();
                let do_confirmed_delete = Action::new_local(move |_: &()| {
                    let api = api_del.clone();
                    let lbl = label_for_action.clone();
                    let encoded = js_sys::encode_uri_component(&lbl).as_string().unwrap_or(lbl.clone());
                    async move {
                        match api.delete(&format!("/documents/{}", encoded)).await {
                            Ok(_) => set_refresh_trigger.update(|c| *c += 1),
                            Err(e) => set_result_msg.set(Some(format!("Delete error: {e}"))),
                        }
                        set_confirm_delete.set(None);
                    }
                });
                view! {
                    <div class="modal-overlay" style="position: fixed; top: 0; left: 0; width: 100vw; height: 100vh; background: rgba(0,0,0,0.6); z-index: 9999; display: flex; align-items: center; justify-content: center;">
                        <div class="card" style="min-width: 360px; max-width: 480px; padding: 1.5rem; border: 1px solid var(--border-light);">
                            <h4 style="margin-bottom: 1rem;">
                                <i class="fa-solid fa-triangle-exclamation" style="color: var(--warning);"></i>
                                " Confirm Delete"
                            </h4>
                            <p style="margin-bottom: 1rem; color: var(--text-secondary);">
                                "Delete document " <code>{label.clone()}</code> "? Facts extracted from this document will be kept."
                            </p>
                            <div style="display: flex; gap: 0.5rem; justify-content: flex-end;">
                                <button class="btn btn-secondary"
                                    on:click=move |_| set_confirm_delete.set(None)>
                                    "Cancel"
                                </button>
                                <button class="btn btn-danger"
                                    on:click=move |_| { let _ = do_confirmed_delete.dispatch(()); }>
                                    <i class="fa-solid fa-trash"></i>" Delete"
                                </button>
                            </div>
                        </div>
                    </div>
                }
            })
            }
        </div>
    }
}

