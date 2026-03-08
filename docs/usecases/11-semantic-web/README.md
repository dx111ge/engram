# Use Case 11: Semantic Web -- Biomedical Drug Interaction Knowledge Graph

### Overview

The semantic web (RDF, JSON-LD, SPARQL) and AI agent memory are converging. Biomedical knowledge -- drugs, enzymes, diseases, interactions -- is one of the strongest domains for structured linked data because **getting it wrong can harm patients**. This walkthrough imports structured biomedical knowledge via JSON-LD from DrugBank, ChEBI, and SNOMED CT vocabularies, enriches it with engram's confidence model and inference engine, detects drug interactions automatically, handles contradictions from unreliable sources, and exports everything as interoperable linked data.

**Why semantic web beats web search here:**

Use case 09 extracted 4 facts about Rust from web search via regex. This use case imports 13 structured nodes with typed relationships in a single JSON-LD call -- drugs, enzymes, diseases, all linked with globally unique identifiers. No regex guessing. No deduplication. No extraction errors. Confidence comes from source tiers (FDA label = 0.95) rather than "how many blogs mentioned it."

**What this demonstrates:**

- JSON-LD import from biomedical ontologies (DrugBank, ChEBI, SNOMED CT)
- Source reliability tiers (FDA: 0.95, clinical trial: 0.85, patient blog: 0.25)
- Drug-enzyme-disease relationship modeling
- Inference rules detect CYP-mediated drug interactions automatically
- Contradiction handling: blog claim debunked by FDA contraindication
- Evidence chain traversal: drug -> enzyme -> interaction -> adverse effect
- JSON-LD export for interoperability with RDF tools (Jena, RDFLib, GraphDB)
- Confidence as a patient safety signal

**What requires external tools:**

- Python script to orchestrate the demo (calls the HTTP API)
- No external ontology access needed (uses simulated JSON-LD)

### Prerequisites

- `engram` binary on your PATH (or `./target/release/engram`)
- Python 3.9+ with `requests` installed

### Files

```
11-semantic-web/
  README.md              # This file
  biomedical_demo.py     # Self-contained demo with simulated biomedical JSON-LD
```

### Step-by-Step

#### Step 1: Start the engram server

```bash
engram serve biomedical.brain 127.0.0.1:3030
```

#### Step 2: Run the demo

```bash
python biomedical_demo.py
```

### What Happens

#### Phase 1: Import Biomedical JSON-LD

A single `/import/jsonld` call imports 13 structured nodes:

| Entity | Type | Vocabulary |
|--------|------|-----------|
| Simvastatin | Drug | DrugBank DB00641 |
| Clarithromycin | Drug | DrugBank DB01211 |
| Aspirin | Drug | DrugBank DB00945 |
| Ibuprofen | Drug | DrugBank DB01050 |
| Warfarin | Drug | DrugBank DB00001 |
| Metformin | Drug | DrugBank DB00563 |
| CYP3A4 | Enzyme | ChEBI 23924 |
| CYP2C9 | Enzyme | ChEBI 23924 |
| COX-1 | Enzyme | ChEBI 23924 |
| COX-2 | Enzyme | ChEBI 23924 |
| Hypercholesterolemia | Condition | SNOMED 13644009 |
| Type 2 Diabetes | Condition | SNOMED 44054006 |
| Rhabdomyolysis | Condition | SNOMED (life-threatening) |

Plus 6 typed edges: `metabolized_by`, `inhibits` relationships between drugs and enzymes.

#### Phase 2: Source Reliability Tiers

| Tier | Sources | Confidence |
|------|---------|------------|
| 1 (regulatory, peer-reviewed) | FDA-Label, DrugBank, PubMed-Meta | 0.92-0.95 |
| 2 (clinical) | ClinicalTrial, UpToDate | 0.85 |
| 3 (anecdotal) | PatientForum, HealthBlog | 0.25-0.30 |

#### Phase 3: Drug-Disease Relationships + Known Interactions

Drugs linked to conditions via `treats` edges (sourced from FDA labels):

```
Simvastatin -[treats]-> Hypercholesterolemia (conf=0.95)
Warfarin -[treats]-> Thromboembolism (conf=0.95)
Metformin -[treats]-> Type 2 Diabetes (conf=0.95)
Aspirin -[treats]-> Inflammation (conf=0.85)
```

Two known drug interactions stored with full mechanism detail:

**Simvastatin + CYP3A4 inhibitor -> Rhabdomyolysis risk** (severity: major)
- Mechanism: CYP3A4 inhibition increases simvastatin plasma levels
- Recommendation: avoid combination or reduce statin dose

**Aspirin + Warfarin -> GI Bleeding risk** (severity: major)
- Mechanism: additive anticoagulant and antiplatelet effects

After phase 3: **62 nodes, 88 edges**.

#### Phase 4: Inference -- Automatic Drug Interaction Detection

Two inference rules:

