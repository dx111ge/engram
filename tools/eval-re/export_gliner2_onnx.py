"""
Export any GLiNER2 PyTorch model to ONNX format (4-file gliner2-onnx runtime format).

Produces:
  encoder.onnx       - mDeBERTa transformer (the heavy part)
  encoder_int8.onnx   - INT8 quantized encoder (optional, --quantize)
  span_rep.onnx       - span representation layer
  count_embed.onnx    - label embedding transform (GRU)
  count_pred.onnx     - count predictor
  classifier.onnx     - scoring head
  gliner2_config.json - model config with special tokens + ONNX file paths
  tokenizer files     - HuggingFace tokenizer

Usage:
    python export_gliner2_onnx.py fastino/gliner2-multi-v1 output_dir/
    python export_gliner2_onnx.py fastino/gliner2-multi-v1 output_dir/ --quantize
    python export_gliner2_onnx.py fastino/gliner2-large-v1 output_dir/ --quantize

Requirements:
    pip install gliner2 torch onnx onnxscript onnxruntime
"""

import sys
import io
import os
import json
import time
import argparse
from pathlib import Path

# Fix Windows encoding
if sys.platform == "win32":
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8")
    sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description="Export GLiNER2 to ONNX")
    parser.add_argument("model_id", help="HuggingFace model ID (e.g., fastino/gliner2-multi-v1)")
    parser.add_argument("output_dir", help="Output directory for ONNX files")
    parser.add_argument("--quantize", action="store_true", help="Also export INT8 quantized encoder")
    parser.add_argument("--opset", type=int, default=18, help="ONNX opset version (default: 18)")
    args = parser.parse_args()

    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    import torch
    import torch.nn as nn
    from gliner2 import GLiNER2

    print(f"Model:  {args.model_id}")
    print(f"Output: {output_dir}")
    print()

    # Load model
    print("Loading PyTorch model...")
    t0 = time.time()
    model = GLiNER2.from_pretrained(args.model_id)
    model.eval()
    print(f"  Loaded in {time.time() - t0:.1f}s")
    print(f"  Encoder: {model.encoder.config.model_type}")
    print(f"  Hidden:  {model.encoder.config.hidden_size}")
    print(f"  Width:   {model.max_width}")
    print()

    hidden_size = model.encoder.config.hidden_size

    # Create a sample batch to trace shapes
    model.processor.change_mode(is_training=False)
    from gliner2.training.trainer import ExtractorCollator
    from torch.utils.data import DataLoader

    text = "Test sentence for export."
    schema = model.create_schema().entities(["person", "company"]).build()
    for cls_config in schema.get("classifications", []):
        cls_config.setdefault("true_label", ["N/A"])
    dataset = [(text, schema)]
    collator = ExtractorCollator(model.processor, is_training=False)
    loader = DataLoader(dataset, batch_size=1, collate_fn=collator)
    batch = next(iter(loader))

    # ---- 1. ENCODER ----
    print("Exporting encoder.onnx ...")
    t0 = time.time()
    torch.onnx.export(
        model.encoder,
        (batch.input_ids, batch.attention_mask),
        str(output_dir / "encoder.onnx"),
        input_names=["input_ids", "attention_mask"],
        output_names=["hidden_state"],
        dynamic_axes={
            "input_ids": {0: "batch", 1: "seq_len"},
            "attention_mask": {0: "batch", 1: "seq_len"},
            "hidden_state": {0: "batch", 1: "seq_len"},
        },
        opset_version=args.opset,
        do_constant_folding=True,
    )
    print(f"  Done ({time.time() - t0:.1f}s)")

    # ---- 2. SPAN_REP (flat, no reshape) ----
    print("Exporting span_rep.onnx ...")
    from gliner.modeling.span_rep import extract_elements

    class SpanRepFlatWrapper(nn.Module):
        def __init__(self, span_rep_layer):
            super().__init__()
            self.project_start = span_rep_layer.project_start
            self.project_end = span_rep_layer.project_end
            self.out_project = span_rep_layer.out_project

        def forward(self, hidden_states, span_start_idx, span_end_idx):
            start_rep = self.project_start(hidden_states)
            end_rep = self.project_end(hidden_states)
            start_span = extract_elements(start_rep, span_start_idx)
            end_span = extract_elements(end_rep, span_end_idx)
            cat = torch.cat([start_span, end_span], dim=-1).relu()
            return self.out_project(cat)

    wrapper = SpanRepFlatWrapper(model.span_rep.span_rep_layer)
    wrapper.eval()

    dummy_h = torch.randn(1, 10, hidden_size)
    dummy_s = torch.randint(0, 10, (1, 55))
    dummy_e = torch.randint(0, 10, (1, 55))

    t0 = time.time()
    torch.onnx.export(
        wrapper,
        (dummy_h, dummy_s, dummy_e),
        str(output_dir / "span_rep.onnx"),
        input_names=["hidden_states", "span_start_idx", "span_end_idx"],
        output_names=["span_representations"],
        dynamic_axes={
            "hidden_states": {0: "batch", 1: "text_len"},
            "span_start_idx": {0: "batch", 1: "num_spans"},
            "span_end_idx": {0: "batch", 1: "num_spans"},
            "span_representations": {0: "batch", 1: "num_spans"},
        },
        opset_version=args.opset,
        do_constant_folding=True,
    )
    print(f"  Done ({time.time() - t0:.1f}s)")

    # ---- 3. COUNT_EMBED (fixed count=1) ----
    print("Exporting count_embed.onnx ...")

    class CountEmbedFixed(nn.Module):
        def __init__(self, count_embed):
            super().__init__()
            self.count_embed = count_embed

        def forward(self, label_embeddings):
            result = self.count_embed(label_embeddings, 1)
            return result.squeeze(0)

    ce_wrapper = CountEmbedFixed(model.count_embed)
    ce_wrapper.eval()

    t0 = time.time()
    torch.onnx.export(
        ce_wrapper,
        (torch.randn(3, hidden_size),),
        str(output_dir / "count_embed.onnx"),
        input_names=["label_embeddings"],
        output_names=["transformed_embeddings"],
        dynamic_axes={
            "label_embeddings": {0: "num_labels"},
            "transformed_embeddings": {0: "num_labels"},
        },
        opset_version=args.opset,
        do_constant_folding=True,
    )
    print(f"  Done ({time.time() - t0:.1f}s)")

    # ---- 4. COUNT_PRED ----
    print("Exporting count_pred.onnx ...")

    class CountPredWrapper(nn.Module):
        def __init__(self, count_pred):
            super().__init__()
            self.count_pred = count_pred

        def forward(self, schema_embedding):
            return self.count_pred(schema_embedding)

    cp_wrapper = CountPredWrapper(model.count_pred)
    cp_wrapper.eval()

    t0 = time.time()
    torch.onnx.export(
        cp_wrapper,
        (torch.randn(1, hidden_size),),
        str(output_dir / "count_pred.onnx"),
        input_names=["schema_embedding"],
        output_names=["count_logits"],
        dynamic_axes={"schema_embedding": {0: "batch"}, "count_logits": {0: "batch"}},
        opset_version=args.opset,
        do_constant_folding=True,
    )
    print(f"  Done ({time.time() - t0:.1f}s)")

    # ---- 5. CLASSIFIER ----
    print("Exporting classifier.onnx ...")

    class ClassifierWrapper(nn.Module):
        def __init__(self, classifier):
            super().__init__()
            self.classifier = classifier

        def forward(self, hidden_state):
            return self.classifier(hidden_state)

    cls_wrapper = ClassifierWrapper(model.classifier)
    cls_wrapper.eval()

    t0 = time.time()
    torch.onnx.export(
        cls_wrapper,
        (torch.randn(1, hidden_size),),
        str(output_dir / "classifier.onnx"),
        input_names=["hidden_state"],
        output_names=["logits"],
        dynamic_axes={"hidden_state": {0: "batch"}, "logits": {0: "batch"}},
        opset_version=args.opset,
        do_constant_folding=True,
    )
    print(f"  Done ({time.time() - t0:.1f}s)")

    # ---- 6. TOKENIZER ----
    print("Saving tokenizer ...")
    model.processor.tokenizer.save_pretrained(str(output_dir))

    # ---- 7. INT8 QUANTIZATION (optional) ----
    if args.quantize:
        print("Quantizing encoder to INT8 ...")
        from onnxruntime.quantization import quantize_dynamic, QuantType

        t0 = time.time()
        quantize_dynamic(
            str(output_dir / "encoder.onnx"),
            str(output_dir / "encoder_int8.onnx"),
            weight_type=QuantType.QInt8,
        )
        print(f"  Done ({time.time() - t0:.1f}s)")

    # ---- 8. CONFIG ----
    print("Saving gliner2_config.json ...")
    tokenizer = model.processor.tokenizer
    special_tokens = {}
    for tok in ["[P]", "[L]", "[E]", "[R]", "[C]", "[SEP_TEXT]", "[DESCRIPTION]", "[EXAMPLE]", "[OUTPUT]"]:
        tid = tokenizer.added_tokens_encoder.get(tok)
        if tid is not None:
            special_tokens[tok] = tid

    onnx_files = {
        "fp32": {
            "encoder": "encoder.onnx",
            "classifier": "classifier.onnx",
            "span_rep": "span_rep.onnx",
            "count_embed": "count_embed.onnx",
            "count_pred": "count_pred.onnx",
        }
    }
    if args.quantize:
        onnx_files["int8"] = {
            "encoder": "encoder_int8.onnx",
            "classifier": "classifier.onnx",
            "span_rep": "span_rep.onnx",
            "count_embed": "count_embed.onnx",
            "count_pred": "count_pred.onnx",
        }

    config = {
        "max_width": model.max_width,
        "special_tokens": special_tokens,
        "onnx_files": onnx_files,
        "model_name": args.model_id,
        "encoder_model": getattr(model.config, "model_name", "unknown"),
        "hidden_size": hidden_size,
        "max_count": model.count_embed.max_count,
    }

    with open(output_dir / "gliner2_config.json", "w") as f:
        json.dump(config, f, indent=2)

    # ---- SUMMARY ----
    print()
    print("=" * 50)
    print("Exported files:")
    print("=" * 50)
    total = 0
    for f in sorted(output_dir.iterdir()):
        size_mb = f.stat().st_size / (1024 * 1024)
        total += size_mb
        print(f"  {f.name:30} {size_mb:8.1f} MB")
    print(f"  {'TOTAL':30} {total:8.1f} MB")
    print()
    print("Done!")


if __name__ == "__main__":
    main()
