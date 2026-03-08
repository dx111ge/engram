# Use Case 7: OSINT -- Open Source Intelligence Gathering

### Overview

Open Source Intelligence (OSINT) involves collecting, correlating, and analyzing publicly available information to build an intelligence picture. Engram's confidence scoring, multi-source provenance, graph traversal, and correction propagation make it a natural fit for OSINT workflows where attribution is uncertain and sources must be cross-referenced.

This walkthrough builds an OSINT knowledge graph correlating domain registrations, IP addresses, social media handles, emails, and threat group attributions. It demonstrates how graph traversal reveals hidden connections (email -> social -> domain -> IP -> threat group) and how correction propagation updates the graph when intelligence is revised.

**What this demonstrates:**

- Multi-source provenance tracking (WHOIS, passive DNS, social media, forum scrapes, threat reports)
- Confidence-based attribution (uncertain links start low, get reinforced by independent sources)
- Graph traversal to discover hidden connections across 3+ hops
- Property-based IOC hunting (`prop:country=RU`, `prop:provider=ProtonMail`)
- Inference rules to auto-correlate shared infrastructure and social-threat linkages
- Correction propagation when a source is discredited

**What requires external tools:**

- Python script to orchestrate the demo (calls the HTTP API)
- Actual OSINT data collection (WHOIS lookups, passive DNS, social media APIs) is external

### Prerequisites

- `engram` binary on your PATH (or `./target/release/engram`)
- Python 3.9+ with `requests` installed

### Files

```
07-osint/
  README.md          # This file
  osint_demo.py      # Full demo script
```

### Step-by-Step

#### Step 1: Start the engram server

```bash
engram serve osint.brain 127.0.0.1:3030
```

#### Step 2: Run the demo

```bash
python osint_demo.py
```

### What Happens

#### Phase 1: Domain Intelligence

Three domains stored from WHOIS lookups:

| Domain | Registrar | Country | Confidence |
|--------|-----------|---------|------------|
| example-target.com | NameCheap | RU | 0.95 |
| target-services.net | NameCheap | RU | 0.95 |
| legit-business.org | GoDaddy | US | 0.90 |

Same registrar + same nameserver + 1 day apart = `likely_same_operator` (0.70).

#### Phase 2: IP Infrastructure (Passive DNS)

Two IPs stored, DNS resolution mapped:

```
example-target.com  -[resolves_to]-> 198.51.100.42 (BulletProof Hosting, NL)
target-services.net -[resolves_to]-> 198.51.100.42 (same IP!)
legit-business.org  -[resolves_to]-> 203.0.113.77  (CloudFlare, US)
```

Both target domains sharing an IP reinforces the same-operator hypothesis.

After phase 2: **5 nodes, 4 edges**.

#### Phase 3: Social Media & Email Correlation

```
@target_user_42 (twitter, bio mentions example-target.com)
  -[associated_with]-> example-target.com (0.60)

targetuser42@proton.me (forum scrape)
  -[possible_same_person]-> @target_user_42 (0.50)
```

Handle similarity (targetuser42) creates a low-confidence link that can be reinforced later.

#### Phase 4: Threat Group Attribution

Vendor A attributes 198.51.100.42 to APT-Phantom (0.60). Vendor B independently confirms:

```
APT-Phantom after 2 vendor confirmations: 0.85
```

After phase 4: **8 nodes, 7 edges**.

#### Phase 5: Graph Traversal -- Hidden Connections

**Forward traversal** from email address (3 hops):

```
targetuser42@proton.me (depth=0, conf=0.70)
  @target_user_42 (depth=1, conf=0.80)
    example-target.com (depth=2, conf=0.95)
      198.51.100.42 (depth=3, conf=0.90)
      target-services.net (depth=3, conf=0.95)
```

A single email address, through graph traversal, links to the entire infrastructure.

**Reverse traversal** from APT-Phantom (3 hops):

```
Upstream entities: 198.51.100.42, example-target.com, target-services.net, @target_user_42
```

#### Phase 6: IOC Hunting via Property Search

| Query | Result |
|-------|--------|
| `prop:country=RU` | example-target.com, target-services.net |
| `BulletProof Hosting` (text) | 198.51.100.42 |
| `prop:provider=ProtonMail` | targetuser42@proton.me |

#### Phase 7: Inference Rules

**Rule 1: Shared infrastructure** -- if two domains resolve to the same IP, derive a `shares_infra_with` edge.

**Rule 2: Social-threat linkage** -- if a social account is associated with a domain that resolves to an IP attributed to a threat group, derive `linked_to_group`.

Result: **12 rules fired, 6 edges created** (shared infra between domain pairs, social-to-group link).

Derived connection verified:

```
@target_user_42 -[associated_with]-> example-target.com
@target_user_42 -[linked_to_group]-> APT-Phantom   (derived!)
```

After inference: **8 nodes, 13 edges** (+6 derived).

#### Phase 8: Intelligence Revision

Vendor A retracts their APT-Phantom attribution:

```
Before: APT-Phantom confidence = 0.85
After:  APT-Phantom confidence = 0.00
```

The IP and domain confidence remain intact (0.90, 0.95) -- the infrastructure facts are still valid, only the attribution is discredited.

#### Phase 9: Explainability

```bash
curl -s http://127.0.0.1:3030/explain/@target_user_42
```

Returns:
- **Confidence**: 0.80
- **Properties**: platform=twitter, followers=127, bio_mentions=example-target.com
- **Outgoing**: associated_with example-target.com (0.60), linked_to_group APT-Phantom (0.50)
- **Incoming**: targetuser42@proton.me possible_same_person (0.50)

### Key Takeaways

- **Multi-source provenance** is critical in OSINT. Engram tracks which source provided each fact and weights confidence accordingly.
- **Graph traversal reveals hidden connections**. A single email address, through 3 hops, links to a threat group. This connection is invisible in flat databases.
- **Confidence filtering** lets analysts focus on high-confidence intelligence or identify weak links that need more evidence.
- **Inference rules automate correlation**. "Two domains sharing an IP" and "social account linked to threat group via domain" are patterns analysts check manually -- rules automate this.
- **Correction propagation** handles intelligence revision. When attribution is retracted, the graph adjusts without manual cleanup.
- **Property search** enables structured IOC hunting (country codes, ISP names, email providers) alongside text search.
