/// Multi-pass synthesis for the debate panel.
///
/// Architecture:
/// - Layer 0: Select-then-Refine (score agents, pick strongest)
/// - Pass 1 (think=ON):  Evidence conclusion -- prose answer to the question
/// - Pass 2 (think=OFF, json): Structured analysis fields, one group at a time
///
/// Only the LAST round's full positions are sent. Earlier rounds are summarized
/// as metadata (confidence trajectory, shifts, concessions). This keeps prompts
/// small enough for any model.

use super::types::*;
use super::llm::{short_output_budget, medium_output_budget};

// ── Condensed context builder ──────────────────────────────────────────

/// Build a condensed debate context: last-round positions + metadata from all rounds.
/// This replaces the full transcript for synthesis, keeping prompt size manageable.
fn build_condensed_context(session: &DebateSession) -> String {
    let mut ctx = String::new();

    // Agent profiles
    ctx.push_str("Agent profiles:\n");
    for a in &session.agents {
        ctx.push_str(&format!("- {} ({}): rigor={:.0}%, bias={}\n",
            a.name, a.id, a.rigor_level * 100.0,
            if a.bias.is_neutral { "NEUTRAL".to_string() } else { a.bias.label.clone() }
        ));
    }

    // Confidence evolution across rounds (compact)
    ctx.push_str("\nConfidence evolution per agent (by round):\n");
    for agent in &session.agents {
        let trajectory: Vec<String> = session.rounds.iter().map(|r| {
            r.turns.iter().find(|t| t.agent_id == agent.id)
                .map(|t| format!("{:.0}%", t.confidence * 100.0))
                .unwrap_or_else(|| "-".into())
        }).collect();
        ctx.push_str(&format!("  {}: {}\n", agent.name, trajectory.join(" -> ")));
    }

    // Position shifts and concessions (all rounds, compact)
    ctx.push_str("\nPosition shifts and concessions:\n");
    for (i, round) in session.rounds.iter().enumerate() {
        let shifts: Vec<String> = round.turns.iter().filter_map(|t| {
            let agent = session.agents.iter().find(|a| a.id == t.agent_id)?;
            if t.position_shift.is_empty() && t.concessions.is_empty() { return None; }
            let mut s = format!("  R{} {}: ", i + 1, agent.name);
            if !t.position_shift.is_empty() { s.push_str(&format!("SHIFT: {} ", t.position_shift)); }
            if !t.concessions.is_empty() { s.push_str(&format!("CONCEDES: {}", t.concessions.join("; "))); }
            Some(s)
        }).collect();
        if !shifts.is_empty() {
            for s in shifts { ctx.push_str(&s); ctx.push('\n'); }
        }
    }

    // Gap research summaries
    let all_gaps: Vec<&GapResearch> = session.rounds.iter()
        .flat_map(|r| r.gap_research.iter())
        .collect();
    if !all_gaps.is_empty() {
        ctx.push_str(&format!("\nGap research ({} topics investigated):\n", all_gaps.len()));
        for gr in &all_gaps {
            let status = if gr.ingested { format!("ingested ({} facts)", gr.facts_stored) } else { "not found".into() };
            ctx.push_str(&format!("  - \"{}\" [{}]\n", gr.gap_query, status));
        }
    }

    // Moderator fact-checks
    let all_checks: Vec<&ModeratorCheck> = session.rounds.iter()
        .flat_map(|r| r.moderator_checks.iter())
        .collect();
    if !all_checks.is_empty() {
        ctx.push_str(&format!("\nModerator fact-checks ({} claims checked):\n", all_checks.len()));
        for mc in &all_checks {
            ctx.push_str(&format!("  - {} [verdict: {:?}, confidence: {}]\n",
                mc.claim, mc.verdict,
                mc.engram_confidence.map(|c| format!("{:.2}", c)).unwrap_or_else(|| "n/a".into())
            ));
        }
    }

    // LAST ROUND: full positions (the core input for synthesis)
    if let Some(last) = session.rounds.last() {
        ctx.push_str(&format!("\n=== Final Round ({}) -- Full Positions ===\n", last.round_number + 1));
        for turn in &last.turns {
            let agent = session.agents.iter().find(|a| a.id == turn.agent_id);
            let name = agent.map(|a| a.name.as_str()).unwrap_or(&turn.agent_id);
            let bias = agent.map(|a| {
                if a.bias.is_neutral { "NEUTRAL".to_string() }
                else { a.bias.label.clone() }
            }).unwrap_or_default();
            ctx.push_str(&format!(
                "\n--- {} [{}] (confidence: {:.0}%) ---\n{}\n",
                name, bias, turn.confidence * 100.0, turn.position,
            ));
        }
    }

    ctx
}

