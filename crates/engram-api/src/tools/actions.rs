/// Tool definitions: assessment, reasoning, actions, reporting, and source management.

use serde_json::Value;

pub fn assessment_tools() -> Vec<Value> {
    vec![
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_assess_create",
                "description": "Create a new assessment (hypothesis tracking) with watched entities and initial probability. WRITE operation.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "Hypothesis title" },
                        "category": { "type": "string", "description": "Category: financial, geopolitical, technical, military, social, other" },
                        "timeframe": { "type": "string", "description": "Time horizon" },
                        "probability": { "type": "number", "description": "Starting probability (0.05-0.95)" },
                        "watches": { "type": "array", "items": { "type": "string" }, "description": "Entities to watch" }
                    },
                    "required": ["title"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_assess_query",
                "description": "Get full assessment detail: probability, history, evidence, watched entities",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "label": { "type": "string", "description": "Assessment label" }
                    },
                    "required": ["label"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_assess_list",
                "description": "List all assessments with probability, evidence count, and status",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "category": { "type": "string", "description": "Filter by category" },
                        "status": { "type": "string", "description": "Filter by status" }
                    }
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_assess_evidence",
                "description": "Add evidence to an assessment (supports or contradicts). WRITE operation.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "assessment": { "type": "string", "description": "Assessment label" },
                        "entity": { "type": "string", "description": "Evidence entity label" },
                        "direction": { "type": "string", "description": "'supports' or 'contradicts'" }
                    },
                    "required": ["assessment", "entity", "direction"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_assess_evaluate",
                "description": "Re-evaluate an assessment's probability based on current evidence",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "label": { "type": "string", "description": "Assessment label" }
                    },
                    "required": ["label"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_assess_watch",
                "description": "Add a watched entity to an assessment for automatic re-evaluation",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "label": { "type": "string", "description": "Assessment label" },
                        "entity_label": { "type": "string", "description": "Entity to watch" }
                    },
                    "required": ["label"]
                }
            }
        }),
    ]
}

pub fn reasoning_tools() -> Vec<Value> {
    vec![
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_what_if",
                "description": "Simulate: if an entity's confidence changes, what cascades through the graph? Shows affected entities and assessments.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "entity": { "type": "string", "description": "Entity to simulate change on" },
                        "new_confidence": { "type": "number", "description": "Hypothetical new confidence (0.0-1.0)" },
                        "depth": { "type": "integer", "description": "How many hops to trace the cascade (default: 2)" }
                    },
                    "required": ["entity", "new_confidence"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_influence_path",
                "description": "Find how entity A could affect entity B through the graph. Traces indirect connections and influence chains.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "from": { "type": "string", "description": "Source entity" },
                        "to": { "type": "string", "description": "Target entity" },
                        "max_depth": { "type": "integer", "description": "Maximum path length (default: 5)" }
                    },
                    "required": ["from", "to"]
                }
            }
        }),
    ]
}

pub fn action_tools() -> Vec<Value> {
    vec![
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_rule_create",
                "description": "Create an action rule (trigger + condition + action). WRITE operation.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Rule name" },
                        "trigger": { "type": "string", "description": "Event that triggers the rule" },
                        "conditions": { "type": "string", "description": "Conditions for rule to fire" },
                        "actions": { "type": "string", "description": "Actions to take when rule fires" }
                    },
                    "required": ["name", "trigger", "actions"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_rule_list",
                "description": "List active action rules with their triggers and conditions",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "filter": { "type": "string", "description": "Filter rules by name or trigger" }
                    }
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_run_inference",
                "description": "Trigger the inference engine now. Evaluates all active rules against current graph state.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "scope": { "type": "string", "description": "Limit inference to specific entity or rule" }
                    }
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_schedule",
                "description": "Create or list scheduled monitoring tasks for entities. Stores schedule metadata as entity properties.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "description": "Action: 'create' or 'list'" },
                        "entity": { "type": "string", "description": "Entity to schedule monitoring for (required for create)" },
                        "interval": { "type": "string", "description": "Check interval: hourly, daily, weekly (default: daily)" }
                    },
                    "required": ["action"]
                }
            }
        }),
    ]
}

pub fn reporting_tools() -> Vec<Value> {
    vec![
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_briefing",
                "description": "Generate a structured briefing on a topic: key entities, relationships, confidence levels, gaps, and temporal context",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "topic": { "type": "string", "description": "Topic or entity to brief on" },
                        "depth": { "type": "string", "description": "Briefing depth: shallow, standard, or deep (default: standard)" },
                        "format": { "type": "string", "description": "Output format: text or json (default: text)" }
                    },
                    "required": ["topic"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_export_subgraph",
                "description": "Export an entity and its N-hop neighborhood as structured data",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "entity": { "type": "string", "description": "Central entity" },
                        "depth": { "type": "integer", "description": "Number of hops (default: 2)" },
                        "format": { "type": "string", "description": "Output format: json-ld or csv (default: json-ld)" }
                    },
                    "required": ["entity"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_entity_timeline",
                "description": "Generate a chronological narrative of an entity's history based on temporal edges",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "entity": { "type": "string", "description": "Entity to narrate" },
                        "from_date": { "type": "string", "description": "Start date (YYYY-MM-DD)" },
                        "to_date": { "type": "string", "description": "End date (YYYY-MM-DD)" }
                    },
                    "required": ["entity"]
                }
            }
        }),
    ]
}

pub fn source_tools() -> Vec<Value> {
    vec![
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_sources_list",
                "description": "List all configured data sources with health status",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "status": { "type": "string", "description": "Filter by status: active, paused, error" }
                    }
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_source_trigger",
                "description": "Trigger an immediate fetch from a configured source",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "source_name": { "type": "string", "description": "Name of the source to trigger" }
                    },
                    "required": ["source_name"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_source_coverage",
                "description": "Analyze what topics/entities a data source covers based on provenance tracking in the graph",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "source_name": { "type": "string", "description": "Source name to analyze coverage for" }
                    },
                    "required": ["source_name"]
                }
            }
        }),
    ]
}
