# Use Case 10: NER-Based Entity Extraction

### Overview

Named Entity Recognition (NER) extracts structured entities (people, organizations, locations) from unstructured text. This walkthrough demonstrates the NER-to-engram pipeline using simulated NER output from 3 documents. No spaCy or other NLP libraries needed for the demo -- for real NER integration, see `ner_pipeline.py`.

**What this demonstrates:**

- Entity extraction with NER type mapping (PERSON, ORG, GPE, FAC, PRODUCT)
- Relationship extraction from dependency parsing
- Co-mention edges for entities in the same sentence
- Cross-document reinforcement (entities in multiple documents get higher confidence)
- Entity resolution: merging aliases (Stanford = Stanford University)
- Confidence reflects NER certainty + corroboration across documents
- Graph traversal surfaces entity neighborhoods

**What requires external tools:**

- Python script to orchestrate the demo (calls the HTTP API)
- For production use: spaCy with `en_core_web_sm` model (see `ner_pipeline.py`)

### Prerequisites

- `engram` binary on your PATH (or `./target/release/engram`)
- Python 3.9+ with `requests` installed
- No NLP libraries needed (uses simulated NER output)

### Files

```
10-ner-entity-extraction/
  README.md             # This file
  ner_demo.py           # Self-contained demo with simulated NER output
  ner_pipeline.py       # Production template (requires spaCy)
```

### Step-by-Step

#### Step 1: Start the engram server

```bash
engram serve ner.brain 127.0.0.1:3030
```

#### Step 2: Run the demo

```bash
python ner_demo.py
```

### What Happens

#### Phase 1: NER Entity Extraction from 3 Documents

Three documents are processed through the simulated NER pipeline:

**tech-news-2024** (8 entities, 4 relationships):
```
Apple Inc. [ORG], Tim Cook [PERSON], Cupertino [GPE], California [GPE],
Google [ORG], Microsoft [ORG], Stanford [ORG], Dr. Sarah Chen [PERSON]

Tim Cook -[lead]-> Apple Inc.
Apple Inc. -[based_in]-> Cupertino
Cupertino -[located_in]-> California
Dr. Sarah Chen -[affiliated_with]-> Stanford
```

**market-report-q1** (6 entities, 3 relationships):
```
Google [ORG], Sundar Pichai [PERSON], Gemini [PRODUCT],
Microsoft [ORG], Azure [PRODUCT], Amazon Web Services [ORG]

Sundar Pichai -[lead]-> Google
Google -[develop]-> Gemini
Microsoft -[develop]-> Azure
```

**research-paper-abstract** (6 entities, 2 relationships):
```
Dr. Sarah Chen [PERSON], Dr. James Liu [PERSON],
Stanford University [ORG], National Institutes of Health [ORG],
Johns Hopkins Hospital [FAC], Google DeepMind [ORG]

Dr. Sarah Chen -[affiliated_with]-> Stanford University
Dr. James Liu -[affiliated_with]-> Stanford University
```

After extraction: **37 nodes, 66 edges** (includes doc nodes, co-mention edges).

#### Phase 2: Cross-Document Entity Reinforcement

Entities appearing in multiple documents get additional reinforcement:

```
Dr. Sarah Chen: 2 docs, conf=0.95
Google: 2 docs, conf=0.95
Microsoft: 2 docs, conf=0.95
```

#### Phase 3: Entity Resolution (Alias Merging)

```
Stanford -> same_as -> Stanford University
Google DeepMind -> subsidiary_of -> Google
```

After resolution: **37 nodes, 68 edges**.

#### Phase 4: Query the Knowledge Graph

**People:**
```
Tim Cook: conf=0.95
Dr. Sarah Chen: conf=0.95
Sundar Pichai: conf=0.95
Dr. James Liu: conf=0.95
```

**Organizations:**
```
Apple Inc.: conf=0.95
Google: conf=0.95
Microsoft: conf=0.95
Stanford University: conf=0.95
Amazon Web Services: conf=0.95
National Institutes of Health: conf=0.95
Google DeepMind: conf=0.95
Stanford: conf=0.85
```

**Text search for "Stanford"** returns both `Stanford` (0.85) and `Stanford University` (0.95), linked by `same_as`.

#### Phase 5: Graph Traversal

**From Google (depth=2):** 17 reachable nodes including Sundar Pichai, Gemini, Google DeepMind, Microsoft, Apple Inc., Amazon Web Services, Azure, and source documents.

**From Dr. Sarah Chen (depth=2):** 15 reachable nodes including Stanford, Stanford University, Dr. James Liu, and entities from both documents she appears in.

#### Phase 6: Confidence Landscape

```
Amazon Web Services            0.95 ###################
Apple Inc.                     0.95 ###################
California                     0.95 ###################
Dr. Sarah Chen                 0.95 ###################
Google                         0.95 ###################
Microsoft                      0.95 ###################
...
Cupertino                      0.90 ##################
Johns Hopkins Hospital         0.90 ##################
Azure                          0.85 #################
Stanford                       0.85 #################
Gemini                         0.80 ################
```

Cross-document entities (Google, Microsoft, Dr. Sarah Chen) reach 0.95 through reinforcement. Single-document entities stay at their NER confidence level.

#### Phase 7: Explainability

```
Explain: Dr. Sarah Chen
  Confidence: 0.95
  Outgoing edges (5):
    -[mentioned_in]-> doc:tech-news-2024 (conf=0.8)
    -[affiliated_with]-> Stanford (conf=0.5)
    -[mentioned_in]-> doc:research-paper-abstract (conf=0.8)
    -[affiliated_with]-> Stanford University (conf=0.6)
    -[co_mentioned]-> Dr. James Liu (conf=0.5)
  Incoming edges (0)
```

Final graph: **37 nodes, 68 edges**.

### Adapting for Real NER

The demo uses simulated NER output. To connect to spaCy:

```bash
pip install spacy
python -m spacy download en_core_web_sm
```

Then use `ner_pipeline.py` which provides:
- `extract_entities(text)` -- runs spaCy NER and stores in engram
- `extract_relationships(text)` -- dependency parsing for verb-based relationships
- `process_document(text, name)` -- full pipeline for a single document
- `find_similar_entities(name)` -- fuzzy matching for entity resolution
- `ingest_text_corpus(texts)` -- end-to-end pipeline for multiple documents

### Key Takeaways

- **NER type mapping** converts spaCy labels (PERSON, ORG, GPE) to engram node types (person, organization, location). This enables typed queries like `type:person`.
- **Relationship extraction** uses dependency parsing to find verb-mediated relationships (e.g., "Tim Cook leads Apple" becomes `Tim Cook -[lead]-> Apple Inc.`).
- **Co-mention edges** connect entities appearing in the same sentence at low confidence (0.40). These are weak signals that accumulate across documents.
- **Cross-document reinforcement** is the key insight: entities mentioned in multiple documents get reinforced, boosting confidence from 0.80-0.90 to 0.95.
- **Entity resolution** merges aliases via `same_as` edges. In production, fuzzy matching (SequenceMatcher) identifies candidates automatically.
- **Confidence as NER quality signal**: base confidence from NER (0.70-0.90) is boosted by reinforcement and capped at 0.95 (user-source cap). Single-mention entities stay at their NER confidence.
- **Provenance tracks NER source**: each entity records `source: ner:doc-name`, enabling trust assessment per document.
