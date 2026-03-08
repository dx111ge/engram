#!/usr/bin/env python3
"""
Use Case 7: OSINT -- Open Source Intelligence Gathering

Builds an OSINT knowledge graph correlating domains, IPs, social accounts,
emails, and threat group attributions. Demonstrates multi-source provenance,
confidence-based attribution, graph traversal for hidden connections,
inference rules, and correction propagation.

Usage:
  engram serve osint.brain 127.0.0.1:3030
  python osint_demo.py
"""

import json
import sys
import requests

ENGRAM = "http://127.0.0.1:3030"


def api(method, path, payload=None):
    url = f"{ENGRAM}{path}"
    if method == "GET":
        r = requests.get(url, timeout=10)
    elif method == "POST":
        r = requests.post(url, json=payload, timeout=10)
    else:
        raise ValueError(f"Unknown method: {method}")
    r.raise_for_status()
    return r.json()


def store(entity, entity_type=None, properties=None, confidence=None, source=None):
    payload = {"entity": entity}
    if entity_type:
        payload["type"] = entity_type
    if properties:
        payload["properties"] = {k: str(v) for k, v in properties.items()}
    if confidence is not None:
        payload["confidence"] = confidence
    if source:
        payload["source"] = source
    return api("POST", "/store", payload)


def relate(from_e, rel, to_e, confidence=None):
    payload = {"from": from_e, "relationship": rel, "to": to_e}
    if confidence is not None:
        payload["confidence"] = confidence
    return api("POST", "/relate", payload)


def section(title):
    print(f"\n{'=' * 60}")
    print(f"  {title}")
    print(f"{'=' * 60}")


def subsection(title):
    print(f"\n--- {title} ---")


