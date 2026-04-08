/// War Room: 3-panel debate layout with live agent cards, activity feed, and evidence board.

use leptos::prelude::*;
use std::collections::HashMap;
use wasm_bindgen::JsCast;

use crate::api::types::{DebateAgent, DebateRound, DebateTurn, DebateSynthesis};
use super::agent_card::AgentCard;
use super::activity_feed::ActivityFeed;
use super::evidence_board::EvidenceBoard;
use super::controls::ControlsBar;

/// Center panel mode -- determines what the middle column shows.
#[derive(Clone, Debug, PartialEq)]
pub enum CenterPanel {
    /// Live rolling SSE events during a round.
    ActivityFeed,
    /// Round summary: each agent's position as collapsible cards.
    RoundSummary,
    /// Synthesis result + export button.
    SynthesisResult,
}

/// Live state for a single agent, updated by SSE events.
#[derive(Clone, Debug, Default)]
pub struct AgentLiveState {
    pub confidence_history: Vec<f32>,
    pub evidence_count: u32,
    pub supported_claims: u32,
    pub contradicted_claims: u32,
    pub agrees_with: Vec<String>,
    pub disagrees_with: Vec<String>,
    pub latest_position: String,
    pub is_active: bool,
}

/// A single entry in the activity feed.
#[derive(Clone, Debug)]
pub struct FeedEntry {
    pub id: u64,
    pub timestamp: String,
    pub event_type: String,
    pub agent_id: Option<String>,
    pub agent_color: Option<String>,
    pub icon: String,
    pub summary: String,
    pub detail: Option<String>,
}

/// An evidence item found during the debate.
#[derive(Clone, Debug)]
pub struct EvidenceItem {
    pub entity: String,
    pub confidence: f32,
    pub source_type: String,
    pub agent_id: Option<String>,
    pub agent_color: Option<String>,
}

