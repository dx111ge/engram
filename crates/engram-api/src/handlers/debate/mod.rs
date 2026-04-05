/// Multi-agent debate panel: auto-generated personas with diverse rigor, bias,
/// source access, and cognitive styles debate a topic using engram's knowledge graph.
///
/// Flow: POST /debate/start -> review/edit agents -> POST /debate/{id}/run -> SSE stream
///       -> optional inject/stop -> POST /debate/{id}/synthesize -> 3-layer synthesis.

pub mod types;
pub mod llm;
pub mod agents;
pub mod research;
pub mod synthesis;
pub mod modes;

pub use types::*;
pub use agents::{assign_agent_slots, parse_turn_metadata, strip_metadata_lines, tools_for_agent};

// ── Debug toggle ──────────────────────────────────────────────────────
// Toggle via POST /config {"debate_debug": true} -- zero cost when off.
// Logs go to debate_debug.log (append, open/close per line).
pub static DEBATE_DEBUG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub use llm::parse_json_from_llm;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;
use tokio::sync::Notify;

use crate::state::AppState;
use super::{api_err, ApiResult};

// ── HTTP handlers ───────────────────────────────────────────────────────

/// POST /debate/start -- create a new debate session and generate the panel.
pub async fn debate_start(
    State(state): State<AppState>,
    Json(req): Json<StartRequest>,
) -> ApiResult<serde_json::Value> {
    let count = req.agent_count.max(2).min(8);
    let session_id = format!("debate-{}", uuid_short());

    // Create deterministic agent slots
    let mut agents = agents::assign_agent_slots(count);

    // Generate persona details via LLM
    let prompt = agents::build_persona_generation_prompt(&req.topic, &agents, &req.mode, req.mode_input.as_deref(), llm::medium_output_budget(&state));
    let response = llm::call_llm(&state, prompt).await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("Panel generation failed: {e}")))?;

    let content = llm::extract_content(&response)
        .ok_or_else(|| api_err(StatusCode::BAD_GATEWAY, "No content in LLM response"))?;

    let persona_data = llm::parse_json_from_llm(&content)
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, e))?;

    // Merge LLM-generated details into agent slots
    if let Some(arr) = persona_data.as_array() {
        for item in arr {
            let id = item.get("id").and_then(|i| i.as_str()).unwrap_or("");
            if let Some(agent) = agents.iter_mut().find(|a| a.id == id) {
                if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                    agent.name = name.to_string();
                }
                if let Some(desc) = item.get("persona_description").and_then(|d| d.as_str()) {
                    agent.persona_description = desc.to_string();
                }
                if let Some(icon) = item.get("icon").and_then(|i| i.as_str()) {
                    agent.icon = icon.to_string();
                }
                if let Some(color) = item.get("color").and_then(|c| c.as_str()) {
                    agent.color = color.to_string();
                }
                // Only update bias for non-neutral agents
                if !agent.bias.is_neutral {
                    if let Some(bl) = item.get("bias_label").and_then(|b| b.as_str()) {
                        agent.bias.label = bl.to_string();
                    }
                    if let Some(bd) = item.get("bias_description").and_then(|b| b.as_str()) {
                        agent.bias.description = bd.to_string();
                    }
                }
            }
        }
    }

    // Fill in any agents that didn't get names from the LLM
    for (i, agent) in agents.iter_mut().enumerate() {
        if agent.name.is_empty() {
            agent.name = format!("Analyst {}", i + 1);
        }
        if agent.persona_description.is_empty() {
            agent.persona_description = format!("A {} analyst", agent.cognitive_style);
        }
    }

    let session = DebateSession {
        session_id: session_id.clone(),
        topic: req.topic.clone(),
        mode: req.mode.clone(),
        status: DebateStatus::AwaitingStart,
        agents: agents.clone(),
        rounds: Vec::new(),
        current_round: 0,
        max_rounds: req.max_rounds as usize,
        selection: None,
        synthesis: None,
        created_at: std::time::Instant::now(),
        notify: Arc::new(Notify::new()),
        pending_injection: None,
        briefing: None,
        researched_gaps: Vec::new(),
        mode_input: req.mode_input.clone(),
        progress: None,
        search_queries: Vec::new(),
    };

    // Store session
    {
        let mut sessions = state.debate_sessions.write().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock failed"))?;
        sessions.insert(session_id.clone(), session);
    }

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "topic": req.topic,
        "mode": req.mode,
        "status": "awaiting_start",
        "agents": agents,
        "max_rounds": req.max_rounds,
        "mode_input": req.mode_input,
    })))
}

/// GET /debate/{id} -- get full debate state.
pub async fn debate_get(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let sessions = state.debate_sessions.read().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock failed"))?;
    let session = sessions.get(&session_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "debate session not found"))?;

    Ok(Json(serde_json::json!({
        "session_id": session.session_id,
        "topic": session.topic,
        "mode": session.mode,
        "status": session.status,
        "agents": session.agents,
        "rounds": session.rounds,
        "current_round": session.current_round,
        "max_rounds": session.max_rounds,
        "selection": session.selection,
        "synthesis": session.synthesis,
        "briefing": session.briefing,
        "mode_input": session.mode_input,
        "progress": session.progress,
    })))
}

