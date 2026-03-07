# Use Case 4: Building a Support Knowledge Base

### Overview

IT support teams accumulate knowledge about infrastructure problems — which error messages mean what, which servers are affected, what the root causes are, and which fixes worked. Engram is well suited for this pattern: it stores structured knowledge as a graph, lets you score solutions by confidence, reinforces working fixes, penalizes wrong diagnoses, and applies decay so stale runbooks fade unless refreshed.

This walkthrough builds a support knowledge base for a fictional e-commerce platform. It shows the full lifecycle from "we see an error" to "we have a confirmed fix" to "old fixes lose confidence over time."

**What this demonstrates today (v0.1.0):**

- Storing servers, services, and error patterns as nodes
- Relating errors to root causes, root causes to solutions
- Property-based filtering (`prop:severity=critical`, `prop:status=open`)
- Confidence scoring: confirmed solutions get higher confidence than guesses
- Learning endpoints: `/learn/reinforce`, `/learn/correct`, `/learn/decay`
- Inference rules via `/learn/derive` to propagate impact across the graph
- Memory tier transitions: active knowledge versus archival

**What requires external tools:**

- Alert ingestion from monitoring systems (PagerDuty, Alertmanager, etc.) requires external tooling to call engram's HTTP API when alerts fire
- Automated ticket resolution detection (marking a ticket as resolved and triggering reinforcement) requires your ticketing system to call engram

### Prerequisites

- `engram` binary on your PATH
- `curl` for HTTP API calls

### Step-by-Step Implementation

#### Step 1: Create and populate the infrastructure knowledge

```bash
engram serve support.brain 127.0.0.1:3030
```

#### Step 2: Store servers and services

```bash
# Store servers
curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{"entity":"web-server-01","type":"server","properties":{"env":"production","region":"us-east-1"},"confidence":0.95}'

curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{"entity":"db-primary-01","type":"server","properties":{"env":"production","role":"database"},"confidence":0.95}'

curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{"entity":"cache-01","type":"server","properties":{"env":"production","role":"cache"},"confidence":0.95}'

# Store services
curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{"entity":"checkout-api","type":"service","properties":{"owner":"payments-team","tier":"1"},"confidence":0.95}'

curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{"entity":"postgresql","type":"service","properties":{"version":"16","port":"5432"},"confidence":0.95}'

curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{"entity":"redis","type":"service","properties":{"port":"6379","maxmemory":"4gb"},"confidence":0.95}'
```

#### Step 3: Relate servers to services they run

```bash
curl -s -X POST http://127.0.0.1:3030/tell \
  -H "Content-Type: application/json" \
  -d '{"statement":"web-server-01 runs on checkout-api","source":"cmdb"}'

curl -s -X POST http://127.0.0.1:3030/tell \
  -H "Content-Type: application/json" \
  -d '{"statement":"db-primary-01 runs on postgresql","source":"cmdb"}'

curl -s -X POST http://127.0.0.1:3030/tell \
  -H "Content-Type: application/json" \
  -d '{"statement":"cache-01 runs on redis","source":"cmdb"}'

curl -s -X POST http://127.0.0.1:3030/relate \
  -H "Content-Type: application/json" \
  -d '{"from":"checkout-api","relationship":"depends_on","to":"postgresql","confidence":0.95}'

curl -s -X POST http://127.0.0.1:3030/relate \
  -H "Content-Type: application/json" \
  -d '{"from":"checkout-api","relationship":"depends_on","to":"redis","confidence":0.95}'
```

#### Step 4: Store known error patterns

