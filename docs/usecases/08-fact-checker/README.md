# Use Case 8: Fact Checker -- Multi-Source Claim Verification

### Overview

Fact-checking requires tracking claims, their sources, corroborating evidence, and contradictions. Engram's confidence model maps naturally to this domain: claims start with confidence based on source reliability, get reinforced when corroborated, and get corrected when debunked. The graph structure captures the web of evidence behind each claim.

This walkthrough builds a fact-checking knowledge base with 13 nodes covering sources, claims, and evidence. It demonstrates source reliability tiers, claim corroboration, contradiction handling, correction propagation, and inference rules for automated flagging.

**What this demonstrates:**

- Source reliability tiers (tier-1: peer-reviewed 0.95, tier-2: news 0.85, tier-3: blogs/social 0.30-0.40)
- Claim corroboration via reinforcement from independent sources
- Contradiction handling with evidence chains
- Correction: debunking claims and discrediting sources
- Evidence chain traversal
- Inference rules for automated credibility assessment
- Property search for claim categories and source tiers

**What requires external tools:**

- Python script to orchestrate the demo (calls the HTTP API)

### Prerequisites

- `engram` binary on your PATH (or `./target/release/engram`)
- Python 3.9+ with `requests` installed

### Files

```
08-fact-checker/
  README.md                  # This file
  fact_checker_demo.py       # Full demo script
```

### Step-by-Step

#### Step 1: Start the engram server

```bash
engram serve factcheck.brain 127.0.0.1:3030
```

#### Step 2: Run the demo

```bash
python fact_checker_demo.py
```

### What Happens

#### Phase 1: Source Reliability Tiers

7 sources registered across 3 reliability tiers:

| Tier | Sources | Confidence |
|------|---------|------------|
| 1 (peer-reviewed, official) | Source:WHO, Source:Nature, Source:Cochrane | 0.95 |
| 2 (major news) | Source:Reuters, Source:BBC | 0.82-0.85 |
| 3 (blogs, social) | Source:HealthBlog, Source:SocialPost | 0.30-0.40 |

#### Phase 2: Claims and Evidence

Four claims stored with different credibility profiles:

**Claim:EarthAge** (verified, conf=0.95)
- Sourced from Nature, corroborated by WHO
- Reinforced by independent confirmation

**Claim:VitaminCCuresCold** (disputed, conf=0.50)
- Sourced from HealthBlog (tier-3)
- Contradicted by Cochrane meta-analysis (11,306 participants)

**Claim:MicroplasticsInBlood** (emerging, conf=0.80)
- Sourced from Reuters, corroborated by BBC
- Supported by VU Amsterdam peer-reviewed study
- 2 confirmations boosted from 0.60 to 0.80

**Claim:5GCausesCovid** (fabricated, conf=0.30)
- Sourced from SocialPost (tier-3)

After phase 2: **13 nodes, 9 edges**.

#### Phase 3: Credibility Assessment

| Claim | Confidence | Status |
|-------|------------|--------|
| Claim:EarthAge | 0.95 | verified |
| Claim:VitaminCCuresCold | 0.50 | disputed |
| Claim:MicroplasticsInBlood | 0.80 | emerging |
| Claim:5GCausesCovid | 0.30 | fabricated |

Property search for health claims returns: VitaminCCuresCold, MicroplasticsInBlood, 5GCausesCovid.

#### Phase 4: Evidence Chain Traversal

Traversing from Claim:MicroplasticsInBlood (depth=2):

```
Claim:MicroplasticsInBlood (depth=0, conf=0.80)
  Source:Reuters (depth=1, conf=0.85)
  Source:BBC (depth=1, conf=0.82)
  Evidence:VUAmsterdam (depth=1, conf=0.90)
```

NL query "What connects to Claim:VitaminCCuresCold?" returns: Evidence:CochraneMeta (contradicts).

#### Phase 5: Debunk via Correction

**Debunk VitaminCCuresCold**:
```
Confidence: 0.50 -> 0.00
Distrust propagated to: Source:HealthBlog
```

**Debunk 5GCausesCovid**:
```
Confidence: 0.30 -> 0.00
Distrust propagated to: Source:SocialPost
```

**Discredit Source:SocialPost** (systematically unreliable):
```
Source:SocialPost confidence: 0.30 -> 0.00
```

#### Phase 6: Inference Rules

**Rule 1**: Flag claims contradicted by meta-analysis evidence:

```
rule contradicted_by_evidence
when edge(evidence, "contradicts", claim)
when prop(evidence, "study_type", "meta-analysis")
then flag(claim, "contradicted by meta-analysis")
```

**Rule 2**: Flag claims from low-reliability sources:

```
rule discredited_source
when edge(claim, "sourced_from", source)
when prop(source, "reliability_tier", "3")
then flag(claim, "sourced from low-reliability tier")
```

Result: Claims VitaminCCuresCold and 5GCausesCovid flagged as "sourced from low-reliability tier".

#### Phase 7: Final Credibility Report

| Claim | Confidence | Status | Flag |
|-------|------------|--------|------|
| Claim:EarthAge | 0.95 | verified | none |
| Claim:VitaminCCuresCold | 0.00 | disputed | sourced from low-reliability tier |
| Claim:MicroplasticsInBlood | 0.80 | emerging | none |
| Claim:5GCausesCovid | 0.00 | fabricated | sourced from low-reliability tier |

Explainability for Claim:MicroplasticsInBlood shows full evidence chain:
- Outgoing: sourced_from Reuters (0.85), corroborated_by BBC (0.82)
- Incoming: Evidence:VUAmsterdam supports (0.90)

### Key Takeaways

- **Source reliability tiers** map to confidence levels. Tier-1 sources start at 0.95, tier-3 at 0.30-0.40. This creates a natural credibility gradient.
- **Corroboration = reinforcement.** Each independent source that confirms a claim triggers `learn/reinforce`, boosting confidence by +0.10 per source.
- **Contradiction = correction.** Debunking a claim zeroes its confidence and propagates distrust to its sources.
- **Source discrediting cascades.** Correcting a source reduces confidence on all claims linked to that source.
- **Inference rules automate patterns** that fact-checkers apply manually: "if evidence contradicts a claim, flag it" and "if a claim comes from a tier-3 source, flag it."
- **Evidence chains are traversable.** Starting from any claim, depth-2 traversal surfaces all sources, corroborations, and contradicting evidence.
- **Confidence is a quantitative credibility score** that updates as new evidence arrives. It is not a binary true/false but a continuous measure of trust.
