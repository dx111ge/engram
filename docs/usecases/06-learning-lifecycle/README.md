# Use Case 6: How Engram Learns, Forgets, and Self-Corrects

### Overview

Engram's learning model is distinct from a traditional database. Facts are not simply "stored" — they have a confidence score that evolves over time based on access, confirmation, contradiction, and elapsed time. This walkthrough demonstrates the full lifecycle of a piece of knowledge from first storage to potential archival.

Understanding this lifecycle is important for building applications on engram: a fact you store today at low confidence will decay if unused. A fact you store and repeatedly confirm will rise toward its source-type cap. A wrong fact that gets corrected penalizes its neighbors, discouraging over-confident wrong conclusions.

**Learning mechanics in v0.1.0 (exact values from the source):**

| Event | Effect |
|---|---|
| Initial store (user source) | confidence = 0.80 |
| Initial store (API source) | confidence = 0.90 |
| Initial store (LLM source) | confidence = 0.30 |
| Access reinforcement | +0.02, capped at source max |
| Confirmation reinforcement (source present) | +0.10, capped at source max |
| Contradiction penalty | -0.20, floored at 0.0 |
| Decay per day inactive | x 0.999 per day |
| Decay threshold (archival candidate) | < 0.10 |
| Tier: core | confidence >= 0.90, access_count >= 10 |
| Tier: archival | confidence < 0.20, or inactive > 90 days |

### Prerequisites

- `engram` binary on your PATH
- `curl` for HTTP API calls

### Step-by-Step Implementation

#### Step 1: Start the server

```bash
engram serve learning.brain 127.0.0.1:3030
```

#### Step 2: Store a fact with low initial confidence (LLM source)

An AI assistant suggests that "Jupiter has 79 moons." You do not fully trust the LLM, so you store it with a low confidence source:

```bash
curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{
    "entity": "Jupiter",
    "type": "planet",
    "properties": {"moon_count": "79", "system": "Solar System"},
    "source": "llm",
    "confidence": 0.30
  }'
```

Expected output:

```json
{"node_id": 1, "label": "Jupiter", "confidence": 0.3}
```

> Note: The `source` field in the HTTP API currently maps to the provenance source ID string, not the source type enum. The confidence value you supply explicitly overrides the default. In v0.1.0, to get the LLM initial confidence of 0.30 automatically, pass `"confidence": 0.30` directly.

#### Step 3: Check initial state

```bash
engram query Jupiter 0 learning.brain
```

Expected output:

```
Node: Jupiter
  id: 1
  confidence: 0.30
  memory_tier: active
Properties:
  moon_count: 79
  system: Solar System
```

#### Step 4: Reinforce via access (reading the fact)

Each time your application reads and uses this fact, call `reinforce` without a source (access reinforcement = +0.02):

```bash
# Simulate 3 accesses
for i in 1 2 3; do
  curl -s -X POST http://127.0.0.1:3030/learn/reinforce \
    -H "Content-Type: application/json" \
    -d '{"entity":"Jupiter"}'
done
```

Check after 3 accesses:

```bash
curl -s http://127.0.0.1:3030/node/Jupiter | python3 -m json.tool
```

Expected confidence (0.30 + 3 x 0.02 = 0.36):

```json
{
  "node_id": 1,
  "label": "Jupiter",
  "confidence": 0.36,
  ...
}
```

#### Step 5: Confirm from an independent source (confirmation reinforcement)

You look it up in a NASA database. The count is actually 95 (as of 2023 — the LLM was outdated). First, correct the property:

```bash
curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{
    "entity": "Jupiter",
    "properties": {"moon_count": "95"},
    "confidence": 0.90
  }'
```

Now reinforce with the NASA source (confirmation boost = +0.10):

```bash
curl -s -X POST http://127.0.0.1:3030/learn/reinforce \
  -H "Content-Type: application/json" \
  -d '{"entity":"Jupiter","source":"nasa-jpl-database"}'
```

Expected output (0.36 + 0.10 = 0.46, but we also set confidence to 0.90 above — the explicit store with confidence=0.90 takes effect):

```json
{"entity":"Jupiter","new_confidence":0.95}
```

> The store with `confidence: 0.90` replaces the stored confidence, then reinforce adds +0.10, capped at the user source cap of 0.95.

#### Step 6: Store a related fact and demonstrate graph learning

