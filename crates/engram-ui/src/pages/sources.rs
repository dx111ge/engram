use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{SourceInfo, AnalyzeRequest, AnalyzeResponse, IngestRequest, IngestItem, IngestResponse};
use crate::components::source_wizard::SourceWizard;

#[component]
pub fn SourcesPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (sources, set_sources) = signal(Vec::<SourceInfo>::new());
    let (wizard_open, set_wizard_open) = signal(false);
    let (refresh_counter, set_refresh_counter) = signal(0u32);

    // Load sources
    let api_load = api.clone();
    let load_sources = Action::new_local(move |_: &()| {
        let api = api_load.clone();
        async move {
            if let Ok(list) = api.get::<Vec<SourceInfo>>("/sources").await {
                set_sources.set(list);
            }
        }
    });

    Effect::new(move |_| {
        let _ = refresh_counter.get();
        load_sources.dispatch(());
    });

    let close_wizard = Callback::new(move |_: ()| set_wizard_open.set(false));
    let on_wizard_created = Callback::new(move |_: ()| {
        set_refresh_counter.update(|c| *c += 1);
        set_wizard_open.set(false);
    });

    // Dry preview / test ingest
    let (preview_text, set_preview_text) = signal(String::new());
    let (preview_result, set_preview_result) = signal(Option::<AnalyzeResponse>::None);
    let (ingest_msg, set_ingest_msg) = signal(Option::<String>::None);

    let api_analyze = api.clone();
    let do_analyze = Action::new_local(move |_: &()| {
        let api = api_analyze.clone();
        let text = preview_text.get_untracked();
        async move {
            set_preview_result.set(None);
            set_ingest_msg.set(None);
            let body = AnalyzeRequest { text };
            match api.post::<_, AnalyzeResponse>("/ingest/analyze", &body).await {
                Ok(result) => set_preview_result.set(Some(result)),
                Err(e) => set_ingest_msg.set(Some(format!("Error: {e}"))),
            }
        }
    });

    let api_ingest = api.clone();
    let do_ingest = Action::new_local(move |_: &()| {
        let api = api_ingest.clone();
        let text = preview_text.get_untracked();
        async move {
            set_ingest_msg.set(None);
            let body = IngestRequest {
                items: vec![IngestItem { content: text, source_url: None }],
                source: Some("manual".into()),
                skip: None,
            };
            match api.post::<_, IngestResponse>("/ingest", &body).await {
                Ok(r) => {
                    set_ingest_msg.set(Some(format!(
                        "Ingested: {} facts, {} relations ({}ms)",
                        r.facts_stored, r.relations_created, r.duration_ms
                    )));
                    set_preview_result.set(None);
                    set_preview_text.set(String::new());
                    set_refresh_counter.update(|c| *c += 1);
                }
                Err(e) => set_ingest_msg.set(Some(format!("Error: {e}"))),
            }
        }
    });

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-plug"></i>" Sources"</h2>
            <div class="page-actions">
                <button class="btn btn-primary" on:click=move |_| set_wizard_open.set(true)>
                    <i class="fa-solid fa-plus"></i>" Add Source"
                </button>
            </div>
        </div>

        // Active sources table
        <div class="card mb-2">
            <h3><i class="fa-solid fa-list" style="color: var(--accent-bright);"></i>" Configured Sources"</h3>
            {move || {
                let src = sources.get();
                if src.is_empty() {
                    view! {
                        <div class="empty-state">
                            <i class="fa-solid fa-plug"></i>
                            <p>"No sources configured. Add one to start ingesting data."</p>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div class="table-wrap mt-1">
                            <table>
                                <thead>
                                    <tr>
                                        <th>"Name"</th>
                                        <th>"Type"</th>
                                        <th>"Status"</th>
                                        <th>"Ingested"</th>
                                        <th>"Last Run"</th>
                                        <th>"Errors"</th>
                                        <th></th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {src.iter().map(|s| {
                                        let status = s.status.clone().unwrap_or_else(|| "unknown".into());
                                        let status_class = match status.as_str() {
                                            "active" => "badge badge-core",
                                            "paused" => "badge badge-active",
                                            "error" => "badge badge-archival",
                                            _ => "badge badge-active",
                                        };
                                        let name_test = s.name.clone();
                                        let name_run = s.name.clone();
                                        let name_del = s.name.clone();
                                        let api_test = api.clone();
                                        let api_run = api.clone();
                                        let api_del = api.clone();
                                        let (test_msg, set_test_msg) = signal(Option::<String>::None);
                                        let set_rc = set_refresh_counter;
                                        let do_test = Action::new_local(move |_: &()| {
                                            let api = api_test.clone();
                                            let name = name_test.clone();
                                            async move {
                                                set_test_msg.set(Some("testing...".into()));
                                                match api.post_text(&format!("/sources/{}/test", name), &()).await {
                                                    Ok(r) => set_test_msg.set(Some(r)),
                                                    Err(e) => set_test_msg.set(Some(format!("Error: {e}"))),
                                                }
                                            }
                                        });
                                        let do_run = Action::new_local(move |_: &()| {
                                            let api = api_run.clone();
                                            let name = name_run.clone();
                                            async move {
                                                set_test_msg.set(Some("running...".into()));
                                                match api.post_text(&format!("/sources/{}/run", name), &()).await {
                                                    Ok(r) => {
                                                        set_test_msg.set(Some(r));
                                                        set_rc.update(|c| *c += 1);
                                                    }
                                                    Err(e) => set_test_msg.set(Some(format!("Error: {e}"))),
                                                }
                                            }
                                        });
                                        let do_delete = Action::new_local(move |_: &()| {
                                            let api = api_del.clone();
                                            let name = name_del.clone();
                                            async move {
                                                match api.delete(&format!("/sources/{}", name)).await {
                                                    Ok(_) => set_rc.update(|c| *c += 1),
                                                    Err(e) => set_test_msg.set(Some(format!("Error: {e}"))),
                                                }
                                            }
                                        });
                                        view! {
                                            <tr>
                                                <td>{s.name.clone()}</td>
                                                <td class="text-secondary">{s.source_type.clone().unwrap_or_default()}</td>
                                                <td><span class=status_class>{status}</span></td>
                                                <td>{s.total_ingested.unwrap_or(0).to_string()}</td>
                                                <td class="text-muted">{s.last_run.clone().unwrap_or_else(|| "never".into())}</td>
                                                <td>{s.error_count.unwrap_or(0).to_string()}</td>
                                                <td style="white-space: nowrap;">
                                                    <button class="btn btn-sm btn-secondary" title="Test connectivity"
                                                        on:click=move |_| { let _ = do_test.dispatch(()); }>
                                                        <i class="fa-solid fa-plug-circle-check"></i>
                                                    </button>
                                                    " "
                                                    <button class="btn btn-sm btn-primary" title="Run now"
                                                        on:click=move |_| { let _ = do_run.dispatch(()); }>
                                                        <i class="fa-solid fa-play"></i>
                                                    </button>
                                                    " "
                                                    <button class="btn btn-sm btn-danger" title="Delete"
                                                        on:click=move |_| { let _ = do_delete.dispatch(()); }>
                                                        <i class="fa-solid fa-trash"></i>
                                                    </button>
                                                </td>
                                            </tr>
                                            {move || test_msg.get().map(|m| view! {
                                                <tr>
                                                    <td colspan="7" style="font-size: 0.8rem; padding: 0.25rem 0.5rem; background: var(--bg-tertiary);">
                                                        <code>{m}</code>
                                                    </td>
                                                </tr>
                                            })}
                                        }
                                    }).collect::<Vec<_>>()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                }
            }}
        </div>

        // Test / Dry preview
        <div class="card">
            <h3><i class="fa-solid fa-flask" style="color: var(--accent-bright);"></i>" Test Ingest"</h3>
            <p class="text-secondary mt-1" style="font-size: 0.85rem;">
                "Paste text to preview extraction or directly ingest."
            </p>
            <div class="form-group mt-1">
                <textarea
                    rows="4"
                    placeholder="Paste text content..."
                    prop:value=preview_text
                    on:input=move |ev| set_preview_text.set(event_target_value(&ev))
                />
            </div>
            <div class="flex gap-sm">
                <button class="btn btn-secondary" on:click=move |_| { do_analyze.dispatch(()); }
                    disabled=move || preview_text.get().is_empty()>
                    <i class="fa-solid fa-eye"></i>" Preview"
                </button>
                <button class="btn btn-success" on:click=move |_| { do_ingest.dispatch(()); }
                    disabled=move || preview_text.get().is_empty()>
                    <i class="fa-solid fa-download"></i>" Ingest"
                </button>
            </div>

            {move || ingest_msg.get().map(|m| view! {
                <div class="mt-1" style="font-size: 0.85rem;">
                    <i class="fa-solid fa-info-circle" style="color: var(--accent-bright);"></i>
                    " " {m}
                </div>
            })}

            {move || preview_result.get().map(|result| view! {
                <div class="mt-2">
                    <h4>"Entities (" {result.entities.len().to_string()} ")"</h4>
                    {if !result.entities.is_empty() {
                        view! {
                            <table class="mt-1">
                                <thead><tr><th>"Text"</th><th>"Type"</th><th>"Confidence"</th><th>"Method"</th></tr></thead>
                                <tbody>
                                    {result.entities.iter().map(|e| view! {
                                        <tr>
                                            <td>{e.text.clone()}</td>
                                            <td><span class="badge badge-active">{e.entity_type.clone()}</span></td>
                                            <td>{format!("{:.0}%", e.confidence * 100.0)}</td>
                                            <td class="text-muted">{e.method.clone()}</td>
                                        </tr>
                                    }).collect::<Vec<_>>()}
                                </tbody>
                            </table>
                        }.into_any()
                    } else {
                        view! { <p class="text-muted">"No entities"</p> }.into_any()
                    }}
                    {if !result.relations.is_empty() {
                        Some(view! {
                            <h4 class="mt-1">"Relations (" {result.relations.len().to_string()} ")"</h4>
                            <table class="mt-1">
                                <thead><tr><th>"From"</th><th>"Rel"</th><th>"To"</th><th>"Confidence"</th></tr></thead>
                                <tbody>
                                    {result.relations.iter().map(|r| view! {
                                        <tr>
                                            <td>{r.from.clone()}</td>
                                            <td class="text-secondary">{r.rel_type.clone()}</td>
                                            <td>{r.to.clone()}</td>
                                            <td>{format!("{:.0}%", r.confidence * 100.0)}</td>
                                        </tr>
                                    }).collect::<Vec<_>>()}
                                </tbody>
                            </table>
                        })
                    } else { None }}
                </div>
            })}
        </div>

        <SourceWizard open=wizard_open on_close=close_wizard on_created=on_wizard_created />
    }
}
