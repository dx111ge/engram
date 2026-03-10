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

### Ingest Pipeline

#### POST /ingest -- Text ingest through NER pipeline

Ingests raw text through the named entity recognition and entity resolution pipeline. Entities and relationships are extracted automatically and stored in the graph.

```bash
curl -X POST http://localhost:3030/ingest \
  -H 'Content-Type: application/json' \
  -d '{
    "text": "Angela Merkel served as Chancellor of Germany from 2005 to 2021.",
    "source": "wikipedia",
    "pipeline": "default",
    "skip": ["coref"]
  }'
```

Response:
```json
{"entities_extracted": 3, "relations_extracted": 2, "pipeline": "default"}
```

#### POST /ingest/file -- File ingest (auto-detect format)

Ingests a file by auto-detecting its format (plain text, JSON, CSV, PDF, HTML, Markdown). The file content is run through the NER pipeline.

```bash
curl -X POST http://localhost:3030/ingest/file \
  -H 'Content-Type: multipart/form-data' \
  -F 'file=@report.pdf' \
  -F 'source=analyst-report' \
  -F 'pipeline=default'
```

Response:
```json
{"entities_extracted": 47, "relations_extracted": 31, "format_detected": "pdf", "pipeline": "default"}
```

#### POST /ingest/configure -- Configure pipeline settings

Configure the NER/entity-resolution pipeline parameters (confidence thresholds, enabled stages, deduplication settings).

```bash
curl -X POST http://localhost:3030/ingest/configure \
  -H 'Content-Type: application/json' \
  -d '{
    "pipeline": "default",
    "min_entity_confidence": 0.6,
    "enable_coref": true,
    "dedup_threshold": 0.85
  }'
```

Response:
```json
{"pipeline": "default", "configured": true}
```

#### POST /ingest/webhook/{pipeline_id} -- Webhook receiver for external data

Receives data from external systems (CI/CD, monitoring, RSS aggregators) and routes it through the specified pipeline.

```bash
curl -X POST http://localhost:3030/ingest/webhook/security-feeds \
  -H 'Content-Type: application/json' \
  -d '{
    "event": "alert",
    "data": "CVE-2026-1234 affects OpenSSL 3.x before 3.2.1"
  }'
```

Response:
```json
{"accepted": true, "pipeline_id": "security-feeds", "queued": true}
```

#### WS /ingest/ws/{pipeline_id} -- WebSocket real-time ingest

WebSocket endpoint for high-throughput real-time ingestion. Accepts newline-delimited JSON (NDJSON) over WebSocket frames. Each line is an independent ingest request processed through the specified pipeline.

```
ws://localhost:3030/ingest/ws/live-feed
> {"text": "Server CPU at 95%", "source": "monitoring"}
> {"text": "Deploy v2.3.1 completed", "source": "ci"}
< {"accepted": 2, "errors": 0}
```

### Sources

#### GET /sources -- List configured sources with health status

Returns all configured data sources and their current health (reachable, last check, error rate).

```bash
curl http://localhost:3030/sources
```

Response:
```json
[
  {"name": "gdelt", "type": "proxy", "healthy": true, "last_check": 1741340400000, "error_rate": 0.01},
  {"name": "rss-security", "type": "rss", "healthy": true, "last_check": 1741340380000, "error_rate": 0.0}
]
```

#### GET /sources/{name}/usage -- Source usage statistics and budget

Returns usage statistics for a source including query count, token consumption, and budget remaining (if configured).

```bash
curl http://localhost:3030/sources/gdelt/usage
```

Response:
```json
{"source": "gdelt", "queries_today": 142, "tokens_used": 0, "budget_remaining": null, "rate_limit_remaining": 858}
```

#### GET /sources/{name}/ledger -- Search ledger (query history, dedup stats)

Returns the search ledger for a source: recent queries, deduplication statistics, and cache hit rates.

```bash
curl http://localhost:3030/sources/gdelt/ledger
```

Response:
```json
{
  "source": "gdelt",
  "total_queries": 1423,
  "dedup_hits": 312,
  "cache_hit_rate": 0.22,
  "recent_queries": [
    {"query": "Ukraine energy infrastructure", "timestamp": 1741340400000, "results": 15, "cached": false}
  ]
}
```

### Action Engine

#### POST /actions/rules -- Load action rules (TOML format)

Load one or more action rules in TOML format. Rules define trigger conditions and effects (alerts, enrichments, auto-tagging).

```bash
curl -X POST http://localhost:3030/actions/rules \
  -H 'Content-Type: application/json' \
  -d '{
    "rules_toml": "[[rule]]\nname = \"high-severity-alert\"\ncondition = \"entity.type == '\''vulnerability'\'' && entity.confidence > 0.8\"\neffect = \"alert\"\n[rule.effect_config]\nchannel = \"security-team\""
  }'
```

Response:
```json
{"loaded": 1, "rule_ids": ["high-severity-alert"]}
```

#### GET /actions/rules -- List loaded action rules

```bash
curl http://localhost:3030/actions/rules
```

