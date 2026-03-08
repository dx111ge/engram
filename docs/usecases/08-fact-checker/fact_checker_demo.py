#!/usr/bin/env python3
"""
Use Case 8: Fact Checker -- Multi-Source Claim Verification

Builds a fact-checking knowledge base that rates claims by source reliability,
cross-references evidence, propagates trust/distrust, and uses inference rules
for automated credibility assessment.

Usage:
  engram serve factcheck.brain 127.0.0.1:3030
  python fact_checker_demo.py
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
        print("Start engram first: engram serve factcheck.brain 127.0.0.1:3030")
        sys.exit(1)

    # ================================================================
    section("PHASE 1: Register Sources with Reliability Tiers")
    # ================================================================

    sources = [
        # Tier 1: peer-reviewed, official
        ("Source:WHO", "source", {"reliability_tier": "1", "type": "intl_organization"}, 0.95),
        ("Source:Nature", "source", {"reliability_tier": "1", "type": "peer_reviewed"}, 0.95),
        ("Source:Cochrane", "source", {"reliability_tier": "1", "type": "meta_analysis"}, 0.95),
        # Tier 2: major news
        ("Source:Reuters", "source", {"reliability_tier": "2", "type": "news_agency"}, 0.85),
        ("Source:BBC", "source", {"reliability_tier": "2", "type": "news_outlet"}, 0.82),
        # Tier 3: blogs, social
        ("Source:HealthBlog", "source", {"reliability_tier": "3", "type": "blog"}, 0.40),
        ("Source:SocialPost", "source", {"reliability_tier": "3", "type": "social_media"}, 0.30),
    ]

    for name, stype, props, conf in sources:
        store(name, stype, props, conf)
        print(f"  {name} (tier={props['reliability_tier']}, conf={conf})")

    stats = api("GET", "/stats")
    print(f"\nSources registered: {stats['nodes']} nodes")

    # ================================================================
    section("PHASE 2: Store Claims with Source Attribution")
    # ================================================================

    subsection("Claim 1: Well-established fact (Earth's age)")
    store("Claim:EarthAge", "claim", {
        "text": "Earth is approximately 4.54 billion years old",
        "category": "science",
        "status": "verified",
    }, confidence=0.90)
    relate("Claim:EarthAge", "sourced_from", "Source:Nature", 0.95)
    relate("Claim:EarthAge", "corroborated_by", "Source:WHO", 0.90)
    api("POST", "/learn/reinforce", {"entity": "Claim:EarthAge", "source": "Source:Nature"})
    node = api("GET", "/node/Claim:EarthAge")
    print(f"  Claim:EarthAge confidence: {node['confidence']:.2f}")

    subsection("Claim 2: Disputed health claim (Vitamin C)")
    store("Claim:VitaminCCuresCold", "claim", {
        "text": "Vitamin C cures the common cold",
        "category": "health",
        "status": "disputed",
    }, confidence=0.50)
    relate("Claim:VitaminCCuresCold", "sourced_from", "Source:HealthBlog", 0.40)
    print(f"  Claim:VitaminCCuresCold: sourced from low-reliability blog")

    # Store contradicting evidence
    store("Evidence:CochraneMeta", "evidence", {
        "text": "Meta-analysis: Vitamin C does not prevent or cure colds",
        "study_type": "meta-analysis",
        "sample_size": "11306",
    }, confidence=0.92)
    relate("Evidence:CochraneMeta", "published_in", "Source:Cochrane", 0.95)
    relate("Evidence:CochraneMeta", "contradicts", "Claim:VitaminCCuresCold", 0.90)
    print(f"  Evidence:CochraneMeta contradicts the claim (meta-analysis, n=11306)")

    subsection("Claim 3: Gaining credibility (Microplastics)")
    store("Claim:MicroplasticsInBlood", "claim", {
        "text": "Microplastics have been found in human blood",
        "category": "health",
        "status": "emerging",
    }, confidence=0.60)
    relate("Claim:MicroplasticsInBlood", "sourced_from", "Source:Reuters", 0.85)
    relate("Claim:MicroplasticsInBlood", "corroborated_by", "Source:BBC", 0.82)
    api("POST", "/learn/reinforce", {
        "entity": "Claim:MicroplasticsInBlood", "source": "Source:BBC"
    })

    # Peer-reviewed study supports the claim
    store("Evidence:VUAmsterdam", "evidence", {
        "text": "Plasticenta study: microplastics in 17/22 blood samples",
        "study_type": "peer-reviewed",
        "year": "2022",
    }, confidence=0.90)
    relate("Evidence:VUAmsterdam", "supports", "Claim:MicroplasticsInBlood", 0.90)
    api("POST", "/learn/reinforce", {
        "entity": "Claim:MicroplasticsInBlood", "source": "Source:Nature"
    })
    node = api("GET", "/node/Claim:MicroplasticsInBlood")
    print(f"  Claim:MicroplasticsInBlood: {node['confidence']:.2f} (3 sources + study)")

    subsection("Claim 4: Fabricated claim from unreliable source")
    store("Claim:5GCausesCovid", "claim", {
        "text": "5G towers caused the COVID-19 pandemic",
        "category": "health",
        "status": "fabricated",
    }, confidence=0.30)
    relate("Claim:5GCausesCovid", "sourced_from", "Source:SocialPost", 0.30)
    print(f"  Claim:5GCausesCovid: low-confidence from social media")

    stats = api("GET", "/stats")
    print(f"\nGraph: {stats['nodes']} nodes, {stats['edges']} edges")

    # ================================================================
    section("PHASE 3: Credibility Assessment")
    # ================================================================

    subsection("Current claim credibility scores")
    claims = ["Claim:EarthAge", "Claim:VitaminCCuresCold",
              "Claim:MicroplasticsInBlood", "Claim:5GCausesCovid"]
    for claim in claims:
        node = api("GET", f"/node/{claim}")
        props = node.get("properties", {})
        print(f"  {claim}: {node['confidence']:.2f} ({props.get('status', '?')})")

    subsection("Search: health claims")
    result = api("POST", "/search", {"query": "prop:category=health", "limit": 10})
    hits = [h["label"] for h in result.get("results", [])]
    print(f"  Health claims: {hits}")

    subsection("Search: tier-1 sources")
    result = api("POST", "/search", {"query": "prop:reliability_tier=1", "limit": 10})
    hits = [h["label"] for h in result.get("results", [])]
    print(f"  Tier-1 sources: {hits}")

    # ================================================================
    section("PHASE 4: Evidence Chain Traversal")
    # ================================================================

    subsection("Traverse from Claim:MicroplasticsInBlood")
    result = api("POST", "/query", {
        "start": "Claim:MicroplasticsInBlood",
        "depth": 2,
        "min_confidence": 0.0,
    })
    nodes = result.get("nodes", [])
    print(f"  Evidence chain: {len(nodes)} nodes")
    for n in sorted(nodes, key=lambda x: (x.get("depth", 0), x["label"])):
        print(f"    [depth={n.get('depth', '?')}] {n['label']} (conf={n['confidence']:.2f})")

    subsection("What contradicts Claim:VitaminCCuresCold?")
    resp = api("POST", "/ask", {"question": "What connects to Claim:VitaminCCuresCold?"})
    results = resp.get("results", [])
    for r in results:
        print(f"  {r['label']} -[{r.get('relationship', '?')}]-> Claim:VitaminCCuresCold")

    # ================================================================
    section("PHASE 5: Debunk via Correction")
    # ================================================================

    subsection("Debunk Claim:VitaminCCuresCold")
    result = api("POST", "/learn/correct", {
        "entity": "Claim:VitaminCCuresCold",
        "reason": "Debunked by Cochrane meta-analysis (11,306 participants)"
    })
    print(f"  Correction result: {result}")

    node = api("GET", "/node/Claim:VitaminCCuresCold")
    print(f"  Claim confidence after debunking: {node['confidence']:.2f}")

    subsection("Debunk Claim:5GCausesCovid")
    result = api("POST", "/learn/correct", {
        "entity": "Claim:5GCausesCovid",
        "reason": "No scientific evidence; physics makes this impossible"
    })
    print(f"  Correction result: {result}")

    subsection("Discredit Source:SocialPost (systematically unreliable)")
    result = api("POST", "/learn/correct", {
        "entity": "Source:SocialPost",
        "reason": "Source publishes fabricated health claims"
    })
    print(f"  Source discredited: {result}")

    node = api("GET", "/node/Source:SocialPost")
    print(f"  Source:SocialPost confidence: {node['confidence']:.2f}")

    # ================================================================
    section("PHASE 6: Inference Rules")
    # ================================================================

    subsection("Rule: Flag claims contradicted by meta-analysis evidence")
    rule1 = (
        'rule contradicted_by_evidence\n'
        'when edge(evidence, "contradicts", claim)\n'
        'when prop(evidence, "study_type", "meta-analysis")\n'
        'then flag(claim, "contradicted by meta-analysis")'
    )

    subsection("Rule: Flag claims from discredited sources")
    rule2 = (
        'rule discredited_source\n'
        'when edge(claim, "sourced_from", source)\n'
        'when prop(source, "reliability_tier", "3")\n'
        'then flag(claim, "sourced from low-reliability tier")'
    )

    result = api("POST", "/learn/derive", {"rules": [rule1, rule2]})
    print(f"  Rules fired: {result['rules_fired']}")
    print(f"  Flags raised: {result['flags_raised']}")

    subsection("Check flags on claims")
    for claim in claims:
        node = api("GET", f"/node/{claim}")
        flag = node.get("properties", {}).get("_flag")
        if flag:
            print(f"  FLAGGED: {claim} -- {flag}")

    # ================================================================
    section("PHASE 7: Final Credibility Report")
    # ================================================================

    subsection("Final claim credibility scores")
    for claim in claims:
        node = api("GET", f"/node/{claim}")
        props = node.get("properties", {})
        flag = props.get("_flag", "none")
        print(f"  {claim}")
        print(f"    Confidence: {node['confidence']:.2f}")
        print(f"    Status: {props.get('status', '?')}")
        print(f"    Flag: {flag}")

    subsection("Explainability: Claim:MicroplasticsInBlood")
    resp = api("GET", "/explain/Claim:MicroplasticsInBlood")
    print(f"  Confidence: {resp.get('confidence', '?'):.2f}")
    edges_from = resp.get("edges_from", [])
    edges_to = resp.get("edges_to", [])
    print(f"  Evidence (outgoing edges: {len(edges_from)}):")
    for e in edges_from:
        print(f"    -[{e['relationship']}]-> {e['to']} (conf={e.get('confidence', '?')})")
    print(f"  Supporting (incoming edges: {len(edges_to)}):")
    for e in edges_to:
        print(f"    {e['from']} -[{e['relationship']}]-> (conf={e.get('confidence', '?')})")

    # ================================================================
    section("SUMMARY")
    # ================================================================

    stats = api("GET", "/stats")
    print(f"\n  Final graph: {stats['nodes']} nodes, {stats['edges']} edges")
    print(f"\n  Fact-checking capabilities demonstrated:")
    print(f"    - Source reliability tiers (tier-1: 0.95, tier-2: 0.85, tier-3: 0.30-0.40)")
    print(f"    - Claim corroboration via reinforcement from independent sources")
    print(f"    - Contradiction handling: correction zeroes confidence")
    print(f"    - Source discrediting cascades distrust to linked claims")
    print(f"    - Evidence chain traversal: claim -> source -> corroboration")
    print(f"    - Inference rules: auto-flag contradicted claims and low-tier sources")


if __name__ == "__main__":
    main()
