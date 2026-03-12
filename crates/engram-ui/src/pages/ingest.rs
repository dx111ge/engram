use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{AnalyzeResponse, GraphEvent, IngestResponse};
use crate::components::sse_listener::SseListener;

#[component]
pub fn IngestPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (ingest_text, set_ingest_text) = signal(String::new());
    let (result_msg, set_result_msg) = signal(String::new());
    let (loading, set_loading) = signal(false);
    let (sse_event, set_sse_event) = signal(Option::<GraphEvent>::None);
    let (event_log, set_event_log) = signal(Vec::<String>::new());
    let (analyze_mode, set_analyze_mode) = signal(false);
    let (analyze_result, set_analyze_result) = signal(Option::<AnalyzeResponse>::None);

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

    let api_ingest = api.clone();
    let do_ingest = Action::new_local(move |_: &()| {
        let api = api_ingest.clone();
        let text = ingest_text.get_untracked();
        async move {
            set_loading.set(true);
            set_analyze_result.set(None);
            let body = serde_json::json!({"text": text});
            match api.post::<_, IngestResponse>("/ingest", &body).await {
                Ok(r) => {
                    let msg = format!(
                        "Stored {} facts, {} relations, {} resolved ({} ms)",
                        r.facts_stored, r.relations_created, r.facts_resolved, r.duration_ms
                    );
                    set_result_msg.set(msg);
                }
                Err(e) => set_result_msg.set(format!("Error: {e}")),
            }
            set_loading.set(false);
        }
    });

    let api_analyze = api.clone();
    let do_analyze = Action::new_local(move |_: &()| {
        let api = api_analyze.clone();
        let text = ingest_text.get_untracked();
        async move {
            set_loading.set(true);
            let body = serde_json::json!({"text": text});
            match api.post::<_, AnalyzeResponse>("/ingest/analyze", &body).await {
                Ok(r) => {
                    set_result_msg.set(format!(
                        "Analyzed: {} entities, {} relations, lang={} ({} ms)",
                        r.entities.len(), r.relations.len(), r.language, r.duration_ms
                    ));
                    set_analyze_result.set(Some(r));
                }
                Err(e) => set_result_msg.set(format!("Analyze error: {e}")),
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
                <div class="button-group">
                    <label class="toggle-label">
                        <input
                            type="checkbox"
                            prop:checked=analyze_mode
                            on:change=move |_| set_analyze_mode.update(|v| *v = !*v)
                        />
                        " Analyze (preview)"
                    </label>
                    <button
                        class="btn btn-primary"
                        on:click=move |_| {
                            if analyze_mode.get_untracked() {
                                do_analyze.dispatch(());
                            } else {
                                do_ingest.dispatch(());
                            }
                        }
                        disabled=move || loading.get()
                    >
                        {move || {
                            if loading.get() {
                                if analyze_mode.get() { "Analyzing..." } else { "Ingesting..." }
                            } else if analyze_mode.get() {
                                "Analyze"
                            } else {
                                "Ingest"
                            }
                        }}
                    </button>
                </div>

                // Analyze preview results
                {move || analyze_result.get().map(|r| view! {
                    <div class="analyze-results">
                        <h4><i class="fa-solid fa-magnifying-glass"></i>" Extracted Entities"</h4>
                        <table class="data-table">
                            <thead>
                                <tr>
                                    <th>"Entity"</th>
                                    <th>"Type"</th>
                                    <th>"Confidence"</th>
                                    <th>"Method"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {r.entities.iter().map(|e| view! {
                                    <tr>
                                        <td>{e.text.clone()}</td>
                                        <td><span class="badge">{e.entity_type.clone()}</span></td>
                                        <td>{format!("{:.2}", e.confidence)}</td>
                                        <td>{e.method.clone()}</td>
                                    </tr>
                                }).collect::<Vec<_>>()}
                            </tbody>
                        </table>

                        {(!r.relations.is_empty()).then(|| view! {
                            <h4><i class="fa-solid fa-link"></i>" Extracted Relations"</h4>
                            <table class="data-table">
                                <thead>
                                    <tr>
                                        <th>"From"</th>
                                        <th>"Relation"</th>
                                        <th>"To"</th>
                                        <th>"Confidence"</th>
                                        <th>"Method"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {r.relations.iter().map(|rel| view! {
                                        <tr>
                                            <td>{rel.from.clone()}</td>
                                            <td><span class="badge badge-rel">{rel.rel_type.clone()}</span></td>
                                            <td>{rel.to.clone()}</td>
                                            <td>{format!("{:.2}", rel.confidence)}</td>
                                            <td>{rel.method.clone()}</td>
                                        </tr>
                                    }).collect::<Vec<_>>()}
                                </tbody>
                            </table>
                        })}
                    </div>
                })}
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
