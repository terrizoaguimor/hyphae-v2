# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Celiums Solutions LLC
"""Orchestrate the LLM+RAG comparator run.

Reads the exported corpus, runs the RAG pipeline over every query
(both `oracle` and `rag` modes are exposed via `--mode`), scores the
output with the comparable-subset scorers plus the two extra
metrics (`ngram_overlap`, `unsupported_claim_rate`), aggregates with
bootstrap CIs, and emits the JSON envelope documented in
`results/README.md`.

See ADR-0027 for design rationale.
"""

from __future__ import annotations

import json
import logging
import os
import platform
import statistics
import sys
import time
from pathlib import Path
from typing import Any

import click
import torch
from rich.console import Console
from rich.progress import (
    BarColumn,
    Progress,
    SpinnerColumn,
    TaskProgressColumn,
    TextColumn,
    TimeElapsedColumn,
)
from transformers import AutoModelForSequenceClassification, AutoTokenizer

from . import __version__
from .corpus_loader import EvalQuery, corpus_sha256, load_corpus
from .metrics_extra import (
    bootstrap_ci,
    has_doubled_connectives,
    ngram_overlap,
    quoted_content_supported_rate,
    unsupported_claim_rate,
    verbatim_pass,
)
from .rag_pipeline import (
    DEFAULT_DECODING,
    DEFAULT_SYSTEM_PROMPT,
    EMBED_MODEL,
    RETRIEVAL_K,
    Embedder,
    LlamaGenerator,
    Mode,
    RagPipeline,
    RagResponse,
)

console = Console(stderr=True)

# Logging goes to stderr; stdout is reserved for the JSON envelope
# when the runner is invoked with no --output (UNIX-style streaming).
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    stream=sys.stderr,
)
log = logging.getLogger("baseline-llm-rag")


# ── NLI classifier (roberta-large-mnli) ──────────────────────


_NLI_MODEL = "roberta-large-mnli"
# Label order in roberta-large-mnli output logits
_NLI_LABELS = ("contradiction", "neutral", "entailment")


class NliPipeline:
    """Wraps a HuggingFace roberta-large-mnli model. Implements the
    `NliClassifier` protocol from `metrics_extra`."""

    def __init__(self, model_name: str = _NLI_MODEL) -> None:
        log.info("Loading NLI model %s", model_name)
        self.tokenizer = AutoTokenizer.from_pretrained(model_name)
        self.model = AutoModelForSequenceClassification.from_pretrained(model_name)
        self.model.eval()
        if torch.cuda.is_available():
            self.model = self.model.to("cuda")
            self.device = "cuda"
        elif torch.backends.mps.is_available():
            self.model = self.model.to("mps")
            self.device = "mps"
        else:
            self.device = "cpu"
        self.model_name = model_name
        log.info("NLI model loaded on %s", self.device)

    @torch.inference_mode()
    def __call__(self, premise: str, hypothesis: str) -> tuple[str, float]:
        inputs = self.tokenizer(
            premise,
            hypothesis,
            return_tensors="pt",
            truncation=True,
            max_length=512,
        )
        if self.device != "cpu":
            inputs = {k: v.to(self.device) for k, v in inputs.items()}
        logits = self.model(**inputs).logits.squeeze(0)
        probs = torch.softmax(logits, dim=-1)
        idx = int(torch.argmax(probs).item())
        return _NLI_LABELS[idx], float(probs[idx].item())


# ── Per-query scoring ────────────────────────────────────────


