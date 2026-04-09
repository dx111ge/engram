use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{GapsResponse, IngestResponse};

const PAGE_SIZE: usize = 50;

/// Map raw BlackAreaKind string to human-readable label.
fn readable_gap_kind(kind: &str) -> &'static str {
    match kind.to_uppercase().as_str() {
        "FRONTIERNODE" | "FRONTIER_NODE" => "Isolated Entity",
        "STRUCTURALHOLE" | "STRUCTURAL_HOLE" => "Missing Connection",
        "ASYMMETRICCLUSTER" | "ASYMMETRIC_CLUSTER" => "Coverage Gap",
        "TEMPORALGAP" | "TEMPORAL_GAP" => "Stale Information",
        "CONFIDENCEDESERT" | "CONFIDENCE_DESERT" => "Low Confidence",
        "COORDINATEDCLUSTER" | "COORDINATED_CLUSTER" => "Suspicious Pattern",
        _ => "Unknown",
    }
}

#[component]
pub fn GapsZone(set_status_msg: WriteSignal<String>) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let (gaps_data, set_gaps_data) = signal(Option::<GapsResponse>::None);
    let (scanning, set_scanning) = signal(false);
    let (current_page, set_current_page) = signal(0usize);
    let (type_filter, set_type_filter) = signal(String::new()); // "" = all

    // Dismissed gap labels (persisted in config)
    let (dismissed, set_dismissed) = signal(Vec::<String>::new());
    let (show_dismissed, set_show_dismissed) = signal(false);

    // Load dismissed from config on mount
    let api_dismissed = api.clone();
    Effect::new(move || {
        let api = api_dismissed.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(text) = api.get_text("/config").await {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    let list: Vec<String> = json.get("dismissed_gaps")
                        .and_then(|v| v.as_array())
                        .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                        .unwrap_or_default();
                    if !list.is_empty() {
                        set_dismissed.set(list);
                    }
                }
            }
        });
    });

    // Save dismissed to config
    let api_save_dismissed = api.clone();
    let save_dismissed = Action::new_local(move |_: &()| {
        let api = api_save_dismissed.clone();
        let list = dismissed.get_untracked();
        async move {
            let body = serde_json::json!({"dismissed_gaps": list});
            let _ = api.post_text("/config", &body).await;
        }
    });

    // Which entity is currently being enriched (shows spinner)
    let (enriching_entity, set_enriching_entity) = signal(Option::<String>::None);
    // Inline result message per entity: (entity_label, message)
    let (enrich_result, set_enrich_result) = signal(Option::<(String, String)>::None);

    // Search mode: which entity has the search input open
    let (search_entity, set_search_entity) = signal(Option::<String>::None);
    let (search_query, set_search_query) = signal(String::new());
    let (searching, set_searching) = signal(false);

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

    // Enrich action: POST /reason/enrich/plan -> POST /reason/enrich/run
    let api_enrich = api.clone();
    let enrich_action = Action::new_local(move |entity: &String| {
        let api = api_enrich.clone();
        let entity = entity.clone();
        async move {
            set_enriching_entity.set(Some(entity.clone()));
            set_enrich_result.set(None);

            // Step 1: Generate search queries via LLM
            let plan_body = serde_json::json!({
                "query": entity,
                "entities": [entity],
            });
            let queries: Vec<String> = match api.post_text("/reason/enrich/plan", &plan_body).await {
                Ok(text) => {
                    serde_json::from_str::<serde_json::Value>(&text).ok()
                        .and_then(|j| j.get("queries")?.as_array().cloned())
                        .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                        .unwrap_or_default()
                }
                Err(e) => {
                    set_enrich_result.set(Some((entity.clone(), format!("Plan failed: {e}"))));
                    set_enriching_entity.set(None);
                    return;
                }
            };

            if queries.is_empty() {
                set_enrich_result.set(Some((entity.clone(), "No search queries generated.".into())));
                set_enriching_entity.set(None);
                return;
            }

            // Step 2: Execute background enrichment
            let run_body = serde_json::json!({
                "queries": queries,
                "max_sources": 5,
            });
            match api.post_text("/reason/enrich/run", &run_body).await {
                Ok(_) => {
                    let msg = format!("Enrichment started: {} search queries", queries.len());
                    set_enrich_result.set(Some((entity.clone(), msg.clone())));
                    set_status_msg.set(msg);
                }
                Err(e) => {
                    set_enrich_result.set(Some((entity.clone(), format!("Run failed: {e}"))));
                }
            }
            set_enriching_entity.set(None);
        }
    });

    // Search action: same flow but with user-provided query
    let api_search = api.clone();
    let search_action = Action::new_local(move |query: &String| {
        let api = api_search.clone();
        let query = query.clone();
        async move {
            set_searching.set(true);
            let run_body = serde_json::json!({
                "queries": [query],
                "max_sources": 5,
            });
            match api.post_text("/reason/enrich/run", &run_body).await {
                Ok(_) => {
                    set_status_msg.set("Enrichment started from search query.".into());
                    set_search_entity.set(None);
                    set_search_query.set(String::new());
                }
                Err(e) => {
                    set_status_msg.set(format!("Search enrichment failed: {e}"));
                }
            }
            set_searching.set(false);
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
                    // Show dismissed toggle
                    <button
                        class="btn btn-sm"
                        style=move || {
                            let count = dismissed.get().len();
                            if count == 0 {
                                "opacity: 0.3; pointer-events: none;".to_string()
                            } else if show_dismissed.get() {
                                "background: var(--accent); color: white;".to_string()
                            } else {
                                String::new()
                            }
                        }
                        on:click=move |_| { set_show_dismissed.update(|v| *v = !*v); }
                        title="Toggle showing dismissed gaps"
                    >
                        <i class="fa-solid fa-eye-slash"></i>
                        {move || {
                            let count = dismissed.get().len();
                            if count > 0 { format!(" Dismissed ({count})") } else { " Dismissed (0)".to_string() }
                        }}
                    </button>
                    <button
                        class="btn btn-primary btn-sm"
                        on:click=move |_| { scan_gaps.dispatch(()); }
                        disabled=move || scanning.get()
                    >
                        <i class="fa-solid fa-radar"></i>
                        {move || if scanning.get() { " Scanning..." } else { " Rescan" }}
                    </button>
                </div>
            </div>

            // Type filter badges + gap count
            {move || gaps_data.get().map(|gd| {
                let total = gd.report.gaps_detected;
                let by_kind = &gd.report.by_kind;

                // Count per type
                let types: Vec<(&str, &str, u64)> = vec![
                    ("frontier_node", "Isolated Entity", by_kind.get("frontier_nodes").and_then(|v| v.as_u64()).unwrap_or(0)),
                    ("structural_hole", "Missing Connection", by_kind.get("structural_holes").and_then(|v| v.as_u64()).unwrap_or(0)),
                    ("asymmetric_cluster", "Coverage Gap", by_kind.get("asymmetric_clusters").and_then(|v| v.as_u64()).unwrap_or(0)),
                    ("temporal_gap", "Stale Information", by_kind.get("temporal_gaps").and_then(|v| v.as_u64()).unwrap_or(0)),
                    ("confidence_desert", "Low Confidence", by_kind.get("confidence_deserts").and_then(|v| v.as_u64()).unwrap_or(0)),
                    ("coordinated_cluster", "Suspicious Pattern", by_kind.get("coordinated_clusters").and_then(|v| v.as_u64()).unwrap_or(0)),
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
                        let dismissed_list = dismissed.get();
                        let show_d = show_dismissed.get();
                        let filtered: Vec<_> = gd.gaps.iter().filter(|g| {
                            // Type filter
                            let type_ok = filter.is_empty() || g.kind == filter;
                            // Dismissed filter: hide dismissed unless show_dismissed is on
                            let entity_label = g.entities.first().cloned().unwrap_or_default();
                            let is_dismissed = dismissed_list.contains(&entity_label);
                            let dismiss_ok = show_d || !is_dismissed;
                            type_ok && dismiss_ok
                        }).cloned().collect();

                        let total_filtered = filtered.len();
                        let page = current_page.get();
                        let total_pages = if total_filtered == 0 { 1 } else { (total_filtered + PAGE_SIZE - 1) / PAGE_SIZE };
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
                                        let entity_for_dismiss = entity_display.clone();
                                        let entity_for_enrich = entity_display.clone();
                                        let entity_for_enrich_display = entity_display.clone();
                                        let entity_for_search = entity_display.clone();
                                        let entity_for_search_queries = entity_display.clone();
                                        let entity_for_search_check = entity_display.clone();
                                        let entity_for_result = entity_display.clone();
                                        let domain = gap.domain.clone().unwrap_or_default();
                                        let dismissed_list2 = dismissed.get();
                                        let is_dismissed = dismissed_list2.contains(&entity_display);
                                        view! {
                                            <tr style=move || if is_dismissed { "opacity: 0.4;" } else { "" }>
                                                <td>
                                                    <span class="badge">{readable_gap_kind(&gap.kind)}</span>
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
                                                    // Suggested queries as clickable chips
                                                    {(!gap.suggested_queries.is_empty()).then(|| {
                                                        let queries = gap.suggested_queries.clone();
                                                        let ent_for_q = entity_for_search_queries.clone();
                                                        view! {
                                                            <div style="display: flex; flex-wrap: wrap; gap: 3px; margin-top: 4px;">
                                                                {queries.into_iter().take(3).map({
                                                                    let ent = ent_for_q.clone();
                                                                    move |q| {
                                                                    let q2 = q.clone();
                                                                    let ent2 = ent.clone();
                                                                    view! {
                                                                        <span class="badge" style="font-size: 0.65rem; cursor: pointer; opacity: 0.7;"
                                                                            title="Click to use as search query"
                                                                            on:click=move |_| {
                                                                                set_search_entity.set(Some(ent2.clone()));
                                                                                set_search_query.set(q2.clone());
                                                                            }
                                                                        >
                                                                            <i class="fa-solid fa-magnifying-glass" style="font-size: 0.55rem; margin-right: 2px;"></i>
                                                                            {q}
                                                                        </span>
                                                                    }
                                                                }}).collect::<Vec<_>>()}
                                                            </div>
                                                        }
                                                    })}
                                                    // Inline enrich result
                                                    {move || {
                                                        let result = enrich_result.get();
                                                        match result {
                                                            Some((ref ent, ref msg)) if *ent == entity_for_result => {
                                                                let is_err = msg.starts_with("Enrich failed");
                                                                let color = if is_err { "#e74c3c" } else { "#2ecc71" };
                                                                Some(view! {
                                                                    <span style=format!("font-size: 0.75rem; display: block; color: {color}; margin-top: 2px;")>
                                                                        {if is_err {
                                                                            view! { <i class="fa-solid fa-circle-xmark"></i> }.into_any()
                                                                        } else {
                                                                            view! { <i class="fa-solid fa-circle-check"></i> }.into_any()
                                                                        }}
                                                                        " "{msg.clone()}
                                                                    </span>
                                                                })
                                                            }
                                                            _ => None,
                                                        }
                                                    }}
                                                </td>
                                                <td>
                                                    <div style="display: flex; gap: 0.25rem; flex-wrap: wrap; align-items: center;">
                                                        // Enrich button
                                                        <button class="btn btn-sm" style="font-size: 0.7rem; padding: 2px 6px;"
                                                            title="Enrich this entity via ingest pipeline"
                                                            disabled=move || enriching_entity.get().is_some()
                                                            on:click={
                                                                let ent = entity_for_enrich.clone();
                                                                move |_| { enrich_action.dispatch(ent.clone()); }
                                                            }
                                                        >
                                                            {move || {
                                                                let loading = enriching_entity.get();
                                                                if loading.as_deref() == Some(&entity_for_enrich_display) {
                                                                    view! { <><i class="fa-solid fa-spinner fa-spin"></i>" Enriching..."</> }.into_any()
                                                                } else {
                                                                    view! { <><i class="fa-solid fa-database"></i>" Enrich"</> }.into_any()
                                                                }
                                                            }}
                                                        </button>
                                                        // Search button / inline input
                                                        {move || {
                                                            let active = search_entity.get();
                                                            if active.as_deref() == Some(&entity_for_search_check) {
                                                                // Show inline search input
                                                                view! {
                                                                    <form style="display: inline-flex; gap: 2px; align-items: center;"
                                                                        on:submit={
                                                                            move |ev: leptos::ev::SubmitEvent| {
                                                                                ev.prevent_default();
                                                                                let q = search_query.get_untracked();
                                                                                if !q.trim().is_empty() {
                                                                                    search_action.dispatch(q);
                                                                                }
                                                                            }
                                                                        }
                                                                    >
                                                                        <input
                                                                            type="text"
                                                                            class="input input-sm"
                                                                            style="font-size: 0.7rem; padding: 2px 4px; width: 140px;"
                                                                            prop:value=move || search_query.get()
                                                                            on:input=move |ev| {
                                                                                set_search_query.set(event_target_value(&ev));
                                                                            }
                                                                        />
                                                                        <button type="submit" class="btn btn-sm" style="font-size: 0.7rem; padding: 2px 6px;"
                                                                            disabled=move || searching.get()
                                                                        >
                                                                            {move || if searching.get() {
                                                                                view! { <i class="fa-solid fa-spinner fa-spin"></i> }.into_any()
                                                                            } else {
                                                                                view! { <i class="fa-solid fa-arrow-right"></i> }.into_any()
                                                                            }}
                                                                        </button>
                                                                        <button type="button" class="btn btn-sm" style="font-size: 0.7rem; padding: 2px 6px; opacity: 0.5;"
                                                                            on:click=move |_| { set_search_entity.set(None); set_search_query.set(String::new()); }
                                                                        >
                                                                            <i class="fa-solid fa-xmark"></i>
                                                                        </button>
                                                                    </form>
                                                                }.into_any()
                                                            } else {
                                                                let ent = entity_for_search.clone();
                                                                view! {
                                                                    <button class="btn btn-sm" style="font-size: 0.7rem; padding: 2px 6px;"
                                                                        title="Web search and ingest"
                                                                        on:click=move |_| {
                                                                            set_search_query.set(ent.clone());
                                                                            set_search_entity.set(Some(ent.clone()));
                                                                        }
                                                                    >
                                                                        <i class="fa-solid fa-globe"></i>" Search"
                                                                    </button>
                                                                }.into_any()
                                                            }
                                                        }}
                                                        // Dismiss button
                                                        <button class="btn btn-sm" style="font-size: 0.7rem; padding: 2px 6px; opacity: 0.5;"
                                                            title="Dismiss this gap"
                                                            on:click={
                                                                let ent = entity_for_dismiss.clone();
                                                                move |_| {
                                                                    set_dismissed.update(|list| {
                                                                        if !list.contains(&ent) {
                                                                            list.push(ent.clone());
                                                                        }
                                                                    });
                                                                    let _ = save_dismissed.dispatch(());
                                                                }
                                                            }
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
