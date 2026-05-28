# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Celiums Solutions LLC
"""Re-score TriviaQA result JSONs with the gold-answer-match metric.

Per the v1 preprint review feedback: the NLI-grounded
`unsupported_claim_rate` measures grounding-in-context but not
correctness-against-gold. TriviaQA carries `answer.value` (+ aliases)
in the dataset; the corpus_external.py converter preserves that under
`_source.answer_value` / `_source.answer_aliases`. This module joins
the gold answer back onto each result JSON's per-query response and
computes a `gold_answer_match_rate` aggregate.

Run:
    uv run python -m baseline_llm_rag.rescore_gold_answer \\
        --corpus corpus-triviaqa-150.json \\
        --results-glob 'results/v0.1-laptop-triviaqa-*.json'

Modifies the result JSONs in place by adding the new metric to each
`per_query[*].metrics` block and the `aggregate` block.
"""

from __future__ import annotations

import glob
import json
import logging
import statistics
import sys
from pathlib import Path

import click

from .metrics_extra import bootstrap_ci, gold_answer_match

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(levelname)s %(message)s",
    stream=sys.stderr,
)
log = logging.getLogger("rescore-gold-answer")


@click.command()
@click.option(
    "--corpus",
    type=click.Path(exists=True, dir_okay=False, path_type=Path),
    required=True,
    help="Path to the corpus JSON with `_source.answer_value` per query.",
)
@click.option(
    "--results-glob",
    type=str,
    required=True,
    help="Glob pattern for result JSONs to re-score.",
)
def main(corpus: Path, results_glob: str) -> None:
    with corpus.open("r", encoding="utf-8") as f:
        queries = json.load(f)
    gold_by_id: dict[str, tuple[str, list[str]]] = {}
    for q in queries:
        src = q.get("_source") or {}
        ans = src.get("answer_value") or ""
        aliases = src.get("answer_aliases") or []
        gold_by_id[q["id"]] = (ans, list(aliases))
    log.info("Loaded gold answers for %d queries", len(gold_by_id))

    paths = sorted(glob.glob(results_glob))
    if not paths:
        raise click.ClickException(f"no result JSONs match: {results_glob}")
    log.info("Re-scoring %d result JSONs", len(paths))

    for path_str in paths:
        path = Path(path_str)
        with path.open("r", encoding="utf-8") as f:
            envelope = json.load(f)

        matches: list[bool] = []
        for pq in envelope["per_query"]:
            qid = pq["query_id"]
            ans, aliases = gold_by_id.get(qid, ("", []))
            hit = gold_answer_match(pq["response"], ans, aliases)
            pq.setdefault("metrics", {})["gold_answer_match"] = hit
            matches.append(hit)

        if matches:
            rate = sum(matches) / len(matches)
            # Bootstrap CI on the binary outcome.
            ci_lo, ci_hi = bootstrap_ci([float(m) for m in matches])
            envelope.setdefault("aggregate", {})
            envelope["aggregate"]["gold_answer_match_rate_mean"] = rate
            envelope["aggregate"]["gold_answer_match_rate_ci95"] = [ci_lo, ci_hi]
            envelope["aggregate"]["gold_answer_match_rate_n"] = len(matches)
            log.info("  %s -> %.3f (%d/%d)", path.name, rate, sum(matches), len(matches))
        else:
            log.warning("  %s -> no per_query entries", path.name)

        with path.open("w", encoding="utf-8") as f:
            json.dump(envelope, f, indent=2, ensure_ascii=False)


if __name__ == "__main__":
    main()
