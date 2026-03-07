import spacy
import requests
from collections import defaultdict
from difflib import SequenceMatcher

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


if __name__ == "__main__":
    # Example corpus
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

    ingest_text_corpus(documents)
