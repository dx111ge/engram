# Use Case 10: NER-Based Entity Extraction from Unstructured Text

### Overview

Named Entity Recognition (NER) identifies real-world entities in unstructured text -- people, organizations, locations, dates, products, technical terms. Combined with dependency parsing, NER can also extract the relationships between entities. This walkthrough shows how to pipe NER output from spaCy into engram to build a knowledge graph from raw text automatically.

This is the production-grade alternative to the regex-based extraction in [Use Case 9](../09-web-search-import/). Where regex catches "X is a Y" patterns, NER catches entities regardless of sentence structure.

**What this demonstrates:**
- Using spaCy NER to extract entities from unstructured text
- Dependency parsing to extract relationships between entities
- Mapping NER entity types to engram node types
- Building a knowledge graph from news articles, reports, or any text corpus
- Combining NER confidence with engram's confidence model
- Multi-document entity resolution (same entity mentioned across documents)

### Prerequisites

- engram binary (running as HTTP server)
- Python 3.8+ with:
  ```bash
  pip install spacy requests
  python -m spacy download en_core_web_sm    # Small model (12MB), fast
  # or for better accuracy:
  python -m spacy download en_core_web_trf   # Transformer model (400MB), slower but more accurate
  ```

### Step 1: Start Engram

```bash
engram create ner-demo.brain
engram serve ner-demo.brain 127.0.0.1:3030
```

### Step 2: The NER-to-Engram Pipeline

Full script: [ner_pipeline.py](ner_pipeline.py)

```python
import spacy
import requests
from collections import defaultdict

API = "http://127.0.0.1:3030"
nlp = spacy.load("en_core_web_sm")

# -- Engram helpers --

def store(entity, entity_type=None, props=None, source="ner", confidence=0.70):
    body = {"entity": entity, "source": source, "confidence": confidence}
    if entity_type:
        body["type"] = entity_type
    if props:
        body["properties"] = props
    return requests.post(f"{API}/store", json=body).json()

def relate(from_e, to_e, rel, confidence=0.60):
    body = {"from": from_e, "to": to_e, "relationship": rel,
            "confidence": confidence}
    return requests.post(f"{API}/relate", json=body).json()

def reinforce(entity, source="ner"):
    return requests.post(f"{API}/learn/reinforce", json={
        "entity": entity, "source": source
    }).json()

# -- spaCy entity type to engram type mapping --

ENTITY_TYPE_MAP = {
    "PERSON": "person",
    "ORG": "organization",
    "GPE": "location",           # Geopolitical entity (country, city, state)
    "LOC": "location",           # Non-GPE location (mountain, river)
    "FAC": "facility",           # Buildings, airports, highways
    "PRODUCT": "product",
    "EVENT": "event",
    "WORK_OF_ART": "work",
    "LAW": "law",
    "LANGUAGE": "language",
    "DATE": "date",
    "TIME": "time",
    "MONEY": "monetary_value",
    "QUANTITY": "quantity",
    "NORP": "group",             # Nationalities, religious/political groups
    "CARDINAL": None,            # Skip bare numbers
    "ORDINAL": None,             # Skip ordinals
    "PERCENT": None,             # Skip percentages
}

def should_store_entity(ent):
    """Filter out entities that are too short or not useful as nodes."""
    if ENTITY_TYPE_MAP.get(ent.label_) is None:
        return False
    if len(ent.text.strip()) < 2:
        return False
    # Skip pure numeric entities
    if ent.text.strip().isdigit():
        return False
    return True
```

### Step 3: Extract Entities from Text

