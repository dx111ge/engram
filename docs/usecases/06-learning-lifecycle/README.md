# Use Case 6: How Engram Learns, Forgets, and Self-Corrects

### Overview

Engram's learning model is distinct from a traditional database. Facts are not simply "stored" -- they have a confidence score that evolves over time based on access, confirmation, contradiction, and elapsed time. This walkthrough demonstrates the full lifecycle of a piece of knowledge from first storage through reinforcement, correction, recovery, and decay.

Understanding this lifecycle is important for building applications on engram: a fact you store today at low confidence will decay if unused. A fact you store and repeatedly confirm will rise toward its cap. A wrong fact that gets corrected penalizes its neighbors, discouraging over-confident wrong conclusions.

**Learning mechanics (exact values from the source):**

| Event | Effect |
|---|---|
| Access reinforcement (no source) | +0.02, capped at 0.95 |
| Confirmation reinforcement (with source) | +0.10, capped at 0.95 |
| Correction | zeroes confidence, propagates distrust at 0.5 damping/hop |
| Decay per day inactive | x 0.999 per day |
| Decay threshold (archival candidate) | < 0.10 |

**What this demonstrates:**

- Storing facts with different initial confidence levels
- Access reinforcement: +0.02 per read
- Confirmation reinforcement: +0.10 per independent source
- Property updates without confidence changes
- Correction with cascading distrust propagation
- Time-based decay
- Inference rules for automated flagging
- Recovery via reinforcement after damage
- Full explainability via `/explain`

**What requires external tools:**

- Python script to orchestrate the demo (calls the HTTP API)

### Prerequisites

- `engram` binary on your PATH (or `./target/release/engram`)
- Python 3.9+ with `requests` installed

### Files

```
06-learning-lifecycle/
  README.md              # This file
  learning_demo.py       # Full demo script
```

### Step-by-Step

#### Step 1: Start the engram server

```bash
engram serve learning.brain 127.0.0.1:3030
```

#### Step 2: Run the demo

```bash
python learning_demo.py
```

### What Happens

#### Phase 1: Initial Storage

Three facts stored with different confidence levels reflecting source trustworthiness:

| Entity | Source | Confidence |
|--------|--------|------------|
| Jupiter | LLM assistant | 0.30 |
| Sun | astronomy textbook | 0.95 |
| Mars | NASA database | 0.90 |

Two relationships: Jupiter -[orbits]-> Sun, Mars -[orbits]-> Sun.

After phase 1: **3 nodes, 2 edges**.

#### Phase 2: Access Reinforcement (+0.02)

Each access boost adds +0.02 to confidence. Simulating 5 accesses to Jupiter:

```
Access 1: 0.32
Access 2: 0.34
Access 3: 0.36
Access 4: 0.38
Access 5: 0.40
```

Jupiter: **0.30 -> 0.40** after 5 accesses.

#### Phase 3: Confirmation Reinforcement (+0.10)

Independent source confirmations add +0.10 each:

```
After NASA confirmation:  0.50
After ESO confirmation:   0.60
```

Jupiter: **0.40 -> 0.60** after 2 confirmations. Confirmations are 5x stronger than access boosts.

#### Phase 4: Property Update

Updating Jupiter's moon count from 79 to 95 via `/store`:

```
Updated Jupiter properties
Jupiter confidence unchanged: 0.60
moon_count property: 95
```

Store updates properties but does **not** change confidence. Confidence is managed exclusively by the learning endpoints (`/learn/reinforce`, `/learn/correct`, `/learn/decay`).

#### Phase 5: Wrong Fact and Correction

A wrong claim is stored with an outgoing edge to Jupiter:

```
Jupiter-has-solid-surface (conf=0.70)
  -[about]-> Jupiter
```

Before correction: Jupiter confidence = 0.60.

**Correction** (`/learn/correct`) zeroes the claim and propagates distrust:

