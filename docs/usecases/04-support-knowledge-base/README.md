# Use Case 4: Building a Support Knowledge Base

### Overview

IT support teams accumulate knowledge about infrastructure problems -- which error messages mean what, which servers are affected, what the root causes are, and which fixes worked. Engram stores this as a graph: errors link to root causes, root causes link to solutions, and confidence scores reflect which fixes are proven versus guesses.

This walkthrough builds a support knowledge base for a fictional e-commerce platform. It shows the full lifecycle from "we see an error" to "we have a confirmed fix" to "old fixes lose confidence over time."

**What this demonstrates:**

- Storing servers, services, error patterns, root causes, and solutions as typed nodes
- Error -> root cause -> solution graph traversal (depth-2 from any error)
- Property-based filtering (`prop:severity=critical`, `prop:status=open`)
- Confidence lifecycle: reinforce working fixes, correct wrong diagnoses, decay stale knowledge
- Inference rules to propagate impact across the infrastructure graph
- Full explainability via `/explain`

**What requires external tools:**

- Python script to orchestrate the demo (calls the HTTP API)

### Prerequisites

- `engram` binary on your PATH (or `./target/release/engram`)
- Python 3.9+ with `requests` installed

### Files

```
04-support-knowledge-base/
  README.md                # This file
  support_kb_demo.py       # Full demo script
```

### Step-by-Step

#### Step 1: Start the engram server

```bash
engram serve support.brain 127.0.0.1:3030
```

#### Step 2: Run the demo

```bash
python support_kb_demo.py
```

### What Happens

#### Phase 1: Infrastructure & Services

The script stores 12 nodes: 6 servers and 6 services (checkout-api, user-api, search-api, postgresql, redis, rabbitmq). Servers are linked to the services they run, and services are linked by dependency edges.

#### Phase 2: Error Patterns, Root Causes, Solutions

Three error patterns are stored, each with a structured chain:

```
ERR:connection_pool_exhausted (conf=0.90, severity=critical)
  -[caused_by]-> CAUSE:pg_max_connections (conf=0.75)
    -[resolved_by]-> FIX:increase_max_connections (conf=0.60)
    -[resolved_by]-> FIX:add_pgbouncer (conf=0.55)
  -[affects]-> postgresql

ERR:redis_oom (conf=0.90, severity=high)
  -[caused_by]-> CAUSE:redis_memory_full (conf=0.80)
    -[resolved_by]-> FIX:redis_eviction_policy (conf=0.85, verified=true)
    -[resolved_by]-> FIX:redis_scale_memory (conf=0.50)
  -[affects]-> redis

ERR:slow_queries (conf=0.85, severity=medium)
  -[caused_by]-> CAUSE:missing_index (conf=0.70)
    -[resolved_by]-> FIX:add_session_index (conf=0.65)
  -[affects]-> postgresql, checkout-api
```

After phase 2: **23 nodes, 24 edges**.

#### Phase 3: Incident Response

**Search**: An on-call engineer sees a PostgreSQL error and searches:

```bash
curl -s -X POST http://127.0.0.1:3030/search \
  -H "Content-Type: application/json" \
  -d '{"query": "connection pool exhausted", "limit": 5}'
```

Returns `ERR:connection_pool_exhausted`.

**Traverse**: Depth-2 traversal from the error surfaces 11 nodes including the root cause, both solutions, affected servers, and dependent services.

**Filter**: `prop:severity=critical` returns only the connection pool error. `prop:status=open` returns both open errors.

#### Phase 4: Learning Lifecycle

**Reinforce** -- the fix worked, two engineers confirm:

```bash
# Confirmation boost: +0.10 each (requires source)
curl -s -X POST http://127.0.0.1:3030/learn/reinforce \
  -H "Content-Type: application/json" \
  -d '{"entity": "FIX:increase_max_connections", "source": "on-call-alice"}'
```

Confidence progression: **0.60 -> 0.70 -> 0.80 -> 0.82** (two confirmations + one access boost).

**Correct** -- wrong diagnosis discarded:

```bash
curl -s -X POST http://127.0.0.1:3030/learn/correct \
  -H "Content-Type: application/json" \
  -d '{"entity": "CAUSE:network_partition", "reason": "postmortem confirmed resource exhaustion"}'
```

Confidence: **0.40 -> 0.00**. The correction zeroes the node's confidence.

**Decay** -- call periodically to let stale knowledge fade:

```bash
curl -s -X POST http://127.0.0.1:3030/learn/decay
```

Confidence multiplied by 0.999 per elapsed day. Nodes below 0.10 become archival candidates.

#### Phase 5: Inference -- Impact Propagation

Two rules derive new `affects` edges:

```
rule server_affected
when edge(server, "runs", service)
when edge(err, "affects", service)
then edge(err, "affects", server, min(e1, e2))
```

```
rule dependency_impact
when edge(svc, "depends_on", dep)
when edge(err, "affects", dep)
then edge(err, "affects", svc, min(e1, e2))
```

After inference: **24 nodes, 38 edges** (+13 derived).

**Blast radius** of `ERR:connection_pool_exhausted`:
```
postgresql, db-primary-01, db-replica-01, checkout-api, user-api, search-api
```

**Blast radius** of `ERR:redis_oom`:
```
redis, cache-01, checkout-api, user-api
```

#### Phase 6: Explainability

```bash
curl -s http://127.0.0.1:3030/explain/FIX:increase_max_connections
```

Returns confidence (0.82 after two confirmations), properties (action, risk, verified status), and provenance.

### Key Takeaways

- **Error -> root cause -> solution** is a natural graph pattern. Depth-2 traversal from any error node immediately surfaces candidate fixes, ranked by confidence.
- **Confirmation reinforcement** (+0.10) versus **access reinforcement** (+0.02) means engineer-confirmed fixes rise faster than passively accessed ones.
- **Correction** zeroes the confidence of wrong diagnoses and can propagate penalties to connected nodes.
- **Property filters** (`prop:severity=critical`, `prop:status=open`) let on-call engineers quickly find relevant issues without traversing the full graph.
- **Inference rules** propagate impact automatically: "if a server runs a service with an error, the server is affected." One rule, zero manual work per incident.
- **Decay** keeps the knowledge base fresh. Solutions that haven't been confirmed recently fade, making room for newer approaches.
