# Engram MCP Server

Model Context Protocol (MCP) server for native integration with Claude, Cursor, Windsurf, and any MCP-compatible AI tool.

MCP is JSON-RPC 2.0 over stdio -- a thin wrapper over the engram graph API.

## Installation

Download the latest binary from the [Releases](https://github.com/dx111ge/engram/releases) page.

## Starting the MCP Server

```bash
# Default: uses ./knowledge.brain
engram mcp

# Custom brain file path
engram mcp /path/to/my.brain
```

The server reads JSON-RPC requests from stdin and writes responses to stdout, one JSON object per line.

## Configuration

### Claude Code

Add to your project's `.mcp.json` or global MCP settings:

```json
{
  "mcpServers": {
    "engram": {
      "command": "engram",
      "args": ["mcp", "/path/to/knowledge.brain"]
    }
  }
}
```

### Cursor / Windsurf

Add to your MCP server configuration:

```json
{
  "engram": {
    "command": "engram",
    "args": ["mcp", "/path/to/knowledge.brain"],
    "transport": "stdio"
  }
}
```

### Claude Desktop

Add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "engram": {
      "command": "/path/to/engram",
      "args": ["mcp", "/path/to/knowledge.brain"]
    }
  }
}
```

## Available Tools

### engram_store

Store a new fact or entity in the knowledge graph.

Parameters:
- `entity` (required) — name/label of the entity
- `type` — entity type (person, server, concept, event, ...)
- `properties` — key-value properties object
- `source` — where this knowledge comes from
- `confidence` — how certain (0.0-1.0)

Example call:
```json
{
  "name": "engram_store",
  "arguments": {
    "entity": "postgresql",
    "type": "database",
    "properties": {"version": "16"},
    "source": "sysadmin"
  }
}
```

### engram_relate

Create a relationship between two entities.

Parameters:
- `from` (required) — source entity
- `to` (required) — target entity
- `relationship` (required) — type of relationship
- `confidence` — relationship confidence

### engram_query

Query the knowledge graph with traversal from a starting entity.

Parameters:
- `start` (required) — starting entity
- `depth` — max traversal depth (default: 2)
- `min_confidence` — minimum confidence threshold

### engram_search

Full-text keyword search across all stored knowledge.

Parameters:
- `query` (required) — search query text
- `limit` — max results (default: 10)

### engram_prove

Find evidence for or against a relationship between two entities using backward chaining.

Parameters:
- `from` (required) — source entity
- `relationship` (required) — relationship to prove
- `to` (required) — target entity

Returns:
```json
{
  "supported": true,
  "confidence": 0.72,
  "chain": [
    {"fact": "A -[is_a]-> B", "confidence": 0.9, "depth": 0},
    {"fact": "B -[is_a]-> C", "confidence": 0.8, "depth": 1}
  ]
}
```

### engram_explain

Explain how a fact was derived, its confidence, edges, and co-occurrences.

Parameters:
- `entity` (required) — entity to explain

### engram_gaps

Detect knowledge gaps in the graph: frontier nodes, structural holes, temporal gaps, and confidence deserts.

Parameters:
- `min_severity` — minimum severity score to include (default: 0.3)
- `limit` — max gaps to return (default: 10)

Returns a ranked list of detected gaps with severity scores and suggested actions.

### engram_frontier

Find frontier nodes at the edges of the knowledge graph that have few connections.

Parameters:
- `max_edges` — maximum edge count to qualify as frontier (default: 2)
- `limit` — max results (default: 20)

### engram_mesh_discover

Discover mesh peers that have knowledge about a given topic.

Parameters:
- `topic` (required) — topic to search for across peers

Returns matching peers with their trust scores and topic relevance.

### engram_mesh_query

Query knowledge across all trusted mesh peers without copying facts locally.

Parameters:
- `query` (required) — search query
- `max_results` — max results per peer (default: 10)
- `min_confidence` — minimum confidence threshold

### engram_ingest

Ingest text through the NER pipeline: entity extraction, resolution, deduplication, and storage.

Parameters:
- `text` (required) — text to process
- `source` — source identifier for provenance

### engram_create_rule

Create an event-driven action rule that triggers on graph changes.

Parameters:
- `name` (required) — rule name
- `trigger` (required) — event type to trigger on (store, relate, correct, etc.)
- `condition` — optional condition expression
- `effect` (required) — effect type (webhook, edge_create, confidence_cascade, etc.)
- `config` — effect-specific configuration

### engram_provenance

Get the full provenance chain for an entity (document -> fact -> entity path).

Parameters:
- `entity` (required) -- entity label
- `depth` -- max chain depth (default: 3)

### engram_documents

Query documents in the knowledge base.

Parameters:
- `query` -- search query (optional, returns all if empty)
- `limit` -- max results (default: 20)

### engram_assess_create

Create a new intelligence assessment with structured evidence.

Parameters:
- `title` (required) -- assessment title
- `hypothesis` (required) -- hypothesis to evaluate
- `initial_probability` -- starting probability (0.0-1.0, default: 0.5)

### engram_assess_list

List all assessments with current probabilities.

Parameters:
- `status` -- filter by status (active, stale, archived)

### engram_assess_get

Get a specific assessment with full details.

Parameters:
- `label` (required) -- assessment label

### engram_assess_evaluate

Re-evaluate an assessment based on current evidence.

Parameters:
- `label` (required) -- assessment label

### engram_assess_evidence

Add evidence to an assessment (updates Bayesian probability).

Parameters:
- `label` (required) -- assessment label
- `entity` (required) -- entity providing evidence
- `direction` -- supports or undermines (default: supports)
- `weight` -- evidence weight (0.0-1.0, default: 0.5)

### engram_assess_watch

Add a watch entity to an assessment (triggers stale alerts when the entity changes).

Parameters:
- `label` (required) -- assessment label
- `entity` (required) -- entity to watch

### engram_analyze_relations

Run NER/RE analysis on text and return extracted entities and relations without storing them.

Parameters:
- `text` (required) -- text to analyze

### engram_kge_train

Train knowledge graph embeddings on the current graph state.

Parameters:
- `epochs` -- training epochs (default: 100)

### engram_kge_predict

Predict missing links using trained knowledge graph embeddings.

Parameters:
- `entity` (required) -- entity to predict links for
- `limit` -- max predictions (default: 10)

---

## Available Resources

### engram://stats

Graph statistics (node and edge counts).

### engram://health

Server health status.

## Protocol Details

The MCP server implements the `2024-11-05` protocol version.

### Supported Methods

| Method | Description |
|--------|-------------|
| `initialize` | Protocol handshake |
| `tools/list` | List available tools |
| `tools/call` | Execute a tool |
| `resources/list` | List available resources |
| `resources/read` | Read a resource |

### Example Session

```
→ {"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
← {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{},"resources":{}},"serverInfo":{"name":"engram","version":"1.1.0"}}}

→ {"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
← {"jsonrpc":"2.0","id":2,"result":{"tools":[...]}}

→ {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"engram_store","arguments":{"entity":"test"}}}
← {"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"{\"stored\":\"test\",\"slot\":1}"}]}}
```

## Troubleshooting

**Server doesn't start**: Make sure the brain file path is writable. The server creates it if it doesn't exist.

**Tool calls fail**: Check that required parameters are provided. The error message in the JSON-RPC response will indicate what's missing.

**Performance**: The MCP server uses the same lock-based Graph access as the HTTP server. For high-throughput scenarios, use the HTTP API instead.