/// PATCH /debate/{id}/agents -- edit agents before starting.
pub async fn debate_edit_agents(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(edits): Json<AgentEdits>,
) -> ApiResult<serde_json::Value> {
    let mut sessions = state.debate_sessions.write().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock failed"))?;
    let session = sessions.get_mut(&session_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "debate session not found"))?;

    if session.status != DebateStatus::AwaitingStart {
        return Err(api_err(StatusCode::CONFLICT, "can only edit agents before debate starts"));
    }

    for edit in &edits.agents {
        if let Some(agent) = session.agents.iter_mut().find(|a| a.id == edit.id) {
            if let Some(ref name) = edit.name { agent.name = name.clone(); }
            if let Some(ref desc) = edit.persona_description { agent.persona_description = desc.clone(); }
            if let Some(rigor) = edit.rigor_level { agent.rigor_level = rigor.clamp(0.0, 1.0); }
            if let Some(ref sa) = edit.source_access { agent.source_access = sa.clone(); }
            if let Some(et) = edit.evidence_threshold { agent.evidence_threshold = et.clamp(0.0, 1.0); }
            if let Some(ref cs) = edit.cognitive_style { agent.cognitive_style = cs.clone(); }
            if let Some(ref bias) = edit.bias { agent.bias = bias.clone(); }
            if let Some(ref icon) = edit.icon { agent.icon = icon.clone(); }
            if let Some(ref color) = edit.color { agent.color = color.clone(); }
        }
    }

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "agents": session.agents,
    })))
}

/// POST /debate/{id}/run -- start or resume the debate.
pub async fn debate_run(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let should_spawn = {
        let mut sessions = state.debate_sessions.write().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock failed"))?;
        let session = sessions.get_mut(&session_id)
            .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "debate session not found"))?;

        match session.status {
            DebateStatus::AwaitingStart => {
                // First start: set to Researching, then spawn loop that does briefing + debate
                session.status = DebateStatus::Researching;
                true
            }
            DebateStatus::AwaitingInput => {
                // Resume: wake the existing loop, don't spawn a new one
                session.status = DebateStatus::Running;
                session.notify.notify_one();
                false
            }
            DebateStatus::AllRoundsComplete => {
                return Err(api_err(StatusCode::CONFLICT, "all rounds complete -- use /synthesize"));
            }
            _ => {
                return Err(api_err(StatusCode::CONFLICT, format!("cannot run debate in {:?} status", session.status)));
            }
        }
    };

    if should_spawn {
        let state_clone = state.clone();
        let sid = session_id.clone();
        tokio::spawn(async move {
            run_debate_loop(state_clone, sid).await;
        });
    }

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "status": "running",
        "message": "Debate running."
    })))
}

/// Update the progress field on the session.
fn set_progress(state: &AppState, session_id: &str, phase: &str, message: &str, agent: Option<&str>, current: usize, total: usize) {
    if let Ok(mut sessions) = state.debate_sessions.write() {
        if let Some(s) = sessions.get_mut(session_id) {
            s.progress = Some(DebateProgress {
                phase: phase.into(),
                message: message.into(),
                active_agent: agent.map(String::from),
                current,
                total,
            });
        }
    }
}