**Rule 1** (CYP interaction): If drug A inhibits enzyme X AND drug B is metabolized by enzyme X, flag drug B.

**Rule 2** (shared target): If drug A and drug B both inhibit the same enzyme, flag both.

Result: **12 rules fired, 4 flags raised:**

```
FLAGGED: Simvastatin -- interaction risk: co-prescribed with CYP inhibitor
FLAGGED: Clarithromycin -- shares enzyme target with another drug
FLAGGED: Aspirin -- shares enzyme target with another drug
FLAGGED: Ibuprofen -- shares enzyme target with another drug
OK: Warfarin -- no flags
OK: Metformin -- no flags
```

The inference engine automatically detected that Clarithromycin (CYP3A4 inhibitor) + Simvastatin (CYP3A4-metabolized) is dangerous, and that Aspirin + Ibuprofen share the COX-1 target.

#### Phase 5: Contradicting Evidence

A health blog claims "taking simvastatin with clarithromycin is perfectly safe" (conf=0.30). The FDA label contradicts this (conf=0.95). Correction zeroes the claim:

```
Claim:StatinMacrolideSafe confidence: 0.30 -> 0.00
```

#### Phase 6: Evidence Chain Traversal

**From Simvastatin (depth=2):** 10 reachable nodes:

```
Simvastatin (depth=0, conf=0.80)
  CYP3A4 (depth=1, conf=0.80)
  Hypercholesterolemia (depth=1, conf=0.80)
  Interaction:Simvastatin-CYP3A4-inhibitor (depth=1, conf=0.93)
  Source:FDA-Label (depth=1, conf=0.95)
  Clarithromycin (depth=2, conf=0.80)
  Rhabdomyolysis (depth=2, conf=0.80)
  Warfarin (depth=2, conf=0.80)
  ...
```

**From CYP3A4 (depth=2):** 7 nodes -- shows all drugs that interact with this enzyme plus their downstream risks.

#### Phase 7: Export as JSON-LD

The entire enriched graph exports as standard JSON-LD (64 nodes):

```json
{
  "@context": {
    "engram": "engram://vocab/",
    "schema": "https://schema.org/",
    "rdfs": "http://www.w3.org/2000/01/rdf-schema#"
  },
  "@graph": [
    {
      "@id": "engram://node/Simvastatin",
      "@type": "engram:Drug",
      "rdfs:label": "Simvastatin",
      "engram:confidence": 0.80,
      "engram:drug_class": "statin",
      "engram:_flag": "interaction risk: co-prescribed with CYP inhibitor"
    }
  ]
}
```

This JSON-LD is consumable by any RDF tool: Apache Jena, RDFLib, Virtuoso, GraphDB.

#### Phase 8: Explainability

```
Interaction:Simvastatin-CYP3A4-inhibitor (conf=0.93)
  Outgoing:
    -[involves]-> Simvastatin (conf=0.95)
    -[involves]-> CYP3A4 (conf=0.95)
    -[causes_risk_of]-> Rhabdomyolysis (conf=0.90)
```

Final graph: **64 nodes, 91 edges**.

### Architecture: Engram as a Semantic Web Bridge

```
+------------------+     JSON-LD     +----------+     JSON-LD     +------------------+
|  DrugBank        | ──────────────> |          | ──────────────> |  Apache Jena     |
|  ChEBI           |                 |  engram  |                 |  RDFLib          |
|  SNOMED CT       |  <── import ──  |          |  ── export ──>  |  GraphDB         |
|  Wikidata        |                 |          |                 |  SPARQL endpoint |
+------------------+                 +----------+                 +------------------+
                                          |
                                     Enrichment:
                                     - Confidence lifecycle
                                     - Inference rules
                                     - Contradiction detection
                                     - Source reliability tiers
                                     - Decay & reinforcement
```

### Key Takeaways

- **JSON-LD is the bridge** between engram and the biomedical semantic web. One import call brings in 13 typed, linked entities.
- **Source tiers matter for patient safety.** FDA labels (0.95) outweigh health blogs (0.25). Confidence quantifies how much to trust each fact.
- **Inference detects interactions automatically.** The CYP3A4 rule flagged Simvastatin without anyone manually encoding every drug pair. This scales to thousands of drugs.
- **Contradiction handling prevents misinformation.** The blog claim about statin+macrolide safety was zeroed by FDA evidence. In healthcare, this distinction saves lives.
- **Evidence chains are traversable.** Starting from any drug, depth-2 traversal surfaces enzymes, interactions, adverse effects, and source provenance.
- **Export preserves enrichment.** The JSON-LD output includes engram's confidence scores and inference flags alongside standard biomedical vocabulary. Any RDF tool can consume it.
- **Structured data >> web scraping.** Regex extraction from search snippets (use case 09) gives you "Rust is a programming language." JSON-LD import gives you typed drug-enzyme-disease relationships with globally unique identifiers and quantified confidence.
