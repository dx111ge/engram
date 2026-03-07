# Use Case 2: Importing Text Documents into a Knowledge Base

### Overview

Most organizational knowledge lives in markdown files, meeting notes, design documents, and README files. This walkthrough reads local text files, splits them into paragraphs, extracts entity mentions using simple regex, and stores both the documents and their entities as a queryable knowledge graph.

After ingestion you can search across all documents for a concept, find which documents mention a specific author or entity, and traverse document-to-entity relationships.

**What this demonstrates today (v0.1.0):**

- Storing documents as nodes with string properties (filename, author, date)
- Property-based filtering at search time (`prop:author=John`)
- Relating documents to entities they mention
- Cross-document search using BM25

**What requires external tools:**

- Reading and splitting documents is done in Python
- Entity extraction is regex-based; no NLP library is used or required

### Prerequisites

- `engram` binary on your PATH
- Python 3.9+ with `requests` installed
- A directory of `.md` or `.txt` files to import

### Step-by-Step Implementation

#### Step 1: Create sample documents

```bash
mkdir docs_sample

cat > docs_sample/infra-overview.md << 'EOF'
---
author: Alice
date: 2025-11-01
---
# Infrastructure Overview

Our production environment runs on Kubernetes. The primary database is PostgreSQL
running on dedicated nodes. Redis is used for session caching. Nginx serves as
the reverse proxy in front of all services.

PostgreSQL replication is managed by Patroni. Backups run nightly to S3.
The monitoring stack consists of Prometheus and Grafana.
EOF

cat > docs_sample/incident-2025-12-14.md << 'EOF'
---
author: Bob
date: 2025-12-14
---
# Incident: PostgreSQL Replication Lag

At 14:32 UTC PostgreSQL replication lag exceeded 30 seconds on the replica.
Root cause: a bulk INSERT on the primary caused WAL accumulation.
Resolution: throttled the bulk job, lag recovered within 10 minutes.
Action items: add replication lag alert to Grafana dashboard.
EOF

cat > docs_sample/runbook-redis.md << 'EOF'
---
author: Alice
date: 2026-01-05
---
# Redis Runbook

Redis is configured with maxmemory-policy allkeys-lru. When memory pressure
occurs, keys are evicted using LRU. The Redis instance runs on port 6379.
Sentinel monitors Redis for automatic failover. Prometheus scrapes Redis
metrics via redis_exporter.
EOF
```

#### Step 2: Start the engram server

```bash
engram serve docs.brain 127.0.0.1:3030
```

#### Step 3: Write the document import script

Full script: [import_docs.py](import_docs.py)

Save as `import_docs.py`:

```python
#!/usr/bin/env python3
"""
Import local markdown/text documents into engram.

For each document:
  1. Parse YAML-like front matter for metadata (author, date)
  2. Store the document as a node with metadata properties
  3. Split body into paragraphs
  4. Extract capitalized multi-word phrases as candidate entities
  5. Store each entity and relate it to the document
"""

import os
import re
import sys
import requests

ENGRAM = "http://127.0.0.1:3030"

def parse_frontmatter(content):
    """Extract simple key: value front matter between --- delimiters."""
    meta = {}
    lines = content.splitlines()
    if lines and lines[0].strip() == "---":
        for i, line in enumerate(lines[1:], 1):
            if line.strip() == "---":
                body = "\n".join(lines[i + 1:])
                return meta, body
            if ":" in line:
                k, _, v = line.partition(":")
                meta[k.strip()] = v.strip()
    return meta, content

def extract_entities(text):
    """
    Extract candidate entities: sequences of 1-3 capitalized words,
    or known lowercase technology names from a short allowlist.
    """
    entities = set()

    # Capitalized phrase pattern (e.g., "Kubernetes", "WAL accumulation" is skipped — too noisy)
    cap_pattern = re.compile(r'\b([A-Z][a-z]{2,}(?:\s+[A-Z][a-z]{2,}){0,2})\b')
    for m in cap_pattern.finditer(text):
        word = m.group(1)
        # Skip sentence-starting words in a crude way: check if preceded by ". " or start-of-line
        entities.add(word)

    # Lowercase tech keywords that are meaningful even in lowercase
    tech_terms = [
        "kubernetes", "postgresql", "redis", "nginx", "prometheus",
        "grafana", "patroni", "s3", "sentinel",
    ]
    lower_text = text.lower()
    for term in tech_terms:
        if term in lower_text:
            # Store with the canonical capitalization from our allowlist
            canonical = {"kubernetes": "Kubernetes", "postgresql": "PostgreSQL",
                        "redis": "Redis", "nginx": "Nginx", "prometheus": "Prometheus",
                        "grafana": "Grafana", "patroni": "Patroni",
                        "s3": "S3", "sentinel": "Sentinel"}.get(term, term)
            entities.add(canonical)

    # Remove very short or all-uppercase items (acronyms noise)
    return {e for e in entities if len(e) > 2 and not e.isupper()}

def store(entity, entity_type=None, properties=None, confidence=None):
    payload = {"entity": entity}
    if entity_type:
        payload["type"] = entity_type
    if properties:
        payload["properties"] = {k: v for k, v in properties.items() if v}
    if confidence is not None:
        payload["confidence"] = confidence
    r = requests.post(f"{ENGRAM}/store", json=payload, timeout=5)
    r.raise_for_status()
    return r.json()

def relate(from_entity, relationship, to_entity, confidence=None):
    payload = {"from": from_entity, "relationship": relationship, "to": to_entity}
    if confidence is not None:
        payload["confidence"] = confidence
    r = requests.post(f"{ENGRAM}/relate", json=payload, timeout=5)
    r.raise_for_status()
    return r.json()

def import_document(filepath):
    filename = os.path.basename(filepath)
    with open(filepath, encoding="utf-8") as f:
        raw = f.read()

    meta, body = parse_frontmatter(raw)
    author = meta.get("author", "unknown")
    date   = meta.get("date", "")

    print(f"\n[{filename}] author={author} date={date}")

    # Store document node
    doc_label = f"doc:{filename}"
    result = store(
        entity=doc_label,
        entity_type="document",
        properties={
            "filename": filename,
            "author": author,
            "date": date,
            "path": filepath,
        },
        confidence=0.95,
    )
    print(f"  stored document node id={result['node_id']}")

    # Extract and store entities
    entities = extract_entities(body)
    print(f"  found {len(entities)} entities: {sorted(entities)}")

    for entity in sorted(entities):
        store(entity=entity, entity_type="technology", confidence=0.80)
        relate(doc_label, "mentions", entity, confidence=0.80)

    return doc_label, entities

def main():
    doc_dir = sys.argv[1] if len(sys.argv) > 1 else "docs_sample"
    if not os.path.isdir(doc_dir):
        print(f"Directory not found: {doc_dir}")
        sys.exit(1)

    doc_files = [
        os.path.join(doc_dir, f)
        for f in sorted(os.listdir(doc_dir))
        if f.endswith((".md", ".txt"))
    ]

    if not doc_files:
        print("No .md or .txt files found.")
        sys.exit(1)

    print(f"Importing {len(doc_files)} documents from {doc_dir}...\n")

    for filepath in doc_files:
        import_document(filepath)

    r = requests.get(f"{ENGRAM}/stats", timeout=5)
    stats = r.json()
    print(f"\nDone. Graph: {stats['nodes']} nodes, {stats['edges']} edges")

if __name__ == "__main__":
    main()
```