#[component]
pub fn WarRoom(
    session_id: ReadSignal<Option<String>>,
    session_status: ReadSignal<String>,
    agents: ReadSignal<Vec<DebateAgent>>,
    rounds: ReadSignal<Vec<DebateRound>>,
    current_round: ReadSignal<usize>,
    max_rounds: ReadSignal<usize>,
    synthesis: ReadSignal<Option<DebateSynthesis>>,
    set_session_status: WriteSignal<String>,
    set_rounds: WriteSignal<Vec<DebateRound>>,
    set_synthesis: WriteSignal<Option<DebateSynthesis>>,
    #[allow(unused)] set_status_msg: WriteSignal<String>,
    inject_text: ReadSignal<String>,
    set_inject_text: WriteSignal<String>,
    progress_msg: ReadSignal<String>,
    set_progress_msg: WriteSignal<String>,
    // Actions from parent
    inject_action: Action<(), ()>,
    continue_action: Action<(), ()>,
    synthesize_action: Action<(), ()>,
    loading: ReadSignal<bool>,
    // Lifted state -- owned by parent, survives remounts
    agent_states: ReadSignal<HashMap<String, AgentLiveState>>,
    set_agent_states: WriteSignal<HashMap<String, AgentLiveState>>,
    feed_entries: ReadSignal<Vec<FeedEntry>>,
    set_feed_entries: WriteSignal<Vec<FeedEntry>>,
    evidence_items: ReadSignal<Vec<EvidenceItem>>,
    set_evidence_items: WriteSignal<Vec<EvidenceItem>>,
    feed_counter: ReadSignal<u64>,
    set_feed_counter: WriteSignal<u64>,
    sse_connected: ReadSignal<bool>,
    set_sse_connected: WriteSignal<bool>,
    center_panel: ReadSignal<CenterPanel>,
    set_center_panel: WriteSignal<CenterPanel>,
) -> impl IntoView {

    // Update agent states from rounds data (initial load + polling fallback)
    // Only rebuild from rounds when agent_states is empty (initial load) or
    // when rounds have more data than current SSE-driven state (polling caught up)
    Effect::new(move |_| {
        let ags = agents.get();
        let rds = rounds.get();
        let current = agent_states.get_untracked();
        // Check if SSE has already populated state with live data
        let has_live_data = current.values().any(|s| !s.confidence_history.is_empty() || s.evidence_count > 0);
        // Count total rounds data available
        let rounds_data_count: usize = rds.iter().map(|r| r.turns.len()).sum();
        // Skip rebuild if SSE state has data AND rounds haven't added new completed rounds
        let current_max_rounds: usize = current.values().map(|s| s.confidence_history.len()).max().unwrap_or(0);
        if has_live_data && rds.len() <= current_max_rounds {
            return;
        }
        // Rebuild from rounds (either initial load or new completed round from poll)
        if rounds_data_count > 0 || !has_live_data {
            let mut states = HashMap::new();
            for agent in &ags {
                let mut state = AgentLiveState::default();
                for round in &rds {
                    if let Some(turn) = round.turns.iter().find(|t| t.agent_id == agent.id) {
                        state.confidence_history.push(turn.confidence);
                        state.evidence_count += turn.evidence.len() as u32;
                        state.agrees_with = turn.agrees_with.clone();
                        state.disagrees_with = turn.disagrees_with.clone();
                        state.latest_position = turn.position.clone();
                    }
                }
                states.insert(agent.id.clone(), state);
            }
            set_agent_states.set(states);
        }
    });

    // SSE connection for live events -- connect ONCE when session_id is available
    {
        let api = use_context::<crate::api::ApiClient>().expect("ApiClient");
        let base_url = api.base_url.clone();

        Effect::new(move |_| {
            let sid = match session_id.get() {
                Some(id) => id,
                None => return,
            };
            // Don't reconnect if already connected
            if sse_connected.get_untracked() { return; }
            set_sse_connected.set(true);

            let encoded = js_sys::encode_uri_component(&sid).as_string().unwrap_or(sid.clone());
            // EventSource can't send Authorization headers, so pass token as query param
            let token_param = crate::api::ApiClient::auth_token()
                .map(|t| format!("?token={}", js_sys::encode_uri_component(&t)))
                .unwrap_or_default();
            let url = format!("{}/debate/{}/stream{}", base_url, encoded, token_param);
            let source = match web_sys::EventSource::new(&url) {
                Ok(s) => s,
                Err(_) => return,
            };

            let ags = agents.get_untracked();
            let agent_colors: HashMap<String, String> = ags.iter()
                .map(|a| (a.id.clone(), a.color.clone()))
                .collect();
            let agent_names: HashMap<String, String> = ags.iter()
                .map(|a| (a.id.clone(), a.name.clone()))
                .collect();

            // Helper: create a feed entry and push it
            let add_entry = move |icon: &str, summary: String, detail: Option<String>,
                                  event_type: &str, aid: Option<String>, color: Option<String>| {
                let now = js_sys::Date::new_0();
                let ts = format!("{:02}:{:02}:{:02}", now.get_hours(), now.get_minutes(), now.get_seconds());
                let id = feed_counter.get_untracked();
                set_feed_counter.set(id + 1);
                set_feed_entries.update(|v| {
                    v.insert(0, FeedEntry {
                        id, timestamp: ts, event_type: event_type.to_string(),
                        agent_id: aid, agent_color: color,
                        icon: icon.to_string(), summary, detail,
                    });
                    if v.len() > 100 { v.truncate(100); }
                });
            };

            // Single handler that dispatches all event types
            let handler = wasm_bindgen::closure::Closure::wrap(Box::new(move |evt: web_sys::MessageEvent| {
                let event_type = evt.type_();
                let data_str = match evt.data().as_string() {
                    Some(s) => s,
                    None => return,
                };
                let v: serde_json::Value = serde_json::from_str(&data_str).unwrap_or_default();

                let aid = v.get("agent_id").and_then(|v| v.as_str()).map(String::from);
                let color = aid.as_ref().and_then(|a| agent_colors.get(a).cloned());

                match event_type.as_str() {
                    "turn_start" => {
                        let name = aid.as_ref().and_then(|a| agent_names.get(a)).map(|s| s.as_str()).unwrap_or("Agent");
                        if let Some(ref a) = aid {
                            let a = a.clone();
                            set_agent_states.update(|states| {
                                for (id, s) in states.iter_mut() { s.is_active = id == &a; }
                            });
                        }
                        add_entry("fa-solid fa-comment", format!("{} is analyzing...", name), None,
                                  &event_type, aid, color);
                    }
                    "turn_complete" => {
                        let name = aid.as_ref().and_then(|a| agent_names.get(a)).map(|s| s.as_str()).unwrap_or("Agent");
                        let conf = v.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let ev_count = v.get("evidence_count").and_then(|v| v.as_u64()).unwrap_or(0);
                        // Position goes to the FEED as detail -- readable in the center panel
                        let position = v.get("position").and_then(|v| v.as_str()).map(String::from);

                        // Update agent state
                        if let Some(ref a) = aid {
                            let a = a.clone();
                            set_agent_states.update(|states| {
                                if let Some(s) = states.get_mut(&a) {
                                    s.confidence_history.push(conf as f32);
                                    s.evidence_count += ev_count as u32;
                                    s.is_active = false;
                                    if let Some(ref p) = position { s.latest_position = p.clone(); }
                                    if let Some(ag) = v.get("agrees_with").and_then(|v| v.as_array()) {
                                        s.agrees_with = ag.iter().filter_map(|v| v.as_str().map(String::from)).collect();
                                    }
                                    if let Some(dg) = v.get("disagrees_with").and_then(|v| v.as_array()) {
                                        s.disagrees_with = dg.iter().filter_map(|v| v.as_str().map(String::from)).collect();
                                    }
                                }
                            });
                        }

                        // Add evidence items from the turn
                        if let Some(evidence) = v.get("evidence").and_then(|v| v.as_array()) {
                            for ev in evidence {
                                let entity = ev.get("entity").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let conf_ev = ev.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                                let src = ev.get("source_type").and_then(|v| v.as_str()).unwrap_or("graph").to_string();
                                if !entity.is_empty() {
                                    set_evidence_items.update(|items| {
                                        items.push(EvidenceItem {
                                            entity, confidence: conf_ev, source_type: src,
                                            agent_id: aid.clone(), agent_color: color.clone(),
                                        });
                                        if items.len() > 200 { items.truncate(200); }
                                    });
                                }
                            }
                        }

                        add_entry("fa-solid fa-check",
                                  format!("{}: {:.0}% confidence, {} evidence", name, conf * 100.0, ev_count),
                                  position, &event_type, aid, color);
                    }
                    "briefing_progress" => {
                        // Only show result events (with facts_found), skip "searching" phase to avoid duplicates
                        let facts = v.get("facts_found").and_then(|v| v.as_u64());
                        if let Some(f) = facts {
                            let q = v.get("question").and_then(|v| v.as_str()).unwrap_or("");
                            let idx = v.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                            let total = v.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
                            let q_short = q.char_indices().nth(80).map(|(i,_)| format!("{}...", &q[..i])).unwrap_or_else(|| q.to_string());
                            let msg = format!("Briefing {}/{}: {} facts -- {}", idx, total, f, q_short);
                            let detail = if q.len() > 80 { Some(q.to_string()) } else { None };
                            add_entry("fa-solid fa-book-open", msg, detail, &event_type, aid, color);
                        }
                        // "searching" phase events update progress bar only
                    }
                    "gap_progress" => {
                        let gap = v.get("gap").and_then(|v| v.as_str()).unwrap_or("");
                        let st = v.get("status").and_then(|v| v.as_str()).unwrap_or("");
                        let icon = match st { "searching" => "fa-solid fa-magnifying-glass", "timeout" => "fa-solid fa-clock", _ => "fa-solid fa-check-circle" };
                        let gap_short = gap.char_indices().nth(60).map(|(i,_)| &gap[..i]).unwrap_or(gap);
                        add_entry(icon, format!("Gap: {} [{}]", gap_short, st), Some(gap.to_string()),
                                  &event_type, aid, color);
                    }
                    "moderator_check" => {
                        let claim = v.get("claim").and_then(|v| v.as_str()).unwrap_or("");
                        let verdict = v.get("verdict").and_then(|v| v.as_str()).unwrap_or("");
                        let icon = match verdict { "Supported" => "fa-solid fa-circle-check", "Contradicted" => "fa-solid fa-circle-xmark", _ => "fa-solid fa-circle-question" };
                        if let Some(ref a) = aid {
                            let a = a.clone();
                            let v2 = verdict.to_string();
                            set_agent_states.update(|states| {
                                if let Some(s) = states.get_mut(&a) {
                                    match v2.as_str() { "Supported" => s.supported_claims += 1, "Contradicted" => s.contradicted_claims += 1, _ => {} }
                                }
                            });
                        }
                        let claim_short = claim.char_indices().nth(80).map(|(i,_)| &claim[..i]).unwrap_or(claim);
                        add_entry(icon, format!("Fact-check: {} [{}]", claim_short, verdict), Some(claim.to_string()),
                                  &event_type, aid, color);
                    }
                    "tool_call" => {
                        let tool = v.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");
                        let name = aid.as_ref().and_then(|a| agent_names.get(a)).map(|s| s.as_str()).unwrap_or("Agent");
                        let icon = match tool {
                            "engram_search" => "fa-solid fa-database",
                            "web_search" => "fa-solid fa-globe",
                            _ => "fa-solid fa-wrench",
                        };
                        add_entry(icon, format!("{}: {}", name, tool.replace('_', " ")), None,
                                  &event_type, aid, color);
                    }
                    "tool_result" => {
                        let summary_text = v.get("summary").and_then(|v| v.as_str()).unwrap_or("");
                        let name = aid.as_ref().and_then(|a| agent_names.get(a)).map(|s| s.as_str()).unwrap_or("Agent");
                        add_entry("fa-solid fa-clipboard-check", format!("{}: {}", name, summary_text), None,
                                  &event_type, aid, color);
                    }
                    "synthesis_step" => {
                        let layer = v.get("layer").and_then(|v| v.as_str()).unwrap_or("");
                        let step = v.get("step").and_then(|v| v.as_u64()).unwrap_or(0);
                        let total = v.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
                        add_entry("fa-solid fa-flask", format!("Synthesis {}/{}: {}", step, total, layer), None,
                                  &event_type, aid, color);
                    }
                    "round_start" => {
                        let round = v.get("round").and_then(|v| v.as_u64()).unwrap_or(0);
                        // New round: clear feed and switch to activity feed
                        set_feed_entries.set(Vec::new());
                        set_center_panel.set(CenterPanel::ActivityFeed);
                        add_entry("fa-solid fa-flag", format!("Round {} started", round), None,
                                  &event_type, aid, color);
                    }
                    "round_complete" => {
                        let round = v.get("round").and_then(|v| v.as_u64()).unwrap_or(0);
                        add_entry("fa-solid fa-flag-checkered", format!("Round {} complete", round), None,
                                  &event_type, aid, color);
                    }
                    "gap_research_start" => {
                        add_entry("fa-solid fa-search", "Gap detection started...".into(), None,
                                  &event_type, aid, color);
                    }
                    "gap_research_progress" => {
                        let msg = v.get("message").and_then(|v| v.as_str()).unwrap_or("Researching gaps...");
                        add_entry("fa-solid fa-search-plus", msg.to_string(), None,
                                  &event_type, aid, color);
                    }
                    "gap_research_complete" => {
                        let gaps = v.get("gaps_researched").and_then(|v| v.as_u64()).unwrap_or(0);
                        let ingested = v.get("ingested_count").and_then(|v| v.as_u64()).unwrap_or(0);
                        add_entry("fa-solid fa-check-double",
                                  format!("Gap research done: {}/{} gaps resolved", ingested, gaps), None,
                                  &event_type, aid, color);
                    }
                    "awaiting_input" => {
                        set_session_status.set("awaiting_input".into());
                        // Switch to round summary -- clear activity feed, show positions
                        set_feed_entries.set(Vec::new());
                        set_center_panel.set(CenterPanel::RoundSummary);
                    }
                    "all_rounds_complete" => {
                        set_session_status.set("all_rounds_complete".into());
                        // Switch to round summary for the final round
                        set_feed_entries.set(Vec::new());
                        set_center_panel.set(CenterPanel::RoundSummary);
                    }
                    "status_change" => {
                        if let Some(st) = v.get("status").and_then(|v| v.as_str()) {
                            set_session_status.set(st.to_lowercase().replace('"', ""));
                        }
                        // Status changes are reflected in the header, no feed entry needed
                    }
                    "synthesis_complete" => {
                        if let Ok(syn) = serde_json::from_str::<DebateSynthesis>(&data_str) {
                            set_synthesis.set(Some(syn));
                            set_session_status.set("complete".into());
                        }
                        // Switch to synthesis result view
                        set_feed_entries.set(Vec::new());
                        set_center_panel.set(CenterPanel::SynthesisResult);
                    }
                    "progress" => {
                        let msg = v.get("message").and_then(|v| v.as_str()).unwrap_or("");
                        let cur = v.get("current").and_then(|v| v.as_u64()).unwrap_or(0);
                        let total = v.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
                        if total > 0 {
                            set_progress_msg.set(format!("{} ({}/{})", msg, cur, total));
                        } else {
                            set_progress_msg.set(msg.to_string());
                        }
                        // Progress updates go to the progress bar, not the feed
                    }
                    "error" => {
                        let err = v.get("error").and_then(|v| v.as_str()).unwrap_or("Unknown error");
                        let name = aid.as_ref().and_then(|a| agent_names.get(a)).map(|s| s.as_str()).unwrap_or("System");
                        add_entry("fa-solid fa-triangle-exclamation",
                                  format!("{}: {}", name, err), None, &event_type, aid, color);
                    }
                    _ => {} // Unknown events silently ignored
                };
            }) as Box<dyn FnMut(web_sys::MessageEvent)>);

            // Register for all known event types
            for event in &["turn_start", "turn_complete", "briefing_progress", "gap_progress",
                          "moderator_check", "synthesis_step", "round_start", "round_complete",
                          "gap_research_start", "gap_research_progress", "gap_research_complete",
                          "awaiting_input", "all_rounds_complete",
                          "status_change", "synthesis_complete", "progress", "state",
                          "tool_call", "tool_result", "error"] {
                let _ = source.add_event_listener_with_callback(event, handler.as_ref().unchecked_ref());
            }
            handler.forget();

            // Close SSE after 30 min (long debates)
            let source_clone = source.clone();
            let timeout = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
                source_clone.close();
            }) as Box<dyn FnMut()>);
            let _ = web_sys::window().unwrap().set_timeout_with_callback_and_timeout_and_arguments_0(
                timeout.as_ref().unchecked_ref(), 1_800_000,
            );
            timeout.forget();
        });
    }

    // Polling fallback: fetch full session state every 15s, only when SSE is NOT connected
    {
        let api = use_context::<crate::api::ApiClient>().expect("ApiClient");
        Effect::new(move |_| {
            let sid = match session_id.get() {
                Some(id) => id,
                None => return,
            };
            let status = session_status.get();
            if status == "complete" || status.is_empty() { return; }

            let api = api.clone();
            leptos::task::spawn_local(async move {
                loop {
                    let st = session_status.get_untracked();
                    if st == "complete" || st.is_empty() { break; }
                    // Skip poll when SSE is actively connected -- avoid cascading re-renders
                    if sse_connected.get_untracked() {
                        gloo_timers::future::TimeoutFuture::new(15_000).await;
                        continue;
                    }
                    let s = match session_id.get_untracked() {
                        Some(id) => id,
                        None => break,
                    };
                    let path = format!("/debate/{}", js_sys::encode_uri_component(&s));
                    if let Ok(resp) = api.get::<crate::api::types::DebateSessionResponse>(&path).await {
                        set_rounds.set(resp.rounds);
                        if let Some(p) = &resp.progress {
                            let msg = if p.total > 0 {
                                format!("{} ({}/{})", p.message, p.current, p.total)
                            } else {
                                p.message.clone()
                            };
                            set_progress_msg.set(msg);
                        }
                        let new_status = resp.status.clone();
                        if new_status != session_status.get_untracked() {
                            set_session_status.set(new_status.clone());
                        }
                        if let Some(syn) = resp.synthesis {
                            set_synthesis.set(Some(syn));
                        }
                    }
                    gloo_timers::future::TimeoutFuture::new(15_000).await;
                }
            });
        });
    }

    view! {
        <div class="war-room">
            // Left: Agent cards
            <div class="war-room-agents">
                {move || {
                    let ags = agents.get();
                    let states = agent_states.get();
                    ags.iter().map(|agent| {
                        let st = states.get(&agent.id).cloned().unwrap_or_default();
                        view! {
                            <AgentCard agent=agent.clone() state=st />
                        }
                    }).collect::<Vec<_>>()
                }}
            </div>

            // Center: switches between activity feed, round summary, and synthesis
            <div class="war-room-feed">
                {move || {
                    match center_panel.get() {
                        CenterPanel::ActivityFeed => {
                            view! { <ActivityFeed entries=feed_entries progress_msg=progress_msg /> }.into_any()
                        }
                        CenterPanel::RoundSummary => {
                            let rds = rounds.get();
                            let ags = agents.get();
                            let latest_round = rds.last().cloned();
                            let cr = rds.len();
                            view! {
                                <RoundSummaryView round=latest_round agents=ags round_num=cr />
                            }.into_any()
                        }
                        CenterPanel::SynthesisResult => {
                            let syn = synthesis.get();
                            view! {
                                <SynthesisResultView
                                    synthesis=syn
                                    session_id=session_id
                                    rounds=rounds
                                    agents=agents
                                />
                            }.into_any()
                        }
                    }
                }}
            </div>

            // Right: Evidence board
            <div class="war-room-evidence">
                <EvidenceBoard items=evidence_items />
            </div>

            // Bottom: Controls
            <div class="war-room-controls">
                <ControlsBar
                    session_status=session_status
                    agents=agents
                    inject_text=inject_text
                    set_inject_text=set_inject_text
                    inject_action=inject_action
                    continue_action=continue_action
                    synthesize_action=synthesize_action
                    loading=loading
                    progress_msg=progress_msg
                    current_round=current_round
                    max_rounds=max_rounds
                />
            </div>
        </div>
    }
}

