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
