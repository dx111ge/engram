#!/usr/bin/env python3
"""
Use Case 12: Codebase Understanding -- Real AST Analysis

Parses the psf/requests library using Python's ast module and builds
a knowledge graph in engram. No simulation -- real code analysis.

Usage:
  git clone --depth 1 https://github.com/psf/requests.git /tmp/requests-repo
  engram serve codebase.brain 127.0.0.1:3030
  python codebase_demo.py
"""

import ast
import os
import sys
import requests as http

ENGRAM = "http://127.0.0.1:3030"
# Try common temp locations (Windows resolves /tmp differently in Python vs bash)
_CANDIDATES = [
    "/tmp/requests-repo/src/requests",
    os.path.join(os.environ.get("TEMP", ""), "requests-repo", "src", "requests"),
    os.path.join(os.environ.get("TMP", ""), "requests-repo", "src", "requests"),
    os.path.expanduser("~/requests-repo/src/requests"),
]
REPO_PATH = next((p for p in _CANDIDATES if os.path.isdir(p)), _CANDIDATES[0])


def api(method, path, payload=None):
    url = f"{ENGRAM}{path}"
    if method == "GET":
        r = http.get(url, timeout=10)
    elif method == "POST":
        r = http.post(url, json=payload, timeout=10)
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


# ── AST Analysis ──────────────────────────────────────────────────

def analyze_module(filepath, module_name):
    """Parse a Python file and extract structural information."""
    with open(filepath, "r", encoding="utf-8", errors="replace") as f:
        source = f.read()

    try:
        tree = ast.parse(source, filename=filepath)
    except SyntaxError as e:
        return {"error": str(e)}

    result = {
        "module": module_name,
        "filepath": filepath,
        "lines": len(source.splitlines()),
        "classes": [],
        "functions": [],
        "imports": [],
        "from_imports": [],
    }

    for node in ast.iter_child_nodes(tree):
        if isinstance(node, ast.ClassDef):
            bases = []
            for base in node.bases:
                if isinstance(base, ast.Name):
                    bases.append(base.id)
                elif isinstance(base, ast.Attribute):
                    bases.append(ast.dump(base))

            methods = []
            for item in node.body:
                if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    methods.append(item.name)

            decorators = []
            for dec in node.decorator_list:
                if isinstance(dec, ast.Name):
                    decorators.append(dec.id)
                elif isinstance(dec, ast.Attribute):
                    decorators.append(f"{ast.dump(dec)}")

            result["classes"].append({
                "name": node.name,
                "bases": bases,
                "methods": methods,
                "method_count": len(methods),
                "line": node.lineno,
                "decorators": decorators,
                "docstring": ast.get_docstring(node) or "",
            })

        elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            args = []
            for arg in node.args.args:
                args.append(arg.arg)

            result["functions"].append({
                "name": node.name,
                "args": args,
                "line": node.lineno,
                "is_async": isinstance(node, ast.AsyncFunctionDef),
                "docstring": ast.get_docstring(node) or "",
            })

        elif isinstance(node, ast.Import):
            for alias in node.names:
                result["imports"].append(alias.name)

        elif isinstance(node, ast.ImportFrom):
            if node.module:
                names = [alias.name for alias in node.names]
                result["from_imports"].append({
                    "module": node.module,
                    "names": names,
                })

    return result


