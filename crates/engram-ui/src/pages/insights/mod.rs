mod assessments;
mod gaps;

use leptos::prelude::*;

use assessments::AssessmentsZone;
use gaps::GapsZone;

#[component]
pub fn InsightsPage() -> impl IntoView {
    let (status_msg, set_status_msg) = signal(String::new());

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-chart-line"></i>" Insights"</h2>
            <p class="text-secondary">"Track hypotheses, monitor intelligence gaps, guide collection"</p>
        </div>

        {move || {
            let msg = status_msg.get();
            (!msg.is_empty()).then(|| view! {
                <div class="alert">{msg.clone()}</div>
            })
        }}

        // Zone A: Assessments (primary)
        <AssessmentsZone set_status_msg />

        // Zone B: Intelligence Gaps (secondary)
        <GapsZone set_status_msg />
    }
}
