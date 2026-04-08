/// Bottom controls bar for the War Room: inject, vote, boost, continue, synthesize.

use leptos::prelude::*;
use crate::api::types::DebateAgent;

#[component]
pub fn ControlsBar(
    session_status: ReadSignal<String>,
    agents: ReadSignal<Vec<DebateAgent>>,
    inject_text: ReadSignal<String>,
    set_inject_text: WriteSignal<String>,
    inject_action: Action<(), ()>,
    continue_action: Action<(), ()>,
    synthesize_action: Action<(), ()>,
    loading: ReadSignal<bool>,
    progress_msg: ReadSignal<String>,
    current_round: ReadSignal<usize>,
    max_rounds: ReadSignal<usize>,
) -> impl IntoView {
    view! {
        <div style="padding: 0.5rem; background: var(--bg-card); border: 1px solid var(--border); border-radius: 8px;">
            {move || {
                let status = session_status.get();
                match status.as_str() {
                    "running" | "researching" | "generatingpanel" | "synthesizing" => {
                        // Running: show progress
                        view! {
                            <div style="display: flex; align-items: center; gap: 0.5rem;">
                                <i class="fa-solid fa-spinner fa-spin" style="color: var(--accent-bright);"></i>
                                <span style="font-size: 0.85rem;">{progress_msg.get()}</span>
                                <span class="badge badge-active" style="font-size: 0.75rem; margin-left: auto;">
                                    {format!("Round {}/{}", current_round.get() + 1, max_rounds.get())}
                                </span>
                            </div>
                        }.into_any()
                    }
                    "awaiting_input" => {
                        // User can inject, vote, continue, or synthesize
                        view! {
                            <div style="display: flex; flex-direction: column; gap: 0.5rem;">
                                // Vote row
                                <div style="display: flex; align-items: center; gap: 0.4rem; flex-wrap: wrap;">
                                    <span style="font-size: 0.8rem; color: var(--text-secondary);">
                                        <i class="fa-solid fa-star" style="color: var(--warning);"></i>" Strongest case?"
                                    </span>
                                    {move || {
                                        agents.get().iter().map(|a| {
                                            let name = a.name.clone();
                                            let name_for_click = name.clone();
                                            let color = a.color.clone();
                                            let aid = a.id.clone();
                                            view! {
                                                <button class="btn btn-sm"
                                                    style={format!("border-color: {}; font-size: 0.75rem;", color)}
                                                    on:click=move |_| {
                                                        // Vote only prefills the inject text -- user can edit and send, or just continue
                                                        set_inject_text.set(format!("[VOTE:{}] I think {} made the strongest case.", aid, name_for_click));
                                                    }
                                                >
                                                    {name}
                                                </button>
                                            }
                                        }).collect::<Vec<_>>()
                                    }}
                                </div>

                                // Inject + action buttons row
                                <div style="display: flex; gap: 0.4rem; align-items: center;">
                                    <input
                                        type="text"
                                        placeholder="Inject a question or boost a topic..."
                                        style="flex: 1; font-size: 0.85rem;"
                                        prop:value=inject_text
                                        on:input=move |ev| set_inject_text.set(event_target_value(&ev))
                                        on:keydown=move |ev| {
                                            if ev.key() == "Enter" && !inject_text.get_untracked().is_empty() {
                                                let _ = inject_action.dispatch(());
                                            }
                                        }
                                    />
                                    <button class="btn btn-sm btn-primary"
                                        disabled=move || inject_text.get().is_empty()
                                        on:click=move |_| { let _ = inject_action.dispatch(()); }>
                                        <i class="fa-solid fa-paper-plane"></i>
                                    </button>
                                    <button class="btn btn-sm btn-secondary"
                                        on:click=move |_| { let _ = continue_action.dispatch(()); }>
                                        <i class="fa-solid fa-forward"></i>" Continue"
                                    </button>
                                    <button class="btn btn-sm btn-success"
                                        on:click=move |_| { let _ = synthesize_action.dispatch(()); }>
                                        <i class="fa-solid fa-flask"></i>" Synthesize"
                                    </button>
                                </div>
                            </div>
                        }.into_any()
                    }
                    "all_rounds_complete" => {
                        view! {
                            <div style="display: flex; align-items: center; gap: 0.5rem;">
                                <span style="font-size: 0.85rem;">
                                    <i class="fa-solid fa-trophy" style="color: var(--warning);"></i>
                                    " All rounds complete."
                                </span>
                                <button class="btn btn-success" style="margin-left: auto;"
                                    disabled=move || loading.get()
                                    on:click=move |_| { let _ = synthesize_action.dispatch(()); }>
                                    <i class="fa-solid fa-flask"></i>" Generate Synthesis"
                                </button>
                            </div>
                        }.into_any()
                    }
                    "complete" => {
                        view! {
                            <div style="display: flex; align-items: center; gap: 0.5rem;">
                                <span class="badge badge-active">
                                    <i class="fa-solid fa-check"></i>" Synthesis complete"
                                </span>
                            </div>
                        }.into_any()
                    }
                    _ => {
                        view! {
                            <div class="text-muted" style="font-size: 0.85rem; text-align: center;">
                                "Waiting..."
                            </div>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}