```bash
curl -s -X POST http://127.0.0.1:3030/tell \
  -H "Content-Type: application/json" \
  -d '{"statement":"Jupiter is a gas giant","source":"astronomy-textbook"}'

curl -s -X POST http://127.0.0.1:3030/tell \
  -H "Content-Type: application/json" \
  -d '{"statement":"Jupiter is a planet","source":"astronomy-textbook"}'

curl -s -X POST http://127.0.0.1:3030/relate \
  -H "Content-Type: application/json" \
  -d '{"from":"Jupiter","relationship":"orbits","to":"Sun","confidence":0.99}'

curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{"entity":"Sun","type":"star","confidence":0.99}'
```

#### Step 7: Introduce a wrong fact, then correct it

Suppose someone asserts "Jupiter has a solid core you can stand on" — confidently wrong:

```bash
curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{
    "entity": "Jupiter has solid surface",
    "type": "claim",
    "confidence": 0.70
  }'

curl -s -X POST http://127.0.0.1:3030/relate \
  -H "Content-Type: application/json" \
  -d '{"from":"Jupiter","relationship":"has_property","to":"Jupiter has solid surface","confidence":0.70}'
```

Now the scientific consensus is confirmed: Jupiter has no solid surface. Correct the wrong fact:

```bash
curl -s -X POST http://127.0.0.1:3030/learn/correct \
  -H "Content-Type: application/json" \
  -d '{
    "entity": "Jupiter has solid surface",
    "reason": "Jupiter is a gas giant with no defined solid surface",
    "source": "peer-reviewed-astrophysics"
  }'
```

Expected output:

```json
{
  "corrected": "Jupiter has solid surface",
  "propagated_to": ["Jupiter"]
}
```

The claim drops from 0.70 to 0.50 (-0.20 penalty). Jupiter's confidence also receives a penalty signal propagation. Check Jupiter:

```bash
curl -s http://127.0.0.1:3030/node/Jupiter | python3 -c "import sys,json; d=json.load(sys.stdin); print('confidence:', d['confidence'])"
```

Expected output (slight drop from the propagated correction):

```
confidence: 0.87
```

#### Step 8: Derive new facts using inference rules

Define a rule: if X is a planet and X orbits Y, then X is part of the same system as Y. Use this to derive a "same_system" edge:

```bash
curl -s -X POST http://127.0.0.1:3030/learn/derive \
  -H "Content-Type: application/json" \
  -d '{
    "rules": [
      "rule planet_in_system\nwhen edge(PLANET, \"is_a\", \"planet\")\nwhen edge(PLANET, \"orbits\", STAR)\nthen edge(PLANET, \"part_of_system\", STAR, min(e1, e2))"
    ]
  }'
```

Expected output:

```json
{
  "rules_evaluated": 1,
  "rules_fired": 1,
  "edges_created": 1,
  "flags_raised": 0
}
```

The edge `Jupiter -[part_of_system]-> Sun` is now derived.

#### Step 9: Demonstrate memory tier transitions

After enough access, a node promotes to the `core` tier. The threshold is confidence >= 0.90 AND access_count >= 10. Simulate 7 more accesses to reach 10 total:

```bash
for i in $(seq 1 7); do
  curl -s -X POST http://127.0.0.1:3030/learn/reinforce \
    -H "Content-Type: application/json" \
    -d '{"entity":"Jupiter"}' > /dev/null
done
```

Check the tier via the CLI:

```bash
engram query Jupiter 0 learning.brain
```

Expected output after sufficient accesses and confidence:

```
Node: Jupiter
  id: 1
  confidence: 0.95
  memory_tier: core
```

The `memory_tier` field transitions from `active` to `core` when the node reaches the promotion threshold.

#### Step 10: Apply decay and observe archival candidates

First verify current stats:

```bash
curl -s http://127.0.0.1:3030/stats
```

```json
{"nodes": 7, "edges": 8}
```

Apply decay:

```bash
curl -s -X POST http://127.0.0.1:3030/learn/decay \
  -H "Content-Type: application/json" \
  -d '{}'
```

Expected output:

```json
{"nodes_decayed": 7}
```

All nodes have their confidence multiplied by `0.999 ^ days_since_last_access`. Nodes accessed very recently (like Jupiter, which we just queried) decay very little. Nodes that were stored and never touched since experience the full decay.

