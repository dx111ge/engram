use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{
    HealthResponse, StatsResponse, ComputeResponse,
};
use crate::components::stat_card::StatCard;

#[component]
pub fn Dashboard() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let api1 = api.clone();
    let api2 = api.clone();
    let api3 = api.clone();

    let health = LocalResource::new(move || {
        let api = api1.clone();
        async move { api.get::<HealthResponse>("/health").await.ok() }
    });

    let stats = LocalResource::new(move || {
        let api = api2.clone();
        async move { api.get::<StatsResponse>("/stats").await.ok() }
    });

    let compute = LocalResource::new(move || {
        let api = api3.clone();
        async move { api.get::<ComputeResponse>("/compute").await.ok() }
    });

    // Stat card values
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

    // Hardware summary (compact, single line)
    let hw_summary = Signal::derive(move || {
        compute.get().flatten().map(|c| {
            let gpu = if c.gpu_available {
                c.gpu_name.clone().unwrap_or_else(|| "GPU".into())
            } else {
                "No GPU".into()
            };
            let cores = c.cpu_cores.map(|n| format!("{n} cores")).unwrap_or_default();
            format!("{cores} | {gpu}")
        }).unwrap_or_else(|| "Loading...".into())
    });

    view! {
        // ── Header ──
        <div class="page-header">
            <h2><i class="fa-solid fa-gauge-high"></i>" Dashboard"</h2>
            <p class="text-secondary">"System status and quick actions."</p>
        </div>

        // ── Stat Cards ──
        <div class="stat-grid">
            <StatCard icon="fa-solid fa-database" label="Facts stored" value=fact_count />
            <StatCard icon="fa-solid fa-arrows-left-right" label="Connections" value=connection_count />
            <StatCard icon="fa-solid fa-file-lines" label="Documents" value=document_count />
            <StatCard icon="fa-solid fa-signal" label="Status" value=status_text />
        </div>

        // ── Quick Actions ──
        <div class="card mt-2">
            <h3><i class="fa-solid fa-bolt" style="color: var(--accent-bright);"></i>" Quick Actions"</h3>
            <div class="flex gap-sm mt-1" style="flex-wrap: wrap;">
                <a href="/knowledge" class="btn btn-primary">
                    <i class="fa-solid fa-plus-circle"></i>" Add Knowledge"
                </a>
                <a href="/debate" class="btn btn-secondary">
                    <i class="fa-solid fa-comments"></i>" Start Debate"
                </a>
                <a href="/ingest" class="btn btn-secondary">
                    <i class="fa-solid fa-file-import"></i>" Ingest Documents"
                </a>
                <a href="/insights" class="btn btn-secondary">
                    <i class="fa-solid fa-chart-line"></i>" View Insights"
                </a>
            </div>
        </div>

        // ── System Overview ──
        <div class="card mt-2">
            <h3><i class="fa-solid fa-server" style="color: var(--accent-bright);"></i>" System"</h3>
            <table class="mt-1" style="width: 100%; border-collapse: collapse;">
                <tbody>
                    <tr>
                        <td style="padding: 0.4rem 0; font-weight: 600; width: 140px;">"API"</td>
                        <td style="padding: 0.4rem 0;">
                            {move || if health.get().flatten().map(|h| h.status == "ok").unwrap_or(false) {
                                view! { <span style="color: var(--success);">"Online"</span> }.into_any()
                            } else {
                                view! { <span style="color: var(--danger);">"Offline"</span> }.into_any()
                            }}
                        </td>
                    </tr>
                    <tr>
                        <td style="padding: 0.4rem 0; font-weight: 600;">"Hardware"</td>
                        <td style="padding: 0.4rem 0;">{hw_summary}</td>
                    </tr>
                    <tr>
                        <td style="padding: 0.4rem 0; font-weight: 600;">"Knowledge"</td>
                        <td style="padding: 0.4rem 0;">
                            {move || {
                                stats.get().flatten().map(|s| format!(
                                    "{} facts, {} connections, {} documents",
                                    s.nodes, s.edges, s.documents
                                )).unwrap_or_else(|| "Loading...".into())
                            }}
                        </td>
                    </tr>
                </tbody>
            </table>
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

        // ── Explore Knowledge link ──
        <div class="card mt-2">
            <h3><i class="fa-solid fa-brain" style="color: var(--accent-bright);"></i>" Knowledge Graph"</h3>
            <p class="mt-1 text-secondary">
                "Search, explore, and add to your knowledge graph."
            </p>
            <p class="mt-1">
                <a href="/knowledge" style="color: var(--accent-bright);">
                    "Explore now "<i class="fa-solid fa-arrow-right"></i>
                </a>
            </p>
        </div>
    }
}
