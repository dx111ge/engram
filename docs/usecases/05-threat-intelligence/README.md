# Use Case 5: Threat Intelligence Knowledge Graph

### Overview

Threat intelligence analysts track relationships between threat actors, malware families, CVEs, attack techniques (TTPs in MITRE ATT&CK terminology), and indicators of compromise (IOCs). These relationships are exactly what a knowledge graph is designed for: many-to-many, evolving, with confidence that degrades when attribution changes.

This walkthrough builds a small threat intelligence graph, demonstrates confidence-scored attribution, uses graph traversal to enumerate attack chains, and shows how to propagate distrust when attribution changes.

**What this demonstrates today (v0.1.0):**

- Storing threat actors, malware, CVEs, and TTPs as typed nodes
- IOCs stored as node properties
- Confidence scoring for threat attribution (attribution is rarely certain)
- Graph traversal to enumerate all TTPs used by a threat actor via malware
- Correction propagation when attribution changes
- Inference rules to propagate threat levels

**What requires external tools:**

- STIX/TAXII feeds require external tooling to call engram's HTTP API
- Automated IOC matching against firewall/SIEM logs requires your SIEM, not engram

### Prerequisites

- `engram` binary on your PATH
- `curl` for HTTP API calls

### Step-by-Step Implementation

#### Step 1: Start the server

```bash
engram serve threat.brain 127.0.0.1:3030
```

#### Step 2: Store threat actors

```bash
# Threat actor APT-X (fictional, for illustration)
curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{
    "entity": "APT-X",
    "type": "threat_actor",
    "properties": {
      "motivation": "espionage",
      "origin": "unknown",
      "aliases": "ShadowFox, Group-42",
      "active_since": "2019"
    },
    "confidence": 0.80
  }'

curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{
    "entity": "APT-Y",
    "type": "threat_actor",
    "properties": {
      "motivation": "financial",
      "origin": "unknown",
      "active_since": "2021"
    },
    "confidence": 0.70
  }'
```

#### Step 3: Store malware families

```bash
curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{
    "entity": "DarkLoader",
    "type": "malware",
    "properties": {
      "category": "loader",
      "delivery": "spear-phishing",
      "ioc_hash_sha256": "a3f1b2c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2",
      "ioc_c2_domain": "update-cdn-proxy.net",
      "first_seen": "2022-03-15"
    },
    "confidence": 0.85
  }'

curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{
    "entity": "MemScraper",
    "type": "malware",
    "properties": {
      "category": "credential_harvester",
      "delivery": "dropper",
      "ioc_hash_sha256": "b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5",
      "first_seen": "2023-01-08"
    },
    "confidence": 0.75
  }'
```

#### Step 4: Store CVEs

```bash
curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{
    "entity": "CVE-2024-1234",
    "type": "vulnerability",
    "properties": {
      "cvss_score": "9.8",
      "affected_product": "ExampleCMS 3.x",
      "patch_available": "true",
      "vector": "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H"
    },
    "confidence": 0.95
  }'

curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{
    "entity": "CVE-2023-5678",
    "type": "vulnerability",
    "properties": {
      "cvss_score": "7.5",
      "affected_product": "OpenSSL 3.0.x",
      "patch_available": "true"
    },
    "confidence": 0.95
  }'
```

#### Step 5: Store TTPs (MITRE ATT&CK style)

```bash
# T1566 — Phishing
curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{
    "entity": "T1566-Phishing",
    "type": "ttp",
    "properties": {"tactic": "Initial Access", "mitre_id": "T1566"}
  }'

# T1059 — Command and Scripting Interpreter
curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{
    "entity": "T1059-ScriptingInterpreter",
    "type": "ttp",
    "properties": {"tactic": "Execution", "mitre_id": "T1059"}
  }'

# T1003 — Credential Dumping
curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{
    "entity": "T1003-CredentialDumping",
    "type": "ttp",
    "properties": {"tactic": "Credential Access", "mitre_id": "T1003"}
  }'

# T1071 — Application Layer Protocol (C2)
curl -s -X POST http://127.0.0.1:3030/store \
  -H "Content-Type: application/json" \
  -d '{
    "entity": "T1071-AppLayerProtocol",
    "type": "ttp",
    "properties": {"tactic": "Command and Control", "mitre_id": "T1071"}
  }'
```