def _score_one(
    query: EvalQuery,
    rag_out: RagResponse,
    nli: NliPipeline,
) -> dict[str, Any]:
    """Run the comparable-subset scorers + extra metrics against one
    pipeline output."""
    seed_bodies = [s.body for s in query.seeds]
    context = "\n".join(rag_out.retrieved_chunks) if rag_out.retrieved_chunks else ""

    # Comparable-subset scorers (ports of Hyphae harness).
    v_pass = verbatim_pass(rag_out.response, seed_bodies) if seed_bodies else True
    c_hygiene = not has_doubled_connectives(rag_out.response)

    # n-gram overlap at n=4/5/8.
    overlap_4 = ngram_overlap(rag_out.response, context, n=4) if context else None
    overlap_5 = ngram_overlap(rag_out.response, context, n=5) if context else None
    overlap_8 = ngram_overlap(rag_out.response, context, n=8) if context else None

    # NLI unsupported-claim rate — both filtered and raw.
    if context:
        rate_filtered, unsup_f, total_f = unsupported_claim_rate(
            rag_out.response, context, nli, exclude_connectives=True
        )
        rate_raw, unsup_r, total_r = unsupported_claim_rate(
            rag_out.response, context, nli, exclude_connectives=False
        )
    else:
        rate_filtered = rate_raw = None
        unsup_f = total_f = unsup_r = total_r = 0

    # Quoted-content support rate — architecturally diagnostic.
    # `None` when the response has no quoted spans (typical LLM
    # output); the aggregator skips Nones.
    q_rate, q_supported, q_total = quoted_content_supported_rate(
        rag_out.response, list(rag_out.retrieved_chunks)
    )

    return {
        "query_id": query.id,
        "response": rag_out.response,
        "retrieved_chunks": list(rag_out.retrieved_chunks),
        "retrieved_provenance": [list(p) for p in rag_out.retrieved_provenance],
        "metrics": {
            "verbatim_pass": v_pass,
            "connective_hygiene_pass": c_hygiene,
            "ngram_overlap_4": overlap_4,
            "ngram_overlap_5": overlap_5,
            "ngram_overlap_8": overlap_8,
            "unsupported_claim_rate_filtered": rate_filtered,
            "unsupported_claim_rate_raw": rate_raw,
            "unsupported_claims_filtered": {"unsupported": unsup_f, "total_factual": total_f},
            "unsupported_claims_raw": {"unsupported": unsup_r, "total_factual": total_r},
            "quoted_content_supported_rate": q_rate,
            "quoted_content_counts": {"supported": q_supported, "total_quoted": q_total},
            "latency_ms": rag_out.latency_ms,
        },
    }


# ── Aggregation ──────────────────────────────────────────────


def _mean_with_ci(values: list[float], key: str) -> dict[str, Any]:
    """Compute mean + bootstrap CI for a per-query metric, skipping
    Nones (queries where the metric was undefined)."""
    clean = [v for v in values if v is not None]
    if not clean:
        return {f"{key}_mean": None, f"{key}_ci95": None, f"{key}_n": 0}
    mean = statistics.fmean(clean)
    lo, hi = bootstrap_ci(clean)
    return {f"{key}_mean": mean, f"{key}_ci95": [lo, hi], f"{key}_n": len(clean)}


