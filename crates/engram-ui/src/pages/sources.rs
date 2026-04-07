use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{SourceInfo, PollResultInfo, AnalyzeRequest, AnalyzeResponse, IngestRequest, IngestItem, IngestResponse};
use crate::components::source_wizard::SourceWizard;

/// Format seconds as human-readable interval (e.g. "5m", "1h 30m", "2d").
fn format_interval(secs: u64) -> String {
    if secs == 0 { return "now".into(); }
    if secs < 60 { return format!("{}s", secs); }
    if secs < 3600 { return format!("{}m", secs / 60); }
    if secs < 86400 {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        return if m > 0 { format!("{}h {}m", h, m) } else { format!("{}h", h) };
    }
    format!("{}d", secs / 86400)
}

/// Compute yield trend icon from poll history.
fn compute_trend(history: &[PollResultInfo]) -> &'static str {
    if history.len() < 2 { return ""; }
    let recent: Vec<u32> = history.iter().rev().take(3).map(|p| p.items_ingested).collect();
    if recent.len() < 2 { return ""; }
    if recent[0] > recent[1] { return "\u{25B2}"; } // up triangle
    if recent[0] < recent[1] { return "\u{25BC}"; } // down triangle
    ""
}

/// Format unix timestamp as HH:MM.
fn format_timestamp(ts: i64) -> String {
    let secs = ts % 86400;
    let h = (secs / 3600) % 24;
    let m = (secs % 3600) / 60;
    format!("{:02}:{:02}", h, m)
}

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
                                        <th>"Next Poll"</th>
                                        <th>"Interval"</th>
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
                                        // Schedule info
                                        let interval_display = s.schedule.as_ref()
                                            .map(|sc| format_interval(sc.interval_secs))
                                            .unwrap_or_else(|| "-".into());
                                        let next_poll_display = s.schedule.as_ref()
                                            .map(|sc| if sc.paused { "paused".into() } else { format_interval(sc.next_poll_in_secs as u64) })
                                            .unwrap_or_else(|| "-".into());
                                        let is_paused = s.schedule.as_ref().map(|sc| sc.paused).unwrap_or(false);
                                        // Yield trend from poll history
                                        let trend_icon = compute_trend(&s.poll_history);
                                        let history = s.poll_history.clone();

                                        let name_test = s.name.clone();
                                        let name_run = s.name.clone();
                                        let name_del = s.name.clone();
                                        let name_pause = s.name.clone();
                                        let api_test = api.clone();
                                        let api_run = api.clone();
                                        let api_del = api.clone();
                                        let api_pause = api.clone();
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
                                        let do_pause = Action::new_local(move |_: &()| {
                                            let api = api_pause.clone();
                                            let name = name_pause.clone();
                                            async move {
                                                match api.post_text(&format!("/sources/{}/pause", name), &()).await {
                                                    Ok(_) => set_rc.update(|c| *c += 1),
                                                    Err(e) => set_test_msg.set(Some(format!("Error: {e}"))),
                                                }
                                            }
                                        });
                                        let (show_history, set_show_history) = signal(false);
                                        view! {
                                            <tr>
                                                <td>{s.name.clone()}</td>
                                                <td class="text-secondary">{s.source_type.clone().unwrap_or_default()}</td>
                                                <td><span class=status_class>{status}</span></td>
                                                <td>{s.total_ingested.unwrap_or(0).to_string()}</td>
                                                <td class="text-muted" title="Next poll">
                                                    <i class="fa-solid fa-clock" style="opacity: 0.5; margin-right: 0.25rem;"></i>
                                                    {next_poll_display}
                                                </td>
                                                <td class="text-muted" title="Poll interval">
                                                    <i class="fa-solid fa-gauge-high" style="opacity: 0.5; margin-right: 0.25rem;"></i>
                                                    {interval_display}
                                                    " "
                                                    <span style="font-size: 0.8rem;">{trend_icon}</span>
                                                </td>
                                                <td style="white-space: nowrap;">
                                                    <button class="btn btn-sm btn-secondary" title={if is_paused { "Resume" } else { "Pause" }}
                                                        on:click=move |_| { let _ = do_pause.dispatch(()); }>
                                                        <i class={if is_paused { "fa-solid fa-play" } else { "fa-solid fa-pause" }}></i>
                                                    </button>
                                                    " "
                                                    <button class="btn btn-sm btn-secondary" title="Test"
                                                        on:click=move |_| { let _ = do_test.dispatch(()); }>
                                                        <i class="fa-solid fa-plug-circle-check"></i>
                                                    </button>
                                                    " "
                                                    <button class="btn btn-sm btn-primary" title="Run now"
                                                        on:click=move |_| { let _ = do_run.dispatch(()); }>
                                                        <i class="fa-solid fa-bolt"></i>
                                                    </button>
                                                    " "
                                                    <button class="btn btn-sm btn-secondary" title="History"
                                                        on:click=move |_| set_show_history.update(|v| *v = !*v)>
                                                        <i class="fa-solid fa-clock-rotate-left"></i>
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
                                            // Poll history expandable row
                                            {move || show_history.get().then(|| {
                                                let h = history.clone();
                                                view! {
                                                    <tr>
                                                        <td colspan="7" style="padding: 0.5rem; background: var(--bg-secondary); font-size: 0.8rem;">
                                                            {if h.is_empty() {
                                                                view! { <span class="text-muted">"No poll history yet"</span> }.into_any()
                                                            } else {
                                                                view! {
                                                                    <table class="data-table" style="margin: 0; font-size: 0.75rem;">
                                                                        <thead><tr>
                                                                            <th>"Time"</th>
                                                                            <th>"Fetched"</th>
                                                                            <th>"Deduped"</th>
                                                                            <th>"Filtered"</th>
                                                                            <th>"Ingested"</th>
                                                                            <th>"Facts"</th>
                                                                            <th>"Duration"</th>
                                                                            <th>"Status"</th>
                                                                        </tr></thead>
                                                                        <tbody>
                                                                            {h.iter().rev().map(|p| {
                                                                                let time = format_timestamp(p.timestamp);
                                                                                let dur = format!("{:.1}s", p.duration_ms as f64 / 1000.0);
                                                                                let status = if let Some(ref e) = p.error {
                                                                                    e.clone()
                                                                                } else {
                                                                                    "ok".into()
                                                                                };
                                                                                let status_style = if p.error.is_some() { "color: var(--danger);" } else { "color: var(--success);" };
                                                                                view! {
                                                                                    <tr>
                                                                                        <td>{time}</td>
                                                                                        <td>{p.items_fetched}</td>
                                                                                        <td>{p.items_deduped}</td>
                                                                                        <td>{p.items_filtered}</td>
                                                                                        <td>{p.items_ingested}</td>
                                                                                        <td>{p.facts_stored}</td>
                                                                                        <td>{dur}</td>
                                                                                        <td style=status_style>{status}</td>
                                                                                    </tr>
                                                                                }
                                                                            }).collect::<Vec<_>>()}
                                                                        </tbody>
                                                                    </table>
                                                                }.into_any()
                                                            }}
                                                        </td>
                                                    </tr>
                                                }
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
