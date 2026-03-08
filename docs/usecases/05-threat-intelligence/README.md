# Use Case 5: Threat Intelligence Knowledge Graph

### Overview

Threat intelligence analysts track relationships between threat actors, malware families, CVEs, attack techniques (TTPs in MITRE ATT&CK terminology), and indicators of compromise (IOCs). These relationships are exactly what a knowledge graph is designed for: many-to-many, evolving, with confidence that degrades when attribution changes.

This walkthrough builds a threat intelligence graph with 18 nodes and 19 relationships, then demonstrates attack chain traversal, IOC search, inference rules, attribution correction with distrust propagation, and full explainability.

**What this demonstrates:**

- Storing threat actors, malware, CVEs, TTPs, and target sectors as typed nodes
- IOCs stored as node properties (SHA-256 hashes, C2 domains, CVSS vectors)
- Attack chain analysis via depth-2 forward/reverse traversal
- Property-based filtering (`prop:cvss_score=9.8`, `prop:patch_available=false`, `prop:motivation=espionage`)
- Inference rules: derive actor-CVE associations, flag actors with critical CVE exposure
- Attribution correction with distrust propagation across the graph
- Full explainability via `/explain`

**What requires external tools:**

- Python script to orchestrate the demo (calls the HTTP API)
- STIX/TAXII feeds require external tooling to call engram's HTTP API
- Automated IOC matching against firewall/SIEM logs requires your SIEM, not engram

### Prerequisites

- `engram` binary on your PATH (or `./target/release/engram`)
- Python 3.9+ with `requests` installed

### Files

```
05-threat-intelligence/
  README.md                # This file
  threat_intel_demo.py     # Full demo script
```

### Step-by-Step

#### Step 1: Start the engram server

```bash
engram serve threat.brain 127.0.0.1:3030
```

#### Step 2: Run the demo

```bash
python threat_intel_demo.py
```

### What Happens

#### Phase 1: Build the Threat Intelligence Graph

The script stores 18 nodes across 5 types:

| Type           | Nodes                                                     | Confidence |
|----------------|-----------------------------------------------------------|------------|
| threat_actor   | APT-X (espionage), APT-Y (financial), APT-Z (hacktivism) | 0.60-0.80  |
| malware        | DarkLoader (loader), MemScraper (credential harvester), RansomCrypt (ransomware) | 0.70-0.85 |
| vulnerability  | CVE-2024-1234 (CVSS 9.8), CVE-2023-5678 (7.5), CVE-2024-9012 (8.1) | 0.90-0.95 |
| ttp            | T1566-Phishing, T1059-Scripting, T1003-CredDump, T1071-C2, T1486-Encryption, T1190-ExploitApp | 0.95 |
| target_sector  | FinancialSector, HealthcareSector, GovernmentSector       | 0.80-0.90  |

19 relationships connect them:

```
APT-X -[uses]-> DarkLoader (0.80)
APT-X -[uses]-> MemScraper (0.65)
APT-Y -[uses]-> RansomCrypt (0.75)
APT-Z -[uses]-> DarkLoader (0.45)        # low-confidence attribution

DarkLoader -[exploits]-> CVE-2024-1234 (0.85)
RansomCrypt -[exploits]-> CVE-2024-9012 (0.80)
APT-Y -[exploits]-> CVE-2023-5678 (0.60)

DarkLoader -[implements]-> T1566-Phishing (0.85)
DarkLoader -[implements]-> T1059-ScriptingInterpreter (0.80)
DarkLoader -[implements]-> T1071-AppLayerProtocol (0.75)
MemScraper -[implements]-> T1003-CredentialDumping (0.75)
MemScraper -[implements]-> T1059-ScriptingInterpreter (0.70)
RansomCrypt -[implements]-> T1486-DataEncryption (0.90)
RansomCrypt -[implements]-> T1190-ExploitPublicApp (0.80)

APT-X -[targets]-> GovernmentSector (0.75)
APT-X -[targets]-> FinancialSector (0.60)
APT-Y -[targets]-> HealthcareSector (0.70)
APT-Y -[targets]-> FinancialSector (0.80)
APT-Z -[targets]-> GovernmentSector (0.50)
```

After phase 1: **18 nodes, 19 edges**.

#### Phase 2: Attack Chain Analysis

**Forward traversal**: APT-X's attack chain (depth=2, min_confidence=0.5) reveals 10 nodes:

```
APT-X (depth=0, conf=0.80)
  DarkLoader (depth=1, conf=0.85)
  MemScraper (depth=1, conf=0.75)
  FinancialSector (depth=1, conf=0.90)
  GovernmentSector (depth=1, conf=0.80)
    CVE-2024-1234 (depth=2, conf=0.95)
    T1566-Phishing (depth=2, conf=0.95)
    T1059-ScriptingInterpreter (depth=2, conf=0.95)
    T1071-AppLayerProtocol (depth=2, conf=0.95)
    T1003-CredentialDumping (depth=2, conf=0.95)
```

