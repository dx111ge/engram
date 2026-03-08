# Using Engram from Python

Engram is a Rust binary that exposes a JSON REST API. Python integrates with it
through plain HTTP — no SDK required. This guide covers everything from a
minimal client class to bulk import, LangChain tool integration, and subprocess
scripting.

**What you will be able to do after this guide:**

- Store structured knowledge with confidence scores
- Create and traverse typed relationships between entities
- Search with BM25 full-text and boolean filters
- Ask natural-language questions and make natural-language statements
- Trigger reinforcement, correction, decay, and forward-chaining rules
- Bulk-import knowledge from CSV and JSON
- Wire engram up as a tool in a LangChain agent
- Drive engram from subprocesses for quick scripting

**Prerequisites:**

- `engram` binary on your `PATH` (build with `cargo build --release`)
- Python 3.9 or later
- `pip install requests` (all HTTP examples)
- `pip install langchain langchain-openai` (LangChain section only)

**Estimated time:** 45–60 minutes

---

## Table of Contents

1. [Prerequisites and Server Startup](#1-prerequisites-and-server-startup)
2. [Quick Start — the EngramClient class](#2-quick-start--the-engramclient-class)
3. [Storing Knowledge](#3-storing-knowledge)
4. [Creating Relationships](#4-creating-relationships)
5. [Querying the Graph](#5-querying-the-graph)
6. [Searching](#6-searching)
7. [Natural Language](#7-natural-language)
8. [Learning Operations](#8-learning-operations)
9. [Bulk Import](#9-bulk-import)
10. [Integration with LangChain](#10-integration-with-langchain)
11. [CLI Wrapper](#11-cli-wrapper)
12. [Error Handling](#12-error-handling)

---

## 1. Prerequisites and Server Startup

### Start the server

```bash
# Uses ./knowledge.brain, listens on 0.0.0.0:3030
engram serve

# Custom brain file and address
engram serve /var/data/myproject.brain 127.0.0.1:8080
```

The server creates the `.brain` file if it does not exist. Leave it running in a
terminal while you work through the examples below.

### Verify it is up

```python
import requests

resp = requests.get("http://localhost:3030/health")
print(resp.json())
# {'status': 'ok', 'version': '0.1.0'}
```

### Install the only dependency

```bash
pip install requests
```

---

## 2. Quick Start — the EngramClient class

All of the sections below use raw `requests` calls to show exactly what goes
over the wire. A thin wrapper class removes the repetition. Both approaches are
shown side-by-side.

```python
"""engram_client.py — minimal wrapper around the Engram REST API."""

import requests
from typing import Any


class EngramError(Exception):
    """Raised when the server returns an error body."""
    def __init__(self, status: int, message: str):
        self.status = status
        self.message = message
        super().__init__(f"HTTP {status}: {message}")


class EngramClient:
    def __init__(self, base_url: str = "http://localhost:3030"):
        self.base = base_url.rstrip("/")
        self.session = requests.Session()
        self.session.headers.update({"Content-Type": "application/json"})

    def _post(self, path: str, body: dict) -> dict:
        resp = self.session.post(f"{self.base}{path}", json=body)
        data = resp.json()
        if "error" in data:
            raise EngramError(resp.status_code, data["error"])
        return data

    def _get(self, path: str) -> dict:
        resp = self.session.get(f"{self.base}{path}")
        data = resp.json()
        if "error" in data:
            raise EngramError(resp.status_code, data["error"])
        return data

    def _delete(self, path: str) -> dict:
        resp = self.session.delete(f"{self.base}{path}")
        data = resp.json()
        if "error" in data:
            raise EngramError(resp.status_code, data["error"])
        return data

    # -- Core graph --

    def store(
        self,
        entity: str,
        *,
        type: str | None = None,
        properties: dict[str, str] | None = None,
        source: str | None = None,
        confidence: float | None = None,
    ) -> dict:
        body: dict[str, Any] = {"entity": entity}
        if type:
            body["type"] = type
        if properties:
            body["properties"] = properties
        if source:
            body["source"] = source
        if confidence is not None:
            body["confidence"] = confidence
        return self._post("/store", body)

    def relate(
        self,
        from_entity: str,
        to_entity: str,
        relationship: str,
        *,
        confidence: float | None = None,
    ) -> dict:
        body: dict[str, Any] = {
            "from": from_entity,
            "to": to_entity,
            "relationship": relationship,
        }
        if confidence is not None:
            body["confidence"] = confidence
        return self._post("/relate", body)

    def batch(
        self,
        *,
        entities: list[dict] | None = None,
        relations: list[dict] | None = None,
        source: str | None = None,
    ) -> dict:
        body: dict[str, Any] = {}
        if entities:
            body["entities"] = entities
        if relations:
            body["relations"] = relations
        if source:
            body["source"] = source
        return self._post("/batch", body)

    def query(
        self,
        start: str,
        *,
        depth: int = 2,
        min_confidence: float = 0.0,
    ) -> dict:
        return self._post("/query", {
            "start": start,
            "depth": depth,
            "min_confidence": min_confidence,
        })

    def search(self, query: str, *, limit: int = 10) -> dict:
        return self._post("/search", {"query": query, "limit": limit})

    def similar(self, text: str, *, limit: int = 10) -> dict:
        return self._post("/similar", {"text": text, "limit": limit})

    def get_node(self, label: str) -> dict:
        return self._get(f"/node/{label}")

    def delete_node(self, label: str) -> dict:
        return self._delete(f"/node/{label}")

    def explain(self, label: str) -> dict:
        return self._get(f"/explain/{label}")

    # -- Natural language --

    def ask(self, question: str) -> dict:
        return self._post("/ask", {"question": question})

    def tell(self, statement: str, *, source: str | None = None) -> dict:
        body: dict[str, Any] = {"statement": statement}
        if source:
            body["source"] = source
        return self._post("/tell", body)

    # -- Learning --

    def reinforce(self, entity: str, *, source: str | None = None) -> dict:
        body: dict[str, Any] = {"entity": entity}
        if source:
            body["source"] = source
        return self._post("/learn/reinforce", body)

    def correct(self, entity: str, reason: str) -> dict:
        return self._post("/learn/correct", {"entity": entity, "reason": reason})

    def decay(self) -> dict:
        return self._post("/learn/decay", {})

    def derive(self, rules: list[str]) -> dict:
        return self._post("/learn/derive", {"rules": rules})

    # -- System --

    def health(self) -> dict:
        return self._get("/health")

    def stats(self) -> dict:
        return self._get("/stats")

    def compute(self) -> dict:
        return self._get("/compute")

    def tools(self) -> list:
        return self._get("/tools")
```

**Smoke test:**

```python
from engram_client import EngramClient

client = EngramClient()
print(client.health())
# {'status': 'ok', 'version': '0.1.0'}

print(client.stats())
# {'nodes': 0, 'edges': 0}
```

---

## 3. Storing Knowledge

### Minimal — entity name only

Every field except `entity` is optional. This is the fastest way to register
that something exists.

```python
import requests

resp = requests.post("http://localhost:3030/store", json={
    "entity": "postgresql"
})
print(resp.json())
# {'node_id': 1, 'label': 'postgresql', 'confidence': 0.75}
```

### With type, properties, source, and confidence

```python
resp = requests.post("http://localhost:3030/store", json={
    "entity": "postgresql",
    "type": "database",
    "properties": {
        "version": "16",
        "role": "primary",
        "host": "db01.internal"
    },
    "source": "sysadmin",
    "confidence": 0.95
})
print(resp.json())
# {'node_id': 1, 'label': 'postgresql', 'confidence': 0.95}
```

**Using the client:**

```python
result = client.store(
    "postgresql",
    type="database",
    properties={"version": "16", "role": "primary", "host": "db01.internal"},
    source="sysadmin",
    confidence=0.95,
)
print(result)
# {'node_id': 1, 'label': 'postgresql', 'confidence': 0.95}
```

### Storing multiple entities

```python
from engram_client import EngramClient

client = EngramClient()

infrastructure = [
    ("postgresql",  "database", {"version": "16", "role": "primary"},   0.95),
    ("redis",       "cache",    {"version": "7.2", "maxmemory": "4gb"},  0.90),
    ("nginx",       "proxy",    {"version": "1.25", "port": "443"},      0.88),
    ("app-server",  "service",  {"language": "python", "port": "8000"},  0.85),
    ("celery",      "worker",   {"concurrency": "8", "queue": "default"},0.80),
]

for entity, etype, props, conf in infrastructure:
    result = client.store(
        entity,
        type=etype,
        properties=props,
        source="infrastructure-audit",
        confidence=conf,
    )
    print(f"stored {result['label']} (node_id={result['node_id']})")

# stored postgresql (node_id=1)
# stored redis (node_id=2)
# stored nginx (node_id=3)
# stored app-server (node_id=4)
# stored celery (node_id=5)
```

### Retrieving a node

```python
resp = requests.get("http://localhost:3030/node/postgresql")
node = resp.json()

print(node["label"])       # postgresql
print(node["confidence"])  # 0.95
print(node["properties"])  # {'version': '16', 'role': 'primary', 'host': 'db01.internal'}
print(node["edges_from"])  # list of outgoing edges
print(node["edges_to"])    # list of incoming edges
```

### Soft-deleting a node

Deletion sets confidence to 0 and records provenance. The node is not physically
removed — history is preserved.

```python
resp = requests.delete("http://localhost:3030/node/redis")
print(resp.json())
# {'deleted': True, 'entity': 'redis'}

# Using the client:
print(client.delete_node("redis"))
# {'deleted': True, 'entity': 'redis'}
```

---

## 4. Creating Relationships

A relationship connects two entities with a typed, directional edge. Both
entities must exist first (store them beforehand).

### Raw request

```python
resp = requests.post("http://localhost:3030/relate", json={
    "from": "app-server",
    "to": "postgresql",
    "relationship": "reads_from",
    "confidence": 0.9
})
print(resp.json())
# {'from': 'app-server', 'to': 'postgresql', 'relationship': 'reads_from', 'edge_slot': 1}
```

### Building a graph of relationships

```python
from engram_client import EngramClient

client = EngramClient()

edges = [
    ("app-server", "postgresql",  "reads_from",   0.90),
    ("app-server", "redis",       "caches_with",  0.85),
    ("app-server", "celery",      "enqueues_to",  0.88),
    ("nginx",      "app-server",  "proxies_to",   0.95),
    ("celery",     "postgresql",  "reads_from",   0.80),
    ("celery",     "redis",       "caches_with",  0.75),
    ("postgresql", "redis",       "replicates_to",0.70),
]

for from_e, to_e, rel, conf in edges:
    result = client.relate(from_e, to_e, rel, confidence=conf)
    print(f"{result['from']} -[{result['relationship']}]-> {result['to']}")

# app-server -[reads_from]-> postgresql
# app-server -[caches_with]-> redis
# app-server -[enqueues_to]-> celery
# nginx -[proxies_to]-> app-server
# celery -[reads_from]-> postgresql
# celery -[caches_with]-> redis
# postgresql -[replicates_to]-> redis
```

### Confidence is not required

When `confidence` is omitted the server assigns a default (typically 0.75).

```python
client.relate("nginx", "app-server", "load_balances")
# {'from': 'nginx', 'to': 'app-server', 'relationship': 'load_balances', 'edge_slot': 8}
```

---

## 5. Querying the Graph

The query endpoint does a breadth-first traversal starting from a node. You
control how many hops to follow and the minimum confidence threshold for
included nodes.

### Raw request

```python
resp = requests.post("http://localhost:3030/query", json={
    "start": "nginx",
    "depth": 2,
    "min_confidence": 0.5
})
result = resp.json()

for node in result["nodes"]:
    print(f"  depth={node['depth']}  {node['label']}  (conf={node['confidence']:.2f})")
# depth=0  nginx       (conf=0.88)
# depth=1  app-server  (conf=0.85)
# depth=2  postgresql  (conf=0.95)
# depth=2  redis       (conf=0.90)
# depth=2  celery      (conf=0.80)
```

### Filtering by confidence

Raising `min_confidence` removes low-certainty nodes from the traversal result.
This is useful when the graph contains speculative data alongside confirmed data.

```python
result = client.query("nginx", depth=3, min_confidence=0.85)

print(f"Total nodes reachable: {len(result['nodes'])}")
for node in result["nodes"]:
    print(f"  {node['label']:20s}  conf={node['confidence']:.2f}  depth={node['depth']}")
```

### Inspecting edges in the result

```python
result = client.query("app-server", depth=1)

print("Edges in traversal:")
for edge in result["edges"]:
    print(f"  {edge['from']} --[{edge['relationship']}]--> {edge['to']}")
```

### Full node detail with explain

`/explain/{label}` returns confidence, properties, co-occurrence statistics, and
all edges. Use this when you want the full provenance picture for one node.

```python
explain = client.explain("postgresql")

print(f"entity:     {explain['entity']}")
print(f"confidence: {explain['confidence']:.2f}")
print(f"properties: {explain['properties']}")
print(f"edges out:  {len(explain['edges_from'])}")
print(f"edges in:   {len(explain['edges_to'])}")
print(f"co-occurs with:")
for hit in explain["cooccurrences"]:
    print(f"  {hit['entity']}  (count={hit['count']})")
```

---

## 6. Searching

Engram has two search modes:

- `/search` — BM25 full-text with boolean filters and field selectors
- `/similar` — semantic similarity (falls back to BM25 if no embedder is
  configured)

### Full-text search

```python
resp = requests.post("http://localhost:3030/search", json={
    "query": "database",
    "limit": 10
})
data = resp.json()

print(f"Found {data['total']} results")
for r in data["results"]:
    print(f"  {r['label']:20s}  score={r['score']:.3f}  conf={r['confidence']:.2f}")
# Found 2 results
#   postgresql           score=1.200  conf=0.95
#   redis                score=0.800  conf=0.90
```

### Boolean and filter syntax

The search engine supports a rich query language. All filters can be combined
with `AND` and `OR`.

```python
# Nodes with confidence above a threshold
client.search("confidence>0.85")

# Nodes of a specific type
client.search("type:database")

# Nodes with a property value
client.search("prop:role=primary")

# Nodes in the active memory tier
client.search("tier:active")

# Combining filters with boolean logic
client.search("type:service AND confidence>0.8")
client.search("type:database OR type:cache")
client.search("prop:version=16 AND confidence>0.9")
```

### Semantic similarity search

```python
resp = requests.post("http://localhost:3030/similar", json={
    "text": "high CPU usage on production server",
    "limit": 5
})
data = resp.json()

for r in data["results"]:
    print(f"  {r['label']:20s}  score={r['score']:.4f}")
```

> Note: `/similar` uses vector embeddings when an ONNX embedder is configured.
> Without one it falls back to BM25, so results are text-based rather than
> semantic. Run `engram reindex` after configuring an embedder.

### Practical search patterns

```python
from engram_client import EngramClient

client = EngramClient()

# Find all services with a known port
services = client.search("type:service AND prop:port", limit=20)
for r in services["results"]:
    print(r["label"])

# Find everything with low confidence — candidates for review
uncertain = client.search("confidence<0.6", limit=50)
print(f"{uncertain['total']} nodes below 0.6 confidence")

# Find nodes with a particular property regardless of type
nginx_nodes = client.search("prop:version=1.25")
```

---

## 7. Natural Language

The `/ask` and `/tell` endpoints provide a rule-based natural-language interface.
This is not an LLM — it is a pattern matcher that handles common English
structures and translates them into graph operations.

### Supported ask patterns

| Pattern | Example | Action |
|---|---|---|
| `What is X?` | "What is postgresql?" | Node lookup with properties and edges |
| `Who is X?` | "Who is alice?" | Node lookup |
| `What does X connect to?` | "What does app-server connect to?" | Outgoing edges |
| `What does X relate to?` | "What does nginx relate to?" | Outgoing edges |
| `What connects to X?` | "What connects to postgresql?" | Incoming edges |
| `How are X and Y related?` | "How are nginx and redis related?" | Path search |
| `Find things like X` | "Find things like database" | Similarity search |
| `Search for X` | "Search for cache" | Full-text search |

### Supported tell patterns

| Pattern | Example | Effect |
|---|---|---|
| `X is a Y` / `X is an Y` | "postgresql is a database" | Store both, add `is_a` edge |
| `X causes Y` | "disk pressure causes OOM" | Store both, add `causes` edge |
| `X depends on Y` | "celery depends on redis" | Store both, add `depends_on` edge |
| `X runs on Y` | "app-server runs on kubernetes" | Store both, add `runs_on` edge |
| `X uses Y` | "app-server uses postgresql" | Store both, add `uses` edge |
| `X has property K = V` | "nginx has property port = 443" | Set property on node |

### Ask examples

```python
import requests

# Look up a node with its properties and edges
resp = requests.post("http://localhost:3030/ask", json={
    "question": "What is postgresql?"
})
data = resp.json()
print(data["interpretation"])
# lookup: postgresql

for r in data["results"]:
    if r.get("relationship"):
        print(f"  -[{r['relationship']}]-> {r['label']}")
    elif r.get("detail"):
        print(f"  property: {r['detail']}")
    else:
        print(f"  entity: {r['label']}  conf={r['confidence']:.2f}")
# entity: postgresql  conf=0.95
# property: version: 16
# property: role: primary
# -[reads_from]-> app-server

# What does a node connect to?
resp = requests.post("http://localhost:3030/ask", json={
    "question": "What does app-server connect to?"
})
data = resp.json()
for r in data["results"]:
    print(f"  -[{r['relationship']}]-> {r['label']}")
# -[reads_from]-> postgresql
# -[caches_with]-> redis
# -[enqueues_to]-> celery

# Path between two nodes
resp = requests.post("http://localhost:3030/ask", json={
    "question": "How are nginx and postgresql related?"
})
print(resp.json()["interpretation"])
# path between nginx and postgresql
```

### Tell examples

```python
# Teach a type relationship
resp = requests.post("http://localhost:3030/tell", json={
    "statement": "postgresql is a relational database",
    "source": "docs-importer"
})
data = resp.json()
print(data["interpretation"])
print(data["actions"])
# postgresql is a type of relational database
# ['stored entity: postgresql', 'stored entity: relational database', 'postgresql -[is_a]-> relational database']

# Teach a causal relationship
resp = requests.post("http://localhost:3030/tell", json={
    "statement": "disk pressure causes OOM killer"
})
data = resp.json()
print(data["interpretation"])
# disk pressure causes OOM killer

# Set a property through natural language
resp = requests.post("http://localhost:3030/tell", json={
    "statement": "nginx has property port = 443"
})
print(resp.json()["actions"])
# ['set nginx.port = 443']
```

### Using the client

```python
from engram_client import EngramClient

client = EngramClient()

# Ask a question
result = client.ask("What does celery connect to?")
print(result["interpretation"])
for r in result["results"]:
    print(f"  -[{r['relationship']}]-> {r['label']}")

# Make a statement
result = client.tell(
    "redis depends on network",
    source="architecture-review"
)
print(result["actions"])
```

---

## 8. Learning Operations

Engram's learning engine updates confidence values over time. These four
operations let you drive that process explicitly from Python.

### Reinforce — boost confidence

Two modes:

- **Access boost** (`+0.02`): called without a source, records that something
  was referenced.
- **Confirmation boost** (`+0.10`): called with a source, records that a second
  party has confirmed the fact.

```python
# Access boost — no source
resp = requests.post("http://localhost:3030/learn/reinforce", json={
    "entity": "postgresql"
})
print(resp.json())
# {'entity': 'postgresql', 'new_confidence': 0.97}

# Confirmation boost — with source
resp = requests.post("http://localhost:3030/learn/reinforce", json={
    "entity": "postgresql",
    "source": "monitoring-system"
})
print(resp.json())
# {'entity': 'postgresql', 'new_confidence': 1.0}
```

### Correct — mark a fact as wrong

Zeroes the node's confidence and propagates distrust to its neighbors (0.5
damping factor per hop, up to 3 hops by default).

```python
resp = requests.post("http://localhost:3030/learn/correct", json={
    "entity": "redis",
    "reason": "decommissioned in sprint 42"
})
data = resp.json()
print(data["corrected"])         # redis
print(data["propagated_to"])     # ['app-server', 'celery', ...]
```

### Decay — apply time-based confidence decay

Applies a `0.999` per-day decay factor to all nodes. Nodes that fall below
`0.10` confidence become archival candidates. Call this on a schedule (daily
cron, for example).

```python
resp = requests.post("http://localhost:3030/learn/decay")
print(resp.json())
# {'nodes_decayed': 47}
```

### Derive — run forward-chaining inference rules

Rules follow the pattern:

```
rule <name>
when edge(A, "<relationship>", B)
when edge(B, "<relationship>", C)
then edge(A, "<relationship>", C, min(e1, e2))
```

The `min(e1, e2)` expression takes the lower of the two edge confidences for
the derived edge, ensuring derived facts are never more certain than their
premises.

```python
# Derive transitive "is_a" relationships
resp = requests.post("http://localhost:3030/learn/derive", json={
    "rules": [
        "rule transitive_is_a\n"
        "when edge(A, \"is_a\", B)\n"
        "when edge(B, \"is_a\", C)\n"
        "then edge(A, \"is_a\", C, min(e1, e2))"
    ]
})
data = resp.json()
print(f"rules evaluated: {data['rules_evaluated']}")
print(f"rules fired:     {data['rules_fired']}")
print(f"edges created:   {data['edges_created']}")
# rules evaluated: 1
# rules fired:     1
# edges created:   3
```

### Scheduled learning loop

A realistic pattern is to run reinforcement for recently accessed entities and
decay for stale ones on a timer.

```python
import time
import requests

BASE = "http://localhost:3030"


def reinforce_accessed(entities: list[str], source: str) -> None:
    for entity in entities:
        requests.post(f"{BASE}/learn/reinforce", json={
            "entity": entity,
            "source": source,
        })


def run_daily_maintenance() -> dict:
    """Call once per day from a scheduler."""
    decay_resp = requests.post(f"{BASE}/learn/decay").json()

    derive_resp = requests.post(f"{BASE}/learn/derive", json={
        "rules": [
            "rule transitive_is_a\n"
            "when edge(A, \"is_a\", B)\n"
            "when edge(B, \"is_a\", C)\n"
            "then edge(A, \"is_a\", C, min(e1, e2))"
        ]
    }).json()

    return {
        "nodes_decayed": decay_resp["nodes_decayed"],
        "edges_derived": derive_resp["edges_created"],
    }


# Run the maintenance cycle
report = run_daily_maintenance()
print(report)
# {'nodes_decayed': 47, 'edges_derived': 3}
```

---

## 9. Bulk Import

### Using the batch endpoint (recommended)

The `/batch` endpoint accepts arrays of entities and relationships in a single
request. Everything runs under one write lock with one deferred checkpoint --
dramatically faster than individual `/store` + `/relate` calls.

```python
client = EngramClient()

result = client.batch(
    entities=[
        {"entity": "kafka",           "type": "message-broker", "properties": {"version": "3.6"}},
        {"entity": "zookeeper",       "type": "coordinator",     "properties": {"version": "3.8"}},
        {"entity": "schema-registry", "type": "service",         "confidence": 0.85},
    ],
    relations=[
        {"from": "kafka",     "to": "zookeeper",       "relationship": "depends_on", "confidence": 0.95},
        {"from": "kafka",     "to": "schema-registry",  "relationship": "uses",       "confidence": 0.90},
    ],
    source="platform-team",
)
print(result)
# {'nodes_stored': 3, 'edges_created': 2, 'errors': None}
```

### Import entities and edges from JSON

```python
import json

client = EngramClient()


def import_graph_json(path: str, *, source: str = "json-import") -> dict:
    """
    Import from a JSON file with the structure:
    {
        "entities": [
            {"entity": "...", "type": "...", "properties": {...}, "confidence": 0.9}
        ],
        "relationships": [
            {"from": "...", "to": "...", "relationship": "...", "confidence": 0.8}
        ]
    }

    Uses the /batch endpoint for single-request bulk ingestion.
    """
    with open(path) as f:
        data = json.load(f)

    return client.batch(
        entities=data.get("entities"),
        relations=data.get("relationships"),
        source=source,
    )


result = import_graph_json("/tmp/platform.json", source="platform-team")
print(result)
# {'nodes_stored': 12, 'edges_created': 18, 'errors': None}
```

### Import from CSV

```python
import csv
import requests

BASE = "http://localhost:3030"


def import_entities_csv(
    path: str,
    *,
    entity_col: str = "name",
    type_col: str | None = "type",
    confidence_col: str | None = "confidence",
    source: str = "csv-import",
    default_confidence: float = 0.75,
) -> dict:
    """
    Import nodes from a CSV file.

    Expected CSV columns (adjust col names with parameters):
        name, type, confidence, [any other columns become properties]

    Example CSV:
        name,type,confidence,version,region
        postgresql,database,0.95,16,us-east-1
        redis,cache,0.90,7.2,us-east-1
    """
    ok = 0
    err = 0

    with open(path, newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            entity = row.pop(entity_col, "").strip()
            if not entity:
                continue

            etype = row.pop(type_col, None) if type_col else None
            conf_raw = row.pop(confidence_col, None) if confidence_col else None

            try:
                confidence = float(conf_raw) if conf_raw else default_confidence
            except ValueError:
                confidence = default_confidence

            # Remaining columns become string properties
            properties = {k: v for k, v in row.items() if v}

            body = {
                "entity": entity,
                "source": source,
                "confidence": confidence,
            }
            if etype:
                body["type"] = etype
            if properties:
                body["properties"] = properties

            r = requests.post(f"{BASE}/store", json=body).json()
            if "error" in r:
                print(f"  WARN: {entity}: {r['error']}")
                err += 1
            else:
                ok += 1

    return {"stored": ok, "failed": err}


result = import_entities_csv(
    "/tmp/servers.csv",
    entity_col="name",
    type_col="type",
    confidence_col="confidence",
    source="cmdb-export",
)
print(result)
# {'stored': 34, 'failed': 0}
```

### Import relationships from CSV

```python
def import_relationships_csv(
    path: str,
    *,
    from_col: str = "from",
    to_col: str = "to",
    rel_col: str = "relationship",
    confidence_col: str | None = "confidence",
    source: str = "csv-import",
    default_confidence: float = 0.75,
) -> dict:
    """
    Import edges from a CSV file.

    Example CSV:
        from,to,relationship,confidence
        app-server,postgresql,reads_from,0.9
        app-server,redis,caches_with,0.85
    """
    ok = 0
    err = 0

    with open(path, newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            from_e = row.get(from_col, "").strip()
            to_e = row.get(to_col, "").strip()
            rel = row.get(rel_col, "").strip()
            if not from_e or not to_e or not rel:
                continue

            conf_raw = row.get(confidence_col, "") if confidence_col else ""
            try:
                confidence = float(conf_raw) if conf_raw else default_confidence
            except ValueError:
                confidence = default_confidence

            body = {
                "from": from_e,
                "to": to_e,
                "relationship": rel,
                "confidence": confidence,
            }
            r = requests.post(f"{BASE}/relate", json=body).json()
            if "error" in r:
                print(f"  WARN: {from_e} -[{rel}]-> {to_e}: {r['error']}")
                err += 1
            else:
                ok += 1

    return {"stored": ok, "failed": err}
```

---

## 10. Integration with LangChain

Engram exposes OpenAI-compatible tool definitions at `GET /tools`. You can use
these directly with LangChain's `StructuredTool` to give an LLM agent persistent
memory that survives between sessions.

### Fetch the tool definitions

```python
import requests

tools_def = requests.get("http://localhost:3030/tools").json()
print(f"Available tools: {len(tools_def)}")
for t in tools_def:
    print(f"  {t['name']:25s}  {t['description'][:60]}")
# Available tools: 5
#   engram_store              Store a new fact or entity in the knowledge graph
#   engram_relate             Create a relationship between two entities
#   engram_query              Query the knowledge graph with traversal
#   engram_search             Full-text keyword search across all stored knowl...
#   engram_explain            Explain how a fact was derived, its confidence, ...
```

### Wrapping engram endpoints as LangChain tools

```python
import requests
from langchain.tools import StructuredTool
from langchain_openai import ChatOpenAI
from langchain.agents import AgentExecutor, create_tool_calling_agent
from langchain.prompts import ChatPromptTemplate, MessagesPlaceholder
from pydantic import BaseModel, Field

BASE = "http://localhost:3030"


# -- Input schemas --

class StoreInput(BaseModel):
    entity: str = Field(description="Name or label of the entity to store")
    type: str | None = Field(default=None, description="Entity type (e.g. server, person, concept)")
    properties: dict[str, str] | None = Field(default=None, description="Key-value properties")
    source: str | None = Field(default=None, description="Source of this knowledge")
    confidence: float | None = Field(default=None, description="Certainty 0.0-1.0")


class RelateInput(BaseModel):
    from_entity: str = Field(description="Source entity label")
    to_entity: str = Field(description="Target entity label")
    relationship: str = Field(description="Relationship type (e.g. depends_on, is_a)")
    confidence: float | None = Field(default=None, description="Certainty 0.0-1.0")


class QueryInput(BaseModel):
    start: str = Field(description="Starting entity for graph traversal")
    depth: int = Field(default=2, description="Number of hops to traverse")
    min_confidence: float = Field(default=0.0, description="Minimum node confidence to include")


class SearchInput(BaseModel):
    query: str = Field(description="Search query. Supports boolean: AND, OR, confidence>0.8, type:database")
    limit: int = Field(default=10, description="Maximum results to return")


class AskInput(BaseModel):
    question: str = Field(description="Natural language question about the knowledge graph")


# -- Tool functions --

def engram_store(
    entity: str,
    type: str | None = None,
    properties: dict[str, str] | None = None,
    source: str | None = None,
    confidence: float | None = None,
) -> str:
    body = {"entity": entity, "source": source or "langchain-agent"}
    if type:
        body["type"] = type
    if properties:
        body["properties"] = properties
    if confidence is not None:
        body["confidence"] = confidence
    r = requests.post(f"{BASE}/store", json=body).json()
    if "error" in r:
        return f"Error: {r['error']}"
    return f"Stored '{r['label']}' (node_id={r['node_id']}, confidence={r['confidence']:.2f})"


def engram_relate(
    from_entity: str,
    to_entity: str,
    relationship: str,
    confidence: float | None = None,
) -> str:
    body = {"from": from_entity, "to": to_entity, "relationship": relationship}
    if confidence is not None:
        body["confidence"] = confidence
    r = requests.post(f"{BASE}/relate", json=body).json()
    if "error" in r:
        return f"Error: {r['error']}"
    return f"Created edge: {r['from']} -[{r['relationship']}]-> {r['to']}"


def engram_query(start: str, depth: int = 2, min_confidence: float = 0.0) -> str:
    r = requests.post(f"{BASE}/query", json={
        "start": start,
        "depth": depth,
        "min_confidence": min_confidence,
    }).json()
    if "error" in r:
        return f"Error: {r['error']}"
    lines = [f"Graph from '{start}' (depth={depth}):"]
    for node in r["nodes"]:
        lines.append(f"  [{node['depth']}] {node['label']} (conf={node['confidence']:.2f})")
    return "\n".join(lines)


def engram_search(query: str, limit: int = 10) -> str:
    r = requests.post(f"{BASE}/search", json={"query": query, "limit": limit}).json()
    if "error" in r:
        return f"Error: {r['error']}"
    if not r["results"]:
        return "No results found."
    lines = [f"Search results for '{query}':"]
    for hit in r["results"]:
        lines.append(f"  {hit['label']} (score={hit['score']:.3f}, conf={hit['confidence']:.2f})")
    return "\n".join(lines)


def engram_ask(question: str) -> str:
    r = requests.post(f"{BASE}/ask", json={"question": question}).json()
    if "error" in r:
        return f"Error: {r['error']}"
    lines = [f"Interpretation: {r['interpretation']}"]
    for result in r["results"]:
        if result.get("relationship"):
            lines.append(f"  -[{result['relationship']}]-> {result['label']}")
        elif result.get("detail"):
            lines.append(f"  {result['detail']}")
        else:
            lines.append(f"  {result['label']} (conf={result['confidence']:.2f})")
    return "\n".join(lines)


# -- Assemble LangChain tools --

tools = [
    StructuredTool.from_function(
        func=engram_store,
        name="engram_store",
        description="Store a fact or entity in the knowledge graph with optional type, properties, and confidence.",
        args_schema=StoreInput,
    ),
    StructuredTool.from_function(
        func=engram_relate,
        name="engram_relate",
        description="Create a typed relationship between two entities in the knowledge graph.",
        args_schema=RelateInput,
    ),
    StructuredTool.from_function(
        func=engram_query,
        name="engram_query",
        description="Traverse the knowledge graph from a starting entity. Returns connected nodes.",
        args_schema=QueryInput,
    ),
    StructuredTool.from_function(
        func=engram_search,
        name="engram_search",
        description="Search the knowledge graph using text, filters, or boolean queries.",
        args_schema=SearchInput,
    ),
    StructuredTool.from_function(
        func=engram_ask,
        name="engram_ask",
        description="Ask a natural language question about the knowledge graph.",
        args_schema=AskInput,
    ),
]


# -- Build the agent --

llm = ChatOpenAI(model="gpt-4o-mini", temperature=0)

prompt = ChatPromptTemplate.from_messages([
    ("system", (
        "You are an infrastructure assistant with access to a knowledge graph. "
        "Use the engram tools to store facts, create relationships, and answer "
        "questions about the infrastructure. When storing knowledge, include "
        "type and confidence when you are certain about them. "
        "Always check what is already known before asserting new facts."
    )),
    ("human", "{input}"),
    MessagesPlaceholder("agent_scratchpad"),
])

agent = create_tool_calling_agent(llm, tools, prompt)
agent_executor = AgentExecutor(agent=agent, tools=tools, verbose=True)


# -- Run it --

result = agent_executor.invoke({
    "input": (
        "Record that our PostgreSQL database (version 16) runs on db01.internal, "
        "and that our app-server depends on it. Use high confidence since this "
        "was confirmed by the sysadmin."
    )
})
print(result["output"])
```

### Minimal agent without LangChain

If you only need tool calling without the full LangChain stack, you can pass the
`/tools` definitions directly to the OpenAI client.

```python
import json
import requests
from openai import OpenAI

BASE = "http://localhost:3030"
client = OpenAI()

# Fetch OpenAI-compatible tool definitions from engram
tools = requests.get(f"{BASE}/tools").json()


def call_engram(tool_name: str, arguments: dict) -> str:
    """Route a tool call to the appropriate engram endpoint."""
    routes = {
        "engram_store":  ("/store",  "POST"),
        "engram_relate": ("/relate", "POST"),
        "engram_query":  ("/query",  "POST"),
        "engram_search": ("/search", "POST"),
        "engram_explain":("/explain","GET"),
    }
    if tool_name not in routes:
        return f"Unknown tool: {tool_name}"

    path, method = routes[tool_name]
    if method == "GET":
        label = arguments.get("entity", "")
        r = requests.get(f"{BASE}{path}/{label}")
    else:
        r = requests.post(f"{BASE}{path}", json=arguments)

    return json.dumps(r.json())


messages = [
    {"role": "user", "content": "What services does app-server connect to?"}
]

response = client.chat.completions.create(
    model="gpt-4o-mini",
    messages=messages,
    tools=tools,
)

# Process tool calls
if response.choices[0].finish_reason == "tool_calls":
    for tc in response.choices[0].message.tool_calls:
        args = json.loads(tc.function.arguments)
        result = call_engram(tc.function.name, args)
        print(f"Tool: {tc.function.name}")
        print(f"Result: {result}")
```

---

## 11. CLI Wrapper

For quick scripting and one-off operations, driving the `engram` binary directly
through `subprocess` is simpler than standing up the HTTP server.

### Basic subprocess calls

```python
import subprocess
import json


BRAIN = "/var/data/myproject.brain"


def engram(*args: str) -> str:
    """Run an engram CLI command and return stdout as a string."""
    result = subprocess.run(
        ["engram", *args],
        capture_output=True,
        text=True,
        check=True,
    )
    return result.stdout.strip()


# Stats
print(engram("stats", BRAIN))
# Nodes: 42
# Edges: 128

# Store a node
print(engram("store", "kubernetes", BRAIN))
# Stored node 'kubernetes' (id: 43)

# Set a property
print(engram("set", "kubernetes", "version", "1.29", BRAIN))
# kubernetes.version = 1.29

# Create a relationship
print(engram("relate", "app-server", "runs_on", "kubernetes", BRAIN))
# app-server -[runs_on]-> kubernetes (edge id: 129)

# Query a node
print(engram("query", "kubernetes", "2", BRAIN))
# Node: kubernetes
#   id: 43
#   confidence: 0.75
#   memory_tier: active
# Properties:
#   version: 1.29
# Edges in:
#   app-server -[runs_on]-> kubernetes

# Search
print(engram("search", "type:service AND confidence>0.8", BRAIN))
# Results (2):
#   app-server
#   celery

# Delete a node
print(engram("delete", "old-service", BRAIN))
# Deleted: old-service
```

### Error handling for subprocess

```python
import subprocess


def safe_engram(*args: str, brain: str = "knowledge.brain") -> tuple[bool, str]:
    """
    Run an engram command. Returns (success, output).
    Never raises — errors are returned in the output string.
    """
    try:
        result = subprocess.run(
            ["engram", *args, brain],
            capture_output=True,
            text=True,
            timeout=10,
        )
        if result.returncode != 0:
            return False, result.stderr.strip() or result.stdout.strip()
        return True, result.stdout.strip()
    except FileNotFoundError:
        return False, "engram binary not found — is it on PATH?"
    except subprocess.TimeoutExpired:
        return False, "engram command timed out"


ok, output = safe_engram("stats")
if ok:
    print(output)
else:
    print(f"Error: {output}")
```

### Starting the server from Python

```python
import subprocess
import time
import requests


def start_engram_server(
    brain_path: str = "knowledge.brain",
    addr: str = "127.0.0.1:3030",
    *,
    timeout: float = 5.0,
) -> subprocess.Popen:
    """
    Start the engram HTTP server as a background process.
    Blocks until the server is accepting connections or timeout is reached.
    Returns the Popen object — call .terminate() to stop it.
    """
    proc = subprocess.Popen(
        ["engram", "serve", brain_path, addr],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    base_url = f"http://{addr}"
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            r = requests.get(f"{base_url}/health", timeout=0.5)
            if r.status_code == 200:
                print(f"engram server ready at {base_url}")
                return proc
        except requests.exceptions.ConnectionError:
            time.sleep(0.1)

    proc.terminate()
    raise RuntimeError(f"engram server did not start within {timeout}s")


# Use as a context manager in tests
import contextlib


@contextlib.contextmanager
def engram_server(brain_path: str = "knowledge.brain", addr: str = "127.0.0.1:3030"):
    """Context manager that starts and stops the engram HTTP server."""
    proc = start_engram_server(brain_path, addr)
    try:
        yield f"http://{addr}"
    finally:
        proc.terminate()
        proc.wait(timeout=5)


# Example usage in a test or script
with engram_server("/tmp/test.brain", "127.0.0.1:3031") as base_url:
    resp = requests.get(f"{base_url}/health")
    print(resp.json())
    # {'status': 'ok', 'version': '0.1.0'}
```

---

## 12. Error Handling

All endpoints return JSON. Errors always have an `"error"` key. HTTP status
codes follow standard conventions:

| Status | Meaning |
|---|---|
| 200 | Success |
| 400 | Bad request — invalid rule syntax, missing required field |
| 404 | Node not found |
| 500 | Internal error — lock poisoned, storage error |

### Checking for errors on raw requests

```python
import requests


def post_engram(url: str, body: dict) -> dict:
    resp = requests.post(url, json=body)
    data = resp.json()
    if "error" in data:
        raise ValueError(f"HTTP {resp.status_code}: {data['error']}")
    return data


try:
    result = post_engram("http://localhost:3030/query", {"start": "nonexistent"})
except ValueError as e:
    print(e)
# HTTP 404: node not found: nonexistent
```

### Handling connection errors

```python
import requests
from requests.exceptions import ConnectionError, Timeout


def safe_post(url: str, body: dict, *, timeout: float = 5.0) -> dict | None:
    """
    POST to engram. Returns None on connection failure or timeout.
    Raises ValueError on application-level errors (4xx/5xx with error body).
    """
    try:
        resp = requests.post(url, json=body, timeout=timeout)
    except ConnectionError:
        print(f"Cannot connect to engram at {url} — is the server running?")
        return None
    except Timeout:
        print(f"Request to {url} timed out after {timeout}s")
        return None

    data = resp.json()
    if "error" in data:
        raise ValueError(f"HTTP {resp.status_code}: {data['error']}")
    return data
```

### Using the client's built-in error type

```python
from engram_client import EngramClient, EngramError

client = EngramClient()

try:
    node = client.get_node("does-not-exist")
except EngramError as e:
    print(f"Status: {e.status}")    # 404
    print(f"Message: {e.message}")  # node not found: does-not-exist
```

### Retrying transient failures

Lock contention (`500 graph lock poisoned`) is transient and safe to retry. Node
not found (`404`) is not — retrying will not help.

```python
import time
import requests
from requests.exceptions import ConnectionError, Timeout


def post_with_retry(
    url: str,
    body: dict,
    *,
    retries: int = 3,
    backoff: float = 0.5,
    timeout: float = 5.0,
) -> dict:
    """
    POST to engram with retry on transient server errors (500).
    Raises immediately on client errors (400, 404).
    """
    last_error: Exception | None = None

    for attempt in range(retries):
        try:
            resp = requests.post(url, json=body, timeout=timeout)
            data = resp.json()

            if resp.status_code == 500:
                last_error = ValueError(data.get("error", "server error"))
                time.sleep(backoff * (attempt + 1))
                continue

            if "error" in data:
                # 400 / 404 — do not retry
                raise ValueError(f"HTTP {resp.status_code}: {data['error']}")

            return data

        except (ConnectionError, Timeout) as e:
            last_error = e
            time.sleep(backoff * (attempt + 1))

    raise RuntimeError(f"All {retries} attempts failed: {last_error}")


# Usage
result = post_with_retry(
    "http://localhost:3030/store",
    {"entity": "important-node", "confidence": 0.99},
    retries=3,
    backoff=0.25,
)
print(result)
# {'node_id': 1, 'label': 'important-node', 'confidence': 0.99}
```

### Common errors reference

| Error message | Cause | Fix |
|---|---|---|
| `node not found: X` | Label `X` does not exist | Store the entity first with `/store` |
| `graph lock poisoned` | Internal Rust mutex panic | Restart the server |
| `missing field 'entity'` | Required field absent | Add `entity` to the request body |
| `invalid rule syntax` | Malformed derive rule | Check rule format: `rule name\nwhen ...\nthen ...` |
| `Connection refused` | Server not running | Run `engram serve` |

---

## Next Steps

- Read the [HTTP API reference](./http-api.md) for the full endpoint list with
  all optional fields documented.
- Read the [MCP server guide](./mcp-server.md) to use engram natively inside
  Claude, Cursor, or Windsurf without writing any Python.
- Run `engram reindex` after configuring an ONNX embedder to enable true
  semantic similarity via `/similar`.
- Schedule `POST /learn/decay` daily and `POST /learn/reinforce` on every access
  to keep confidence values meaningful over time.
- Use `POST /learn/derive` with transitive rules to materialise implicit
  relationships — for example, that a service inherits the properties of its
  base type.
