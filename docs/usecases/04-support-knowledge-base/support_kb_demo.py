#!/usr/bin/env python3
"""
Use Case 4: Building a Support Knowledge Base

Builds an IT support knowledge base for a fictional e-commerce platform.
Demonstrates the full lifecycle: error patterns, root causes, solutions,
confidence reinforcement, correction, decay, and inference rules.

Usage:
  engram serve support.brain 127.0.0.1:3030
  python support_kb_demo.py
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
        print("Start engram first: engram serve support.brain 127.0.0.1:3030")
        sys.exit(1)

    # ================================================================
    section("PHASE 1: Infrastructure & Services")
    # ================================================================

    servers = [
        ("web-server-01", "server", {"env": "production", "region": "us-east-1"}),
        ("web-server-02", "server", {"env": "production", "region": "us-east-1"}),
        ("db-primary-01", "server", {"env": "production", "role": "database"}),
        ("db-replica-01", "server", {"env": "production", "role": "database-replica"}),
        ("cache-01",      "server", {"env": "production", "role": "cache"}),
        ("queue-01",      "server", {"env": "production", "role": "queue"}),
    ]

    services = [
        ("checkout-api",  "service", {"owner": "payments-team", "tier": "1"}),
        ("user-api",      "service", {"owner": "identity-team", "tier": "1"}),
        ("search-api",    "service", {"owner": "catalog-team", "tier": "2"}),
        ("postgresql",    "service", {"version": "16", "port": "5432"}),
        ("redis",         "service", {"port": "6379", "maxmemory": "4gb"}),
        ("rabbitmq",      "service", {"port": "5672"}),
    ]

    print("\nStoring servers...")
    for name, stype, props in servers:
        r = store(name, entity_type=stype, properties=props, confidence=0.95)
        print(f"  {name} (id={r['node_id']})")

    print("\nStoring services...")
    for name, stype, props in services:
        r = store(name, entity_type=stype, properties=props, confidence=0.95)
        print(f"  {name} (id={r['node_id']})")

    # Server-to-service relationships
    runs_on = [
        ("web-server-01", "runs", "checkout-api"),
        ("web-server-02", "runs", "user-api"),
        ("db-primary-01", "runs", "postgresql"),
        ("db-replica-01", "runs", "postgresql"),
        ("cache-01",      "runs", "redis"),
        ("queue-01",      "runs", "rabbitmq"),
    ]

    # Service dependencies
    deps = [
        ("checkout-api", "depends_on", "postgresql", 0.95),
        ("checkout-api", "depends_on", "redis",      0.90),
        ("checkout-api", "depends_on", "rabbitmq",   0.80),
        ("user-api",     "depends_on", "postgresql", 0.95),
        ("user-api",     "depends_on", "redis",      0.85),
        ("search-api",   "depends_on", "postgresql", 0.80),
    ]

    print("\nCreating relationships...")
    for from_e, rel, to_e in runs_on:
        relate(from_e, rel, to_e, confidence=0.95)
        print(f"  {from_e} -[{rel}]-> {to_e}")

    for from_e, rel, to_e, conf in deps:
        relate(from_e, rel, to_e, confidence=conf)
        print(f"  {from_e} -[{rel}]-> {to_e} ({conf})")

    stats = api("GET", "/stats")
    print(f"\nGraph: {stats['nodes']} nodes, {stats['edges']} edges")

    # ================================================================
    section("PHASE 2: Error Patterns, Root Causes, Solutions")
    # ================================================================

    # Error pattern 1: Connection pool exhaustion
    subsection("Error Pattern 1: Connection Pool Exhaustion")
    store("ERR:connection_pool_exhausted", "error_pattern", {
        "severity": "critical",
        "symptom": "FATAL: remaining connection slots are reserved",
        "status": "open",
    }, confidence=0.90)
    store("CAUSE:pg_max_connections", "root_cause", {
        "component": "postgresql", "category": "resource_exhaustion",
    }, confidence=0.75)
    store("FIX:increase_max_connections", "solution", {
        "action": "ALTER SYSTEM SET max_connections = 500; SELECT pg_reload_conf();",
        "risk": "low", "verified": "false",
    }, confidence=0.60)
    store("FIX:add_pgbouncer", "solution", {
        "action": "Deploy PgBouncer connection pooler in front of PostgreSQL",
        "risk": "medium", "verified": "false",
    }, confidence=0.55)

    relate("ERR:connection_pool_exhausted", "caused_by", "CAUSE:pg_max_connections", 0.75)
    relate("CAUSE:pg_max_connections", "resolved_by", "FIX:increase_max_connections", 0.60)
    relate("CAUSE:pg_max_connections", "resolved_by", "FIX:add_pgbouncer", 0.55)
    relate("ERR:connection_pool_exhausted", "affects", "postgresql", 0.90)
    print("  Stored: ERR -> CAUSE -> 2 FIXes, affects postgresql")

    # Error pattern 2: Redis OOM
    subsection("Error Pattern 2: Redis Out of Memory")
    store("ERR:redis_oom", "error_pattern", {
        "severity": "high",
        "symptom": "OOM command not allowed when used memory > maxmemory",
        "status": "open",
    }, confidence=0.90)
    store("CAUSE:redis_memory_full", "root_cause", {
        "component": "redis", "category": "resource_exhaustion",
    }, confidence=0.80)
    store("FIX:redis_eviction_policy", "solution", {
        "action": "CONFIG SET maxmemory-policy allkeys-lru",
        "risk": "low", "verified": "true",
    }, confidence=0.85)
    store("FIX:redis_scale_memory", "solution", {
        "action": "Increase maxmemory to 8gb and restart",
        "risk": "medium", "verified": "false",
    }, confidence=0.50)

    relate("ERR:redis_oom", "caused_by", "CAUSE:redis_memory_full", 0.80)
    relate("CAUSE:redis_memory_full", "resolved_by", "FIX:redis_eviction_policy", 0.85)
    relate("CAUSE:redis_memory_full", "resolved_by", "FIX:redis_scale_memory", 0.50)
    relate("ERR:redis_oom", "affects", "redis", 0.90)
    print("  Stored: ERR -> CAUSE -> 2 FIXes, affects redis")

    # Error pattern 3: Slow queries
    subsection("Error Pattern 3: Slow Query Performance")
    store("ERR:slow_queries", "error_pattern", {
        "severity": "medium",
        "symptom": "Query execution time > 5s on user_sessions table",
        "status": "investigating",
    }, confidence=0.85)
    store("CAUSE:missing_index", "root_cause", {
        "component": "postgresql", "category": "performance",
    }, confidence=0.70)
    store("FIX:add_session_index", "solution", {
        "action": "CREATE INDEX CONCURRENTLY idx_sessions_user ON user_sessions(user_id, created_at);",
        "risk": "low", "verified": "false",
    }, confidence=0.65)

    relate("ERR:slow_queries", "caused_by", "CAUSE:missing_index", 0.70)
    relate("CAUSE:missing_index", "resolved_by", "FIX:add_session_index", 0.65)
    relate("ERR:slow_queries", "affects", "postgresql", 0.80)
    relate("ERR:slow_queries", "affects", "checkout-api", 0.70)
    print("  Stored: ERR -> CAUSE -> FIX, affects postgresql + checkout-api")

    stats = api("GET", "/stats")
    print(f"\nGraph: {stats['nodes']} nodes, {stats['edges']} edges")

    # ================================================================
    section("PHASE 3: Incident Response — Search & Traverse")
    # ================================================================

    subsection("On-call sees 'connection slots' error, searches for it")
    result = api("POST", "/search", {"query": "connection pool exhausted", "limit": 5})
    hits = result.get("results", [])
    print(f"  Found: {[h['label'] for h in hits]}")

    subsection("Traverse from error to find solutions")
    result = api("POST", "/query", {
        "start": "ERR:connection_pool_exhausted", "depth": 2, "min_confidence": 0.0
    })
    nodes = result.get("nodes", [])
    print(f"  Traversal: {len(nodes)} nodes reachable")
    for n in nodes:
        label = n["label"]
        conf = n["confidence"]
        depth = n.get("depth", "?")
        prefix = "  " * (depth + 1) if isinstance(depth, int) else "  "
        print(f"  {prefix}{label} (conf={conf:.2f}, depth={depth})")

    subsection("Search critical open issues")
    result = api("POST", "/search", {"query": "prop:severity=critical", "limit": 10})
    hits = result.get("results", [])
    print(f"  Critical issues: {[h['label'] for h in hits]}")

    subsection("Search all open issues")
    result = api("POST", "/search", {"query": "prop:status=open", "limit": 10})
    hits = result.get("results", [])
    print(f"  Open issues: {[h['label'] for h in hits]}")

    # ================================================================
    section("PHASE 4: Learning — Reinforce, Correct, Decay")
    # ================================================================

    subsection("Fix worked! Reinforce the solution")
    # Confirmation boost (+0.10 with source)
    r1 = api("POST", "/learn/reinforce", {
        "entity": "FIX:increase_max_connections", "source": "on-call-alice"
    })
    print(f"  After Alice confirms: confidence = {r1['new_confidence']:.2f}")

    r2 = api("POST", "/learn/reinforce", {
        "entity": "FIX:increase_max_connections", "source": "on-call-bob"
    })
    print(f"  After Bob confirms:   confidence = {r2['new_confidence']:.2f}")

    # Access boost (+0.02 without source)
    r3 = api("POST", "/learn/reinforce", {"entity": "FIX:increase_max_connections"})
    print(f"  After access boost:   confidence = {r3['new_confidence']:.2f}")

    subsection("Wrong diagnosis — correct it")
    store("CAUSE:network_partition", "root_cause", confidence=0.40)
    relate("ERR:connection_pool_exhausted", "caused_by", "CAUSE:network_partition", 0.40)
    print("  Stored wrong diagnosis: CAUSE:network_partition (conf=0.40)")

    result = api("POST", "/learn/correct", {
        "entity": "CAUSE:network_partition",
        "reason": "postmortem confirmed resource exhaustion, not network"
    })
    print(f"  Corrected: {result}")

    # Check confidence after correction
    node = api("GET", "/node/CAUSE:network_partition")
    print(f"  CAUSE:network_partition confidence now: {node['confidence']:.2f}")

    subsection("Simulate time-based decay")
    result = api("POST", "/learn/decay")
    print(f"  Nodes decayed: {result.get('nodes_decayed', '?')}")

    # Show current confidence levels for key nodes
    subsection("Current confidence scores")
    for label in ["FIX:increase_max_connections", "FIX:add_pgbouncer",
                   "FIX:redis_eviction_policy", "CAUSE:network_partition",
                   "ERR:connection_pool_exhausted"]:
        try:
            node = api("GET", f"/node/{label}")
            print(f"  {label}: {node['confidence']:.3f}")
        except Exception:
            print(f"  {label}: not found")

    # ================================================================
    section("PHASE 5: Inference — Propagate Impact")
    # ================================================================

    subsection("Rule: If server runs service with error, server is affected")
    rule = (
        'rule server_affected\n'
        'when edge(server, "runs", service)\n'
        'when edge(err, "affects", service)\n'
        'then edge(err, "affects", server, min(e1, e2))'
    )
    result = api("POST", "/learn/derive", {"rules": [rule]})
    print(f"  Rules fired: {result['rules_fired']}")
    print(f"  Edges created: {result['edges_created']}")

    subsection("Rule: If service depends on affected service, it is also affected")
    dep_rule = (
        'rule dependency_impact\n'
        'when edge(svc, "depends_on", dep)\n'
        'when edge(err, "affects", dep)\n'
        'then edge(err, "affects", svc, min(e1, e2))'
    )
    result = api("POST", "/learn/derive", {"rules": [dep_rule]})
    print(f"  Rules fired: {result['rules_fired']}")
    print(f"  Edges created: {result['edges_created']}")

    subsection("Blast radius: What does ERR:connection_pool_exhausted affect?")
    resp = api("POST", "/ask", {"question": "What does ERR:connection_pool_exhausted connect to?"})
    results = resp.get("results", [])
    affected = [(r["label"], r.get("relationship", "?")) for r in results if r.get("relationship") == "affects"]
    print(f"  Affected entities: {affected}")

    subsection("Blast radius: What does ERR:redis_oom affect?")
    resp = api("POST", "/ask", {"question": "What does ERR:redis_oom connect to?"})
    results = resp.get("results", [])
    affected = [(r["label"], r.get("relationship", "?")) for r in results if r.get("relationship") == "affects"]
    print(f"  Affected entities: {affected}")

    # ================================================================
    section("PHASE 6: Explainability")
    # ================================================================

    subsection("Explain: FIX:increase_max_connections")
    resp = api("GET", "/explain/FIX:increase_max_connections")
    print(f"  Confidence: {resp.get('confidence', '?'):.3f}")
    print(f"  Properties: {json.dumps(resp.get('properties', {}), indent=4)}")
    edges = resp.get("edges", [])
    if edges:
        print(f"  Edges ({len(edges)}):")
        for e in edges:
            print(f"    -[{e['relationship']}]-> {e['to']}")

    subsection("Explain: ERR:connection_pool_exhausted")
    resp = api("GET", "/explain/ERR:connection_pool_exhausted")
    print(f"  Confidence: {resp.get('confidence', '?'):.3f}")
    edges = resp.get("edges", [])
    print(f"  Edges ({len(edges)}):")
    for e in edges:
        print(f"    -[{e['relationship']}]-> {e['to']} (conf={e.get('confidence', '?')})")

    # ================================================================
    section("SUMMARY")
    # ================================================================

    stats = api("GET", "/stats")
    print(f"\n  Final graph: {stats['nodes']} nodes, {stats['edges']} edges")
    print(f"\n  Pattern: ERR -> CAUSE -> FIX with confidence lifecycle")
    print(f"  - Confirmed fixes rise via /learn/reinforce")
    print(f"  - Wrong diagnoses drop via /learn/correct")
    print(f"  - Stale knowledge fades via /learn/decay")
    print(f"  - Impact propagates via inference rules")
    print(f"  - Everything is explainable via /explain")


if __name__ == "__main__":
    main()
