use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{
    HealthResponse, StatsResponse, ComputeResponse, AnalyzeResponse, AnalyzeRequest,
    IngestRequest, IngestItem, IngestResponse, SourceInfo, KbEndpointInfo,
};
use crate::components::stat_card::StatCard;

#[component]
pub fn Dashboard() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let api1 = api.clone();
    let api2 = api.clone();
    let api3 = api.clone();
    let api_sources = api.clone();

    // Version signal to trigger stats re-fetch after seed
    let (stats_version, set_stats_version) = signal(0u32);

    let health = LocalResource::new(move || {
        let api = api1.clone();
        async move { api.get::<HealthResponse>("/health").await.ok() }
    });

    let stats = LocalResource::new(move || {
        let _v = stats_version.get(); // re-run when version changes
        let api = api2.clone();
        async move { api.get::<StatsResponse>("/stats").await.ok() }
    });

    let compute = LocalResource::new(move || {
        let api = api3.clone();
        async move { api.get::<ComputeResponse>("/compute").await.ok() }
    });

    let sources = LocalResource::new(move || {
        let api = api_sources.clone();
        async move { api.get::<Vec<SourceInfo>>("/sources").await.ok() }
    });

    let api_kb = api.clone();
    let kb_endpoints = LocalResource::new(move || {
        let api = api_kb.clone();
        async move {
            // API returns {"endpoints": [...]} wrapper
            #[derive(Clone, Debug, serde::Deserialize)]
            struct KbResponse { #[serde(default)] endpoints: Vec<KbEndpointInfo> }
            api.get::<KbResponse>("/config/kb").await.ok().map(|r| r.endpoints)
        }
    });

    let api_cfg = api.clone();
    let config = LocalResource::new(move || {
        let api = api_cfg.clone();
        async move {
            api.get::<serde_json::Value>("/config").await.ok()
        }
    });

    // 4 stat cards: Facts stored, Connections, Documents, Status
    let fact_count = Signal::derive(move || {
        stats.get().flatten().map(|s| s.nodes.to_string()).unwrap_or_else(|| "--".into())
    });
    let connection_count = Signal::derive(move || {
        stats.get().flatten().map(|s| s.edges.to_string()).unwrap_or_else(|| "--".into())
    });
    let document_count = Signal::derive(move || {
        stats.get().flatten().map(|s| s.documents.to_string()).unwrap_or_else(|| "--".into())
    });
    let status_text = Signal::derive(move || {
        health.get().flatten().map(|h| {
            if h.status == "ok" { "Online".to_string() } else { "Offline".to_string() }
        }).unwrap_or_else(|| "Offline".into())
    });

    // Hardware card details
    let api_status = Signal::derive(move || {
        health.get().flatten().map(|h| h.status == "ok").unwrap_or(false)
    });

    let cpu_info = move || {
        compute.get().flatten().map(|c| {
            let cores = c.cpu_cores.map(|n| format!("{n} cores")).unwrap_or_else(|| "-- cores".into());
            let avx2 = if c.has_avx2 == Some(true) { " + AVX2" } else { "" };
            format!("{cores}{avx2}")
        })
    };

    let gpu_info = move || {
        compute.get().flatten().and_then(|c| {
            if c.gpu_available {
                let name = c.gpu_name.unwrap_or_else(|| "Available".into());
                let backend = c.gpu_backend.map(|b| format!(" ({b})")).unwrap_or_default();
                Some(format!("{name}{backend}"))
            } else {
                Some("Not available".into())
            }
        })
    };

    let npu_info = move || {
        compute.get().flatten().map(|c| {
            if c.npu_available {
                c.npu_name.unwrap_or_else(|| "Available".into())
            } else {
                "Not available".into()
            }
        })
    };

    // NER / Add Knowledge
    let (seed_text, set_seed_text) = signal(String::new());
    let (analyze_result, set_analyze_result) = signal(Option::<AnalyzeResponse>::None);
    let (seed_msg, set_seed_msg) = signal(Option::<String>::None);

    let api_analyze = api.clone();
    let do_analyze = Action::new_local(move |_: &()| {
        let api = api_analyze.clone();
        let text = seed_text.get_untracked();
        async move {
            set_analyze_result.set(None);
            set_seed_msg.set(None);
            let body = AnalyzeRequest { text };
            match api.post::<_, AnalyzeResponse>("/ingest/analyze", &body).await {
                Ok(result) => set_analyze_result.set(Some(result)),
                Err(e) => set_seed_msg.set(Some(format!("Analysis failed: {e}"))),
            }
        }
    });

    let api_ingest = api.clone();
    let do_seed = Action::new_local(move |_: &()| {
        let api = api_ingest.clone();
        let text = seed_text.get_untracked();
        async move {
            set_seed_msg.set(None);
            let body = IngestRequest {
                items: vec![IngestItem { content: text, source_url: None }],
                source: Some("kb-seed".into()),
                skip: None,
            };
            match api.post::<_, IngestResponse>("/ingest", &body).await {
                Ok(r) => {
                    set_seed_msg.set(Some(format!(
                        "Seeded: {} facts, {} relations ({}ms)",
                        r.facts_stored, r.relations_created, r.duration_ms
                    )));
                    set_analyze_result.set(None);
                    set_seed_text.set(String::new());
                    // Trigger stats refresh
                    set_stats_version.update(|v| *v += 1);
                }
                Err(e) => set_seed_msg.set(Some(format!("Ingest failed: {e}"))),
            }
        }
    });

    // Example statements for empty graphs
    let is_empty = move || {
        stats.get().flatten().map(|s| s.nodes == 0).unwrap_or(true)
    };

    let examples = vec![
        "PostgreSQL is a relational database management system developed by the PostgreSQL Global Development Group.",
        "Rust is a systems programming language that runs blazingly fast and prevents segfaults.",
        "Berlin is the capital of Germany and has a population of approximately 3.7 million people.",
    ];

    view! {
        // ── Header ──
        <div class="page-header">
            <h2><i class="fa-solid fa-brain"></i>" Your Knowledge Base"</h2>
            <p class="text-secondary">"Explore connections and discover insights."</p>
        </div>

        // ── 3 Stat Cards ──
        <div class="stat-grid">
            <StatCard icon="fa-solid fa-database" label="Facts stored" value=fact_count />
            <StatCard icon="fa-solid fa-arrows-left-right" label="Connections" value=connection_count />
            <StatCard icon="fa-solid fa-file-lines" label="Documents" value=document_count />
            <StatCard icon="fa-solid fa-signal" label="Status" value=status_text />
        </div>

        // ── Hardware Card ──
        <div class="card mt-2">
            <h3><i class="fa-solid fa-server" style="color: var(--accent-bright);"></i>" Hardware"</h3>
            <table class="mt-1" style="width: 100%; border-collapse: collapse;">
                <tbody>
                    <tr>
                        <td style="padding: 0.4rem 0; font-weight: 600; width: 120px;">"API"</td>
                        <td style="padding: 0.4rem 0;">
                            {move || if api_status.get() {
                                view! { <span style="color: var(--success);">"Online"</span> }.into_any()
                            } else {
                                view! { <span style="color: var(--danger);">"Offline"</span> }.into_any()
                            }}
                        </td>
                    </tr>
                    <tr>
                        <td style="padding: 0.4rem 0; font-weight: 600;">"CPU"</td>
                        <td style="padding: 0.4rem 0;">
                            {move || cpu_info().unwrap_or_else(|| "--".into())}
                        </td>
                    </tr>
                    <tr>
                        <td style="padding: 0.4rem 0; font-weight: 600;">"GPU"</td>
                        <td style="padding: 0.4rem 0;">
                            {move || gpu_info().unwrap_or_else(|| "--".into())}
                        </td>
                    </tr>
                    <tr>
                        <td style="padding: 0.4rem 0; font-weight: 600;">"NPU"</td>
                        <td style="padding: 0.4rem 0;">
                            {move || npu_info().unwrap_or_else(|| "--".into())}
                        </td>
                    </tr>
                </tbody>
            </table>
            // Feature dots
            <div class="mt-1" style="display: flex; gap: 1.5rem; flex-wrap: wrap; padding-top: 0.5rem; border-top: 1px solid var(--border);">
                <span style="display: flex; align-items: center; gap: 0.4rem;">
                    <i class="fa-solid fa-circle" style="font-size: 0.5rem; color: var(--success);"></i>
                    " Ingest"
                </span>
                <span style="display: flex; align-items: center; gap: 0.4rem;">
                    <i class="fa-solid fa-circle" style="font-size: 0.5rem; color: var(--success);"></i>
                    " Actions"
                </span>
                <span style="display: flex; align-items: center; gap: 0.4rem;">
                    <i class="fa-solid fa-circle" style="font-size: 0.5rem; color: var(--success);"></i>
                    " Reasoning"
                </span>
                <span style="display: flex; align-items: center; gap: 0.4rem;">
                    <i class="fa-solid fa-circle" style="font-size: 0.5rem; color: var(--success);"></i>
                    " Mesh"
                </span>
            </div>
        </div>

        // ── Active Sources Card ──
        <div class="card mt-2">
            <div style="display: flex; justify-content: space-between; align-items: center;">
                <h3><i class="fa-solid fa-plug" style="color: var(--accent-bright);"></i>" Active Sources"</h3>
                <a href="/sources" class="btn btn-sm btn-secondary">
                    <i class="fa-solid fa-plus"></i>" Add Source"
                </a>
            </div>
            {move || {
                let src_list = sources.get().flatten().unwrap_or_default();
                let kb_list = kb_endpoints.get().flatten().unwrap_or_default();
                let kb_enabled: Vec<_> = kb_list.iter().filter(|k| k.enabled).collect();
                let web_search = config.get().flatten()
                    .and_then(|c| c.get("web_search_provider").and_then(|v| v.as_str()).map(|s| s.to_string()));
                let has_any = !src_list.is_empty() || !kb_enabled.is_empty() || web_search.is_some();
                if !has_any {
                    view! {
                        <p class="text-secondary mt-1" style="font-size: 0.9rem;">"No sources configured yet."</p>
                    }.into_any()
                } else {
                    let ws_name = web_search.clone().map(|p| match p.as_str() {
                        "brave" => "Brave Search".to_string(),
                        "searxng" => "SearXNG".to_string(),
                        _ => "DuckDuckGo".to_string(),
                    });
                    view! {
                        <table class="mt-1">
                            <thead><tr><th>"Name"</th><th>"Type"</th><th>"Status"</th></tr></thead>
                            <tbody>
                                {src_list.iter().map(|s| view! {
                                    <tr>
                                        <td>{s.name.clone()}</td>
                                        <td class="text-secondary">{s.source_type.clone().unwrap_or_default()}</td>
                                        <td>{s.total_ingested.unwrap_or(0).to_string()}" ingested"</td>
                                    </tr>
                                }).collect::<Vec<_>>()}
                                {kb_enabled.iter().map(|k| view! {
                                    <tr>
                                        <td><i class="fa-solid fa-database" style="margin-right: 0.3rem; color: var(--accent);"></i>{k.name.clone()}</td>
                                        <td class="text-secondary">"SPARQL endpoint"</td>
                                        <td><span style="color: #66bb6a;"><i class="fa-solid fa-circle" style="font-size: 0.5rem; margin-right: 0.3rem;"></i>"Active"</span></td>
                                    </tr>
                                }).collect::<Vec<_>>()}
                                {ws_name.map(|name| view! {
                                    <tr>
                                        <td><i class="fa-solid fa-magnifying-glass" style="margin-right: 0.3rem; color: var(--accent);"></i>{name}</td>
                                        <td class="text-secondary">"Web search"</td>
                                        <td><span style="color: #66bb6a;"><i class="fa-solid fa-circle" style="font-size: 0.5rem; margin-right: 0.3rem;"></i>"Active"</span></td>
                                    </tr>
                                })}
                            </tbody>
                        </table>
                    }.into_any()
                }
            }}
        </div>

        // ── Add Knowledge ──
        <div class="card mt-2">
            <h3><i class="fa-solid fa-plus-circle" style="color: var(--accent-bright);"></i>" Add Knowledge"</h3>
            <p class="text-secondary mt-1" style="font-size: 0.85rem;">
                "Enter text to extract entities and relationships, then add them to your knowledge graph."
            </p>
            <div class="form-group mt-1">
                <textarea
                    rows="4"
                    placeholder="Paste or type new information to analyze and add..."
                    prop:value=seed_text
                    on:input=move |ev| set_seed_text.set(event_target_value(&ev))
                />
            </div>

            {move || if is_empty() {
                Some(view! {
                    <div class="mb-1">
                        <p class="text-secondary" style="font-size: 0.85rem; margin-bottom: 0.5rem;">"Try these examples:"</p>
                        {examples.iter().map(|ex| {
                            let ex_str = ex.to_string();
                            let ex_clone = ex_str.clone();
                            view! {
                                <button class="btn btn-secondary btn-sm mb-1" style="display: block; text-align: left; white-space: normal;"
                                    on:click=move |_| set_seed_text.set(ex_clone.clone())>
                                    {ex_str}
                                </button>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                })
            } else { None }}

            <div class="flex gap-sm">
                <button class="btn btn-primary" on:click=move |_| { do_analyze.dispatch(()); }
                    disabled=move || seed_text.get().is_empty()>
                    <i class="fa-solid fa-magnifying-glass-chart"></i>" Analyze"
                </button>
                <button class="btn btn-success" on:click=move |_| { do_seed.dispatch(()); }
                    disabled=move || seed_text.get().is_empty()>
                    <i class="fa-solid fa-download"></i>" Seed KB"
                </button>
            </div>

            {move || seed_msg.get().map(|m| view! {
                <div class="mt-1" style="font-size: 0.85rem;">
                    <i class="fa-solid fa-info-circle" style="color: var(--accent-bright);"></i>
                    " " {m}
                </div>
            })}

            // Analysis results
            {move || analyze_result.get().map(|result| view! {
                <div class="mt-2">
                    <h4>"Extracted Entities (" {result.entities.len().to_string()} ")"</h4>
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
                        view! { <p class="text-muted mt-1">"No entities detected"</p> }.into_any()
                    }}

                    {if !result.relations.is_empty() {
                        Some(view! {
                            <h4 class="mt-2">"Extracted Relations (" {result.relations.len().to_string()} ")"</h4>
                            <table class="mt-1">
                                <thead><tr><th>"From"</th><th>"Relation"</th><th>"To"</th><th>"Confidence"</th></tr></thead>
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

                    {if !result.warnings.is_empty() {
                        Some(view! {
                            <div class="mt-1" style="color: var(--warning); font-size: 0.85rem;">
                                {result.warnings.iter().map(|w| view! {
                                    <p><i class="fa-solid fa-triangle-exclamation"></i>" " {w.clone()}</p>
                                }).collect::<Vec<_>>()}
                            </div>
                        })
                    } else { None }}

                    <p class="text-muted mt-1" style="font-size: 0.8rem;">
                        {format!("Language: {} | Duration: {}ms", result.language, result.duration_ms)}
                    </p>
                </div>
            })}
        </div>

        // ── Overview ──
        <div class="card mt-2">
            <h3><i class="fa-solid fa-chart-pie" style="color: var(--accent-bright);"></i>" Overview"</h3>
            <p class="mt-1">
                {move || {
                    let s = stats.get().flatten();
                    match s {
                        Some(st) => format!(
                            "{} facts stored with {} connections between them.",
                            st.nodes, st.edges
                        ),
                        None => "Loading...".to_string(),
                    }
                }}
            </p>
            <p class="mt-1">
                <a href="/graph" style="color: var(--accent-bright);">
                    "Explore now "<i class="fa-solid fa-arrow-right"></i>
                </a>
            </p>
        </div>
    }
}
