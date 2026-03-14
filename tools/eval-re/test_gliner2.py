"""
Test GLiNER2 multilingual NER + RE with German text.

Two approaches tested:
  A) gliner library (predict_entities for NER)
  B) gliner2 library (predict for NER+RE with schema, native relation support)

Prerequisites:
    pip install gliner gliner2 optimum[onnxruntime]

ONNX export:
    optimum-cli export onnx --model fastino/gliner2-multi-v1 --task feature-extraction gliner2_onnx/
"""

import sys
import time
import json
from pathlib import Path

# Test sentences
TEST_CASES = [
    {
        "id": "S0", "lang": "EN",
        "text": "Bill Gates is an American businessman who co-founded Microsoft.",
        "ner_labels": ["person", "company", "city", "country"],
        "re_schema": {
            "entities": ["person", "company"],
            "relations": [
                {"label": "founded", "source": "person", "target": "company"},
            ],
        },
        "expected_entities": [("Bill Gates", "person"), ("Microsoft", "company")],
        "expected_relations": [("Bill Gates", "founded", "Microsoft")],
    },
    {
        "id": "S1", "lang": "DE",
        "text": "Tim Cook ist der CEO von Apple. Apple hat seinen Hauptsitz in Cupertino.",
        "ner_labels": ["person", "company", "city", "country"],
        "re_schema": {
            "entities": ["person", "company", "city"],
            "relations": [
                {"label": "works_at", "source": "person", "target": "company"},
                {"label": "headquartered_in", "source": "company", "target": "city"},
            ],
        },
        "expected_entities": [("Tim Cook", "person"), ("Apple", "company"), ("Cupertino", "city")],
        "expected_relations": [("Tim Cook", "works_at", "Apple"), ("Apple", "headquartered_in", "Cupertino")],
    },
    {
        "id": "S2", "lang": "DE",
        "text": "Max arbeitet bei Siemens in Muenchen.",
        "ner_labels": ["person", "company", "city"],
        "re_schema": {
            "entities": ["person", "company", "city"],
            "relations": [
                {"label": "works_at", "source": "person", "target": "company"},
                {"label": "located_in", "source": "company", "target": "city"},
            ],
        },
        "expected_entities": [("Max", "person"), ("Siemens", "company"), ("Muenchen", "city")],
        "expected_relations": [("Max", "works_at", "Siemens"), ("Siemens", "located_in", "Muenchen")],
    },
    {
        "id": "S3", "lang": "DE",
        "text": "Angela Merkel war Bundeskanzlerin von Deutschland.",
        "ner_labels": ["person", "country", "political role"],
        "re_schema": {
            "entities": ["person", "country"],
            "relations": [
                {"label": "leads", "source": "person", "target": "country"},
            ],
        },
        "expected_entities": [("Angela Merkel", "person"), ("Deutschland", "country")],
        "expected_relations": [("Angela Merkel", "leads", "Deutschland")],
    },
    {
        "id": "S4", "lang": "DE",
        "text": "Putin und Zelensky verhandeln ueber den Konflikt in der Ukraine. NATO unterstuetzt die Ukraine mit HIMARS.",
        "ner_labels": ["person", "country", "organization", "weapon"],
        "re_schema": {
            "entities": ["person", "country", "organization", "weapon"],
            "relations": [
                {"label": "supports", "source": "organization", "target": "country"},
                {"label": "citizen_of", "source": "person", "target": "country"},
            ],
        },
        "expected_entities": [("Putin", "person"), ("Zelensky", "person"), ("Ukraine", "country"), ("NATO", "organization"), ("HIMARS", "weapon")],
        "expected_relations": [("NATO", "supports", "Ukraine")],
    },
]

RESULTS = {}


def test_approach_a_gliner():
    """Test with the 'gliner' library (NER only, predict_entities)."""
    print("=" * 60)
    print("Approach A: gliner library (NER only)")
    print("=" * 60)
    print()

    try:
        from gliner import GLiNER
    except ImportError:
        print("SKIP: 'gliner' not installed (pip install gliner)")
        return

    print("Loading fastino/gliner2-multi-v1 via gliner ...")
    load_start = time.time()
    try:
        model = GLiNER.from_pretrained("fastino/gliner2-multi-v1", trust_remote_code=True)
        print(f"Loaded in {time.time() - load_start:.1f}s")
    except Exception as e:
        print(f"ERROR: {e}")
        return

    print()
    for tc in TEST_CASES:
        print(f"--- {tc['id']} ({tc['lang']}) ---")
        print(f'Input: "{tc["text"]}"')
        start = time.time()
        try:
            entities = model.predict_entities(tc["text"], tc["ner_labels"], threshold=0.3)
            elapsed_ms = (time.time() - start) * 1000
            print(f"  NER ({elapsed_ms:.1f}ms):")
            for ent in entities:
                print(f"    {ent['text']:20} | {ent['label']:15} | {ent['score']:.1%}")
            if not entities:
                print("    (none)")
            print(f"  Expected:")
            for text, label in tc["expected_entities"]:
                print(f"    {text:20} | {label:15}")
            RESULTS.setdefault(tc["id"], {})["gliner_ner"] = entities
            RESULTS[tc["id"]]["gliner_ner_ms"] = elapsed_ms
        except Exception as e:
            print(f"  ERROR: {e}")
        print()

    # Check for RE capability
    print("Checking for RE methods on GLiNER model...")
    re_methods = [m for m in dir(model) if 'relat' in m.lower() or 'predict' in m.lower()]
    print(f"  Relevant methods: {re_methods}")
    print()