def main():
    try:
        health = api("GET", "/health")
        print(f"Server: {health}")
    except Exception as e:
        print(f"Server not reachable at {ENGRAM}: {e}")
        print("Start engram first: engram serve osint.brain 127.0.0.1:3030")
        sys.exit(1)

    # ================================================================
    section("PHASE 1: Domain Intelligence")
    # ================================================================

    subsection("Store domains from WHOIS lookups")
    store("example-target.com", "domain", {
        "registrar": "NameCheap",
        "registered": "2024-01-15",
        "nameservers": "ns1.hostingco.net",
        "country": "RU",
    }, confidence=0.95, source="whois-lookup")
    print("  example-target.com (RU, NameCheap, conf=0.95)")

    store("target-services.net", "domain", {
        "registrar": "NameCheap",
        "registered": "2024-01-16",
        "nameservers": "ns1.hostingco.net",
        "country": "RU",
    }, confidence=0.95, source="whois-lookup")
    print("  target-services.net (RU, NameCheap, conf=0.95)")

    store("legit-business.org", "domain", {
        "registrar": "GoDaddy",
        "registered": "2020-06-01",
        "country": "US",
    }, confidence=0.90, source="whois-lookup")
    print("  legit-business.org (US, GoDaddy, conf=0.90)")

    # Same registrar + nameserver + 1 day apart = likely same operator
    relate("example-target.com", "likely_same_operator", "target-services.net", 0.70)
    print("  example-target.com -[likely_same_operator]-> target-services.net (0.70)")

    # ================================================================
    section("PHASE 2: IP Infrastructure (Passive DNS)")
    # ================================================================

    subsection("Store IP addresses and DNS resolution")
    store("198.51.100.42", "ip_address", {
        "asn": "AS12345",
        "isp": "BulletProof Hosting Ltd",
        "country": "NL",
        "first_seen": "2024-02-01",
    }, confidence=0.90, source="passive-dns")
    print("  198.51.100.42 (AS12345, NL, conf=0.90)")

    store("203.0.113.77", "ip_address", {
        "asn": "AS67890",
        "isp": "CloudFlare Inc",
        "country": "US",
        "first_seen": "2020-01-01",
    }, confidence=0.90, source="passive-dns")
    print("  203.0.113.77 (AS67890, US, conf=0.90)")

    # DNS resolution
    relate("example-target.com", "resolves_to", "198.51.100.42", 0.95)
    relate("target-services.net", "resolves_to", "198.51.100.42", 0.95)
    relate("legit-business.org", "resolves_to", "203.0.113.77", 0.95)
    print("  Both target domains resolve to same IP: 198.51.100.42")
    print("  legit-business.org resolves to different IP: 203.0.113.77")

    # Shared IP reinforces same-operator link
    api("POST", "/learn/reinforce", {
        "entity": "example-target.com", "source": "passive-dns"
    })
    api("POST", "/learn/reinforce", {
        "entity": "target-services.net", "source": "passive-dns"
    })

    stats = api("GET", "/stats")
    print(f"\nGraph: {stats['nodes']} nodes, {stats['edges']} edges")

    # ================================================================
    section("PHASE 3: Social Media & Email Correlation")
    # ================================================================

    subsection("Social media account linked to domain")
    store("@target_user_42", "social_account", {
        "platform": "twitter",
        "created": "2023-11-20",
        "followers": "127",
        "bio_mentions": "example-target.com",
    }, confidence=0.80, source="social-media-scan")
    relate("@target_user_42", "associated_with", "example-target.com", 0.60)
    print("  @target_user_42 -[associated_with]-> example-target.com (0.60)")

    subsection("Email from forum scrape")
    store("targetuser42@proton.me", "email", {
        "provider": "ProtonMail",
        "first_seen_in": "forum-post-2024-03",
    }, confidence=0.70, source="forum-scrape")
    relate("targetuser42@proton.me", "possible_same_person", "@target_user_42", 0.50)
    print("  targetuser42@proton.me -[possible_same_person]-> @target_user_42 (0.50)")

    # ================================================================
    section("PHASE 4: Threat Group Attribution")
    # ================================================================

    subsection("Vendor A attributes infrastructure to APT-Phantom")
    store("APT-Phantom", "threat_group", {
        "aliases": "PhantomBear, Group-42",
        "region": "Eastern Europe",
        "active_since": "2022",
    }, confidence=0.75, source="threat-report-vendor-A")
    relate("198.51.100.42", "attributed_to", "APT-Phantom", 0.60)
    print("  198.51.100.42 -[attributed_to]-> APT-Phantom (0.60)")

    subsection("Vendor B independently confirms attribution")
    api("POST", "/learn/reinforce", {
        "entity": "APT-Phantom", "source": "threat-report-vendor-B"
    })
    node = api("GET", "/node/APT-Phantom")
    print(f"  APT-Phantom after 2 vendor confirmations: {node['confidence']:.2f}")

    stats = api("GET", "/stats")
    print(f"\nGraph: {stats['nodes']} nodes, {stats['edges']} edges")

    # ================================================================
    section("PHASE 5: Graph Traversal -- Discover Hidden Connections")
    # ================================================================

    subsection("Start from email, traverse 3 hops")
    result = api("POST", "/query", {
        "start": "targetuser42@proton.me",
        "depth": 3,
        "min_confidence": 0.0,
        "direction": "out",
    })
    nodes = result.get("nodes", [])
    print(f"  Reachable: {len(nodes)} nodes from email address")
    for n in sorted(nodes, key=lambda x: (x.get("depth", 0), x["label"])):
        print(f"    [depth={n.get('depth', '?')}] {n['label']} (conf={n['confidence']:.2f})")

    subsection("Reverse: Who connects to APT-Phantom?")
    result = api("POST", "/query", {
        "start": "APT-Phantom",
        "depth": 3,
        "min_confidence": 0.0,
        "direction": "in",
    })
    nodes = result.get("nodes", [])
    upstream = [n["label"] for n in nodes if n["label"] != "APT-Phantom"]
    print(f"  Upstream entities: {upstream}")

    # ================================================================
    section("PHASE 6: Property Search (IOC Hunting)")
    # ================================================================

    subsection("Find all Russian-registered domains")
    result = api("POST", "/search", {"query": "prop:country=RU", "limit": 10})
    hits = [h["label"] for h in result.get("results", [])]
    print(f"  Russian domains: {hits}")

    subsection("Find BulletProof hosting IPs")
    result = api("POST", "/search", {"query": "BulletProof Hosting", "limit": 10})
    hits = [h["label"] for h in result.get("results", [])]
    print(f"  BulletProof Hosting: {hits}")

    subsection("Find ProtonMail accounts")
    result = api("POST", "/search", {"query": "prop:provider=ProtonMail", "limit": 10})
    hits = [h["label"] for h in result.get("results", [])]
    print(f"  ProtonMail accounts: {hits}")

    # ================================================================
    section("PHASE 7: Inference Rules")
    # ================================================================

    subsection("Rule 1: Shared infrastructure correlation")
    rule1 = (
        'rule shared_infra\n'
        'when edge(a, "resolves_to", ip)\n'
        'when edge(b, "resolves_to", ip)\n'
        'then edge(a, "shares_infra_with", b, min(e1, e2))'
    )

    subsection("Rule 2: Social account linked to threat group via domain")
    rule2 = (
        'rule social_threat_link\n'
        'when edge(account, "associated_with", domain)\n'
        'when edge(domain, "resolves_to", ip)\n'
        'when edge(ip, "attributed_to", group)\n'
        'then edge(account, "linked_to_group", group, min(e1, e2))'
    )

    result = api("POST", "/learn/derive", {"rules": [rule1, rule2]})
    print(f"  Rules fired: {result['rules_fired']}")
    print(f"  Edges created: {result['edges_created']}")
    print(f"  Flags raised: {result['flags_raised']}")

    subsection("Verify derived connections")
    resp = api("POST", "/ask", {"question": "What does @target_user_42 connect to?"})
    results = resp.get("results", [])
    for r in results:
        print(f"  @target_user_42 -[{r.get('relationship', '?')}]-> {r['label']}")

    # ================================================================
    section("PHASE 8: Intelligence Revision")
    # ================================================================

    subsection("Before correction: confidence levels")
    for label in ["APT-Phantom", "198.51.100.42", "example-target.com"]:
        node = api("GET", f"/node/{label}")
        print(f"  {label}: {node['confidence']:.2f}")

    subsection("Vendor A retracts attribution")
    result = api("POST", "/learn/correct", {
        "entity": "APT-Phantom",
        "reason": "Retracted: infrastructure overlap was coincidental"
    })
    print(f"  Correction result: {result}")

    subsection("After correction: confidence levels")
    for label in ["APT-Phantom", "198.51.100.42", "example-target.com"]:
        node = api("GET", f"/node/{label}")
        print(f"  {label}: {node['confidence']:.2f}")

    # ================================================================
    section("PHASE 9: Explainability")
    # ================================================================

    subsection("Explain: @target_user_42")
    resp = api("GET", "/explain/@target_user_42")
    print(f"  Confidence: {resp.get('confidence', '?'):.2f}")
    print(f"  Properties: {json.dumps(resp.get('properties', {}), indent=4)}")
    edges_from = resp.get("edges_from", [])
    edges_to = resp.get("edges_to", [])
    print(f"  Outgoing edges ({len(edges_from)}):")
    for e in edges_from:
        print(f"    -[{e['relationship']}]-> {e['to']} (conf={e.get('confidence', '?')})")
    print(f"  Incoming edges ({len(edges_to)}):")
    for e in edges_to:
        print(f"    {e['from']} -[{e['relationship']}]-> (conf={e.get('confidence', '?')})")

    # ================================================================
    section("SUMMARY")
    # ================================================================

    stats = api("GET", "/stats")
    print(f"\n  Final graph: {stats['nodes']} nodes, {stats['edges']} edges")
    print(f"\n  OSINT capabilities demonstrated:")
    print(f"    - Multi-source provenance (WHOIS, passive DNS, social, forums)")
    print(f"    - Confidence-based attribution (uncertain links start low)")
    print(f"    - Graph traversal: email -> social -> domain -> IP -> threat group")
    print(f"    - Inference rules: auto-correlate shared infrastructure")
    print(f"    - Correction propagation when attribution is revised")
    print(f"    - Property search for IOC hunting (country, ISP, provider)")


if __name__ == "__main__":
    main()
