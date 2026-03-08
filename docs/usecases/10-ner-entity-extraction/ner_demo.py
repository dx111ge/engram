#!/usr/bin/env python3
"""
Use Case 10: NER-Based Entity Extraction

Demonstrates building a knowledge graph from simulated NER (Named Entity
Recognition) output. No spaCy or other NLP libraries needed -- uses
pre-extracted entities to show the engram integration pattern.

For real NER integration, see ner_pipeline.py (requires spaCy).

Usage:
  engram serve ner.brain 127.0.0.1:3030
  python ner_demo.py
"""

import sys
import requests

ENGRAM = "http://127.0.0.1:3030"


def api(method, path, payload=None):
    url = f"{ENGRAM}{path}"
    if method == "GET":
        r = requests.get(url, timeout=10)
    elif method == "POST":
        r = requests.post(url, json=payload, timeout=10)
    else:
        raise ValueError(f"Unknown method: {method}")
    r.raise_for_status()
    return r.json()


def store(entity, entity_type=None, properties=None, confidence=None, source=None):
    payload = {"entity": entity}
    if entity_type:
        payload["type"] = entity_type
    if properties:
        payload["properties"] = {k: str(v) for k, v in properties.items()}
    if confidence is not None:
        payload["confidence"] = confidence
    if source:
        payload["source"] = source
    return api("POST", "/store", payload)


def relate(from_e, rel, to_e, confidence=None):
    payload = {"from": from_e, "relationship": rel, "to": to_e}
    if confidence is not None:
        payload["confidence"] = confidence
    return api("POST", "/relate", payload)


def reinforce(entity, source="ner"):
    return api("POST", "/learn/reinforce", {"entity": entity, "source": source})


def section(title):
    print(f"\n{'=' * 60}")
    print(f"  {title}")
    print(f"{'=' * 60}")


def subsection(title):
    print(f"\n--- {title} ---")


# Simulated NER output from 3 documents
# Each document has pre-extracted entities and relationships
# (in production, spaCy or similar NER model provides these)

DOCUMENTS = {
    "tech-news-2024": {
        "text": (
            "Apple Inc. announced on Tuesday that Tim Cook will lead "
            "the company's new artificial intelligence division based "
            "in Cupertino, California. The initiative, valued at "
            "$2 billion, aims to compete with Google and Microsoft "
            "in the enterprise AI market. Former Stanford professor "
            "Dr. Sarah Chen has been appointed as chief scientist."
        ),
        "entities": [
            ("Apple Inc.", "ORG", 0.85),
            ("Tim Cook", "PERSON", 0.90),
            ("Cupertino", "GPE", 0.80),
            ("California", "GPE", 0.85),
            ("Google", "ORG", 0.85),
            ("Microsoft", "ORG", 0.85),
            ("Stanford", "ORG", 0.75),
            ("Dr. Sarah Chen", "PERSON", 0.80),
        ],
        "relationships": [
            ("Tim Cook", "lead", "Apple Inc.", 0.55),
            ("Apple Inc.", "based_in", "Cupertino", 0.60),
            ("Cupertino", "located_in", "California", 0.70),
            ("Dr. Sarah Chen", "affiliated_with", "Stanford", 0.50),
        ],
        "co_mentions": [
            ("Google", "Microsoft", 0.40),
            ("Apple Inc.", "Google", 0.40),
            ("Apple Inc.", "Microsoft", 0.40),
        ],
    },
    "market-report-q1": {
        "text": (
            "Google reported record revenue of $86 billion in Q1 2024, "
            "driven by cloud computing and AI services. CEO Sundar Pichai "
            "highlighted the company's investment in Gemini. Microsoft, "
            "the main competitor, saw Azure revenue grow 31% year-over-year. "
            "Amazon Web Services maintained its market lead with $25 billion "
            "in quarterly revenue."
        ),
        "entities": [
            ("Google", "ORG", 0.90),
            ("Sundar Pichai", "PERSON", 0.90),
            ("Gemini", "PRODUCT", 0.70),
            ("Microsoft", "ORG", 0.90),
            ("Azure", "PRODUCT", 0.75),
            ("Amazon Web Services", "ORG", 0.85),
        ],
        "relationships": [
            ("Sundar Pichai", "lead", "Google", 0.55),
            ("Google", "develop", "Gemini", 0.50),
            ("Microsoft", "develop", "Azure", 0.50),
        ],
        "co_mentions": [
            ("Google", "Microsoft", 0.40),
            ("Google", "Amazon Web Services", 0.40),
            ("Microsoft", "Amazon Web Services", 0.40),
        ],
    },
    "research-paper-abstract": {
        "text": (
            "Dr. Sarah Chen and Dr. James Liu at Stanford University "
            "published a breakthrough paper on transformer architectures "
            "for medical imaging. The research, funded by the National "
            "Institutes of Health, demonstrated a 15% improvement in "
            "early cancer detection at Johns Hopkins Hospital compared "
            "to methods developed by Google DeepMind."
        ),
        "entities": [
            ("Dr. Sarah Chen", "PERSON", 0.90),
            ("Dr. James Liu", "PERSON", 0.85),
            ("Stanford University", "ORG", 0.90),
            ("National Institutes of Health", "ORG", 0.85),
            ("Johns Hopkins Hospital", "FAC", 0.80),
            ("Google DeepMind", "ORG", 0.85),
        ],
        "relationships": [
            ("Dr. Sarah Chen", "affiliated_with", "Stanford University", 0.60),
            ("Dr. James Liu", "affiliated_with", "Stanford University", 0.60),
        ],
        "co_mentions": [
            ("Dr. Sarah Chen", "Dr. James Liu", 0.50),
            ("Stanford University", "National Institutes of Health", 0.40),
            ("Johns Hopkins Hospital", "Google DeepMind", 0.35),
        ],
    },
}