/// Background debate execution loop.
async fn run_debate_loop(state: AppState, session_id: String) {
    dbg_debate!("[debate] ========== DEBATE LOOP START session={} ==========", session_id);
    // Get session info
    let (agents, max_rounds, current_round, topic, mode, mode_input, tx) = {
        let sessions = match state.debate_sessions.read() {
            Ok(s) => s,
            Err(_) => return,
        };
        let session = match sessions.get(&session_id) {
            Some(s) => s,
            None => return,
        };
        let (tx, _) = tokio::sync::broadcast::channel::<String>(256);
        (session.agents.clone(), session.max_rounds, session.current_round, session.topic.clone(),
         session.mode.clone(), session.mode_input.clone(), tx)
    };

    // ── Starter Plate: build briefing before Round 1 ──
    {
        let needs_briefing = {
            let sessions = match state.debate_sessions.read() {
                Ok(s) => s,
                Err(_) => return,
            };
            sessions.get(&session_id).map(|s| s.briefing.is_none()).unwrap_or(false)
        };

        if needs_briefing {
            set_progress(&state, &session_id, "researching", "Building factual briefing...", None, 0, 0);
            let _ = tx.send(format!("event: status_change\ndata: {}\n\n",
                serde_json::json!({"status": "Researching", "message": "Building factual briefing..."})));

            let t_briefing = std::time::Instant::now();
            dbg_debate!("[debate] >> briefing START topic=\"{}\"", &topic[..topic.len().min(60)]);
            let briefing = research::build_starter_plate(&state, &topic).await;
            dbg_debate!("[debate] << briefing DONE {} facts, {} stored, {:.1}s", briefing.facts.len(), briefing.facts_stored, t_briefing.elapsed().as_secs_f32());
            eprintln!("[debate] briefing took {:.1}s ({} facts, {} stored)", t_briefing.elapsed().as_secs_f32(), briefing.facts.len(), briefing.facts_stored);

            // Store briefing + set status to Running
            {
                let mut sessions = match state.debate_sessions.write() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                if let Some(s) = sessions.get_mut(&session_id) {
                    // Don't add briefing questions to researched_gaps -- they're broad context,
                    // not specific gap queries. Gap-closing should find NEW specific gaps.
                    s.briefing = Some(briefing);
                    s.status = DebateStatus::Running;
                }
            }

            let _ = tx.send(format!("event: status_change\ndata: {}\n\n",
                serde_json::json!({"status": "Running", "message": "Briefing complete. Starting debate."})));
        }
    }

    // ── Generate short search queries from topic (once, reuse everywhere) ──
    let search_queries = {
        let already_has = state.debate_sessions.read().ok()
            .and_then(|s| s.get(&session_id).map(|s| !s.search_queries.is_empty()))
            .unwrap_or(false);
        if already_has {
            state.debate_sessions.read().ok()
                .and_then(|s| s.get(&session_id).map(|s| s.search_queries.clone()))
                .unwrap_or_default()
        } else {
            dbg_debate!("[debate] >> generating short search query from topic");
            let prompt = serde_json::json!({
                "messages": [{"role": "system", "content": format!(
                    "Convert this debate topic into ONE short search engine query (max 8 words). \
                     Return ONLY the query text, nothing else. No quotes, no JSON.\n\nTopic: \"{}\"", topic
                )}],
                "temperature": 0.1,
                "max_tokens": 64,
                "think": false
            });
            let queries = match llm::call_llm(&state, prompt).await {
                Ok(resp) => {
                    let content = llm::extract_content(&resp).unwrap_or_default().trim().trim_matches('"').to_string();
                    if content.len() > 5 { vec![content] } else { vec![topic.clone()] }
                }
                Err(_) => vec![topic.clone()],
            };
            dbg_debate!("[debate] << search query: {:?}", queries);
            // Store on session
            if let Ok(mut sessions) = state.debate_sessions.write() {
                if let Some(s) = sessions.get_mut(&session_id) {
                    s.search_queries = queries.clone();
                }
            }
            queries
        }
    };

    for round_idx in current_round..max_rounds {
        // Check if we should stop
        {
            let sessions = match state.debate_sessions.read() {
                Ok(s) => s,
                Err(_) => return,
            };
            if let Some(s) = sessions.get(&session_id) {
                if s.status == DebateStatus::Complete || s.status == DebateStatus::Error {
                    return;
                }
            }
        }

        // Get any pending injection
        let user_injection = {
            let mut sessions = match state.debate_sessions.write() {
                Ok(s) => s,
                Err(_) => return,
            };
            if let Some(s) = sessions.get_mut(&session_id) {
                s.pending_injection.take()
            } else {
                None
            }
        };

        let _ = tx.send(format!("event: round_start\ndata: {}\n\n",
            serde_json::json!({"round": round_idx + 1})));

        let t_round = std::time::Instant::now();
        dbg_debate!("[debate] ---- ROUND {} START ----", round_idx + 1);
        let mut round_turns = Vec::new();

        // Search once per round using short query, share across agents
        let search_query = search_queries.first().map(|s| s.as_str()).unwrap_or(&topic);
        let cached_web = research::execute_web_search(&state, search_query).await;
        let cached_web_opt = if cached_web.is_empty() { None } else { Some(cached_web.as_str()) };

        // Execute each agent's turn
        let agent_total = agents.len();
        for (agent_idx, agent) in agents.iter().enumerate() {
            set_progress(&state, &session_id, "debating",
                &format!("{} is analyzing...", agent.name),
                Some(&agent.name), agent_idx + 1, agent_total);
            let _ = tx.send(format!("event: turn_start\ndata: {}\n\n",
                serde_json::json!({"agent_id": agent.id, "agent_name": agent.name, "round": round_idx + 1})));

            // Get previous turns, gap research, and briefing
            let (prev_turns, prev_gap_research, briefing_summary) = {
                let sessions = match state.debate_sessions.read() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                if let Some(s) = sessions.get(&session_id) {
                    let briefing_text = s.briefing.as_ref().map(|b| b.summary.clone()).unwrap_or_default();
                    if let Some(prev_round) = s.rounds.last() {
                        (prev_round.turns.clone(), prev_round.gap_research.clone(), briefing_text)
                    } else {
                        (Vec::new(), Vec::new(), briefing_text)
                    }
                } else {
                    (Vec::new(), Vec::new(), String::new())
                }
            };

            dbg_debate!("[debate] >> agent turn {}/{} \"{}\" round={}", agent_idx + 1, agent_total, agent.name, round_idx + 1);
            let t_agent = std::time::Instant::now();
            match agents::execute_agent_turn(
                &state, agent, &topic, round_idx, &prev_turns, &agents,
                user_injection.as_deref(), &prev_gap_research, &briefing_summary,
                &mode, mode_input.as_deref(), &tx, cached_web_opt,
            ).await {
                Ok(turn) => {
                    dbg_debate!("[debate] << agent turn \"{}\" done conf={:.0}% evidence={} took={:.1}s", agent.name, turn.confidence * 100.0, turn.evidence.len(), t_agent.elapsed().as_secs_f32());
                    let _ = tx.send(format!("event: turn_complete\ndata: {}\n\n",
                        serde_json::json!({
                            "agent_id": agent.id,
                            "agent_name": agent.name,
                            "position": turn.position,
                            "confidence": turn.confidence,
                            "evidence_count": turn.evidence.len(),
                            "agrees_with": turn.agrees_with,
                            "disagrees_with": turn.disagrees_with,
                        })));
                    round_turns.push(turn);
                }
                Err(e) => {
                    dbg_debate!("[debate] << agent turn \"{}\" FAILED took={:.1}s err={}", agent.name, t_agent.elapsed().as_secs_f32(), e);
                    let _ = tx.send(format!("event: error\ndata: {}\n\n",
                        serde_json::json!({"agent_id": agent.id, "error": e})));
                }
            }
        }

        // Build the round
        // Get already-researched gaps for dedup
        let already_researched = {
            let sessions = match state.debate_sessions.read() {
                Ok(s) => s,
                Err(_) => return,
            };
            sessions.get(&session_id).map(|s| s.researched_gaps.clone()).unwrap_or_default()
        };

        let mut round = DebateRound {
            round_number: round_idx,
            turns: round_turns,
            user_injection: user_injection.clone(),
            gap_research: Vec::new(),
            moderator_checks: Vec::new(),
        };

        let t_agents_done = t_round.elapsed();
        dbg_debate!("[debate] round {} agents done {:.1}s ({} turns)", round_idx + 1, t_agents_done.as_secs_f32(), round.turns.len());
        eprintln!("[debate] round {} agents took {:.1}s ({} turns)", round_idx + 1, t_agents_done.as_secs_f32(), round.turns.len());

        // ── Moderator fact-check ──
        set_progress(&state, &session_id, "moderating",
            "Fact-checking claims...", None, 0, 0);
        let t_mod = std::time::Instant::now();
        dbg_debate!("[debate] >> moderator fact-check round={}", round_idx + 1);
        let checks = research::moderate_round(&state, &round, &agents, &topic).await;
        dbg_debate!("[debate] << moderator done {} checks {:.1}s", checks.len(), t_mod.elapsed().as_secs_f32());
        eprintln!("[debate] round {} moderator took {:.1}s ({} checks)", round_idx + 1, t_mod.elapsed().as_secs_f32(), checks.len());
        round.moderator_checks = checks;

        // ── Gap-closing research between rounds ──
        if round_idx + 1 < max_rounds {
            set_progress(&state, &session_id, "gap_closing",
                "Detecting knowledge gaps...", None, 0, 0);
            let _ = tx.send(format!("event: gap_research_start\ndata: {}\n\n",
                serde_json::json!({"round": round_idx + 1})));

            let t_gap_start = std::time::Instant::now();
            dbg_debate!("[debate] >> gap detection round={}", round_idx + 1);
            let gap_queries = research::detect_gaps(&state, &round, &agents, &topic, &already_researched).await;
            dbg_debate!("[debate] << gap detection found {} gaps {:.1}s", gap_queries.len(), t_gap_start.elapsed().as_secs_f32());
            eprintln!("[debate] gap detection took {:.1}s, found {} gaps", t_gap_start.elapsed().as_secs_f32(), gap_queries.len());

            if !gap_queries.is_empty() {
                let _ = tx.send(format!("event: gap_research_progress\ndata: {}\n\n",
                    serde_json::json!({"gaps": gap_queries, "message": format!("Researching {} gaps...", gap_queries.len())})));

                // Close gaps sequentially (Ollama serializes anyway) with per-gap progress
                let t_close_start = std::time::Instant::now();
                let mut gap_results = Vec::new();
                for (gi, gq) in gap_queries.iter().enumerate() {
                    set_progress(&state, &session_id, "gap_closing",
                        &format!("Closing gap {}/{}: {}...", gi + 1, gap_queries.len(), &gq[..gq.len().min(50)]),
                        None, gi, gap_queries.len());
                    dbg_debate!("[debate] >> gap {}/{} \"{}\"", gi + 1, gap_queries.len(), &gq[..gq.len().min(60)]);
                    let t_gap = std::time::Instant::now();
                    let result = research::close_single_gap_with_timeout(&state, gq, &topic, 90).await;
                    dbg_debate!("[debate] << gap {}/{} ingested={} facts={} {:.1}s", gi + 1, gap_queries.len(), result.ingested, result.facts_stored, t_gap.elapsed().as_secs_f32());
                    gap_results.push(result);
                }
                eprintln!("[debate] gap-closing took {:.1}s total for {} gaps", t_close_start.elapsed().as_secs_f32(), gap_queries.len());

                let findings_count: usize = gap_results.iter().map(|g| g.findings.len()).sum();
                let ingested_count: usize = gap_results.iter().filter(|g| g.ingested).count();

                set_progress(&state, &session_id, "gap_closing",
                    &format!("Gaps complete: {}/{} resolved", ingested_count, gap_results.len()), None, gap_results.len(), gap_results.len());
                let _ = tx.send(format!("event: gap_research_complete\ndata: {}\n\n",
                    serde_json::json!({
                        "gaps_researched": gap_results.len(),
                        "findings": findings_count,
                        "ingested": ingested_count,
                    })));

                round.gap_research = gap_results;
            }
        } else {
            dbg_debate!("[debate] skip gap-closing on final round (by design)");
        }

        dbg_debate!("[debate] ---- ROUND {} DONE {:.1}s ----", round_idx + 1, t_round.elapsed().as_secs_f32());
        eprintln!("[debate] round {} TOTAL: {:.1}s", round_idx + 1, t_round.elapsed().as_secs_f32());

        // Store round results
        {
            let mut sessions = match state.debate_sessions.write() {
                Ok(s) => s,
                Err(_) => return,
            };
            if let Some(s) = sessions.get_mut(&session_id) {
                // Store gap queries for dedup in future rounds
                for gr in &round.gap_research {
                    // Only mark gap as "done" if it was actually closed (found real data).
                    // Unclosed gaps will be retried in the next round.
                    if gr.ingested && !s.researched_gaps.contains(&gr.gap_query) {
                        s.researched_gaps.push(gr.gap_query.clone());
                    }
                }
                s.rounds.push(round);
                s.current_round = round_idx + 1;
            }
        }

        let _ = tx.send(format!("event: round_complete\ndata: {}\n\n",
            serde_json::json!({"round": round_idx + 1})));

        // Pause for user input between rounds (except after the last round)
        if round_idx + 1 < max_rounds {
            {
                let mut sessions = match state.debate_sessions.write() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                if let Some(s) = sessions.get_mut(&session_id) {
                    s.status = DebateStatus::AwaitingInput;
                }
            }

            let _ = tx.send(format!("event: awaiting_input\ndata: {}\n\n",
                serde_json::json!({"round": round_idx + 1, "prompt": "Continue, inject a question, or synthesize?"})));

            // Wait for user to resume
            let notify = {
                let sessions = match state.debate_sessions.read() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                sessions.get(&session_id).map(|s| s.notify.clone())
            };

            if let Some(n) = notify {
                n.notified().await;
            }

            // Check if status changed (might have been set to Synthesizing or stopped)
            {
                let sessions = match state.debate_sessions.read() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                if let Some(s) = sessions.get(&session_id) {
                    match s.status {
                        DebateStatus::Running => {} // continue
                        DebateStatus::Synthesizing => break, // skip to synthesis
                        _ => return, // stopped or error
                    }
                }
            }
        }
    }

    // All rounds complete -- check if synthesis was requested early
    {
        let sessions = match state.debate_sessions.read() {
            Ok(s) => s,
            Err(_) => return,
        };
        if let Some(s) = sessions.get(&session_id) {
            if s.status == DebateStatus::Complete {
                return;
            }
        }
    }

    // All rounds done -- set distinct status so frontend knows to show only Synthesize
    {
        let mut sessions = match state.debate_sessions.write() {
            Ok(s) => s,
            Err(_) => return,
        };
        if let Some(s) = sessions.get_mut(&session_id) {
            s.status = DebateStatus::AllRoundsComplete;
        }
    }

    let _ = tx.send(format!("event: all_rounds_complete\ndata: {}\n\n",
        serde_json::json!({"prompt": "All rounds complete. Click Synthesize to generate the analysis."})));
}

