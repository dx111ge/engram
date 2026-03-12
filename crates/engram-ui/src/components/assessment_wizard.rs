use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::AssessmentCreate;

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
    let (watches_text, set_watches_text) = signal(String::new());
    let (result_msg, set_result_msg) = signal(Option::<String>::None);

    let overlay_class = move || {
        if open.get() { "modal-overlay active" } else { "modal-overlay" }
    };
    let close = move |_| {
        set_step.set(1);
        on_close.run(());
    };

    let api_create = api.clone();
    let do_create = Action::new_local(move |_: &()| {
        let api = api_create.clone();
        let watches: Vec<String> = watches_text.get_untracked()
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
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
                    if let Some(cb) = on_created { cb.run(()); }
                }
                Err(e) => set_result_msg.set(Some(format!("Error: {e}"))),
            }
        }
    });

    view! {
        <div class=overlay_class on:click=close>
            <div class="modal" style="max-width: 550px;" on:click=|e| e.stop_propagation()>
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
                                    <input type="text" placeholder="e.g. Russia will invade by Q3"
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
                                            <option value="other">"Other"</option>
                                        </select>
                                    </div>
                                    <div class="form-group">
                                        <label>"Timeframe"</label>
                                        <input type="text" placeholder="e.g. 2026-Q3"
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
                                <button class="btn btn-primary" on:click=move |_| set_step.set(2)>
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
                                    <button class="btn btn-primary" on:click=move |_| set_step.set(3)>
                                        " Next" <i class="fa-solid fa-arrow-right"></i>
                                    </button>
                                </div>
                            </div>
                        }.into_any(),
                        _ => view! {
                            <div>
                                <div class="form-group">
                                    <label>"Watched Entities (one per line)"</label>
                                    <textarea placeholder="entity1\nentity2\n..."
                                        rows="4"
                                        prop:value=watches_text
                                        on:input=move |ev| set_watches_text.set(event_target_value(&ev)) />
                                    <p class="help-text">"These entities will be monitored for changes that affect the assessment."</p>
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
