mod assessments;
mod conflicts;
mod documents;
mod gaps;

use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::StatsResponse;
use assessments::AssessmentsZone;
use conflicts::ConflictsZone;
use documents::DocumentsZone;
use gaps::GapsZone;

#[component]
pub fn InsightsPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let (status_msg, set_status_msg) = signal(String::new());

    let api_stats = api.clone();
    let stats = LocalResource::new(move || {
        let api = api_stats.clone();
        async move { api.get::<StatsResponse>("/stats").await.ok() }
    });

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-chart-line"></i>" Insights"</h2>
            <p class="text-secondary">"Analytics, assessments, contradictions, and knowledge gaps"</p>
        </div>

        {move || {
            let msg = status_msg.get();
            (!msg.is_empty()).then(|| view! {
                <div class="alert">{msg.clone()}</div>
            })
        }}

        // ── Stats overview ──
        <div class="card">
            <h3><i class="fa-solid fa-database" style="color: var(--accent-bright);"></i>" Knowledge Stats"</h3>
            <div class="flex gap-sm mt-1" style="flex-wrap: wrap;">
                {move || {
                    stats.get().flatten().map(|s| view! {
                        <span class="badge badge-active" style="font-size: 0.85rem;">{format!("{} nodes", s.nodes)}</span>
                        <span class="badge badge-active" style="font-size: 0.85rem;">{format!("{} connections", s.edges)}</span>
                        <span class="badge badge-active" style="font-size: 0.85rem;">{format!("{} documents", s.documents)}</span>
                    })
                }}
            </div>
        </div>

        // ── Contradictions & Fact Review ──
        <ConflictsZone set_status_msg />

        // Zone A: Documents (pending/processed)
        <DocumentsZone set_status_msg />

        // Zone B: Assessments (primary)
        <AssessmentsZone set_status_msg />

        // Zone C: Intelligence Gaps (secondary)
        <GapsZone set_status_msg />
    }
}
