use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{AddEvidenceRequest, Assessment, AssessmentDetail};
use crate::components::assessment_wizard::AssessmentWizard;
use crate::components::chat_types::ChatCurrentAssessment;

#[component]
pub fn AssessmentsZone(set_status_msg: WriteSignal<String>) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let (assessments, set_assessments) = signal(Vec::<Assessment>::new());
    let (selected_detail, set_selected_detail) = signal(Option::<AssessmentDetail>::None);
    let (show_wizard, set_show_wizard) = signal(false);
    let (show_evidence, set_show_evidence) = signal(Option::<String>::None);

    // Evidence form signals
    let (ev_entity, set_ev_entity) = signal(String::new());
    let (ev_direction, set_ev_direction) = signal("supporting".to_string());
    let (ev_weight, set_ev_weight) = signal("0.5".to_string());

    // Load assessments
    let api_load = api.clone();
    let load_assessments = Action::new_local(move |_: &()| {
        let api = api_load.clone();
        async move {
            match api.get::<crate::api::types::AssessmentsResponse>("/assessments").await {
                Ok(resp) => set_assessments.set(resp.assessments),
                Err(e) => set_status_msg.set(format!("Assessments error: {e}")),
            }
        }
    });
    load_assessments.dispatch(());

    // Load detail
    let chat_assessment = use_context::<ChatCurrentAssessment>();
    let api_detail = api.clone();
    let load_detail = Action::new_local(move |label: &String| {
        let api = api_detail.clone();
        let label = label.clone();
        async move {
            if let Some(ctx) = chat_assessment { ctx.0.set(Some(label.clone())); }
            let path = format!("/assessments/{}", js_sys::encode_uri_component(&label));
            match api.get::<AssessmentDetail>(&path).await {
                Ok(d) => set_selected_detail.set(Some(d)),
                Err(e) => set_status_msg.set(format!("Detail error: {e}")),
            }
        }
    });

    // Evaluate
    let api_eval = api.clone();
    let load_assessments2 = load_assessments.clone();
    let evaluate = Action::new_local(move |label: &String| {
        let api = api_eval.clone();
        let reload = load_assessments2.clone();
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

    // Add evidence
    let api_evidence = api.clone();
    let load_assessments3 = load_assessments.clone();
    let add_evidence = Action::new_local(move |label: &String| {
        let api = api_evidence.clone();
        let reload = load_assessments3.clone();
        let label2 = label.clone();
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
                    set_ev_entity.set(String::new());
                    set_ev_weight.set("0.5".to_string());
                    reload.dispatch(());
                    // Refresh detail if open
                    if let Some(d) = selected_detail.get_untracked() {
                        if d.label == label2 {
                            let path2 = format!("/assessments/{}", js_sys::encode_uri_component(&label2));
                            if let Ok(refreshed) = api.get::<AssessmentDetail>(&path2).await {
                                set_selected_detail.set(Some(refreshed));
                            }
                        }
                    }
                }
                Err(e) => set_status_msg.set(format!("Evidence error: {e}")),
            }
        }
    });

    let reload_after_create = load_assessments.clone();
    let on_created = Callback::new(move |_: ()| {
        reload_after_create.dispatch(());
    });

    view! {
        <div style="margin-bottom: 1.5rem;">
            <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem;">
                <h3 style="margin: 0;"><i class="fa-solid fa-scale-balanced"></i>" Assessments"</h3>
                <div style="display: flex; gap: 0.5rem;">
                    <button class="btn btn-success btn-sm" on:click=move |_| set_show_wizard.set(true)>
                        <i class="fa-solid fa-plus"></i>" New Assessment"
                    </button>
                    <button class="btn btn-secondary btn-sm" on:click=move |_| { load_assessments.dispatch(()); }>
                        <i class="fa-solid fa-refresh"></i>
                    </button>
                </div>
            </div>

            // Assessment cards
            {move || {
                let list = assessments.get();
                if list.is_empty() {
                    return view! {
                        <div style="text-align: center; padding: 2rem 0; opacity: 0.6;">
                            <i class="fa-solid fa-scale-balanced" style="font-size: 1.5rem; display: block; margin-bottom: 0.5rem;"></i>
                            "No assessments yet. Create one to start tracking hypotheses."
                        </div>
                    }.into_any();
                }
                view! {
                    <div class="assessment-grid">
                        {list.into_iter().map(|a| {
                            let label = a.label.clone();
                            let label_click = label.clone();
                            let prob = a.probability.unwrap_or(0.0);
                            let prob_pct = (prob * 100.0) as u32;
                            let prob_color = if prob >= 0.7 { "var(--danger, #e74c3c)" } else if prob >= 0.4 { "var(--warning, #f1c40f)" } else { "var(--success, #2ecc71)" };
                            let shift = a.probability_shift.unwrap_or(0.0);
                            let shift_icon = if shift > 0.0 { "fa-solid fa-arrow-trend-up" } else if shift < 0.0 { "fa-solid fa-arrow-trend-down" } else { "fa-solid fa-minus" };
                            let shift_color = if shift > 0.0 { "#e74c3c" } else if shift < 0.0 { "#2ecc71" } else { "#888" };
                            view! {
                                <div class="assessment-card"
                                    on:click=move |_| { load_detail.dispatch(label_click.clone()); }>
                                    <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.5rem;">
                                        <strong style="font-size: 0.95rem;">{label}</strong>
                                        {a.category.clone().map(|c| view! { <span class="badge">{c}</span> })}
                                    </div>
                                    <div style="margin-bottom: 0.5rem;">
                                        <div style="display: flex; align-items: center; gap: 0.5rem;">
                                            <div class="assessment-card-prob-bar">
                                                <div class="assessment-card-prob-fill" style=format!("width: {}%; background: {};", prob_pct, prob_color)></div>
                                            </div>
                                            <span style="font-size: 0.85rem; font-weight: 600;">{format!("{}%", prob_pct)}</span>
                                            <i class=shift_icon style=format!("color: {}; font-size: 0.8rem;", shift_color)></i>
                                        </div>
                                    </div>
                                    <div style="display: flex; justify-content: space-between; font-size: 0.8rem; opacity: 0.7;">
                                        <span><i class="fa-solid fa-file-lines"></i>" "{a.evidence_count.unwrap_or(0)}" evidence"</span>
                                        <span>{a.status.clone().unwrap_or_default()}</span>
                                    </div>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            }}
        </div>

        // ── Detail modal ──
        <div class=move || if selected_detail.get().is_some() { "modal-overlay active" } else { "modal-overlay" }
            on:click=move |_| set_selected_detail.set(None)>
            <div class="modal assessment-detail-modal" on:click=|e| e.stop_propagation()>
                {move || selected_detail.get().map(|d| {
                    let label_eval = d.label.clone();
                    let label_ev = d.label.clone();
                    view! {
                        <div class="modal-header">
                            <h3>
                                <i class="fa-solid fa-scale-balanced"></i>" "{d.label.clone()}
                                {d.category.clone().map(|c| view! { <span class="badge" style="margin-left: 0.5rem;">{c}</span> })}
                                {d.status.clone().map(|s| view! { <span class="badge badge-active" style="margin-left: 0.5rem;">{s}</span> })}
                            </h3>
                            <button class="btn-icon modal-close" on:click=move |_| set_selected_detail.set(None)>
                                <i class="fa-solid fa-xmark"></i>
                            </button>
                        </div>
                        <div class="modal-body">
                            // Probability bar
                            {d.probability.map(|p| {
                                let pct = (p * 100.0) as u32;
                                let color = if p >= 0.7 { "#e74c3c" } else if p >= 0.4 { "#f1c40f" } else { "#2ecc71" };
                                view! {
                                    <div style="margin-bottom: 1rem;">
                                        <div style="display: flex; align-items: center; gap: 0.5rem;">
                                            <div style="flex: 1; height: 8px; background: #333; border-radius: 4px; overflow: hidden;">
                                                <div style=format!("width: {}%; height: 100%; background: {}; border-radius: 4px; transition: width 0.3s;", pct, color)></div>
                                            </div>
                                            <span style="font-weight: 700;">{format!("{}%", pct)}</span>
                                        </div>
                                    </div>
                                }
                            })}

                            // Description
                            {d.description.clone().map(|desc| view! { <p style="margin-bottom: 0.75rem;">{desc}</p> })}

                            // Timeframe
                            {d.timeframe.clone().map(|t| view! {
                                <div style="margin-bottom: 0.75rem;">
                                    <i class="fa-solid fa-calendar"></i>" "{t}
                                </div>
                            })}

                            // Watches
                            {(!d.watches.is_empty()).then(|| view! {
                                <div style="margin-bottom: 0.75rem;">
                                    <strong>"Watches: "</strong>
                                    {d.watches.iter().map(|w| view! {
                                        <span class="badge" style="margin: 2px;">{w.clone()}</span>
                                    }).collect::<Vec<_>>()}
                                </div>
                            })}

                            // Evidence table
                            {(!d.evidence.is_empty()).then(|| view! {
                                <h4 style="margin-top: 0.75rem; margin-bottom: 0.5rem;">
                                    <i class="fa-solid fa-file-lines"></i>" Evidence"
                                </h4>
                                <table class="data-table evidence-table">
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

                            // History table
                            {(!d.history.is_empty()).then(|| view! {
                                <h4 style="margin-top: 0.75rem; margin-bottom: 0.5rem;">
                                    <i class="fa-solid fa-clock-rotate-left"></i>" History"
                                </h4>
                                <table class="data-table evidence-table">
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

                            // Action buttons
                            <div class="button-group" style="margin-top: 1rem;">
                                <button class="btn btn-primary btn-sm"
                                    on:click=move |_| { evaluate.dispatch(label_eval.clone()); }>
                                    <i class="fa-solid fa-rotate"></i>" Evaluate"
                                </button>
                                <button class="btn btn-secondary btn-sm"
                                    on:click=move |_| { set_show_evidence.set(Some(label_ev.clone())); }>
                                    <i class="fa-solid fa-plus"></i>" Add Evidence"
                                </button>
                                <button class="btn btn-secondary btn-sm"
                                    on:click=move |_| set_selected_detail.set(None)>
                                    "Close"
                                </button>
                            </div>
                        </div>
                    }
                })}
            </div>
        </div>

        // ── Evidence modal ──
        <div class=move || if show_evidence.get().is_some() { "modal-overlay active" } else { "modal-overlay" }
            on:click=move |_| set_show_evidence.set(None)>
            <div class="modal" style="max-width: 420px;" on:click=|e| e.stop_propagation()>
                {move || show_evidence.get().map(|label| {
                    let label_submit = label.clone();
                    view! {
                        <div class="modal-header">
                            <h3><i class="fa-solid fa-plus"></i>" Add Evidence"</h3>
                            <button class="btn-icon modal-close" on:click=move |_| set_show_evidence.set(None)>
                                <i class="fa-solid fa-xmark"></i>
                            </button>
                        </div>
                        <div class="modal-body">
                            <p class="text-secondary" style="margin-bottom: 0.75rem;">"For: "{label}</p>
                            <div class="form-group" style="margin-bottom: 0.75rem;">
                                <label>"Entity"</label>
                                <input type="text" placeholder="Entity name..." prop:value=ev_entity
                                    on:input=move |ev| set_ev_entity.set(event_target_value(&ev)) />
                            </div>
                            <div class="form-group" style="margin-bottom: 0.75rem;">
                                <label>"Direction"</label>
                                <select prop:value=ev_direction
                                    on:change=move |ev| set_ev_direction.set(event_target_value(&ev))>
                                    <option value="supporting">"Supporting"</option>
                                    <option value="contradicting">"Contradicting"</option>
                                </select>
                            </div>
                            <div class="form-group" style="margin-bottom: 0.75rem;">
                                <label>"Weight"</label>
                                <input type="range" min="0" max="1" step="0.05"
                                    prop:value=ev_weight
                                    on:input=move |ev| set_ev_weight.set(event_target_value(&ev)) />
                                <div style="text-align: center; font-size: 0.85rem; opacity: 0.7;">
                                    {move || ev_weight.get()}
                                </div>
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
            </div>
        </div>

        // ── Assessment wizard ──
        <AssessmentWizard
            open=show_wizard
            on_close=Callback::new(move |_| set_show_wizard.set(false))
            on_created=on_created
        />
    }
}
