# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Celiums Solutions LLC
"""Score Hyphae's realizer output with the same metrics the
LLM+RAG comparator uses.

Hyphae's output is produced by `cargo run -p hyphae-eval --example
export_results` — see `crates/hyphae-eval/examples/export_results.rs`.
That JSON carries `(query_id, response, retrieved_chunks, latency_ms)`
for each query. This module applies the same Python scoring
functions used by `eval_runner` (n-gram overlap, NLI unsupported-
claim rate, verbatim, connective hygiene) so both columns of the
head-to-head table come from a single source of truth.

Run:
    uv run python -m baseline_llm_rag.score_hyphae \\
        --hyphae-output hyphae-results.json \\
        --output results/v0.1-laptop-hyphae.json
"""

from __future__ import annotations

import json
import logging
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
)

from . import __version__
from .eval_runner import NliPipeline, _aggregate, _hardware_metadata
from .metrics_extra import (
    has_doubled_connectives,
    ngram_overlap,
    quoted_content_supported_rate,
    unsupported_claim_rate,
    verbatim_pass,
)

console = Console(stderr=True)
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    stream=sys.stderr,
)
log = logging.getLogger("score-hyphae")


def _score_one_hyphae(item: dict[str, Any], nli: NliPipeline) -> dict[str, Any]:
    """Apply the comparable-subset scorers to one Hyphae output."""
    response = item["response"]
    retrieved = list(item["retrieved_chunks"])
    context = "\n".join(retrieved) if retrieved else ""

    v_pass = verbatim_pass(response, retrieved) if retrieved else True
    c_hygiene = not has_doubled_connectives(response)

    if context:
        overlap_4 = ngram_overlap(response, context, n=4)
        overlap_5 = ngram_overlap(response, context, n=5)
        overlap_8 = ngram_overlap(response, context, n=8)
        rate_filt, unsup_f, total_f = unsupported_claim_rate(
            response, context, nli, exclude_connectives=True
        )
        rate_raw, unsup_r, total_r = unsupported_claim_rate(
            response, context, nli, exclude_connectives=False
        )
    else:
        overlap_4 = overlap_5 = overlap_8 = None
        rate_filt = rate_raw = None
        unsup_f = total_f = unsup_r = total_r = 0

    q_rate, q_supported, q_total = quoted_content_supported_rate(response, retrieved)

    return {
        "query_id": item["query_id"],
        "response": response,
        "retrieved_chunks": retrieved,
        "retrieved_provenance": [],
        "metrics": {
            "verbatim_pass": v_pass,
            "connective_hygiene_pass": c_hygiene,
            "ngram_overlap_4": overlap_4,
            "ngram_overlap_5": overlap_5,
            "ngram_overlap_8": overlap_8,
            "unsupported_claim_rate_filtered": rate_filt,
            "unsupported_claim_rate_raw": rate_raw,
            "unsupported_claims_filtered": {"unsupported": unsup_f, "total_factual": total_f},
            "unsupported_claims_raw": {"unsupported": unsup_r, "total_factual": total_r},
            "quoted_content_supported_rate": q_rate,
            "quoted_content_counts": {"supported": q_supported, "total_quoted": q_total},
            # Hyphae latency stays as a float — sub-millisecond
            # precision matters for the head-to-head across three
            # orders of magnitude. See export_results.rs for the
            # rationale.
            "latency_ms": float(item["latency_ms"]),
        },
    }


@click.command()
@click.option(
    "--hyphae-output",
    type=click.Path(exists=True, dir_okay=False, path_type=Path),
    required=True,
    help="JSON emitted by `cargo run -p hyphae-eval --example export_results`.",
)
@click.option(
    "--output",
    type=click.Path(dir_okay=False, path_type=Path),
    required=True,
    help="Where to write the scored result envelope.",
)
def main(hyphae_output: Path, output: Path) -> None:
    """Score Hyphae's output against the same metrics the LLM
    comparator uses."""
    t_start = time.perf_counter()

    log.info("Loading Hyphae output from %s", hyphae_output)
    with hyphae_output.open("r", encoding="utf-8") as f:
        items = json.load(f)
    log.info("Loaded %d query outputs", len(items))

    nli = NliPipeline()

    per_query: list[dict[str, Any]] = []
    with Progress(
        SpinnerColumn(),
        TextColumn("[progress.description]{task.description}"),
        BarColumn(),
        TaskProgressColumn(),
        console=console,
    ) as progress:
        task = progress.add_task("scoring hyphae outputs", total=len(items))
        for item in items:
            per_query.append(_score_one_hyphae(item, nli))
            progress.update(task, advance=1)

    aggregate = _aggregate(per_query)
    wall_clock_s = time.perf_counter() - t_start

    envelope = {
        "metadata": {
            "comparator_version": __version__,
            "adr": "0027",
            "system": "hyphae",
            "mode": "native",  # Hyphae uses its own realizer, no retrieval LLM
            "nli": "roberta-large-mnli",
            "hardware": _hardware_metadata(),
            "wall_clock_s": round(wall_clock_s, 2),
            "hyphae_output_path": str(hyphae_output),
            "queries_run": len(per_query),
        },
        "per_query": per_query,
        "aggregate": aggregate,
        "caveats": [
            "Hyphae's latency is measured for the realizer pass only (composer + lexicon "
            "lookup + boundary smoothing). Substrate ingest + recall are measured separately "
            "in docs/perf/v0.2-baseline.md.",
            "verbatim_pass_rate = 1.0 here is the architectural commitment surfaced by the "
            "metric, not a result. ngram_overlap and unsupported_claim_rate are the "
            "informative measurements.",
        ],
    }

    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("w", encoding="utf-8") as f:
        json.dump(envelope, f, indent=2)
    log.info("Wrote %d-query Hyphae scored envelope to %s", len(per_query), output)
    console.print(f"[bold green]done[/] — {output}")


if __name__ == "__main__":
    main()