/// POST /debate/{id}/inject -- user injects a question between rounds.
pub async fn debate_inject(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(req): Json<InjectRequest>,
) -> ApiResult<serde_json::Value> {
    let mut sessions = state.debate_sessions.write().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock failed"))?;
    let session = sessions.get_mut(&session_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "debate session not found"))?;

    if session.status != DebateStatus::AwaitingInput {
        return Err(api_err(StatusCode::CONFLICT, "can only inject when debate is awaiting input"));
    }

    session.pending_injection = Some(req.message.clone());
    session.status = DebateStatus::Running;
    session.notify.notify_one();

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "injected": req.message,
        "status": "running"
    })))
}

/// POST /debate/{id}/stop -- pause the debate.
pub async fn debate_stop(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut sessions = state.debate_sessions.write().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock failed"))?;
    let session = sessions.get_mut(&session_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "debate session not found"))?;

    session.status = DebateStatus::AwaitingInput;

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "status": "awaiting_input"
    })))
}

/// POST /debate/{id}/synthesize -- trigger final synthesis.
pub async fn debate_synthesize(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    // Get session data for synthesis
    let session_data = {
        let mut sessions = state.debate_sessions.write().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock failed"))?;
        let session = sessions.get_mut(&session_id)
            .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "debate session not found"))?;

        if session.rounds.is_empty() {
            return Err(api_err(StatusCode::CONFLICT, "no rounds completed yet"));
        }

        session.status = DebateStatus::Synthesizing;
        // If debate loop is waiting, wake it up
        session.notify.notify_one();
        session.clone()
    };

    // Helper: run an LLM call with a per-call timeout (120s).
    // Returns Err on timeout or LLM failure.
    let call_with_timeout = |state: AppState, prompt: serde_json::Value| async move {
        match tokio::time::timeout(
            std::time::Duration::from_secs(120),
            llm::call_llm(&state, prompt),
        ).await {
            Ok(result) => result,
            Err(_) => Err("LLM call timed out (120s)".to_string()),
        }
    };

    let total_steps = if modes::uses_selection(&session_data.mode) { 6 } else { 5 };
    let mut step = 0;

    // ── Layer 0: Select-then-Refine (mode-gated) ──
    let selection = if modes::uses_selection(&session_data.mode) {
        step += 1;
        set_progress(&state, &session_id, "synthesizing", "Layer 0: Scoring agents and selecting strongest position...", None, step, total_steps);
        eprintln!("[debate] synthesis: Layer 0 -- selecting strongest position...");
        let t0 = std::time::Instant::now();
        let sel_prompt = synthesis::build_selection_prompt(&session_data, llm::medium_output_budget(&state));
        match call_with_timeout(state.clone(), sel_prompt).await {
            Ok(sel_response) => {
                let sel_content = llm::extract_content(&sel_response);
                let sel = match sel_content.and_then(|c| llm::parse_json_from_llm(&c).ok()) {
                    Some(sel_json) => serde_json::from_value::<SelectionResult>(sel_json).ok(),
                    None => { eprintln!("[debate] Layer 0: parse failed, skipping selection"); None }
                };
                if let Some(ref s) = sel {
                    eprintln!("[debate] Layer 0: selected {} (score {:.1}) in {:.1}s", s.selected_agent_name,
                        s.scores.iter().find(|sc| sc.agent_id == s.selected_agent_id).map(|sc| sc.total_score).unwrap_or(0.0),
                        t0.elapsed().as_secs_f32());
                }
                sel
            }
            Err(e) => { eprintln!("[debate] Layer 0: failed: {}, skipping", e); None }
        }
    } else {
        None
    };

    // Store selection on session
    if selection.is_some() {
        let mut sessions = state.debate_sessions.write().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock failed"))?;
        if let Some(session) = sessions.get_mut(&session_id) {
            session.selection = selection.clone();
        }
    }

    // ── Pass 1: Evidence conclusion (think=ON, prose) ──
    step += 1;
    set_progress(&state, &session_id, "synthesizing", "Pass 1: Writing evidence-based conclusion (deep analysis)...", None, step, total_steps);
    eprintln!("[debate] synthesis: Pass 1 -- generating evidence conclusion...");
    let t0 = std::time::Instant::now();
    let conclusion_prompt = synthesis::build_conclusion_prompt(&session_data, selection.as_ref(), llm::medium_output_budget(&state));
    let conclusion_response = call_with_timeout(state.clone(), conclusion_prompt).await
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, format!("Synthesis conclusion failed: {e}")))?;
    let conclusion_content = llm::extract_content(&conclusion_response)
        .ok_or_else(|| api_err(StatusCode::BAD_GATEWAY, "No content in conclusion response"))?;
    let (evidence_conclusion, conclusion_confidence) = synthesis::parse_conclusion(&conclusion_content);
    eprintln!("[debate] Pass 1 done: {} chars, confidence={:.2} in {:.1}s", evidence_conclusion.len(), conclusion_confidence, t0.elapsed().as_secs_f32());

    // ── Pass 2: Structured extraction (think=OFF, sequential to avoid Ollama queue stacking) ──
    let short_budget = llm::short_output_budget(&state);

    // 2a: Evidence gaps
    step += 1;
    set_progress(&state, &session_id, "synthesizing", "Pass 2a: Identifying evidence gaps and investigations...", None, step, total_steps);
    eprintln!("[debate] synthesis: Pass 2a -- gaps...");
    let t0 = std::time::Instant::now();
    let gaps_prompt = synthesis::build_gaps_prompt(&session_data, &evidence_conclusion, short_budget);
    let gaps_json = parse_extraction_result(call_with_timeout(state.clone(), gaps_prompt).await, "gaps");
    eprintln!("[debate] Pass 2a done in {:.1}s", t0.elapsed().as_secs_f32());

    // 2b: Influence map
    step += 1;
    set_progress(&state, &session_id, "synthesizing", "Pass 2b: Mapping agent influence and bias distortion...", None, step, total_steps);
    eprintln!("[debate] synthesis: Pass 2b -- influence...");
    let t0 = std::time::Instant::now();
    let influence_prompt = synthesis::build_influence_prompt(&session_data, &evidence_conclusion, short_budget);
    let influence_json = parse_extraction_result(call_with_timeout(state.clone(), influence_prompt).await, "influence");
    eprintln!("[debate] Pass 2b done in {:.1}s", t0.elapsed().as_secs_f32());

    // 2c: Consensus (agreements, disagreements, tensions)
    step += 1;
    set_progress(&state, &session_id, "synthesizing", "Pass 2c: Analyzing consensus, disagreements, and tensions...", None, step, total_steps);
    eprintln!("[debate] synthesis: Pass 2c -- consensus...");
    let t0 = std::time::Instant::now();
    let consensus_prompt = synthesis::build_consensus_prompt(&session_data, &evidence_conclusion, short_budget);
    let consensus_json = parse_extraction_result(call_with_timeout(state.clone(), consensus_prompt).await, "consensus");
    eprintln!("[debate] Pass 2c done in {:.1}s", t0.elapsed().as_secs_f32());

    // 2d: Evolution + final positions
    step += 1;
    set_progress(&state, &session_id, "synthesizing", "Pass 2d: Tracking agent evolution and final positions...", None, step, total_steps);
    eprintln!("[debate] synthesis: Pass 2d -- evolution...");
    let t0 = std::time::Instant::now();
    let evolution_prompt = synthesis::build_evolution_prompt(&session_data, &evidence_conclusion, short_budget);
    let evolution_json = parse_extraction_result(call_with_timeout(state.clone(), evolution_prompt).await, "evolution");
    eprintln!("[debate] Pass 2d done in {:.1}s", t0.elapsed().as_secs_f32());

    // Assemble final Synthesis struct
    let synthesis = Synthesis {
        evidence_conclusion,
        conclusion_confidence,
        evidence_gaps: llm::extract_string_array(&gaps_json, "evidence_gaps"),
        key_evidence: Vec::new(),
        influence_map: serde_json::from_value(
            influence_json.get("influence_map").cloned().unwrap_or(serde_json::json!([]))
        ).unwrap_or_default(),
        unexpected_alignments: Vec::new(),
        cherry_picks: serde_json::from_value(
            influence_json.get("cherry_picks").cloned().unwrap_or(serde_json::json!([]))
        ).unwrap_or_default(),
        hidden_agendas: Vec::new(),
        beneficiary_map: Vec::new(),
        parallel_interests: Vec::new(),
        blind_spots: Vec::new(),
        areas_of_agreement: serde_json::from_value(
            consensus_json.get("areas_of_agreement").cloned().unwrap_or(serde_json::json!([]))
        ).unwrap_or_default(),
        areas_of_disagreement: serde_json::from_value(
            consensus_json.get("areas_of_disagreement").cloned().unwrap_or(serde_json::json!([]))
        ).unwrap_or_default(),
        key_tensions: llm::extract_string_array(&consensus_json, "key_tensions"),
        recommended_investigations: llm::extract_string_array(&gaps_json, "recommended_investigations"),
        evolution: serde_json::from_value(
            evolution_json.get("evolution").cloned().unwrap_or(serde_json::json!([]))
        ).unwrap_or_default(),
        agent_positions: serde_json::from_value(
            evolution_json.get("agent_positions").cloned().unwrap_or(serde_json::json!([]))
        ).unwrap_or_default(),
        raw_llm_output: None,
    };

    eprintln!("[debate] synthesis complete: conclusion={} chars, gaps={}, influence={}, tensions={}, evolution={}",
        synthesis.evidence_conclusion.len(),
        synthesis.evidence_gaps.len(),
        synthesis.influence_map.len(),
        synthesis.key_tensions.len(),
        synthesis.evolution.len(),
    );

    // Store synthesis on session
    {
        let mut sessions = state.debate_sessions.write().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock failed"))?;
        if let Some(session) = sessions.get_mut(&session_id) {
            session.synthesis = Some(synthesis.clone());
            session.status = DebateStatus::Complete;
        }
    }

    set_progress(&state, &session_id, "complete", "Synthesis complete", None, total_steps, total_steps);

    // Retrieve selection for the response
    let selection_out = {
        let sessions = state.debate_sessions.read().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock failed"))?;
        sessions.get(&session_id).and_then(|s| s.selection.clone())
    };

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "status": "complete",
        "selection": selection_out,
        "synthesis": synthesis,
    })))
}