```python
def extract_entities(text, source_name="document"):
    """Run NER on text and store entities in engram."""
    doc = nlp(text)
    entities_found = {}

    for ent in doc.ents:
        if not should_store_entity(ent):
            continue

        entity_name = ent.text.strip()
        entity_type = ENTITY_TYPE_MAP[ent.label_]

        # Store in engram
        result = store(entity_name, entity_type, {
            "ner_label": ent.label_,
            "source_doc": source_name
        }, source=f"ner:{source_name}", confidence=0.70)

        entities_found[entity_name] = {
            "type": entity_type,
            "spacy_label": ent.label_,
            "node_id": result.get("node_id")
        }

        print(f"  Entity: {entity_name} [{ent.label_}] -> {entity_type}")

    return entities_found

# -- Example: extract from a news paragraph --

text = """
Apple Inc. announced on Tuesday that Tim Cook will lead the company's
new artificial intelligence division based in Cupertino, California.
The initiative, valued at $2 billion, aims to compete with Google and
Microsoft in the enterprise AI market. Former Stanford professor
Dr. Sarah Chen has been appointed as chief scientist. The project
will initially focus on healthcare applications at Johns Hopkins Hospital.
"""

print("=== Extracting entities ===")
entities = extract_entities(text, source_name="tech-news-2024")
print(f"\nFound {len(entities)} entities")
```

Expected output:
```
=== Extracting entities ===
  Entity: Apple Inc. [ORG] -> organization
  Entity: Tuesday [DATE] -> date
  Entity: Tim Cook [PERSON] -> person
  Entity: Cupertino [GPE] -> location
  Entity: California [GPE] -> location
  Entity: $2 billion [MONEY] -> monetary_value
  Entity: Google [ORG] -> organization
  Entity: Microsoft [ORG] -> organization
  Entity: Stanford [ORG] -> organization
  Entity: Dr. Sarah Chen [PERSON] -> person
  Entity: Johns Hopkins Hospital [FAC] -> facility

Found 11 entities
```

### Step 4: Extract Relationships via Dependency Parsing

NER finds entities, but dependency parsing reveals how they relate to each other:

```python
def extract_relationships(text, source_name="document"):
    """Extract entity-to-entity relationships using dependency parsing."""
    doc = nlp(text)
    relationships = []

    # Strategy: for each sentence, find entities and the verbs connecting them
    for sent in doc.sents:
        sent_ents = [ent for ent in sent.ents if should_store_entity(ent)]

        if len(sent_ents) < 2:
            continue

        # Find the root verb of the sentence
        root = [tok for tok in sent if tok.dep_ == "ROOT"]
        if not root:
            continue
        root_verb = root[0]

        # Find subject and object entities
        subj_ent = None
        obj_ent = None

        for ent in sent_ents:
            for token in ent:
                # Walk up dependency tree to find relation to root
                head = token.head
                while head != head.head:
                    if head == root_verb:
                        break
                    head = head.head

                if token.dep_ in ("nsubj", "nsubjpass") or \
                   any(t.dep_ in ("nsubj", "nsubjpass") for t in ent):
                    subj_ent = ent
                elif token.dep_ in ("dobj", "pobj", "attr", "oprd") or \
                     any(t.dep_ in ("dobj", "pobj", "attr") for t in ent):
                    obj_ent = ent

        if subj_ent and obj_ent and subj_ent != obj_ent:
            rel_type = root_verb.lemma_  # Use lemma for consistent naming
            relationships.append((subj_ent.text, rel_type, obj_ent.text))

            # Store in engram
            relate(subj_ent.text.strip(), obj_ent.text.strip(),
                   rel_type, confidence=0.55)

            print(f"  Rel: {subj_ent.text} -[{rel_type}]-> {obj_ent.text}")

    # Co-occurrence: entities in the same sentence are likely related
    for sent in doc.sents:
        sent_ents = [ent for ent in sent.ents if should_store_entity(ent)]
        for i, e1 in enumerate(sent_ents):
            for e2 in sent_ents[i+1:]:
                if e1.text != e2.text:
                    relate(e1.text.strip(), e2.text.strip(),
                           "co_mentioned", confidence=0.40)

    return relationships

print("\n=== Extracting relationships ===")
rels = extract_relationships(text, source_name="tech-news-2024")
print(f"\nFound {len(rels)} explicit relationships")
```

Expected output:
```
=== Extracting relationships ===
  Rel: Tim Cook -[lead]-> Apple Inc.
  Rel: Dr. Sarah Chen -[appoint]-> chief scientist

Found 2 explicit relationships
```

### Step 5: Process Multiple Documents