def test_approach_b_gliner2():
    """Test with the 'gliner2' library (NER + RE with schema)."""
    print("=" * 60)
    print("Approach B: gliner2 library (NER + RE schema)")
    print("=" * 60)
    print()

    try:
        from gliner2 import GLiNER
    except ImportError:
        print("SKIP: 'gliner2' not installed (pip install gliner2)")
        print("  This library supports schema-based relation extraction.")
        return

    print("Loading fastino/gliner2-multi-v1 via gliner2 ...")
    load_start = time.time()
    try:
        model = GLiNER.from_pretrained("fastino/gliner2-multi-v1")
        print(f"Loaded in {time.time() - load_start:.1f}s")
    except Exception as e:
        print(f"ERROR: {e}")
        return

    print()
    for tc in TEST_CASES:
        print(f"--- {tc['id']} ({tc['lang']}) ---")
        print(f'Input: "{tc["text"]}"')
        start = time.time()
        try:
            entities, relations = model.predict(tc["text"], tc["re_schema"])
            elapsed_ms = (time.time() - start) * 1000

            print(f"  NER ({elapsed_ms:.1f}ms):")
            if entities:
                for ent in entities:
                    text = ent.get("text", ent.get("span", "?"))
                    label = ent.get("label", ent.get("class", "?"))
                    score = ent.get("score", ent.get("probability", 0))
                    print(f"    {str(text):20} | {str(label):15} | {score:.1%}")
            else:
                print("    (none)")

            print(f"  RE:")
            if relations:
                for rel in relations:
                    print(f"    {rel}")
            else:
                print("    (none)")

            print(f"  Expected Relations:")
            for s, r, o in tc["expected_relations"]:
                print(f"    {s:20} --[{r:15}]--> {o}")

            RESULTS.setdefault(tc["id"], {})["gliner2_ner"] = entities
            RESULTS[tc["id"]]["gliner2_re"] = relations
            RESULTS[tc["id"]]["gliner2_ms"] = elapsed_ms

        except TypeError as e:
            # model.predict might not return (entities, relations) tuple
            print(f"  predict() signature mismatch: {e}")
            print("  Trying alternative API patterns...")
            try:
                result = model.predict(tc["text"], tc["re_schema"])
                print(f"  Raw result type: {type(result)}")
                print(f"  Raw result: {result}")
            except Exception as e2:
                print(f"  ERROR: {e2}")
        except Exception as e:
            print(f"  ERROR: {e}")
        print()


def test_onnx_export():
    """Export GLiNER2 to ONNX via optimum-cli."""
    print("=" * 60)
    print("ONNX Export Test")
    print("=" * 60)
    print()

    output_dir = Path.home() / ".engram" / "models" / "rel" / "gliner2-multi-v1"
    output_dir.mkdir(parents=True, exist_ok=True)

    # Try optimum-cli export
    import subprocess
    print(f"Exporting to {output_dir} via optimum-cli ...")
    try:
        result = subprocess.run(
            [
                sys.executable, "-m", "optimum.exporters.onnx",
                "--model", "fastino/gliner2-multi-v1",
                "--task", "feature-extraction",
                str(output_dir),
            ],
            capture_output=True, text=True, timeout=600,
        )
        print(f"  stdout: {result.stdout[-500:] if result.stdout else '(empty)'}")
        if result.returncode != 0:
            print(f"  stderr: {result.stderr[-500:] if result.stderr else '(empty)'}")
            print(f"  Exit code: {result.returncode}")
        else:
            print("  Export succeeded!")
            for f in sorted(output_dir.iterdir()):
                size_mb = f.stat().st_size / (1024 * 1024)
                print(f"    {f.name}: {size_mb:.1f} MB")
    except FileNotFoundError:
        print("  ERROR: optimum not installed (pip install optimum[onnxruntime])")
    except subprocess.TimeoutExpired:
        print("  ERROR: Export timed out after 600s")
    except Exception as e:
        print(f"  ERROR: {e}")
    print()


def write_results():
    """Write results to JSON for comparison."""
    output_path = Path(__file__).parent / "gliner2_results.json"
    with open(output_path, "w") as f:
        json.dump(RESULTS, f, indent=2, default=str)
    print(f"Results saved to {output_path}")


if __name__ == "__main__":
    print("=" * 60)
    print("GLiNER2 Multilingual Evaluation")
    print("Model: fastino/gliner2-multi-v1")
    print("=" * 60)
    print()

    # Test Approach A: gliner library (NER only)
    test_approach_a_gliner()

    # Test Approach B: gliner2 library (NER + RE)
    test_approach_b_gliner2()

    # ONNX export (only if requested)
    if "--export" in sys.argv:
        test_onnx_export()

    write_results()
    print()
    print("=== Evaluation complete ===")
