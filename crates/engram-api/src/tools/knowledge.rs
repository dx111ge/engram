/// Tool definitions: core knowledge graph operations (store, query, search, etc.)

use serde_json::Value;

pub fn knowledge_tools() -> Vec<Value> {
    vec![
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_store",
                "description": "Store a new fact or entity in the knowledge graph. WRITE operation -- requires user confirmation.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "entity": { "type": "string", "description": "Name/label of the entity -- use EXACT spelling as it appears in prior tool results when referencing existing entities" },
                        "type": { "type": "string", "description": "Entity type (person, server, concept, event, ...)" },
                        "properties": {
                            "type": "object",
                            "description": "Key-value properties",
                            "additionalProperties": { "type": "string" }
                        },
                        "source": { "type": "string", "description": "Where this knowledge comes from" },
                        "confidence": { "type": "number", "description": "How certain (0.0-1.0), default based on source" }
                    },
                    "required": ["entity"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_relate",
                "description": "Create a relationship between two entities. Supports temporal bounds (valid_from/valid_to). WRITE operation.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "from": { "type": "string", "description": "Source entity label -- must be the EXACT name as it appears in the graph (copy from query/search results, do not modify casing or formatting)" },
                        "to": { "type": "string", "description": "Target entity label -- must be the EXACT name as it appears in the graph (copy from query/search results, do not modify casing or formatting)" },
                        "relationship": { "type": "string", "description": "Type of relationship (causes, is_a, part_of, ...)" },
                        "confidence": { "type": "number", "description": "Relationship confidence" },
                        "valid_from": { "type": "string", "description": "Date when relationship became valid (YYYY-MM-DD)" },
                        "valid_to": { "type": "string", "description": "Date when relationship ceased (YYYY-MM-DD, omit if still current)" }
                    },
                    "required": ["from", "to", "relationship"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_query",
                "description": "Query the knowledge graph with traversal from a starting entity. Returns nodes, edges, and temporal bounds.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "start": { "type": "string", "description": "Starting entity -- use EXACT name as it appears in the graph (copy from query/search results)" },
                        "depth": { "type": "integer", "description": "Max traversal depth (default: 2)" },
                        "min_confidence": { "type": "number", "description": "Minimum confidence threshold" },
                        "direction": { "type": "string", "description": "Traversal direction: out, in, or both (default: both)" }
                    },
                    "required": ["start"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_search",
                "description": "Full-text keyword search across all stored knowledge",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query text" },
                        "limit": { "type": "integer", "description": "Max results to return (default: 10)" }
                    },
                    "required": ["query"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_similar",
                "description": "Semantic similarity search -- find entities related to a concept",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "text": { "type": "string", "description": "Text to find similar entities for" },
                        "limit": { "type": "integer", "description": "Max results (default: 10)" }
                    },
                    "required": ["text"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_prove",
                "description": "Find evidence for or against a relationship between two entities",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "from": { "type": "string", "description": "Source entity -- must be the EXACT name as it appears in the graph (copy from query/search results, do not modify casing or formatting)" },
                        "relationship": { "type": "string", "description": "Relationship to prove" },
                        "to": { "type": "string", "description": "Target entity -- must be the EXACT name as it appears in the graph (copy from query/search results, do not modify casing or formatting)" }
                    },
                    "required": ["from", "relationship", "to"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_explain",
                "description": "Explain how a fact was derived: confidence, edges (with temporal bounds), properties, and provenance",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "entity": { "type": "string", "description": "Entity to explain -- use EXACT name as it appears in the graph (copy from query/search results)" }
                    },
                    "required": ["entity"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_reinforce",
                "description": "Increase confidence of a fact through access or confirmation. WRITE operation.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "entity": { "type": "string", "description": "Entity to reinforce -- use EXACT name as it appears in the graph (copy from query/search results)" },
                        "source": { "type": "string", "description": "Confirmation source (omit for access-only boost)" }
                    },
                    "required": ["entity"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_correct",
                "description": "Mark a fact as wrong -- zeroes confidence and propagates distrust. WRITE operation.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "entity": { "type": "string", "description": "Entity to correct -- use EXACT name as it appears in the graph (copy from query/search results)" },
                        "reason": { "type": "string", "description": "Why this fact is wrong" }
                    },
                    "required": ["entity", "reason"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_delete",
                "description": "Soft-delete an entity (confidence to 0, provenance recorded). WRITE operation.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "entity": { "type": "string", "description": "Entity to delete -- use EXACT name as it appears in the graph (copy from query/search results)" }
                    },
                    "required": ["entity"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_gaps",
                "description": "List knowledge gaps (black areas) ranked by severity. Supports domain/topic filtering.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "min_severity": { "type": "number", "description": "Minimum severity (0.0-1.0, default: 0.3)" },
                        "limit": { "type": "integer", "description": "Max results (default: 20)" },
                        "domain": { "type": "string", "description": "Filter to a specific domain or topic" }
                    }
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_sources",
                "description": "List configured data sources with health status and usage statistics",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "engram_frontier",
                "description": "List frontier nodes -- entities at the edge of knowledge with few connections",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
    ]
}
