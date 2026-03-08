#!/usr/bin/env python3
"""
Import local markdown/text documents into engram.

For each document:
  1. Parse YAML-like front matter for metadata (author, date)
  2. Store the document as a node with metadata and content excerpt
  3. Extract technology entities using allowlist + capitalization heuristics
  4. Store entities and relate them to documents
  5. Store authors as person nodes and relate to their documents
  6. Cross-link: find shared entities between documents

Usage:
  python import_docs.py                    # uses docs_sample/ next to this script
  python import_docs.py /path/to/docs      # uses custom directory
"""

import os
import re
import sys
import requests

ENGRAM = "http://127.0.0.1:3030"
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))

# Common words that look capitalized at sentence starts but aren't entities
STOP_WORDS = {
    "the", "our", "when", "this", "that", "these", "those", "its", "his", "her",
    "action", "resolution", "root", "backups", "incident", "overview", "runbook",
    "at", "on", "in", "for", "with", "from", "into", "all", "each", "every",
    "we", "if", "no", "set", "use", "add", "run", "has", "was", "are", "not",
    "summary", "timeline", "prerequisites", "process", "ensure", "verify",
    "merge", "trigger", "previous", "multiple", "applied", "identified",
    "implement", "review", "january", "february", "march", "april", "may",
    "june", "july", "august", "september", "october", "november", "december",
    "utc", "service", "oncall", "insert", "api", "json", "deployments",
    "gitopsbased",
}

# Known tech terms (case-insensitive matching, canonical output)
TECH_TERMS = {
    "kubernetes": "Kubernetes", "postgresql": "PostgreSQL",
    "redis": "Redis", "nginx": "Nginx", "prometheus": "Prometheus",
    "grafana": "Grafana", "patroni": "Patroni", "s3": "S3",
    "sentinel": "Sentinel", "docker": "Docker", "terraform": "Terraform",
    "ansible": "Ansible", "jenkins": "Jenkins", "elasticsearch": "Elasticsearch",
    "kafka": "Kafka", "rabbitmq": "RabbitMQ", "mongodb": "MongoDB",
    "mysql": "MySQL", "cassandra": "Cassandra", "consul": "Consul",
    "vault": "Vault", "istio": "Istio", "envoy": "Envoy",
    "argocd": "ArgoCD", "helm": "Helm", "gitops": "GitOps",
    "memcached": "Memcached", "kubectl": "kubectl",
    "wal": "WAL", "lru": "LRU", "jsonb": "JSONB",
    "statefulsets": "StatefulSets", "replicaset": "ReplicaSet",
    "setnx": "SETNX",
}


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


def extract_title(body):
    """Extract the first H1 heading as the document title."""
    for line in body.splitlines():
        line = line.strip()
        if line.startswith("# ") and not line.startswith("## "):
            return line[2:].strip()
    return None


def extract_excerpt(body, max_len=200):
    """Extract first meaningful paragraph as excerpt."""
    paragraphs = re.split(r'\n\s*\n', body)
    for p in paragraphs:
        text = " ".join(p.split())
        # Skip headings and very short lines
        if text.startswith("#") or len(text) < 20:
            continue
        return text[:max_len]
    return ""


def extract_entities(text):
    """
    Extract candidate entities from document text.

    Strategy:
      1. Match known tech terms from an allowlist (case-insensitive)
      2. Match capitalized words within sentences (not at line/sentence starts)
      3. Filter out headings, short words, and noise
    """
    entities = set()

    lower_text = text.lower()
    for term, canonical in TECH_TERMS.items():
        if term in lower_text:
            entities.add(canonical)

    # Capitalized words within sentences (not at line starts or after headings)
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        # Split into sentences, skip the first word of each sentence
        sentences = re.split(r'[.!?]\s+', line)
        for sent in sentences:
            words = sent.split()
            for word in words[1:]:
                clean = re.sub(r'[^A-Za-z]', '', word)
                if clean and clean[0].isupper() and len(clean) >= 3:
                    if clean.lower() not in STOP_WORDS:
                        entities.add(clean)

    return {e for e in entities if len(e) >= 3}