```
Claim confidence: 0.70 -> 0.00
Distrust propagated to: Jupiter, Sun

Jupiter: 0.60 -> 0.25 (direct neighbor, 0.5 damping)
Sun:     0.95 -> 0.78 (2-hop, 0.5 x 0.5 damping)
Mars:    0.90 -> 0.90 (not connected to claim)
```

This is intentional: wrong facts should damage confidence in connected knowledge. The cascading distrust reflects reality -- if an analyst's claim about Jupiter is wrong, other things they asserted about Jupiter become less certain too.

#### Phase 6: Decay

```
Nodes decayed: 0
```

Decay returns 0 because all nodes were accessed within the current session (0 days elapsed). In production, decay is called periodically (e.g., daily cron) and multiplies confidence by `0.999^days_since_last_access`.

Decay projections for an untouched node at 0.50:
- After 30 days: 0.485
- After 90 days: 0.457
- After 365 days: 0.347
- Reaches archival threshold (0.10) after ~1600 days

#### Phase 7: Inference Rules

**Flag unverified claims**:

```
rule flag_unverified
when prop(node, "status", "unverified")
then flag(node, "unverified claim -- needs review")
```

Result: **2 rules fired, 1 flag raised**. Jupiter-has-solid-surface is flagged for review.

#### Phase 8: Recovery via Reinforcement

Jupiter was damaged by distrust propagation (0.25). Recovery through confirmations and accesses:

```
Jupiter before recovery:       0.25
After 3 confirmations (+0.30): 0.55
After 10 accesses (+0.20):     0.75
```

Sun recovery:

```
Sun before recovery:            0.78
After 3 confirmations + 10 accesses: 0.95
```

The system self-heals: wrong facts are corrected, distrust propagates, then good facts are reinforced back to healthy confidence levels by continued use and independent confirmation.

#### Phase 9: Explainability

```bash
curl -s http://127.0.0.1:3030/explain/Jupiter
```

Returns:
- **Confidence**: 0.75 (recovered from 0.25 post-correction)
- **Properties**: moon_count=95, system=Solar System
- **Outgoing edges**: orbits Sun (0.99)
- **Incoming edges**: Jupiter-has-solid-surface about Jupiter (0.70)

### Full Lifecycle Summary

| Step | Event | Jupiter Confidence |
|------|-------|--------------------|
| 1 | Stored with LLM confidence | 0.30 |
| 2 | 5 access reinforcements (+0.02 each) | 0.40 |
| 3 | 2 confirmation reinforcements (+0.10 each) | 0.60 |
| 4 | Property update (no confidence change) | 0.60 |
| 5 | Distrust propagation from corrected neighbor | 0.25 |
| 6 | Decay (0 days elapsed, no change) | 0.25 |
| 7 | Flagging rules (no direct effect on Jupiter) | 0.25 |
| 8 | Recovery: 3 confirmations + 10 accesses | 0.75 |

### Key Takeaways

- **Confidence reflects trust, not truth.** A fact stored at 0.30 from an LLM is not "wrong" -- it is unverified. Reinforcement builds trust over time.
- **Access vs. confirmation**: 10 accesses from one application (+0.20 total) equals 2 independent confirmations (+0.20 total). Confirmations require a `source` parameter.
- **Correction cascades**: Wrong facts penalize their neighbors through distrust propagation (0.5 damping per hop). This prevents orphan wrong conclusions from staying high-confidence.
- **Recovery is natural**: After correction, continued use and confirmation rebuilds confidence. The system self-heals without manual intervention beyond normal usage.
- **Store and learn are separate**: `/store` manages properties and creates nodes. `/learn/reinforce`, `/learn/correct`, `/learn/decay` manage confidence. This separation prevents accidental confidence resets.
- **Decay requires explicit calls**: Confidence does not decay automatically. Call `/learn/decay` on a schedule (e.g., daily cron) to let stale knowledge fade.
- **Inference rules automate review**: Property-matching rules can flag nodes for human review (e.g., all unverified claims, all nodes below a confidence threshold).
