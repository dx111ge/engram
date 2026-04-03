/// Unit tests for the multi-agent debate panel.

use engram_api::handlers::debate::*;

#[test]
fn agent_slot_assignment_diversity() {
    let agents = assign_agent_slots(5);
    assert_eq!(agents.len(), 5);

    // First and last should be neutral
    assert!(agents[0].bias.is_neutral);
    assert!(agents[4].bias.is_neutral);

    // Middle agents should not be neutral
    assert!(!agents[1].bias.is_neutral);
    assert!(!agents[2].bias.is_neutral);
    assert!(!agents[3].bias.is_neutral);

    // Rigor should spread from 0.0 to 1.0
    assert!((agents[0].rigor_level - 0.0).abs() < 0.01);
    assert!((agents[4].rigor_level - 1.0).abs() < 0.01);

    // All IDs should be unique
    let ids: std::collections::HashSet<_> = agents.iter().map(|a| &a.id).collect();
    assert_eq!(ids.len(), 5);
}

#[test]
fn agent_slot_min_max() {
    let min = assign_agent_slots(1);
    assert_eq!(min.len(), 2); // clamped to min 2

    let max = assign_agent_slots(10);
    assert_eq!(max.len(), 8); // clamped to max 8
}

#[test]
fn agent_slot_two_agents() {
    let agents = assign_agent_slots(2);
    assert_eq!(agents.len(), 2);
    assert!(agents[0].bias.is_neutral);
    // With only 2 agents, second should be biased (need at least 1 biased for a debate)
    assert!(!agents[1].bias.is_neutral);
}

#[test]
fn parse_turn_metadata_basic() {
    let text = "This is my position.\n\nCONFIDENCE: 0.75\nAGREES_WITH: [none]\nDISAGREES_WITH: [Agent A, Agent B]";
    let agents = vec![
        DebateAgent {
            id: "agent-1".into(), name: "Agent A".into(), persona_description: String::new(),
            rigor_level: 0.5, source_access: SourceAccess::GraphOnly, evidence_threshold: 0.3,
            cognitive_style: CognitiveStyle::Empirical,
            bias: AgentBias { label: String::new(), description: String::new(), is_neutral: true },
            icon: String::new(), color: String::new(),
        },
        DebateAgent {
            id: "agent-2".into(), name: "Agent B".into(), persona_description: String::new(),
            rigor_level: 0.5, source_access: SourceAccess::GraphOnly, evidence_threshold: 0.3,
            cognitive_style: CognitiveStyle::Skeptical,
            bias: AgentBias { label: String::new(), description: String::new(), is_neutral: true },
            icon: String::new(), color: String::new(),
        },
    ];
    let meta = parse_turn_metadata(text, &agents);
    assert!((meta.confidence - 0.75).abs() < 0.01);
    assert!(meta.agrees_with.is_empty());
    assert_eq!(meta.disagrees_with.len(), 2);
    assert!(meta.position_shift.is_empty());
    assert!(meta.concessions.is_empty());
}

#[test]
fn strip_metadata_lines_test() {
    let text = "My position is clear.\n\nI believe X.\n\nCONFIDENCE: 0.8\nAGREES_WITH: [none]\nDISAGREES_WITH: [none]";
    let stripped = strip_metadata_lines(text);
    assert!(stripped.contains("My position is clear"));
    assert!(!stripped.contains("CONFIDENCE"));
}

#[test]
fn parse_json_from_llm_direct() {
    let input = r#"[{"id": "agent-1", "name": "Test"}]"#;
    let result = parse_json_from_llm(input);
    assert!(result.is_ok());
}

#[test]
fn parse_json_from_llm_markdown() {
    let input = "Here is the result:\n```json\n[{\"id\": \"agent-1\"}]\n```\n";
    let result = parse_json_from_llm(input);
    assert!(result.is_ok());
}

#[test]
fn source_access_display() {
    assert_eq!(format!("{}", SourceAccess::GraphOnly), "Engram graph only");
    assert_eq!(format!("{}", SourceAccess::WebOnly), "External web sources only");
}

#[test]
fn cognitive_style_display() {
    assert!(format!("{}", CognitiveStyle::Skeptical).contains("demands proof"));
}

#[test]
fn tools_for_graph_only_agent() {
    let agent = DebateAgent {
        id: "a1".into(), name: "Test".into(), persona_description: String::new(),
        rigor_level: 0.5, source_access: SourceAccess::GraphOnly, evidence_threshold: 0.3,
        cognitive_style: CognitiveStyle::Empirical,
        bias: AgentBias { label: String::new(), description: String::new(), is_neutral: true },
        icon: String::new(), color: String::new(),
    };
    let tools = tools_for_agent(&agent);
    let arr = tools.as_array().unwrap();
    let names: Vec<&str> = arr.iter()
        .filter_map(|t| t.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()))
        .collect();
    assert!(!names.contains(&"engram_investigate"));
    assert!(names.contains(&"engram_search"));
}