def api_store(entity, entity_type=None, properties=None, confidence=None):
    payload = {"entity": entity}
    if entity_type:
        payload["type"] = entity_type
    if properties:
        payload["properties"] = {k: str(v) for k, v in properties.items() if v}
    if confidence is not None:
        payload["confidence"] = confidence
    r = requests.post(f"{ENGRAM}/store", json=payload, timeout=10)
    r.raise_for_status()
    return r.json()


def api_relate(from_entity, relationship, to_entity, confidence=None):
    payload = {"from": from_entity, "relationship": relationship, "to": to_entity}
    if confidence is not None:
        payload["confidence"] = confidence
    r = requests.post(f"{ENGRAM}/relate", json=payload, timeout=10)
    r.raise_for_status()
    return r.json()


def api_tell(statement, source=None):
    payload = {"statement": statement}
    if source:
        payload["source"] = source
    r = requests.post(f"{ENGRAM}/tell", json=payload, timeout=10)
    r.raise_for_status()
    return r.json()


def api_ask(question):
    r = requests.post(f"{ENGRAM}/ask", json={"question": question}, timeout=10)
    r.raise_for_status()
    return r.json()


def api_search(query, limit=10):
    r = requests.post(f"{ENGRAM}/search", json={"query": query, "limit": limit}, timeout=10)
    r.raise_for_status()
    return r.json()


def api_query(start, depth=2, min_confidence=0.0):
    r = requests.post(f"{ENGRAM}/query",
                      json={"start": start, "depth": depth, "min_confidence": min_confidence},
                      timeout=10)
    r.raise_for_status()
    return r.json()


def api_stats():
    r = requests.get(f"{ENGRAM}/stats", timeout=5)
    r.raise_for_status()
    return r.json()


def import_document(filepath):
    """Import a single document, return (doc_label, entities, author)."""
    filename = os.path.basename(filepath)
    with open(filepath, encoding="utf-8") as f:
        raw = f.read()

    meta, body = parse_frontmatter(raw)
    author = meta.get("author", "unknown")
    date = meta.get("date", "")
    title = extract_title(body) or filename
    excerpt = extract_excerpt(body)

    print(f"\n[{filename}] author={author} date={date}")
    print(f"  title: {title}")

    # Store document node with content excerpt for BM25 searchability
    doc_label = f"doc:{filename}"
    result = api_store(
        entity=doc_label,
        entity_type="document",
        properties={
            "filename": filename,
            "title": title,
            "author": author,
            "date": date,
            "excerpt": excerpt,
        },
        confidence=0.95,
    )
    print(f"  stored node id={result['node_id']}")

    # Store author as a person node and relate
    api_store(author, entity_type="person", confidence=0.90)
    api_relate(doc_label, "authored_by", author, confidence=0.90)

    # Extract and store entities
    entities = extract_entities(body)
    print(f"  entities: {sorted(entities)}")

    for entity in sorted(entities):
        api_store(entity=entity, entity_type="technology", confidence=0.80)
        api_relate(doc_label, "mentions", entity, confidence=0.80)

    return doc_label, entities, author


