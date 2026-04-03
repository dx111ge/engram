/// Synthesis prompt builder for the multi-agent debate panel.
/// Generates the 4-layer analysis: Evidence, Influence, Hidden Agendas, Evolution.

use super::types::*;

/// Build the synthesis prompt with full debate transcript.
pub fn build_synthesis_prompt(session: &DebateSession) -> serde_json::Value {
    let mut transcript = String::new();
    for round in &session.rounds {
        transcript.push_str(&format!("\n=== Round {} ===\n", round.round_number + 1));
        if let Some(ref inj) = round.user_injection {
            transcript.push_str(&format!("Moderator asked: \"{}\"\n\n", inj));
        }
        // Include gap research results if any
        if !round.gap_research.is_empty() {
            transcript.push_str("Gap-closing research conducted:\n");
            for gr in &round.gap_research {
                transcript.push_str(&format!("  Investigated: \"{}\"\n", gr.gap_query));
                for f in &gr.findings {
                    transcript.push_str(&format!("    {}\n", f));
                }
                if gr.ingested {
                    transcript.push_str(&format!("    [Ingested: {} facts, {} relations]\n", gr.facts_stored, gr.relations_created));
                }
            }
            transcript.push('\n');
        }
        for turn in &round.turns {
            let agent = session.agents.iter().find(|a| a.id == turn.agent_id);
            let name = agent.map(|a| a.name.as_str()).unwrap_or(&turn.agent_id);
            let bias_info = agent.map(|a| {
                if a.bias.is_neutral { "NEUTRAL".to_string() }
                else { format!("BIAS: {} -- {}", a.bias.label, a.bias.description) }
            }).unwrap_or_default();
            let shift_info = if turn.position_shift.is_empty() { String::new() }
                else { format!("\nPOSITION SHIFT: {}", turn.position_shift) };
            let concession_info = if turn.concessions.is_empty() { String::new() }
                else { format!("\nCONCESSIONS: {}", turn.concessions.join(", ")) };
            transcript.push_str(&format!(
                "{} [{}] (confidence: {:.2}):\n{}{}{}\n\nEvidence cited: {} items | Tools used: {}\n\n",
                name, bias_info, turn.confidence, turn.position,
                shift_info, concession_info,
                turn.evidence.len(), turn.tools_used.len()
            ));
        }
    }

    let agent_profiles: Vec<String> = session.agents.iter().map(|a| {
        format!("- {} ({}): rigor={:.1}, source={}, bias={}",
            a.name, a.id, a.rigor_level, a.source_access,
            if a.bias.is_neutral { "NEUTRAL".to_string() } else { a.bias.label.clone() }
        )
    }).collect();

    let mut system_prompt = format!(
        r#"You are a senior intelligence synthesis analyst. You must produce a structured 4-layer analysis of the following debate.

Topic: "{topic}"
Rounds completed: {rounds}

Agent profiles:
{profiles}

Full debate transcript:
{transcript}

The original question/thesis being debated is: "{topic}"

Produce your analysis in EXACTLY this JSON structure (no markdown, no commentary):
{{
  "evidence_conclusion": "MUST DIRECTLY ANSWER THE QUESTION. Start with: 'Based on the evidence, [concrete answer]. ...' Commit to a probabilistic assessment with probability ranges.",
  "conclusion_confidence": 0.XX,
  "evidence_gaps": ["SPECIFIC gaps: 'Missing: [fact/data] -- needed because [why]'. Ingestible as search targets."],
  "key_evidence": [{{"entity": "...", "fact": "...", "confidence": 0.XX}}],

  "influence_map": [{{"agent_id": "...", "agent_name": "...", "bias_label": "...", "position_pushed": "...", "evidence_backed": true/false, "distortion_summary": "..."}}],
  "unexpected_alignments": [{{"agents": ["id1","id2"], "common_position": "...", "reason": "..."}}],
  "cherry_picks": [{{"agent_id": "...", "ignored_evidence": "...", "why_ignored": "..."}}],

  "hidden_agendas": [{{"agent_id": "...", "agent_name": "...", "stated_position": "...", "underlying_interest": "...", "who_benefits": "...", "what_they_avoid": "...", "what_they_lose": "...", "second_order_effects": ["..."]}}],
  "beneficiary_map": [{{"position": "...", "beneficiaries": ["..."], "mechanism": "..."}}],
  "parallel_interests": [{{"agents": ["id1","id2"], "surface_disagreement": "...", "hidden_alignment": "..."}}],
  "blind_spots": [{{"agent_id": "...", "topic_avoided": "...", "likely_reason": "..."}}],

  "areas_of_agreement": [{{"statement": "...", "agents": ["id1","id2"], "confidence": 0.XX}}],
  "areas_of_disagreement": [{{"statement": "...", "positions": [["agent_id", "their view"]]}}],
  "key_tensions": ["..."],
  "recommended_investigations": ["Actionable: 'Search for [topic] in [source] to determine [what]'"],
  "evolution": [{{"agent_id": "...", "agent_name": "...", "confidence_trajectory": [0.XX, 0.YY], "net_shift": 0.XX, "pivot_cause": "...", "flexibility_score": 0.XX, "key_concessions": ["..."], "bias_override": false}}],

  "agent_positions": [{{"agent_id": "...", "agent_name": "...", "final_position": "...", "confidence": 0.XX, "evidence_count": N}}]
}}

CRITICAL INSTRUCTIONS:
- evidence_conclusion MUST DIRECTLY ANSWER "{topic}". Commit to a most-likely outcome with probability range.
- evidence_gaps: SPECIFIC data points missing. "Missing: Iran daily oil exports in barrels" not "more research".
- recommended_investigations: Actionable search queries, not platitudes.
- Layer 2: How did biased agents distort? Which biased arguments were actually valid?
- Layer 3: Cui bono -- who really benefits? What are they hiding?
- Layer 4: confidence_trajectory per round, net_shift, flexibility_score (+adapted/-dug in), bias_override."#,
        topic = session.topic,
        rounds = session.rounds.len(),
        profiles = agent_profiles.join("\n"),
        transcript = transcript,
    );

    // Add mode-specific synthesis instructions
    let mode_additions = super::modes::synthesis_additions(&session.mode, session.mode_input.as_deref());
    if !mode_additions.is_empty() {
        system_prompt.push_str(&mode_additions);
    }

    serde_json::json!({
        "messages": [
            { "role": "system", "content": system_prompt }
        ],
        "temperature": 0.3,
        "max_tokens": 4096
    })
}