/// GET /debate/{id}/stream -- SSE event stream for debate progress.
/// Uses polling (every 2s) to emit state changes, round completions, and synthesis.
pub async fn debate_stream(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> axum::response::Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>> {
    struct PollState {
        state: AppState,
        session_id: String,
        last_round_count: usize,
        last_status: String,
        last_progress: String,
        sent_initial: bool,
    }

    let initial = PollState {
        state,
        session_id,
        last_round_count: 0,
        last_status: String::new(),
        last_progress: String::new(),
        sent_initial: false,
    };

    let stream = futures::stream::unfold(initial, |mut ps| async move {
        if !ps.sent_initial {
            ps.sent_initial = true;
            let data = {
                let sessions = ps.state.debate_sessions.read().unwrap_or_else(|e| e.into_inner());
                sessions.get(&ps.session_id).map(|session| {
                    ps.last_status = format!("{:?}", session.status);
                    ps.last_round_count = session.rounds.len();
                    serde_json::json!({
                        "status": session.status,
                        "agents": session.agents,
                        "current_round": session.current_round,
                        "rounds_completed": session.rounds.len(),
                    })
                })
            };
            return match data {
                Some(d) => Some((
                    Ok(axum::response::sse::Event::default().event("state").data(d.to_string())),
                    ps,
                )),
                None => None,
            };
        }

        // Poll every 2 seconds
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Snapshot session state (drop lock before returning ps)
        let snapshot = ps.state.debate_sessions.read().ok().and_then(|sessions| {
            sessions.get(&ps.session_id).map(|s| {
                (format!("{:?}", s.status), s.status.clone(), s.rounds.clone(), s.synthesis.clone(), s.progress.clone())
            })
        });

        let (status, status_enum, rounds, synthesis, progress) = match snapshot {
            Some(s) => s,
            None => return None, // session deleted
        };

        // Emit progress updates (synthesis steps, gap-closing, etc.)
        if let Some(ref prog) = progress {
            let prog_msg = format!("{}:{}", prog.phase, prog.message);
            if prog_msg != ps.last_progress {
                ps.last_progress = prog_msg;
                let data = serde_json::json!({
                    "phase": prog.phase,
                    "message": prog.message,
                    "current": prog.current,
                    "total": prog.total,
                    "active_agent": prog.active_agent,
                });
                return Some((
                    Ok(axum::response::sse::Event::default().event("progress").data(data.to_string())),
                    ps,
                ));
            }
        }

        let round_count = rounds.len();

        // Emit new round data
        if round_count > ps.last_round_count {
            if let Some(round) = rounds.get(ps.last_round_count) {
                ps.last_round_count += 1;
                let data = serde_json::json!({
                    "round": round.round_number + 1,
                    "turns": round.turns,
                    "user_injection": round.user_injection,
                });
                return Some((
                    Ok(axum::response::sse::Event::default().event("round_complete").data(data.to_string())),
                    ps,
                ));
            }
        }

        // Emit status changes
        if status != ps.last_status {
            ps.last_status = status.clone();

            // If complete, emit synthesis
            if status_enum == DebateStatus::Complete {
                if let Some(ref syn) = synthesis {
                    let data = serde_json::to_string(syn).unwrap_or_default();
                    return Some((
                        Ok(axum::response::sse::Event::default().event("synthesis_complete").data(data)),
                        ps,
                    ));
                }
            }

            let data = serde_json::json!({"status": status});
            return Some((
                Ok(axum::response::sse::Event::default().event("status_change").data(data.to_string())),
                ps,
            ));
        }

        // Complete -- stop stream
        if status_enum == DebateStatus::Complete {
            return None;
        }

        // Keepalive
        Some((
            Ok(axum::response::sse::Event::default().comment("keepalive")),
            ps,
        ))
    });

    axum::response::Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::default())
}

