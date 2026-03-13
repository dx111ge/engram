# Engram MCP Server

Model Context Protocol (MCP) server for native integration with Claude, Cursor, Windsurf, and any MCP-compatible AI tool.

MCP is JSON-RPC 2.0 over stdio -- a thin wrapper over the engram graph API.

## Installation

```bash
# Build from source
cargo build --release

# The engram binary includes the MCP server
```

## Starting the MCP Server

```bash
# Default: uses ./knowledge.brain
engram mcp

# Custom brain file path
engram mcp /path/to/my.brain
```

The server reads JSON-RPC requests from stdin and writes responses to stdout, one JSON object per line.

## Configuration

### Authentication

When the engram server has authentication enabled (admin account created), MCP connections need an API key. Generate one via the Security tab in the web UI or the HTTP API:

```bash
curl -X POST http://localhost:3030/auth/api-keys \
  -H 'Authorization: Bearer YOUR_SESSION_TOKEN' \
  -H 'Content-Type: application/json' \
  -d '{"label":"MCP Server"}'
```

Pass the API key via the `ENGRAM_API_KEY` environment variable in your MCP config.

### Claude Code

Add to your project's `.mcp.json` or global MCP settings:

```json
{
  "mcpServers": {
    "engram": {
      "command": "engram",
      "args": ["mcp", "/path/to/knowledge.brain"],
      "env": {
        "ENGRAM_API_KEY": "egk_your_key_here"
      }
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
    "transport": "stdio",
    "env": {
      "ENGRAM_API_KEY": "egk_your_key_here"
    }
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
      "args": ["mcp", "/path/to/knowledge.brain"],
      "env": {
        "ENGRAM_API_KEY": "egk_your_key_here"
      }
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

List knowledge gaps (black areas) in the graph, ranked by severity.

Parameters:
- `min_severity` — minimum severity threshold (0.0-1.0, default: 0.0)
- `limit` — max results (default: 10)

Example call:
```json
{
  "name": "engram_gaps",
  "arguments": {
    "min_severity": 0.5,
    "limit": 5
  }
}
```

Returns:
```json
[
  {"gap_id": "g-001", "description": "No source attribution for 12 entities in cluster 'networking'", "severity": 0.82, "affected_nodes": 12}
]
```

### engram_frontier

List frontier nodes -- entities with few connections at the boundary of current knowledge.

Parameters: none (returns all frontier nodes)

Example call:
```json
{
  "name": "engram_frontier",
  "arguments": {}
}
```

Returns:
```json
[
  {"label": "CVE-2026-9999", "connections": 1, "confidence": 0.6, "type": "vulnerability"}
]
```

### engram_mesh_discover

Find mesh peers whose knowledge profiles cover a given topic.

Parameters:
- `topic` (required) — topic to search for

Example call:
```json
{
  "name": "engram_mesh_discover",
  "arguments": {
    "topic": "cybersecurity"
  }
}
```

Returns:
```json
[
  {"peer_id": "a3f8c21b", "name": "team-server", "relevance": 0.91, "depth": 3}
]
```

### engram_mesh_query

Federated query across the knowledge mesh. Fans out to relevant peers, merges and deduplicates results.

Parameters:
- `query` (required) — search query text
- `min_confidence` — minimum confidence threshold (default: 0.0)
- `max_hops` — maximum peer hops (default: 2)
- `clearance` — sensitivity clearance level: `public`, `internal`, `confidential`, `restricted` (default: `public`)

Example call:
```json
{
  "name": "engram_mesh_query",
  "arguments": {
    "query": "What is known about CVE-2026-1234?",
    "min_confidence": 0.3,
    "max_hops": 2,
    "clearance": "internal"
  }
}
```

Returns:
```json
{
  "results": [
    {"label": "CVE-2026-1234", "confidence": 0.88, "source_peer": "a3f8c21b", "hops": 1}
  ],
  "peers_queried": 3,
  "peers_responded": 2
}
```

### engram_ingest

Ingest text through the NER/entity-resolution pipeline. **Restricted** -- requires appropriate access level.

Uses the configured NER backend (GLiNER via candle, or built-in rules) with optional coreference resolution (pronoun -> canonical entity mapping) and NLI-based relation extraction (zero-shot, multilingual).

Parameters:
- `text` (required) — text to ingest
- `source` — provenance label for extracted entities
- `pipeline` — pipeline name (default: `default`)
- `skip` — list of pipeline stages to skip (e.g., `["coref", "nli-rel"]`)

Example call:
```json
{
  "name": "engram_ingest",
  "arguments": {
    "text": "Angela Merkel served as Chancellor of Germany from 2005 to 2021.",
    "source": "wikipedia",
    "pipeline": "default"
  }
}
```

Returns:
```json
{"entities_extracted": 3, "relations_extracted": 2, "pipeline": "default"}
```

### engram_create_rule

Create an action engine rule. **Restricted** -- rules can trigger alerts and automated effects.

Parameters:
- `name` (required) — rule name/identifier
- `condition` (required) — trigger condition expression
- `effect` (required) — effect type (`alert`, `enrich`, `tag`, `webhook`)
- `effect_config` — effect-specific configuration object

Example call:
```json
{
  "name": "engram_create_rule",
  "arguments": {
    "name": "high-severity-alert",
    "condition": "entity.type == 'vulnerability' && entity.confidence > 0.8",
    "effect": "alert",
    "effect_config": {"channel": "security-team"}
  }
}
```

Returns:
```json
{"loaded": 1, "rule_ids": ["high-severity-alert"]}
```

### engram_assess_create

Create an assessment (hypothesis) with watched entities and initial probability.

Parameters:
- `title` (required) -- assessment title/hypothesis
- `category` -- category label (financial, security, geopolitical, ...)
- `description` -- detailed description
- `watches` -- array of entity labels to watch
- `initial_probability` -- starting probability (0.05-0.95, default: 0.50)

Example call:
```json
{
  "name": "engram_assess_create",
  "arguments": {
    "title": "NVIDIA stock > $200 by Q3 2026",
    "category": "financial",
    "watches": ["NVIDIA", "GPU market"],
    "initial_probability": 0.50
  }
}
```

Returns:
```json
{"label": "Assessment:nvidia-stock-gt-200", "probability": 0.50, "watches": 2}
```

### engram_assess_list

List assessments with optional category and status filters.

Parameters:
- `category` -- filter by category
- `status` -- filter by status (active, paused, archived, resolved)

### engram_assess_get

Get full assessment detail including history, evidence, and watches.

Parameters:
- `label` (required) -- assessment label (e.g., "Assessment:nvidia-stock-gt-200")

### engram_assess_evaluate

Trigger manual re-evaluation of an assessment. Recalculates probability from all current evidence.

Parameters:
- `label` (required) -- assessment label

Returns:
```json
{"label": "Assessment:nvidia-stock-gt-200", "old_probability": 0.50, "new_probability": 0.62, "shift": 0.12}
```

### engram_assess_evidence

Add evidence to an assessment (supporting or contradicting).

Parameters:
- `label` (required) -- assessment label
- `node_label` (required) -- entity label to add as evidence
- `direction` (required) -- `"supports"` or `"contradicts"`

### engram_assess_watch

Add or remove a watched entity from an assessment.

Parameters:
- `label` (required) -- assessment label
- `entity_label` (required) -- entity to watch
- `action` -- `"add"` (default) or `"remove"`

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
