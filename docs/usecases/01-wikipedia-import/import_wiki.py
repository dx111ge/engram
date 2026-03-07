#!/usr/bin/env python3
"""
Import Wikipedia article summaries into engram.

Uses the Wikipedia REST API (no auth required):
  https://en.wikipedia.org/api/rest_v1/page/summary/{title}

For each article:
  1. Fetch the summary JSON
  2. Store the article title as a node with properties
  3. Use /tell to assert "X is a Y" facts found in the extract
  4. Create explicit relationships to paradigms and creators
"""

import re
import requests
import time

ENGRAM = "http://127.0.0.1:3030"
WIKI   = "https://en.wikipedia.org/api/rest_v1/page/summary"

# Articles to import with their known relationships
ARTICLES = [
    {
        "title": "Python_(programming_language)",
        "label": "Python",
        "paradigms": ["object-oriented programming", "functional programming", "imperative programming"],
        "creator": "Guido van Rossum",
        "typing": "dynamic typing",
    },
    {
        "title": "Rust_(programming_language)",
        "label": "Rust",
        "paradigms": ["systems programming", "functional programming", "concurrent programming"],
        "creator": "Graydon Hoare",
        "typing": "static typing",
    },
    {
        "title": "JavaScript",
        "label": "JavaScript",
        "paradigms": ["event-driven programming", "functional programming", "object-oriented programming"],
        "creator": "Brendan Eich",
        "typing": "dynamic typing",
    },
    {
        "title": "Go_(programming_language)",
        "label": "Go",
        "paradigms": ["concurrent programming", "imperative programming"],
        "creator": "Rob Pike",
        "typing": "static typing",
    },
]

def store(entity, entity_type=None, properties=None, confidence=None):
    payload = {"entity": entity}
    if entity_type:
        payload["type"] = entity_type
    if properties:
        payload["properties"] = properties
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

def tell(statement, source=None):
    payload = {"statement": statement}
    if source:
        payload["source"] = source
    r = requests.post(f"{ENGRAM}/tell", json=payload, timeout=5)
    r.raise_for_status()
    return r.json()

def fetch_wiki_summary(title):
    url = f"{WIKI}/{title}"
    r = requests.get(url, headers={"User-Agent": "engram-demo/1.0"}, timeout=10)
    r.raise_for_status()
    return r.json()

def extract_is_a_facts(label, extract):
    """
    Very simple rule-based extraction: find sentences matching
    '<label> is a ...' or '<label> is an ...' in the first 200 chars.
    Returns a list of (subject, predicate) pairs suitable for /tell.
    """
    facts = []
    # Normalize whitespace
    text = " ".join(extract.split())
    # Match "Label is a/an <noun phrase ending at period or comma>"
    pattern = re.compile(
        rf"{re.escape(label)}\s+is\s+(?:a|an)\s+([^,.;]+)",
        re.IGNORECASE,
    )
    for m in pattern.finditer(text[:300]):
        predicate = m.group(1).strip().rstrip(".")
        # Truncate to first two words to avoid over-long predicates
        short_pred = " ".join(predicate.split()[:3])
        if short_pred:
            facts.append((label, short_pred))
    return facts

def main():
    print(f"Importing {len(ARTICLES)} programming languages into engram...\n")

    for article in ARTICLES:
        label = article["label"]
        print(f"--- {label} ---")

        # Fetch Wikipedia summary
        try:
            data = fetch_wiki_summary(article["title"])
        except Exception as e:
            print(f"  Wikipedia fetch failed: {e}")
            continue

        extract = data.get("extract", "")
        description = data.get("description", "")
        print(f"  extract: {extract[:100]}...")

        # Store the language as a node with metadata
        result = store(
            entity=label,
            entity_type="programming_language",
            properties={
                "description": description,
                "wikipedia_title": article["title"],
                "creator": article["creator"],
                "typing": article["typing"],
                "source_url": data.get("content_urls", {}).get("desktop", {}).get("page", ""),
            },
            confidence=0.90,  # Wikipedia API source
        )
        print(f"  stored node id={result['node_id']} confidence={result['confidence']}")

        # Store the creator as a node and relate
        store(entity=article["creator"], entity_type="person", confidence=0.90)
        relate(label, "created_by", article["creator"], confidence=0.90)
        print(f"  related: {label} -[created_by]-> {article['creator']}")

        # Store paradigms and relate
        for paradigm in article["paradigms"]:
            store(entity=paradigm, entity_type="programming_paradigm", confidence=0.90)
            relate(label, "uses", paradigm, confidence=0.90)
            print(f"  related: {label} -[uses]-> {paradigm}")

        # Store typing discipline
        store(entity=article["typing"], entity_type="type_system", confidence=0.90)
        relate(label, "has", article["typing"], confidence=0.90)

        # Use /tell to assert "X is a Y" facts extracted from the summary
        facts = extract_is_a_facts(label, extract)
        for subject, predicate in facts:
            resp = tell(f"{subject} is a {predicate}", source="wikipedia")
            print(f"  told: {resp['interpretation']}")

        # Polite delay between Wikipedia requests
        time.sleep(0.5)

    print("\nImport complete.")

    # Show stats
    r = requests.get(f"{ENGRAM}/stats", timeout=5)
    stats = r.json()
    print(f"Graph: {stats['nodes']} nodes, {stats['edges']} edges")

if __name__ == "__main__":
    main()