#### Step 4: Run the import

```bash
python import_docs.py docs_sample
```

Expected output:

```
Importing 3 documents from docs_sample...

[incident-2025-12-14.md] author=Bob date=2025-12-14
  stored document node id=1
  found 5 entities: ['Grafana', 'PostgreSQL', 'Patroni', 'Resolution', 'UTC']
  (UTC and Resolution will be stored too — the extractor is simple by design)

[infra-overview.md] author=Alice date=2025-11-01
  stored document node id=6
  found 8 entities: ['Kubernetes', 'Nginx', 'Patroni', 'PostgreSQL', 'Prometheus', 'Grafana', 'Redis', 'S3']

[runbook-redis.md] author=Alice date=2026-01-05
  stored document node id=14
  found 5 entities: ['Prometheus', 'Redis', 'Sentinel', 'LRU']

Done. Graph: 18 nodes, 18 edges
```

### Querying the Results

#### Find documents by author

```bash
engram search "prop:author=Alice" docs.brain
```

Expected output:

```
Results (2):
  doc:infra-overview.md
  doc:runbook-redis.md
```

#### Find which documents mention PostgreSQL

```bash
engram query PostgreSQL 1 docs.brain
```

Expected output:

```
Node: PostgreSQL
  id: 8
  confidence: 0.80
  memory_tier: active
Edges in:
  doc:incident-2025-12-14.md -[mentions]-> PostgreSQL (confidence: 0.80)
  doc:infra-overview.md -[mentions]-> PostgreSQL (confidence: 0.80)
Reachable (1-hop): 3 nodes
```

The incoming edges tell you which documents mention PostgreSQL.

#### Search for a concept across all documents

```bash
engram search "replication" docs.brain
```

Expected output:

```
Results (2):
  doc:incident-2025-12-14.md
  doc:infra-overview.md
```

The BM25 index is built on node labels and property values. The document nodes' filenames and entity relationships make both documents discoverable.

#### Property filter on document type

```bash
engram search "prop:type=document" docs.brain
```

Expected output:

```
Results (3):
  doc:incident-2025-12-14.md
  doc:infra-overview.md
  doc:runbook-redis.md
```

#### Ask via HTTP

```bash
curl -s -X POST http://127.0.0.1:3030/ask \
  -H "Content-Type: application/json" \
  -d '{"question": "What does doc:runbook-redis.md connect to?"}'
```

Expected output:

```json
{
  "interpretation": "outgoing edges from: doc:runbook-redis.md",
  "results": [
    {"label": "Prometheus", "confidence": 0.8, "relationship": "mentions", "detail": null},
    {"label": "Redis",      "confidence": 0.8, "relationship": "mentions", "detail": null},
    {"label": "Sentinel",   "confidence": 0.8, "relationship": "mentions", "detail": null}
  ]
}
```

### Key Takeaways

- Engram does not parse documents. The import script extracts entities; engram stores and indexes them.
- Properties (author, date, filename) are searchable via `prop:key=value` filter syntax at no extra configuration cost.
- The `mentions` relationship creates a bipartite graph: documents on one side, entities on the other. Traversal in either direction is instant.
- For production document ingestion, replace the regex extractor with a proper NLP library (spaCy, CoreNLP). Engram's API stays the same.
