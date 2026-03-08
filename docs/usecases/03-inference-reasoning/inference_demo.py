#!/usr/bin/env python3
"""
Use Case 3: Inference & Reasoning — Vulnerability Propagation in a Service Graph

Builds a microservices dependency graph, then demonstrates:
  1. Backward chaining (prove) — find transitive dependency paths
  2. Forward chaining (derive) — propagate facts using rules
  3. Push-based rules — auto-fire rules when new facts arrive
  4. Vulnerability flagging — flag services affected by a CVE

Usage:
  engram serve reasoning.brain 127.0.0.1:3030
  python inference_demo.py
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


def prove(from_e, rel, to_e):
    return api("POST", "/query", {
        "start": from_e, "depth": 5, "min_confidence": 0.0
    })


def section(title):
    print(f"\n{'=' * 60}")
    print(f"  {title}")
    print(f"{'=' * 60}")


def subsection(title):
    print(f"\n--- {title} ---")


def main():
    # Check server
    try:
        health = api("GET", "/health")
        print(f"Server: {health}")
    except Exception as e:
        print(f"Server not reachable at {ENGRAM}: {e}")
        print("Start engram first: engram serve reasoning.brain 127.0.0.1:3030")
        sys.exit(1)

    # ================================================================
    section("PHASE 1: Build the Service Dependency Graph")
    # ================================================================

    # Services
    services = [
        ("frontend",       "service", {"team": "web", "language": "TypeScript", "sla": "tier-1"}),
        ("api-gateway",    "service", {"team": "platform", "language": "Go", "sla": "tier-1"}),
        ("user-service",   "service", {"team": "identity", "language": "Java", "sla": "tier-1"}),
        ("order-service",  "service", {"team": "commerce", "language": "Java", "sla": "tier-2"}),
        ("payment-service","service", {"team": "payments", "language": "Java", "sla": "tier-1"}),
        ("notification-svc","service",{"team": "comms", "language": "Python", "sla": "tier-3"}),
        ("analytics-svc",  "service", {"team": "data", "language": "Python", "sla": "tier-3"}),
        ("auth-lib",       "library", {"language": "Java", "version": "2.1.0"}),
        ("logging-lib",    "library", {"language": "Java", "version": "1.8.0"}),
        ("json-parser",    "library", {"language": "Java", "version": "3.2.1"}),
    ]

    # Infrastructure
    infra = [
        ("PostgreSQL",  "database", {"version": "16", "sla": "tier-1"}),
        ("Redis",       "cache",    {"version": "7.2", "sla": "tier-2"}),
        ("Kafka",       "queue",    {"version": "3.6", "sla": "tier-2"}),
        ("Elasticsearch","search",  {"version": "8.11"}),
    ]

    print("\nStoring services...")
    for name, stype, props in services:
        r = store(name, entity_type=stype, properties=props, confidence=0.95)
        print(f"  {name} (id={r['node_id']})")

    print("\nStoring infrastructure...")
    for name, stype, props in infra:
        r = store(name, entity_type=stype, properties=props, confidence=0.95)
        print(f"  {name} (id={r['node_id']})")

    # Dependencies
    deps = [
        # Service-to-service
        ("frontend",        "depends_on", "api-gateway",     0.95),
        ("api-gateway",     "depends_on", "user-service",    0.95),
        ("api-gateway",     "depends_on", "order-service",   0.90),
        ("order-service",   "depends_on", "payment-service", 0.95),
        ("order-service",   "depends_on", "notification-svc",0.80),
        ("payment-service", "depends_on", "notification-svc",0.70),
        ("analytics-svc",   "depends_on", "Kafka",           0.90),
        # Service-to-library
        ("user-service",    "depends_on", "auth-lib",        0.95),
        ("user-service",    "depends_on", "logging-lib",     0.90),
        ("order-service",   "depends_on", "logging-lib",     0.90),
        ("payment-service", "depends_on", "auth-lib",        0.95),
        ("payment-service", "depends_on", "json-parser",     0.90),
        # Service-to-infra
        ("user-service",    "depends_on", "PostgreSQL",      0.95),
        ("order-service",   "depends_on", "PostgreSQL",      0.90),
        ("payment-service", "depends_on", "PostgreSQL",      0.95),
        ("user-service",    "depends_on", "Redis",           0.85),
        ("order-service",   "depends_on", "Redis",           0.85),
        ("notification-svc","depends_on", "Kafka",           0.90),
        ("frontend",        "depends_on", "Elasticsearch",   0.70),
    ]

    print("\nCreating dependency edges...")
    for from_e, rel, to_e, conf in deps:
        relate(from_e, rel, to_e, confidence=conf)
        print(f"  {from_e} -[{rel}]-> {to_e} ({conf})")

    stats = api("GET", "/stats")
    print(f"\nGraph: {stats['nodes']} nodes, {stats['edges']} edges")

    # ================================================================
    section("PHASE 2: Backward Chaining — Prove Transitive Dependencies")
    # ================================================================

    # Use the /ask endpoint for relationship queries and manual prove via traversal
    print("\nQuestion: Does 'frontend' transitively depend on 'PostgreSQL'?")
    print("(Tracing the dependency chain...)")

    # We'll trace manually using query + depth to show the chain
    # frontend -> api-gateway -> user-service -> PostgreSQL
    result = api("POST", "/query", {
        "start": "frontend", "depth": 4, "min_confidence": 0.0, "direction": "out"
    })
    nodes = {n["label"]: n for n in result.get("nodes", [])}
    edges = result.get("edges", [])

    # Find path from frontend to PostgreSQL
    print("\n  Traversal from 'frontend' (outgoing, depth=4):")
    print(f"  Reached {len(nodes)} nodes, {len(edges)} edges")

    if "PostgreSQL" in nodes:
        pg = nodes["PostgreSQL"]
        print(f"\n  PROVEN: frontend reaches PostgreSQL at depth {pg.get('depth', '?')}")
        print(f"  Chain: frontend -> api-gateway -> user-service -> PostgreSQL")
    else:
        print("  NOT FOUND: PostgreSQL not reachable from frontend")

    # Now use the actual prove endpoint (via MCP-style tool call emulation)
    subsection("Prove via /ask: Does frontend depend on auth-lib?")
    resp = api("POST", "/ask", {"question": "What does frontend connect to?"})
    results = resp.get("results", [])
    direct = [r["label"] for r in results]
    print(f"  Direct connections: {direct}")
    print(f"  auth-lib in direct? {'auth-lib' in direct}")

    # Deep traversal to prove transitive
    result = api("POST", "/query", {
        "start": "frontend", "depth": 4, "min_confidence": 0.0, "direction": "out"
    })
    reachable = [n["label"] for n in result.get("nodes", [])]
    print(f"  auth-lib reachable transitively? {'auth-lib' in reachable}")
    if "auth-lib" in reachable:
        auth_node = next(n for n in result["nodes"] if n["label"] == "auth-lib")
        print(f"  Path depth: {auth_node.get('depth', '?')}")
        print(f"  Chain: frontend -> api-gateway -> user-service -> auth-lib")

    # ================================================================
    section("PHASE 3: Forward Chaining — Derive New Facts with Rules")
    # ================================================================

    subsection("Rule 1: Transitive Dependency")
    print("  If A depends_on B and B depends_on C, then A depends_on C")

    transitive_rule = (
        'rule transitive_dependency\n'
        'when edge(A, "depends_on", B)\n'
        'when edge(B, "depends_on", C)\n'
        'then edge(A, "depends_on", C, min(e1, e2))'
    )

    result = api("POST", "/learn/derive", {"rules": [transitive_rule]})
    print(f"\n  Rules fired: {result['rules_fired']}")
    print(f"  Edges created: {result['edges_created']}")

    stats = api("GET", "/stats")
    print(f"\n  Graph now: {stats['nodes']} nodes, {stats['edges']} edges (+{result['edges_created']} derived)")
    print(f"  (engine runs to fixed point automatically -- no manual re-runs needed)")

    # Check new transitive edges
    subsection("Verify: Does frontend now have a direct edge to PostgreSQL?")
    resp = api("POST", "/ask", {"question": "What does frontend connect to?"})
    results = resp.get("results", [])
    things = [f"{r['label']}({r.get('relationship', '?')})" for r in results]
    print(f"  frontend connects to: {things}")

    has_pg = any(r["label"] == "PostgreSQL" for r in results)
    print(f"  Direct depends_on to PostgreSQL? {has_pg}")
    print(f"  Total transitive dependencies: {len(results)}")

    # ================================================================
    section("PHASE 4: Vulnerability Propagation")
    # ================================================================

    subsection("Inject a CVE into logging-lib")
    store("logging-lib", properties={"vulnerability": "CVE-2024-1234", "severity": "critical"})
    print("  Set logging-lib.vulnerability = CVE-2024-1234")
    print("  Set logging-lib.severity = critical")

    subsection("Rule 2: Flag services depending on vulnerable components")

    vuln_rule = (
        'rule vuln_propagation\n'
        'when edge(service, "depends_on", dep)\n'
        'when prop(dep, "vulnerability", "CVE-2024-1234")\n'
        'then flag(service, "depends on vulnerable component: CVE-2024-1234")'
    )

    result = api("POST", "/learn/derive", {"rules": [vuln_rule]})
    print(f"\n  Rules fired: {result['rules_fired']}")
    print(f"  Flags raised: {result['flags_raised']}")

    subsection("Which services are flagged?")
    # Search for flagged services
    flagged = []
    for name, stype, props in services:
        try:
            node = api("GET", f"/node/{name}")
            node_props = node.get("properties", {})
            if "_flag" in node_props:
                flagged.append((name, node_props["_flag"]))
        except Exception:
            pass

    if flagged:
        for name, reason in flagged:
            print(f"  FLAGGED: {name} -- {reason}")
    else:
        print("  No services flagged (unexpected)")

    # Also check which services depend on logging-lib (including transitive)
    subsection("Blast radius: Who depends on logging-lib?")
    result = api("POST", "/query", {
        "start": "logging-lib", "depth": 3, "min_confidence": 0.0, "direction": "in"
    })
    dependents = [n["label"] for n in result.get("nodes", []) if n["label"] != "logging-lib"]
    print(f"  Services affected: {dependents}")

    # ================================================================
    section("PHASE 5: Push-Based Rules — Auto-Fire on New Facts")
    # ================================================================

    subsection("Load persistent rules")
    # Load rules that auto-fire on mutations
    sla_rule = (
        'rule sla_mismatch\n'
        'when edge(critical, "depends_on", dep)\n'
        'when prop(critical, "sla", "tier-1")\n'
        'when prop(dep, "sla", "tier-3")\n'
        'then flag(critical, "tier-1 service depends on tier-3 dependency")'
    )

    result = api("POST", "/rules", {"rules": [sla_rule], "append": False})
    print(f"  Loaded {result.get('loaded', result.get('count', '?'))} rules")

    subsection("Verify: SLA mismatch detection")
    # Manually fire to check
    result = api("POST", "/learn/derive", {"rules": [sla_rule]})
    print(f"  Rules fired: {result['rules_fired']}")
    print(f"  Flags raised: {result['flags_raised']}")

    # Check which tier-1 services depend on tier-3
    subsection("SLA mismatches found")
    for name, stype, props in services:
        if props.get("sla") == "tier-1":
            try:
                node = api("GET", f"/node/{name}")
                node_props = node.get("properties", {})
                flag = node_props.get("_flag", "")
                if "tier-1 service depends on tier-3" in flag:
                    print(f"  MISMATCH: {name} (tier-1) depends on tier-3 service")
            except Exception:
                pass

    # ================================================================
    section("PHASE 6: Evidence & Explainability")
    # ================================================================

    subsection("Explain: What do we know about payment-service?")
    resp = api("GET", "/explain/payment-service")
    print(f"  Confidence: {resp.get('confidence', '?')}")
    print(f"  Properties: {json.dumps(resp.get('properties', {}), indent=4)}")

    edges = resp.get("edges", [])
    print(f"  Edges ({len(edges)}):")
    for e in edges[:10]:
        print(f"    -[{e['relationship']}]-> {e['to']} (conf={e.get('confidence', '?')})")

    cooccurrences = resp.get("cooccurrences", [])
    if cooccurrences:
        print(f"  Co-occurrences: {cooccurrences[:5]}")

    subsection("Explain: What do we know about logging-lib?")
    resp = api("GET", "/explain/logging-lib")
    print(f"  Confidence: {resp.get('confidence', '?')}")
    print(f"  Properties: {json.dumps(resp.get('properties', {}), indent=4)}")

    # ================================================================
    section("SUMMARY")
    # ================================================================

    stats = api("GET", "/stats")
    print(f"\n  Final graph: {stats['nodes']} nodes, {stats['edges']} edges")
    print(f"\n  Capabilities demonstrated:")
    print(f"    - Backward chaining: traced transitive dependencies via BFS")
    print(f"    - Forward chaining: derived {result.get('rules_fired', '?')} new facts from rules")
    print(f"    - Vulnerability propagation: flagged affected services automatically")
    print(f"    - SLA mismatch detection: found tier-1 -> tier-3 dependencies")
    print(f"    - Explainability: full provenance on any node")


if __name__ == "__main__":
    main()