// ── Layer 0: Select-then-Refine ────────────────────────────────────────

/// Build the selection prompt that scores each agent and picks the strongest position.
pub fn build_selection_prompt(session: &DebateSession, max_tokens: u64) -> serde_json::Value {
    let last_round = session.rounds.last();
    let mut agent_positions = String::new();
    for agent in &session.agents {
        let final_turn = last_round.and_then(|r| r.turns.iter().find(|t| t.agent_id == agent.id));
        let turn = final_turn.or_else(|| {
            session.rounds.iter().rev().find_map(|r| r.turns.iter().find(|t| t.agent_id == agent.id))
        });
        if let Some(t) = turn {
            agent_positions.push_str(&format!(
                "=== {} ({}) ===\nBias: {}\nConfidence: {:.2}\nEvidence cited: {} items\nPosition:\n{}\n\n",
                agent.name, agent.id,
                if agent.bias.is_neutral { "NEUTRAL".to_string() } else { agent.bias.label.clone() },
                t.confidence, t.evidence.len(), t.position,
            ));
        }
    }

    let system_prompt = format!(
        r#"You are a debate judge. Score each agent's FINAL position and select the strongest.

Topic: "{topic}"

{positions}

Score each on 4 criteria (0-10): evidence_quality, internal_consistency, counterargument_handling, confidence_calibration.
total_score = evidence_quality*0.35 + internal_consistency*0.20 + counterargument_handling*0.25 + confidence_calibration*0.20

Return JSON:
{{"scores":[{{"agent_id":"...","agent_name":"...","evidence_quality":0,"internal_consistency":0,"counterargument_handling":0,"confidence_calibration":0,"total_score":0}}],"selected_agent_id":"...","selected_agent_name":"...","selected_position":"full position text","best_counterpoints":[{{"agent_id":"...","agent_name":"...","point":"...","relevance":"..."}}],"selection_rationale":"..."}}"#,
        topic = session.topic,
        positions = agent_positions,
    );

    serde_json::json!({
        "messages": [{ "role": "system", "content": system_prompt }],
        "temperature": 0.2,
        "max_tokens": max_tokens,
        "think": false
    })
}

// ── Pass 1: Evidence Conclusion (think=ON, prose) ──────────────────────