TTPs used by APT-X: `T1566-Phishing, T1059-ScriptingInterpreter, T1071-AppLayerProtocol, T1003-CredentialDumping`

**Reverse traversal**: Who exploits CVE-2024-1234? Direction=`in`, depth=2 surfaces: `DarkLoader, APT-X, APT-Z`

**NL query**: "What connects to FinancialSector?" returns: `APT-X (targets), APT-Y (targets)`

#### Phase 3: IOC & Property Search

| Query | Result |
|-------|--------|
| `update-cdn-proxy.net` (C2 domain) | DarkLoader |
| `CVE-2024-1234` (text search) | CVE-2024-1234, CVE-2024-9012, CVE-2023-5678, RansomCrypt |
| `prop:cvss_score=9.8` | CVE-2024-1234 |
| `prop:patch_available=false` | CVE-2024-9012 |
| `prop:motivation=espionage` | APT-X |
| `prop:category=credential_harvester` | MemScraper |

#### Phase 4: Inference Rules

**Rule 1: Actor-CVE association** -- if actor uses malware and malware exploits CVE, derive actor-CVE link:

```
rule actor_exploits_via_malware
when edge(actor, "uses", malware)
when edge(malware, "exploits", cve)
then edge(actor, "associated_with", cve, min(e1, e2))
```

Result: **6 rules fired, 3 edges created** (APT-X -> CVE-2024-1234, APT-Y -> CVE-2024-9012, APT-Z -> CVE-2024-1234)

**Rule 2: Critical CVE flagging** -- flag any actor associated with a CVSS 9.8 CVE:

```
rule critical_cve_exposure
when edge(actor, "associated_with", cve)
when prop(cve, "cvss_score", "9.8")
then flag(actor, "associated with critical CVE")
```

Result: **4 rules fired, 2 flags raised** -- APT-X and APT-Z flagged (both linked to CVE-2024-1234 via DarkLoader).

After inference: **18 nodes, 22 edges** (+3 derived).

#### Phase 5: Attribution Change -- Correct & Propagate

New intelligence: DarkLoader was re-attributed from APT-X to APT-Y.

**Correction**: `/learn/correct` zeroes APT-X's confidence and propagates distrust:

```
Before: APT-X confidence = 0.80
After:  APT-X confidence = 0.00

Distrust propagated to: DarkLoader, MemScraper, GovernmentSector,
  FinancialSector, CVE-2024-1234, T1566-Phishing,
  T1059-ScriptingInterpreter, T1071-AppLayerProtocol,
  T1003-CredentialDumping
```

**Re-attribution**: Add APT-Y -> DarkLoader (0.75), reinforce APT-Y.

**Final confidence comparison**:

| Actor | Before | After |
|-------|--------|-------|
| APT-X | 0.80   | 0.00  |
| APT-Y | 0.70   | 0.80  |
| APT-Z | 0.60   | 0.60  |

#### Phase 6: Explainability

```bash
curl -s http://127.0.0.1:3030/explain/DarkLoader
```

Returns:
- **Confidence**: 0.45 (reduced by distrust propagation from APT-X correction)
- **Properties**: category=loader, delivery=spear-phishing, C2 domain, SHA-256 hash, first_seen
- **Outgoing edges** (4): exploits CVE-2024-1234, implements T1566/T1059/T1071
- **Incoming edges** (3): used by APT-X (0.80), APT-Z (0.45), APT-Y (0.75)

### Key Takeaways

- **Attack chain traversal**: 2-hop traversal from any threat actor surfaces the complete kill chain -- tools, techniques, and exploited vulnerabilities.
- **IOC search via properties**: C2 domains, hashes, CVSS scores are stored as node properties and searchable with `prop:key=value` filters. No separate IOC database needed.
- **Inference rules derive relationships automatically**: "actor uses malware that exploits CVE" produces "actor associated_with CVE" without analysts manually creating every combination. Rules run to fixed point in a single API call.
- **Critical CVE flagging**: Combining edge traversal with property matching flags actors with critical exposure automatically.
- **Attribution correction propagates distrust**: When APT-X is corrected, confidence drops propagate through the graph to all connected nodes. The correction is not local -- it reflects the reality that wrong attribution invalidates downstream analysis.
- **Confidence lifecycle matters**: Threat intelligence is rarely certain. The confidence model reflects that, and the reinforce/correct/decay cycle keeps the graph current as new intel arrives.
