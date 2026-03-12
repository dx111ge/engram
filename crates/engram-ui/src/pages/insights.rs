use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{
    ActionRule, AddEvidenceRequest, Assessment, AssessmentCreate, AssessmentDetail,
    GapsResponse, InferenceRule, StatsResponse,
};
use crate::components::collapsible_section::CollapsibleSection;

// ── Helper response types ──

#[derive(Clone, Debug, serde::Deserialize)]
struct RuleNamesResponse {
    #[serde(default)]
    names: Vec<String>,
}

#[derive(Clone, Debug, serde::Deserialize)]
struct DryRunResponse {
    #[serde(default)]
    results: Vec<serde_json::Value>,
    #[serde(default)]
    message: Option<String>,
}

// ── Main page ──

#[component]
pub fn InsightsPage() -> impl IntoView {
    let _api = use_context::<ApiClient>().expect("ApiClient context");
    let (status_msg, set_status_msg) = signal(String::new());

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-chart-line"></i>" Insights"</h2>
            <p class="text-secondary">"What your knowledge base needs attention on"</p>
        </div>

        <div class="warning-banner" style="background: #2a2a1a; border-left: 4px solid #d4a017; padding: 0.75rem 1rem; margin-bottom: 1rem; border-radius: 4px;">
            <i class="fa-solid fa-triangle-exclamation" style="color: #d4a017;"></i>
            " Suggested queries are mechanically generated from graph structure, not AI-generated."
        </div>

        {move || {
            let msg = status_msg.get();
            (!msg.is_empty()).then(|| view! {
                <div class="alert">{msg}</div>
            })
        }}

        <KnowledgeHealthSection />
        <KnowledgeGapsSection set_status_msg />
        <AssessmentsSection set_status_msg />
        <FullAnalysisSection set_status_msg />
        <RecommendedActionsSection set_status_msg />
        <InferenceRulesSection set_status_msg />
        <ActionRulesSection set_status_msg />
    }
}

// ── 1. Knowledge Health ──

#[component]
fn KnowledgeHealthSection() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let stats = LocalResource::new(move || {
        let api = api.clone();
        async move { api.get::<StatsResponse>("/stats").await.ok() }
    });

    view! {
        <CollapsibleSection title="Knowledge Health" icon="fa-solid fa-heart-pulse">
            <Suspense fallback=|| view! { <p>"Loading stats..."</p> }>
                {move || {
                    let data = stats.get().flatten();
                    data.map(|s| {
                        let type_count = s.types.len();
                        view! {
                            <div class="stat-grid" style="display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 1rem;">
                                <div class="stat-card card" style="text-align: center; padding: 1rem;">
                                    <div class="stat-value" style="font-size: 1.8rem; font-weight: bold;">{s.nodes}</div>
                                    <div class="stat-label" style="opacity: 0.7;">
                                        "Facts"
                                    </div>
                                </div>
                                <div class="stat-card card" style="text-align: center; padding: 1rem;">
                                    <div class="stat-value" style="font-size: 1.8rem; font-weight: bold;">{s.edges}</div>
                                    <div class="stat-label" style="opacity: 0.7;">
                                        "Connections"
                                    </div>
                                </div>
                                <div class="stat-card card" style="text-align: center; padding: 1rem;">
                                    <div class="stat-value" style="font-size: 1.8rem; font-weight: bold; color: var(--text-secondary, #8899aa);">"--"</div>
                                    <div class="stat-label" style="opacity: 0.7;">
                                        "Types"
                                    </div>
                                </div>
                                <div class="stat-card card" style="text-align: center; padding: 1rem;">
                                    <div class="stat-value" style="font-size: 1.8rem; font-weight: bold; color: var(--text-secondary, #8899aa);">"--"</div>
                                    <div class="stat-label" style="opacity: 0.7;">
                                        "Avg Strength"
                                    </div>
                                </div>
                            </div>
                        }
                    })
                }}
            </Suspense>
        </CollapsibleSection>
    }
}

// ── 2. Knowledge Gaps ──