```python
def process_document(text, source_name):
    """Full pipeline: extract entities, relationships, and co-occurrences."""
    print(f"\n{'='*60}")
    print(f"Processing: {source_name}")
    print(f"{'='*60}")

    # Store the document itself as a node
    store(f"doc:{source_name}", "document", {
        "char_count": str(len(text)),
        "source": source_name
    }, source=source_name, confidence=0.90)

    # Extract entities
    entities = extract_entities(text, source_name)

    # Link entities to their source document
    for entity_name in entities:
        relate(entity_name, f"doc:{source_name}",
               "mentioned_in", confidence=0.80)

    # Extract relationships
    rels = extract_relationships(text, source_name)

    # Reinforce entities seen in multiple documents
    for entity_name in entities:
        reinforce(entity_name, source=source_name)

    return entities, rels

# -- Process a corpus of documents --

documents = {
    "tech-news-2024": """
        Apple Inc. announced on Tuesday that Tim Cook will lead the company's
        new artificial intelligence division based in Cupertino, California.
        The initiative, valued at $2 billion, aims to compete with Google and
        Microsoft in the enterprise AI market. Former Stanford professor
        Dr. Sarah Chen has been appointed as chief scientist.
    """,

    "market-report-q1": """
        Google reported record revenue of $86 billion in Q1 2024, driven by
        cloud computing and AI services. CEO Sundar Pichai highlighted the
        company's investment in Gemini, its flagship AI model. Microsoft,
        the main competitor, saw Azure revenue grow 31% year-over-year.
        Amazon Web Services maintained its market lead with $25 billion
        in quarterly revenue.
    """,

    "research-paper-abstract": """
        Dr. Sarah Chen and Dr. James Liu at Stanford University published
        a breakthrough paper on transformer architectures for medical imaging.
        The research, funded by the National Institutes of Health, demonstrated
        a 15% improvement in early cancer detection rates at Johns Hopkins
        Hospital compared to existing methods developed by Google DeepMind.
    """,
}

all_entities = {}
for doc_name, doc_text in documents.items():
    ents, rels = process_document(doc_text, doc_name)
    for name, info in ents.items():
        if name in all_entities:
            # Entity seen in multiple documents -- confidence grows
            print(f"    [cross-doc] {name} seen again (reinforced)")
        all_entities[name] = info

print(f"\n{'='*60}")
print(f"Total unique entities across all documents: {len(all_entities)}")
```

### Step 6: Query the NER-Built Knowledge Graph

```bash
# Find all people
engram search "type:person" ner-demo.brain
```

Expected:
```
Results (3):
  Tim Cook
  Dr. Sarah Chen
  Dr. James Liu
  Sundar Pichai
```

```bash
# Find all organizations
engram search "type:organization" ner-demo.brain
```

Expected:
```
Results (6):
  Apple Inc.
  Google
  Microsoft
  Stanford
  Amazon Web Services
  National Institutes of Health
```

```bash
# What entities are connected to Google?
curl -s -X POST http://127.0.0.1:3030/query \
  -H "Content-Type: application/json" \
  -d '{"start": "Google", "depth": 1, "min_confidence": 0.3}'
```

Traversal reveals all entities co-mentioned or explicitly related to Google across all three documents -- Sundar Pichai, Microsoft, Apple, Gemini, Google DeepMind, etc.

```bash
# Find entities mentioned in multiple documents (higher confidence)
engram search "confidence>0.8" ner-demo.brain
```

Entities like "Google", "Microsoft", and "Dr. Sarah Chen" appear in multiple documents and have been reinforced, giving them higher confidence than single-mention entities.

### Step 7: Advanced -- Custom NER for Domain-Specific Entities

spaCy's built-in NER handles general entities (people, orgs, locations). For domain-specific entities (CVE IDs, software packages, API endpoints, medical terms), add custom patterns:

