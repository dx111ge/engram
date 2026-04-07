use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{DocumentsResponse, ReprocessResponse};

#[component]
pub fn DocumentsZone(
    #[prop(into)] set_status_msg: WriteSignal<String>,
) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (processing, set_processing) = signal(false);
    let (result_msg, set_result_msg) = signal(Option::<String>::None);

    let api_docs = api.clone();
    let docs = LocalResource::new(move || {
        let api = api_docs.clone();
        async move {
            let body = serde_json::json!({ "limit": 100 });
            api.post::<_, DocumentsResponse>("/documents", &body).await.ok()
        }
    });

    let api_reprocess = api.clone();
    let do_reprocess = Action::new_local(move |_: &()| {
        let api = api_reprocess.clone();
        set_processing.set(true);
        set_result_msg.set(None);
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
                    }
                    // Keep spinner if background processing started
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
