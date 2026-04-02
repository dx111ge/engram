use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api::ApiClient;
use crate::api::types::AssessmentCreate;

#[derive(Clone, Debug)]
struct SuggestedWatch {
    label: String,
    node_type: String,
    edge_count: u32,
    selected: bool,
}

#[component]
pub fn AssessmentWizard(
    #[prop(into)] open: ReadSignal<bool>,
    #[prop(into)] on_close: Callback<()>,
    #[prop(optional, into)] on_created: Option<Callback<()>>,
) -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    let (step, set_step) = signal(1u32);
    let (label, set_label) = signal(String::new());
    let (category, set_category) = signal(String::new());
    let (description, set_description) = signal(String::new());
    let (probability, set_probability) = signal(0.5f64);
    let (timeframe, set_timeframe) = signal(String::new());
    let (success_criteria, set_success_criteria) = signal(String::new());
    let (result_msg, set_result_msg) = signal(Option::<String>::None);

    // Step 3: suggested watches
    let (suggested_watches, set_suggested_watches) = signal(Vec::<SuggestedWatch>::new());
    let (loading_suggestions, set_loading_suggestions) = signal(false);
    let (manual_watch, set_manual_watch) = signal(String::new());

    let overlay_class = move || {
        if open.get() { "modal-overlay active" } else { "modal-overlay" }
    };
    let close = move |_| {
        set_step.set(1);
        on_close.run(());
    };

    // Fetch suggestions when entering step 3
    let api_suggest = api.clone();
    let fetch_suggestions = Action::new_local(move |_: &()| {
        let api = api_suggest.clone();
        let hypothesis = format!("{} {} {}", label.get_untracked(), description.get_untracked(), timeframe.get_untracked());
        async move {
            set_loading_suggestions.set(true);
            let body = serde_json::json!({ "hypothesis": hypothesis });
            match api.post::<_, serde_json::Value>("/assessments/suggest-watches", &body).await {
                Ok(resp) => {
                    if let Some(suggestions) = resp.get("suggestions").and_then(|v| v.as_array()) {
                        let watches: Vec<SuggestedWatch> = suggestions.iter().map(|s| {
                            SuggestedWatch {
                                label: s.get("label").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
                                node_type: s.get("node_type").and_then(|v| v.as_str()).unwrap_or("entity").to_string(),
                                edge_count: s.get("edge_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                                selected: true,
                            }
                        }).collect();
                        set_suggested_watches.set(watches);
                    }
                }
                Err(_) => { /* fall back to manual input */ }
            }
            set_loading_suggestions.set(false);
        }
    });

    let api_create = api.clone();
    let do_create = Action::new_local(move |_: &()| {
        let api = api_create.clone();
        let watches: Vec<String> = suggested_watches.get_untracked()
            .iter()
            .filter(|w| w.selected)
            .map(|w| w.label.clone())
            .collect();
        let sc = success_criteria.get_untracked();
        let body = AssessmentCreate {
            label: label.get_untracked(),
            category: {
                let c = category.get_untracked();
                if c.is_empty() { None } else { Some(c) }
            },
            description: {
                let d = description.get_untracked();
                if d.is_empty() { None } else { Some(d) }
            },
            probability: Some(probability.get_untracked()),
            timeframe: {
                let t = timeframe.get_untracked();
                if t.is_empty() { None } else { Some(t) }
            },
            watches: if watches.is_empty() { None } else { Some(watches) },
        };
        async move {
            set_result_msg.set(None);
            match api.post_text("/assessments", &body).await {
                Ok(_) => {
                    set_result_msg.set(Some("Assessment created".into()));
                    set_step.set(1);
                    set_label.set(String::new());
                    set_description.set(String::new());
                    set_success_criteria.set(String::new());
                    set_suggested_watches.set(Vec::new());
                    if let Some(cb) = on_created { cb.run(()); }
                    on_close.run(());
                }
                Err(e) => set_result_msg.set(Some(format!("Error: {e}"))),
            }
        }
    });

    view! {
        <div class=overlay_class on:click=close>
            <div class="modal" style="max-width: 580px;" on:click=|e| e.stop_propagation()>
                <div class="modal-header">
                    <h3><i class="fa-solid fa-scale-balanced"></i>" New Assessment"</h3>
                    <button class="btn-icon modal-close" on:click=close>
                        <i class="fa-solid fa-xmark"></i>
                    </button>
                </div>
                <div class="modal-body">
                    <div class="text-secondary mb-2" style="font-size: 0.85rem;">
                        {move || format!("Step {} of 3", step.get())}
                    </div>

                    {move || result_msg.get().map(|m| view! {
                        <div class="card" style="padding: 0.5rem; margin-bottom: 0.75rem;">
                            <i class="fa-solid fa-info-circle" style="color: var(--accent-bright);"></i>
                            " " {m}
                        </div>
                    })}

                    {move || match step.get() {
                        1 => view! {
                            <div>
                                <div class="form-group">
                                    <label>"Assessment Title"</label>
                                    <input type="text" placeholder="e.g. Will sanctions weaken Russia by 2027?"
                                        prop:value=label
                                        on:input=move |ev| set_label.set(event_target_value(&ev)) />
                                </div>
                                <div class="form-row">
                                    <div class="form-group">
                                        <label>"Category"</label>
                                        <select prop:value=category on:change=move |ev| set_category.set(event_target_value(&ev))>
                                            <option value="">"-- Select --"</option>
                                            <option value="geopolitical">"Geopolitical"</option>
                                            <option value="economic">"Economic"</option>
                                            <option value="technology">"Technology"</option>
                                            <option value="security">"Security"</option>
                                            <option value="social">"Social"</option>
                                            <option value="military">"Military"</option>
                                            <option value="financial">"Financial"</option>
                                            <option value="other">"Other"</option>
                                        </select>
                                    </div>
                                    <div class="form-group">
                                        <label>"Timeframe"</label>
                                        <input type="text" placeholder="e.g. by Q3 2027"
                                            prop:value=timeframe
                                            on:input=move |ev| set_timeframe.set(event_target_value(&ev)) />
                                    </div>
                                </div>
                                <div class="form-group">
                                    <label>"Description"</label>
                                    <textarea placeholder="Detailed hypothesis..."
                                        prop:value=description
                                        on:input=move |ev| set_description.set(event_target_value(&ev)) />
                                </div>
                                <div class="form-group">
                                    <label><i class="fa-solid fa-bullseye" style="margin-right: 4px;"></i>"Success Criteria"</label>
                                    <textarea placeholder="How will you verify this assessment? What would confirm or deny it?"
                                        rows="2"
                                        prop:value=success_criteria
                                        on:input=move |ev| set_success_criteria.set(event_target_value(&ev)) />
                                </div>
                                <button class="btn btn-primary"
                                    disabled=move || label.get().trim().is_empty()
                                    on:click=move |_| set_step.set(2)>
                                    " Next" <i class="fa-solid fa-arrow-right"></i>
                                </button>
                            </div>
                        }.into_any(),
                        2 => view! {
                            <div>
                                <div class="form-group">
                                    <label>"Initial Probability"</label>
                                    <input type="range" min="0" max="1" step="0.05"
                                        prop:value=move || probability.get().to_string()
                                        on:input=move |ev| {
                                            if let Ok(v) = event_target_value(&ev).parse() {
                                                set_probability.set(v);
                                            }
                                        } />
                                    <div class="flex-between">
                                        <span class="text-muted">"Very Unlikely"</span>
                                        <span style="font-weight: 700; color: var(--accent-bright);">
                                            {move || format!("{:.0}%", probability.get() * 100.0)}
                                        </span>
                                        <span class="text-muted">"Very Likely"</span>
                                    </div>
                                </div>
                                <div class="flex gap-sm">
                                    <button class="btn btn-secondary" on:click=move |_| set_step.set(1)>
                                        <i class="fa-solid fa-arrow-left"></i>" Back"
                                    </button>
                                    <button class="btn btn-primary" on:click=move |_| {
                                        set_step.set(3);
                                        fetch_suggestions.dispatch(());
                                    }>
                                        " Next" <i class="fa-solid fa-arrow-right"></i>
                                    </button>
                                </div>
                            </div>
                        }.into_any(),
                        _ => view! {
                            <div>
                                <h4 style="margin-bottom: 0.5rem;">
                                    <i class="fa-solid fa-eye"></i>" Entities to Watch"
                                </h4>
                                <p class="help-text" style="margin-bottom: 0.75rem;">
                                    "Based on your hypothesis, we suggest monitoring these entities. Check/uncheck to adjust."
                                </p>

                                // Loading state
                                {move || loading_suggestions.get().then(|| view! {
                                    <div style="text-align: center; padding: 1rem; opacity: 0.6;">
                                        <i class="fa-solid fa-spinner fa-spin"></i>" Finding relevant entities..."
                                    </div>
                                })}

                                // Suggested watches as checkboxes
                                {move || {
                                    let watches = suggested_watches.get();
                                    if watches.is_empty() && !loading_suggestions.get() {
                                        return view! {
                                            <div style="padding: 0.5rem; opacity: 0.6; font-size: 0.85rem;">
                                                "No entities found in graph matching your hypothesis. Add manually below."
                                            </div>
                                        }.into_any();
                                    }
                                    view! {
                                        <div style="max-height: 250px; overflow-y: auto; margin-bottom: 0.75rem;">
                                            {watches.iter().enumerate().map(|(idx, w)| {
                                                let label_text = w.label.clone();
                                                let node_type = w.node_type.clone();
                                                let ec = w.edge_count;
                                                let checked = w.selected;
                                                view! {
                                                    <label style="display: flex; align-items: center; gap: 0.5rem; padding: 4px 0; cursor: pointer; font-size: 0.9rem;">
                                                        <input type="checkbox"
                                                            prop:checked=checked
                                                            on:change=move |_| {
                                                                set_suggested_watches.update(|ws| {
                                                                    if let Some(w) = ws.get_mut(idx) {
                                                                        w.selected = !w.selected;
                                                                    }
                                                                });
                                                            }
                                                        />
                                                        <span style="font-weight: 500;">{label_text}</span>
                                                        <span class="badge" style="font-size: 0.7rem;">{node_type}</span>
                                                        <span style="font-size: 0.75rem; opacity: 0.5; margin-left: auto;">{format!("{ec} connections")}</span>
                                                    </label>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </div>
                                    }.into_any()
                                }}

                                // Manual add
                                <div style="display: flex; gap: 0.5rem; margin-bottom: 0.75rem;">
                                    <input type="text" placeholder="Add entity manually..."
                                        style="flex: 1;"
                                        prop:value=manual_watch
                                        on:input=move |ev| set_manual_watch.set(event_target_value(&ev))
                                        on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                                            if ev.key() == "Enter" {
                                                let name = manual_watch.get_untracked().trim().to_string();
                                                if !name.is_empty() {
                                                    set_suggested_watches.update(|ws| {
                                                        if !ws.iter().any(|w| w.label == name) {
                                                            ws.push(SuggestedWatch {
                                                                label: name,
                                                                node_type: "manual".to_string(),
                                                                edge_count: 0,
                                                                selected: true,
                                                            });
                                                        }
                                                    });
                                                    set_manual_watch.set(String::new());
                                                }
                                            }
                                        }
                                    />
                                    <button class="btn btn-sm" on:click=move |_| {
                                        let name = manual_watch.get_untracked().trim().to_string();
                                        if !name.is_empty() {
                                            set_suggested_watches.update(|ws| {
                                                if !ws.iter().any(|w| w.label == name) {
                                                    ws.push(SuggestedWatch {
                                                        label: name,
                                                        node_type: "manual".to_string(),
                                                        edge_count: 0,
                                                        selected: true,
                                                    });
                                                }
                                            });
                                            set_manual_watch.set(String::new());
                                        }
                                    }>
                                        <i class="fa-solid fa-plus"></i>
                                    </button>
                                </div>

                                // Selected count
                                <div style="font-size: 0.8rem; opacity: 0.6; margin-bottom: 0.75rem;">
                                    {move || {
                                        let count = suggested_watches.get().iter().filter(|w| w.selected).count();
                                        format!("{count} entities selected for monitoring")
                                    }}
                                </div>

                                <div class="flex gap-sm">
                                    <button class="btn btn-secondary" on:click=move |_| set_step.set(2)>
                                        <i class="fa-solid fa-arrow-left"></i>" Back"
                                    </button>
                                    <button class="btn btn-success" on:click=move |_| { do_create.dispatch(()); }>
                                        <i class="fa-solid fa-check"></i>" Create Assessment"
                                    </button>
                                </div>
                            </div>
                        }.into_any(),
                    }}
                </div>
            </div>
        </div>
    }
}