```python
from spacy.language import Language
from spacy.tokens import Span

@Language.component("custom_tech_ner")
def custom_tech_ner(doc):
    """Add custom entity patterns for tech domain."""
    import re

    new_ents = list(doc.ents)

    # CVE IDs
    for match in re.finditer(r'CVE-\d{4}-\d{4,}', doc.text):
        span = doc.char_span(match.start(), match.end(), label="CVE")
        if span and not any(ent.start <= span.start < ent.end for ent in new_ents):
            new_ents.append(span)

    # Software versions (e.g., "PostgreSQL 16.2", "Python 3.12")
    for match in re.finditer(
        r'(PostgreSQL|MySQL|Redis|Python|Rust|Node\.js|Java)\s+\d+\.\d+(?:\.\d+)?',
        doc.text
    ):
        span = doc.char_span(match.start(), match.end(), label="SOFTWARE")
        if span and not any(ent.start <= span.start < ent.end for ent in new_ents):
            new_ents.append(span)

    # IP addresses
    for match in re.finditer(r'\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}', doc.text):
        span = doc.char_span(match.start(), match.end(), label="IP_ADDRESS")
        if span and not any(ent.start <= span.start < ent.end for ent in new_ents):
            new_ents.append(span)

    # Sort by start position and resolve overlaps
    new_ents = sorted(new_ents, key=lambda e: e.start)
    doc.ents = new_ents
    return doc

# Add to pipeline
nlp.add_pipe("custom_tech_ner", after="ner")

# Extend the type map
ENTITY_TYPE_MAP["CVE"] = "vulnerability"
ENTITY_TYPE_MAP["SOFTWARE"] = "software"
ENTITY_TYPE_MAP["IP_ADDRESS"] = "ip_address"

# Now process tech-specific text
security_text = """
A critical vulnerability CVE-2024-21762 was discovered in FortiOS by
Fortinet. The exploit allows remote code execution on servers running
FortiOS 7.4.2. Attackers from IP 198.51.100.42 have been observed
exploiting this vulnerability against PostgreSQL 16.2 instances.
CISA has added this to the Known Exploited Vulnerabilities catalog.
"""

process_document(security_text, "security-advisory-2024")
```

Expected:
```
  Entity: CVE-2024-21762 [CVE] -> vulnerability
  Entity: FortiOS [ORG] -> organization
  Entity: Fortinet [ORG] -> organization
  Entity: FortiOS 7.4.2 [SOFTWARE] -> software
  Entity: 198.51.100.42 [IP_ADDRESS] -> ip_address
  Entity: PostgreSQL 16.2 [SOFTWARE] -> software
  Entity: CISA [ORG] -> organization
```

### Step 8: Entity Resolution -- Merging Duplicates

Real-world text mentions the same entity in different forms. spaCy sees "Apple", "Apple Inc.", and "Apple Inc" as different entities. Use fuzzy matching to merge:

```python
from difflib import SequenceMatcher

def find_similar_entities(entity_name, threshold=0.80):
    """Search engram for entities similar to this one."""
    resp = requests.post(f"{API}/search", json={
        "query": entity_name, "limit": 5
    })
    hits = resp.json().get("results", [])

    similar = []
    for hit in hits:
        ratio = SequenceMatcher(None,
            entity_name.lower(), hit["label"].lower()
        ).ratio()
        if ratio >= threshold and hit["label"] != entity_name:
            similar.append((hit["label"], ratio))

    return similar

def merge_entities(canonical, aliases):
    """Create 'same_as' relationships for entity aliases."""
    for alias in aliases:
        relate(alias, canonical, "same_as", confidence=0.85)
        print(f"  Merged: {alias} -> {canonical}")

# After processing all documents, run dedup
print("\n=== Entity resolution ===")
all_labels = [e for e in all_entities]
merged = set()

for label in all_labels:
    if label in merged:
        continue
    similar = find_similar_entities(label)
    if similar:
        print(f"\n  {label} has similar entities:")
        aliases = []
        for sim_label, ratio in similar:
            print(f"    - {sim_label} ({ratio:.0%} similar)")
            aliases.append(sim_label)
            merged.add(sim_label)
        merge_entities(label, aliases)
```

### Step 9: Confidence Mapping -- NER Score to Engram Confidence

