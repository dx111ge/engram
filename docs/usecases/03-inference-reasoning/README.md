# Use Case 3: Inference & Reasoning -- Vulnerability Propagation in a Service Graph

### Overview

Engram's rule engine and backward chaining let you derive new facts from existing knowledge and prove transitive relationships. This walkthrough builds a microservices dependency graph and demonstrates how engram can automatically propagate vulnerability alerts, detect SLA mismatches, and prove multi-hop dependencies -- all without writing application code.

**What this demonstrates:**

- Backward chaining via graph traversal (prove transitive dependencies)
- Forward chaining with the rule engine (`/learn/derive`)
- Iterative transitive closure (running rules multiple rounds to propagate deeper)
- Vulnerability flagging with property-based rule conditions
- SLA mismatch detection across dependency chains
- Push-based rules that auto-fire on mutations
- Full explainability via `/explain`

**What requires external tools:**

- Python script to orchestrate the demo (calls the HTTP API)

### Prerequisites

- `engram` binary on your PATH (or `./target/release/engram`)
- Python 3.9+ with `requests` installed

### Files

```
03-inference-reasoning/
  README.md              # This file
  inference_demo.py      # Full demo script
```

### Step-by-Step

#### Step 1: Start the engram server

```bash
engram serve reasoning.brain 127.0.0.1:3030
```

#### Step 2: Run the demo

```bash
python inference_demo.py
```

### What Happens

#### Phase 1: Build the Service Graph

The script stores 14 nodes (10 services/libraries + 4 infrastructure) and 19 `depends_on` edges:

```
frontend -> api-gateway -> user-service -> PostgreSQL
                        -> order-service -> payment-service -> notification-svc
                                         -> logging-lib
                                         -> PostgreSQL
```

Each node has properties: team, language, SLA tier, version.

#### Phase 2: Backward Chaining -- Prove Transitive Dependencies

Question: "Does frontend transitively depend on PostgreSQL?"

Using outgoing traversal at depth 4:

```bash
curl -s -X POST http://127.0.0.1:3030/query \
  -H "Content-Type: application/json" \
  -d '{"start": "frontend", "depth": 4, "direction": "out"}'
```

Result: **13 nodes reachable**. PostgreSQL found at **depth 3** via the chain:
```
frontend -> api-gateway -> user-service -> PostgreSQL
```

Similarly, `auth-lib` is not a direct dependency of `frontend`, but is reachable transitively at depth 3.

#### Phase 3: Forward Chaining -- Derive Transitive Dependencies

The transitive dependency rule:

```
rule transitive_dependency
when edge(A, "depends_on", B)
when edge(B, "depends_on", C)
then edge(A, "depends_on", C, min(e1, e2))
```

```bash
curl -s -X POST http://127.0.0.1:3030/learn/derive \
  -H "Content-Type: application/json" \
  -d '{"rules": ["rule transitive_dependency\nwhen edge(A, \"depends_on\", B)\nwhen edge(B, \"depends_on\", C)\nthen edge(A, \"depends_on\", C, min(e1, e2))"]}'
```

The engine runs to **fixed point** automatically -- it keeps re-evaluating until no new facts are produced. A single call derives the full transitive closure.

Result: **14 nodes, 41 edges** (+22 derived). Frontend now has direct `depends_on` edges to all 12 transitive dependencies including PostgreSQL, Redis, auth-lib, and Kafka.

```
frontend connects to: api-gateway, Elasticsearch, user-service, order-service,
  auth-lib, logging-lib, PostgreSQL, Redis, payment-service, notification-svc,
  json-parser, Kafka
```

The `min(e1, e2)` confidence expression means derived edges carry the minimum confidence of their chain -- a natural "weakest link" model.

#### Phase 4: Vulnerability Propagation

Inject a CVE into `logging-lib`:

```bash
curl -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{"entity": "logging-lib", "properties": {"vulnerability": "CVE-2024-1234", "severity": "critical"}}'
```

Then run the vulnerability propagation rule:

```
rule vuln_propagation
when edge(service, "depends_on", dep)
when prop(dep, "vulnerability", "CVE-2024-1234")
then flag(service, "depends on vulnerable component: CVE-2024-1234")
```

Result: **4 services flagged**:
```
FLAGGED: frontend -- depends on vulnerable component: CVE-2024-1234
FLAGGED: api-gateway -- depends on vulnerable component: CVE-2024-1234
FLAGGED: user-service -- depends on vulnerable component: CVE-2024-1234
FLAGGED: order-service -- depends on vulnerable component: CVE-2024-1234
```

Because the transitive closure already created direct `depends_on` edges from frontend and api-gateway to logging-lib, the vulnerability rule catches them all in a single pass.

**Blast radius** (incoming traversal from logging-lib): 4 services affected.

#### Phase 5: SLA Mismatch Detection

Load a persistent rule that fires automatically on mutations:

```
rule sla_mismatch
when edge(critical, "depends_on", dep)
when prop(critical, "sla", "tier-1")
when prop(dep, "sla", "tier-3")
then flag(critical, "tier-1 service depends on tier-3 dependency")
```

Result: **3 SLA mismatches found**:
```
MISMATCH: frontend (tier-1) depends on tier-3 service
MISMATCH: api-gateway (tier-1) depends on tier-3 service
MISMATCH: payment-service (tier-1) depends on tier-3 service
```

These are tier-1 services that (transitively) depend on `notification-svc` or `analytics-svc`, both tier-3.

#### Phase 6: Explainability

```bash
curl -s http://127.0.0.1:3030/explain/payment-service
```

Returns full provenance: confidence, properties (including `_flag`), edges, and co-occurrences.

```json
{
  "confidence": 0.95,
  "properties": {
    "language": "Java",
    "team": "payments",
    "sla": "tier-1",
    "_flag": "tier-1 service depends on tier-3 dependency"
  }
}
```

### Rule Syntax Reference

```
rule <name>
when edge(<from_var>, "<relationship>", <to_var>)
when prop(<node_var>, "<key>", "<value>")
when confidence(<var>, "<op>", <threshold>)
then edge(<from_var>, "<rel>", <to_var>, <confidence_expr>)
then prop(<node_var>, "<key>", "<value>")
then flag(<node_var>, "<reason>")
```

**Confidence expressions**: `min(e1, e2)`, `product(e1, e2)`, or a literal like `0.75`.

**Operators** for confidence conditions: `>`, `>=`, `<`, `<=`.

### Key Takeaways

- **Forward chaining runs to fixed point.** The engine automatically re-evaluates until no new facts are derived. A single call to `/learn/derive` turned 19 direct edges into 41 (22 derived).
- **Rules compose.** The transitive closure rule creates edges that the vulnerability rule then matches. No coordination needed -- the graph is the shared state.
- **Confidence propagates naturally.** Derived edges carry `min(e1, e2)` confidence, so a chain through a low-confidence link reduces the derived confidence. This models real-world uncertainty.
- **Flag is non-destructive.** The `flag` action sets a `_flag` property that can be queried, searched, or cleared. It doesn't modify confidence or edges.
- **Blast radius is a query.** Incoming traversal from a vulnerable component shows all affected services: `{"start": "logging-lib", "depth": 3, "direction": "in"}`.
- **14 nodes, 41 edges.** From 19 hand-written edges, the rule engine derived 22 more. The graph becomes denser and more queryable without manual effort.