/// Round summary: collapsible position cards for each agent after a round completes.
#[component]
fn RoundSummaryView(
    round: Option<DebateRound>,
    agents: Vec<DebateAgent>,
    round_num: usize,
) -> impl IntoView {
    let round = match round {
        Some(r) => r,
        None => return view! { <div class="text-muted" style="padding: 1rem; text-align: center;">"No round data yet."</div> }.into_any(),
    };

    let turns = round.turns.clone();
    let gap_research = round.gap_research.clone();

    view! {
        <div>
            <h4 style="margin: 0 0 0.75rem; font-size: 0.9rem; color: var(--text-secondary);">
                <i class="fa-solid fa-flag-checkered" style="margin-right: 0.3rem;"></i>
                {format!("Round {} Summary", round_num)}
                <span class="badge" style="font-size: 0.7rem; margin-left: 0.5rem;">
                    {format!("{} positions", turns.len())}
                </span>
            </h4>

            // Agent positions as collapsible cards
            {turns.iter().map(|turn| {
                let agent = agents.iter().find(|a| a.id == turn.agent_id).cloned();
                render_position_card(turn.clone(), agent)
            }).collect::<Vec<_>>()}

            // Gap research results (if any)
            {(!gap_research.is_empty()).then(|| {
                let gaps = gap_research.clone();
                view! {
                    <details style="margin-top: 0.75rem;">
                        <summary style="cursor: pointer; font-size: 0.85rem; font-weight: 600; padding: 0.4rem 0.6rem; background: rgba(46,204,113,0.08); border-radius: 4px; border-left: 3px solid var(--success); list-style: none; display: flex; align-items: center; gap: 0.5rem;">
                            <i class="fa-solid fa-magnifying-glass-plus"></i>
                            {format!(" Gap-Closing Research ({} gaps)", gaps.len())}
                        </summary>
                        <div style="padding: 0.5rem 0.5rem 0 0.75rem;">
                            {gaps.iter().map(|gr| {
                                let ingested = gr.ingested;
                                view! {
                                    <div style="margin-bottom: 0.5rem; font-size: 0.8rem;">
                                        <div><i class="fa-solid fa-search" style="opacity: 0.6;"></i>{format!(" {}", gr.gap_query)}</div>
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
                    </details>
                }
            })}
        </div>
    }.into_any()
}

/// A single agent position card -- collapsed by default, click to expand.
fn render_position_card(turn: DebateTurn, agent: Option<DebateAgent>) -> impl IntoView {
    let color = agent.as_ref().map(|a| a.color.clone()).unwrap_or_else(|| "#666".into());
    let icon = agent.as_ref().map(|a| a.icon.clone()).unwrap_or_else(|| "fa-user".into());
    let name = agent.as_ref().map(|a| a.name.clone()).unwrap_or_else(|| turn.agent_id.clone());
    let bias_label = agent.as_ref().map(|a| {
        if a.bias.is_neutral { "Neutral".to_string() } else { a.bias.label.clone() }
    }).unwrap_or_default();
    let conf_pct = (turn.confidence * 100.0) as u32;
    let ev_count = turn.evidence.len();
    let color_inner = color.clone();

    let (expanded, set_expanded) = signal(false);

    view! {
        <div style={format!("border-left: 3px solid {}; margin-bottom: 0.5rem; background: var(--bg-secondary); border-radius: 4px; overflow: hidden;", color)}>
            // Header -- always visible, click to toggle
            <div style="padding: 0.5rem 0.6rem; cursor: pointer; display: flex; justify-content: space-between; align-items: center;"
                 on:click=move |_| set_expanded.update(|v| *v = !*v)>
                <span style="display: flex; align-items: center; gap: 0.4rem;">
                    <i class={format!("fa-solid {}", icon)} style={format!("color: {};", color_inner)}></i>
                    <strong style="font-size: 0.85rem;">{name.clone()}</strong>
                    <span class="badge" style="font-size: 0.6rem;">{bias_label}</span>
                </span>
                <span style="display: flex; align-items: center; gap: 0.5rem;">
                    <span class="badge" style="font-size: 0.7rem;">{format!("{}%", conf_pct)}</span>
                    <span class="text-muted" style="font-size: 0.7rem;">{format!("{} evidence", ev_count)}</span>
                    <i class=move || if expanded.get() { "fa-solid fa-chevron-up" } else { "fa-solid fa-chevron-down" }
                       style="font-size: 0.6rem; opacity: 0.5;"></i>
                </span>
            </div>
            // Body -- only when expanded
            {move || expanded.get().then(|| {
                view! {
                    <div style="padding: 0 0.6rem 0.6rem;">
                        // Position shift
                        {(!turn.position_shift.is_empty()).then(|| view! {
                            <div style="margin-bottom: 0.4rem; font-size: 0.8rem; padding: 0.25rem 0.5rem; background: rgba(52,152,219,0.1); border-radius: 3px; border-left: 2px solid var(--info);">
                                <i class="fa-solid fa-arrows-rotate" style="color: var(--info);"></i>
                                {format!(" {}", turn.position_shift)}
                            </div>
                        })}
                        // Concessions
                        {(!turn.concessions.is_empty()).then(|| view! {
                            <div style="margin-bottom: 0.4rem; font-size: 0.8rem; color: var(--success);">
                                <i class="fa-solid fa-hand-holding-heart"></i>
                                {format!(" Concedes: {}", turn.concessions.join("; "))}
                            </div>
                        })}
                        // Position text
                        <div style="white-space: pre-wrap; font-size: 0.83rem; line-height: 1.5; max-height: 400px; overflow-y: auto;">
                            {turn.position.clone()}
                        </div>
                    </div>
                }
            })}
        </div>
    }
}

/// Synthesis result view for the center panel.
#[component]
fn SynthesisResultView(
    synthesis: Option<DebateSynthesis>,
    session_id: ReadSignal<Option<String>>,
    rounds: ReadSignal<Vec<DebateRound>>,
    agents: ReadSignal<Vec<DebateAgent>>,
) -> impl IntoView {
    let syn = match synthesis {
        Some(s) => s,
        None => return view! {
            <div style="padding: 2rem; text-align: center;">
                <i class="fa-solid fa-spinner fa-spin" style="font-size: 1.5rem; color: var(--accent-bright);"></i>
                <p class="text-muted" style="margin-top: 0.5rem;">"Generating synthesis..."</p>
            </div>
        }.into_any(),
    };

    let (tab, set_tab) = signal("evidence".to_string());
    let s1 = syn.clone();
    let s2 = syn.clone();
    let s3 = syn.clone();
    let s4 = syn.clone();

    let has_influence = !syn.influence_map.is_empty() || !syn.cherry_picks.is_empty();
    let has_agendas = !syn.hidden_agendas.is_empty() || !syn.blind_spots.is_empty();
    let has_evolution = !syn.evolution.is_empty();

    view! {
        <div>
            <h4 style="margin: 0 0 0.75rem; font-size: 0.9rem;">
                <i class="fa-solid fa-flask" style="margin-right: 0.3rem; color: var(--accent-bright);"></i>
                "Synthesis"
                <span class="badge badge-active" style="font-size: 0.7rem; margin-left: 0.5rem;">
                    {format!("Confidence: {:.0}%", syn.conclusion_confidence * 100.0)}
                </span>
            </h4>

            // Tab buttons -- only show tabs that have data
            <div style="display: flex; gap: 0.3rem; margin-bottom: 0.75rem; border-bottom: 1px solid var(--border); padding-bottom: 0.5rem; flex-wrap: wrap;">
                <button class=move || if tab.get() == "evidence" { "btn btn-sm btn-primary" } else { "btn btn-sm" }
                    on:click=move |_| set_tab.set("evidence".into())>
                    <i class="fa-solid fa-search"></i>" Evidence"
                </button>
                {has_influence.then(|| view! {
                    <button class=move || if tab.get() == "influence" { "btn btn-sm btn-primary" } else { "btn btn-sm" }
                        on:click=move |_| set_tab.set("influence".into())>
                        <i class="fa-solid fa-arrows-alt"></i>" Influence"
                    </button>
                })}
                {has_agendas.then(|| view! {
                    <button class=move || if tab.get() == "agendas" { "btn btn-sm btn-primary" } else { "btn btn-sm" }
                        on:click=move |_| set_tab.set("agendas".into())>
                        <i class="fa-solid fa-mask"></i>" Agendas"
                    </button>
                })}
                {has_evolution.then(|| view! {
                    <button class=move || if tab.get() == "evolution" { "btn btn-sm btn-primary" } else { "btn btn-sm" }
                        on:click=move |_| set_tab.set("evolution".into())>
                        <i class="fa-solid fa-chart-line"></i>" Evolution"
                    </button>
                })}
            </div>

            // Tab content
            <div style="max-height: calc(100vh - 450px); overflow-y: auto;">
                {move || match tab.get().as_str() {
                    "evidence" => super::render_evidence_tab(s1.clone()).into_any(),
                    "influence" => super::render_influence_tab(s2.clone()).into_any(),
                    "agendas" => super::render_agendas_tab(s3.clone()).into_any(),
                    "evolution" => super::render_evolution_tab(s4.clone()).into_any(),
                    _ => view! { <p>"Unknown tab"</p> }.into_any(),
                }}
            </div>

            // Export button
            <div style="margin-top: 1rem; border-top: 1px solid var(--border); padding-top: 0.75rem;">
                <button class="btn btn-primary" style="font-size: 0.85rem;"
                    on:click=move |_| {
                        super::export_debate(
                            session_id.get_untracked(),
                            rounds.get_untracked(),
                            agents.get_untracked(),
                            Some(syn.clone()),
                        );
                    }
                >
                    <i class="fa-solid fa-download"></i>" Export Debate"
                </button>
            </div>
        </div>
    }.into_any()
}