spaCy doesn't expose per-entity confidence scores in the default API, but the transformer model (`en_core_web_trf`) provides them. Map NER confidence to engram's confidence model:

```python
def ner_confidence_to_engram(ent, base_confidence=0.70):
    """Map NER characteristics to engram confidence."""
    confidence = base_confidence

    # Boost for well-known entity types
    if ent.label_ in ("ORG", "PERSON", "GPE"):
        confidence += 0.10  # High-confidence NER categories

    # Boost for longer entities (less likely to be false positives)
    if len(ent.text.split()) >= 2:
        confidence += 0.05  # Multi-word entities are more specific

    # Penalty for entities at sentence boundaries (more error-prone)
    if ent.start == ent.sent.start or ent.end == ent.sent.end:
        confidence -= 0.05

    return min(confidence, 0.95)  # Cap at user-source max
```

### Step 10: Full Pipeline -- From Raw Text to Queryable Graph

```python
def ingest_text_corpus(texts, pipeline_name="ner-pipeline"):
    """Complete NER-to-engram pipeline for a text corpus."""
    total_entities = 0
    total_rels = 0
    entity_counts = defaultdict(int)

    for doc_name, text in texts.items():
        ents, rels = process_document(text, doc_name)
        total_entities += len(ents)
        total_rels += len(rels)
        for name in ents:
            entity_counts[name] += 1

    # Cross-document reinforcement
    for entity, count in entity_counts.items():
        if count > 1:
            for _ in range(count - 1):
                reinforce(entity, source=pipeline_name)

    # Run entity resolution
    print("\n--- Entity resolution pass ---")
    for label in entity_counts:
        similar = find_similar_entities(label, threshold=0.85)
        if similar:
            merge_entities(label, [s[0] for s in similar])

    stats = requests.get(f"{API}/stats").json()
    print(f"\n=== Pipeline complete ===")
    print(f"Documents processed: {len(texts)}")
    print(f"Entities extracted: {total_entities}")
    print(f"Relationships found: {total_rels}")
    print(f"Knowledge graph: {stats['nodes']} nodes, {stats['edges']} edges")

# Run the full pipeline
ingest_text_corpus(documents)
```

### Comparison: Regex vs NER vs LLM Extraction

| Approach | Speed | Accuracy | Dependencies | Cost |
|----------|-------|----------|-------------|------|
| Regex (`/tell` patterns) | <1ms/doc | Low -- only catches "X is a Y" | None | Free |
| spaCy NER (sm model) | ~5ms/doc | Medium -- good for standard entities | 12MB model | Free |
| spaCy NER (trf model) | ~100ms/doc | High -- transformer-based | 400MB model | Free |
| LLM extraction (via `ENGRAM_LLM_ENDPOINT`) | ~1s/doc | Highest -- understands context | API key | $0.001-0.01/doc |

**Recommended approach:** Use spaCy NER for bulk import (fast, free, good accuracy), then use the LLM fallback for complex or ambiguous text that NER misses.

### Key Takeaways

- **NER automates what regex cannot.** spaCy identifies entities by context, not just pattern matching. "Apple announced" extracts "Apple" as ORG, while "apple pie recipe" would extract "apple" as a food reference (or skip it).
- **Dependency parsing extracts relationships** that no keyword matcher can find. "Tim Cook will lead the company's AI division" produces `Tim Cook -[lead]-> Apple Inc.`
- **Co-occurrence is a powerful signal.** Entities mentioned in the same sentence are likely related. Co-occurrence edges with low confidence (0.40) provide weak-but-useful graph connectivity.
- **Cross-document reinforcement** naturally identifies important entities. If "Google" appears in 10 documents, it gets reinforced 10 times and rises to high confidence.
- **Entity resolution** handles the real-world problem of inconsistent naming. Fuzzy matching + `same_as` edges keep the graph clean without losing variant forms.
- **Custom NER patterns** extend spaCy for domain-specific entities (CVEs, software versions, IPs) without retraining the model.
- **The pipeline is modular.** Each step (NER, relationship extraction, entity resolution, confidence mapping) can be customized or replaced independently. Engram is the storage layer -- the NLP is pluggable.
