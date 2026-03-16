use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::GapsResponse;

#[component]
pub fn GapsZone(set_status_msg: WriteSignal<String>) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let (gaps_data, set_gaps_data) = signal(Option::<GapsResponse>::None);
    let (scanning, set_scanning) = signal(false);
    let (suggesting, set_suggesting) = signal(false);
    let (suggestions, set_suggestions) = signal(String::new());

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
                    set_status_msg.set(format!("Scan complete: {} gaps found", r.report.total_gaps));
                    set_gaps_data.set(Some(r));
                }
                Err(e) => set_status_msg.set(format!("Scan error: {e}")),
            }
            set_scanning.set(false);
        }
    });

    // AI Suggestions
    let api_suggest = api.clone();
    let suggest = Action::new_local(move |_: &()| {
        let api = api_suggest.clone();
        async move {
            set_suggesting.set(true);
            let body = serde_json::json!({});
            match api.post_text("/reason/suggest", &body).await {
                Ok(r) => set_suggestions.set(r),
                Err(e) => set_status_msg.set(format!("Suggest error: {e}")),
            }
            set_suggesting.set(false);
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
                <div style="display: flex; gap: 0.5rem; align-items: center;">
                    <button
                        class="btn btn-primary btn-sm"
                        on:click=move |_| { scan_gaps.dispatch(()); }
                        disabled=move || scanning.get()
                    >
                        <i class="fa-solid fa-radar"></i>
                        {move || if scanning.get() { " Scanning..." } else { " Rescan" }}
                    </button>
                    <button
                        class="btn btn-secondary btn-sm"
                        on:click=move |_| { suggest.dispatch(()); }
                        disabled=move || suggesting.get()
                    >
                        <i class="fa-solid fa-wand-magic-sparkles"></i>
                        {move || if suggesting.get() { " Loading..." } else { " AI Suggestions" }}
                    </button>
                </div>
            </div>

            // Gap count
            {move || gaps_data.get().map(|gd| {
                let total = gd.report.total_gaps;
                if total > 0 {
                    view! {
                        <div style="text-align: right; font-size: 0.85rem; opacity: 0.7; margin-bottom: 0.5rem;">
                            {format!("{total} gaps found")}
                        </div>
                    }.into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            })}

            // Gap table
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
                        let gaps = gd.gaps.clone();
                        view! {
                            <table class="data-table gap-table">
                                <thead>
                                    <tr>
                                        <th>"Kind"</th>
                                        <th>"Severity"</th>
                                        <th>"Entities"</th>
                                        <th>"Suggested Queries"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {gaps.into_iter().map(|gap| {
                                        let sev = gap.severity;
                                        let color = severity_color(sev);
                                        let width = format!("{}%", (sev * 100.0) as u32);
                                        view! {
                                            <tr>
                                                <td><span class="badge">{gap.kind}</span></td>
                                                <td style="min-width: 120px;">
                                                    <div class="gap-severity">
                                                        <div class="gap-severity-bar">
                                                            <div class="gap-severity-fill" style=format!("width: {width}; background: {color};")></div>
                                                        </div>
                                                        <span style="font-size: 0.8rem;">{format!("{:.0}%", sev * 100.0)}</span>
                                                    </div>
                                                </td>
                                                <td>{gap.entities.join(", ")}</td>
                                                <td>
                                                    {gap.suggested_queries.into_iter().map(|q| view! {
                                                        <span class="suggested-query">{q}</span>
                                                    }).collect::<Vec<_>>()}
                                                </td>
                                            </tr>
                                        }
                                    }).collect::<Vec<_>>()}
                                </tbody>
                            </table>
                        }.into_any()
                    }
                }
            }}

            // AI suggestions output
            {move || {
                let s = suggestions.get();
                (!s.is_empty()).then(|| view! {
                    <div style="margin-top: 1rem;">
                        <h4 style="margin-bottom: 0.5rem;">
                            <i class="fa-solid fa-wand-magic-sparkles"></i>" AI Suggestions"
                            <span class="badge" style="margin-left: 0.5rem; font-size: 0.7rem;" title="Generated by LLM -- verify before acting">
                                <i class="fa-solid fa-info-circle"></i>" LLM"
                            </span>
                        </h4>
                        <pre style="background: #1a1a2e; padding: 0.75rem; border-radius: 4px; overflow-x: auto; font-size: 0.85rem; white-space: pre-wrap;">{s}</pre>
                    </div>
                })
            }}
        </div>
    }
}