def _aggregate(per_query: list[dict[str, Any]]) -> dict[str, Any]:
    n = len(per_query)
    if n == 0:
        return {"queries": 0}

    metrics = [pq["metrics"] for pq in per_query]
    verbatim_rate = sum(1 for m in metrics if m["verbatim_pass"]) / n
    hygiene_rate = sum(1 for m in metrics if m["connective_hygiene_pass"]) / n

    overlap_4 = [m["ngram_overlap_4"] for m in metrics]
    overlap_5 = [m["ngram_overlap_5"] for m in metrics]
    overlap_8 = [m["ngram_overlap_8"] for m in metrics]
    unsup_f = [m["unsupported_claim_rate_filtered"] for m in metrics]
    unsup_r = [m["unsupported_claim_rate_raw"] for m in metrics]
    quoted = [m["quoted_content_supported_rate"] for m in metrics]

    latencies = [m["latency_ms"] for m in metrics]
    latencies_sorted = sorted(latencies)
    p50 = latencies_sorted[len(latencies_sorted) // 2]
    p95_idx = max(0, int(len(latencies_sorted) * 0.95) - 1)
    p95 = latencies_sorted[p95_idx]

    agg: dict[str, Any] = {
        "queries": n,
        "verbatim_pass_rate": verbatim_rate,
        "connective_hygiene_pass_rate": hygiene_rate,
        "latency_p50_ms": p50,
        "latency_p95_ms": p95,
        "latency_mean_ms": statistics.fmean(latencies),
    }
    agg.update(_mean_with_ci(overlap_4, "ngram_overlap_4"))
    agg.update(_mean_with_ci(overlap_5, "ngram_overlap_5"))
    agg.update(_mean_with_ci(overlap_8, "ngram_overlap_8"))
    agg.update(_mean_with_ci(unsup_f, "unsupported_claim_rate_filtered"))
    agg.update(_mean_with_ci(unsup_r, "unsupported_claim_rate_raw"))
    agg.update(_mean_with_ci(quoted, "quoted_content_supported_rate"))
    return agg


# ── Hardware metadata ────────────────────────────────────────


def _hardware_metadata() -> dict[str, Any]:
    """Capture enough to reproduce the run. Intentionally minimal —
    `platform` standard library only; psutil is not in the dep set."""
    return {
        "os": f"{platform.system()} {platform.release()}",
        "machine": platform.machine(),
        "processor": platform.processor(),
        "python_version": platform.python_version(),
        "cpu_count": os.cpu_count(),
        "torch_device_available": {
            "cuda": torch.cuda.is_available(),
            "mps": torch.backends.mps.is_available(),
        },
    }


# ── CLI ─────────────────────────────────────────────────────


@click.command()
@click.option(
    "--mode",
    type=click.Choice(["oracle", "rag"]),
    required=True,
    help="oracle: LLM sees corpus seeds directly. rag: full FAISS retrieval.",
)
@click.option(
    "--corpus",
    type=click.Path(exists=True, dir_okay=False, path_type=Path),
    required=True,
    help="Path to corpus JSON exported by `hyphae-eval --example export_corpus`.",
)
@click.option(
    "--output",
    type=click.Path(dir_okay=False, path_type=Path),
    required=True,
    help="Where to write the result JSON.",
)
@click.option(
    "--model-path",
    type=click.Path(exists=True, dir_okay=False, path_type=Path),
    default=Path("models/Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf"),
    show_default=True,
    help="Path to the Llama GGUF file (downloaded by scripts/download-model.sh).",
)
@click.option(
    "--retrieval-k",
    type=int,
    default=RETRIEVAL_K,
    show_default=True,
    help="Top-k for FAISS retrieval (rag mode only).",
)
@click.option(
    "--limit",
    type=int,
    default=None,
    help="Optional cap on queries for smoke testing.",
)
def main(
    mode: Mode,
    corpus: Path,
    output: Path,
    model_path: Path,
    retrieval_k: int,
    limit: int | None,
) -> None:
    """Run the LLM+RAG comparator end-to-end."""
    t_start = time.perf_counter()

    log.info("Loading corpus from %s", corpus)
    queries = load_corpus(corpus)
    if limit is not None:
        queries = queries[:limit]
    log.info("Loaded %d queries", len(queries))

    nli = NliPipeline()
    generator = LlamaGenerator(model_path)
    pipeline = RagPipeline(
        mode=mode,
        generator=generator,
        retrieval_k=retrieval_k,
    )
    if mode == "rag":
        pipeline.build_index(queries)

    per_query: list[dict[str, Any]] = []
    with Progress(
        SpinnerColumn(),
        TextColumn("[progress.description]{task.description}"),
        BarColumn(),
        TaskProgressColumn(),
        TimeElapsedColumn(),
        console=console,
    ) as progress:
        task = progress.add_task(f"running {mode} mode", total=len(queries))
        for q in queries:
            rag_out = pipeline.run_query(q)
            scored = _score_one(q, rag_out, nli)
            per_query.append(scored)
            progress.update(task, advance=1)

    aggregate = _aggregate(per_query)
    wall_clock_s = time.perf_counter() - t_start

    envelope = {
        "metadata": {
            "comparator_version": __version__,
            "adr": "0027",
            "mode": mode,
            "model": {
                "repo": "bartowski/Meta-Llama-3.1-8B-Instruct-GGUF",
                "file": model_path.name,
                "sha256": _file_sha256(model_path),
            },
            "embedder": EMBED_MODEL,
            "nli": _NLI_MODEL,
            "vector_index": "faiss.IndexFlatIP",
            "chunk_size": 256,
            "chunk_overlap": 32,
            "retrieval_k": retrieval_k if mode == "rag" else None,
            "decoding": dict(DEFAULT_DECODING),
            "system_prompt": DEFAULT_SYSTEM_PROMPT,
            "hardware": _hardware_metadata(),
            "wall_clock_s": round(wall_clock_s, 2),
            "corpus_path": str(corpus),
            "corpus_sha256": corpus_sha256(corpus),
            "queries_run": len(per_query),
        },
        "per_query": per_query,
        "aggregate": aggregate,
        "caveats": [
            "ADR-0027: only verbatim_pass + connective_hygiene from the Hyphae 9-dimension "
            "scorer are directly comparable across systems. Schema fidelity, limitation "
            "recall/precision, lexical diversity, role coverage, boundary smoothness are "
            "Hyphae-specific by design — see ADR-0027 §'Comparable subset' before reading "
            "the writeup.",
            "Unsupported-claim rate is reported BOTH filtered (excluding Hyphae connective "
            "sentences) and raw. The filter benefits Hyphae; consult both columns.",
            f"N=1 hardware ({_hardware_metadata()['processor']}). Generalisation to other "
            "hardware classes is a separate ADR (planned: 0028).",
            "No ablation studies in this run — every Hyphae component is enabled. ADR-0029 "
            "(planned) covers the ablation harness.",
        ],
    }

    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("w", encoding="utf-8") as f:
        json.dump(envelope, f, indent=2)
    log.info("Wrote %d-query result envelope to %s", len(per_query), output)
    console.print(f"[bold green]done[/] — {output}")


def _file_sha256(path: Path) -> str:
    import hashlib

    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


if __name__ == "__main__":
    main()