The `Jupiter has solid surface` node, already at 0.50 after correction, continues to decay. After approximately 90 days of inactivity:

- Confidence after 30 days: `0.50 x 0.999^30 ~ 0.485`
- Confidence after 90 days: `0.50 x 0.999^90 ~ 0.455`
- Confidence after 365 days: `0.50 x 0.999^365 ~ 0.347`
- Confidence reaches archival threshold (0.20) after roughly 916 days without access

Additionally, because that node has been inactive for 90 days, the tier system demotes it to `archival` at the 90-day mark even before confidence reaches the decay floor.

#### Step 11: Use a flag rule to surface low-confidence nodes for review

```bash
curl -s -X POST http://127.0.0.1:3030/learn/derive \
  -H "Content-Type: application/json" \
  -d '{
    "rules": [
      "rule flag_stale_claims\nwhen confidence(node, \"<\", 0.3)\nthen flag(node, \"low confidence — review or delete\")"
    ]
  }'
```

Expected output (if any node is currently below 0.30):

```json
{
  "rules_evaluated": 1,
  "rules_fired": 0,
  "edges_created": 0,
  "flags_raised": 0
}
```

No flags in this case because the claim node is currently at 0.50. In a real system running over months, this rule would catch decayed facts automatically.

#### Step 12: Check the full lifecycle state

```bash
engram stats learning.brain
```

```
Nodes: 7
Edges: 8
```

```bash
curl -s http://127.0.0.1:3030/explain/Jupiter | python3 -m json.tool
```

The `/explain` endpoint returns the full picture: confidence, properties, cooccurrences, and all edges:

```json
{
  "entity": "Jupiter",
  "confidence": 0.95,
  "properties": {
    "moon_count": "95",
    "system": "Solar System"
  },
  "cooccurrences": [
    {"entity": "Sun",       "count": 3, "probability": 0.0},
    {"entity": "gas giant", "count": 2, "probability": 0.0}
  ],
  "edges_from": [
    {"from": "Jupiter", "to": "planet",                    "relationship": "is_a",           "confidence": 0.80},
    {"from": "Jupiter", "to": "gas giant",                 "relationship": "is_a",           "confidence": 0.80},
    {"from": "Jupiter", "to": "Jupiter has solid surface", "relationship": "has_property",   "confidence": 0.70},
    {"from": "Jupiter", "to": "Sun",                       "relationship": "orbits",         "confidence": 0.99},
    {"from": "Jupiter", "to": "Sun",                       "relationship": "part_of_system", "confidence": 0.80}
  ],
  "edges_to": []
}
```

### Full Lifecycle Summary

The following table shows what happened to the Jupiter node across this walkthrough:

| Step | Event | Confidence |
|---|---|---|
| 1 | Stored with explicit confidence 0.30 (LLM-quality claim) | 0.30 |
| 2 | 3 access reinforcements (+0.02 each) | 0.36 |
| 3 | Re-stored with explicit confidence 0.90 (verified fact) | 0.90 |
| 4 | Confirmation reinforcement from NASA source (+0.10) | 0.95 (at cap) |
| 5 | Correction propagation from wrong claim neighbor (-small) | 0.87 |
| 6 | 7 more access reinforcements | 0.95 (at cap) |
| 7 | Memory tier promotion | tier: core |
| 8 | Decay applied (accessed recently, negligible decay) | 0.95 |

### Key Takeaways

- Initial confidence reflects how much you trust the source. LLM assertions start at 0.30; sensor readings start at 0.95. Store data with explicit confidence values to override these defaults.
- Access reinforcement (+0.02) rewards frequently used facts. Confirmation reinforcement (+0.10) rewards independently verified facts. The difference matters: ten accesses from one application equals one external confirmation.
- Correction propagates one hop by default (depth=3 in the implementation). Wrong claims penalize their neighbors, which means garbage in the graph gets penalized outward before it is removed.
- Memory tiers (core / active / archival) give consuming applications a way to prioritize: load only `tier:core` facts for tight context budgets, fall back to `tier:active` for normal queries, and search `tier:archival` only when needed.
- Decay is not automatic — you must call `/learn/decay` on a schedule. Without periodic decay calls, confidence does not change over time.
- All learning operations are idempotent in effect and append-only in the WAL. You can call reinforce or decay multiple times safely.