/// POST /debate/test-gap -- test gap-closing directly (debug endpoint).
pub async fn debate_test_gap(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let query = body.get("query").and_then(|v| v.as_str()).unwrap_or("test gap query");
    let topic = body.get("topic").and_then(|v| v.as_str()).unwrap_or("general");
    eprintln!("[test-gap] Starting: query=\"{}\"", query);
    dbg_debate!("[test-gap] query=\"{}\"", query);
    let result = research::close_gaps(&state, &[query.to_string()], topic).await;
    let gap = result.into_iter().next().unwrap_or_else(|| GapResearch {
        gap_query: query.to_string(), source: "test".into(), findings: Vec::new(),
        ingested: false, entities_stored: Vec::new(), facts_stored: 0, relations_created: 0,
    });
    Ok(Json(serde_json::json!({
        "gap_query": gap.gap_query,
        "ingested": gap.ingested,
        "facts_stored": gap.facts_stored,
        "relations_created": gap.relations_created,
        "findings": gap.findings,
        "entities_stored": gap.entities_stored,
    })))
}

/// POST /debate/test-fetch -- test article fetching in isolation (debug endpoint).
pub async fn debate_test_fetch(
    Json(body): Json<serde_json::Value>,
) -> ApiResult<serde_json::Value> {
    let urls: Vec<String> = if let Some(arr) = body.get("urls").and_then(|v| v.as_array()) {
        arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()
    } else if let Some(url) = body.get("url").and_then(|v| v.as_str()) {
        vec![url.to_string()]
    } else {
        return Ok(Json(serde_json::json!({"error": "provide 'url' or 'urls'"})));
    };

    let mut results = Vec::new();
    for url in &urls {
        let t0 = std::time::Instant::now();
        let content = research::fetch_article_content_debug(url).await;
        let elapsed = t0.elapsed().as_secs_f32();
        results.push(serde_json::json!({
            "url": url,
            "elapsed_s": format!("{:.1}", elapsed),
            "chars": content.as_ref().map(|c| c.len()).unwrap_or(0),
            "ok": content.is_some(),
            "preview": content.as_ref().map(|c| &c[..c.len().min(200)]).unwrap_or(""),
        }));
    }

    Ok(Json(serde_json::json!({"results": results})))
}