# Map NER labels to engram node types
ENTITY_TYPE_MAP = {
    "PERSON": "person",
    "ORG": "organization",
    "GPE": "location",
    "FAC": "facility",
    "PRODUCT": "product",
}


def process_document(doc_name, doc_data):
    """Import one document's NER output into engram."""
    subsection(f"Document: {doc_name}")

    # Store the document node
    store(f"doc:{doc_name}", "document", {
        "char_count": len(doc_data["text"]),
        "source": doc_name,
    }, confidence=0.90, source=doc_name)

    # Store entities
    entities_stored = []
    for name, ner_label, conf in doc_data["entities"]:
        engram_type = ENTITY_TYPE_MAP.get(ner_label, "entity")
        store(name, engram_type, {
            "ner_label": ner_label,
            "source_doc": doc_name,
        }, confidence=conf, source=f"ner:{doc_name}")

        # Link entity to source document
        relate(name, "mentioned_in", f"doc:{doc_name}", 0.80)
        entities_stored.append(name)
        print(f"  Entity: {name} [{ner_label}] -> {engram_type} (conf={conf})")

    # Store relationships (from dependency parsing)
    for from_e, rel, to_e, conf in doc_data["relationships"]:
        relate(from_e, rel, to_e, conf)
        print(f"  Rel: {from_e} -[{rel}]-> {to_e} (conf={conf})")

    # Store co-mention edges (entities in same sentence)
    for e1, e2, conf in doc_data["co_mentions"]:
        relate(e1, "co_mentioned", e2, conf)

    # Reinforce entities (each mention counts)
    for name in entities_stored:
        reinforce(name, source=doc_name)

    return entities_stored