#[component]
fn KnowledgeGapsSection(set_status_msg: WriteSignal<String>) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let (gaps_data, set_gaps_data) = signal(Option::<GapsResponse>::None);
    let (scanning, set_scanning) = signal(false);

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

    let severity_color = |sev: f64| -> &'static str {
        if sev >= 0.7 { "#e74c3c" }
        else if sev >= 0.4 { "#f1c40f" }
        else { "#2ecc71" }
    };

    view! {
        <CollapsibleSection title="Things to Improve" icon="fa-solid fa-triangle-exclamation">
            <div class="button-group" style="margin-bottom: 1rem;">
                <button
                    class="btn btn-primary"
                    on:click=move |_| { scan_gaps.dispatch(()); }
                    disabled=move || scanning.get()
                >
                    <i class="fa-solid fa-radar"></i>
                    {move || if scanning.get() { " Scanning..." } else { " Scan for Gaps" }}
                </button>
            </div>

            {move || gaps_data.get().map(|gd| {
                let gaps = gd.gaps.clone();
                if gaps.is_empty() {
                    return view! {
                        <p style="opacity: 0.6;"><i class="fa-solid fa-circle-check"></i>" No knowledge gaps detected."</p>
                    }.into_any();
                }
                view! {
                    <div style="margin-bottom: 0.5rem; opacity: 0.7;">
                        {format!("{} gaps found", gd.report.total_gaps)}
                    </div>
                    <table class="data-table">
                        <thead>
                            <tr>
                                <th>"Kind"</th>
                                <th>"Severity"</th>
                                <th>"Entities"</th>
                                <th>"Domain"</th>
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
                                            <div style="display: flex; align-items: center; gap: 0.5rem;">
                                                <div style="flex: 1; height: 8px; background: #333; border-radius: 4px; overflow: hidden;">
                                                    <div style=format!("width: {width}; height: 100%; background: {color}; border-radius: 4px;")></div>
                                                </div>
                                                <span style="font-size: 0.8rem;">{format!("{:.0}%", sev * 100.0)}</span>
                                            </div>
                                        </td>
                                        <td>{gap.entities.join(", ")}</td>
                                        <td>{gap.domain.unwrap_or_default()}</td>
                                        <td>
                                            {gap.suggested_queries.into_iter().map(|q| view! {
                                                <div class="badge" style="margin: 2px; font-size: 0.75rem;">{q}</div>
                                            }).collect::<Vec<_>>()}
                                        </td>
                                    </tr>
                                }
                            }).collect::<Vec<_>>()}
                        </tbody>
                    </table>
                }.into_any()
            })}
        </CollapsibleSection>
    }
}

// ── 3. Assessments ──

