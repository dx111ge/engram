use leptos::prelude::*;

/// A connection to review, with tier and acceptance state.
#[derive(Clone, Debug)]
pub struct ReviewConnection {
    pub idx: usize,
    pub from: String,
    pub to: String,
    pub rel_type: String,
    pub confidence: f32,
    pub source: String,
    pub tier: String,      // "confirmed", "likely", "uncertain", "no_relation"
}

/// The user's review decisions.
#[derive(Clone, Debug, Default)]
pub struct ReviewDecisions {
    pub accepted: Vec<usize>,
    pub modified: Vec<(usize, String)>,  // (idx, new_rel_type)
    pub skipped: Vec<usize>,
}

/// Reusable relation review panel with 4-tier grouped display.
/// Used by onboarding wizard (seed phase 2) and ingest page.
#[component]
pub fn RelationReviewPanel(
    /// All connections to review.
    connections: ReadSignal<Vec<ReviewConnection>>,
    /// Known relation types for autocomplete dropdowns.
    known_rel_types: ReadSignal<Vec<String>>,
    /// Callback when user confirms their review.
    on_confirm: Callback<ReviewDecisions>,
    /// Whether this is currently submitting.
    #[prop(into, optional)]
    submitting: Option<ReadSignal<bool>>,
) -> impl IntoView {
    // Track acceptance state per connection: (idx, accepted, new_rel_type)
    let decisions = RwSignal::new(Vec::<(usize, bool, Option<String>)>::new());

    // Initialize decisions whenever connections change
    Effect::new(move || {
        let conns = connections.get();
        let initial: Vec<(usize, bool, Option<String>)> = conns.iter().map(|c| {
            let auto_accept = c.tier == "confirmed";
            (c.idx, auto_accept, None)
        }).collect();
        decisions.set(initial);
    });

    let toggle_accept = move |idx: usize| {
        decisions.update(|d| {
            if let Some(entry) = d.iter_mut().find(|(i, _, _)| *i == idx) {
                entry.1 = !entry.1;
            }
        });
    };

    let set_new_type = move |idx: usize, new_type: String| {
        decisions.update(|d| {
            if let Some(entry) = d.iter_mut().find(|(i, _, _)| *i == idx) {
                entry.1 = true; // auto-accept when retyped
                entry.2 = Some(new_type);
            }
        });
    };

    let do_confirm = move |_| {
        let d = decisions.get_untracked();
        let mut result = ReviewDecisions::default();
        for (idx, accepted, new_type) in &d {
            if let Some(nt) = new_type {
                result.modified.push((*idx, nt.clone()));
            } else if *accepted {
                result.accepted.push(*idx);
            } else {
                result.skipped.push(*idx);
            }
        }
        on_confirm.run(result);
    };

    let is_submitting = move || submitting.map(|s| s.get()).unwrap_or(false);

    view! {
        <div class="relation-review-panel">
            // Group by tier
            {move || {
                let conns = connections.get();
                let decs = decisions.get();

                let tiers: Vec<(&str, &str, &str, &str)> = vec![
                    ("confirmed", "Confirmed (SPARQL + high-confidence)", "fa-solid fa-circle-check", "#66bb6a"),
                    ("likely", "Likely (GLiNER2 50-70%)", "fa-solid fa-circle-question", "#4fc3f7"),
                    ("uncertain", "Uncertain (GLiNER2 < 50%)", "fa-solid fa-circle-exclamation", "#ffa726"),
                    ("no_relation", "Co-occurred but unclassified", "fa-solid fa-circle-xmark", "#ef5350"),
                ];

                tiers.into_iter().filter_map(|(tier_id, label, icon, color)| {
                    let tier_conns: Vec<&ReviewConnection> = conns.iter()
                        .filter(|c| c.tier == tier_id)
                        .collect();
                    if tier_conns.is_empty() { return None; }
                    let count = tier_conns.len();

                    Some(view! {
                        <div style="margin-bottom: 0.75rem;">
                            <div style="display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.35rem;">
                                <i class=icon style=format!("color: {};", color)></i>
                                <strong style="font-size: 0.85rem;">{label}</strong>
                                <span style="font-size: 0.75rem; color: rgba(255,255,255,0.4);">{format!("({} relations)", count)}</span>
                            </div>
                            <div style="border: 1px solid rgba(255,255,255,0.08); border-radius: 6px; overflow: hidden;">
                                {tier_conns.into_iter().map(|conn| {
                                    let idx = conn.idx;
                                    let from = conn.from.clone();
                                    let to = conn.to.clone();
                                    let rel = conn.rel_type.clone();
                                    let conf = conn.confidence;
                                    let is_no_rel = tier_id == "no_relation";

                                    let is_accepted = decs.iter()
                                        .find(|(i, _, _)| *i == idx)
                                        .map(|(_, a, _)| *a)
                                        .unwrap_or(false);
                                    let has_retype = decs.iter()
                                        .find(|(i, _, _)| *i == idx)
                                        .and_then(|(_, _, nt)| nt.clone());

                                    let row_bg = if is_accepted { "rgba(102,187,106,0.06)" } else { "transparent" };

                                    view! {
                                        <div style=format!("display: flex; align-items: center; gap: 0.5rem; padding: 5px 10px; border-bottom: 1px solid rgba(255,255,255,0.05); font-size: 0.82rem; background: {};", row_bg)>
                                            <input type="checkbox"
                                                prop:checked=is_accepted
                                                on:change=move |_| toggle_accept(idx)
                                            />
                                            <span style="color: #e0e0e0;"><strong>{from}</strong></span>
                                            <span style="color: rgba(255,255,255,0.4);">"\u{2192}"</span>
                                            {
                                                // Editable dropdown for all tiers
                                                let known = known_rel_types.get_untracked();
                                                let current = has_retype.unwrap_or(rel);
                                                let placeholder = if is_no_rel { "-- Pick type --" } else { &current };
                                                let placeholder_val = if is_no_rel { "skip".to_string() } else { current.clone() };
                                                view! {
                                                    <select
                                                        style="background: rgba(255,255,255,0.08); border: 1px solid rgba(255,255,255,0.15); color: #fff; padding: 2px 4px; font-size: 0.75rem; border-radius: 3px; max-width: 150px;"
                                                        on:change=move |ev| {
                                                            let val = event_target_value(&ev);
                                                            if !val.is_empty() && val != "skip" {
                                                                set_new_type(idx, val);
                                                            }
                                                        }
                                                    >
                                                        <option value=placeholder_val.clone() selected=true>{placeholder.to_string()}</option>
                                                        {known.into_iter().filter(|t| t.as_str() != placeholder_val).map(|t| {
                                                            let t2 = t.clone();
                                                            view! { <option value=t>{t2}</option> }
                                                        }).collect::<Vec<_>>()}
                                                    </select>
                                                }.into_any()
                                            }
                                            <span style="color: rgba(255,255,255,0.4);">"\u{2192}"</span>
                                            <span style="color: #e0e0e0;"><strong>{to}</strong></span>
                                            <span style="margin-left: auto; color: rgba(255,255,255,0.3); font-size: 0.7rem;">
                                                {format!("{:.0}%", conf * 100.0)}
                                            </span>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        </div>
                    })
                }).collect::<Vec<_>>()
            }}

            // Summary + Commit button
            <div style="display: flex; align-items: center; gap: 0.75rem; margin-top: 0.75rem;">
                <button class="btn btn-primary"
                    on:click=do_confirm
                    disabled=is_submitting
                >
                    {move || if is_submitting() {
                        view! { <span class="spinner"></span>" Committing..." }.into_any()
                    } else {
                        view! { <><i class="fa-solid fa-check-double"></i>" Commit Reviewed"</> }.into_any()
                    }}
                </button>
                <span style="font-size: 0.8rem; color: rgba(255,255,255,0.4);">
                    {move || {
                        let d = decisions.get();
                        let accepted = d.iter().filter(|(_, a, _)| *a).count();
                        let total = d.len();
                        format!("{} of {} accepted", accepted, total)
                    }}
                </span>
            </div>
        </div>
    }
}