Response:
```json
[
  {"id": "high-severity-alert", "name": "high-severity-alert", "condition": "entity.type == 'vulnerability' && entity.confidence > 0.8", "effect": "alert"}
]
```

#### GET /actions/rules/{id} -- Get specific rule

```bash
curl http://localhost:3030/actions/rules/high-severity-alert
```

Response:
```json
{"id": "high-severity-alert", "name": "high-severity-alert", "condition": "entity.type == 'vulnerability' && entity.confidence > 0.8", "effect": "alert", "effect_config": {"channel": "security-team"}}
```

#### DELETE /actions/rules/{id} -- Delete a rule

```bash
curl -X DELETE http://localhost:3030/actions/rules/high-severity-alert
```

Response:
```json
{"deleted": "high-severity-alert"}
```

#### POST /actions/dry-run -- Dry run: test an event against rules

Tests a synthetic event against all loaded rules without executing any effects. Returns which rules would fire.

```bash
curl -X POST http://localhost:3030/actions/dry-run \
  -H 'Content-Type: application/json' \
  -d '{
    "event": {
      "entity": "CVE-2026-5678",
      "type": "vulnerability",
      "confidence": 0.92
    }
  }'
```

Response:
```json
{"matched_rules": ["high-severity-alert"], "effects_suppressed": true}
```

### Reason / Gap Detection

#### GET /reason/gaps -- List knowledge gaps ranked by severity

Identifies areas of the graph with missing information, weak connections, or contradictions (black areas). Returns gaps ranked by severity.

```bash
curl "http://localhost:3030/reason/gaps?min_severity=0.5&limit=10"
```

Response:
```json
[
  {"gap_id": "g-001", "description": "No source attribution for 12 entities in cluster 'networking'", "severity": 0.82, "affected_nodes": 12},
  {"gap_id": "g-002", "description": "Contradictory facts about server-01 uptime", "severity": 0.65, "affected_nodes": 3}
]
```

#### POST /reason/scan -- Full graph scan for black areas

Triggers a full graph scan to detect knowledge gaps, orphan nodes, weak clusters, and contradictions. More thorough than `/reason/gaps` but slower.

```bash
curl -X POST http://localhost:3030/reason/scan
```

Response:
```json
{"gaps_found": 7, "orphan_nodes": 3, "weak_clusters": 2, "contradictions": 1, "scan_time_ms": 142}
```

#### GET /reason/frontier -- List frontier nodes

Returns frontier nodes -- entities with very few connections that represent the boundary of current knowledge.

```bash
curl http://localhost:3030/reason/frontier
```

Response:
```json
[
  {"label": "CVE-2026-9999", "connections": 1, "confidence": 0.6, "type": "vulnerability"},
  {"label": "unknown-actor-7", "connections": 0, "confidence": 0.4, "type": "threat_actor"}
]
```

#### POST /reason/suggest -- LLM-powered investigation suggestions

Analyzes knowledge gaps and generates investigation suggestions. Returns mechanical query suggestions per gap, and optionally LLM-generated investigation plans if an LLM endpoint is configured.

```bash
curl -X POST http://localhost:3030/reason/suggest \
  -H 'Content-Type: application/json' \
  -d '{"gap_id": "g-001", "use_llm": true}'
```

Response:
```json
{
  "gap_id": "g-001",
  "mechanical_suggestions": [
    "Search GDELT for entities in cluster 'networking'",
    "Query mesh peers for 'networking' topic coverage"
  ],
  "llm_suggestions": [
    "Investigate network topology documentation from infrastructure team",
    "Cross-reference with recent change management tickets"
  ]
}
```

### Mesh Discovery

#### GET /mesh/profiles -- List peer knowledge profiles

Returns auto-derived knowledge profiles for all known mesh peers, including their domain coverage, depth, and freshness.

```bash
curl http://localhost:3030/mesh/profiles
```

Response:
```json
[
  {
    "peer_id": "a3f8c21b",
    "name": "team-server",
    "domains": [
      {"domain": "security", "depth": 3, "node_count": 1200, "avg_confidence": 0.78, "freshness": 0.92}
    ]
  }
]
```

#### GET /mesh/discover?topic=X -- Find peers by topic

Discover mesh peers whose knowledge profiles cover the requested topic.

```bash
curl "http://localhost:3030/mesh/discover?topic=cybersecurity"
```

Response:
```json
[
  {"peer_id": "a3f8c21b", "name": "team-server", "relevance": 0.91, "depth": 3, "node_count": 1200}
]
```

#### POST /mesh/query -- Federated query across mesh peers

Execute a query that fans out to relevant mesh peers, merges results with deduplication, and ranks by confidence. Respects ACL sensitivity clearance.

```bash
curl -X POST http://localhost:3030/mesh/query \
  -H 'Content-Type: application/json' \
  -d '{
    "query": "What is known about CVE-2026-1234?",
    "min_confidence": 0.3,
    "max_hops": 2,
    "clearance": "internal"
  }'
```