def import_module_to_engram(analysis):
    """Store one module's analysis in engram."""
    mod_name = analysis["module"]
    mod_label = f"mod:{mod_name}"

    # Store module node
    store(mod_label, "module", {
        "lines": analysis["lines"],
        "class_count": len(analysis["classes"]),
        "function_count": len(analysis["functions"]),
        "import_count": len(analysis["imports"]) + len(analysis["from_imports"]),
    }, confidence=0.95, source="ast-parser")

    # Store classes
    for cls in analysis["classes"]:
        cls_label = f"class:{mod_name}.{cls['name']}"
        store(cls_label, "class", {
            "method_count": cls["method_count"],
            "line": cls["line"],
            "has_docstring": "yes" if cls["docstring"] else "no",
        }, confidence=0.95, source="ast-parser")

        # Class defined in module
        relate(cls_label, "defined_in", mod_label, 0.95)

        # Inheritance
        for base in cls["bases"]:
            base_label = f"class:{base}"
            store(base_label, "class", {}, confidence=0.70, source="ast-parser")
            relate(cls_label, "inherits_from", base_label, 0.90)

        # Methods
        for method in cls["methods"]:
            if method.startswith("_") and method != "__init__":
                continue  # skip private/dunder (except __init__)
            meth_label = f"fn:{mod_name}.{cls['name']}.{method}"
            store(meth_label, "method", {
                "class": cls["name"],
            }, confidence=0.90, source="ast-parser")
            relate(meth_label, "defined_in", cls_label, 0.95)

    # Store module-level functions
    for func in analysis["functions"]:
        fn_label = f"fn:{mod_name}.{func['name']}"
        store(fn_label, "function", {
            "args": ",".join(func["args"]),
            "line": func["line"],
            "is_async": str(func["is_async"]),
            "has_docstring": "yes" if func["docstring"] else "no",
        }, confidence=0.90, source="ast-parser")
        relate(fn_label, "defined_in", mod_label, 0.95)

    # Store import relationships
    for imp in analysis["imports"]:
        imp_label = f"mod:{imp}"
        store(imp_label, "external_module", {}, confidence=0.80, source="ast-parser")
        relate(mod_label, "imports", imp_label, 0.85)

    for frm in analysis["from_imports"]:
        imp_mod = frm["module"]
        # Internal vs external
        if imp_mod.startswith(".") or imp_mod.startswith("requests"):
            # Internal import -- resolve to our module
            if imp_mod.startswith("."):
                imp_label = f"mod:{imp_mod.lstrip('.')}"
            else:
                parts = imp_mod.split(".")
                imp_label = f"mod:{parts[-1]}" if len(parts) > 1 else f"mod:{imp_mod}"
            relate(mod_label, "imports", imp_label, 0.90)
        else:
            imp_label = f"mod:{imp_mod}"
            store(imp_label, "external_module", {}, confidence=0.80, source="ast-parser")
            relate(mod_label, "imports", imp_label, 0.80)

    return analysis


