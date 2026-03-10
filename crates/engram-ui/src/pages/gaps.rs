use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{BlackArea, GapsResponse};
use crate::components::warning_banner::WarningBanner;

#[component]
pub fn GapsPage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (gaps, set_gaps) = signal(Vec::<BlackArea>::new());
    let (total, set_total) = signal(0usize);
    let (loading, set_loading) = signal(false);

    let api_c = api.clone();
    let scan = Action::new_local(move |_: &()| {
        let api = api_c.clone();
        async move {
            set_loading.set(true);
            match api.get::<GapsResponse>("/reason/gaps").await {
                Ok(resp) => {
                    set_total.set(resp.report.total_gaps);
                    set_gaps.set(resp.gaps);
                }
                Err(_) => {
                    set_gaps.set(vec![]);
                    set_total.set(0);
                }
            }
            set_loading.set(false);
        }
    });

    scan.dispatch(());

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-map"></i>" Knowledge Gaps"</h2>
            <div class="page-actions">
                <button
                    class="btn btn-primary"
                    on:click=move |_| { scan.dispatch(()); }
                    disabled=move || loading.get()
                >
                    <i class="fa-solid fa-satellite-dish"></i>
                    {move || if loading.get() { " Scanning..." } else { " Scan" }}
                </button>
            </div>
        </div>

        <WarningBanner message="Suggested queries are mechanically generated from graph structure. They are NOT AI-generated and may not be meaningful." />

        <div class="stat-row">
            <span class="stat-inline"><strong>"Total gaps: "</strong>{move || total.get().to_string()}</span>
        </div>

        <div class="table-wrapper">
            <table class="data-table">
                <thead>
                    <tr>
                        <th>"Kind"</th>
                        <th>"Severity"</th>
                        <th>"Entities"</th>
                        <th>"Suggested Queries"</th>
                    </tr>
                </thead>
                <tbody>
                    <For
                        each={move || gaps.get()}
                        key={|g| format!("{}-{}", g.kind, g.entities.join(","))}
                        children={move |gap| {
                            let sev_class = if gap.severity >= 0.7 {
                                "severity-high"
                            } else if gap.severity >= 0.4 {
                                "severity-medium"
                            } else {
                                "severity-low"
                            };
                            let entities_text = {
                                let mut s = gap.entities.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
                                if gap.entities.len() > 5 {
                                    s.push_str(&format!(" (+{})", gap.entities.len() - 5));
                                }
                                s
                            };
                            let queries = gap.suggested_queries.iter().take(3).cloned().collect::<Vec<_>>();
                            view! {
                                <tr>
                                    <td><span class="badge">{gap.kind.clone()}</span></td>
                                    <td>
                                        <span class=sev_class>
                                            {format!("{:.2}", gap.severity)}
                                        </span>
                                    </td>
                                    <td><div class="entity-list">{entities_text}</div></td>
                                    <td>
                                        <ul class="suggestion-list">
                                            {queries.into_iter().map(|q| view! {
                                                <li>{q}</li>
                                            }).collect::<Vec<_>>()}
                                        </ul>
                                    </td>
                                </tr>
                            }
                        }}
                    />
                </tbody>
            </table>
        </div>
    }
}
