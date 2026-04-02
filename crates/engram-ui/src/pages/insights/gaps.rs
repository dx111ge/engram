use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::GapsResponse;

const PAGE_SIZE: usize = 50;

#[component]
pub fn GapsZone(set_status_msg: WriteSignal<String>) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let (gaps_data, set_gaps_data) = signal(Option::<GapsResponse>::None);
    let (scanning, set_scanning) = signal(false);
    let (current_page, set_current_page) = signal(0usize);
    let (type_filter, set_type_filter) = signal(String::new()); // "" = all

    // Auto-load gaps on mount
    let api_load = api.clone();
    let load_gaps = Action::new_local(move |_: &()| {
        let api = api_load.clone();
        async move {
            match api.get::<GapsResponse>("/reason/gaps").await {
                Ok(r) => set_gaps_data.set(Some(r)),
                Err(e) => set_status_msg.set(format!("Gaps load error: {e}")),
            }
        }
    });
    load_gaps.dispatch(());

    // Rescan
    let api_scan = api.clone();
    let scan_gaps = Action::new_local(move |_: &()| {
        let api = api_scan.clone();
        async move {
            set_scanning.set(true);
            let body = serde_json::json!({});
            match api.post::<_, GapsResponse>("/reason/scan", &body).await {
                Ok(r) => {
                    set_status_msg.set(format!("Scan complete: {} gaps found", r.report.gaps_detected));
                    set_gaps_data.set(Some(r));
                    set_current_page.set(0);
                }
                Err(e) => set_status_msg.set(format!("Scan error: {e}")),
            }
            set_scanning.set(false);
        }
    });

    let severity_color = |sev: f64| -> &'static str {
        if sev >= 0.7 { "#e74c3c" }
        else if sev >= 0.4 { "#f1c40f" }
        else { "#2ecc71" }
    };

    view! {
        <div class="card" style="padding: 1.5rem;">
            <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem;">
                <h3 style="margin: 0;"><i class="fa-solid fa-triangle-exclamation"></i>" Intelligence Gaps"</h3>
                <button
                    class="btn btn-primary btn-sm"
                    on:click=move |_| { scan_gaps.dispatch(()); }
                    disabled=move || scanning.get()
                >
                    <i class="fa-solid fa-radar"></i>
                    {move || if scanning.get() { " Scanning..." } else { " Rescan" }}
                </button>
            </div>

            // Type filter badges + gap count
            {move || gaps_data.get().map(|gd| {
                let total = gd.report.gaps_detected;
                let by_kind = &gd.report.by_kind;

                // Count per type
                let types: Vec<(&str, &str, u64)> = vec![
                    ("frontier_node", "Frontier", by_kind.get("frontier_nodes").and_then(|v| v.as_u64()).unwrap_or(0)),
                    ("structural_hole", "Structural Hole", by_kind.get("structural_holes").and_then(|v| v.as_u64()).unwrap_or(0)),
                    ("asymmetric_cluster", "Asymmetric", by_kind.get("asymmetric_clusters").and_then(|v| v.as_u64()).unwrap_or(0)),
                    ("temporal_gap", "Temporal", by_kind.get("temporal_gaps").and_then(|v| v.as_u64()).unwrap_or(0)),
                    ("confidence_desert", "Low Confidence", by_kind.get("confidence_deserts").and_then(|v| v.as_u64()).unwrap_or(0)),
                    ("coordinated_cluster", "Coordinated", by_kind.get("coordinated_clusters").and_then(|v| v.as_u64()).unwrap_or(0)),
                ];

                view! {
                    <div style="display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: center; margin-bottom: 0.75rem;">
                        <button
                            class="btn btn-sm"
                            style=move || if type_filter.get().is_empty() { "background: var(--accent); color: white;" } else { "opacity: 0.6;" }
                            on:click=move |_| { set_type_filter.set(String::new()); set_current_page.set(0); }
                        >
                            {format!("All ({})", total)}
                        </button>
                        {types.into_iter().filter(|(_, _, c)| *c > 0).map(|(kind, label, count)| {
                            let kind_owned = kind.to_string();
                            let kind_for_click = kind_owned.clone();
                            view! {
                                <button
                                    class="btn btn-sm"
                                    style=move || if type_filter.get() == kind_owned { "background: var(--accent); color: white;" } else { "opacity: 0.6;" }
                                    on:click=move |_| { set_type_filter.set(kind_for_click.clone()); set_current_page.set(0); }
                                >
                                    {format!("{label} ({count})")}
                                </button>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }
            })}

            // Paginated gap table
            {move || {
                let data = gaps_data.get();
                match data {
                    None => view! {
                        <div style="text-align: center; padding: 2rem 0; opacity: 0.6;">
                            <i class="fa-solid fa-spinner fa-spin"></i>" Loading gaps..."
                        </div>
                    }.into_any(),
                    Some(gd) if gd.gaps.is_empty() => view! {
                        <div style="text-align: center; padding: 2rem 0; opacity: 0.6;">
                            <i class="fa-solid fa-circle-check" style="font-size: 1.5rem; display: block; margin-bottom: 0.5rem;"></i>
                            "No gaps detected."
                        </div>
                    }.into_any(),
                    Some(gd) => {
                        let filter = type_filter.get();
                        let filtered: Vec<_> = if filter.is_empty() {
                            gd.gaps.clone()
                        } else {
                            gd.gaps.iter().filter(|g| g.kind == filter).cloned().collect()
                        };

                        let total_filtered = filtered.len();
                        let page = current_page.get();
                        let total_pages = (total_filtered + PAGE_SIZE - 1) / PAGE_SIZE;
                        let start = page * PAGE_SIZE;
                        let page_gaps: Vec<_> = filtered.into_iter().skip(start).take(PAGE_SIZE).collect();

                        view! {
                            <table class="data-table gap-table">
                                <thead>
                                    <tr>
                                        <th>"Kind"</th>
                                        <th>"Severity"</th>
                                        <th>"Entities"</th>
                                        <th>"Actions"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {page_gaps.into_iter().map(|gap| {
                                        let sev = gap.severity;
                                        let color = severity_color(sev);
                                        let width = format!("{}%", (sev * 100.0) as u32);
                                        let entity_display = gap.entities.first().cloned().unwrap_or_default();
                                        let domain = gap.domain.clone().unwrap_or_default();
                                        view! {
                                            <tr>
                                                <td>
                                                    <span class="badge">{gap.kind.clone()}</span>
                                                    {(!domain.is_empty()).then(|| view! {
                                                        <span style="font-size: 0.7rem; opacity: 0.6; display: block;">{domain.clone()}</span>
                                                    })}
                                                </td>
                                                <td style="min-width: 100px;">
                                                    <div class="gap-severity">
                                                        <div class="gap-severity-bar">
                                                            <div class="gap-severity-fill" style=format!("width: {width}; background: {color};")></div>
                                                        </div>
                                                        <span style="font-size: 0.8rem;">{format!("{:.0}%", sev * 100.0)}</span>
                                                    </div>
                                                </td>
                                                <td>
                                                    <span style="font-weight: 500;">{entity_display.clone()}</span>
                                                    {(gap.entities.len() > 1).then(|| {
                                                        let extra = gap.entities.len() - 1;
                                                        view! { <span style="font-size: 0.75rem; opacity: 0.6;">{format!(" +{extra} more")}</span> }
                                                    })}
                                                </td>
                                                <td>
                                                    <div style="display: flex; gap: 0.25rem; flex-wrap: wrap;">
                                                        <button class="btn btn-sm" style="font-size: 0.7rem; padding: 2px 6px;"
                                                            title="Enrich this entity from Wikidata"
                                                        >
                                                            <i class="fa-solid fa-database"></i>" Enrich"
                                                        </button>
                                                        <button class="btn btn-sm" style="font-size: 0.7rem; padding: 2px 6px;"
                                                            title="Web search and ingest"
                                                        >
                                                            <i class="fa-solid fa-globe"></i>" Search"
                                                        </button>
                                                        <button class="btn btn-sm" style="font-size: 0.7rem; padding: 2px 6px; opacity: 0.5;"
                                                            title="Dismiss this gap"
                                                        >
                                                            <i class="fa-solid fa-xmark"></i>
                                                        </button>
                                                    </div>
                                                </td>
                                            </tr>
                                        }
                                    }).collect::<Vec<_>>()}
                                </tbody>
                            </table>

                            // Pagination controls
                            {(total_pages > 1).then(|| view! {
                                <div style="display: flex; justify-content: center; align-items: center; gap: 0.5rem; margin-top: 1rem;">
                                    <button
                                        class="btn btn-sm"
                                        disabled=move || current_page.get() == 0
                                        on:click=move |_| set_current_page.update(|p| *p = p.saturating_sub(1))
                                    >
                                        <i class="fa-solid fa-chevron-left"></i>
                                    </button>
                                    <span style="font-size: 0.85rem;">
                                        {format!("Page {} of {}", page + 1, total_pages)}
                                    </span>
                                    <button
                                        class="btn btn-sm"
                                        disabled=move || current_page.get() + 1 >= total_pages
                                        on:click=move |_| set_current_page.update(|p| *p += 1)
                                    >
                                        <i class="fa-solid fa-chevron-right"></i>
                                    </button>
                                    <span style="font-size: 0.75rem; opacity: 0.6;">
                                        {format!("({total_filtered} gaps)")}
                                    </span>
                                </div>
                            })}
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}