def run_queries():
    """Run a battery of queries to demonstrate the imported data."""
    print("\n" + "=" * 60)
    print("QUERY RESULTS")
    print("=" * 60)

    stats = api_stats()
    print(f"\nGraph size: {stats['nodes']} nodes, {stats['edges']} edges")

    # Author filter
    print("\n--- Documents by Author ---")
    for author in ["Alice", "Bob", "Charlie"]:
        results = api_search(f"prop:author={author}", limit=10)
        hits = results.get("results", [])
        docs = [r["label"] for r in hits if r["label"].startswith("doc:")]
        if docs:
            print(f"  {author}: {docs}")

    # Keyword search (now works because excerpt is stored as property)
    print("\n--- Keyword Search ---")
    for query in ["replication", "cache", "deployment", "monitoring", "failover"]:
        results = api_search(query, limit=5)
        hits = results.get("results", [])
        labels = [r["label"] for r in hits[:5]]
        print(f"  '{query}': {labels}")

    # Which documents mention a technology?
    print("\n--- Technology Mentions ---")
    for tech in ["PostgreSQL", "Redis", "Prometheus", "Kubernetes", "Grafana"]:
        resp = api_ask(f"What connects to {tech}?")
        results = resp.get("results", [])
        docs = [r["label"] for r in results if r["label"].startswith("doc:")]
        print(f"  {tech} mentioned in: {docs}")

    # What does a document connect to?
    print("\n--- Document Contents ---")
    for doc in ["doc:infra-overview.md", "doc:postmortem-2026-01-20.md"]:
        resp = api_ask(f"What does {doc} connect to?")
        results = resp.get("results", [])
        things = [f"{r['label']}({r.get('relationship', '?')})" for r in results]
        print(f"  {doc}: {things}")

    # Graph traversal from a technology
    print("\n--- Graph Traversal (2-hop from PostgreSQL) ---")
    result = api_query("PostgreSQL", depth=2, min_confidence=0.7)
    nodes = result.get("nodes", [])
    edges = result.get("edges", [])
    print(f"  {len(nodes)} nodes, {len(edges)} edges reachable")
    for n in nodes[:8]:
        print(f"    {n['label']} (conf={n['confidence']:.2f}, depth={n.get('depth', '?')})")

    # Date-based search
    print("\n--- Documents by Date ---")
    for query in ["prop:date=2025-11-01", "prop:date=2026-01-20"]:
        results = api_search(query, limit=5)
        hits = results.get("results", [])
        labels = [r["label"] for r in hits]
        print(f"  {query}: {labels}")


def main():
    doc_dir = sys.argv[1] if len(sys.argv) > 1 else os.path.join(SCRIPT_DIR, "docs_sample")
    if not os.path.isdir(doc_dir):
        print(f"Directory not found: {doc_dir}")
        sys.exit(1)

    # Verify server
    try:
        r = requests.get(f"{ENGRAM}/health", timeout=5)
        r.raise_for_status()
        print(f"Server healthy: {r.json()}")
    except Exception as e:
        print(f"Server not reachable at {ENGRAM}: {e}")
        print("Start engram first: engram serve docs.brain 127.0.0.1:3030")
        sys.exit(1)

    doc_files = [
        os.path.join(doc_dir, f)
        for f in sorted(os.listdir(doc_dir))
        if f.endswith((".md", ".txt"))
    ]

    if not doc_files:
        print("No .md or .txt files found.")
        sys.exit(1)

    print(f"\nImporting {len(doc_files)} documents from {doc_dir}...\n")

    # Track entities per document for cross-linking
    doc_entities = {}

    for filepath in doc_files:
        try:
            doc_label, entities, author = import_document(filepath)
            doc_entities[doc_label] = entities
        except Exception as e:
            print(f"  [ERROR] {e}")

    # Cross-link: documents that share entities get a "related_to" edge
    print("\n--- Cross-Document Links ---")
    doc_labels = list(doc_entities.keys())
    for i in range(len(doc_labels)):
        for j in range(i + 1, len(doc_labels)):
            shared = doc_entities[doc_labels[i]] & doc_entities[doc_labels[j]]
            if shared:
                try:
                    api_relate(doc_labels[i], "shares_topics_with", doc_labels[j], confidence=0.70)
                    print(f"  {doc_labels[i]} <-> {doc_labels[j]} (shared: {sorted(shared)})")
                except Exception as e:
                    print(f"  [warn] {e}")

    stats = api_stats()
    print(f"\nDone. Graph: {stats['nodes']} nodes, {stats['edges']} edges")

    # Run queries
    run_queries()


if __name__ == "__main__":
    main()
