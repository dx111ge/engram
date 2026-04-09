#!/usr/bin/env python3
"""
Use Case 11: Semantic Web -- Biomedical Drug Interaction Knowledge Graph

Demonstrates importing structured biomedical knowledge via JSON-LD,
enriching with confidence from multiple source tiers, using inference
rules to detect drug interactions, handling contradictions, and
exporting as interoperable linked data.

Usage:
  engram serve biomedical.brain 127.0.0.1:3030
  python biomedical_demo.py
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


# ── Simulated Biomedical JSON-LD ──────────────────────────────────
# Modeled after DrugBank, ChEBI, and SNOMED CT structured data.
# In production, this would come from real ontology endpoints.

BIOMEDICAL_JSONLD = {
    "@context": {
        "schema": "https://schema.org/",
        "drugbank": "https://go.drugbank.com/drugs/",
        "chebi": "http://purl.obolibrary.org/obo/CHEBI_",
        "snomed": "http://snomed.info/id/",
        "engram": "engram://vocab/",
        "rdfs": "http://www.w3.org/2000/01/rdf-schema#"
    },
    "@graph": [
        # ── Drugs ──
        {
            "@id": "drugbank:DB00641",
            "@type": "schema:Drug",
            "rdfs:label": "Simvastatin",
            "schema:description": "HMG-CoA reductase inhibitor (statin) for cholesterol",
            "engram:drug_class": "statin",
            "engram:indication": "hypercholesterolemia",
            "engram:metabolized_by": {"@id": "engram://node/CYP3A4"},
        },
        {
            "@id": "drugbank:DB01211",
            "@type": "schema:Drug",
            "rdfs:label": "Clarithromycin",
            "schema:description": "Macrolide antibiotic for bacterial infections",
            "engram:drug_class": "macrolide_antibiotic",
            "engram:indication": "bacterial_infection",
            "engram:inhibits": {"@id": "engram://node/CYP3A4"},
        },
        {
            "@id": "drugbank:DB00945",
            "@type": "schema:Drug",
            "rdfs:label": "Aspirin",
            "schema:description": "NSAID and antiplatelet agent",
            "engram:drug_class": "NSAID",
            "engram:indication": "pain,inflammation,antiplatelet",
            "engram:inhibits": {"@id": "engram://node/COX-1"},
        },
        {
            "@id": "drugbank:DB01050",
            "@type": "schema:Drug",
            "rdfs:label": "Ibuprofen",
            "schema:description": "Non-steroidal anti-inflammatory drug",
            "engram:drug_class": "NSAID",
            "engram:indication": "pain,inflammation",
            "engram:inhibits": {"@id": "engram://node/COX-1"},
            "engram:inhibits_2": {"@id": "engram://node/COX-2"},
        },
        {
            "@id": "drugbank:DB00001",
            "@type": "schema:Drug",
            "rdfs:label": "Warfarin",
            "schema:description": "Vitamin K antagonist anticoagulant",
            "engram:drug_class": "anticoagulant",
            "engram:indication": "thromboembolism",
            "engram:metabolized_by": {"@id": "engram://node/CYP2C9"},
        },
        {
            "@id": "drugbank:DB00563",
            "@type": "schema:Drug",
            "rdfs:label": "Metformin",
            "schema:description": "Biguanide antidiabetic agent",
            "engram:drug_class": "biguanide",
            "engram:indication": "type_2_diabetes",
        },
        # ── Enzymes ──
        {
            "@id": "engram://node/CYP3A4",
            "@type": "chebi:23924",
            "rdfs:label": "CYP3A4",
            "schema:description": "Major cytochrome P450 enzyme, metabolizes ~50% of drugs",
            "engram:enzyme_family": "cytochrome_P450",
        },
        {
            "@id": "engram://node/CYP2C9",
            "@type": "chebi:23924",
            "rdfs:label": "CYP2C9",
            "schema:description": "Cytochrome P450 enzyme, metabolizes warfarin and NSAIDs",
            "engram:enzyme_family": "cytochrome_P450",
        },
        {
            "@id": "engram://node/COX-1",
            "@type": "chebi:23924",
            "rdfs:label": "COX-1",
            "schema:description": "Cyclooxygenase-1, constitutive prostaglandin synthesis",
            "engram:enzyme_family": "cyclooxygenase",
        },
        {
            "@id": "engram://node/COX-2",
            "@type": "chebi:23924",
            "rdfs:label": "COX-2",
            "schema:description": "Cyclooxygenase-2, inducible prostaglandin synthesis",
            "engram:enzyme_family": "cyclooxygenase",
        },
        # ── Diseases ──
        {
            "@id": "snomed:13644009",
            "@type": "schema:MedicalCondition",
            "rdfs:label": "Hypercholesterolemia",
            "schema:description": "Elevated blood cholesterol levels",
        },
        {
            "@id": "snomed:44054006",
            "@type": "schema:MedicalCondition",
            "rdfs:label": "Type 2 Diabetes",
            "schema:description": "Chronic metabolic disorder with insulin resistance",
        },
        {
            "@id": "snomed:371068009",
            "@type": "schema:MedicalCondition",
            "rdfs:label": "Rhabdomyolysis",
            "schema:description": "Breakdown of muscle tissue releasing myoglobin into blood",
            "engram:severity": "life-threatening",
        },
    ]
}


def main():
    try:
        health = api("GET", "/health")
        print(f"Server: {health}")
    except Exception as e:
        print(f"Server not reachable at {ENGRAM}: {e}")
        print("Start engram first: engram serve biomedical.brain 127.0.0.1:3030")
        sys.exit(1)

    # ================================================================
    section("PHASE 1: Import Biomedical JSON-LD")
    # ================================================================

    subsection("Importing structured data (DrugBank, ChEBI, SNOMED CT)")
    nodes_imported = 0
    edges_imported = 0
    graph = BIOMEDICAL_JSONLD.get("@graph", [])
    for item in graph:
        label = item.get("rdfs:label", item.get("@id", ""))
        node_type = item.get("@type", "").split("/")[-1].split(":")[-1].lower()
        props = {}
        rels = []
        for key, val in item.items():
            if key.startswith("@") or key == "rdfs:label":
                continue
            if key == "schema:description":
                props["description"] = val
            elif key.startswith("engram:") and isinstance(val, dict) and "@id" in val:
                rel_name = key.split(":")[-1]
                target = val["@id"].replace("engram://node/", "")
                rels.append((rel_name, target))
            elif key.startswith("engram:"):
                props[key.split(":")[-1]] = val
        store(label, node_type, props, source="ontology")
        nodes_imported += 1
        for rel_name, target in rels:
            relate(label, rel_name, target)
            edges_imported += 1
    print(f"  Nodes imported: {nodes_imported}")
    print(f"  Edges imported: {edges_imported}")

    stats = api("GET", "/stats")
    print(f"  Graph: {stats['nodes']} nodes, {stats['edges']} edges")

    # ================================================================
    section("PHASE 2: Source Reliability Tiers")
    # ================================================================

    subsection("Register sources with reliability tiers")

    sources = [
        ("Source:FDA-Label", "source", {"tier": "1", "type": "regulatory"}, 0.95),
        ("Source:DrugBank", "source", {"tier": "1", "type": "curated_database"}, 0.92),
        ("Source:PubMed-Meta", "source", {"tier": "1", "type": "meta_analysis"}, 0.93),
        ("Source:ClinicalTrial", "source", {"tier": "2", "type": "clinical_trial"}, 0.85),
        ("Source:UpToDate", "source", {"tier": "2", "type": "clinical_reference"}, 0.85),
        ("Source:PatientForum", "source", {"tier": "3", "type": "anecdotal"}, 0.30),
        ("Source:HealthBlog", "source", {"tier": "3", "type": "blog"}, 0.25),
    ]
    for name, stype, props, conf in sources:
        store(name, stype, props, conf)
        print(f"  {name} (tier={props['tier']}, conf={conf})")

    # ================================================================
    section("PHASE 3: Drug-Disease Relationships")
    # ================================================================

    subsection("Link drugs to conditions they treat")
    drug_treats = [
        ("Simvastatin", "treats", "Hypercholesterolemia", 0.95, "Source:FDA-Label"),
        ("Warfarin", "treats", "Thromboembolism", 0.95, "Source:FDA-Label"),
        ("Metformin", "treats", "Type 2 Diabetes", 0.95, "Source:FDA-Label"),
        ("Aspirin", "treats", "Inflammation", 0.85, "Source:DrugBank"),
    ]

    # Store missing disease/condition nodes
    store("Thromboembolism", "condition", {"snomed": "371073003"}, 0.95, "Source:DrugBank")
    store("Inflammation", "condition", {}, 0.90, "Source:DrugBank")

    for drug, rel, condition, conf, source in drug_treats:
        relate(drug, rel, condition, conf)
        relate(drug, "sourced_from", source, 0.90)
        print(f"  {drug} -[{rel}]-> {condition} (conf={conf}, source={source})")

    subsection("Known adverse effects")
    # Simvastatin + CYP3A4 inhibition -> rhabdomyolysis risk
    store("Interaction:Simvastatin-CYP3A4-inhibitor", "drug_interaction", {
        "severity": "major",
        "mechanism": "CYP3A4 inhibition increases simvastatin plasma levels",
        "risk": "rhabdomyolysis",
        "recommendation": "avoid combination or reduce statin dose",
    }, confidence=0.93, source="Source:FDA-Label")
    relate("Interaction:Simvastatin-CYP3A4-inhibitor", "involves", "Simvastatin", 0.95)
    relate("Interaction:Simvastatin-CYP3A4-inhibitor", "involves", "CYP3A4", 0.95)
    relate("Interaction:Simvastatin-CYP3A4-inhibitor", "causes_risk_of", "Rhabdomyolysis", 0.90)
    print(f"  Interaction: Simvastatin + CYP3A4 inhibitor -> Rhabdomyolysis risk")

    # Aspirin + Warfarin -> bleeding risk
    store("Interaction:Aspirin-Warfarin", "drug_interaction", {
        "severity": "major",
        "mechanism": "Additive anticoagulant and antiplatelet effects",
        "risk": "gastrointestinal_bleeding",
        "recommendation": "avoid unless benefit outweighs risk",
    }, confidence=0.92, source="Source:FDA-Label")
    store("GI Bleeding", "adverse_effect", {"severity": "serious"}, 0.90)
    relate("Interaction:Aspirin-Warfarin", "involves", "Aspirin", 0.95)
    relate("Interaction:Aspirin-Warfarin", "involves", "Warfarin", 0.95)
    relate("Interaction:Aspirin-Warfarin", "causes_risk_of", "GI Bleeding", 0.90)
    print(f"  Interaction: Aspirin + Warfarin -> GI Bleeding risk")

    stats = api("GET", "/stats")
    print(f"\n  Graph: {stats['nodes']} nodes, {stats['edges']} edges")

    # ================================================================
    section("PHASE 4: Inference -- Detect New Drug Interactions")
    # ================================================================

    subsection("Rule: CYP3A4 inhibitor + CYP3A4-metabolized drug = interaction risk")
    # Clarithromycin inhibits CYP3A4, Simvastatin is metabolized by CYP3A4
    # The inference engine should flag this combination

    rule_cyp_interaction = (
        'rule cyp_interaction\n'
        'when edge(drug_a, "inhibits", enzyme)\n'
        'when edge(drug_b, "metabolized_by", enzyme)\n'
        'then flag(drug_b, "interaction risk: co-prescribed with CYP inhibitor")'
    )

    rule_same_target = (
        'rule same_enzyme_target\n'
        'when edge(drug_a, "inhibits", enzyme)\n'
        'when edge(drug_b, "inhibits", enzyme)\n'
        'then flag(drug_a, "shares enzyme target with another drug")'
    )

    result = api("POST", "/learn/derive", {"rules": [rule_cyp_interaction, rule_same_target]})
    print(f"  Rules fired: {result.get('rules_fired', '?')}")
    print(f"  Flags raised: {result.get('flags_raised', '?')}")

    subsection("Check flags on drugs")
    drugs = ["Simvastatin", "Clarithromycin", "Aspirin", "Ibuprofen", "Warfarin", "Metformin"]
    for drug in drugs:
        try:
            node = api("GET", f"/node/{drug}")
            flag = node.get("properties", {}).get("_flag")
            if flag:
                print(f"  FLAGGED: {drug} -- {flag}")
            else:
                print(f"  OK: {drug} -- no flags")
        except Exception:
            print(f"  SKIP: {drug} -- not found")

    # ================================================================
    section("PHASE 5: Contradicting Evidence")
    # ================================================================

    subsection("Blog claims Simvastatin + Clarithromycin is safe")
    store("Claim:StatinMacrolideSafe", "claim", {
        "text": "Taking simvastatin with clarithromycin is perfectly safe",
        "status": "disputed",
    }, confidence=0.30, source="Source:HealthBlog")
    relate("Claim:StatinMacrolideSafe", "sourced_from", "Source:HealthBlog", 0.25)
    print(f"  Claim stored from HealthBlog (conf=0.30)")

    subsection("FDA label contradicts the claim")
    store("Evidence:FDA-Contraindication", "evidence", {
        "text": "Simvastatin is contraindicated with strong CYP3A4 inhibitors including clarithromycin",
        "study_type": "regulatory_label",
    }, confidence=0.95, source="Source:FDA-Label")
    relate("Evidence:FDA-Contraindication", "contradicts", "Claim:StatinMacrolideSafe", 0.95)
    relate("Evidence:FDA-Contraindication", "sourced_from", "Source:FDA-Label", 0.95)
    print(f"  FDA evidence contradicts the claim (conf=0.95)")

    subsection("Debunk the claim via correction")
    result = api("POST", "/learn/correct", {
        "entity": "Claim:StatinMacrolideSafe",
        "reason": "Contradicted by FDA label: simvastatin contraindicated with CYP3A4 inhibitors"
    })
    node = api("GET", "/node/Claim:StatinMacrolideSafe")
    print(f"  Claim confidence after correction: {node['confidence']:.2f}")

    # ================================================================
    section("PHASE 6: Evidence Chain Traversal")
    # ================================================================

    subsection("Traverse from Simvastatin (depth=2)")
    result = api("POST", "/query", {
        "start": "Simvastatin", "depth": 2, "min_confidence": 0.0,
    })
    nodes = result.get("nodes", [])
    print(f"  Reachable: {len(nodes)} nodes")
    for n in sorted(nodes, key=lambda x: (x.get("depth", 0), x["label"]))[:12]:
        print(f"    [depth={n.get('depth', '?')}] {n['label']} (conf={n['confidence']:.2f})")
    if len(nodes) > 12:
        print(f"    ... and {len(nodes) - 12} more")

    subsection("Traverse from CYP3A4 (depth=2) -- what drugs involve this enzyme?")
    result = api("POST", "/query", {
        "start": "CYP3A4", "depth": 2, "min_confidence": 0.0,
    })
    nodes = result.get("nodes", [])
    print(f"  Reachable: {len(nodes)} nodes")
    for n in sorted(nodes, key=lambda x: (x.get("depth", 0), x["label"]))[:10]:
        print(f"    [depth={n.get('depth', '?')}] {n['label']} (conf={n['confidence']:.2f})")

    # ================================================================
    section("PHASE 7: Export as JSON-LD")
    # ================================================================

    subsection("Export enriched knowledge as linked data")
    jsonld = api("GET", "/export/jsonld")
    graph_nodes = jsonld.get("@graph", [])
    print(f"  Exported {len(graph_nodes)} nodes as JSON-LD")
    print(f"  Context namespaces: {list(jsonld.get('@context', {}).keys())}")

    # Show a sample node
    for node in graph_nodes:
        if node.get("rdfs:label") == "Simvastatin":
            print(f"\n  Sample node (Simvastatin):")
            print(f"    @id: {node.get('@id')}")
            print(f"    @type: {node.get('@type')}")
            print(f"    confidence: {node.get('engram:confidence')}")
            props = {k: v for k, v in node.items()
                     if not k.startswith('@') and k != 'engram:confidence' and k != 'engram:memoryTier'}
            for k, v in list(props.items())[:5]:
                print(f"    {k}: {v}")
            break

    # ================================================================
    section("PHASE 8: Explainability")
    # ================================================================

    subsection("Explain: Interaction:Simvastatin-CYP3A4-inhibitor")
    resp = api("GET", "/explain/Interaction:Simvastatin-CYP3A4-inhibitor")
    print(f"  Confidence: {resp.get('confidence', '?'):.2f}")
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
    print(f"\n  Biomedical semantic web pipeline demonstrated:")
    print(f"    - JSON-LD import from DrugBank, ChEBI, SNOMED CT vocabularies")
    print(f"    - Source reliability tiers (FDA: 0.95, clinical: 0.85, blog: 0.25)")
    print(f"    - Drug-enzyme-disease relationship modeling")
    print(f"    - Inference rules detect CYP-mediated drug interactions")
    print(f"    - Contradiction: blog claim debunked by FDA contraindication")
    print(f"    - Evidence chain traversal from drug to enzyme to interaction to risk")
    print(f"    - JSON-LD export for interoperability with RDF tools")
    print(f"    - Confidence as patient safety signal")


if __name__ == "__main__":
    main()