#[test]
fn tools_for_web_agent() {
    let agent = DebateAgent {
        id: "a1".into(), name: "Test".into(), persona_description: String::new(),
        rigor_level: 0.5, source_access: SourceAccess::GraphAndWeb, evidence_threshold: 0.3,
        cognitive_style: CognitiveStyle::Empirical,
        bias: AgentBias { label: String::new(), description: String::new(), is_neutral: true },
        icon: String::new(), color: String::new(),
    };
    let tools = tools_for_agent(&agent);
    let arr = tools.as_array().unwrap();
    let names: Vec<&str> = arr.iter()
        .filter_map(|t| t.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()))
        .collect();
    assert!(names.contains(&"engram_investigate"));
}

#[test]
fn debate_status_serde() {
    let s = DebateStatus::AwaitingStart;
    let json = serde_json::to_string(&s).unwrap();
    assert_eq!(json, "\"awaiting_start\"");
    let back: DebateStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(back, DebateStatus::AwaitingStart);
}

#[test]
fn evidence_threshold_scales_with_rigor() {
    let agents = assign_agent_slots(5);
    assert!(agents[0].evidence_threshold < agents[4].evidence_threshold);
}

#[test]
fn agent_bias_auto_assignment() {
    let agents = assign_agent_slots(6);
    // Count neutral vs biased
    let neutral_count = agents.iter().filter(|a| a.bias.is_neutral).count();
    let biased_count = agents.iter().filter(|a| !a.bias.is_neutral).count();
    assert!(neutral_count >= 2, "should have at least 2 neutral agents");
    assert!(biased_count >= 3, "should have at least 3 biased agents");
}

#[test]
fn source_access_diversity() {
    let agents = assign_agent_slots(5);
    let sources: std::collections::HashSet<String> = agents.iter()
        .map(|a| format!("{}", a.source_access))
        .collect();
    // With 5 agents, should have at least 3 different source access types
    assert!(sources.len() >= 3, "should have diverse source access, got {:?}", sources);
}

#[test]
fn cognitive_style_diversity() {
    let agents = assign_agent_slots(7);
    let styles: std::collections::HashSet<String> = agents.iter()
        .map(|a| format!("{}", a.cognitive_style))
        .collect();
    assert!(styles.len() >= 5, "should have diverse cognitive styles, got {:?}", styles);
}

#[test]
fn parse_turn_confidence_clamp() {
    let text = "Position.\nCONFIDENCE: 1.5\nAGREES_WITH: [none]\nDISAGREES_WITH: [none]";
    let meta = parse_turn_metadata(text, &[]);
    assert!(meta.confidence <= 1.0, "confidence should be clamped to 1.0");
}

#[test]
fn parse_turn_no_metadata() {
    let text = "Just a position with no metadata lines at all.";
    let meta = parse_turn_metadata(text, &[]);
    assert!((meta.confidence - 0.5).abs() < 0.01, "default confidence should be 0.5");
    assert!(meta.agrees_with.is_empty());
    assert!(meta.disagrees_with.is_empty());
}

#[test]
fn parse_turn_with_evolution_metadata() {
    let text = "My revised position.\n\nCONFIDENCE: 0.60\nAGREES_WITH: [Agent A]\nDISAGREES_WITH: [none]\nPOSITION_SHIFT: [I now acknowledge the economic argument has merit]\nCONCESSIONS: [Economic impact is valid, Job losses are real]";
    let agents = vec![
        DebateAgent {
            id: "agent-1".into(), name: "Agent A".into(), persona_description: String::new(),
            rigor_level: 0.5, source_access: SourceAccess::GraphOnly, evidence_threshold: 0.3,
            cognitive_style: CognitiveStyle::Empirical,
            bias: AgentBias { label: String::new(), description: String::new(), is_neutral: true },
            icon: String::new(), color: String::new(),
        },
    ];
    let meta = parse_turn_metadata(text, &agents);
    assert!((meta.confidence - 0.60).abs() < 0.01);
    assert_eq!(meta.agrees_with.len(), 1);
    assert!(!meta.position_shift.is_empty());
    assert!(meta.position_shift.contains("economic"));
    assert_eq!(meta.concessions.len(), 2);
}

#[test]
fn strip_metadata_includes_evolution_lines() {
    let text = "Position text.\n\nCONFIDENCE: 0.8\nAGREES_WITH: [none]\nDISAGREES_WITH: [none]\nPOSITION_SHIFT: [shifted]\nCONCESSIONS: [point A]";
    let stripped = strip_metadata_lines(text);
    assert!(stripped.contains("Position text"));
    assert!(!stripped.contains("POSITION_SHIFT"));
    assert!(!stripped.contains("CONCESSIONS"));
}
