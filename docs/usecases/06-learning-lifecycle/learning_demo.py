#!/usr/bin/env python3
"""
Use Case 6: How Engram Learns, Forgets, and Self-Corrects

Demonstrates the full confidence lifecycle: initial storage, access
reinforcement, confirmation reinforcement, correction with distrust
propagation, decay, memory tiers, and inference rules.

Usage:
  engram serve learning.brain 127.0.0.1:3030
  python learning_demo.py
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
    elif method == "DELETE":
        r = requests.delete(url, timeout=10)
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
        print("Start engram first: engram serve learning.brain 127.0.0.1:3030")
        sys.exit(1)

    # ================================================================
    section("PHASE 1: Initial Storage with Different Confidence Levels")
    # ================================================================

    subsection("Store facts with varying initial confidence")
    r1 = store("Jupiter", "planet", {
        "moon_count": "79", "system": "Solar System"
    }, confidence=0.30, source="llm-assistant")
    print(f"  Jupiter (LLM claim, conf=0.30): id={r1['node_id']}")

    r2 = store("Sun", "star", {
        "spectral_class": "G2V", "age_billion_years": "4.6"
    }, confidence=0.95, source="astronomy-textbook")
    print(f"  Sun (textbook, conf=0.95): id={r2['node_id']}")

    r3 = store("Mars", "planet", {
        "moon_count": "2", "system": "Solar System"
    }, confidence=0.90, source="nasa-database")
    print(f"  Mars (NASA, conf=0.90): id={r3['node_id']}")

    relate("Jupiter", "orbits", "Sun", confidence=0.99)
    relate("Mars", "orbits", "Sun", confidence=0.99)
    print("  Jupiter -[orbits]-> Sun (0.99)")
    print("  Mars -[orbits]-> Sun (0.99)")

    stats = api("GET", "/stats")
    print(f"\nGraph: {stats['nodes']} nodes, {stats['edges']} edges")

    # ================================================================
    section("PHASE 2: Access Reinforcement (+0.02 per access)")
    # ================================================================

    subsection("Simulate 5 accesses to Jupiter (no source = access boost)")
    for i in range(5):
        r = api("POST", "/learn/reinforce", {"entity": "Jupiter"})
        print(f"  Access {i+1}: confidence = {r['new_confidence']:.2f}")

    node = api("GET", "/node/Jupiter")
    print(f"\n  Jupiter after 5 accesses: {node['confidence']:.2f}")
    print(f"  Expected: 0.30 + 5*0.02 = 0.40")

    # ================================================================
    section("PHASE 3: Confirmation Reinforcement (+0.10 with source)")
    # ================================================================

    subsection("Independent source confirms Jupiter data")
    r = api("POST", "/learn/reinforce", {
        "entity": "Jupiter", "source": "nasa-jpl-database"
    })
    print(f"  After NASA confirmation: confidence = {r['new_confidence']:.2f}")

    r = api("POST", "/learn/reinforce", {
        "entity": "Jupiter", "source": "eso-observatory"
    })
    print(f"  After ESO confirmation:  confidence = {r['new_confidence']:.2f}")

    node = api("GET", "/node/Jupiter")
    print(f"\n  Jupiter after 5 accesses + 2 confirmations: {node['confidence']:.2f}")
    print(f"  Expected: 0.40 + 2*0.10 = 0.60")

    # ================================================================
    section("PHASE 4: Update Properties")
    # ================================================================

    subsection("Correct moon count (79 -> 95)")
    r = store("Jupiter", properties={"moon_count": "95"})
    print(f"  Updated Jupiter properties")

    node = api("GET", "/node/Jupiter")
    print(f"  Jupiter confidence unchanged: {node['confidence']:.2f}")
    props = node.get("properties", {})
    print(f"  moon_count property: {props.get('moon_count', '?')}")
    print(f"  (store updates properties but confidence is managed by learn/*)")

    # ================================================================
    section("PHASE 5: Wrong Fact and Correction")
    # ================================================================

    subsection("Store a wrong claim with outgoing edge to Jupiter")
    store("Jupiter-has-solid-surface", "claim", {
        "status": "unverified",
    }, confidence=0.70)
    # Edge FROM the claim TO Jupiter so correction propagates distrust to Jupiter
    relate("Jupiter-has-solid-surface", "about", "Jupiter", confidence=0.70)
    print("  Stored: Jupiter-has-solid-surface (conf=0.70)")
    print("  Jupiter-has-solid-surface -[about]-> Jupiter (0.70)")

    subsection("Before correction")
    node = api("GET", "/node/Jupiter")
    print(f"  Jupiter confidence: {node['confidence']:.2f}")

    subsection("Correct the wrong claim")
    result = api("POST", "/learn/correct", {
        "entity": "Jupiter-has-solid-surface",
        "reason": "Jupiter is a gas giant with no defined solid surface"
    })
    print(f"  Correction result: {result}")

    node = api("GET", "/node/Jupiter-has-solid-surface")
    print(f"  Claim confidence after correction: {node['confidence']:.2f}")

    subsection("Check distrust propagation to Jupiter")
    node = api("GET", "/node/Jupiter")
    print(f"  Jupiter confidence after neighbor correction: {node['confidence']:.2f}")
    print(f"  (distrust propagates through outgoing edges of corrected node)")

    # ================================================================
    section("PHASE 6: Decay")
    # ================================================================

    subsection("Apply time-based decay")
    result = api("POST", "/learn/decay")
    print(f"  Nodes decayed: {result.get('nodes_decayed', '?')}")

    subsection("Confidence after decay")
    for label in ["Jupiter", "Sun", "Mars", "Jupiter-has-solid-surface"]:
        try:
            node = api("GET", f"/node/{label}")
            print(f"  {label}: {node['confidence']:.3f}")
        except Exception:
            print(f"  {label}: not found")

    # ================================================================
    section("PHASE 7: Inference Rules")
    # ================================================================

    subsection("Rule: Flag unverified claims for review")
    rule_flag = (
        'rule flag_unverified\n'
        'when prop(node, "status", "unverified")\n'
        'then flag(node, "unverified claim -- needs review")'
    )
    result = api("POST", "/learn/derive", {"rules": [rule_flag]})
    print(f"  Rules fired: {result['rules_fired']}, flags raised: {result['flags_raised']}")

    subsection("Check flags")
    node = api("GET", "/node/Jupiter-has-solid-surface")
    flag = node.get("properties", {}).get("_flag")
    if flag:
        print(f"  FLAGGED: Jupiter-has-solid-surface -- {flag}")
    else:
        print(f"  Jupiter-has-solid-surface: no flag")

    # ================================================================
    section("PHASE 8: Recovery via Reinforcement")
    # ================================================================

    subsection("Jupiter was damaged by distrust propagation -- recover it")
    node = api("GET", "/node/Jupiter")
    print(f"  Jupiter before recovery: {node['confidence']:.2f}")

    for src in ["verified-nasa-data", "eso-2026", "textbook-astronomy"]:
        api("POST", "/learn/reinforce", {"entity": "Jupiter", "source": src})
    node = api("GET", "/node/Jupiter")
    print(f"  Jupiter after 3 confirmations: {node['confidence']:.2f}")

    for i in range(10):
        api("POST", "/learn/reinforce", {"entity": "Jupiter"})
    node = api("GET", "/node/Jupiter")
    print(f"  Jupiter after 10 more accesses: {node['confidence']:.2f}")

    subsection("Recover Sun (also hit by cascading distrust)")
    node = api("GET", "/node/Sun")
    print(f"  Sun before recovery: {node['confidence']:.2f}")

    for src in ["textbook-A", "textbook-B", "nasa"]:
        api("POST", "/learn/reinforce", {"entity": "Sun", "source": src})
    for i in range(10):
        api("POST", "/learn/reinforce", {"entity": "Sun"})
    node = api("GET", "/node/Sun")
    print(f"  Sun after recovery: {node['confidence']:.2f}")

    # ================================================================
    section("PHASE 9: Explainability")
    # ================================================================

    subsection("Explain: Jupiter")
    resp = api("GET", "/explain/Jupiter")
    print(f"  Confidence: {resp.get('confidence', '?'):.3f}")
    print(f"  Properties: {json.dumps(resp.get('properties', {}), indent=4)}")
    edges_from = resp.get("edges_from", [])
    edges_to = resp.get("edges_to", [])
    print(f"  Outgoing edges ({len(edges_from)}):")
    for e in edges_from:
        print(f"    Jupiter -[{e['relationship']}]-> {e['to']} (conf={e.get('confidence', '?')})")
    print(f"  Incoming edges ({len(edges_to)}):")
    for e in edges_to:
        print(f"    {e['from']} -[{e['relationship']}]-> Jupiter (conf={e.get('confidence', '?')})")

    # ================================================================
    section("SUMMARY")
    # ================================================================

    stats = api("GET", "/stats")
    print(f"\n  Final graph: {stats['nodes']} nodes, {stats['edges']} edges")

    print(f"\n  Confidence lifecycle demonstrated:")
    print(f"    - Initial storage: LLM=0.30, API=0.90, textbook=0.95")
    print(f"    - Access reinforcement: +0.02 per read")
    print(f"    - Confirmation reinforcement: +0.10 per independent source")
    print(f"    - Correction: zeroes confidence, propagates distrust to neighbors")
    print(f"    - Decay: 0.999/day, archival below 0.10")
    print(f"    - Inference: flag claims for human review")
    print(f"    - Explainability: full provenance via /explain")


if __name__ == "__main__":
    main()