def main():
    try:
        health = api("GET", "/health")
        print(f"Server: {health}")
    except Exception as e:
        print(f"Server not reachable at {ENGRAM}: {e}")
        print("Start engram first: engram serve ner.brain 127.0.0.1:3030")
        sys.exit(1)

    # ================================================================
    section("PHASE 1: NER Entity Extraction from 3 Documents")
    # ================================================================

    all_entities = {}
    for doc_name, doc_data in DOCUMENTS.items():
        entities = process_document(doc_name, doc_data)
        for e in entities:
            all_entities[e] = all_entities.get(e, 0) + 1

    stats = api("GET", "/stats")
    print(f"\n  Graph after NER extraction: {stats['nodes']} nodes, {stats['edges']} edges")

    # ================================================================
    section("PHASE 2: Cross-Document Entity Reinforcement")
    # ================================================================

    subsection("Entities appearing in multiple documents")
    cross_doc = {e: c for e, c in all_entities.items() if c > 1}
    for entity, count in sorted(cross_doc.items()):
        # Extra reinforcement for cross-document mentions
        for _ in range(count - 1):
            reinforce(entity, source="cross-doc")
        node = api("GET", f"/node/{entity}")
        print(f"  {entity}: {count} docs, conf={node['confidence']:.2f}")

    # ================================================================
    section("PHASE 3: Entity Resolution (Alias Merging)")
    # ================================================================

    subsection("Merge aliases into canonical entities")

    # Stanford and Stanford University are the same
    relate("Stanford", "same_as", "Stanford University", 0.85)
    reinforce("Stanford University", source="resolution")
    print("  Stanford -> same_as -> Stanford University")

    # Google and Google DeepMind -- parent/subsidiary
    relate("Google DeepMind", "subsidiary_of", "Google", 0.80)
    print("  Google DeepMind -> subsidiary_of -> Google")

    stats = api("GET", "/stats")
    print(f"\n  Graph after resolution: {stats['nodes']} nodes, {stats['edges']} edges")

    # ================================================================
    section("PHASE 4: Query the Knowledge Graph")
    # ================================================================

    subsection("Search: people")
    result = api("POST", "/search", {"query": "type:person", "limit": 10})
    hits = result.get("results", [])
    for h in hits:
        print(f"  {h['label']}: conf={h['confidence']:.2f}")

    subsection("Search: organizations")
    result = api("POST", "/search", {"query": "type:organization", "limit": 10})
    hits = result.get("results", [])
    for h in hits:
        print(f"  {h['label']}: conf={h['confidence']:.2f}")

    subsection("Text search: 'Stanford'")
    result = api("POST", "/search", {"query": "Stanford", "limit": 5})
    hits = result.get("results", [])
    for h in hits:
        print(f"  {h['label']}: conf={h['confidence']:.2f}")

    # ================================================================
    section("PHASE 5: Graph Traversal from Key Entities")
    # ================================================================

    for start in ["Google", "Dr. Sarah Chen"]:
        subsection(f"Traverse from '{start}' (depth=2)")
        result = api("POST", "/query", {
            "start": start, "depth": 2, "min_confidence": 0.0,
        })
        nodes = result.get("nodes", [])
        print(f"  Reachable: {len(nodes)} nodes")
        for n in sorted(nodes, key=lambda x: (x.get("depth", 0), x["label"]))[:10]:
            print(f"    [depth={n.get('depth', '?')}] {n['label']} (conf={n['confidence']:.2f})")
        if len(nodes) > 10:
            print(f"    ... and {len(nodes) - 10} more")

    # ================================================================
    section("PHASE 6: Confidence Landscape")
    # ================================================================

    subsection("All entities sorted by confidence")
    entity_names = set()
    for doc_data in DOCUMENTS.values():
        for name, _, _ in doc_data["entities"]:
            entity_names.add(name)

    confidence_map = []
    for name in sorted(entity_names):
        try:
            node = api("GET", f"/node/{name}")
            confidence_map.append((name, node["confidence"]))
        except Exception:
            pass

    for name, conf in sorted(confidence_map, key=lambda x: -x[1]):
        bar = "#" * int(conf * 20)
        print(f"  {name:30s} {conf:.2f} {bar}")

    # ================================================================
    section("PHASE 7: Explainability")
    # ================================================================

    subsection("Explain: Dr. Sarah Chen")
    resp = api("GET", "/explain/Dr. Sarah Chen")
    print(f"  Confidence: {resp.get('confidence', '?'):.2f}")
    edges_from = resp.get("edges_from", [])
    edges_to = resp.get("edges_to", [])
    print(f"  Outgoing edges ({len(edges_from)}):")
    for e in edges_from:
        print(f"    -[{e['relationship']}]-> {e['to']} (conf={e.get('confidence', '?')})")
    print(f"  Incoming edges ({len(edges_to)}):")
    for e in edges_to:
        print(f"    {e['from']} -[{e['relationship']}]-> (conf={e.get('confidence', '?')})")

    # ================================================================
    section("SUMMARY")
    # ================================================================

    stats = api("GET", "/stats")
    print(f"\n  Final graph: {stats['nodes']} nodes, {stats['edges']} edges")
    print(f"  Documents processed: {len(DOCUMENTS)}")
    print(f"  Unique entities: {len(entity_names)}")
    print(f"  Cross-document entities: {len(cross_doc)}")
    print(f"\n  NER-to-engram pipeline demonstrated:")
    print(f"    - Entity extraction with type mapping (PERSON, ORG, GPE, etc.)")
    print(f"    - Relationship extraction from dependency parsing")
    print(f"    - Co-mention edges for entities in the same sentence")
    print(f"    - Cross-document reinforcement boosts confidence")
    print(f"    - Entity resolution merges aliases (Stanford = Stanford University)")
    print(f"    - Confidence reflects NER certainty + corroboration")
    print(f"    - Graph traversal surfaces entity neighborhoods")


if __name__ == "__main__":
    main()
