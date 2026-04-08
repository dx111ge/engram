/// Multi-agent debate panel page.
/// UX: setup -> review/edit agents -> War Room (3-panel) -> synthesis on top.

mod war_room;
mod agent_card;
mod activity_feed;
mod evidence_board;
mod controls;

use leptos::prelude::*;

use crate::api::ApiClient;
use crate::api::types::{
    DebateAgent, DebateStartResponse, DebateSessionResponse,
    DebateRunResponse, DebateInjectResponse, DebateSynthesizeResponse,
    DebateRound, DebateTurn,
};
use crate::app::ActiveDebate;

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
    let (briefing, set_briefing) = signal(Option::<crate::api::types::DebateBriefing>::None);

    // UI state
    let (inject_text, set_inject_text) = signal(String::new());
    let (polling, set_polling) = signal(false);
    let (progress_msg, set_progress_msg) = signal(String::new());
    let (editing_agent, set_editing_agent) = signal(Option::<String>::None);

    // War Room state -- lifted here so it survives status-change remounts
    let (agent_states, set_agent_states) = signal(std::collections::HashMap::<String, war_room::AgentLiveState>::new());
    let (feed_entries, set_feed_entries) = signal(Vec::<war_room::FeedEntry>::new());
    let (evidence_items, set_evidence_items) = signal(Vec::<war_room::EvidenceItem>::new());
    let (feed_counter, set_feed_counter) = signal(0u64);
    let (sse_connected, set_sse_connected) = signal(false);
    let (center_panel, set_center_panel) = signal(war_room::CenterPanel::ActivityFeed);

    // Edit form signals
    let (edit_name, set_edit_name) = signal(String::new());
    let (edit_persona, set_edit_persona) = signal(String::new());
    let (edit_bias_label, set_edit_bias_label) = signal(String::new());
    let (edit_bias_desc, set_edit_bias_desc) = signal(String::new());
    let (edit_rigor, set_edit_rigor) = signal(String::new());
    let (edit_source, set_edit_source) = signal(String::new());

    // ── Sync with global ActiveDebate context ──
    let global_debate = use_context::<ActiveDebate>();
    Effect::new(move |_| {
        if let Some(ref gd) = global_debate {
            let sid = session_id.get();
            let st = session_status.get();
            let t = topic.get();
            let m = debate_mode.get();
            let cr = current_round.get();
            let mr = max_rounds.get();
            gd.session_id.set(sid.clone());
            gd.status.set(st);
            gd.topic.set(t);
            gd.mode.set(m);
            gd.current_round.set(cr);
            gd.max_rounds.set(mr);
            // Persist to localStorage for session recovery
            if let Some(w) = web_sys::window() {
                if let Ok(Some(storage)) = w.local_storage() {
                    if let Some(ref id) = sid {
                        let _ = storage.set_item("engram_debate_session", id);
                    } else {
                        let _ = storage.remove_item("engram_debate_session");
                    }
                }
            }
        }
    });

    // ── Session recovery: restore active debate on mount ──
    {
        let api_recover = api.clone();
        Effect::new(move |prev: Option<bool>| {
            // Only run once on mount
            if prev.is_some() { return false; }
            if session_id.get_untracked().is_some() { return false; }
            // Check localStorage for a saved session
            let saved_sid = web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
                .and_then(|s| s.get_item("engram_debate_session").ok().flatten());
            if let Some(sid) = saved_sid {
                let api = api_recover.clone();
                leptos::task::spawn_local(async move {
                    let path = format!("/debate/{}", js_sys::encode_uri_component(&sid));
                    if let Ok(resp) = api.get::<DebateSessionResponse>(&path).await {
                        // Only restore if session is still active (not complete/error)
                        if !matches!(resp.status.as_str(), "complete" | "error" | "") {
                            set_session_id.set(Some(sid));
                            set_session_status.set(resp.status.clone());
                            set_agents.set(resp.agents);
                            set_rounds.set(resp.rounds);
                            set_current_round.set(resp.current_round);
                            set_max_rounds.set(resp.max_rounds);
                            if let Some(b) = resp.briefing {
                                set_briefing.set(Some(b));
                            }
                            if let Some(syn) = resp.synthesis {
                                set_synthesis.set(Some(syn));
                            }
                            if !resp.topic.is_empty() {
                                set_topic.set(resp.topic);
                            }
                            if !resp.mode.is_empty() {
                                set_debate_mode.set(resp.mode);
                            }
                            if let Some(p) = &resp.progress {
                                set_progress_msg.set(p.message.clone());
                            }
                            // Resume polling if still running
                            if matches!(resp.status.as_str(), "running" | "researching" | "gap_closing" | "synthesizing") {
                                set_polling.set(true);
                            }
                            // Set center panel based on restored status
                            match resp.status.as_str() {
                                "awaiting_input" | "all_rounds_complete" => {
                                    set_center_panel.set(war_room::CenterPanel::RoundSummary);
                                }
                                "complete" => {
                                    set_center_panel.set(war_room::CenterPanel::SynthesisResult);
                                }
                                _ => {}
                            }
                            set_status_msg.set(match resp.status.as_str() {
                                "panel_ready" => "Session restored. Review agents and start.".into(),
                                "awaiting_input" => "Session restored. Continue, inject, or synthesize.".into(),
                                "all_rounds_complete" => "Session restored. Click Synthesize.".into(),
                                "complete" => "Session restored. Synthesis complete.".into(),
                                _ => "Session restored. Debate in progress...".into(),
                            });
                        } else {
                            // Session is done, clear localStorage
                            if let Some(w) = web_sys::window() {
                                if let Ok(Some(s)) = w.local_storage() {
                                    let _ = s.remove_item("engram_debate_session");
                                }
                            }
                        }
                    }
                });
            }
            false
        });
    }

    // ── Reset for new debate ──
    let reset_all = move || {
        // Clear localStorage session
        if let Some(w) = web_sys::window() {
            if let Ok(Some(s)) = w.local_storage() {
                let _ = s.remove_item("engram_debate_session");
            }
        }
        set_session_id.set(None);
        set_session_status.set(String::new());
        set_agents.set(Vec::new());
        set_rounds.set(Vec::new());
        set_synthesis.set(None);
        set_briefing.set(None);
        set_status_msg.set(String::new());
        set_topic.set(String::new());
        set_debate_mode.set("analyze".into());
        set_mode_input.set(String::new());
        set_current_round.set(0);
        set_center_panel.set(war_room::CenterPanel::ActivityFeed);
        // Reset War Room state
        set_agent_states.set(std::collections::HashMap::new());
        set_feed_entries.set(Vec::new());
        set_evidence_items.set(Vec::new());
        set_feed_counter.set(0);
        set_sse_connected.set(false);
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
                        if resp.status != session_status.get_untracked() {
                            set_session_status.set(resp.status.clone());
                        }
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
                        set_rounds.set(resp.rounds);
                        if let Some(b) = resp.briefing {
                            set_briefing.set(Some(b));
                        }
                        if let Some(syn) = resp.synthesis {
                            set_synthesis.set(Some(syn));
                        }
                        match resp.status.as_str() {
                            "awaiting_input" => {
                                let r = current_round.get_untracked();
                                let m = max_rounds.get_untracked();
                                set_status_msg.set(format!("Round {} of {} complete. Continue, inject a question, or synthesize.", r, m));
                                set_center_panel.set(war_room::CenterPanel::RoundSummary);
                                set_feed_entries.set(Vec::new());
                                set_polling.set(false);
                                break;
                            }
                            "all_rounds_complete" => {
                                let m = max_rounds.get_untracked();
                                set_status_msg.set(format!("All {} rounds complete. Click Synthesize to generate the analysis.", m));
                                set_center_panel.set(war_room::CenterPanel::RoundSummary);
                                set_feed_entries.set(Vec::new());
                                set_polling.set(false);
                                break;
                            }
                            "complete" => {
                                set_status_msg.set("Synthesis complete.".into());
                                set_center_panel.set(war_room::CenterPanel::SynthesisResult);
                                set_feed_entries.set(Vec::new());
                                set_polling.set(false);
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
                        // Reset center panel to activity feed for the next round
                        set_feed_entries.set(Vec::new());
                        set_center_panel.set(war_room::CenterPanel::ActivityFeed);
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
                        // Reset center panel to activity feed for the next round
                        set_feed_entries.set(Vec::new());
                        set_center_panel.set(war_room::CenterPanel::ActivityFeed);
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
                        // Switch to synthesis result in center panel
                        set_center_panel.set(war_room::CenterPanel::SynthesisResult);
                        set_feed_entries.set(Vec::new());
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
                                {format!("Round {} of {} complete", cr.max(1), mr)}
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
                                {move || if loading.get() {
                                    view! { <><i class="fa-solid fa-spinner fa-spin"></i>" Generating..."</> }.into_any()
                                } else {
                                    view! { <><i class="fa-solid fa-wand-magic-sparkles"></i>" Generate Panel"</> }.into_any()
                                }}
                            </button>
                        </div>
                    </div>
                })
            } else { None }
        }}

        // ── Start button (only awaiting_start) ──
        {move || {
            (session_status.get() == "awaiting_start").then(|| view! {
                <div class="card" style="position: sticky; top: 48px; z-index: 10; margin-bottom: 1rem; display: flex; gap: 0.5rem; align-items: center;">
                    <button class="btn btn-primary" on:click=move |_| { run_debate.dispatch(()); }
                        disabled=move || loading.get()>
                        <i class="fa-solid fa-play"></i>" Start Debate"
                    </button>
                    <span class="text-secondary" style="font-size: 0.85rem;">"Review agents below, click to edit. Then start."</span>
                </div>
            })
        }}

        // Synthesis is now shown inside the War Room center panel (SynthesisResultView)

        // ── Briefing (collapsible, shown when briefing data available) ──
        {move || {
            briefing.get().map(|b| {
                let fact_count = b.facts.len();
                let q_count = b.questions.len();
                let _summary = b.summary.clone();
                view! {
                    <details style="margin-bottom: 0.5rem;">
                        <summary class="card" style="cursor: pointer; padding: 0.5rem 0.75rem; list-style: none; display: flex; align-items: center; gap: 0.5rem;">
                            <i class="fa-solid fa-book-open" style="color: var(--accent-bright);"></i>
                            <strong style="font-size: 0.85rem;">"Briefing"</strong>
                            <span class="badge" style="font-size: 0.7rem;">{format!("{} questions", q_count + 1)}</span>
                            <span class="badge badge-active" style="font-size: 0.7rem;">{format!("{} facts", fact_count)}</span>
                            {if b.facts_stored > 0 || b.relations_created > 0 {
                                view! {
                                    <span class="badge badge-active" style="font-size: 0.7rem;">
                                        {format!("{} stored, {} relations", b.facts_stored, b.relations_created)}
                                    </span>
                                }.into_any()
                            } else if fact_count > 0 {
                                view! {
                                    <span class="badge" style="font-size: 0.7rem; background: var(--bg-tertiary); color: var(--text-secondary);">
                                        "graph only"
                                    </span>
                                }.into_any()
                            } else {
                                view! {
                                    <span class="badge" style="font-size: 0.7rem; opacity: 0.5;">
                                        "no data"
                                    </span>
                                }.into_any()
                            }}
                        </summary>
                        <div class="card" style="margin-top: 0; border-top: none; border-radius: 0 0 8px 8px; padding: 0.75rem; font-size: 0.85rem;">
                            // Structured facts grouped by question
                            {b.facts.iter().fold(Vec::<(String, Vec<crate::api::types::BriefingFact>)>::new(), |mut acc, f| {
                                if let Some(group) = acc.iter_mut().find(|(q, _)| q == &f.question) {
                                    group.1.push(f.clone());
                                } else {
                                    acc.push((f.question.clone(), vec![f.clone()]));
                                }
                                acc
                            }).iter().map(|(question, facts)| {
                                let q = question.clone();
                                let fs = facts.clone();
                                view! {
                                    <details style="margin-bottom: 0.5rem;">
                                        <summary style="cursor: pointer; font-weight: 600; font-size: 0.8rem; padding: 0.3rem 0; color: var(--text-primary);">
                                            <i class="fa-solid fa-magnifying-glass" style="margin-right: 0.3rem; opacity: 0.5; font-size: 0.7rem;"></i>
                                            {q}
                                            <span class="badge" style="font-size: 0.6rem; margin-left: 0.3rem;">{format!("{}", fs.len())}</span>
                                        </summary>
                                        <ul style="margin: 0.2rem 0 0; padding-left: 1.2rem; font-size: 0.78rem;">
                                            {fs.iter().take(5).map(|f| {
                                                let source_icon = match f.source.as_str() {
                                                    "graph" => "fa-solid fa-database",
                                                    "web" => "fa-solid fa-globe",
                                                    "sparql" => "fa-solid fa-link",
                                                    "wikipedia" => "fa-brands fa-wikipedia-w",
                                                    _ => "fa-solid fa-circle-info",
                                                };
                                                let conf_pct = (f.confidence * 100.0) as u32;
                                                view! {
                                                    <li style="margin-bottom: 0.15rem; line-height: 1.4;">
                                                        <i class=source_icon style="font-size: 0.65rem; opacity: 0.6; margin-right: 0.2rem;"></i>
                                                        {f.content.clone()}
                                                        <span class="text-muted" style="font-size: 0.65rem;">{format!(" ({}%)", conf_pct)}</span>
                                                    </li>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </ul>
                                    </details>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    </details>
                }
            })
        }}

        // ── Topic + mode bar (shown as soon as panel exists) ──
        {move || {
            let status = session_status.get();
            let show_topic = matches!(status.as_str(), "panel_ready" | "running" | "researching" | "awaiting_input" | "all_rounds_complete" | "synthesizing");
            show_topic.then(|| {
                let t = topic.get();
                let m = debate_mode.get();
                let mode_label = match m.as_str() {
                    "analyze" => "Analyze", "red_team" => "Red Team", "outcome_engineering" => "Outcome Engineering",
                    "scenario_forecast" => "Scenario Forecast", "stakeholder_simulation" => "Stakeholder Simulation",
                    "pre_mortem" => "Pre-Mortem", "decision_matrix" => "Decision Matrix",
                    _ => &m,
                };
                view! {
                    <div style="display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.5rem; padding: 0.4rem 0.6rem; background: var(--bg-card); border: 1px solid var(--border); border-radius: 6px; font-size: 0.85rem;">
                        <span class="badge badge-active" style="font-size: 0.7rem;">{mode_label.to_string()}</span>
                        <strong style="flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{t}</strong>
                    </div>
                }
            })
        }}

        // ── War Room (3-panel layout when debate is active) ──
        {move || {
            let status = session_status.get();
            let is_war_room = matches!(status.as_str(), "running" | "researching" | "awaiting_input" | "all_rounds_complete" | "synthesizing" | "complete");
            is_war_room.then(|| {
                view! {
                <war_room::WarRoom
                    session_id=session_id
                    session_status=session_status
                    agents=agents
                    rounds=rounds
                    current_round=current_round
                    max_rounds=max_rounds
                    synthesis=synthesis
                    set_session_status=set_session_status
                    set_rounds=set_rounds
                    set_synthesis=set_synthesis
                    set_status_msg=set_status_msg
                    inject_text=inject_text
                    set_inject_text=set_inject_text
                    progress_msg=progress_msg
                    set_progress_msg=set_progress_msg
                    inject_action=inject_action
                    continue_action=continue_action
                    synthesize_action=synthesize_action
                    loading=loading
                    // Lifted state -- survives remounts
                    agent_states=agent_states
                    set_agent_states=set_agent_states
                    feed_entries=feed_entries
                    set_feed_entries=set_feed_entries
                    evidence_items=evidence_items
                    set_evidence_items=set_evidence_items
                    feed_counter=feed_counter
                    set_feed_counter=set_feed_counter
                    sse_connected=sse_connected
                    set_sse_connected=set_sse_connected
                    center_panel=center_panel
                    set_center_panel=set_center_panel
                />
            }}) // view! + then
        }}

        // ── Agent panel (only shown when awaiting_start for editing) ──
        {move || {
            let current_agents = agents.get();
            if current_agents.is_empty() { return None; }
            let is_editable = session_status.get() == "awaiting_start";
            if !is_editable { return None; }
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
                                    <option value="Comprehensive">"Deep research (graph + web)"</option>
                                    <option value="BriefingFocused">"Briefing-focused"</option>
                                    <option value="Contrarian">"Contrarian research"</option>
                                    <option value="WeakSignals">"Weak signals & low-confidence"</option>
                                    <option value="GraphOnly">"Graph only"</option>
                                    <option value="GraphAndWeb">"Graph + Web search"</option>
                                    <option value="WebOnly">"Web only"</option>
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

        // Timeline view removed -- all debate content now lives in the War Room center panel.
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

pub(super) fn render_evidence_tab(s: crate::api::types::DebateSynthesis) -> impl IntoView {
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
            {(!s.key_tensions.is_empty()).then(|| view! {
                <h4><i class="fa-solid fa-bolt"></i>" Key Tensions"</h4>
                <ul>{s.key_tensions.iter().map(|t| view! { <li>{t.clone()}</li> }).collect::<Vec<_>>()}</ul>
            })}
            {(!s.recommended_investigations.is_empty()).then(|| view! {
                <h4><i class="fa-solid fa-magnifying-glass"></i>" Recommended Investigations"</h4>
                <ul>{s.recommended_investigations.iter().map(|r| view! { <li>{r.clone()}</li> }).collect::<Vec<_>>()}</ul>
            })}
        </div>
    }
}

pub(super) fn render_influence_tab(s: crate::api::types::DebateSynthesis) -> impl IntoView {
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

pub(super) fn render_agendas_tab(s: crate::api::types::DebateSynthesis) -> impl IntoView {
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

pub(super) fn render_evolution_tab(s: crate::api::types::DebateSynthesis) -> impl IntoView {
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

pub(super) fn export_debate(
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
