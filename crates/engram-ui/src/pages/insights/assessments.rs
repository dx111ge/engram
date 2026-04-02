use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{AddEvidenceRequest, Assessment, AssessmentDetail, UpdateAssessmentRequest};
use crate::components::assessment_wizard::AssessmentWizard;
use crate::components::chat_types::ChatCurrentAssessment;

#[component]
pub fn AssessmentsZone(set_status_msg: WriteSignal<String>) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");
    let (assessments, set_assessments) = signal(Vec::<Assessment>::new());
    let (expanded_label, set_expanded_label) = signal(Option::<String>::None);
    let (expanded_detail, set_expanded_detail) = signal(Option::<AssessmentDetail>::None);
    let (show_wizard, set_show_wizard) = signal(false);
    let (show_evidence, set_show_evidence) = signal(Option::<String>::None);

    // Evidence form signals
    let (ev_entity, set_ev_entity) = signal(String::new());
    let (ev_direction, set_ev_direction) = signal("supporting".to_string());
    let (ev_weight, set_ev_weight) = signal("0.5".to_string());

    // Watch suggestions signals
    let (watch_suggestions, set_watch_suggestions) = signal(Vec::<serde_json::Value>::new());
    let (watch_loading, set_watch_loading) = signal(false);

    // ── Filter / Sort signals ──
    let (search_text, set_search_text) = signal(String::new());
    let (filter_category, set_filter_category) = signal("All".to_string());
    let (filter_status, set_filter_status) = signal("All".to_string());
    let (sort_by, set_sort_by) = signal("Probability".to_string());

    // ── Edit-mode signals ──
    let (editing, set_editing) = signal(false);
    let (edit_title, set_edit_title) = signal(String::new());
    let (edit_description, set_edit_description) = signal(String::new());
    let (edit_category, set_edit_category) = signal(String::new());
    let (edit_timeframe, set_edit_timeframe) = signal(String::new());
    let (edit_success_criteria, set_edit_success_criteria) = signal(String::new());
    let (edit_tags, set_edit_tags) = signal(String::new());

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

    // Load detail for inline expansion
    let chat_assessment = use_context::<ChatCurrentAssessment>();
    let api_detail = api.clone();
    let load_detail = Action::new_local(move |label: &String| {
        let api = api_detail.clone();
        let label = label.clone();
        async move {
            if let Some(ctx) = chat_assessment { ctx.0.set(Some(label.clone())); }
            let path = format!("/assessments/{}", js_sys::encode_uri_component(&label));
            match api.get::<AssessmentDetail>(&path).await {
                Ok(d) => {
                    set_expanded_label.set(Some(label));
                    set_editing.set(false);
                    set_expanded_detail.set(Some(d));
                }
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
        let label2 = label.clone();
        let path = format!("/assessments/{}/evaluate", js_sys::encode_uri_component(label));
        async move {
            let body = serde_json::json!({});
            match api.post_text(&path, &body).await {
                Ok(r) => {
                    set_status_msg.set(format!("Evaluated: {r}"));
                    reload.dispatch(());
                    let detail_path = format!("/assessments/{}", js_sys::encode_uri_component(&label2));
                    if let Ok(refreshed) = api.get::<AssessmentDetail>(&detail_path).await {
                        set_expanded_detail.set(Some(refreshed));
                    }
                }
                Err(e) => set_status_msg.set(format!("Evaluate error: {e}")),
            }
        }
    });

    // Delete assessment
    let api_delete = api.clone();
    let load_assessments4 = load_assessments.clone();
    let delete_assessment = Action::new_local(move |label: &String| {
        let api = api_delete.clone();
        let reload = load_assessments4.clone();
        let path = format!("/assessments/{}", js_sys::encode_uri_component(label));
        async move {
            match api.delete(&path).await {
                Ok(_) => {
                    set_status_msg.set("Assessment deleted".to_string());
                    set_expanded_label.set(None);
                    set_expanded_detail.set(None);
                    reload.dispatch(());
                }
                Err(e) => set_status_msg.set(format!("Delete error: {e}")),
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
                    if expanded_label.get_untracked().as_deref() == Some(&label2) {
                        let path2 = format!("/assessments/{}", js_sys::encode_uri_component(&label2));
                        if let Ok(refreshed) = api.get::<AssessmentDetail>(&path2).await {
                            set_expanded_detail.set(Some(refreshed));
                        }
                    }
                }
                Err(e) => set_status_msg.set(format!("Evidence error: {e}")),
            }
        }
    });

    // ── PATCH update (inline edit save) ──
    let api_patch = api.clone();
    let load_assessments_patch = load_assessments.clone();
    let save_edit = Action::new_local(move |label: &String| {
        let api = api_patch.clone();
        let reload = load_assessments_patch.clone();
        let label = label.clone();
        let path = format!("/assessments/{}", js_sys::encode_uri_component(&label));
        let title_val = edit_title.get_untracked();
        let desc_val = edit_description.get_untracked();
        let cat_val = edit_category.get_untracked();
        let tf_val = edit_timeframe.get_untracked();
        let sc_val = edit_success_criteria.get_untracked();
        let tags_val = edit_tags.get_untracked();
        let tags_vec: Vec<String> = tags_val.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        let body = UpdateAssessmentRequest {
            title: if title_val.is_empty() { None } else { Some(title_val) },
            description: if desc_val.is_empty() { None } else { Some(desc_val) },
            category: if cat_val.is_empty() { None } else { Some(cat_val) },
            timeframe: if tf_val.is_empty() { None } else { Some(tf_val) },
            success_criteria: if sc_val.is_empty() { None } else { Some(sc_val) },
            tags: if tags_vec.is_empty() { None } else { Some(tags_vec) },
            status: None,
            resolution: None,
        };
        async move {
            match api.patch::<UpdateAssessmentRequest, serde_json::Value>(&path, &body).await {
                Ok(_) => {
                    set_status_msg.set("Assessment updated".to_string());
                    set_editing.set(false);
                    reload.dispatch(());
                    // Refresh detail
                    let detail_path = format!("/assessments/{}", js_sys::encode_uri_component(&label));
                    if let Ok(refreshed) = api.get::<AssessmentDetail>(&detail_path).await {
                        set_expanded_detail.set(Some(refreshed));
                    }
                }
                Err(e) => set_status_msg.set(format!("Update error: {e}")),
            }
        }
    });

    // ── Load watch suggestions for an assessment ──
    let api_suggest = api.clone();
    let load_watch_suggestions = Action::new_local(move |label: &String| {
        let api = api_suggest.clone();
        let path = format!("/assessments/{}/suggest-watches", js_sys::encode_uri_component(label));
        set_watch_loading.set(true);
        set_watch_suggestions.set(vec![]);
        async move {
            let body = serde_json::json!({});
            match api.post_text(&path, &body).await {
                Ok(text) => {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(arr) = parsed.get("suggestions").and_then(|v| v.as_array()) {
                            set_watch_suggestions.set(arr.clone());
                        }
                    }
                }
                Err(e) => set_status_msg.set(format!("Suggest watches error: {e}")),
            }
            set_watch_loading.set(false);
        }
    });

    // ── Add a suggested watch to an assessment ──
    let api_add_watch = api.clone();
    let load_assessments_watch = load_assessments.clone();
    let add_watch = Action::new_local(move |args: &(String, String)| {
        let api = api_add_watch.clone();
        let reload = load_assessments_watch.clone();
        let (label, entity) = args.clone();
        let path = format!("/assessments/{}/watch", js_sys::encode_uri_component(&label));
        let body = serde_json::json!({ "entity_label": entity });
        async move {
            match api.post_text(&path, &body).await {
                Ok(_) => {
                    set_status_msg.set(format!("Watch added: {}", entity));
                    // Remove from suggestions list
                    set_watch_suggestions.update(|list| {
                        list.retain(|s| s.get("label").and_then(|v| v.as_str()) != Some(&entity));
                    });
                    reload.dispatch(());
                    // Refresh detail
                    let detail_path = format!("/assessments/{}", js_sys::encode_uri_component(&label));
                    if let Ok(refreshed) = api.get::<AssessmentDetail>(&detail_path).await {
                        set_expanded_detail.set(Some(refreshed));
                    }
                }
                Err(e) => set_status_msg.set(format!("Add watch error: {e}")),
            }
        }
    });

    // ── PATCH status change (instant) ──
    let api_status = api.clone();
    let load_assessments_status = load_assessments.clone();
    let update_status = Action::new_local(move |args: &(String, String)| {
        let api = api_status.clone();
        let reload = load_assessments_status.clone();
        let (label, new_status) = args.clone();
        let path = format!("/assessments/{}", js_sys::encode_uri_component(&label));
        let body = UpdateAssessmentRequest {
            title: None,
            description: None,
            category: None,
            timeframe: None,
            success_criteria: None,
            tags: None,
            status: Some(new_status.clone()),
            resolution: Some(new_status),
        };
        async move {
            match api.patch::<UpdateAssessmentRequest, serde_json::Value>(&path, &body).await {
                Ok(_) => {
                    set_status_msg.set("Status updated".to_string());
                    reload.dispatch(());
                    let detail_path = format!("/assessments/{}", js_sys::encode_uri_component(&label));
                    if let Ok(refreshed) = api.get::<AssessmentDetail>(&detail_path).await {
                        set_expanded_detail.set(Some(refreshed));
                    }
                }
                Err(e) => set_status_msg.set(format!("Status update error: {e}")),
            }
        }
    });

    let reload_after_create = load_assessments.clone();
    let on_created = Callback::new(move |_: ()| {
        reload_after_create.dispatch(());
    });

    // ── Derive unique categories from loaded assessments ──
    let categories = Memo::new(move |_| {
        let list = assessments.get();
        let mut cats: Vec<String> = list.iter().filter_map(|a| a.category.clone()).collect();
        cats.sort();
        cats.dedup();
        cats
    });

    // ── Filtered + sorted list ──
    let filtered_list = Memo::new(move |_| {
        let mut list = assessments.get();
        let search = search_text.get().to_lowercase();
        let cat = filter_category.get();
        let status = filter_status.get();
        let sort = sort_by.get();

        // Text search
        if !search.is_empty() {
            list.retain(|a| {
                a.label.to_lowercase().contains(&search)
                    || a.title.as_deref().unwrap_or("").to_lowercase().contains(&search)
                    || a.description.as_deref().unwrap_or("").to_lowercase().contains(&search)
            });
        }

        // Category filter
        if cat != "All" {
            list.retain(|a| a.category.as_deref() == Some(cat.as_str()));
        }

        // Status filter
        if status != "All" {
            list.retain(|a| {
                a.status.as_deref() == Some(status.as_str())
                    || a.resolution.as_deref() == Some(status.as_str())
            });
        }

        // Sort
        match sort.as_str() {
            "Evidence Count" => list.sort_by(|a, b| {
                b.evidence_count.unwrap_or(0).cmp(&a.evidence_count.unwrap_or(0))
            }),
            "Recent" => list.sort_by(|a, b| {
                let a_ts = a.last_evaluated.as_deref().unwrap_or("");
                let b_ts = b.last_evaluated.as_deref().unwrap_or("");
                b_ts.cmp(a_ts)
            }),
            _ => list.sort_by(|a, b| {
                let ap = a.probability.or(a.current_probability).unwrap_or(0.0);
                let bp = b.probability.or(b.current_probability).unwrap_or(0.0);
                bp.partial_cmp(&ap).unwrap_or(std::cmp::Ordering::Equal)
            }),
        }

        list
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

            // ── Search & Filter Bar ──
            <div style="display: flex; flex-wrap: wrap; gap: 0.5rem; margin-bottom: 1rem; align-items: center;">
                <div style="flex: 1; min-width: 180px;">
                    <div style="position: relative;">
                        <i class="fa-solid fa-search" style="position: absolute; left: 0.6rem; top: 50%; transform: translateY(-50%); opacity: 0.5; font-size: 0.8rem;"></i>
                        <input type="text" placeholder="Search assessments..."
                            style="width: 100%; padding-left: 2rem; font-size: 0.85rem;"
                            prop:value=search_text
                            on:input=move |ev| set_search_text.set(event_target_value(&ev)) />
                    </div>
                </div>
                <select style="font-size: 0.85rem; min-width: 120px;"
                    prop:value=filter_category
                    on:change=move |ev| set_filter_category.set(event_target_value(&ev))>
                    <option value="All">"All Categories"</option>
                    {move || categories.get().into_iter().map(|c| {
                        let c2 = c.clone();
                        view! { <option value=c>{c2}</option> }
                    }).collect::<Vec<_>>()}
                </select>
                <select style="font-size: 0.85rem; min-width: 120px;"
                    prop:value=filter_status
                    on:change=move |ev| set_filter_status.set(event_target_value(&ev))>
                    <option value="All">"All Statuses"</option>
                    <option value="active">"Active"</option>
                    <option value="confirmed">"Confirmed"</option>
                    <option value="denied">"Denied"</option>
                    <option value="inconclusive">"Inconclusive"</option>
                    <option value="superseded">"Superseded"</option>
                </select>
                <select style="font-size: 0.85rem; min-width: 120px;"
                    prop:value=sort_by
                    on:change=move |ev| set_sort_by.set(event_target_value(&ev))>
                    <option value="Probability">"Sort: Probability"</option>
                    <option value="Evidence Count">"Sort: Evidence"</option>
                    <option value="Recent">"Sort: Recent"</option>
                </select>
            </div>

            // Assessment cards grid
            {move || {
                let list = filtered_list.get();
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
                            let label_is_expanded = label.clone();
                            let prob = a.probability.or(a.current_probability).unwrap_or(0.0);
                            let prob_pct = (prob * 100.0) as u32;
                            let prob_color = if prob >= 0.7 { "var(--danger, #e74c3c)" } else if prob >= 0.4 { "var(--warning, #f1c40f)" } else { "var(--success, #2ecc71)" };
                            let shift = a.probability_shift.or(a.last_shift.map(|v| v as f64)).unwrap_or(0.0);
                            let shift_icon = if shift > 0.0 { "fa-solid fa-arrow-trend-up" } else if shift < 0.0 { "fa-solid fa-arrow-trend-down" } else { "fa-solid fa-minus" };
                            let shift_color = if shift > 0.0 { "#e74c3c" } else if shift < 0.0 { "#2ecc71" } else { "#888" };
                            let lbl_class = label_is_expanded.clone();
                            let lbl_style = label_is_expanded.clone();
                            let lbl_click_check = label_is_expanded.clone();
                            let lbl_chevron = label_is_expanded.clone();
                            let lbl_body = label_is_expanded.clone();
                            let resolution = a.resolution.clone().or_else(|| a.status.clone()).unwrap_or_default();
                            let res_color = resolution_color(&resolution);
                            let card_tags = a.tags.clone();
                            let pending_count = a.pending_count;
                            let is_stale = a.stale;

                            view! {
                                <div
                                    class=move || if expanded_label.get() == Some(lbl_class.clone()) { "assessment-card assessment-card-expanded" } else { "assessment-card" }
                                    style=move || if expanded_label.get() == Some(lbl_style.clone()) { "order: -1; grid-column: 1 / -1;" } else { "" }
                                >
                                    // -- Compact header (always shown) --
                                    <div class="assessment-card-header"
                                        style="cursor: pointer;"
                                        on:click={
                                            let label_click = label_click.clone();
                                            let lbl_check = lbl_click_check.clone();
                                            move |_| {
                                                if expanded_label.get() == Some(lbl_check.clone()) {
                                                    set_expanded_label.set(None);
                                                    set_expanded_detail.set(None);
                                                    set_editing.set(false);
                                                } else {
                                                    load_detail.dispatch(label_click.clone());
                                                }
                                            }
                                        }
                                    >
                                        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.5rem;">
                                            <strong style="font-size: 0.95rem;">{label.clone()}</strong>
                                            <div style="display: flex; gap: 0.4rem; align-items: center;">
                                                <span class="badge" style=format!("background: {}; color: #fff;", res_color)>{resolution}</span>
                                                {a.category.clone().map(|c| view! { <span class="badge">{c}</span> })}
                                                {(pending_count > 0).then(|| view! {
                                                    <span class="badge" style="background: #f1c40f; color: #333; font-size: 0.7rem; padding: 0.1rem 0.35rem;">
                                                        <i class="fa-solid fa-bell" style="margin-right: 0.2rem; font-size: 0.65rem;"></i>
                                                        {format!("{} new", pending_count)}
                                                    </span>
                                                })}
                                                {is_stale.then(|| view! {
                                                    <span class="badge" style="background: #e67e22; color: #fff; font-size: 0.7rem; padding: 0.1rem 0.35rem;">
                                                        <i class="fa-solid fa-clock" style="margin-right: 0.2rem; font-size: 0.65rem;"></i>
                                                        "Review needed"
                                                    </span>
                                                })}
                                            </div>
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
                                        <div style="display: flex; justify-content: space-between; align-items: center; font-size: 0.8rem; opacity: 0.7;">
                                            <span><i class="fa-solid fa-file-lines"></i>" "{a.evidence_count.unwrap_or(0)}" evidence"</span>
                                            <div style="display: flex; gap: 0.3rem; align-items: center;">
                                                {card_tags.iter().map(|t| {
                                                    let t = t.clone();
                                                    view! { <span class="badge" style="font-size: 0.7rem; padding: 0.1rem 0.35rem; background: var(--surface-alt, #2a2a3e); color: var(--text-secondary, #a0a0b0);">{t}</span> }
                                                }).collect::<Vec<_>>()}
                                                <i class=move || if expanded_label.get() == Some(lbl_chevron.clone()) { "fa-solid fa-chevron-up" } else { "fa-solid fa-chevron-down" } style="font-size: 0.75rem;"></i>
                                            </div>
                                        </div>
                                    </div>

                                    // -- Expanded detail (conditionally rendered) --
                                    {move || {
                                        if expanded_label.get() != Some(lbl_body.clone()) {
                                            return view! { <div></div> }.into_any();
                                        }
                                        match expanded_detail.get() {
                                            None => view! {
                                                <div style="padding: 1rem; text-align: center; opacity: 0.6;">
                                                    <i class="fa-solid fa-spinner fa-spin"></i>" Loading..."
                                                </div>
                                            }.into_any(),
                                            Some(d) => {
                                                let label_eval = d.label.clone();
                                                let label_ev = d.label.clone();
                                                let label_del = d.label.clone();
                                                let label_del_confirm = d.label.clone();
                                                let label_save = d.label.clone();
                                                let label_status = d.label.clone();
                                                let label_suggest = d.label.clone();
                                                let d_for_edit = d.clone();
                                                let is_editing = editing.get();

                                                // Current resolution/status
                                                let detail_resolution = d.resolution.clone()
                                                    .or_else(|| d.status.clone())
                                                    .unwrap_or_else(|| "active".to_string());
                                                let detail_res_color = resolution_color(&detail_resolution);
                                                let detail_resolution2 = detail_resolution.clone();

                                                // Effective probability (prefer current_probability)
                                                let eff_prob = d.current_probability.or(d.probability);

                                                // Merge evidence lists for display
                                                let all_evidence: Vec<_> = {
                                                    let mut all = Vec::new();
                                                    for ev in d.evidence_for.iter() {
                                                        all.push((ev.clone(), "supporting".to_string()));
                                                    }
                                                    for ev in d.evidence_against.iter() {
                                                        all.push((ev.clone(), "contradicting".to_string()));
                                                    }
                                                    // Also include legacy evidence field
                                                    for ev in d.evidence.iter() {
                                                        let dir = ev.direction.clone().unwrap_or_else(|| "supporting".to_string());
                                                        all.push((ev.clone(), dir));
                                                    }
                                                    all
                                                };
                                                let has_evidence = !all_evidence.is_empty();

                                                view! {
                                                    <div class="assessment-expanded-body">
                                                        // ── Header with edit toggle + status dropdown ──
                                                        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.75rem;">
                                                            <div style="display: flex; align-items: center; gap: 0.5rem;">
                                                                <span class="badge" style=format!("background: {}; color: #fff; font-size: 0.8rem;", detail_res_color)>
                                                                    {detail_resolution.clone()}
                                                                </span>
                                                                // Status dropdown (always visible)
                                                                <select style="font-size: 0.8rem; padding: 0.15rem 0.3rem; background: var(--surface-alt, #1e1e2e); color: var(--text, #e0e0e0); border: 1px solid var(--border, #333); border-radius: 4px;"
                                                                    on:change={
                                                                        let label_status = label_status.clone();
                                                                        move |ev| {
                                                                            let new_val = event_target_value(&ev);
                                                                            if !new_val.is_empty() {
                                                                                update_status.dispatch((label_status.clone(), new_val));
                                                                            }
                                                                        }
                                                                    }
                                                                >
                                                                    <option value="" disabled=true selected=true>"Change status..."</option>
                                                                    <option value="active">"active"</option>
                                                                    <option value="confirmed">"confirmed"</option>
                                                                    <option value="denied">"denied"</option>
                                                                    <option value="inconclusive">"inconclusive"</option>
                                                                    <option value="superseded">"superseded"</option>
                                                                </select>
                                                            </div>
                                                            <button class="btn-icon" title="Toggle edit mode"
                                                                style="opacity: 0.7; font-size: 0.9rem;"
                                                                on:click={
                                                                    let d_for_edit = d_for_edit.clone();
                                                                    move |_| {
                                                                        let currently = editing.get_untracked();
                                                                        if !currently {
                                                                            // Populate edit signals
                                                                            set_edit_title.set(d_for_edit.title.clone().unwrap_or_else(|| d_for_edit.label.clone()));
                                                                            set_edit_description.set(d_for_edit.description.clone().unwrap_or_default());
                                                                            set_edit_category.set(d_for_edit.category.clone().unwrap_or_default());
                                                                            set_edit_timeframe.set(d_for_edit.timeframe.clone().unwrap_or_default());
                                                                            set_edit_success_criteria.set(d_for_edit.success_criteria.clone().unwrap_or_default());
                                                                            set_edit_tags.set(d_for_edit.tags.join(", "));
                                                                        }
                                                                        set_editing.set(!currently);
                                                                    }
                                                                }
                                                            >
                                                                <i class=move || if editing.get() { "fa-solid fa-xmark" } else { "fa-solid fa-pencil" }></i>
                                                            </button>
                                                        </div>

                                                        // ── Edit mode form ──
                                                        {if is_editing {
                                                            let label_save2 = label_save.clone();
                                                            view! {
                                                                <div style="display: flex; flex-direction: column; gap: 0.6rem; margin-bottom: 1rem; padding: 0.75rem; background: var(--surface-alt, #1a1a2e); border-radius: 6px; border: 1px solid var(--border, #333);">
                                                                    <div class="form-group">
                                                                        <label style="font-size: 0.8rem; opacity: 0.7;">"Title"</label>
                                                                        <input type="text" prop:value=edit_title
                                                                            on:input=move |ev| set_edit_title.set(event_target_value(&ev))
                                                                            style="font-size: 0.85rem;" />
                                                                    </div>
                                                                    <div class="form-group">
                                                                        <label style="font-size: 0.8rem; opacity: 0.7;">"Description"</label>
                                                                        <textarea prop:value=edit_description
                                                                            on:input=move |ev| set_edit_description.set(event_target_value(&ev))
                                                                            style="font-size: 0.85rem; min-height: 60px;" />
                                                                    </div>
                                                                    <div style="display: flex; gap: 0.5rem;">
                                                                        <div class="form-group" style="flex: 1;">
                                                                            <label style="font-size: 0.8rem; opacity: 0.7;">"Category"</label>
                                                                            <input type="text" prop:value=edit_category
                                                                                on:input=move |ev| set_edit_category.set(event_target_value(&ev))
                                                                                style="font-size: 0.85rem;" />
                                                                        </div>
                                                                        <div class="form-group" style="flex: 1;">
                                                                            <label style="font-size: 0.8rem; opacity: 0.7;">"Timeframe"</label>
                                                                            <input type="text" prop:value=edit_timeframe
                                                                                on:input=move |ev| set_edit_timeframe.set(event_target_value(&ev))
                                                                                style="font-size: 0.85rem;" />
                                                                        </div>
                                                                    </div>
                                                                    <div class="form-group">
                                                                        <label style="font-size: 0.8rem; opacity: 0.7;"><i class="fa-solid fa-bullseye" style="margin-right: 0.3rem;"></i>"Success Criteria"</label>
                                                                        <textarea prop:value=edit_success_criteria
                                                                            on:input=move |ev| set_edit_success_criteria.set(event_target_value(&ev))
                                                                            style="font-size: 0.85rem; min-height: 50px;" />
                                                                    </div>
                                                                    <div class="form-group">
                                                                        <label style="font-size: 0.8rem; opacity: 0.7;"><i class="fa-solid fa-tags" style="margin-right: 0.3rem;"></i>"Tags (comma-separated)"</label>
                                                                        <input type="text" prop:value=edit_tags
                                                                            on:input=move |ev| set_edit_tags.set(event_target_value(&ev))
                                                                            style="font-size: 0.85rem;" />
                                                                    </div>
                                                                    <div style="display: flex; gap: 0.5rem;">
                                                                        <button class="btn btn-primary btn-sm"
                                                                            on:click=move |_| { save_edit.dispatch(label_save2.clone()); }>
                                                                            <i class="fa-solid fa-check"></i>" Save"
                                                                        </button>
                                                                        <button class="btn btn-secondary btn-sm"
                                                                            on:click=move |_| set_editing.set(false)>
                                                                            "Cancel"
                                                                        </button>
                                                                    </div>
                                                                </div>
                                                            }.into_any()
                                                        } else {
                                                            // ── Read-only detail view ──
                                                            let d_title = d.title.clone();
                                                            let d_desc = d.description.clone();
                                                            let d_timeframe = d.timeframe.clone();
                                                            let d_success = d.success_criteria.clone();
                                                            let d_tags = d.tags.clone();

                                                            view! {
                                                                <div style="margin-bottom: 1rem;">
                                                                    // Title (if different from label)
                                                                    {d_title.filter(|t| t != &d.label).map(|t| view! {
                                                                        <h4 style="margin: 0 0 0.5rem 0; font-size: 1rem;">{t}</h4>
                                                                    })}

                                                                    // Probability bar (large)
                                                                    {eff_prob.map(|p| {
                                                                        let pct = (p * 100.0) as u32;
                                                                        let color = if p >= 0.7 { "#e74c3c" } else if p >= 0.4 { "#f1c40f" } else { "#2ecc71" };
                                                                        view! {
                                                                            <div style="margin-bottom: 0.75rem;">
                                                                                <div style="display: flex; align-items: center; gap: 0.5rem;">
                                                                                    <div style="flex: 1; height: 10px; background: #333; border-radius: 5px; overflow: hidden;">
                                                                                        <div style=format!("width: {}%; height: 100%; background: {}; border-radius: 5px; transition: width 0.3s;", pct, color)></div>
                                                                                    </div>
                                                                                    <span style="font-weight: 700; font-size: 1.1rem;">{format!("{}%", pct)}</span>
                                                                                </div>
                                                                            </div>
                                                                        }
                                                                    })}

                                                                    // Description
                                                                    {d_desc.map(|desc| view! { <p style="margin-bottom: 0.75rem; opacity: 0.85;">{desc}</p> })}

                                                                    // Timeframe
                                                                    {d_timeframe.map(|t| view! {
                                                                        <div style="margin-bottom: 0.75rem; font-size: 0.9rem;">
                                                                            <i class="fa-solid fa-calendar"></i>" "{t}
                                                                        </div>
                                                                    })}

                                                                    // Success criteria
                                                                    {d_success.map(|sc| view! {
                                                                        <div style="margin-bottom: 0.75rem; font-size: 0.9rem; padding: 0.5rem; background: var(--surface-alt, #1a1a2e); border-radius: 4px; border-left: 3px solid var(--primary, #6c63ff);">
                                                                            <i class="fa-solid fa-bullseye" style="margin-right: 0.3rem; color: var(--primary, #6c63ff);"></i>
                                                                            <strong style="font-size: 0.8rem; opacity: 0.7;">"Success Criteria"</strong>
                                                                            <div style="margin-top: 0.3rem;">{sc}</div>
                                                                        </div>
                                                                    })}

                                                                    // Tags as colored badges
                                                                    {(!d_tags.is_empty()).then(|| {
                                                                        let tags = d_tags.clone();
                                                                        view! {
                                                                            <div style="display: flex; flex-wrap: wrap; gap: 0.3rem; margin-bottom: 0.75rem;">
                                                                                {tags.into_iter().map(|t| {
                                                                                    let bg = tag_color(&t);
                                                                                    view! {
                                                                                        <span class="badge" style=format!("background: {}; color: #fff; font-size: 0.75rem; padding: 0.15rem 0.45rem;", bg)>
                                                                                            <i class="fa-solid fa-tag" style="margin-right: 0.2rem; font-size: 0.65rem;"></i>{t}
                                                                                        </span>
                                                                                    }
                                                                                }).collect::<Vec<_>>()}
                                                                            </div>
                                                                        }
                                                                    })}
                                                                </div>
                                                            }.into_any()
                                                        }}

                                                        // Watches + Suggest new watches
                                                        <div style="margin-bottom: 1rem;">
                                                            <div style="display: flex; justify-content: space-between; align-items: center;">
                                                                <strong style="font-size: 0.85rem; text-transform: uppercase; opacity: 0.6;">"Watches"</strong>
                                                                <button class="btn btn-secondary btn-sm" style="font-size: 0.75rem; padding: 0.15rem 0.5rem;"
                                                                    on:click={
                                                                        let label_suggest = label_suggest.clone();
                                                                        move |_| { load_watch_suggestions.dispatch(label_suggest.clone()); }
                                                                    }>
                                                                    <i class="fa-solid fa-lightbulb" style="margin-right: 0.2rem;"></i>"Suggest watches"
                                                                </button>
                                                            </div>
                                                            {(!d.watches.is_empty()).then(|| view! {
                                                                <div style="margin-top: 0.35rem; display: flex; flex-wrap: wrap; gap: 0.3rem;">
                                                                    {d.watches.iter().map(|w| view! {
                                                                        <span class="badge">{w.clone()}</span>
                                                                    }).collect::<Vec<_>>()}
                                                                </div>
                                                            })}
                                                            // Watch suggestions panel
                                                            {move || {
                                                                let loading = watch_loading.get();
                                                                let suggestions = watch_suggestions.get();
                                                                if loading {
                                                                    return view! {
                                                                        <div style="margin-top: 0.5rem; font-size: 0.85rem; opacity: 0.7;">
                                                                            <i class="fa-solid fa-spinner fa-spin"></i>" Finding relevant entities..."
                                                                        </div>
                                                                    }.into_any();
                                                                }
                                                                if suggestions.is_empty() {
                                                                    return view! { <div></div> }.into_any();
                                                                }
                                                                let current_label = expanded_label.get().unwrap_or_default();
                                                                view! {
                                                                    <div style="margin-top: 0.5rem; padding: 0.5rem; background: var(--surface-alt, #1a1a2e); border-radius: 6px; border: 1px solid var(--border, #333);">
                                                                        <div style="font-size: 0.8rem; opacity: 0.7; margin-bottom: 0.4rem;">
                                                                            <i class="fa-solid fa-lightbulb" style="color: #f1c40f; margin-right: 0.3rem;"></i>
                                                                            "Suggested entities to watch:"
                                                                        </div>
                                                                        <div style="display: flex; flex-wrap: wrap; gap: 0.3rem;">
                                                                            {suggestions.into_iter().map(move |s| {
                                                                                let lbl = s.get("label").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                                                                let nt = s.get("node_type").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                                                                let ec = s.get("edge_count").and_then(|v| v.as_u64()).unwrap_or(0);
                                                                                let lbl_display = lbl.clone();
                                                                                let lbl_add = lbl.clone();
                                                                                let cl = current_label.clone();
                                                                                view! {
                                                                                    <button class="badge" style="cursor: pointer; border: 1px solid var(--border, #444); background: var(--surface, #252535); padding: 0.2rem 0.5rem; font-size: 0.75rem;"
                                                                                        title=format!("{} ({} edges)", nt, ec)
                                                                                        on:click=move |_| {
                                                                                            add_watch.dispatch((cl.clone(), lbl_add.clone()));
                                                                                        }>
                                                                                        <i class="fa-solid fa-plus" style="margin-right: 0.2rem; font-size: 0.65rem; color: #2ecc71;"></i>
                                                                                        {lbl_display}
                                                                                    </button>
                                                                                }
                                                                            }).collect::<Vec<_>>()}
                                                                        </div>
                                                                    </div>
                                                                }.into_any()
                                                            }}
                                                        </div>

                                                        // Pending evidence notice
                                                        {(d.pending_count > 0).then(|| {
                                                            let count = d.pending_count;
                                                            view! {
                                                                <div style="margin-bottom: 1rem; padding: 0.5rem 0.75rem; background: rgba(241, 196, 15, 0.1); border: 1px solid rgba(241, 196, 15, 0.3); border-radius: 6px; font-size: 0.85rem;">
                                                                    <i class="fa-solid fa-bell" style="color: #f1c40f; margin-right: 0.3rem;"></i>
                                                                    <strong>{format!("{} pending auto-detected evidence", count)}</strong>
                                                                    " -- click Evaluate to review and incorporate."
                                                                </div>
                                                            }
                                                        })}

                                                        // Stale alert in expanded view
                                                        {d.stale.then(|| view! {
                                                            <div style="margin-bottom: 1rem; padding: 0.5rem 0.75rem; background: rgba(230, 126, 34, 0.1); border: 1px solid rgba(230, 126, 34, 0.3); border-radius: 6px; font-size: 0.85rem;">
                                                                <i class="fa-solid fa-clock" style="color: #e67e22; margin-right: 0.3rem;"></i>
                                                                <strong>"Review needed"</strong>
                                                                " -- this assessment has not been evaluated in over 7 days."
                                                            </div>
                                                        })}

                                                        // Evidence table
                                                        {has_evidence.then(|| view! {
                                                            <div style="margin-bottom: 1rem;">
                                                                <h4 style="margin-bottom: 0.5rem; font-size: 0.9rem;">
                                                                    <i class="fa-solid fa-file-lines"></i>" Evidence"
                                                                </h4>
                                                                <table class="data-table evidence-table">
                                                                    <thead>
                                                                        <tr><th>"Entity"</th><th>"Direction"</th><th>"Weight"</th><th>"Source"</th></tr>
                                                                    </thead>
                                                                    <tbody>
                                                                        {all_evidence.iter().map(|(ev, dir)| {
                                                                            let entity = ev.node_label.clone().or_else(|| ev.entity.clone()).unwrap_or_default();
                                                                            let dir_icon = if dir == "supporting" { "fa-solid fa-arrow-up" } else { "fa-solid fa-arrow-down" };
                                                                            let dir_color = if dir == "supporting" { "#2ecc71" } else { "#e74c3c" };
                                                                            let weight = ev.confidence.or(ev.weight).map(|w| format!("{:.2}", w)).unwrap_or_default();
                                                                            let source = ev.source.clone().unwrap_or_default();
                                                                            let dir = dir.clone();
                                                                            view! {
                                                                                <tr>
                                                                                    <td>{entity}</td>
                                                                                    <td>
                                                                                        <i class=dir_icon style=format!("color: {}; margin-right: 0.3rem;", dir_color)></i>
                                                                                        {dir}
                                                                                    </td>
                                                                                    <td>{weight}</td>
                                                                                    <td>{source}</td>
                                                                                </tr>
                                                                            }
                                                                        }).collect::<Vec<_>>()}
                                                                    </tbody>
                                                                </table>
                                                            </div>
                                                        })}

                                                        // History table
                                                        {(!d.history.is_empty()).then(|| view! {
                                                            <div style="margin-bottom: 1rem;">
                                                                <h4 style="margin-bottom: 0.5rem; font-size: 0.9rem;">
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
                                                            </div>
                                                        })}

                                                        // Action bar
                                                        <div class="button-group" style="margin-top: 0.75rem; display: flex; gap: 0.5rem; flex-wrap: wrap;">
                                                            <button class="btn btn-primary btn-sm"
                                                                on:click=move |_| { evaluate.dispatch(label_eval.clone()); }>
                                                                <i class="fa-solid fa-rotate"></i>" Evaluate"
                                                            </button>
                                                            <button class="btn btn-secondary btn-sm"
                                                                on:click=move |_| { set_show_evidence.set(Some(label_ev.clone())); }>
                                                                <i class="fa-solid fa-plus"></i>" Add Evidence"
                                                            </button>
                                                            <button class="btn btn-sm" style="background: var(--danger, #e74c3c); color: #fff; border: none;"
                                                                on:click={
                                                                    let label_del = label_del.clone();
                                                                    let label_del_confirm = label_del_confirm.clone();
                                                                    move |_| {
                                                                        let msg = format!("Delete assessment {}?", label_del_confirm);
                                                                        let confirmed = web_sys::window()
                                                                            .and_then(|w| w.confirm_with_message(&msg).ok())
                                                                            .unwrap_or(false);
                                                                        if confirmed {
                                                                            delete_assessment.dispatch(label_del.clone());
                                                                        }
                                                                    }
                                                                }>
                                                                <i class="fa-solid fa-trash"></i>" Delete"
                                                            </button>
                                                            <button class="btn btn-secondary btn-sm"
                                                                on:click=move |_| {
                                                                    set_expanded_label.set(None);
                                                                    set_expanded_detail.set(None);
                                                                    set_editing.set(false);
                                                                }>
                                                                <i class="fa-solid fa-xmark"></i>" Close"
                                                            </button>
                                                        </div>
                                                    </div>
                                                }.into_any()
                                            }
                                        }
                                    }}
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            }}
        </div>

        // -- Evidence modal (kept as modal -- sub-form) --
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

        // -- Assessment wizard (kept as modal -- creation flow) --
        <AssessmentWizard
            open=show_wizard
            on_close=Callback::new(move |_| set_show_wizard.set(false))
            on_created=on_created
        />
    }
}

/// Map resolution/status to a badge color.
fn resolution_color(resolution: &str) -> &'static str {
    match resolution {
        "active" => "#2ecc71",
        "confirmed" => "#3498db",
        "denied" => "#e74c3c",
        "inconclusive" => "#888",
        "superseded" => "#95a5a6",
        _ => "#666",
    }
}

/// Deterministic tag color from a small palette.
fn tag_color(tag: &str) -> &'static str {
    const COLORS: &[&str] = &["#6c63ff", "#e67e22", "#1abc9c", "#9b59b6", "#e74c3c", "#3498db", "#2ecc71", "#f39c12"];
    let hash: usize = tag.bytes().map(|b| b as usize).sum();
    COLORS[hash % COLORS.len()]
}
