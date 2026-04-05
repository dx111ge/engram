/// Multi-agent debate panel page.
/// UX: setup -> review/edit agents -> run rounds -> synthesis on top.
/// Rounds collapse after completion. Sticky controls. Clear round indicators.

use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{
    DebateAgent, DebateStartResponse, DebateSessionResponse,
    DebateRunResponse, DebateInjectResponse, DebateSynthesizeResponse,
    DebateRound, DebateTurn,
};

#[component]
pub fn DebatePage() -> impl IntoView {
    let api = use_context::<ApiClient>().expect("ApiClient context");

    // Setup state
    let (topic, set_topic) = signal(String::new());
    let (debate_mode, set_debate_mode) = signal("analyze".to_string());
    let (mode_input, set_mode_input) = signal(String::new());
    let (agent_count, set_agent_count) = signal("5".to_string());
    let (max_rounds_input, set_max_rounds_input) = signal("3".to_string());
    let (loading, set_loading) = signal(false);
    let (status_msg, set_status_msg) = signal(String::new());

    // Session state
    let (session_id, set_session_id) = signal(Option::<String>::None);
    let (session_status, set_session_status) = signal(String::new());
    let (agents, set_agents) = signal(Vec::<DebateAgent>::new());
    let (rounds, set_rounds) = signal(Vec::<DebateRound>::new());
    let (max_rounds, set_max_rounds) = signal(3usize);
    let (current_round, set_current_round) = signal(0usize);
    let (synthesis, set_synthesis) = signal(Option::<crate::api::types::DebateSynthesis>::None);

    // UI state
    let (inject_text, set_inject_text) = signal(String::new());
    let (synthesis_tab, set_synthesis_tab) = signal("evidence".to_string());
    let (polling, set_polling) = signal(false);
    let (progress_msg, set_progress_msg) = signal(String::new());
    let (expanded_round, set_expanded_round) = signal(Option::<usize>::None);
    let (editing_agent, set_editing_agent) = signal(Option::<String>::None);

    // Edit form signals
    let (edit_name, set_edit_name) = signal(String::new());
    let (edit_persona, set_edit_persona) = signal(String::new());
    let (edit_bias_label, set_edit_bias_label) = signal(String::new());
    let (edit_bias_desc, set_edit_bias_desc) = signal(String::new());
    let (edit_rigor, set_edit_rigor) = signal(String::new());
    let (edit_source, set_edit_source) = signal(String::new());

    // ── Reset for new debate ──
    let reset_all = move || {
        set_session_id.set(None);
        set_session_status.set(String::new());
        set_agents.set(Vec::new());
        set_rounds.set(Vec::new());
        set_synthesis.set(None);
        set_status_msg.set(String::new());
        set_topic.set(String::new());
        set_debate_mode.set("analyze".into());
        set_mode_input.set(String::new());
        set_current_round.set(0);
        set_expanded_round.set(None);
    };

    // ── Start debate ──
    let api_start = api.clone();
    let start_debate = Action::new_local(move |_: &()| {
        let api = api_start.clone();
        let t = topic.get_untracked();
        let ac: u8 = agent_count.get_untracked().parse().unwrap_or(5);
        let mr: u8 = max_rounds_input.get_untracked().parse().unwrap_or(3);
        async move {
            if t.trim().is_empty() {
                set_status_msg.set("Please enter a topic".into());
                return;
            }
            set_loading.set(true);
            set_status_msg.set("Generating debate panel...".into());
            let m = debate_mode.get_untracked();
            let mi = mode_input.get_untracked();
            let body = serde_json::json!({
                "topic": t, "mode": m, "agent_count": ac, "max_rounds": mr,
                "mode_input": if mi.is_empty() { None } else { Some(mi) }
            });
            match api.post::<_, DebateStartResponse>("/debate/start", &body).await {
                Ok(resp) => {
                    set_session_id.set(Some(resp.session_id));
                    set_session_status.set(resp.status);
                    set_agents.set(resp.agents);
                    set_rounds.set(Vec::new());
                    set_synthesis.set(None);
                    set_max_rounds.set(mr as usize);
                    set_status_msg.set("Panel generated. Review and edit agents, then start.".into());
                }
                Err(e) => set_status_msg.set(format!("Start error: {e}")),
            }
            set_loading.set(false);
        }
    });

    // ── Run debate ──
    let api_run = api.clone();
    let run_debate = Action::new_local(move |_: &()| {
        let api = api_run.clone();
        let sid = session_id.get_untracked();
        async move {
            if let Some(id) = sid {
                set_loading.set(true);
                set_status_msg.set("Debate running...".into());
                let path = format!("/debate/{}/run", js_sys::encode_uri_component(&id));
                match api.post::<_, DebateRunResponse>(&path, &serde_json::json!({})).await {
                    Ok(_) => {
                        set_session_status.set("running".into());
                        set_polling.set(true);
                    }
                    Err(e) => set_status_msg.set(format!("Run error: {e}")),
                }
                set_loading.set(false);
            }
        }
    });

    // ── Poll for updates ──
    let api_poll = api.clone();
    Effect::new(move |_| {
        if !polling.get() { return; }
        let api = api_poll.clone();
        leptos::task::spawn_local(async move {
            loop {
                if !polling.get_untracked() { break; }
                let sid = match session_id.get_untracked() {
                    Some(id) => id,
                    None => break,
                };
                let path = format!("/debate/{}", js_sys::encode_uri_component(&sid));
                match api.get::<DebateSessionResponse>(&path).await {
                    Ok(resp) => {
                        set_session_status.set(resp.status.clone());
                        set_agents.set(resp.agents);
                        set_current_round.set(resp.current_round);
                        set_max_rounds.set(resp.max_rounds);
                        // Update progress message
                        if let Some(p) = &resp.progress {
                            let msg = if p.total > 0 {
                                format!("{} ({}/{})", p.message, p.current, p.total)
                            } else {
                                p.message.clone()
                            };
                            set_progress_msg.set(msg);
                        }
                        // Auto-expand latest round
                        if resp.rounds.len() > rounds.get_untracked().len() {
                            set_expanded_round.set(Some(resp.rounds.len() - 1));
                            // Scroll to bottom
                            if let Some(w) = web_sys::window() {
                                let _ = w.scroll_to_with_x_and_y(0.0, 99999.0);
                            }
                        }
                        set_rounds.set(resp.rounds);
                        if let Some(syn) = resp.synthesis {
                            set_synthesis.set(Some(syn));
                        }
                        match resp.status.as_str() {
                            "awaiting_input" => {
                                let r = current_round.get_untracked();
                                let m = max_rounds.get_untracked();
                                set_status_msg.set(format!("Round {} of {} complete. Continue, inject a question, or synthesize.", r, m));
                                set_polling.set(false);
                                break;
                            }
                            "all_rounds_complete" => {
                                let m = max_rounds.get_untracked();
                                set_status_msg.set(format!("All {} rounds complete. Click Synthesize to generate the analysis.", m));
                                set_polling.set(false);
                                break;
                            }
                            "complete" => {
                                set_status_msg.set("Synthesis complete.".into());
                                set_polling.set(false);
                                // Scroll to top to show synthesis
                                if let Some(w) = web_sys::window() {
                                    let _ = w.scroll_to_with_x_and_y(0.0, 0.0);
                                }
                                break;
                            }
                            "error" => {
                                set_status_msg.set("Debate encountered an error.".into());
                                set_polling.set(false);
                                break;
                            }
                            _ => {} // still running
                        }
                    }
                    Err(e) => {
                        set_status_msg.set(format!("Poll error: {e}"));
                        set_polling.set(false);
                        break;
                    }
                }
                gloo_timers::future::TimeoutFuture::new(3_000).await;
            }
        });
    });

    // ── Inject question ──
    let api_inject = api.clone();
    let inject_action = Action::new_local(move |_: &()| {
        let api = api_inject.clone();
        let sid = session_id.get_untracked();
        let msg = inject_text.get_untracked();
        async move {
            if let Some(id) = sid {
                if msg.trim().is_empty() {
                    set_status_msg.set("Enter a question to inject".into());
                    return;
                }
                let path = format!("/debate/{}/inject", js_sys::encode_uri_component(&id));
                let body = serde_json::json!({"message": msg});
                match api.post::<_, DebateInjectResponse>(&path, &body).await {
                    Ok(_) => {
                        set_inject_text.set(String::new());
                        set_session_status.set("running".into());
                        set_status_msg.set("Question injected, debate resuming...".into());
                        set_polling.set(true);
                    }
                    Err(e) => set_status_msg.set(format!("Inject error: {e}")),
                }
            }
        }
    });

    // ── Continue (resume without inject) ──
    let api_continue = api.clone();
    let continue_action = Action::new_local(move |_: &()| {
        let api = api_continue.clone();
        let sid = session_id.get_untracked();
        async move {
            if let Some(id) = sid {
                let path = format!("/debate/{}/run", js_sys::encode_uri_component(&id));
                match api.post::<_, DebateRunResponse>(&path, &serde_json::json!({})).await {
                    Ok(_) => {
                        set_session_status.set("running".into());
                        set_status_msg.set("Debate continuing...".into());
                        set_polling.set(true);
                    }
                    Err(e) => set_status_msg.set(format!("Continue error: {e}")),
                }
            }
        }
    });

    // ── Synthesize ──
    let api_synth = api.clone();
    let synthesize_action = Action::new_local(move |_: &()| {
        let api = api_synth.clone();
        let sid = session_id.get_untracked();
        async move {
            if let Some(id) = sid {
                set_loading.set(true);
                set_status_msg.set("Synthesizing debate results...".into());
                let path = format!("/debate/{}/synthesize", js_sys::encode_uri_component(&id));
                match api.post::<_, DebateSynthesizeResponse>(&path, &serde_json::json!({})).await {
                    Ok(resp) => {
                        set_synthesis.set(Some(resp.synthesis));
                        set_session_status.set("complete".into());
                        set_status_msg.set("Synthesis complete.".into());
                        // Collapse all rounds, scroll to top
                        set_expanded_round.set(None);
                        if let Some(w) = web_sys::window() {
                            let _ = w.scroll_to_with_x_and_y(0.0, 0.0);
                        }
                    }
                    Err(e) => set_status_msg.set(format!("Synthesis error: {e}")),
                }
                set_loading.set(false);
            }
        }
    });

    // ── Save agent edit ──
    let api_edit = api.clone();
    let save_agent_edit = Action::new_local(move |_: &()| {
        let api = api_edit.clone();
        let sid = session_id.get_untracked();
        let agent_id = editing_agent.get_untracked();
        async move {
            if let (Some(id), Some(aid)) = (sid, agent_id) {
                let rigor: f32 = edit_rigor.get_untracked().parse().unwrap_or(50.0) / 100.0;
                let bias_lbl = edit_bias_label.get_untracked();
                let is_neutral = bias_lbl.to_lowercase() == "neutral" || bias_lbl.is_empty();
                let body = serde_json::json!({
                    "agents": [{
                        "id": aid,
                        "name": edit_name.get_untracked(),
                        "persona_description": edit_persona.get_untracked(),
                        "rigor_level": rigor,
                        "source_access": edit_source.get_untracked(),
                        "bias": {
                            "label": if is_neutral { "Neutral analyst" } else { &bias_lbl },
                            "description": edit_bias_desc.get_untracked(),
                            "is_neutral": is_neutral,
                        }
                    }]
                });
                let path = format!("/debate/{}/agents", js_sys::encode_uri_component(&id));
                match api.patch::<_, serde_json::Value>(&path, &body).await {
                    Ok(_) => {
                        // Refresh agents
                        let get_path = format!("/debate/{}", js_sys::encode_uri_component(&id));
                        if let Ok(resp) = api.get::<DebateSessionResponse>(&get_path).await {
                            set_agents.set(resp.agents);
                        }
                        set_editing_agent.set(None);
                        set_status_msg.set("Agent updated.".into());
                    }
                    Err(e) => set_status_msg.set(format!("Edit error: {e}")),
                }
            }
        }
    });

    view! {
        <div class="page-header">
            <h2><i class="fa-solid fa-comments"></i>" Analysis Panel"</h2>
            <div style="display: flex; gap: 0.5rem; align-items: center;">
                // Round indicator
                {move || {
                    let status = session_status.get();
                    let cr = current_round.get();
                    let mr = max_rounds.get();
                    match status.as_str() {
                        "running" => Some(view! {
                            <span class="badge badge-active" style="font-size: 0.8rem;">
                                <i class="fa-solid fa-spinner fa-spin"></i>
                                {format!(" Round {} of {}", cr + 1, mr)}
                            </span>
                        }.into_any()),
                        "awaiting_input" => Some(view! {
                            <span class="badge" style="font-size: 0.8rem;">
                                {format!("Round {} of {} complete", cr, mr)}
                            </span>
                        }.into_any()),
                        "all_rounds_complete" => Some(view! {
                            <span class="badge badge-warn" style="font-size: 0.8rem;">
                                {format!("All {} rounds done", mr)}
                            </span>
                        }.into_any()),
                        "complete" => Some(view! {
                            <span class="badge badge-active" style="font-size: 0.8rem;">
                                <i class="fa-solid fa-check"></i>" Complete"
                            </span>
                        }.into_any()),
                        _ => None,
                    }
                }}
                // New Debate + Export (always available when session exists)
                {move || {
                    session_id.get().map(|_| view! {
                        <button class="btn" style="font-size: 0.8rem;" on:click=move |_| { reset_all(); }>
                            <i class="fa-solid fa-plus"></i>" New"
                        </button>
                        {(session_status.get() == "complete").then(|| view! {
                            <button class="btn" style="font-size: 0.8rem;"
                                on:click=move |_| {
                                    export_debate(session_id.get_untracked(), rounds.get_untracked(), agents.get_untracked(), synthesis.get_untracked());
                                }
                            >
                                <i class="fa-solid fa-download"></i>" Export"
                            </button>
                        })}
                    })
                }}
            </div>
        </div>

        // Status message
        {move || {
            let msg = status_msg.get();
            if msg.is_empty() { None } else {
                Some(view! { <div class="alert" style="margin-bottom: 1rem;">{msg}</div> })
            }
        }}

        // ── Setup form (no session) ──
        {move || {
            if session_id.get().is_none() {
                Some(view! {
                    <div class="card" style="margin-bottom: 1.5rem;">
                        <h3>"Start Analysis"</h3>

                        // Mode selector
                        <div class="debate-modes">
                            {[
                                ("analyze", "fa-search", "Analyze", "#4a9eff",
                                 "What is happening? What's likely?",
                                 "Diverse analysts with different biases debate evidence to answer your question. Best for intelligence questions and situational assessment."),
                                ("red_team", "fa-crosshairs", "Red Team", "#e74c3c",
                                 "How to achieve X? What breaks it?",
                                 "Strategists propose plans while red team members attack them. Finds vulnerabilities, counter-strategies, and resource gaps."),
                                ("outcome_engineering", "fa-bullseye", "Outcome Engineering", "#e67e22",
                                 "What must be true for X to happen?",
                                 "Works backwards from a desired end state. Maps dependency chains, blocking factors, and highest-leverage intervention points."),
                                ("scenario_forecast", "fa-code-branch", "Scenario Forecast", "#9b59b6",
                                 "What are the plausible futures?",
                                 "Each agent builds one distinct scenario: best case, worst case, most likely, wild card. Identifies branching conditions and early warnings."),
                                ("stakeholder_simulation", "fa-users-cog", "Stakeholder Simulation", "#1abc9c",
                                 "How will real-world actors react?",
                                 "Each agent becomes a real-world player (country, org, leader) and argues from their actual interests. Predicts moves and reaction chains."),
                                ("premortem", "fa-skull-crossbones", "Pre-mortem", "#c0392b",
                                 "Assume plan X failed. Why?",
                                 "Pessimistic agents each find a different failure mode: technical, human, external, adversarial, slow-onset. Ranked by probability and severity."),
                                ("decision_matrix", "fa-th-list", "Decision Matrix", "#2ecc71",
                                 "Should we do A, B, or C?",
                                 "One advocate per option plus neutral evaluators. Scores on cost, risk, speed, impact, and feasibility. Produces ranked recommendation."),
                            ].into_iter().map(|(val, icon, label, color, question, desc)| {
                                let val_s = val.to_string();
                                let val_c = val.to_string();
                                view! {
                                    <div
                                        class=move || if debate_mode.get() == val_s { "debate-mode-card active" } else { "debate-mode-card" }
                                        style=format!("--mode-color: {}", color)
                                        on:click=move |_| { set_debate_mode.set(val_c.clone()); }
                                    >
                                        <div class="debate-mode-header">
                                            <span class="debate-mode-icon"><i class=format!("fa-solid {}", icon)></i></span>
                                            <span class="debate-mode-title">{label}</span>
                                        </div>
                                        <div class="debate-mode-question">{question}</div>
                                        <div class="debate-mode-desc">{desc}</div>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>

                        // Unified input area -- adapts labels and fields to the selected mode
                        {move || {
                            let m = debate_mode.get();
                            let (topic_label, topic_placeholder, extra_label, extra_placeholder) = match m.as_str() {
                                "red_team" => (
                                    "Topic / Context",
                                    "e.g., Our company's migration to microservices and its impact on reliability",
                                    Some("Desired Outcome"),
                                    Some("What should be achieved? e.g., Zero-downtime deployments within 6 months"),
                                ),
                                "outcome_engineering" => (
                                    "Topic / Context",
                                    "e.g., Reducing urban traffic congestion in a growing metropolitan area",
                                    Some("Desired End State"),
                                    Some("What end state should be reverse-engineered? e.g., 30% reduction in peak-hour commute times"),
                                ),
                                "stakeholder_simulation" => (
                                    "Scenario / Situation",
                                    "e.g., A major tech company announces open-sourcing its AI models -- how will key actors respond?",
                                    Some("Actors to Simulate"),
                                    Some("Comma-separated list: e.g., Competitors, Regulators, Open-source community, Investors"),
                                ),
                                "premortem" => (
                                    "Topic / Context",
                                    "e.g., Product launch strategy for a new SaaS platform",
                                    Some("Plan to Stress-Test"),
                                    Some("What plan has already 'failed'? e.g., The freemium launch flopped and user acquisition stalled at 1000"),
                                ),
                                "decision_matrix" => (
                                    "Decision Context",
                                    "e.g., How should our team handle the growing technical debt in the payment system?",
                                    Some("Options to Evaluate"),
                                    Some("Pipe-separated: e.g., Full rewrite | Incremental refactor | Buy a vendor solution"),
                                ),
                                "scenario_forecast" => (
                                    "Topic / Situation to Forecast",
                                    "e.g., How will remote work policies evolve in the tech industry over the next 5 years?",
                                    None, None,
                                ),
                                _ => (
                                    "Topic / Question",
                                    "e.g., What are the implications of quantum computing on current encryption standards?",
                                    None, None,
                                ),
                            };
                            view! {
                                <div style="display: flex; gap: 1rem; flex-wrap: wrap; align-items: end;">
                                    <div style="flex: 1; min-width: 300px;">
                                        <label>{topic_label}</label>
                                        <input type="text" placeholder=topic_placeholder
                                            prop:value=topic on:input=move |ev| set_topic.set(event_target_value(&ev))
                                            style="width: 100%;" />
                                    </div>
                                    <div style="width: 100px;">
                                        <label>"Agents"</label>
                                        <select prop:value=agent_count on:change=move |ev| set_agent_count.set(event_target_value(&ev))>
                                            <option value="2">"2"</option><option value="3">"3"</option>
                                            <option value="4">"4"</option><option value="5" selected>"5"</option>
                                            <option value="6">"6"</option><option value="7">"7"</option><option value="8">"8"</option>
                                        </select>
                                    </div>
                                    <div style="width: 100px;">
                                        <label>"Rounds"</label>
                                        <select prop:value=max_rounds_input on:change=move |ev| set_max_rounds_input.set(event_target_value(&ev))>
                                            <option value="1">"1"</option><option value="2">"2"</option>
                                            <option value="3" selected>"3"</option><option value="4">"4"</option><option value="5">"5"</option>
                                        </select>
                                    </div>
                                </div>
                                {extra_label.map(|lbl| view! {
                                    <div style="margin-top: 0.75rem;">
                                        <label>{lbl}</label>
                                        <textarea rows="2" placeholder=extra_placeholder.unwrap_or("")
                                            prop:value=mode_input on:input=move |ev| set_mode_input.set(event_target_value(&ev))
                                            style="width: 100%; resize: vertical;" />
                                    </div>
                                })}
                            }
                        }}

                        <div style="margin-top: 0.75rem;">
                            <button class="btn btn-primary" on:click=move |_| { start_debate.dispatch(()); }
                                disabled=move || loading.get()>
                                {move || if loading.get() { "Generating..." } else { "Generate Panel" }}
                            </button>
                        </div>
                    </div>
                })
            } else { None }
        }}

        // ── Sticky controls bar ──
        {move || {
            let status = session_status.get();
            match status.as_str() {
                "awaiting_start" => Some(view! {
                    <div class="card" style="position: sticky; top: 48px; z-index: 10; margin-bottom: 1rem; display: flex; gap: 0.5rem; align-items: center;">
                        <button class="btn btn-primary" on:click=move |_| { run_debate.dispatch(()); }
                            disabled=move || loading.get()>
                            <i class="fa-solid fa-play"></i>" Start Debate"
                        </button>
                        <span class="text-secondary" style="font-size: 0.85rem;">"Review agents below, click to edit. Then start."</span>
                    </div>
                }.into_any()),
                "running" | "researching" => Some(view! {
                    <div class="card" style="position: sticky; top: 48px; z-index: 10; margin-bottom: 1rem; display: flex; gap: 0.5rem; align-items: center;">
                        <span class="badge badge-active"><i class="fa-solid fa-spinner fa-spin"></i>
                            {move || {
                                let msg = progress_msg.get();
                                if msg.is_empty() { " Working...".to_string() } else { format!(" {}", msg) }
                            }}
                        </span>
                    </div>
                }.into_any()),
                "awaiting_input" => Some(view! {
                    <div class="card" style="position: sticky; top: 48px; z-index: 10; margin-bottom: 1rem; display: flex; gap: 0.5rem; align-items: center; flex-wrap: wrap;">
                        <input type="text" placeholder="Inject a question for the next round..."
                            prop:value=inject_text on:input=move |ev| set_inject_text.set(event_target_value(&ev))
                            style="flex: 1; min-width: 200px;" />
                        <button class="btn btn-primary" on:click=move |_| { inject_action.dispatch(()); }>
                            <i class="fa-solid fa-paper-plane"></i>" Inject"
                        </button>
                        <button class="btn" on:click=move |_| { continue_action.dispatch(()); }>
                            <i class="fa-solid fa-forward"></i>" Continue"
                        </button>
                        <button class="btn" on:click=move |_| { synthesize_action.dispatch(()); }
                            disabled=move || loading.get()>
                            <i class="fa-solid fa-flask"></i>" Synthesize"
                        </button>
                    </div>
                }.into_any()),
                "all_rounds_complete" => Some(view! {
                    <div class="card" style="position: sticky; top: 48px; z-index: 10; margin-bottom: 1rem; display: flex; gap: 0.5rem; align-items: center;">
                        <span style="flex: 1; font-weight: bold;">"All rounds complete."</span>
                        <button class="btn btn-primary" on:click=move |_| { synthesize_action.dispatch(()); }
                            disabled=move || loading.get()>
                            <i class="fa-solid fa-flask"></i>
                            {move || if loading.get() { " Synthesizing..." } else { " Generate Synthesis" }}
                        </button>
                    </div>
                }.into_any()),
                _ => None,
            }
        }}

        // ── SYNTHESIS ON TOP (when complete) ──
        {move || {
            let syn = synthesis.get();
            syn.map(|s| {
                let s2 = s.clone(); let s3 = s.clone(); let s4 = s.clone();
                view! {
                    <div class="card" style="margin-bottom: 1.5rem; border: 1px solid var(--primary);">
                        <h3><i class="fa-solid fa-flask"></i>" Synthesis"</h3>
                        <div style="display: flex; gap: 0.5rem; margin-bottom: 1rem; border-bottom: 1px solid var(--border); padding-bottom: 0.5rem;">
                            <button class=move || if synthesis_tab.get() == "evidence" { "btn btn-primary" } else { "btn" }
                                on:click=move |_| set_synthesis_tab.set("evidence".into())>
                                <i class="fa-solid fa-search"></i>" Evidence"
                            </button>
                            <button class=move || if synthesis_tab.get() == "influence" { "btn btn-primary" } else { "btn" }
                                on:click=move |_| set_synthesis_tab.set("influence".into())>
                                <i class="fa-solid fa-arrows-alt"></i>" Influence"
                            </button>
                            <button class=move || if synthesis_tab.get() == "agendas" { "btn btn-primary" } else { "btn" }
                                on:click=move |_| set_synthesis_tab.set("agendas".into())>
                                <i class="fa-solid fa-mask"></i>" Hidden Agendas"
                            </button>
                            <button class=move || if synthesis_tab.get() == "evolution" { "btn btn-primary" } else { "btn" }
                                on:click=move |_| set_synthesis_tab.set("evolution".into())>
                                <i class="fa-solid fa-chart-line"></i>" Evolution"
                            </button>
                        </div>
                        {move || match synthesis_tab.get().as_str() {
                            "evidence" => render_evidence_tab(s.clone()).into_any(),
                            "influence" => render_influence_tab(s2.clone()).into_any(),
                            "agendas" => render_agendas_tab(s3.clone()).into_any(),
                            "evolution" => render_evolution_tab(s4.clone()).into_any(),
                            _ => view! { <p>"Unknown tab"</p> }.into_any(),
                        }}
                        {move || {
                            let syn = synthesis.get();
                            syn.map(|ss| view! {
                                <div style="margin-top: 1rem; border-top: 1px solid var(--border); padding-top: 1rem;">
                                    {(!ss.key_tensions.is_empty()).then(|| view! {
                                        <h4><i class="fa-solid fa-bolt"></i>" Key Tensions"</h4>
                                        <ul>{ss.key_tensions.iter().map(|t| view! { <li>{t.clone()}</li> }).collect::<Vec<_>>()}</ul>
                                    })}
                                    {(!ss.recommended_investigations.is_empty()).then(|| view! {
                                        <h4><i class="fa-solid fa-magnifying-glass"></i>" Recommended Investigations"</h4>
                                        <ul>{ss.recommended_investigations.iter().map(|r| view! { <li>{r.clone()}</li> }).collect::<Vec<_>>()}</ul>
                                    })}
                                </div>
                            })
                        }}
                    </div>
                }
            })
        }}

        // ── Agent panel (collapsible) ──
        {move || {
            let current_agents = agents.get();
            if current_agents.is_empty() { return None; }
            let is_editable = session_status.get() == "awaiting_start";
            Some(view! {
                <div class="card" style="margin-bottom: 1rem;">
                    <h3 style="margin: 0;"><i class="fa-solid fa-users"></i>" Agent Panel"
                        {is_editable.then(|| view! { <span class="text-secondary" style="font-size: 0.8rem; font-weight: normal;">" (click agent to edit)"</span> })}
                    </h3>
                    <div style="display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 0.75rem; margin-top: 0.75rem;">
                        {current_agents.into_iter().map(|agent| {
                            let aid = agent.id.clone();
                            let color = agent.color.clone();
                            let icon = agent.icon.clone();
                            let sa = agent.source_access.clone();
                            let sa2 = sa.clone();
                            let editable = is_editable;
                            let agent_for_edit = agent.clone();
                            view! {
                                <div class="card" style=format!("border-left: 4px solid {}; padding: 0.6rem; cursor: {};", color, if editable { "pointer" } else { "default" })
                                    on:click=move |_| {
                                        if editable {
                                            set_editing_agent.set(Some(aid.clone()));
                                            set_edit_name.set(agent_for_edit.name.clone());
                                            set_edit_persona.set(agent_for_edit.persona_description.clone());
                                            set_edit_bias_label.set(if agent_for_edit.bias.is_neutral { "Neutral".into() } else { agent_for_edit.bias.label.clone() });
                                            set_edit_bias_desc.set(agent_for_edit.bias.description.clone());
                                            set_edit_rigor.set(format!("{:.0}", agent_for_edit.rigor_level * 100.0));
                                            set_edit_source.set(agent_for_edit.source_access.clone());
                                        }
                                    }
                                >
                                    <div style="display: flex; align-items: center; gap: 0.4rem; margin-bottom: 0.3rem;">
                                        <i class=format!("fa-solid {}", icon) style=format!("color: {};", color)></i>
                                        <strong style="font-size: 0.9rem;">{agent.name.clone()}</strong>
                                    </div>
                                    <p style="font-size: 0.8rem; margin: 0.2rem 0; color: var(--text-secondary); line-height: 1.3;">
                                        {agent.persona_description.clone()}
                                    </p>
                                    <div style="display: flex; flex-wrap: wrap; gap: 0.3rem; margin-top: 0.4rem;">
                                        <span class="badge" style="font-size: 0.65rem;"><i class="fa-solid fa-ruler"></i>{format!(" {:.0}%", agent.rigor_level * 100.0)}</span>
                                        <span class="badge" style="font-size: 0.65rem;"><i class=format!("fa-solid {}", source_badge(&sa))></i>{format!(" {}", source_label(&sa2))}</span>
                                        {if agent.bias.is_neutral {
                                            view! { <span class="badge badge-active" style="font-size: 0.65rem;"><i class="fa-solid fa-balance-scale"></i>" Neutral"</span> }.into_any()
                                        } else {
                                            view! { <span class="badge badge-warn" style="font-size: 0.65rem;"><i class="fa-solid fa-flag"></i>{format!(" {}", agent.bias.label)}</span> }.into_any()
                                        }}
                                    </div>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                </div>
            })
        }}

        // ── Agent edit modal ──
        {move || {
            editing_agent.get().map(|aid| view! {
                <div style="position: fixed; inset: 0; background: rgba(0,0,0,0.6); z-index: 100; display: flex; align-items: center; justify-content: center;"
                    on:click=move |_| set_editing_agent.set(None)>
                    <div class="card" style="width: 450px; max-width: 90vw;" on:click=move |ev| { ev.stop_propagation(); }>
                        <h3>{format!("Edit Agent: {}", aid)}</h3>
                        <div style="display: flex; flex-direction: column; gap: 0.75rem;">
                            <div>
                                <label>"Name"</label>
                                <input type="text" prop:value=edit_name on:input=move |ev| set_edit_name.set(event_target_value(&ev)) style="width: 100%;" />
                            </div>
                            <div>
                                <label>"Persona Description"</label>
                                <textarea rows="3" prop:value=edit_persona on:input=move |ev| set_edit_persona.set(event_target_value(&ev)) style="width: 100%; resize: vertical;" />
                            </div>
                            <div>
                                <label>"Rigor (%)"</label>
                                <input type="number" min="0" max="100" prop:value=edit_rigor on:input=move |ev| set_edit_rigor.set(event_target_value(&ev)) style="width: 100%;" />
                            </div>
                            <div>
                                <label>"Source Access"</label>
                                <select prop:value=edit_source on:change=move |ev| set_edit_source.set(event_target_value(&ev)) style="width: 100%;">
                                    <option value="graph_only">"Graph only"</option>
                                    <option value="graph_and_web">"Graph + Web"</option>
                                    <option value="graph_low_confidence">"Graph (low confidence)"</option>
                                    <option value="web_only">"Web only"</option>
                                </select>
                            </div>
                            <div>
                                <label>"Bias Label (\"Neutral\" for no bias)"</label>
                                <textarea rows="2" prop:value=edit_bias_label on:input=move |ev| set_edit_bias_label.set(event_target_value(&ev)) style="width: 100%; resize: vertical;" />
                            </div>
                            <div>
                                <label>"Bias Description"</label>
                                <textarea rows="3" prop:value=edit_bias_desc on:input=move |ev| set_edit_bias_desc.set(event_target_value(&ev)) style="width: 100%; resize: vertical;" />
                            </div>
                            <div style="display: flex; gap: 0.5rem; justify-content: flex-end;">
                                <button class="btn" on:click=move |_| set_editing_agent.set(None)>"Cancel"</button>
                                <button class="btn btn-primary" on:click=move |_| { save_agent_edit.dispatch(()); }>"Save"</button>
                            </div>
                        </div>
                    </div>
                </div>
            })
        }}

        // ── Debate timeline (collapsible rounds) ──
        {move || {
            let current_rounds = rounds.get();
            if current_rounds.is_empty() { return None; }
            let current_agents = agents.get();
            Some(view! {
                <div style="margin-bottom: 1.5rem;">
                    <h3><i class="fa-solid fa-timeline"></i>" Debate Timeline"</h3>
                    {current_rounds.into_iter().enumerate().map(|(idx, round)| {
                        let round_num = round.round_number + 1;
                        let turn_count = round.turns.len();
                        let gap_count = round.gap_research.len();
                        let injection = round.user_injection.clone();
                        let agents_for_round = current_agents.clone();
                        let is_expanded = move || expanded_round.get() == Some(idx);
                        view! {
                            <div class="card" style="margin-bottom: 0.5rem; padding: 0;">
                                // Round header (clickable to expand/collapse)
                                <div style="padding: 0.6rem 1rem; cursor: pointer; display: flex; justify-content: space-between; align-items: center;"
                                    on:click=move |_| {
                                        if expanded_round.get_untracked() == Some(idx) {
                                            set_expanded_round.set(None);
                                        } else {
                                            set_expanded_round.set(Some(idx));
                                        }
                                    }>
                                    <span>
                                        <i class=move || if is_expanded() { "fa-solid fa-chevron-down" } else { "fa-solid fa-chevron-right" }></i>
                                        <strong>{format!(" Round {}", round_num)}</strong>
                                        <span class="text-secondary" style="margin-left: 0.5rem; font-size: 0.85rem;">
                                            {format!("{} turns", turn_count)}
                                            {(gap_count > 0).then(|| format!(" + {} gaps researched", gap_count))}
                                        </span>
                                        {injection.as_ref().map(|inj| view! {
                                            <span class="badge" style="font-size: 0.65rem; margin-left: 0.5rem;">
                                                <i class="fa-solid fa-comment-dots"></i>{format!(" {}", truncate_str(inj, 40))}
                                            </span>
                                        })}
                                    </span>
                                </div>
                                // Round body (expanded)
                                {move || {
                                    if !is_expanded() { return None; }
                                    let inj2 = round.user_injection.clone();
                                    Some(view! {
                                        <div style="padding: 0 1rem 0.75rem;">
                                            {inj2.map(|inj| view! {
                                                <div class="alert" style="margin: 0.5rem 0; font-style: italic;">
                                                    <i class="fa-solid fa-comment-dots"></i>
                                                    {format!(" Moderator: {}", inj)}
                                                </div>
                                            })}
                                            {round.turns.iter().map(|turn| {
                                                let agent = agents_for_round.iter().find(|a| a.id == turn.agent_id).cloned();
                                                render_turn(turn.clone(), agent)
                                            }).collect::<Vec<_>>()}
                                            // Gap research results
                                            {(!round.gap_research.is_empty()).then(|| {
                                                let gaps = round.gap_research.clone();
                                                view! {
                                                    <div style="margin-top: 0.75rem; padding: 0.6rem; background: rgba(46,204,113,0.08); border-radius: 4px; border-left: 3px solid var(--success);">
                                                        <strong style="font-size: 0.85rem;"><i class="fa-solid fa-magnifying-glass-plus"></i>" Gap-Closing Research"</strong>
                                                        {gaps.iter().map(|gr| {
                                                            let ingested = gr.ingested;
                                                            view! {
                                                                <div style="margin-top: 0.4rem; font-size: 0.8rem;">
                                                                    <div><i class="fa-solid fa-search"></i>{format!(" {}", gr.gap_query)}</div>
                                                                    {gr.findings.iter().map(|f| {
                                                                        view! { <div style="margin-left: 1rem; color: var(--text-secondary);">{format!("- {}", f)}</div> }
                                                                    }).collect::<Vec<_>>()}
                                                                    {ingested.then(|| view! {
                                                                        <div style="margin-left: 1rem; color: var(--success); font-size: 0.75rem;">
                                                                            <i class="fa-solid fa-database"></i>" Added to knowledge graph"
                                                                        </div>
                                                                    })}
                                                                </div>
                                                            }
                                                        }).collect::<Vec<_>>()}
                                                    </div>
                                                }
                                            })}
                                        </div>
                                    })
                                }}
                            </div>
                        }
                    }).collect::<Vec<_>>()}
                </div>
            })
        }}
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() } else { format!("{}...", &s[..max]) }
}

fn source_badge(sa: &str) -> &'static str {
    match sa {
        "graph_only" => "fa-database",
        "graph_and_web" => "fa-globe",
        "graph_low_confidence" => "fa-question-circle",
        "web_only" => "fa-globe-americas",
        _ => "fa-circle",
    }
}

fn source_label(sa: &str) -> &'static str {
    match sa {
        "graph_only" => "Graph only",
        "graph_and_web" => "Graph + Web",
        "graph_low_confidence" => "Low confidence",
        "web_only" => "Web only",
        _ => "Other",
    }
}

fn render_turn(turn: DebateTurn, agent: Option<DebateAgent>) -> impl IntoView {
    let color = agent.as_ref().map(|a| a.color.clone()).unwrap_or_else(|| "#666".into());
    let icon = agent.as_ref().map(|a| a.icon.clone()).unwrap_or_else(|| "fa-user".into());
    let name = agent.as_ref().map(|a| a.name.clone()).unwrap_or_else(|| turn.agent_id.clone());
    let bias_label = agent.as_ref().map(|a| {
        if a.bias.is_neutral { "Neutral".to_string() } else { a.bias.label.clone() }
    }).unwrap_or_default();

    view! {
        <div style=format!("border-left: 3px solid {}; padding: 0.6rem; margin: 0.4rem 0; background: var(--bg-secondary); border-radius: 4px;", color)>
            <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.4rem;">
                <span>
                    <i class=format!("fa-solid {}", icon) style=format!("color: {};", color)></i>
                    <strong>{format!(" {}", name)}</strong>
                    <span class="badge" style="font-size: 0.6rem; margin-left: 0.4rem;">{bias_label}</span>
                </span>
                <span class="badge" style="font-size: 0.7rem;">{format!("{:.0}%", turn.confidence * 100.0)}</span>
            </div>
            // Position shift (evolution)
            {(!turn.position_shift.is_empty()).then(|| view! {
                <div style="margin-bottom: 0.4rem; font-size: 0.8rem; padding: 0.25rem 0.5rem; background: rgba(52,152,219,0.1); border-radius: 3px; border-left: 2px solid var(--info);">
                    <i class="fa-solid fa-arrows-rotate" style="color: var(--info);"></i>
                    {format!(" {}", turn.position_shift)}
                </div>
            })}
            {(!turn.concessions.is_empty()).then(|| view! {
                <div style="margin-bottom: 0.4rem; font-size: 0.8rem; color: var(--success);">
                    <i class="fa-solid fa-hand-holding-heart"></i>
                    {format!(" Concedes: {}", turn.concessions.join("; "))}
                </div>
            })}
            <div style="white-space: pre-wrap; font-size: 0.85rem; line-height: 1.4;">{turn.position.clone()}</div>
            <div style="margin-top: 0.4rem; font-size: 0.75rem; color: var(--text-secondary); display: flex; gap: 1rem; flex-wrap: wrap;">
                {(!turn.evidence.is_empty()).then(|| format!("{} evidence", turn.evidence.len()))}
                {(!turn.tools_used.is_empty()).then(|| format!("{} tools", turn.tools_used.len()))}
                {(!turn.agrees_with.is_empty()).then(|| view! {
                    <span style="color: var(--success);"><i class="fa-solid fa-handshake"></i>{format!(" {}", turn.agrees_with.join(", "))}</span>
                })}
                {(!turn.disagrees_with.is_empty()).then(|| view! {
                    <span style="color: var(--danger);"><i class="fa-solid fa-xmark"></i>{format!(" {}", turn.disagrees_with.join(", "))}</span>
                })}
            </div>
        </div>
    }
}

fn render_evidence_tab(s: crate::api::types::DebateSynthesis) -> impl IntoView {
    view! {
        <div>
            <h4><i class="fa-solid fa-check-circle"></i>" Evidence-Based Conclusion"</h4>
            <div style="background: var(--bg-secondary); padding: 1rem; border-radius: 4px; margin-bottom: 1rem;">
                <div style="white-space: pre-wrap;">{s.evidence_conclusion.clone()}</div>
                <div style="margin-top: 0.5rem;">
                    <span class="badge badge-active" style="font-size: 0.8rem;">{format!("Confidence: {:.0}%", s.conclusion_confidence * 100.0)}</span>
                </div>
            </div>
            {(!s.evidence_gaps.is_empty()).then(|| view! {
                <h4><i class="fa-solid fa-exclamation-triangle"></i>" Evidence Gaps"</h4>
                <ul>{s.evidence_gaps.iter().map(|g| view! { <li>{g.clone()}</li> }).collect::<Vec<_>>()}</ul>
            })}
        </div>
    }
}

fn render_influence_tab(s: crate::api::types::DebateSynthesis) -> impl IntoView {
    view! {
        <div>
            {(!s.influence_map.is_empty()).then(|| view! {
                <h4><i class="fa-solid fa-arrows-alt"></i>" Influence Map"</h4>
                {s.influence_map.iter().map(|item| {
                    let name = item.get("agent_name").and_then(|n| n.as_str()).unwrap_or("?");
                    let bias = item.get("bias_label").and_then(|b| b.as_str()).unwrap_or("");
                    let position = item.get("position_pushed").and_then(|p| p.as_str()).unwrap_or("");
                    let distortion = item.get("distortion_summary").and_then(|d| d.as_str()).unwrap_or("");
                    let backed = item.get("evidence_backed").and_then(|b| b.as_bool()).unwrap_or(false);
                    view! {
                        <div class="card" style="padding: 0.6rem; margin-bottom: 0.4rem;">
                            <strong>{name.to_string()}</strong>
                            <span class="badge badge-warn" style="font-size: 0.6rem; margin-left: 0.4rem;">{bias.to_string()}</span>
                            {backed.then(|| view! { <span class="badge badge-active" style="font-size: 0.6rem; margin-left: 0.25rem;">"Evidence-backed"</span> })}
                            <p style="margin: 0.2rem 0; font-size: 0.85rem;">{format!("Position: {}", position)}</p>
                            {(!distortion.is_empty()).then(|| view! {
                                <p style="font-size: 0.8rem; color: var(--text-secondary); font-style: italic;">{format!("Distortion: {}", distortion)}</p>
                            })}
                        </div>
                    }
                }).collect::<Vec<_>>()}
            })}
            {(!s.cherry_picks.is_empty()).then(|| view! {
                <h4><i class="fa-solid fa-filter"></i>" Cherry-Picking"</h4>
                {s.cherry_picks.iter().map(|cp| {
                    let ignored = cp.get("ignored_evidence").and_then(|i| i.as_str()).unwrap_or("");
                    let why = cp.get("why_ignored").and_then(|w| w.as_str()).unwrap_or("");
                    view! { <div style="padding: 0.4rem; font-size: 0.85rem;"><i class="fa-solid fa-eye-slash" style="color: var(--danger);"></i>{format!(" {} -- {}", ignored, why)}</div> }
                }).collect::<Vec<_>>()}
            })}
        </div>
    }
}

fn render_agendas_tab(s: crate::api::types::DebateSynthesis) -> impl IntoView {
    view! {
        <div>
            {(!s.hidden_agendas.is_empty()).then(|| view! {
                <h4><i class="fa-solid fa-mask"></i>" Cui Bono?"</h4>
                {s.hidden_agendas.iter().map(|ha| {
                    let name = ha.get("agent_name").and_then(|n| n.as_str()).unwrap_or("?");
                    let stated = ha.get("stated_position").and_then(|s| s.as_str()).unwrap_or("");
                    let underlying = ha.get("underlying_interest").and_then(|u| u.as_str()).unwrap_or("");
                    let who = ha.get("who_benefits").and_then(|w| w.as_str()).unwrap_or("");
                    let avoid = ha.get("what_they_avoid").and_then(|a| a.as_str()).unwrap_or("");
                    let lose = ha.get("what_they_lose").and_then(|l| l.as_str()).unwrap_or("");
                    view! {
                        <div class="card" style="padding: 0.6rem; margin-bottom: 0.5rem; border-left: 3px solid var(--danger);">
                            <strong>{name.to_string()}</strong>
                            <div style="font-size: 0.85rem; margin-top: 0.2rem;">
                                <p><i class="fa-solid fa-comment"></i>{format!(" Says: {}", stated)}</p>
                                <p><i class="fa-solid fa-eye" style="color: var(--danger);"></i>{format!(" Really wants: {}", underlying)}</p>
                                <p><i class="fa-solid fa-trophy"></i>{format!(" Benefits: {}", who)}</p>
                                {(!avoid.is_empty()).then(|| view! { <p><i class="fa-solid fa-eye-slash"></i>{format!(" Avoids: {}", avoid)}</p> })}
                                {(!lose.is_empty()).then(|| view! { <p><i class="fa-solid fa-arrow-down" style="color: var(--danger);"></i>{format!(" Stands to lose: {}", lose)}</p> })}
                            </div>
                        </div>
                    }
                }).collect::<Vec<_>>()}
            })}
            {(!s.blind_spots.is_empty()).then(|| view! {
                <h4><i class="fa-solid fa-low-vision"></i>" Blind Spots"</h4>
                {s.blind_spots.iter().map(|bs| {
                    let topic = bs.get("topic_avoided").and_then(|t| t.as_str()).unwrap_or("");
                    let reason = bs.get("likely_reason").and_then(|r| r.as_str()).unwrap_or("");
                    view! { <div style="padding: 0.4rem; font-size: 0.85rem;"><i class="fa-solid fa-exclamation-circle" style="color: var(--warning);"></i>{format!(" {}: {}", topic, reason)}</div> }
                }).collect::<Vec<_>>()}
            })}
        </div>
    }
}

fn render_evolution_tab(s: crate::api::types::DebateSynthesis) -> impl IntoView {
    view! {
        <div>
            {if s.evolution.is_empty() {
                view! { <p class="text-secondary">"No evolution data (need multiple rounds)."</p> }.into_any()
            } else {
                view! {
                    <div>
                        <h4><i class="fa-solid fa-chart-line"></i>" Position Evolution"</h4>
                        {s.evolution.iter().map(|ev| {
                            let name = ev.get("agent_name").and_then(|n| n.as_str()).unwrap_or("?");
                            let trajectory: Vec<String> = ev.get("confidence_trajectory")
                                .and_then(|t| t.as_array())
                                .map(|arr| arr.iter().map(|v| format!("{:.0}%", v.as_f64().unwrap_or(0.0) * 100.0)).collect())
                                .unwrap_or_default();
                            let net_shift = ev.get("net_shift").and_then(|n| n.as_f64()).unwrap_or(0.0);
                            let pivot = ev.get("pivot_cause").and_then(|p| p.as_str()).unwrap_or("");
                            let flexibility = ev.get("flexibility_score").and_then(|f| f.as_f64()).unwrap_or(0.0);
                            let bias_override = ev.get("bias_override").and_then(|b| b.as_bool()).unwrap_or(false);
                            let concessions: Vec<String> = ev.get("key_concessions")
                                .and_then(|c| c.as_array())
                                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                                .unwrap_or_default();
                            let shift_color = if net_shift.abs() < 0.05 { "var(--text-secondary)" }
                                else if net_shift > 0.0 { "var(--success)" } else { "var(--danger)" };
                            let flex_label = if flexibility > 0.3 { "Adapted" }
                                else if flexibility < -0.3 { "Dug in" } else { "Steady" };
                            view! {
                                <div class="card" style="padding: 0.6rem; margin-bottom: 0.5rem;">
                                    <div style="display: flex; justify-content: space-between; align-items: center;">
                                        <strong>{name.to_string()}</strong>
                                        <div style="display: flex; gap: 0.4rem;">
                                            <span class="badge" style="font-size: 0.65rem;">{format!("{} ({:+.0}%)", flex_label, flexibility * 100.0)}</span>
                                            {bias_override.then(|| view! { <span class="badge badge-warn" style="font-size: 0.6rem;"><i class="fa-solid fa-shield-alt"></i>" Bias override"</span> })}
                                        </div>
                                    </div>
                                    <div style="margin-top: 0.4rem; font-size: 0.85rem;">
                                        <i class="fa-solid fa-chart-line"></i>{format!(" {}", trajectory.join(" -> "))}
                                        <span style=format!("margin-left: 0.5rem; font-weight: bold; color: {};", shift_color)>{format!("(net: {:+.0}%)", net_shift * 100.0)}</span>
                                    </div>
                                    {(!pivot.is_empty()).then(|| view! {
                                        <div style="margin-top: 0.2rem; font-size: 0.85rem;"><i class="fa-solid fa-lightbulb" style="color: var(--warning);"></i>{format!(" Pivot: {}", pivot)}</div>
                                    })}
                                    {(!concessions.is_empty()).then(|| view! {
                                        <div style="margin-top: 0.2rem; font-size: 0.85rem;"><i class="fa-solid fa-hand-holding-heart" style="color: var(--success);"></i>{format!(" Conceded: {}", concessions.join("; "))}</div>
                                    })}
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            }}
        </div>
    }
}

fn export_debate(
    session_id: Option<String>,
    rounds: Vec<DebateRound>,
    agents: Vec<DebateAgent>,
    synthesis: Option<crate::api::types::DebateSynthesis>,
) {
    use wasm_bindgen::JsCast;
    let mut text = String::from("=== ENGRAM DEBATE EXPORT ===\n\n");
    if let Some(ref sid) = session_id { text.push_str(&format!("Session: {}\n\n", sid)); }
    text.push_str("--- AGENTS ---\n");
    for a in &agents {
        text.push_str(&format!("  {} | Rigor: {:.0}% | Source: {} | Bias: {}\n    {}\n\n",
            a.name, a.rigor_level * 100.0, a.source_access,
            if a.bias.is_neutral { "Neutral".to_string() } else { a.bias.label.clone() }, a.persona_description));
    }
    for round in &rounds {
        text.push_str(&format!("\n--- ROUND {} ---\n", round.round_number + 1));
        if let Some(ref inj) = round.user_injection { text.push_str(&format!("Moderator: {}\n\n", inj)); }
        for turn in &round.turns {
            let name = agents.iter().find(|a| a.id == turn.agent_id).map(|a| a.name.as_str()).unwrap_or(&turn.agent_id);
            text.push_str(&format!("[{}] (confidence: {:.0}%)\n{}\n", name, turn.confidence * 100.0, turn.position));
            if !turn.position_shift.is_empty() { text.push_str(&format!("  SHIFT: {}\n", turn.position_shift)); }
            if !turn.concessions.is_empty() { text.push_str(&format!("  CONCEDES: {}\n", turn.concessions.join("; "))); }
            text.push('\n');
        }
    }
    if let Some(ref syn) = synthesis {
        text.push_str(&format!("\n--- SYNTHESIS (confidence: {:.0}%) ---\n\n{}\n", syn.conclusion_confidence * 100.0, syn.evidence_conclusion));
        if !syn.evidence_gaps.is_empty() { text.push_str("\nEvidence Gaps:\n"); for g in &syn.evidence_gaps { text.push_str(&format!("  - {}\n", g)); } }
        if !syn.key_tensions.is_empty() { text.push_str("\nKey Tensions:\n"); for t in &syn.key_tensions { text.push_str(&format!("  - {}\n", t)); } }
        if !syn.recommended_investigations.is_empty() { text.push_str("\nInvestigations:\n"); for r in &syn.recommended_investigations { text.push_str(&format!("  - {}\n", r)); } }
    }
    if let Some(window) = web_sys::window() {
        if let Some(doc) = window.document() {
            if let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(
                &js_sys::Array::of1(&wasm_bindgen::JsValue::from_str(&text)),
                web_sys::BlobPropertyBag::new().type_("text/plain"),
            ) {
                if let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) {
                    if let Ok(a) = doc.create_element("a") {
                        let _ = a.set_attribute("href", &url);
                        let _ = a.set_attribute("download", &format!("debate-{}.txt", session_id.as_deref().unwrap_or("export")));
                        let _ = a.set_attribute("style", "display:none");
                        if let Some(body) = doc.body() {
                            let _ = body.append_child(&a);
                            let el: web_sys::HtmlElement = a.unchecked_into();
                            el.click();
                            let el2: web_sys::Element = el.unchecked_into();
                            let _ = body.remove_child(&el2);
                        }
                        let _ = web_sys::Url::revoke_object_url(&url);
                    }
                }
            }
        }
    }
}
