# Use Case 7: OSINT -- Open Source Intelligence Gathering

### Overview

Open Source Intelligence (OSINT) involves collecting, correlating, and analyzing publicly available information to build an intelligence picture. Engram's confidence scoring, multi-source provenance, graph traversal, and correction propagation make it a natural fit for OSINT workflows where attribution is uncertain and sources must be cross-referenced.

This walkthrough builds an OSINT knowledge graph that correlates domain registrations, IP addresses, social media handles, and organizational affiliations. It demonstrates how confidence scoring tracks attribution certainty and how correction propagation updates the graph when intelligence is revised.

**What this demonstrates:**
- Multi-source provenance tracking (each source gets its own confidence weight)
- Confidence-based attribution (uncertain links start low, get reinforced)
- Graph traversal to discover hidden connections
- Correction propagation when a source is discredited
- Property-based filtering for structured IOC data

### Prerequisites

- engram binary
- Python 3.8+ with `requests`
- Public data sources (no API keys needed for this walkthrough)

### Step 1: Create the OSINT Knowledge Base

```bash
engram create osint.brain
engram serve osint.brain 127.0.0.1:3030
```

### Step 2: Python OSINT Collector

Full script: [osint_collector.py](osint_collector.py)

```python
import requests
import json

API = "http://127.0.0.1:3030"

def store(entity, entity_type=None, props=None, source="osint", confidence=None):
    body = {"entity": entity, "source": source}
    if entity_type:
        body["type"] = entity_type
    if props:
        body["properties"] = props
    if confidence:
        body["confidence"] = confidence
    return requests.post(f"{API}/store", json=body).json()

def relate(from_e, to_e, rel, confidence=None):
    body = {"from": from_e, "to": to_e, "relationship": rel}
    if confidence:
        body["confidence"] = confidence
    return requests.post(f"{API}/relate", json=body).json()

def reinforce(entity, source=None):
    body = {"entity": entity}
    if source:
        body["source"] = source
    return requests.post(f"{API}/learn/reinforce", json=body).json()

# -- Phase 1: Domain Intelligence --

# Store domains and their registration data
store("example-target.com", "domain", {
    "registrar": "NameCheap",
    "registered": "2024-01-15",
    "nameservers": "ns1.hostingco.net",
    "country": "RU"
}, source="whois-lookup", confidence=0.95)

store("target-services.net", "domain", {
    "registrar": "NameCheap",
    "registered": "2024-01-16",
    "nameservers": "ns1.hostingco.net",
    "country": "RU"
}, source="whois-lookup", confidence=0.95)

# Same registrar + same nameserver + one day apart = likely same operator
relate("example-target.com", "target-services.net",
       "likely_same_operator", confidence=0.7)

# -- Phase 2: IP Infrastructure --

store("198.51.100.42", "ip_address", {
    "asn": "AS12345",
    "isp": "BulletProof Hosting Ltd",
    "country": "NL",
    "first_seen": "2024-02-01"
}, source="passive-dns", confidence=0.90)

relate("example-target.com", "198.51.100.42",
       "resolves_to", confidence=0.95)
relate("target-services.net", "198.51.100.42",
       "resolves_to", confidence=0.95)

# Shared IP reinforces the "same operator" link
reinforce("example-target.com", source="passive-dns")
reinforce("target-services.net", source="passive-dns")

# -- Phase 3: Social Media Correlation --

store("@target_user_42", "social_account", {
    "platform": "twitter",
    "created": "2023-11-20",
    "followers": "127",
    "bio_mentions": "example-target.com"
}, source="social-media-scan", confidence=0.80)

# Bio mentions the domain -- moderate confidence link
relate("@target_user_42", "example-target.com",
       "associated_with", confidence=0.6)

store("targetuser42@proton.me", "email", {
    "provider": "ProtonMail",
    "first_seen_in": "forum-post-2024-03"
}, source="forum-scrape", confidence=0.70)

# Email handle matches social handle -- possible link
relate("targetuser42@proton.me", "@target_user_42",
       "possible_same_person", confidence=0.5)

# -- Phase 4: Organizational Attribution --

store("APT-Phantom", "threat_group", {
    "aliases": "PhantomBear, Group-42",
    "region": "Eastern Europe",
    "active_since": "2022"
}, source="threat-report-vendor-A", confidence=0.75)

# Vendor A attributes the infrastructure to APT-Phantom
relate("198.51.100.42", "APT-Phantom",
       "attributed_to", confidence=0.60)

# Vendor B independently confirms
store("APT-Phantom", "threat_group",
      source="threat-report-vendor-B", confidence=0.85)
reinforce("APT-Phantom", source="threat-report-vendor-B")

# The independent confirmation boosts confidence
print("Two independent sources now corroborate APT-Phantom attribution")
```

