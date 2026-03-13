#!/usr/bin/env python3
"""
Export GLiREL (zero-shot relation extraction) to ONNX format.

Usage:
    pip install glirel torch onnx onnxruntime transformers
    python export_glirel_onnx.py [--output-dir ./glirel-large-v0-onnx]

Downloads jackboyla/glirel-large-v0 (CC BY-NC-SA 4.0) and exports to ONNX.
The exported model takes pre-tokenized inputs (prompt + text) and returns
relation logit scores.

ONNX Inputs:
  input_ids        [B, seq_len]          int64  - tokenized [REL] prompt + text
  attention_mask   [B, seq_len]          int64  - attention mask
  span_idx         [B, num_entities, 2]  int64  - entity start/end token positions
  span_mask        [B, num_entities]     float  - which entity slots are real
  relations_idx    [B, num_pairs, 2]     int64  - entity pair indices
  rel_type_mask    [B, num_labels]       float  - which label slots are real
  num_classes      []                    int64  - number of active relation labels
  prompt_lengths   [B]                   int64  - tokens in [REL] prompt per sample

ONNX Output:
  scores           [B, num_pairs, num_labels] float - logit scores (apply sigmoid)
"""

import argparse
import os
import shutil
import sys

import torch
import torch.nn as nn


def main():
    parser = argparse.ArgumentParser(description="Export GLiREL to ONNX")
    parser.add_argument(
        "--model-id",
        default="jackboyla/glirel-large-v0",
        help="HuggingFace model ID",
    )
    parser.add_argument(
        "--output-dir",
        default="./glirel-large-v0-onnx",
        help="Output directory for ONNX model",
    )
    parser.add_argument(
        "--opset",
        type=int,
        default=17,
        help="ONNX opset version",
    )
    args = parser.parse_args()

    print(f"Loading GLiREL model: {args.model_id}")
    try:
        from glirel import GLiREL
    except ImportError:
        print("ERROR: glirel not installed. Run: pip install glirel")
        sys.exit(1)

    model = GLiREL.from_pretrained(args.model_id)
    model.eval()

    # Extract sub-modules we need
    token_rep = model.token_rep_layer  # transformer backbone
    rnn = model.rnn  # bidirectional LSTM
    span_rep = model.span_rep  # span representation layer
    prompt_rep = model.prompt_rep  # prompt FFN
    rel_rep = model.rel_rep  # relation pair representation layer

    # Check for scorer (could be named differently)
    scorer = None
    if hasattr(model, "classifier"):
        scorer = model.classifier
    elif hasattr(model, "scorer"):
        scorer = model.scorer

    print(f"Model loaded. Hidden size: {model.config.hidden_size if hasattr(model, 'config') else 'unknown'}")

    # Inspect model structure
    print("\nModel attributes:")
    for attr in dir(model):
        if not attr.startswith("_"):
            obj = getattr(model, attr, None)
            if isinstance(obj, nn.Module):
                print(f"  {attr}: {type(obj).__name__}")

    class GLiRELWrapper(nn.Module):
        """Wraps GLiREL for ONNX export with pre-tokenized inputs."""

        def __init__(self, glirel_model):
            super().__init__()
            self.model = glirel_model

        def forward(
            self,
            input_ids: torch.Tensor,       # [B, seq_len]
            attention_mask: torch.Tensor,   # [B, seq_len]
            span_idx: torch.Tensor,         # [B, num_entities, 2]
            span_mask: torch.Tensor,        # [B, num_entities]
            relations_idx: torch.Tensor,    # [B, num_pairs, 2]
            rel_type_mask: torch.Tensor,    # [B, num_labels]
            num_classes: torch.Tensor,      # [] scalar
            prompt_lengths: torch.Tensor,   # [B]
        ) -> torch.Tensor:
            """
            Returns logit scores [B, num_pairs, num_labels].
            Apply sigmoid externally to get probabilities.
            """
            # Step 1: Get transformer output
            # token_rep_layer expects dict with input_ids, attention_mask
            token_input = {
                "input_ids": input_ids,
                "attention_mask": attention_mask,
            }

            # Get word representations from transformer
            # The token_rep_layer may return different formats
            word_rep = self.model.token_rep_layer(token_input)
            if isinstance(word_rep, dict):
                word_rep = word_rep.get("embeddings", word_rep.get("last_hidden_state"))
            elif isinstance(word_rep, tuple):
                word_rep = word_rep[0]

            B = word_rep.shape[0]
            hidden_size = word_rep.shape[-1]

            # Step 2: Split into prompt representations and text representations
            # prompt_lengths tells us where the [REL]...[SEP] prompt ends
            max_prompt_len = prompt_lengths.max().item()
            num_classes_val = num_classes.item()

            # Extract prompt (relation type) representations
            # Take the [REL] token positions from the prompt
            prompt_reps = word_rep[:, :max_prompt_len, :]  # [B, max_prompt_len, H]

            # Extract text representations (after the prompt)
            text_reps_list = []
            max_text_len = word_rep.shape[1] - max_prompt_len
            for b in range(B):
                pl = prompt_lengths[b].item()
                text_rep_b = word_rep[b, pl:, :]  # [remaining, H]
                # Pad to uniform length
                if text_rep_b.shape[0] < max_text_len:
                    pad = torch.zeros(
                        max_text_len - text_rep_b.shape[0],
                        hidden_size,
                        device=word_rep.device,
                        dtype=word_rep.dtype,
                    )
                    text_rep_b = torch.cat([text_rep_b, pad], dim=0)
                else:
                    text_rep_b = text_rep_b[:max_text_len]
                text_reps_list.append(text_rep_b)
            text_reps = torch.stack(text_reps_list, dim=0)  # [B, max_text_len, H]

            # Step 3: LSTM on text representations
            if hasattr(self.model, 'rnn') and self.model.rnn is not None:
                text_reps = self.model.rnn(text_reps)
                if isinstance(text_reps, tuple):
                    text_reps = text_reps[0]

            # Step 4: Get span representations for entities
            span_reps = self.model.span_rep(text_reps, span_idx)  # [B, num_entities, H]

            # Step 5: Get relation pair representations
            rel_reps = self.model.rel_rep(span_reps, relations_idx)  # [B, num_pairs, H]

            # Step 6: Process prompt representations to get relation type embeddings
            # Extract [REL] token representations (every other token in prompt)
            # The prompt format is: [REL] label1 [REL] label2 ... [SEP]
            # We need the representation at each [REL] position
            if hasattr(self.model, 'prompt_rep') and self.model.prompt_rep is not None:
                prompt_type_reps = self.model.prompt_rep(prompt_reps, num_classes_val)
            else:
                # Fallback: take every other position starting from 0
                prompt_type_reps = prompt_reps[:, ::2, :][:, :num_classes_val, :]

            # Step 7: Score using bilinear/dot product
            # rel_reps: [B, num_pairs, H]
            # prompt_type_reps: [B, num_labels, H]
            scores = torch.einsum("bph,blh->bpl", rel_reps, prompt_type_reps)

            return scores

    wrapper = GLiRELWrapper(model)
    wrapper.eval()

    # Create dummy inputs for tracing
    B, seq_len, num_entities, num_pairs, num_labels = 1, 128, 4, 12, 5
    dummy_inputs = (
        torch.randint(0, 1000, (B, seq_len), dtype=torch.long),     # input_ids
        torch.ones(B, seq_len, dtype=torch.long),                    # attention_mask
        torch.tensor([[[1, 3], [5, 7], [10, 12], [15, 17]]], dtype=torch.long),  # span_idx
        torch.ones(B, num_entities, dtype=torch.float),              # span_mask
        torch.tensor([[[0, 1], [0, 2], [0, 3], [1, 0], [1, 2], [1, 3],
                       [2, 0], [2, 1], [2, 3], [3, 0], [3, 1], [3, 2]]], dtype=torch.long),  # relations_idx
        torch.ones(B, num_labels, dtype=torch.float),                # rel_type_mask
        torch.tensor(num_labels, dtype=torch.long),                  # num_classes
        torch.tensor([20], dtype=torch.long),                        # prompt_lengths
    )

    input_names = [
        "input_ids",
        "attention_mask",
        "span_idx",
        "span_mask",
        "relations_idx",
        "rel_type_mask",
        "num_classes",
        "prompt_lengths",
    ]
    output_names = ["scores"]

    dynamic_axes = {
        "input_ids": {0: "batch", 1: "seq_len"},
        "attention_mask": {0: "batch", 1: "seq_len"},
        "span_idx": {0: "batch", 1: "num_entities"},
        "span_mask": {0: "batch", 1: "num_entities"},
        "relations_idx": {0: "batch", 1: "num_pairs"},
        "rel_type_mask": {0: "batch", 1: "num_labels"},
        "prompt_lengths": {0: "batch"},
        "scores": {0: "batch", 1: "num_pairs", 2: "num_labels"},
    }

    os.makedirs(args.output_dir, exist_ok=True)
    onnx_path = os.path.join(args.output_dir, "model.onnx")

    print(f"\nExporting to ONNX: {onnx_path}")
    print("This may take a few minutes...")

    try:
        # First try torch.onnx.export
        with torch.no_grad():
            torch.onnx.export(
                wrapper,
                dummy_inputs,
                onnx_path,
                input_names=input_names,
                output_names=output_names,
                dynamic_axes=dynamic_axes,
                opset_version=args.opset,
                do_constant_folding=True,
            )
        print(f"ONNX export successful: {onnx_path}")
    except Exception as e:
        print(f"\ntorch.onnx.export failed: {e}")
        print("\nThis likely means GLiREL's token_rep_layer uses operations that")
        print("aren't directly traceable. Attempting scripted export...")

        # Fallback: try torch.jit.trace first
        try:
            with torch.no_grad():
                traced = torch.jit.trace(wrapper, dummy_inputs)
                torch.onnx.export(
                    traced,
                    dummy_inputs,
                    onnx_path,
                    input_names=input_names,
                    output_names=output_names,
                    dynamic_axes=dynamic_axes,
                    opset_version=args.opset,
                )
            print(f"ONNX export (traced) successful: {onnx_path}")
        except Exception as e2:
            print(f"\nTraced export also failed: {e2}")
            print("\n--- MANUAL STEPS NEEDED ---")
            print("GLiREL's architecture may require manual decomposition.")
            print("The wrapper needs debugging with the actual model loaded.")
            print("Run this script interactively to inspect model internals:")
            print("  python -i export_glirel_onnx.py")
            sys.exit(1)

    # Verify with onnxruntime
    print("\nVerifying ONNX model with onnxruntime...")
    try:
        import onnxruntime as ort
        import numpy as np

        sess = ort.InferenceSession(onnx_path)

        print("ONNX model inputs:")
        for inp in sess.get_inputs():
            print(f"  {inp.name}: {inp.shape} ({inp.type})")
        print("ONNX model outputs:")
        for out in sess.get_outputs():
            print(f"  {out.name}: {out.shape} ({out.type})")

        # Run dummy inference
        feeds = {}
        for i, name in enumerate(input_names):
            arr = dummy_inputs[i].numpy()
            feeds[name] = arr

        result = sess.run(None, feeds)
        print(f"Output shape: {result[0].shape}")
        print("Verification passed!")
    except Exception as e:
        print(f"Verification warning: {e}")
        print("The ONNX file was created but runtime verification failed.")
        print("This may still work — check the error above.")

    # Copy tokenizer
    print("\nCopying tokenizer.json...")
    try:
        from transformers import AutoTokenizer
        tokenizer = AutoTokenizer.from_pretrained(args.model_id)
        tokenizer.save_pretrained(args.output_dir)
        print("Tokenizer saved.")
    except Exception as e:
        print(f"Warning: Could not save tokenizer via transformers: {e}")
        print("You may need to manually copy tokenizer.json from the HF cache.")

    # Save config
    config_path = os.path.join(args.output_dir, "glirel_config.json")
    import json
    config = {
        "model_type": "glirel",
        "base_model": args.model_id,
        "license": "CC-BY-NC-SA-4.0",
        "onnx_opset": args.opset,
        "inputs": input_names,
        "outputs": output_names,
        "notes": "Exported from PyTorch. Apply sigmoid to scores output for probabilities.",
    }
    with open(config_path, "w") as f:
        json.dump(config, f, indent=2)
    print(f"Config saved: {config_path}")

    print(f"\nDone! Output directory: {args.output_dir}")
    print("Contents:")
    for fname in sorted(os.listdir(args.output_dir)):
        fpath = os.path.join(args.output_dir, fname)
        size_mb = os.path.getsize(fpath) / (1024 * 1024)
        print(f"  {fname} ({size_mb:.1f} MB)")

    print(f"\nTo upload to HuggingFace:")
    print(f"  huggingface-cli upload dx111ge/glirel-large-v0-onnx {args.output_dir}")


if __name__ == "__main__":
    main()
