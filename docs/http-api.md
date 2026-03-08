# Engram HTTP API

REST API for the engram knowledge graph. All endpoints accept and return JSON.

## Installation

Download the latest binary from the [Releases](https://github.com/dx111ge/engram/releases) page.

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

#### POST /batch — Bulk store entities and relationships

```bash
curl -X POST http://localhost:3030/batch \
  -H 'Content-Type: application/json' \
  -d '{
    "entities": [
      {"entity": "postgresql", "type": "database", "properties": {"version": "16"}, "confidence": 0.95},
      {"entity": "redis", "type": "cache", "confidence": 0.90},
      {"entity": "nginx", "type": "proxy", "confidence": 0.90}
    ],
    "relations": [
      {"from": "postgresql", "to": "redis", "relationship": "caches_with", "confidence": 0.9},
      {"from": "nginx", "to": "postgresql", "relationship": "proxies", "confidence": 0.85}
    ],
    "source": "infrastructure-scan"
  }'
```

Response:
```json
{"nodes_stored": 3, "edges_created": 2, "errors": null}
```

All entities and relationships are written under a single write lock with a single deferred checkpoint. This is dramatically faster than individual `/store` + `/relate` calls for bulk ingestion (e.g., mesh delta sync, imports).

If some operations fail, the successful ones are kept and errors are returned:
```json
{"nodes_stored": 2, "edges_created": 1, "errors": ["relate foo -> bar: node not found"]}
```

#### POST /query — Graph traversal

```bash
curl -X POST http://localhost:3030/query \
  -H 'Content-Type: application/json' \
  -d '{
    "start": "postgresql",
    "depth": 2,
    "min_confidence": 0.5,
    "direction": "both"
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

The `direction` parameter controls which edges to follow during traversal:
- `"both"` (default) -- follow both incoming and outgoing edges (full neighborhood)
- `"out"` -- outgoing edges only (traditional forward BFS)
- `"in"` -- incoming edges only (reverse traversal)

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

Evaluates rules against the graph using forward chaining. Runs to **fixed point** — automatically repeats until no new facts are derived (max 10 rounds). Duplicate edges, properties, and flags are skipped.

```bash
curl -X POST http://localhost:3030/learn/derive \
  -H 'Content-Type: application/json' \
  -d '{
    "rules": [
      "rule transitive_type\nwhen edge(A, \"is_a\", B)\nwhen edge(B, \"is_a\", C)\nthen edge(A, \"is_a\", C, min(e1, e2))"
    ]
  }'
```

### JSON-LD (Linked Data)

#### GET /export/jsonld -- Export entire graph as JSON-LD

Returns the full knowledge graph as a JSON-LD document with `@context`, `@graph`, and semantic URIs. Nodes are subjects, edges are predicates, properties are datatype assertions.

```bash
curl http://localhost:3030/export/jsonld
```

```json
{
  "@context": {
    "engram": "engram://vocab/",
    "schema": "https://schema.org/",
    "rdf": "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
    "rdfs": "http://www.w3.org/2000/01/rdf-schema#"
  },
  "@graph": [
    {
      "@id": "engram://node/Rust",
      "@type": "engram:Language",
      "rdfs:label": "Rust",
      "engram:confidence": 0.8,
      "engram:compiles_to": { "@id": "engram://node/WebAssembly", "engram:confidence": 0.8 }
    }
  ]
}
```

#### POST /import/jsonld -- Import JSON-LD into the graph

Parses a JSON-LD document and creates nodes and edges. Supports `@graph` arrays, `@type`, `rdfs:label`, and `schema:name` for labels. Object references (with `@id`) become edges.

```bash
curl -X POST http://localhost:3030/import/jsonld \
  -H 'Content-Type: application/json' \
  -d '{
    "data": {
      "@context": { "schema": "https://schema.org/" },
      "@graph": [
        {
          "@id": "schema:Rust",
          "@type": "schema:ComputerLanguage",
          "rdfs:label": "Rust",
          "schema:dateCreated": "2010",
          "schema:creator": { "@id": "schema:GraydonHoare" }
        },
        {
          "@id": "schema:GraydonHoare",
          "rdfs:label": "Graydon Hoare",
          "@type": "schema:Person"
        }
      ]
    },
    "source": "schema.org"
  }'
```

```json
{ "nodes_imported": 2, "edges_imported": 1, "errors": null }
```

### Vector Quantization

#### POST /quantize -- Enable or disable int8 quantization

Int8 scalar quantization reduces vector memory by ~4x with ~1% accuracy loss. When enabled, HNSW graph traversal uses quantized int8 vectors for fast candidate filtering, then reranks final results with full f32 precision.

```bash
# Enable int8 quantization
curl -X POST http://localhost:3030/quantize \
  -H 'Content-Type: application/json' \
  -d '{"enabled": true}'
```

```json
{"mode": "int8", "vector_memory_bytes": 460800}
```

```bash
# Disable quantization
curl -X POST http://localhost:3030/quantize \
  -H 'Content-Type: application/json' \
  -d '{"enabled": false}'
```

```json
{"mode": "none", "vector_memory_bytes": 153600}
```

**Memory impact**: A 1M vector collection at 384 dimensions uses ~1.5 GB with f32 only. With int8 enabled, the f32 vectors are kept for reranking accuracy, and int8 copies are added for traversal. For storage-only savings (future), binary quantization (32x) and product quantization are planned.

### System

#### GET /health

```bash
curl http://localhost:3030/health
```

```json
{"status": "ok", "version": "1.0.0"}
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

#### GET /compute — Hardware and embedder info

```bash
curl http://localhost:3030/compute
```

```json
{
  "cpu_cores": 20,
  "has_avx2": true,
  "has_neon": false,
  "has_gpu": true,
  "gpu_name": "NVIDIA GeForce RTX 5070",
  "gpu_backend": "Vulkan",
  "has_npu": true,
  "npu_name": "Intel(R) Graphics",
  "dedicated_npu": [],
  "embedder_model": "nomic-embed-text-v2-moe:latest",
  "embedder_dim": 768,
  "embedder_endpoint": "http://localhost:11434/v1"
}
```

#### GET /tools — LLM tool definitions

```bash
curl http://localhost:3030/tools
```

Returns OpenAI-compatible tool/function definitions for LLM integration.

## Concurrency Model

Engram uses **RwLock** (not Mutex) for graph access:

- **Readers** (`/search`, `/query`, `/similar`, `/ask`, `/node/{label}`, `/explain/{label}`, `/stats`) acquire a **read lock** and can run concurrently with each other.
- **Writers** (`/store`, `/relate`, `/batch`, `/tell`, `/delete`, `/learn/*`) acquire an **exclusive write lock**.
- **Deferred checkpoint**: Writes are immediately crash-recoverable, but the expensive disk flush happens on a background timer every 5 seconds. This means writes complete in microseconds instead of milliseconds.
- **Batch endpoint**: For bulk operations (imports, mesh sync), use `/batch` instead of individual calls. One write lock acquisition, one checkpoint — not N.

## CORS

All origins are allowed by default (permissive CORS).

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