Response:
```json
{
  "results": [
    {"label": "CVE-2026-1234", "confidence": 0.88, "source_peer": "a3f8c21b", "hops": 1},
    {"label": "OpenSSL-3.x", "confidence": 0.72, "source_peer": "local", "hops": 0}
  ],
  "peers_queried": 3,
  "peers_responded": 2
}
```

### Streaming

#### GET /events/stream -- SSE event subscription (graph changes)

Server-Sent Events stream of real-time graph changes (node created, edge added, confidence updated, etc.).

```bash
curl -N http://localhost:3030/events/stream
```

```
event: node_created
data: {"label": "CVE-2026-5678", "type": "vulnerability", "confidence": 0.9}

event: edge_added
data: {"from": "CVE-2026-5678", "to": "OpenSSL", "relationship": "affects"}

event: confidence_updated
data: {"label": "server-01", "old": 0.85, "new": 0.87, "reason": "reinforced"}
```

#### GET /batch/jobs/{id}/stream -- SSE batch job progress

Stream progress updates for a running batch import job.

```bash
curl -N http://localhost:3030/batch/jobs/job-001/stream
```

```
event: progress
data: {"job_id": "job-001", "processed": 500, "total": 2000, "errors": 0}

event: progress
data: {"job_id": "job-001", "processed": 1000, "total": 2000, "errors": 1}

event: complete
data: {"job_id": "job-001", "processed": 2000, "total": 2000, "errors": 3}
```

#### GET /enrich/stream?q=X -- SSE enrichment streaming

Stream enrichment results as they arrive from multiple sources (GDELT, RSS, LLM, mesh peers).

```bash
curl -N "http://localhost:3030/enrich/stream?q=Ukraine+energy+infrastructure"
```

```
event: source_started
data: {"source": "gdelt", "query": "Ukraine energy infrastructure"}

event: result
data: {"source": "gdelt", "entity": "Zaporizhzhia NPP", "confidence": 0.85}

event: result
data: {"source": "rss-security", "entity": "IAEA Report March 2026", "confidence": 0.78}

event: enrichment_complete
data: {"sources_queried": 3, "total_results": 12}
```

### Proxy

#### GET /proxy/gdelt -- GDELT proxy

Proxies requests to the GDELT API, adding caching, rate limiting, and result normalization.

```bash
curl "http://localhost:3030/proxy/gdelt?query=cybersecurity+Russia&maxrecords=10&format=json"
```

Response: GDELT API response (JSON), cached and rate-limited.

#### GET /proxy/rss -- RSS feed proxy

Fetches and parses RSS feeds, normalizing entries into engram-compatible entity format.

```bash
curl "http://localhost:3030/proxy/rss?url=https://feeds.example.com/security.xml&limit=20"
```

Response:
```json
{"entries": [{"title": "New vulnerability disclosed", "published": "2026-03-10T12:00:00Z", "summary": "..."}], "count": 20}
```

#### POST /proxy/llm -- LLM proxy

Proxies requests to the configured LLM endpoint (Ollama, OpenAI, vLLM), adding context from the knowledge graph.

```bash
curl -X POST http://localhost:3030/proxy/llm \
  -H 'Content-Type: application/json' \
  -d '{
    "prompt": "Analyze the implications of CVE-2026-1234",
    "context_entities": ["CVE-2026-1234", "OpenSSL"],
    "max_tokens": 500
  }'
```

Response:
```json
{"response": "Based on the knowledge graph, CVE-2026-1234 affects...", "tokens_used": 312, "context_injected": 2}
```

#### GET /proxy/search -- Web search proxy

Proxies web search queries through configured search backends, with result caching and deduplication.

```bash
curl "http://localhost:3030/proxy/search?q=CVE-2026-1234+exploit&limit=10"
```

Response:
```json
{"results": [{"title": "CVE-2026-1234 Analysis", "url": "https://...", "snippet": "..."}], "count": 10, "cached": false}
```

### Batch Streaming

#### POST /batch/stream -- NDJSON streaming batch import

Accepts newline-delimited JSON for streaming batch import. Each line is processed independently. Results stream back as NDJSON.

```bash
curl -X POST http://localhost:3030/batch/stream \
  -H 'Content-Type: application/x-ndjson' \
  -d '{"entity": "server-01", "type": "server", "confidence": 0.9}
{"entity": "server-02", "type": "server", "confidence": 0.85}
{"from": "server-01", "to": "server-02", "relationship": "replicates_to"}'
```

Response (NDJSON):
```
{"ok": true, "action": "store", "label": "server-01"}
{"ok": true, "action": "store", "label": "server-02"}
{"ok": true, "action": "relate", "from": "server-01", "to": "server-02"}
```

### System

#### GET /health

```bash
curl http://localhost:3030/health
```

```json
{"status": "ok", "version": "1.1.0"}
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
- **Deferred checkpoint**: Writes go to WAL + mmap immediately (crash-recoverable), but the expensive disk flush (`msync`/`FlushViewOfFile`) happens on a background timer every 5 seconds. This means writes complete in microseconds instead of milliseconds.
- **Batch endpoint**: For bulk operations (imports, mesh sync), use `/batch` instead of individual calls. One write lock acquisition, one checkpoint — not N.

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
