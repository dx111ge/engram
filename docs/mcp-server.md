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
← {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{},"resources":{}},"serverInfo":{"name":"engram","version":"0.1.0"}}}

→ {"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
← {"jsonrpc":"2.0","id":2,"result":{"tools":[...]}}

→ {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"engram_store","arguments":{"entity":"test"}}}
← {"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"{\"stored\":\"test\",\"slot\":1}"}]}}
```

## Troubleshooting

**Server doesn't start**: Make sure the brain file path is writable. The server creates it if it doesn't exist.

**Tool calls fail**: Check that required parameters are provided. The error message in the JSON-RPC response will indicate what's missing.

**Performance**: The MCP server uses the same lock-based Graph access as the HTTP server. For high-throughput scenarios, use the HTTP API instead.
