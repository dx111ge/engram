/// Tool definitions: temporal queries, comparison, analytics, and investigation.

use serde_json::Value;

pub fn temporal_tools() -> Vec<Value> {
    vec![
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_temporal_query",
                "description": "Query edges for an entity within a time range. Returns only edges valid during the specified period.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "entity": { "type": "string", "description": "Entity to query temporal edges for -- use EXACT name as it appears in the graph (copy from query/search results)" },
                        "from_date": { "type": "string", "description": "Start date (YYYY-MM-DD)" },
                        "to_date": { "type": "string", "description": "End date (YYYY-MM-DD)" },
                        "relationship": { "type": "string", "description": "Filter to specific relationship type" }
                    },
                    "required": ["entity"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_timeline",
                "description": "Get chronological events/edges for an entity, ordered by temporal bounds",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "entity": { "type": "string", "description": "Entity to build timeline for -- use EXACT name as it appears in the graph (copy from query/search results)" },
                        "limit": { "type": "integer", "description": "Max events to return (default: 20)" }
                    },
                    "required": ["entity"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_current_state",
                "description": "Get only current (non-expired) relations for an entity. Filters out edges with valid_to in the past.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "entity": { "type": "string", "description": "Entity to get current state for -- use EXACT name as it appears in the graph (copy from query/search results)" },
                        "depth": { "type": "integer", "description": "Traversal depth (default: 1)" }
                    },
                    "required": ["entity"]
                }
            }
        }),
    ]
}

pub fn compare_tools() -> Vec<Value> {
    vec![
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_compare",
                "description": "Side-by-side comparison of two entities: shared edges, unique edges, common neighbors, property differences",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "entity_a": { "type": "string", "description": "First entity to compare -- use EXACT name as it appears in the graph (copy from query/search results)" },
                        "entity_b": { "type": "string", "description": "Second entity to compare -- use EXACT name as it appears in the graph (copy from query/search results)" },
                        "aspects": { "type": "array", "items": { "type": "string" }, "description": "Specific aspects to compare" }
                    },
                    "required": ["entity_a", "entity_b"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_shortest_path",
                "description": "Find the shortest path between two entities in the knowledge graph",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "from": { "type": "string", "description": "Source entity -- must be the EXACT name as it appears in the graph (copy from query/search results, do not modify casing or formatting)" },
                        "to": { "type": "string", "description": "Target entity -- must be the EXACT name as it appears in the graph (copy from query/search results, do not modify casing or formatting)" },
                        "max_depth": { "type": "integer", "description": "Maximum path length (default: 6)" }
                    },
                    "required": ["from", "to"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_most_connected",
                "description": "Find the top-N most connected entities by edge count",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "description": "Number of results (default: 10)" },
                        "node_type": { "type": "string", "description": "Filter to specific entity type" }
                    }
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_isolated",
                "description": "Find isolated nodes with few or no connections",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "max_edges": { "type": "integer", "description": "Maximum edge count to qualify as isolated (default: 1)" },
                        "node_type": { "type": "string", "description": "Filter to specific entity type" }
                    }
                }
            }
        }),
    ]
}

pub fn investigation_tools() -> Vec<Value> {
    vec![
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_ingest_text",
                "description": "Run the full NER+RE pipeline on text and store extracted entities/relations. WRITE operation.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "text": { "type": "string", "description": "Text to analyze and ingest" },
                        "source": { "type": "string", "description": "Source attribution for provenance" }
                    },
                    "required": ["text"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_analyze_relations",
                "description": "Extract entities and relations from text WITHOUT storing (dry-run preview)",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "text": { "type": "string", "description": "Text to analyze" }
                    },
                    "required": ["text"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_changes",
                "description": "What changed in the graph since a given timestamp. Shows recently stored/updated entities.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "since": { "type": "string", "description": "ISO date (YYYY-MM-DD) to look back from" },
                        "entity": { "type": "string", "description": "Filter to changes affecting a specific entity -- use EXACT name as it appears in the graph" }
                    },
                    "required": ["since"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_investigate",
                "description": "Investigate an entity: web search for latest information, then run NER+RE pipeline to extract and store new facts. WRITE operation.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "entity": { "type": "string", "description": "Entity to investigate -- use EXACT name as it appears in the graph (copy from query/search results)" },
                        "depth": { "type": "string", "description": "Investigation depth: shallow (1 search) or deep (3 searches with follow-ups). Default: shallow" }
                    },
                    "required": ["entity"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_watch",
                "description": "Mark an entity as watched for change monitoring. Sets a _watched property on the entity.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "entity": { "type": "string", "description": "Entity to watch -- use EXACT name as it appears in the graph (copy from query/search results)" }
                    },
                    "required": ["entity"]
                }
            }
        }),
    ]
}