### Step 3: Discover Hidden Connections via Graph Traversal

```bash
# Start from the email and traverse 3 hops
curl -s -X POST http://127.0.0.1:3030/query \
  -H "Content-Type: application/json" \
  -d '{"start": "targetuser42@proton.me", "depth": 3, "min_confidence": 0.3}'
```

Expected result: the traversal follows the chain:
```
targetuser42@proton.me
  -> @target_user_42 (possible_same_person)
    -> example-target.com (associated_with)
      -> 198.51.100.42 (resolves_to)
      -> target-services.net (likely_same_operator)
      -> APT-Phantom (attributed_to, via IP)
```

A single email address, through graph traversal, links back to an APT group -- exactly the kind of connection OSINT analysts look for.

### Step 4: Confidence-Based Filtering

```bash
# Show only high-confidence entities (confirmed by multiple sources)
engram search "confidence>0.8" osint.brain
```

```bash
# Show only domain entities
engram search "prop:country=RU" osint.brain
```

```bash
# Show entities attributed with low confidence (needs more evidence)
engram search "confidence<0.6" osint.brain
```

### Step 5: Handle Intelligence Revision

When a source is later discredited or attribution changes:

```python
# Vendor A retracts their APT-Phantom attribution
result = requests.post(f"{API}/learn/correct", json={
    "entity": "APT-Phantom",
    "source": "threat-report-vendor-A",
    "reason": "Retracted: infrastructure overlap was coincidental"
}).json()

print(f"Corrected. Propagated to: {result['propagated_to']}")
# The correction propagates: APT-Phantom confidence drops,
# and entities within 3 hops have their confidence reduced
# by 0.5 damping per hop.
```

After correction, re-query:

```bash
# APT-Phantom now has reduced confidence
curl -s http://127.0.0.1:3030/node/APT-Phantom | python -m json.tool
```

### Step 6: Inference Rules for Automated Correlation

```python
rules = [
    # If two domains share an IP, they are likely related
    """rule shared_infrastructure
when edge(A, "resolves_to", B)
when edge(C, "resolves_to", B)
then edge(A, "shares_infrastructure_with", C, min(e1, e2))""",

    # If a social account is associated with a domain,
    # and the domain is attributed to a group,
    # flag the social account for review
    """rule social_attribution
when edge(A, "associated_with", B)
when edge(B, "attributed_to", C)
then flag(A, "review: linked to threat group via domain")"""
]

result = requests.post(f"{API}/learn/derive", json={"rules": rules}).json()
print(f"Rules evaluated: {result['rules_evaluated']}")
print(f"Rules fired: {result['rules_fired']}")
print(f"Edges created: {result['edges_created']}")
print(f"Flags raised: {result['flags_raised']}")
```

### Key Takeaways

- **Multi-source provenance** is critical in OSINT. Engram tracks which source provided each fact and weights confidence accordingly.
- **Reinforcement from independent sources** increases confidence. Two vendors confirming the same attribution is stronger than one.
- **Correction propagation** handles the real-world scenario where intelligence is revised. The graph automatically adjusts.
- **Graph traversal** discovers non-obvious connections that would be invisible in flat databases.
- **Confidence filtering** lets analysts focus on high-confidence intelligence or identify weak links that need more evidence.
- **Inference rules** automate correlation patterns that analysts would otherwise check manually.