/// Build the conclusion prompt. Returns prose, not JSON.
pub fn build_conclusion_prompt(session: &DebateSession, selection: Option<&SelectionResult>, max_tokens: u64, output_language: Option<&str>) -> serde_json::Value {
    let context = build_condensed_context(session);

    let selection_note = if let Some(sel) = selection {
        format!(
            "\nThe strongest position was from {} (score: {:.1}): \"{}\"\n\
             Key counterpoints from other agents:\n{}\n",
            sel.selected_agent_name,
            sel.scores.iter().find(|s| s.agent_id == sel.selected_agent_id)
                .map(|s| s.total_score).unwrap_or(0.0),
            &sel.selected_position[..sel.selected_position.len().min(2000)],
            sel.best_counterpoints.iter()
                .map(|cp| format!("- {}: {}", cp.agent_name, cp.point))
                .collect::<Vec<_>>().join("\n"),
        )
    } else {
        String::new()
    };

    let mode_additions = super::modes::synthesis_additions(&session.mode, session.mode_input.as_deref());

    let mode_conclusion_guidance = match session.mode {
        DebateMode::ScenarioForecast => r#"
This was a SCENARIO FORECAST debate where each agent built a distinct future scenario.
Your conclusion must:
- Compare and contrast ALL scenarios presented, not elevate one to "the answer"
- Assign probability ranges to each scenario (must sum to ~100%)
- Identify which branching conditions determine which scenario unfolds
- Highlight where scenarios converge (consensus) vs diverge (genuine uncertainty)
- State which early warning indicators to watch for each scenario"#,
        DebateMode::RedTeam => r#"
This was a RED TEAM exercise. Your conclusion must focus on:
- Which strategies survived red team scrutiny and which were broken
- The most critical vulnerabilities discovered
- Concrete recommendations ranked by feasibility"#,
        DebateMode::Premortem => r#"
This was a PRE-MORTEM analysis. Your conclusion must:
- Rank failure modes by probability * severity
- Identify which failures are preventable vs must be mitigated
- Provide specific preventive actions for the top failure modes"#,
        DebateMode::DecisionMatrix => r#"
This was a DECISION MATRIX evaluation. Your conclusion must:
- Present the final ranked recommendation with scores
- Explain why the top option won and what trade-offs it involves
- State under what conditions the ranking would change"#,
        _ => "",
    };

    let system_prompt = format!(
        r#"You are a senior intelligence analyst writing the final assessment for a {rounds}-round debate.

Topic: "{topic}"
{selection_note}
{context}
{mode_additions}
{mode_conclusion_guidance}

Write a comprehensive evidence-based conclusion that DIRECTLY ANSWERS the question/topic.
- Commit to a probabilistic assessment with specific probability ranges
- Ground your conclusion in the strongest arguments and evidence from the debate
- Acknowledge key vulnerabilities and counterarguments
- Be specific and actionable, not vague
- Your confidence score must reflect genuine uncertainty -- speculative extrapolations should not exceed 0.70, only well-evidenced assessments with multiple corroborating sources warrant 0.80+

Write 3-5 paragraphs of prose. No JSON, no markdown headers, just clear analytical writing.
End with a single line: "CONFIDENCE: X.XX" (0.00-1.00) reflecting your overall assessment confidence.{lang_instruction}"#,
        topic = session.topic,
        rounds = session.rounds.len(),
        selection_note = selection_note,
        context = context,
        mode_additions = mode_additions,
        lang_instruction = match output_language {
            Some(lang) if !lang.is_empty() && lang != "en" => format!("\n\nRespond in {}.", super::llm::language_name(lang)),
            _ => String::new(),
        },
    );

    serde_json::json!({
        "messages": [{ "role": "system", "content": system_prompt }],
        "temperature": 0.3,
        "max_tokens": max_tokens,
        "think": true
    })
}

/// Parse the conclusion prose and extract confidence from the last line.
pub fn parse_conclusion(content: &str) -> (String, f32) {
    let lines: Vec<&str> = content.trim().lines().collect();
    let mut confidence = 0.5f32;
    let mut conclusion_lines = lines.clone();

    // Look for "CONFIDENCE: X.XX" at the end
    for (i, line) in lines.iter().enumerate().rev() {
        let trimmed = line.trim().to_uppercase();
        if trimmed.starts_with("CONFIDENCE:") || trimmed.starts_with("CONFIDENCE =") {
            if let Some(val) = trimmed.split_whitespace().last()
                .and_then(|v| v.parse::<f32>().ok()) {
                confidence = val.clamp(0.0, 1.0);
                conclusion_lines = lines[..i].to_vec();
                break;
            }
        }
    }

    (conclusion_lines.join("\n").trim().to_string(), confidence)
}

// ── Pass 2: Structured fields (think=OFF, format=json) ─────────────────