#### Step 6: Build the relationship graph

```bash
# Threat actor uses malware
curl -s -X POST http://127.0.0.1:3030/relate \
  -H "Content-Type: application/json" \
  -d '{"from":"APT-X","relationship":"uses","to":"DarkLoader","confidence":0.80}'

curl -s -X POST http://127.0.0.1:3030/relate \
  -H "Content-Type: application/json" \
  -d '{"from":"APT-X","relationship":"uses","to":"MemScraper","confidence":0.65}'

# Malware exploits CVEs
curl -s -X POST http://127.0.0.1:3030/relate \
  -H "Content-Type: application/json" \
  -d '{"from":"DarkLoader","relationship":"exploits","to":"CVE-2024-1234","confidence":0.85}'

# Malware implements TTPs
curl -s -X POST http://127.0.0.1:3030/relate \
  -H "Content-Type: application/json" \
  -d '{"from":"DarkLoader","relationship":"implements","to":"T1566-Phishing","confidence":0.85}'

curl -s -X POST http://127.0.0.1:3030/relate \
  -H "Content-Type: application/json" \
  -d '{"from":"DarkLoader","relationship":"implements","to":"T1059-ScriptingInterpreter","confidence":0.80}'

curl -s -X POST http://127.0.0.1:3030/relate \
  -H "Content-Type: application/json" \
  -d '{"from":"DarkLoader","relationship":"implements","to":"T1071-AppLayerProtocol","confidence":0.75}'

curl -s -X POST http://127.0.0.1:3030/relate \
  -H "Content-Type: application/json" \
  -d '{"from":"MemScraper","relationship":"implements","to":"T1003-CredentialDumping","confidence":0.75}'

curl -s -X POST http://127.0.0.1:3030/relate \
  -H "Content-Type: application/json" \
  -d '{"from":"MemScraper","relationship":"implements","to":"T1059-ScriptingInterpreter","confidence":0.70}'

# Second actor uses a CVE too (shared vulnerability)
curl -s -X POST http://127.0.0.1:3030/relate \
  -H "Content-Type: application/json" \
  -d '{"from":"APT-Y","relationship":"exploits","to":"CVE-2023-5678","confidence":0.60}'
```

### Querying the Results

#### Find all TTPs used by APT-X (2-hop traversal through malware)

```bash
curl -s -X POST http://127.0.0.1:3030/query \
  -H "Content-Type: application/json" \
  -d '{"start":"APT-X","depth":2,"min_confidence":0.6}'
```

Expected output — note depth values showing the path:

```json
{
  "nodes": [
    {"label": "APT-X",                     "confidence": 0.80, "depth": 0},
    {"label": "DarkLoader",                 "confidence": 0.85, "depth": 1},
    {"label": "MemScraper",                 "confidence": 0.75, "depth": 1},
    {"label": "CVE-2024-1234",              "confidence": 0.95, "depth": 2},
    {"label": "T1566-Phishing",             "confidence": 1.0,  "depth": 2},
    {"label": "T1059-ScriptingInterpreter", "confidence": 1.0,  "depth": 2},
    {"label": "T1071-AppLayerProtocol",     "confidence": 1.0,  "depth": 2},
    {"label": "T1003-CredentialDumping",    "confidence": 1.0,  "depth": 2}
  ],
  "edges": [...]
}
```

The 2-hop traversal gives you the complete attack chain: threat actor, tools, techniques. Filter `min_confidence: 0.6` excludes very uncertain attributions.

#### Search by CVE ID

