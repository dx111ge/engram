# Engram HTTP API

REST API for the engram knowledge graph. All endpoints accept and return JSON.

## Installation

```bash
# Build from source
cargo build --release

# The binary includes the HTTP server — no separate install
```

## Starting the Server

```bash
# Default: listens on 0.0.0.0:3030, uses ./knowledge.brain
engram serve

# Custom path and address
engram serve /path/to/my.brain 127.0.0.1:8080
```

## Endpoints

### Core Graph Operations

#### POST /store — Store a new entity

```bash
curl -X POST http://localhost:3030/store \
  -H 'Content-Type: application/json' \
  -d '{
    "entity": "postgresql",
    "type": "database",
    "properties": {"version": "16", "role": "primary"},
    "source": "sysadmin",
    "confidence": 0.95
  }'
```

Response:
```json
{"node_id": 1, "label": "postgresql", "confidence": 0.95}
```

Only `entity` is required. `type`, `properties`, `source`, and `confidence` are optional.

#### POST /relate — Create a relationship

```bash
curl -X POST http://localhost:3030/relate \
  -H 'Content-Type: application/json' \
  -d '{
    "from": "postgresql",
    "to": "redis",
    "relationship": "caches_with",
    "confidence": 0.9
  }'
```

Response:
```json
{"from": "postgresql", "to": "redis", "relationship": "caches_with", "edge_slot": 1}
```

#### POST /query — Graph traversal

```bash
curl -X POST http://localhost:3030/query \
  -H 'Content-Type: application/json' \
  -d '{
    "start": "postgresql",
    "depth": 2,
    "min_confidence": 0.5
  }'
```

Response:
```json
{
  "nodes": [
    {"node_id": 1, "label": "postgresql", "confidence": 0.95, "depth": 0},
    {"node_id": 2, "label": "redis", "confidence": 0.8, "depth": 1}
  ],
  "edges": [...]
}
```

#### POST /search — Full-text keyword search

```bash
curl -X POST http://localhost:3030/search \
  -H 'Content-Type: application/json' \
  -d '{"query": "database", "limit": 5}'
```

#### POST /similar — Semantic similarity search

```bash
curl -X POST http://localhost:3030/similar \
  -H 'Content-Type: application/json' \
  -d '{"text": "high CPU usage on production", "limit": 10}'
```

Requires an embedder to be configured for vector search. Falls back to BM25 text search.

#### GET /node/{label} — Get full node details

```bash
curl http://localhost:3030/node/postgresql
```

Response includes properties, outgoing edges, and incoming edges.

#### DELETE /node/{label} — Soft-delete

```bash
curl -X DELETE http://localhost:3030/node/redis
```

Sets confidence to 0 and records provenance. The node is not physically removed.

### Learning Operations

#### POST /learn/reinforce — Boost confidence

```bash
# Access boost (+0.02)
curl -X POST http://localhost:3030/learn/reinforce \
  -H 'Content-Type: application/json' \
  -d '{"entity": "postgresql"}'

# Confirmation boost (+0.10, requires source)
curl -X POST http://localhost:3030/learn/reinforce \
  -H 'Content-Type: application/json' \
  -d '{"entity": "postgresql", "source": "monitoring"}'
```

#### POST /learn/correct — Mark fact as wrong

```bash
curl -X POST http://localhost:3030/learn/correct \
  -H 'Content-Type: application/json' \
  -d '{"entity": "postgresql", "reason": "decommissioned"}'
```

Zeroes the node's confidence and propagates distrust to neighbors (0.5 damping per hop).

#### POST /learn/decay — Trigger decay cycle

```bash
curl -X POST http://localhost:3030/learn/decay
```

Applies time-based confidence decay (0.999/day) to all nodes. Nodes below 0.10 become archival candidates.

#### POST /learn/derive — Run inference rules

```bash
curl -X POST http://localhost:3030/learn/derive \
  -H 'Content-Type: application/json' \
  -d '{
    "rules": [
      "rule transitive_type\nwhen edge(A, \"is_a\", B)\nwhen edge(B, \"is_a\", C)\nthen edge(A, \"is_a\", C, min(e1, e2))"
    ]
  }'
```

### System

#### GET /health

```bash
curl http://localhost:3030/health
```

```json
{"status": "ok", "version": "0.1.0"}
```

#### GET /stats

```bash
curl http://localhost:3030/stats
```

```json
{"nodes": 42, "edges": 128}
```

#### GET /explain/{label} — Full provenance

```bash
curl http://localhost:3030/explain/postgresql
```

Returns confidence, properties, co-occurrences, and all edges.

#### GET /tools — LLM tool definitions

```bash
curl http://localhost:3030/tools
```

Returns OpenAI-compatible tool/function definitions for LLM integration.

## CORS

All origins are allowed by default (permissive CORS). Restrict in production by modifying `server.rs`.

## Error Handling

All errors return JSON:

```json
{"error": "node not found: nonexistent"}
```

HTTP status codes:
- `200` — success
- `400` — bad request (invalid rule syntax, missing fields)
- `404` — node not found
- `500` — internal error (lock poisoned, storage error)