```bash
# Error: connection pool exhaustion
curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{
    "entity": "ERR:connection_pool_exhausted",
    "type": "error_pattern",
    "properties": {
      "severity": "critical",
      "symptom": "FATAL: remaining connection slots are reserved",
      "status": "open"
    },
    "confidence": 0.90
  }'

# Root cause node
curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{
    "entity": "CAUSE:pg_max_connections_exceeded",
    "type": "root_cause",
    "properties": {"component": "postgresql", "category": "resource_exhaustion"},
    "confidence": 0.75
  }'

# Solution node
curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{
    "entity": "FIX:increase_pg_max_connections",
    "type": "solution",
    "properties": {
      "action": "ALTER SYSTEM SET max_connections = 500; SELECT pg_reload_conf();",
      "risk": "low",
      "verified": "false"
    },
    "confidence": 0.60
  }'

curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{
    "entity": "FIX:add_pgbouncer",
    "type": "solution",
    "properties": {
      "action": "Deploy PgBouncer connection pooler in front of PostgreSQL",
      "risk": "medium",
      "verified": "false"
    },
    "confidence": 0.55
  }'

# Link error -> root cause -> solutions
curl -s -X POST http://127.0.0.1:3030/relate \
  -H "Content-Type: application/json" \
  -d '{"from":"ERR:connection_pool_exhausted","relationship":"caused_by","to":"CAUSE:pg_max_connections_exceeded","confidence":0.75}'

curl -s -X POST http://127.0.0.1:3030/relate \
  -H "Content-Type: application/json" \
  -d '{"from":"CAUSE:pg_max_connections_exceeded","relationship":"resolved_by","to":"FIX:increase_pg_max_connections","confidence":0.60}'

curl -s -X POST http://127.0.0.1:3030/relate \
  -H "Content-Type: application/json" \
  -d '{"from":"CAUSE:pg_max_connections_exceeded","relationship":"resolved_by","to":"FIX:add_pgbouncer","confidence":0.55}'

# Link error to the affected service
curl -s -X POST http://127.0.0.1:3030/relate \
  -H "Content-Type: application/json" \
  -d '{"from":"ERR:connection_pool_exhausted","relationship":"affects","to":"postgresql","confidence":0.90}'
```

#### Step 5: Search for the error during an incident

An on-call engineer sees the error message and searches for it:

```bash
curl -s -X POST http://127.0.0.1:3030/search \
  -H "Content-Type: application/json" \
  -d '{"query":"connection pool exhausted","limit":5}'
```

Expected output:

```json
{
  "results": [
    {
      "node_id": 7,
      "label": "ERR:connection_pool_exhausted",
      "confidence": 0.9,
      "score": 2.847,
      "depth": null
    }
  ],
  "total": 1
}
```

#### Step 6: Traverse from the error to find solutions

```bash
curl -s -X POST http://127.0.0.1:3030/query \
  -H "Content-Type: application/json" \
  -d '{"start":"ERR:connection_pool_exhausted","depth":2,"min_confidence":0.0}'
```

Expected output — the traversal reaches the root cause and both solutions:

```json
{
  "nodes": [
    {"label": "ERR:connection_pool_exhausted",     "confidence": 0.9,  "depth": 0},
    {"label": "CAUSE:pg_max_connections_exceeded", "confidence": 0.75, "depth": 1},
    {"label": "postgresql",                        "confidence": 0.95, "depth": 1},
    {"label": "FIX:increase_pg_max_connections",   "confidence": 0.60, "depth": 2},
    {"label": "FIX:add_pgbouncer",                 "confidence": 0.55, "depth": 2}
  ],
  "edges": [...]
}
```

#### Step 7: Apply the fix — reinforce what worked

The engineer increases `max_connections`. The error clears. They confirm the fix worked:

```bash
# Reinforce with confirmation (source present = confirmation boost of +0.10)
curl -s -X POST http://127.0.0.1:3030/learn/reinforce \
  -H "Content-Type: application/json" \
  -d '{"entity":"FIX:increase_pg_max_connections","source":"on-call-alice"}'
```

Expected output:

```json
{"entity":"FIX:increase_pg_max_connections","new_confidence":0.7}
```

Confidence went from 0.60 to 0.70 (confirmation boost of +0.10). Reinforce again when the second engineer validates the runbook:

```bash
curl -s -X POST http://127.0.0.1:3030/learn/reinforce \
  -H "Content-Type: application/json" \
  -d '{"entity":"FIX:increase_pg_max_connections","source":"on-call-bob"}'
```

Expected output:

```json
{"entity":"FIX:increase_pg_max_connections","new_confidence":0.8}
```

#### Step 8: Mark a wrong diagnosis — correct it

Suppose someone initially guessed the problem was a network issue. That diagnosis was stored at low confidence and needs to be corrected:

```bash
curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{"entity":"CAUSE:network_partition","type":"root_cause","confidence":0.40}'

curl -s -X POST http://127.0.0.1:3030/relate \
  -H "Content-Type: application/json" \
  -d '{"from":"ERR:connection_pool_exhausted","relationship":"caused_by","to":"CAUSE:network_partition","confidence":0.40}'

# Now correct the wrong diagnosis
curl -s -X POST http://127.0.0.1:3030/learn/correct \
  -H "Content-Type: application/json" \
  -d '{"entity":"CAUSE:network_partition","reason":"post-mortem confirmed resource exhaustion, not network","source":"post-mortem"}'
```

Expected output — correction propagates to neighbors:

```json
{
  "corrected": "CAUSE:network_partition",
  "propagated_to": ["ERR:connection_pool_exhausted"]
}
```

The `CAUSE:network_partition` node receives a contradiction penalty (-0.20), dropping from 0.40 to 0.20. The error node that pointed to it also gets a reduced confidence signal propagated.

#### Step 9: Search critical open issues

```bash
curl -s -X POST http://127.0.0.1:3030/search \
  -H "Content-Type: application/json" \
  -d '{"query":"prop:severity=critical","limit":10}'
```

Expected output:

```json
{
  "results": [
    {"label": "ERR:connection_pool_exhausted", "confidence": 0.9, "score": 1.0, "depth": null}
  ],
  "total": 1
}
```

#### Step 10: Use inference to propagate impact

Define a rule that says: if a server runs a service, and that service has an error, then the server is affected. Submit it via `/learn/derive`:

```bash
curl -s -X POST http://127.0.0.1:3030/learn/derive \
  -H "Content-Type: application/json" \
  -d '{
    "rules": [
      "rule server_affected_by_service_error\nwhen edge(S, \"runs_on\", SVC)\nwhen edge(ERR, \"affects\", SVC)\nthen edge(ERR, \"affects\", S, min(e1, e2))"
    ]
  }'
```

Expected output:

```json
{
  "rules_evaluated": 1,
  "rules_fired": 1,
  "edges_created": 1,
  "flags_raised": 0
}
```

One new edge was created: `ERR:connection_pool_exhausted -[affects]-> db-primary-01`. The confidence of that edge is `min(runs_on confidence, affects confidence)` — the weakest link in the chain.

#### Step 11: Simulate decay for old issues

After 90 days of inactivity, apply decay to show stale knowledge fading:

```bash
# In production this would run on a scheduled basis
curl -s -X POST http://127.0.0.1:3030/learn/decay \
  -H "Content-Type: application/json" \
  -d '{}'
```

Expected output:

```json
{"nodes_decayed": 12}
```

All 12 nodes in the graph had their confidence multiplied by the time-elapsed decay factor. Nodes not accessed recently lose confidence faster. At confidence below 0.20 they are flagged for archival; below 0.10 they are candidates for garbage collection.

### Key Takeaways

- The error -> root cause -> solution chain is a natural graph pattern. Depth-2 traversal from any error node immediately surfaces candidate fixes.
- Confirmation reinforcement (+0.10) versus access reinforcement (+0.02) means engineer-confirmed fixes rise faster than passively accessed ones.
- The correction endpoint propagates a confidence penalty to graph neighbors, so wrong diagnoses do not silently remain at their original confidence.
- Inference rules let you derive "server is affected" edges without storing them manually for every combination of server and service.
- Decay is a scheduled operation — call it daily via a cron job or scheduled task to keep confidence scores reflecting recency.