#[component]
fn AssessmentsSection(set_status_msg: WriteSignal<String>) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let (assessments, set_assessments) = signal(Vec::<Assessment>::new());
    let (selected_detail, set_selected_detail) = signal(Option::<AssessmentDetail>::None);
    let (show_create, set_show_create) = signal(false);
    let (show_evidence, set_show_evidence) = signal(Option::<String>::None);

    // Create form signals
    let (cr_label, set_cr_label) = signal(String::new());
    let (cr_category, set_cr_category) = signal(String::new());
    let (cr_description, set_cr_description) = signal(String::new());
    let (cr_probability, set_cr_probability) = signal("50".to_string());
    let (cr_timeframe, set_cr_timeframe) = signal(String::new());
    let (cr_watches, set_cr_watches) = signal(String::new());

    // Evidence form signals
    let (ev_entity, set_ev_entity) = signal(String::new());
    let (ev_direction, set_ev_direction) = signal("supporting".to_string());
    let (ev_weight, set_ev_weight) = signal("0.5".to_string());

    let api_load = api.clone();
    let load_assessments = Action::new_local(move |_: &()| {
        let api = api_load.clone();
        async move {
            match api.get::<Vec<Assessment>>("/assessments").await {
                Ok(list) => set_assessments.set(list),
                Err(e) => set_status_msg.set(format!("Assessments error: {e}")),
            }
        }
    });
    load_assessments.dispatch(());

    let api_detail = api.clone();
    let load_detail = Action::new_local(move |label: &String| {
        let api = api_detail.clone();
        let label = label.clone();
        async move {
            let path = format!("/assessments/{}", js_sys::encode_uri_component(&label));
            match api.get::<AssessmentDetail>(&path).await {
                Ok(d) => set_selected_detail.set(Some(d)),
                Err(e) => set_status_msg.set(format!("Detail error: {e}")),
            }
        }
    });

    let api_create = api.clone();
    let load_assessments2 = load_assessments.clone();
    let create_assessment = Action::new_local(move |_: &()| {
        let api = api_create.clone();
        let reload = load_assessments2.clone();
        let body = AssessmentCreate {
            label: cr_label.get_untracked(),
            category: Some(cr_category.get_untracked()).filter(|s| !s.is_empty()),
            description: Some(cr_description.get_untracked()).filter(|s| !s.is_empty()),
            probability: cr_probability.get_untracked().parse::<f64>().ok().map(|v| v / 100.0),
            timeframe: Some(cr_timeframe.get_untracked()).filter(|s| !s.is_empty()),
            watches: {
                let w = cr_watches.get_untracked();
                if w.is_empty() { None } else {
                    Some(w.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
                }
            },
        };
        async move {
            match api.post_text("/assessments", &body).await {
                Ok(_) => {
                    set_status_msg.set("Assessment created".to_string());
                    set_show_create.set(false);
                    reload.dispatch(());
                }
                Err(e) => set_status_msg.set(format!("Create error: {e}")),
            }
        }
    });

    let api_eval = api.clone();
    let load_assessments3 = load_assessments.clone();
    let evaluate = Action::new_local(move |label: &String| {
        let api = api_eval.clone();
        let reload = load_assessments3.clone();
        let path = format!("/assessments/{}/evaluate", js_sys::encode_uri_component(label));
        async move {
            let body = serde_json::json!({});
            match api.post_text(&path, &body).await {
                Ok(r) => {
                    set_status_msg.set(format!("Evaluated: {r}"));
                    reload.dispatch(());
                }
                Err(e) => set_status_msg.set(format!("Evaluate error: {e}")),
            }
        }
    });

    let api_evidence = api.clone();
    let load_assessments4 = load_assessments.clone();
    let add_evidence = Action::new_local(move |label: &String| {
        let api = api_evidence.clone();
        let reload = load_assessments4.clone();
        let path = format!("/assessments/{}/evidence", js_sys::encode_uri_component(label));
        let body = AddEvidenceRequest {
            entity: ev_entity.get_untracked(),
            text: None,
            source: None,
            weight: ev_weight.get_untracked().parse::<f64>().ok(),
            direction: Some(ev_direction.get_untracked()),
        };
        async move {
            match api.post_text(&path, &body).await {
                Ok(_) => {
                    set_status_msg.set("Evidence added".to_string());
                    set_show_evidence.set(None);
                    reload.dispatch(());
                }
                Err(e) => set_status_msg.set(format!("Evidence error: {e}")),
            }
        }
    });

    view! {
        <CollapsibleSection title="Assessments" icon="fa-solid fa-scale-balanced">
            <div class="button-group" style="margin-bottom: 1rem;">
                <button class="btn btn-success" on:click=move |_| set_show_create.update(|v| *v = !*v)>
                    <i class="fa-solid fa-plus"></i>" New Assessment"
                </button>
            </div>

            // Create form
            {move || show_create.get().then(|| view! {
                <div class="card" style="background: #1e1e2e; padding: 1rem; margin-bottom: 1rem;">
                    <h4>"Create Assessment"</h4>
                    <div class="form-row" style="margin-bottom: 0.5rem;">
                        <label>"Label"</label>
                        <input type="text" placeholder="Assessment label..." prop:value=cr_label
                            on:input=move |ev| set_cr_label.set(event_target_value(&ev)) />
                    </div>
                    <div class="form-row" style="margin-bottom: 0.5rem;">
                        <label>"Category"</label>
                        <input type="text" placeholder="e.g. geopolitical, economic..." prop:value=cr_category
                            on:input=move |ev| set_cr_category.set(event_target_value(&ev)) />
                    </div>
                    <div class="form-row" style="margin-bottom: 0.5rem;">
                        <label>"Description"</label>
                        <textarea placeholder="Description..." prop:value=cr_description
                            on:input=move |ev| set_cr_description.set(event_target_value(&ev)) />
                    </div>
                    <div class="form-row" style="margin-bottom: 0.5rem;">
                        <label>{move || format!("Initial Probability: {}%", cr_probability.get())}</label>
                        <input type="range" min="0" max="100" prop:value=cr_probability
                            on:input=move |ev| set_cr_probability.set(event_target_value(&ev)) />
                    </div>
                    <div class="form-row" style="margin-bottom: 0.5rem;">
                        <label>"Timeframe"</label>
                        <input type="text" placeholder="e.g. 6 months, 2026-Q3..." prop:value=cr_timeframe
                            on:input=move |ev| set_cr_timeframe.set(event_target_value(&ev)) />
                    </div>
                    <div class="form-row" style="margin-bottom: 0.5rem;">
                        <label>"Watches (one per line)"</label>
                        <textarea placeholder="Entity to watch..." prop:value=cr_watches
                            on:input=move |ev| set_cr_watches.set(event_target_value(&ev)) />
                    </div>
                    <div class="button-group">
                        <button class="btn btn-primary" on:click=move |_| { create_assessment.dispatch(()); }>
                            <i class="fa-solid fa-check"></i>" Create"
                        </button>
                        <button class="btn btn-secondary" on:click=move |_| set_show_create.set(false)>
                            "Cancel"
                        </button>
                    </div>
                </div>
            })}

            // Assessment cards
            {move || {
                let list = assessments.get();
                if list.is_empty() {
                    return view! {
                        <p style="opacity: 0.6;">"No assessments yet."</p>
                    }.into_any();
                }
                view! {
                    <div class="assessment-grid" style="display: grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: 1rem;">
                        {list.into_iter().map(|a| {
                            let label = a.label.clone();
                            let label_click = label.clone();
                            let label_eval = label.clone();
                            let label_ev = label.clone();
                            let prob = a.probability.unwrap_or(0.0);
                            let prob_pct = (prob * 100.0) as u32;
                            let prob_color = if prob >= 0.7 { "#e74c3c" } else if prob >= 0.4 { "#f1c40f" } else { "#2ecc71" };
                            let shift = a.probability_shift.unwrap_or(0.0);
                            let shift_icon = if shift > 0.0 { "fa-solid fa-arrow-trend-up" } else if shift < 0.0 { "fa-solid fa-arrow-trend-down" } else { "fa-solid fa-minus" };
                            let shift_color = if shift > 0.0 { "#e74c3c" } else if shift < 0.0 { "#2ecc71" } else { "#888" };
                            view! {
                                <div class="card" style="padding: 1rem; cursor: pointer;"
                                    on:click=move |_| { load_detail.dispatch(label_click.clone()); }>
                                    <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.5rem;">
                                        <strong>{label.clone()}</strong>
                                        {a.category.clone().map(|c| view! { <span class="badge">{c}</span> })}
                                    </div>
                                    <div style="margin-bottom: 0.5rem;">
                                        <div style="display: flex; align-items: center; gap: 0.5rem;">
                                            <div style="flex: 1; height: 8px; background: #333; border-radius: 4px; overflow: hidden;">
                                                <div style=format!("width: {}%; height: 100%; background: {}; border-radius: 4px;", prob_pct, prob_color)></div>
                                            </div>
                                            <span style="font-size: 0.85rem;">{format!("{}%", prob_pct)}</span>
                                            <i class=shift_icon style=format!("color: {}; font-size: 0.8rem;", shift_color)></i>
                                        </div>
                                    </div>
                                    <div style="display: flex; justify-content: space-between; font-size: 0.8rem; opacity: 0.7;">
                                        <span><i class="fa-solid fa-file-lines"></i>" "{a.evidence_count.unwrap_or(0)}" evidence"</span>
                                        <span>{a.status.clone().unwrap_or_default()}</span>
                                    </div>
                                    {a.last_evaluated.clone().map(|le| view! {
                                        <div style="font-size: 0.75rem; opacity: 0.5; margin-top: 0.25rem;">
                                            <i class="fa-solid fa-clock"></i>" Last: "{le}
                                        </div>
                                    })}
                                    <div class="button-group" style="margin-top: 0.75rem;"
                                        on:click=move |ev| { ev.stop_propagation(); }>
                                        <button class="btn btn-sm btn-primary"
                                            on:click=move |_| { evaluate.dispatch(label_eval.clone()); }>
                                            <i class="fa-solid fa-rotate"></i>" Evaluate"
                                        </button>
                                        <button class="btn btn-sm btn-secondary"
                                            on:click=move |_| { set_show_evidence.set(Some(label_ev.clone())); }>
                                            <i class="fa-solid fa-plus"></i>" Evidence"
                                        </button>
                                    </div>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            }}

            // Add evidence modal
            {move || show_evidence.get().map(|label| {
                let label_submit = label.clone();
                view! {
                    <div class="card" style="background: #1e1e2e; padding: 1rem; margin-top: 1rem;">
                        <h4>"Add Evidence for: "{label}</h4>
                        <div class="form-row" style="margin-bottom: 0.5rem;">
                            <label>"Entity"</label>
                            <input type="text" placeholder="Entity name..." prop:value=ev_entity
                                on:input=move |ev| set_ev_entity.set(event_target_value(&ev)) />
                        </div>
                        <div class="form-row" style="margin-bottom: 0.5rem;">
                            <label>"Direction"</label>
                            <select prop:value=ev_direction
                                on:change=move |ev| set_ev_direction.set(event_target_value(&ev))>
                                <option value="supporting">"Supporting"</option>
                                <option value="contradicting">"Contradicting"</option>
                            </select>
                        </div>
                        <div class="form-row" style="margin-bottom: 0.5rem;">
                            <label>"Weight"</label>
                            <input type="number" step="0.1" min="0" max="1" prop:value=ev_weight
                                on:input=move |ev| set_ev_weight.set(event_target_value(&ev)) />
                        </div>
                        <div class="button-group">
                            <button class="btn btn-primary"
                                on:click=move |_| { add_evidence.dispatch(label_submit.clone()); }>
                                <i class="fa-solid fa-check"></i>" Add"
                            </button>
                            <button class="btn btn-secondary" on:click=move |_| set_show_evidence.set(None)>
                                "Cancel"
                            </button>
                        </div>
                    </div>
                }
            })}

            // Detail panel
            {move || selected_detail.get().map(|d| {
                view! {
                    <div class="card" style="background: #1e1e2e; padding: 1rem; margin-top: 1rem;">
                        <div style="display: flex; justify-content: space-between; align-items: center;">
                            <h4><i class="fa-solid fa-info-circle"></i>" "{d.label.clone()}</h4>
                            <button class="btn btn-sm btn-secondary" on:click=move |_| set_selected_detail.set(None)>
                                <i class="fa-solid fa-xmark"></i>
                            </button>
                        </div>
                        {d.description.clone().map(|desc| view! { <p>{desc}</p> })}
                        <div style="display: flex; gap: 1rem; flex-wrap: wrap; margin: 0.5rem 0;">
                            {d.category.clone().map(|c| view! { <span class="badge">{c}</span> })}
                            {d.status.clone().map(|s| view! { <span class="badge badge-active">{s}</span> })}
                            {d.timeframe.clone().map(|t| view! { <span><i class="fa-solid fa-calendar"></i>" "{t}</span> })}
                        </div>
                        {(!d.watches.is_empty()).then(|| view! {
                            <div style="margin: 0.5rem 0;">
                                <strong>"Watches: "</strong>
                                {d.watches.iter().map(|w| view! {
                                    <span class="badge" style="margin: 2px;">{w.clone()}</span>
                                }).collect::<Vec<_>>()}
                            </div>
                        })}
                        {(!d.evidence.is_empty()).then(|| view! {
                            <h5 style="margin-top: 0.75rem;">"Evidence"</h5>
                            <table class="data-table">
                                <thead>
                                    <tr><th>"Entity"</th><th>"Direction"</th><th>"Weight"</th><th>"Source"</th></tr>
                                </thead>
                                <tbody>
                                    {d.evidence.iter().map(|ev| view! {
                                        <tr>
                                            <td>{ev.entity.clone().unwrap_or_default()}</td>
                                            <td>{ev.direction.clone().unwrap_or_default()}</td>
                                            <td>{ev.weight.map(|w| format!("{:.2}", w)).unwrap_or_default()}</td>
                                            <td>{ev.source.clone().unwrap_or_default()}</td>
                                        </tr>
                                    }).collect::<Vec<_>>()}
                                </tbody>
                            </table>
                        })}
                        {(!d.history.is_empty()).then(|| view! {
                            <h5 style="margin-top: 0.75rem;">"History"</h5>
                            <table class="data-table">
                                <thead>
                                    <tr><th>"Timestamp"</th><th>"Probability"</th><th>"Reason"</th></tr>
                                </thead>
                                <tbody>
                                    {d.history.iter().map(|h| view! {
                                        <tr>
                                            <td>{h.timestamp.clone().unwrap_or_default()}</td>
                                            <td>{h.probability.map(|p| format!("{:.0}%", p * 100.0)).unwrap_or_default()}</td>
                                            <td>{h.reason.clone().unwrap_or_default()}</td>
                                        </tr>
                                    }).collect::<Vec<_>>()}
                                </tbody>
                            </table>
                        })}
                    </div>
                }
            })}
        </CollapsibleSection>
    }
}

// ── Full Analysis ──

#[component]
fn FullAnalysisSection(set_status_msg: WriteSignal<String>) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let (scanning, set_scanning) = signal(false);
    let (result_text, set_result_text) = signal(String::new());

    let api_scan = api.clone();
    let scan = Action::new_local(move |_: &()| {
        let api = api_scan.clone();
        async move {
            set_scanning.set(true);
            let body = serde_json::json!({});
            match api.post_text("/reason/scan", &body).await {
                Ok(r) => set_result_text.set(r),
                Err(e) => set_status_msg.set(format!("Scan error: {e}")),
            }
            set_scanning.set(false);
        }
    });

    view! {
        <div class="card" style="padding: 1.5rem; margin-bottom: 1rem;">
            <h3 style="margin-bottom: 0.5rem;">"Full Analysis"</h3>
            <p class="text-secondary" style="margin-bottom: 1rem;">"Run a comprehensive scan of your knowledge base to find conflicts, gaps, and areas for improvement."</p>
            <button class="btn btn-primary" on:click=move |_| { scan.dispatch(()); }
                disabled=move || scanning.get()>
                <i class="fa-solid fa-magnifying-glass"></i>
                {move || if scanning.get() { " Scanning..." } else { " Scan Now" }}
            </button>
            {move || {
                let r = result_text.get();
                (!r.is_empty()).then(|| view! {
                    <pre style="margin-top: 1rem; background: #1a1a2e; padding: 0.75rem; border-radius: 4px; overflow-x: auto; font-size: 0.85rem;">{r}</pre>
                })
            }}
        </div>
    }
}

// ── Recommended Actions ──

#[component]
fn RecommendedActionsSection(set_status_msg: WriteSignal<String>) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let (loading, set_loading) = signal(false);
    let (result_text, set_result_text) = signal(String::new());

    let api_suggest = api.clone();
    let suggest = Action::new_local(move |_: &()| {
        let api = api_suggest.clone();
        async move {
            set_loading.set(true);
            let body = serde_json::json!({});
            match api.post_text("/reason/suggest", &body).await {
                Ok(r) => set_result_text.set(r),
                Err(e) => set_status_msg.set(format!("Suggest error: {e}")),
            }
            set_loading.set(false);
        }
    });

    view! {
        <div class="card" style="padding: 1.5rem; margin-bottom: 1rem;">
            <h3><i class="fa-solid fa-lightbulb"></i>" Recommended Actions"</h3>
            <button class="btn btn-primary" style="margin-top: 0.75rem;" on:click=move |_| { suggest.dispatch(()); }
                disabled=move || loading.get()>
                <i class="fa-solid fa-wand-magic-sparkles"></i>
                {move || if loading.get() { " Loading..." } else { " Get Recommendations" }}
            </button>
            {move || {
                let r = result_text.get();
                (!r.is_empty()).then(|| view! {
                    <pre style="margin-top: 1rem; background: #1a1a2e; padding: 0.75rem; border-radius: 4px; overflow-x: auto; font-size: 0.85rem;">{r}</pre>
                })
            }}
        </div>
    }
}

// ── 4. Inference Rules ──

#[component]
fn InferenceRulesSection(set_status_msg: WriteSignal<String>) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let (rule_names, set_rule_names) = signal(Vec::<String>::new());
    let (new_rules_text, set_new_rules_text) = signal(String::new());

    let api_load = api.clone();
    let load_rules = Action::new_local(move |_: &()| {
        let api = api_load.clone();
        async move {
            match api.get::<RuleNamesResponse>("/rules").await {
                Ok(r) => set_rule_names.set(r.names),
                Err(e) => set_status_msg.set(format!("Rules load error: {e}")),
            }
        }
    });
    load_rules.dispatch(());

    let api_add = api.clone();
    let load_rules2 = load_rules.clone();
    let add_rules = Action::new_local(move |_: &()| {
        let api = api_add.clone();
        let reload = load_rules2.clone();
        let text = new_rules_text.get_untracked();
        let rules: Vec<InferenceRule> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| InferenceRule {
                name: None,
                rule: l.trim().to_string(),
                description: None,
            })
            .collect();
        async move {
            let body = serde_json::json!({ "rules": rules, "append": true });
            match api.post_text("/rules", &body).await {
                Ok(r) => {
                    set_status_msg.set(format!("Rules loaded: {r}"));
                    set_new_rules_text.set(String::new());
                    reload.dispatch(());
                }
                Err(e) => set_status_msg.set(format!("Rules add error: {e}")),
            }
        }
    });

    view! {
        <CollapsibleSection title="Inference Rules" icon="fa-solid fa-brain" collapsed=true>
            {move || {
                let names = rule_names.get();
                if names.is_empty() {
                    view! { <p style="opacity: 0.6;">"No inference rules loaded."</p> }.into_any()
                } else {
                    view! {
                        <div style="margin-bottom: 1rem;">
                            {names.into_iter().map(|n| view! {
                                <div style="padding: 0.25rem 0;">
                                    <i class="fa-solid fa-gavel" style="margin-right: 0.5rem; opacity: 0.6;"></i>{n}
                                </div>
                            }).collect::<Vec<_>>()}
                        </div>
                    }.into_any()
                }
            }}

            <div class="form-row" style="margin-bottom: 0.5rem;">
                <label>"Add Rules (one per line, format: IF condition THEN action)"</label>
                <textarea
                    class="code-area"
                    rows="4"
                    placeholder="IF entity has_type Person AND missing email THEN suggest find email for {entity}"
                    prop:value=new_rules_text
                    on:input=move |ev| set_new_rules_text.set(event_target_value(&ev))
                />
            </div>
            <button class="btn btn-primary" on:click=move |_| { add_rules.dispatch(()); }>
                <i class="fa-solid fa-upload"></i>" Load Rules"
            </button>
        </CollapsibleSection>
    }
}

// ── 5. Action Rules ──

#[component]
fn ActionRulesSection(set_status_msg: WriteSignal<String>) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let (action_rules, set_action_rules) = signal(Vec::<ActionRule>::new());
    let (dry_run_output, set_dry_run_output) = signal(String::new());
    let (new_rule_json, set_new_rule_json) = signal(String::new());
    let (dry_running, set_dry_running) = signal(false);

    let api_load = api.clone();
    let load_action_rules = Action::new_local(move |_: &()| {
        let api = api_load.clone();
        async move {
            match api.get::<Vec<ActionRule>>("/actions/rules").await {
                Ok(list) => set_action_rules.set(list),
                Err(e) => set_status_msg.set(format!("Action rules error: {e}")),
            }
        }
    });
    load_action_rules.dispatch(());

    let api_dry = api.clone();
    let dry_run = Action::new_local(move |_: &()| {
        let api = api_dry.clone();
        async move {
            set_dry_running.set(true);
            let body = serde_json::json!({});
            match api.post_text("/actions/dry-run", &body).await {
                Ok(r) => set_dry_run_output.set(r),
                Err(e) => set_dry_run_output.set(format!("Error: {e}")),
            }
            set_dry_running.set(false);
        }
    });

    let api_add = api.clone();
    let load_action_rules2 = load_action_rules.clone();
    let add_action_rule = Action::new_local(move |_: &()| {
        let api = api_add.clone();
        let reload = load_action_rules2.clone();
        let json_str = new_rule_json.get_untracked();
        async move {
            match serde_json::from_str::<serde_json::Value>(&json_str) {
                Ok(body) => {
                    match api.post_text("/actions/rules", &body).await {
                        Ok(r) => {
                            set_status_msg.set(format!("Rule added: {r}"));
                            set_new_rule_json.set(String::new());
                            reload.dispatch(());
                        }
                        Err(e) => set_status_msg.set(format!("Add rule error: {e}")),
                    }
                }
                Err(e) => set_status_msg.set(format!("Invalid JSON: {e}")),
            }
        }
    });

    let api_del = api.clone();
    let load_action_rules3 = load_action_rules.clone();
    let delete_rule = Action::new_local(move |id: &String| {
        let api = api_del.clone();
        let reload = load_action_rules3.clone();
        let path = format!("/actions/rules/{}", js_sys::encode_uri_component(id));
        async move {
            match api.delete(&path).await {
                Ok(_) => {
                    set_status_msg.set("Rule deleted".to_string());
                    reload.dispatch(());
                }
                Err(e) => set_status_msg.set(format!("Delete error: {e}")),
            }
        }
    });

    let api_toggle = api.clone();
    let load_action_rules4 = load_action_rules.clone();
    let toggle_rule = Action::new_local(move |args: &(String, bool)| {
        let api = api_toggle.clone();
        let reload = load_action_rules4.clone();
        let (id, new_state) = args.clone();
        let path = format!("/actions/rules/{}", js_sys::encode_uri_component(&id));
        let body = serde_json::json!({ "enabled": new_state });
        async move {
            match api.patch::<_, serde_json::Value>(&path, &body).await {
                Ok(_) => { reload.dispatch(()); },
                Err(e) => set_status_msg.set(format!("Toggle error: {e}")),
            }
        }
    });

    view! {
        <CollapsibleSection title="Action Rules" icon="fa-solid fa-bolt" collapsed=true>
            <div class="button-group" style="margin-bottom: 1rem;">
                <button
                    class="btn btn-warning"
                    on:click=move |_| { dry_run.dispatch(()); }
                    disabled=move || dry_running.get()
                >
                    <i class="fa-solid fa-flask"></i>
                    {move || if dry_running.get() { " Running..." } else { " Dry Run" }}
                </button>
            </div>

            {move || {
                let output = dry_run_output.get();
                (!output.is_empty()).then(|| view! {
                    <pre style="background: #1a1a2e; padding: 0.75rem; border-radius: 4px; overflow-x: auto; margin-bottom: 1rem; font-size: 0.85rem;">{output}</pre>
                })
            }}

            {move || {
                let rules = action_rules.get();
                if rules.is_empty() {
                    return view! {
                        <p style="opacity: 0.6;">"No action rules configured."</p>
                    }.into_any();
                }
                view! {
                    <table class="data-table">
                        <thead>
                            <tr>
                                <th>"Name"</th>
                                <th>"Description"</th>
                                <th>"Enabled"</th>
                                <th>"Actions"</th>
                            </tr>
                        </thead>
                        <tbody>
                            {rules.into_iter().map(|r| {
                                let id_toggle = r.id.clone();
                                let enabled = r.enabled;
                                let id_del = r.id.clone();
                                view! {
                                    <tr>
                                        <td><strong>{r.name}</strong></td>
                                        <td>{r.description.unwrap_or_default()}</td>
                                        <td>
                                            <input type="checkbox" prop:checked=enabled
                                                on:change=move |_| {
                                                    toggle_rule.dispatch((id_toggle.clone(), !enabled));
                                                } />
                                        </td>
                                        <td>
                                            <button class="btn btn-sm btn-danger"
                                                on:click=move |_| { delete_rule.dispatch(id_del.clone()); }>
                                                <i class="fa-solid fa-trash"></i>
                                            </button>
                                        </td>
                                    </tr>
                                }
                            }).collect::<Vec<_>>()}
                        </tbody>
                    </table>
                }.into_any()
            }}

            <div style="margin-top: 1rem;">
                <h4>"Add Action Rule (JSON)"</h4>
                <textarea
                    class="code-area"
                    rows="6"
                    placeholder="{\"name\": \"my-rule\", \"trigger\": \"node_created\", \"conditions\": {}, \"actions\": {}}"
                    prop:value=new_rule_json
                    on:input=move |ev| set_new_rule_json.set(event_target_value(&ev))
                />
                <button class="btn btn-primary" style="margin-top: 0.5rem;"
                    on:click=move |_| { add_action_rule.dispatch(()); }>
                    <i class="fa-solid fa-plus"></i>" Add Rule"
                </button>
            </div>
        </CollapsibleSection>
    }
}
