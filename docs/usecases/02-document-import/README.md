# Use Case 2: Importing Text Documents into a Knowledge Base

### Overview

Most organizational knowledge lives in markdown files, meeting notes, design documents, and README files. This walkthrough reads local text files, splits them into paragraphs, extracts entity mentions using a tech-term allowlist and capitalization heuristics, and stores both the documents and their entities as a queryable knowledge graph.

After ingestion you can search across all documents by keyword, find which documents mention a specific technology, discover cross-document relationships through shared entities, and traverse the graph bidirectionally from any node.

**What this demonstrates:**

- Storing documents as nodes with string properties (filename, author, date, excerpt)
- Property-based filtering at search time (`prop:author=Alice`)
- Relating documents to extracted entities (`mentions` relationships)
- Author nodes linked to their documents (`authored_by`)
- Cross-document linking via shared entities (`shares_topics_with`)
- Bidirectional graph traversal (following both incoming and outgoing edges)
- BM25 keyword search across document excerpts

**What requires external tools:**

- Reading and splitting documents is done in Python
- Entity extraction uses an allowlist + capitalization heuristics; no NLP library required

### Prerequisites

- `engram` binary on your PATH (or `./target/release/engram`)
- Python 3.9+ with `requests` installed (`pip install requests`)

### Files

```
02-document-import/
  README.md              # This file
  import_docs.py         # Import script
  docs_sample/           # 6 sample documents
    architecture-decisions.md
    deployment-guide.md
    incident-2025-12-14.md
    infra-overview.md
    postmortem-2026-01-20.md
    runbook-redis.md
```

### Step-by-Step

#### Step 1: Start the engram server

```bash
engram serve docs.brain 127.0.0.1:3030
```

#### Step 2: Run the import

```bash
python import_docs.py
```

The script auto-detects `docs_sample/` relative to its own location. You can also pass a custom directory:

```bash
python import_docs.py /path/to/your/docs
```

#### Expected output

```
Importing 6 documents...

[architecture-decisions.md] author=Alice date=2025-10-15
  title: Architecture Decision Records
  stored node id=1
  entities: ['ArgoCD', 'GitOps', 'Helm', 'JSONB', 'Kubernetes', 'Memcached', 'MySQL', 'PostgreSQL', 'Redis', 'StatefulSets']

[deployment-guide.md] author=Charlie date=2026-02-10
  title: Deployment Guide
  stored node id=13
  entities: ['ArgoCD', 'Docker', 'Grafana', 'Jenkins', 'Kubernetes', 'Prometheus', 'ReplicaSet', 'Terraform', 'kubectl']

...

--- Cross-Document Links ---
  doc:architecture-decisions.md <-> doc:deployment-guide.md (shared: ['ArgoCD', 'Kubernetes'])
  doc:architecture-decisions.md <-> doc:infra-overview.md (shared: ['Kubernetes', 'PostgreSQL', 'Redis'])
  doc:incident-2025-12-14.md <-> doc:postmortem-2026-01-20.md (shared: ['Grafana', 'PostgreSQL'])
  ...

Done. Graph: 52 nodes, 60 edges
```

The script imports 6 documents, extracts ~30 unique technology entities, creates `mentions` and `authored_by` relationships, then discovers 14 cross-document links through shared entities.

### Querying the Results

#### Find documents by author

```bash
curl -s -X POST http://127.0.0.1:3030/search \
  -H "Content-Type: application/json" \
  -d '{"query": "prop:author=Alice", "limit": 10}'
```

Returns: `doc:architecture-decisions.md`, `doc:infra-overview.md`, `doc:runbook-redis.md`

#### Keyword search across document content

```bash
curl -s -X POST http://127.0.0.1:3030/search \
  -H "Content-Type: application/json" \
  -d '{"query": "replication", "limit": 5}'
```

Returns: `doc:incident-2025-12-14.md` -- found via the document excerpt stored as a property.

Other keyword searches that work: "cache", "deployment", "failover".

#### Which documents mention a technology?

```bash
curl -s -X POST http://127.0.0.1:3030/ask \
  -H "Content-Type: application/json" \
  -d '{"question": "What connects to PostgreSQL?"}'
```

Returns 4 documents that mention PostgreSQL:
```json
{
  "interpretation": "incoming edges to: PostgreSQL",
  "results": [
    {"label": "doc:architecture-decisions.md", "relationship": "mentions"},
    {"label": "doc:incident-2025-12-14.md", "relationship": "mentions"},
    {"label": "doc:infra-overview.md", "relationship": "mentions"},
    {"label": "doc:postmortem-2026-01-20.md", "relationship": "mentions"}
  ]
}
```

#### Bidirectional graph traversal

```bash
curl -s -X POST http://127.0.0.1:3030/query \
  -H "Content-Type: application/json" \
  -d '{"start": "PostgreSQL", "depth": 2}'
```

Returns **25 nodes, 54 edges** -- starting from PostgreSQL, the traversal follows incoming `mentions` edges to reach the 4 documents, then at depth 2 follows those documents' outgoing edges to discover all their other technologies, authors, and cross-document links.

The `direction` parameter controls traversal:
- `"both"` (default) -- follow both incoming and outgoing edges
- `"out"` -- outgoing edges only (traditional BFS)
- `"in"` -- incoming edges only (reverse traversal)

```bash
# Only outgoing edges from a document
curl -s -X POST http://127.0.0.1:3030/query \
  -H "Content-Type: application/json" \
  -d '{"start": "doc:infra-overview.md", "depth": 1, "direction": "out"}'
```

#### What does a document contain?

```bash
curl -s -X POST http://127.0.0.1:3030/ask \
  -H "Content-Type: application/json" \
  -d '{"question": "What does doc:infra-overview.md connect to?"}'
```

Returns all technologies, the author, and cross-document links:
```
Alice(authored_by), Grafana(mentions), Kubernetes(mentions), Nginx(mentions),
Patroni(mentions), PostgreSQL(mentions), Prometheus(mentions), Redis(mentions),
doc:postmortem-2026-01-20.md(shares_topics_with), doc:runbook-redis.md(shares_topics_with)
```

### Web UI

Open `http://127.0.0.1:3030/` and navigate to the Graph tab:

- Search for "PostgreSQL" -- see all documents that reference it and their interconnections
- Search for "doc:infra-overview.md" -- see all technologies mentioned in that document
- Use the **Direction** dropdown to switch between bidirectional, outgoing-only, and incoming-only traversal
- Adjust depth to explore further (depth 3 reaches technologies shared by related documents)

### Key Takeaways

- **Engram does not parse documents.** The import script extracts entities; engram stores and indexes them.
- **Properties are searchable.** Author, date, filename, and document excerpts are all searchable via `prop:key=value` or keyword search -- no extra configuration needed.
- **Bidirectional traversal is the default.** When you query "PostgreSQL", you see both what PostgreSQL connects to AND what connects to PostgreSQL. This makes every node in the graph explorable, not just nodes with outgoing edges.
- **Cross-document discovery.** The `shares_topics_with` edges reveal document relationships that aren't obvious from reading individual files. Two incident reports that both mention PostgreSQL and Grafana are likely related.
- **52 nodes, 60 edges from 6 documents.** The bipartite graph (documents on one side, entities on the other) plus author nodes and cross-document links creates a rich, traversable structure from a small document set.