```bash
curl -s -X POST http://127.0.0.1:3030/search \
  -H "Content-Type: application/json" \
  -d '{"query":"CVE-2024-1234","limit":5}'
```

Expected output:

```json
{
  "results": [{"label": "CVE-2024-1234", "confidence": 0.95, "score": 3.1}],
  "total": 1
}
```

#### Find all high-CVSS vulnerabilities

```bash
curl -s -X POST http://127.0.0.1:3030/search \
  -H "Content-Type: application/json" \
  -d '{"query":"prop:cvss_score=9.8","limit":10}'
```

Expected output:

```json
{
  "results": [{"label": "CVE-2024-1234", "confidence": 0.95, "score": 1.0}],
  "total": 1
}
```

#### Find what connects to a CVE (who exploits it)

```bash
curl -s -X POST http://127.0.0.1:3030/ask \
  -H "Content-Type: application/json" \
  -d '{"question":"What connects to CVE-2024-1234?"}'
```

Expected output:

```json
{
  "interpretation": "incoming edges to: CVE-2024-1234",
  "results": [
    {"label": "DarkLoader", "confidence": 0.85, "relationship": "exploits", "detail": null}
  ]
}
```

#### Apply an inference rule to propagate threat level

Define a rule: if a threat actor uses malware and that malware exploits a CVE, then the threat actor has a direct connection to that CVE at reduced confidence:

```bash
curl -s -X POST http://127.0.0.1:3030/learn/derive \
  -H "Content-Type: application/json" \
  -d '{
    "rules": [
      "rule actor_exploits_via_malware\nwhen edge(ACTOR, \"uses\", MALWARE)\nwhen edge(MALWARE, \"exploits\", CVE)\nthen edge(ACTOR, \"associated_with\", CVE, min(e1, e2))"
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

Now `APT-X -[associated_with]-> CVE-2024-1234` exists with confidence `min(0.80, 0.85) = 0.80`.

#### Simulate attribution change — correct and propagate distrust

Intelligence changes: new analysis suggests DarkLoader was actually operated by APT-Y, not APT-X. Correct the APT-X attribution:

```bash
# The attribution APT-X -> DarkLoader was wrong
curl -s -X POST http://127.0.0.1:3030/learn/correct \
  -H "Content-Type: application/json" \
  -d '{
    "entity": "APT-X",
    "reason": "re-attribution: DarkLoader infrastructure traced to APT-Y cluster",
    "source": "threat-intel-report-2026-03"
  }'
```

Expected output:

```json
{
  "corrected": "APT-X",
  "propagated_to": ["DarkLoader", "MemScraper"]
}
```

The correction penalty (-0.20) drops APT-X's confidence from 0.80 to 0.60. The penalty propagates to DarkLoader and MemScraper (neighbors), dropping their attributed confidence. The CVE and TTP nodes are unaffected — they are facts, not attributions.

#### Add the correct attribution

```bash
curl -s -X POST http://127.0.0.1:3030/relate \
  -H "Content-Type: application/json" \
  -d '{"from":"APT-Y","relationship":"uses","to":"DarkLoader","confidence":0.75}'

# Reinforce based on the new report
curl -s -X POST http://127.0.0.1:3030/learn/reinforce \
  -H "Content-Type: application/json" \
  -d '{"entity":"APT-Y","source":"threat-intel-report-2026-03"}'
```

Expected output:

```json
{"entity":"APT-Y","new_confidence":0.8}
```

### Key Takeaways

- Threat intelligence is naturally graph-shaped: actors use tools, tools exploit CVEs, CVEs enable TTPs. A 2-hop traversal from any threat actor surfaces the complete attack chain.
- IOCs stored as node properties are searchable with property filters — no separate IOC database required.
- Confidence scoring matters for attribution. Intelligence analysts rarely have certainty; the confidence model reflects that.
- The correction endpoint propagates distrust through the graph when attribution changes, avoiding silent orphan facts.
- Inference rules can derive "actor is associated with CVE" edges without analysts creating them manually for every combination.
