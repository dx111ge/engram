/// LLM tool definitions — OpenAI-compatible function/tool calling interface.
///
/// GET /tools returns tool definitions that any LLM can use to interact
/// with the engram knowledge graph via function calling.

use serde_json::Value;

pub fn tool_definitions() -> Value {
    serde_json::json!({
        "tools": [
            {
                "type": "function",
                "function": {
                    "name": "engram_store",
                    "description": "Store a new fact or entity in the knowledge graph",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "entity": { "type": "string", "description": "Name/label of the entity" },
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
            },
            {
                "type": "function",
                "function": {
                    "name": "engram_relate",
                    "description": "Create a relationship between two entities",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "from": { "type": "string", "description": "Source entity" },
                            "to": { "type": "string", "description": "Target entity" },
                            "relationship": { "type": "string", "description": "Type of relationship (causes, is_a, part_of, ...)" },
                            "confidence": { "type": "number", "description": "Relationship confidence" }
                        },
                        "required": ["from", "to", "relationship"]
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "engram_query",
                    "description": "Query the knowledge graph with traversal from a starting entity",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "start": { "type": "string", "description": "Starting entity" },
                            "depth": { "type": "integer", "description": "Max traversal depth (default: 2)" },
                            "min_confidence": { "type": "number", "description": "Minimum confidence threshold" },
                            "direction": { "type": "string", "description": "Traversal direction: out, in, or both (default: both)" }
                        },
                        "required": ["start"]
                    }
                }
            },
            {
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
            },
            {
                "type": "function",
                "function": {
                    "name": "engram_similar",
                    "description": "Semantic similarity search — find entities related to a concept",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "text": { "type": "string", "description": "Text to find similar entities for" },
                            "limit": { "type": "integer", "description": "Max results (default: 10)" }
                        },
                        "required": ["text"]
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "engram_prove",
                    "description": "Find evidence for or against a relationship between two entities",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "from": { "type": "string", "description": "Source entity" },
                            "relationship": { "type": "string", "description": "Relationship to prove" },
                            "to": { "type": "string", "description": "Target entity" }
                        },
                        "required": ["from", "relationship", "to"]
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "engram_explain",
                    "description": "Explain how a fact was derived, its confidence, edges, and provenance",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "entity": { "type": "string", "description": "Entity to explain" }
                        },
                        "required": ["entity"]
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "engram_reinforce",
                    "description": "Increase confidence of a fact through access or confirmation",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "entity": { "type": "string", "description": "Entity to reinforce" },
                            "source": { "type": "string", "description": "Confirmation source (omit for access-only boost)" }
                        },
                        "required": ["entity"]
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "engram_correct",
                    "description": "Mark a fact as wrong — zeroes confidence and propagates distrust",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "entity": { "type": "string", "description": "Entity to correct" },
                            "reason": { "type": "string", "description": "Why this fact is wrong" }
                        },
                        "required": ["entity", "reason"]
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "engram_delete",
                    "description": "Soft-delete an entity (confidence to 0, provenance recorded)",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "entity": { "type": "string", "description": "Entity to delete" }
                        },
                        "required": ["entity"]
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "engram_gaps",
                    "description": "List knowledge gaps (black areas) ranked by severity. Detects frontier nodes, structural holes, temporal gaps, confidence deserts, and coordinated clusters.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "min_severity": { "type": "number", "description": "Minimum severity to include (0.0-1.0, default: 0.3)" },
                            "limit": { "type": "integer", "description": "Max results (default: 20)" }
                        }
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "engram_sources",
                    "description": "List configured data sources with health status and usage statistics",
                    "parameters": {
                        "type": "object",
                        "properties": {}
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "engram_frontier",
                    "description": "List frontier nodes — entities at the edge of knowledge with few connections",
                    "parameters": {
                        "type": "object",
                        "properties": {}
                    }
                }
            }
        ]
    })
}