def main():
    try:
        health = api("GET", "/health")
        print(f"Server: {health}")
    except Exception as e:
        print(f"Server not reachable at {ENGRAM}: {e}")
        print("Start engram first: engram serve codebase.brain 127.0.0.1:3030")
        sys.exit(1)

    if not os.path.isdir(REPO_PATH):
        print(f"Repo not found at {REPO_PATH}")
        print("Clone first: git clone --depth 1 https://github.com/psf/requests.git /tmp/requests-repo")
        sys.exit(1)

    # ================================================================
    section("PHASE 1: AST Analysis of psf/requests")
    # ================================================================

    # Store the project root
    store("requests", "library", {
        "repo": "https://github.com/psf/requests",
        "description": "Python HTTP library for humans",
    }, confidence=0.95, source="github")

    py_files = sorted([
        f for f in os.listdir(REPO_PATH)
        if f.endswith(".py") and not f.startswith("__pycache__")
    ])

    all_analyses = {}
    total_classes = 0
    total_functions = 0
    total_lines = 0

    for filename in py_files:
        filepath = os.path.join(REPO_PATH, filename)
        mod_name = filename.replace(".py", "")

        analysis = analyze_module(filepath, mod_name)
        if "error" in analysis:
            print(f"  SKIP {filename}: {analysis['error']}")
            continue

        all_analyses[mod_name] = analysis
        import_module_to_engram(analysis)

        # Link module to library
        relate(f"mod:{mod_name}", "part_of", "requests", 0.95)

        nc = len(analysis["classes"])
        nf = len(analysis["functions"])
        total_classes += nc
        total_functions += nf
        total_lines += analysis["lines"]

        print(f"  {filename:25s} {analysis['lines']:4d} lines, "
              f"{nc} classes, {nf} functions, "
              f"{len(analysis['imports']) + len(analysis['from_imports'])} imports")

    stats = api("GET", "/stats")
    print(f"\n  Totals: {len(py_files)} modules, {total_classes} classes, "
          f"{total_functions} functions, {total_lines} lines")
    print(f"  Graph: {stats['nodes']} nodes, {stats['edges']} edges")

    # ================================================================
    section("PHASE 2: Architectural Insights")
    # ================================================================

    subsection("Core modules by size (lines)")
    modules = []
    for mod_name, analysis in sorted(all_analyses.items()):
        modules.append((mod_name, analysis["lines"], len(analysis["classes"]),
                         len(analysis["functions"])))
    for mod, lines, nc, nf in sorted(modules, key=lambda x: -x[1])[:10]:
        bar = "#" * (lines // 20)
        print(f"  {mod:25s} {lines:4d} lines  {nc} cls  {nf} fn  {bar}")

    subsection("Class hierarchy")
    for mod_name, analysis in sorted(all_analyses.items()):
        for cls in analysis["classes"]:
            if cls["bases"]:
                cls_label = f"{mod_name}.{cls['name']}"
                bases_str = ", ".join(cls["bases"])
                print(f"  {cls_label} -> {bases_str}")

    subsection("Modules with most imports (coupling)")
    import_counts = []
    for mod_name, analysis in all_analyses.items():
        count = len(analysis["imports"]) + len(analysis["from_imports"])
        import_counts.append((mod_name, count))
    for mod, count in sorted(import_counts, key=lambda x: -x[1])[:8]:
        print(f"  {mod:25s} {count} imports")

    # ================================================================
    section("PHASE 3: Inference -- Derive Architectural Patterns")
    # ================================================================

    subsection("Rule: transitive dependencies")
    rule_transitive = (
        'rule transitive_dep\n'
        'when edge(mod_a, "imports", mod_b)\n'
        'when edge(mod_b, "imports", mod_c)\n'
        'then edge(mod_a, "transitively_depends_on", mod_c, min(e1, e2))'
    )

    subsection("Rule: flag modules with high coupling")
    rule_coupling = (
        'rule high_coupling\n'
        'when edge(mod, "imports", dep1)\n'
        'when edge(mod, "imports", dep2)\n'
        'when edge(mod, "imports", dep3)\n'
        'then flag(mod, "high coupling: 3+ imports")'
    )

    result = api("POST", "/learn/derive", {"rules": [rule_transitive, rule_coupling]})
    print(f"  Rules fired: {result.get('rules_fired', '?')}")
    print(f"  Flags raised: {result.get('flags_raised', '?')}")

    subsection("Flagged modules (high coupling)")
    for mod_name in sorted(all_analyses.keys()):
        try:
            node = api("GET", f"/node/mod:{mod_name}")
            flag = node.get("properties", {}).get("_flag")
            if flag:
                print(f"  FLAGGED: mod:{mod_name} -- {flag}")
        except Exception:
            pass

    # ================================================================
    section("PHASE 4: Query the Codebase Graph")
    # ================================================================

    subsection("Search: all classes")
    result = api("POST", "/search", {"query": "type:class", "limit": 20})
    hits = result.get("results", [])
    print(f"  Classes found: {len(hits)}")
    for h in hits[:10]:
        print(f"    {h['label']} (conf={h['confidence']:.2f})")
    if len(hits) > 10:
        print(f"    ... and {len(hits) - 10} more")

    subsection("Search: all functions")
    result = api("POST", "/search", {"query": "type:function", "limit": 30})
    hits = result.get("results", [])
    print(f"  Functions found: {len(hits)}")
    for h in hits[:8]:
        print(f"    {h['label']} (conf={h['confidence']:.2f})")
    if len(hits) > 8:
        print(f"    ... and {len(hits) - 8} more")

    subsection("Text search: 'session'")
    result = api("POST", "/search", {"query": "session", "limit": 10})
    hits = result.get("results", [])
    for h in hits[:5]:
        print(f"  {h['label']}: conf={h['confidence']:.2f}")

    # ================================================================
    section("PHASE 5: Architectural Exploration via Traversal")
    # ================================================================

    subsection("Traverse from mod:sessions (depth=2) -- the heart of requests")
    result = api("POST", "/query", {
        "start": "mod:sessions", "depth": 2, "min_confidence": 0.0,
    })
    nodes = result.get("nodes", [])
    print(f"  Reachable: {len(nodes)} nodes")
    for n in sorted(nodes, key=lambda x: (x.get("depth", 0), x["label"]))[:15]:
        print(f"    [depth={n.get('depth', '?')}] {n['label']} (conf={n['confidence']:.2f})")
    if len(nodes) > 15:
        print(f"    ... and {len(nodes) - 15} more")

    subsection("Traverse from class:models.Response (depth=2)")
    result = api("POST", "/query", {
        "start": "class:models.Response", "depth": 2, "min_confidence": 0.0,
    })
    nodes = result.get("nodes", [])
    print(f"  Reachable: {len(nodes)} nodes")
    for n in sorted(nodes, key=lambda x: (x.get("depth", 0), x["label"]))[:10]:
        print(f"    [depth={n.get('depth', '?')}] {n['label']} (conf={n['confidence']:.2f})")

    # ================================================================
    section("PHASE 6: Explainability")
    # ================================================================

    subsection("Explain: mod:sessions")
    resp = api("GET", "/explain/mod:sessions")
    print(f"  Confidence: {resp.get('confidence', '?'):.2f}")
    edges_from = resp.get("edges_from", [])
    edges_to = resp.get("edges_to", [])
    print(f"  Outgoing edges ({len(edges_from)}):")
    for e in edges_from[:8]:
        print(f"    -[{e['relationship']}]-> {e['to']} (conf={e.get('confidence', '?')})")
    if len(edges_from) > 8:
        print(f"    ... and {len(edges_from) - 8} more")
    print(f"  Incoming edges ({len(edges_to)}):")
    for e in edges_to[:5]:
        print(f"    {e['from']} -[{e['relationship']}]-> (conf={e.get('confidence', '?')})")
    if len(edges_to) > 5:
        print(f"    ... and {len(edges_to) - 5} more")

    # ================================================================
    section("PHASE 7: Export as JSON-LD")
    # ================================================================

    subsection("Export codebase knowledge as linked data")
    jsonld = api("GET", "/export/jsonld")
    graph_nodes = jsonld.get("@graph", [])
    print(f"  Exported {len(graph_nodes)} nodes as JSON-LD")

    # ================================================================
    section("SUMMARY")
    # ================================================================

    stats = api("GET", "/stats")
    print(f"\n  Final graph: {stats['nodes']} nodes, {stats['edges']} edges")
    print(f"  Source: psf/requests ({total_lines} lines Python)")
    print(f"  Modules: {len(py_files)}, Classes: {total_classes}, "
          f"Functions: {total_functions}")
    print(f"\n  Codebase understanding demonstrated:")
    print(f"    - Real AST parsing (no simulation)")
    print(f"    - Module, class, function, and method extraction")
    print(f"    - Import dependency graph (internal + external)")
    print(f"    - Class inheritance hierarchy")
    print(f"    - Inference: transitive deps + coupling detection")
    print(f"    - Architectural exploration via graph traversal")
    print(f"    - Text search across code entities")
    print(f"    - JSON-LD export of codebase knowledge")


if __name__ == "__main__":
    main()
