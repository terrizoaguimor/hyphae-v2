# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Celiums Solutions LLC
"""Echo / identity baseline -- the trivial control.

Per the v2.3 review: Hyphae quotes the seed sentence verbatim; in
oracle mode the LLM receives the same sentence. The natural control
is `print(retrieved_sentence)` -- emit the retrieved seed body with
no model, no lexicon, no connective scaffolding, no composition.
This baseline isolates what Hyphae's realizer adds *on top of*
verbatim echo (connective tissue, multi-fragment composition,
hash-chained provenance) from what is already guaranteed by
quoting verbatim at all.

The echo response for a query is the concatenation of its seed
bodies, verbatim, with a single space separator and nothing else.
Output envelope matches export_results.rs so the same scoring
pipeline (score_hyphae + rescore_gold_answer) applies unchanged.

Run:
    uv run python -m baseline_llm_rag.echo_baseline \\
        --corpus corpus-triviaqa-150.json \\
        --output echo-results-triviaqa.json
"""

from __future__ import annotations

import json
from pathlib import Path

import click


@click.command()
@click.option(
    "--corpus",
    type=click.Path(exists=True, dir_okay=False, path_type=Path),
    required=True,
)
@click.option(
    "--output",
    type=click.Path(dir_okay=False, path_type=Path),
    required=True,
)
def main(corpus: Path, output: Path) -> None:
    """Emit the verbatim-echo baseline output for a corpus."""
    with corpus.open("r", encoding="utf-8") as f:
        queries = json.load(f)

    out = []
    for q in queries:
        seed_bodies = [s["body"] for s in q.get("seeds", [])]
        # Echo: emit the seed bodies verbatim, space-joined, nothing
        # else. No quotes, no connectives, no "Drawing from working
        # memory". Just the retrieved text.
        response = " ".join(seed_bodies)
        out.append(
            {
                "query_id": q["id"],
                "response": response,
                "retrieved_chunks": seed_bodies,
                # Echo is a string concatenation -- microsecond-floor
                # latency like Hyphae, reported as ~0 for honesty (no
                # real measurement of a no-op).
                "latency_ms": 0.001,
            }
        )

    with output.open("w", encoding="utf-8") as f:
        json.dump(out, f, indent=2, ensure_ascii=False)
    print(f"Wrote {len(out)} echo responses to {output}")


if __name__ == "__main__":
    main()
