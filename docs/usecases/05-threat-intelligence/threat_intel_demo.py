#!/usr/bin/env python3
"""
Use Case 5: Threat Intelligence Knowledge Graph

Builds a threat intelligence graph with actors, malware, CVEs, and TTPs.
Demonstrates attribution, attack chain traversal, IOC search, inference
rules, and attribution correction with distrust propagation.

Usage:
  engram serve threat.brain 127.0.0.1:3030
  python threat_intel_demo.py
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


def store(entity, entity_type=None, properties=None, confidence=None):
    payload = {"entity": entity}
    if entity_type:
        payload["type"] = entity_type
    if properties:
        payload["properties"] = {k: str(v) for k, v in properties.items()}
    if confidence is not None:
        payload["confidence"] = confidence
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
        print("Start engram first: engram serve threat.brain 127.0.0.1:3030")
        sys.exit(1)

    # ================================================================
    section("PHASE 1: Build the Threat Intelligence Graph")
    # ================================================================

    # Threat actors
    actors = [
        ("APT-X", "threat_actor", {
            "motivation": "espionage", "origin": "unknown",
            "aliases": "ShadowFox, Group-42", "active_since": "2019",
        }, 0.80),
        ("APT-Y", "threat_actor", {
            "motivation": "financial", "origin": "unknown",
            "active_since": "2021",
        }, 0.70),
        ("APT-Z", "threat_actor", {
            "motivation": "hacktivism", "origin": "unknown",
            "active_since": "2023",
        }, 0.60),
    ]

    # Malware families
    malware = [
        ("DarkLoader", "malware", {
            "category": "loader", "delivery": "spear-phishing",
            "ioc_hash_sha256": "a3f1b2c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2",
            "ioc_c2_domain": "update-cdn-proxy.net",
            "first_seen": "2022-03-15",
        }, 0.85),
        ("MemScraper", "malware", {
            "category": "credential_harvester", "delivery": "dropper",
            "ioc_hash_sha256": "b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5",
            "first_seen": "2023-01-08",
        }, 0.75),
        ("RansomCrypt", "malware", {
            "category": "ransomware", "delivery": "exploit_kit",
            "ioc_c2_domain": "payment-gateway-secure.org",
            "first_seen": "2024-06-01",
        }, 0.70),
    ]

    # CVEs
    cves = [
        ("CVE-2024-1234", "vulnerability", {
            "cvss_score": "9.8", "affected_product": "ExampleCMS 3.x",
            "patch_available": "true",
            "vector": "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H",
        }, 0.95),
        ("CVE-2023-5678", "vulnerability", {
            "cvss_score": "7.5", "affected_product": "OpenSSL 3.0.x",
            "patch_available": "true",
        }, 0.95),
        ("CVE-2024-9012", "vulnerability", {
            "cvss_score": "8.1", "affected_product": "Apache Struts 2.x",
            "patch_available": "false",
        }, 0.90),
    ]

    # TTPs (MITRE ATT&CK)
    ttps = [
        ("T1566-Phishing", "ttp", {"tactic": "Initial Access", "mitre_id": "T1566"}),
        ("T1059-ScriptingInterpreter", "ttp", {"tactic": "Execution", "mitre_id": "T1059"}),
        ("T1003-CredentialDumping", "ttp", {"tactic": "Credential Access", "mitre_id": "T1003"}),
        ("T1071-AppLayerProtocol", "ttp", {"tactic": "Command and Control", "mitre_id": "T1071"}),
        ("T1486-DataEncryption", "ttp", {"tactic": "Impact", "mitre_id": "T1486"}),
        ("T1190-ExploitPublicApp", "ttp", {"tactic": "Initial Access", "mitre_id": "T1190"}),
    ]

    # Targets
    targets = [
        ("FinancialSector", "target_sector", {"region": "global"}, 0.90),
        ("HealthcareSector", "target_sector", {"region": "EU"}, 0.85),
        ("GovernmentSector", "target_sector", {"region": "NA"}, 0.80),
    ]

    print("\nStoring threat actors...")
    for name, stype, props, conf in actors:
        r = store(name, stype, props, conf)
        print(f"  {name} ({props['motivation']}, conf={conf})")

    print("\nStoring malware families...")
    for name, stype, props, conf in malware:
        r = store(name, stype, props, conf)
        print(f"  {name} ({props['category']})")

    print("\nStoring CVEs...")
    for name, stype, props, conf in cves:
        r = store(name, stype, props, conf)
        print(f"  {name} (CVSS {props['cvss_score']})")

    print("\nStoring TTPs...")
    for name, stype, props in ttps:
        store(name, stype, props, confidence=0.95)
        print(f"  {name} ({props['tactic']})")

    print("\nStoring target sectors...")
    for name, stype, props, conf in targets:
        store(name, stype, props, conf)
        print(f"  {name}")

    # Relationships
    print("\nCreating relationships...")
    relations = [
        # Actor -> Malware (attribution)
        ("APT-X", "uses", "DarkLoader", 0.80),
        ("APT-X", "uses", "MemScraper", 0.65),
        ("APT-Y", "uses", "RansomCrypt", 0.75),
        ("APT-Z", "uses", "DarkLoader", 0.45),  # low confidence attribution

        # Malware -> CVE (exploitation)
        ("DarkLoader", "exploits", "CVE-2024-1234", 0.85),
        ("RansomCrypt", "exploits", "CVE-2024-9012", 0.80),
        ("APT-Y", "exploits", "CVE-2023-5678", 0.60),

        # Malware -> TTP (techniques)
        ("DarkLoader", "implements", "T1566-Phishing", 0.85),
        ("DarkLoader", "implements", "T1059-ScriptingInterpreter", 0.80),
        ("DarkLoader", "implements", "T1071-AppLayerProtocol", 0.75),
        ("MemScraper", "implements", "T1003-CredentialDumping", 0.75),
        ("MemScraper", "implements", "T1059-ScriptingInterpreter", 0.70),
        ("RansomCrypt", "implements", "T1486-DataEncryption", 0.90),
        ("RansomCrypt", "implements", "T1190-ExploitPublicApp", 0.80),

        # Actor -> Target (targeting)
        ("APT-X", "targets", "GovernmentSector", 0.75),
        ("APT-X", "targets", "FinancialSector", 0.60),
        ("APT-Y", "targets", "HealthcareSector", 0.70),
        ("APT-Y", "targets", "FinancialSector", 0.80),
        ("APT-Z", "targets", "GovernmentSector", 0.50),
    ]

    for from_e, rel, to_e, conf in relations:
        relate(from_e, rel, to_e, confidence=conf)
        print(f"  {from_e} -[{rel}]-> {to_e} ({conf})")

    stats = api("GET", "/stats")
    print(f"\nGraph: {stats['nodes']} nodes, {stats['edges']} edges")

    # ================================================================
    section("PHASE 2: Attack Chain Analysis")
    # ================================================================

    subsection("APT-X attack chain (2-hop traversal)")
    result = api("POST", "/query", {
        "start": "APT-X", "depth": 2, "min_confidence": 0.5, "direction": "out"
    })
    nodes = result.get("nodes", [])
    print(f"  Reachable: {len(nodes)} nodes")
    for n in sorted(nodes, key=lambda x: (x.get("depth", 0), x["label"])):
        print(f"    [depth={n.get('depth', '?')}] {n['label']} (conf={n['confidence']:.2f})")

    subsection("What TTPs does APT-X use?")
    # TTPs are at depth 2: APT-X -> malware -> TTP
    ttps_found = [n["label"] for n in nodes if n["label"].startswith("T1")]
    print(f"  TTPs: {ttps_found}")

    subsection("Who exploits CVE-2024-1234? (reverse traversal)")
    result = api("POST", "/query", {
        "start": "CVE-2024-1234", "depth": 2, "min_confidence": 0.0, "direction": "in"
    })
    exploiters = [n["label"] for n in result.get("nodes", []) if n["label"] != "CVE-2024-1234"]
    print(f"  Exploited by: {exploiters}")

    subsection("Who targets the Financial Sector?")
    resp = api("POST", "/ask", {"question": "What connects to FinancialSector?"})
    results = resp.get("results", [])
    print(f"  Actors: {[(r['label'], r.get('relationship')) for r in results]}")

    # ================================================================
    section("PHASE 3: IOC & Property Search")
    # ================================================================

    subsection("Search by C2 domain")
    result = api("POST", "/search", {"query": "update-cdn-proxy.net", "limit": 5})
    hits = result.get("results", [])
    print(f"  Found: {[h['label'] for h in hits]}")

    subsection("Search by CVE ID")
    result = api("POST", "/search", {"query": "CVE-2024-1234", "limit": 5})
    hits = result.get("results", [])
    print(f"  Found: {[h['label'] for h in hits]}")

    subsection("Find critical CVSS vulnerabilities")
    result = api("POST", "/search", {"query": "prop:cvss_score=9.8", "limit": 10})
    hits = result.get("results", [])
    print(f"  CVSS 9.8: {[h['label'] for h in hits]}")

    subsection("Find unpatched vulnerabilities")
    result = api("POST", "/search", {"query": "prop:patch_available=false", "limit": 10})
    hits = result.get("results", [])
    print(f"  Unpatched: {[h['label'] for h in hits]}")

    subsection("Find all espionage actors")
    result = api("POST", "/search", {"query": "prop:motivation=espionage", "limit": 10})
    hits = result.get("results", [])
    print(f"  Espionage: {[h['label'] for h in hits]}")

    subsection("Find credential-related malware")
    result = api("POST", "/search", {"query": "prop:category=credential_harvester", "limit": 10})
    hits = result.get("results", [])
    print(f"  Credential harvesters: {[h['label'] for h in hits]}")

    # ================================================================
    section("PHASE 4: Inference Rules")
    # ================================================================

    subsection("Rule: Actor associated with CVE via malware")
    rule1 = (
        'rule actor_exploits_via_malware\n'
        'when edge(actor, "uses", malware)\n'
        'when edge(malware, "exploits", cve)\n'
        'then edge(actor, "associated_with", cve, min(e1, e2))'
    )
    result = api("POST", "/learn/derive", {"rules": [rule1]})
    print(f"  Rules fired: {result['rules_fired']}, edges created: {result['edges_created']}")

    subsection("Rule: Flag actors with critical CVE exposure")
    rule2 = (
        'rule critical_cve_exposure\n'
        'when edge(actor, "associated_with", cve)\n'
        'when prop(cve, "cvss_score", "9.8")\n'
        'then flag(actor, "associated with critical CVE")'
    )
    result = api("POST", "/learn/derive", {"rules": [rule2]})
    print(f"  Rules fired: {result['rules_fired']}, flags raised: {result['flags_raised']}")

    subsection("Verify: APT-X now linked to CVE-2024-1234?")
    resp = api("POST", "/ask", {"question": "What does APT-X connect to?"})
    results = resp.get("results", [])
    cve_links = [(r["label"], r.get("relationship")) for r in results
                 if r["label"].startswith("CVE")]
    print(f"  CVE associations: {cve_links}")

    subsection("Check flags")
    for actor, _, _, _ in actors:
        try:
            node = api("GET", f"/node/{actor}")
            flag = node.get("properties", {}).get("_flag")
            if flag:
                print(f"  FLAGGED: {actor} -- {flag}")
        except Exception:
            pass

    stats = api("GET", "/stats")
    print(f"\n  Graph after inference: {stats['nodes']} nodes, {stats['edges']} edges")

    # ================================================================
    section("PHASE 5: Attribution Change -- Correct & Propagate")
    # ================================================================

    subsection("Before correction: APT-X confidence")
    node = api("GET", "/node/APT-X")
    print(f"  APT-X confidence: {node['confidence']:.2f}")

    subsection("New intel: DarkLoader re-attributed to APT-Y")
    result = api("POST", "/learn/correct", {
        "entity": "APT-X",
        "reason": "re-attribution: DarkLoader traced to APT-Y cluster"
    })
    print(f"  Correction result: {result}")

    node = api("GET", "/node/APT-X")
    print(f"  APT-X confidence after: {node['confidence']:.2f}")

    subsection("Add correct attribution")
    relate("APT-Y", "uses", "DarkLoader", confidence=0.75)
    print("  APT-Y -[uses]-> DarkLoader (0.75)")

    # Reinforce APT-Y based on new report
    r = api("POST", "/learn/reinforce", {
        "entity": "APT-Y", "source": "threat-intel-report-2026-03"
    })
    print(f"  APT-Y reinforced: confidence = {r['new_confidence']:.2f}")

    subsection("After correction: confidence comparison")
    for actor, _, _, _ in actors:
        try:
            node = api("GET", f"/node/{actor}")
            print(f"  {actor}: {node['confidence']:.2f}")
        except Exception:
            pass

    # ================================================================
    section("PHASE 6: Explainability")
    # ================================================================

    subsection("Explain: DarkLoader")
    resp = api("GET", "/explain/DarkLoader")
    print(f"  Confidence: {resp.get('confidence', '?'):.2f}")
    print(f"  Properties: {json.dumps(resp.get('properties', {}), indent=4)}")
    edges_from = resp.get("edges_from", [])
    edges_to = resp.get("edges_to", [])
    print(f"  Outgoing edges ({len(edges_from)}):")
    for e in edges_from:
        print(f"    DarkLoader -[{e['relationship']}]-> {e['to']} (conf={e.get('confidence', '?')})")
    print(f"  Incoming edges ({len(edges_to)}):")
    for e in edges_to:
        print(f"    {e['from']} -[{e['relationship']}]-> DarkLoader (conf={e.get('confidence', '?')})")

    # ================================================================
    section("SUMMARY")
    # ================================================================

    stats = api("GET", "/stats")
    print(f"\n  Final graph: {stats['nodes']} nodes, {stats['edges']} edges")
    print(f"\n  Capabilities demonstrated:")
    print(f"    - Attack chain traversal: actor -> malware -> CVE/TTP")
    print(f"    - IOC search via properties (C2 domains, hashes, CVSS)")
    print(f"    - Inference: derived actor-CVE associations automatically")
    print(f"    - Attribution correction with confidence propagation")
    print(f"    - Property filters for threat hunting queries")


if __name__ == "__main__":
    main()
