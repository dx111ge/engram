/// LLM tool definitions -- OpenAI-compatible function/tool calling interface.
///
/// GET /tools returns tool definitions that any LLM can use to interact
/// with the engram knowledge graph via function calling.
///
/// Split into sub-modules by category:
/// - `knowledge` -- core CRUD, search, explain, gaps
/// - `analysis` -- temporal, compare, analytics, investigation
/// - `actions` -- assessment, reasoning, action engine, reporting, sources

pub mod knowledge;
pub mod analysis;
pub mod actions;

use serde_json::Value;

pub fn tool_definitions() -> Value {
    let mut tools = Vec::new();

    // Core knowledge tools
    tools.extend(knowledge::knowledge_tools());

    // Temporal query tools
    tools.extend(analysis::temporal_tools());

    // Compare & analytics tools
    tools.extend(analysis::compare_tools());

    // Investigation tools
    tools.extend(analysis::investigation_tools());

    // Assessment tools
    tools.extend(actions::assessment_tools());

    // Reasoning tools
    tools.extend(actions::reasoning_tools());

    // Action engine tools
    tools.extend(actions::action_tools());

    // Reporting tools
    tools.extend(actions::reporting_tools());

    // Source management tools
    tools.extend(actions::source_tools());

    serde_json::json!({ "tools": tools })
}