/// DELETE /debate/{id} -- remove a debate session.
pub async fn debate_delete(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let mut sessions = state.debate_sessions.write().map_err(|_| api_err(StatusCode::INTERNAL_SERVER_ERROR, "lock failed"))?;
    sessions.remove(&session_id);
    Ok(Json(serde_json::json!({"deleted": session_id})))
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Parse an extraction LLM result into JSON, logging failures.
fn parse_extraction_result(
    result: Result<serde_json::Value, String>,
    label: &str,
) -> serde_json::Value {
    match result {
        Ok(response) => {
            let content = llm::extract_content(&response).unwrap_or_default();
            if content.is_empty() {
                eprintln!("[debate] Pass 2 ({label}): empty content");
                return serde_json::json!({});
            }
            match llm::parse_json_from_llm(&content) {
                Ok(json) => {
                    eprintln!("[debate] Pass 2 ({label}): OK");
                    json
                }
                Err(e) => {
                    eprintln!("[debate] Pass 2 ({label}): JSON parse failed: {}", e);
                    serde_json::json!({})
                }
            }
        }
        Err(e) => {
            eprintln!("[debate] Pass 2 ({label}): LLM call failed: {}", e);
            serde_json::json!({})
        }
    }
}

/// Generate a short UUID-like ID.
fn uuid_short() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    format!("{:x}", ts & 0xFFFF_FFFF_FFFF)
}