/// Build a focused extraction prompt for a specific set of fields.
/// Each call targets one logical group with a simple JSON schema.
fn build_extraction_prompt(
    session: &DebateSession,
    conclusion: &str,
    _field_group: &str,
    schema: &str,
    max_tokens: u64,
) -> serde_json::Value {
    let context = build_condensed_context(session);

    let system_prompt = format!(
        r#"You are extracting structured data from a completed debate analysis.

Topic: "{topic}"
Conclusion: "{conclusion}"

Debate context:
{context}

Extract the following and return ONLY valid JSON matching this exact structure:
{schema}

Rules:
- Use the exact field names shown
- Every field must be present
- Arrays can be empty [] if nothing applies
- Be specific and factual, not vague"#,
        topic = session.topic,
        conclusion = &conclusion[..conclusion.len().min(1500)],
        context = context,
        schema = schema,
    );

    serde_json::json!({
        "messages": [{ "role": "system", "content": system_prompt }],
        "temperature": 0.1,
        "max_tokens": max_tokens,
        "think": false
    })
}

/// Build extraction prompt for evidence gaps + recommended investigations.
pub fn build_gaps_prompt(session: &DebateSession, conclusion: &str, max_tokens: u64) -> serde_json::Value {
    build_extraction_prompt(session, conclusion, "gaps", r#"{
  "evidence_gaps": ["Missing: [specific data] -- needed because [why]"],
  "recommended_investigations": ["Search for [topic] in [source] to determine [what]"]
}"#, max_tokens)
}

/// Build extraction prompt for influence map.
pub fn build_influence_prompt(session: &DebateSession, conclusion: &str, max_tokens: u64) -> serde_json::Value {
    build_extraction_prompt(session, conclusion, "influence", r#"{
  "influence_map": [{"agent_id": "agent-N", "agent_name": "...", "bias_label": "...", "position_pushed": "one sentence", "evidence_backed": true, "distortion_summary": "one sentence"}],
  "cherry_picks": [{"agent_id": "agent-N", "ignored_evidence": "...", "why_ignored": "..."}]
}"#, max_tokens)
}

/// Build extraction prompt for agreements, disagreements, tensions.
pub fn build_consensus_prompt(session: &DebateSession, conclusion: &str, max_tokens: u64) -> serde_json::Value {
    build_extraction_prompt(session, conclusion, "consensus", r#"{
  "areas_of_agreement": [{"statement": "...", "agents": ["agent-1","agent-2"], "confidence": 0.8}],
  "areas_of_disagreement": [{"statement": "...", "positions": [["agent-1", "their view"], ["agent-2", "their view"]]}],
  "key_tensions": ["tension 1", "tension 2"]
}"#, max_tokens)
}

/// Build extraction prompt for evolution + final positions.
pub fn build_evolution_prompt(session: &DebateSession, conclusion: &str, max_tokens: u64) -> serde_json::Value {
    // Pre-compute trajectories for the prompt
    let mut trajectories = String::new();
    for agent in &session.agents {
        let confs: Vec<f32> = session.rounds.iter().filter_map(|r| {
            r.turns.iter().find(|t| t.agent_id == agent.id).map(|t| t.confidence)
        }).collect();
        let evidence_count = session.rounds.last()
            .and_then(|r| r.turns.iter().find(|t| t.agent_id == agent.id))
            .map(|t| t.evidence.len()).unwrap_or(0);
        trajectories.push_str(&format!("  {} ({}): confidence={:?}, evidence_count={}\n",
            agent.name, agent.id, confs, evidence_count));
    }

    let system_prompt = format!(
        r#"You are extracting agent evolution data from a debate.

Topic: "{topic}"
Conclusion: "{conclusion}"

Agent trajectories:
{trajectories}

Return ONLY this JSON:
{{
  "evolution": [{{"agent_id": "agent-N", "agent_name": "...", "confidence_trajectory": [0.5, 0.6], "net_shift": 0.1, "pivot_cause": "why they changed", "flexibility_score": 0.7, "key_concessions": ["..."], "bias_override": false}}],
  "agent_positions": [{{"agent_id": "agent-N", "agent_name": "...", "final_position": "one sentence summary", "confidence": 0.8, "evidence_count": 5}}]
}}"#,
        topic = session.topic,
        conclusion = &conclusion[..conclusion.len().min(500)],
        trajectories = trajectories,
    );

    serde_json::json!({
        "messages": [{ "role": "system", "content": system_prompt }],
        "temperature": 0.1,
        "max_tokens": max_tokens,
        "think": false
    })
}
