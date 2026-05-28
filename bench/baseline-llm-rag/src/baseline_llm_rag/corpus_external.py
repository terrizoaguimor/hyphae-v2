# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Celiums Solutions LLC
"""Convert TriviaQA (rc) validation samples to our corpus JSON format.

Per ADR-0031 (planned): adds a *standard benchmark* column to the
multi-LLM matrix so reviewers cannot dismiss the head-to-head as
'evaluated only on the authors' own corpus'.

Mapping rules:
- TriviaQA `question` -> our `query`
- The first sentence of any wiki_context page containing the answer
  (or one of its aliases) becomes our seed body. Quotation of that
  sentence is the verbatim_quotation target Hyphae's realizer
  enforces.
- intent = Dialogue, schema = DialogueReply (factual-recall queries
  don't need the specialised schemas; ADR-0023/0024/0025 are
  Hyphae-specific anyway).
- Defaults: valence 0.0, confabulation_risk 0.1, from_cascade true.

Reject conditions:
- wiki_context has no pages.
- No sentence in any page contains the answer or any alias.
- The chosen sentence is shorter than 30 chars (probably a heading)
  or longer than 250 chars (probably a paragraph the realizer would
  struggle with at v0.1 scope).
"""

from __future__ import annotations

import json
import logging
import random
import re
from pathlib import Path

import click

log = logging.getLogger(__name__)


# Sentence boundary on `. ! ?` followed by whitespace. Handles
# Wikipedia text reasonably well.
_SENTENCE_RE = re.compile(r"(?<=[.!?])\s+")


def _split_sentences(text: str) -> list[str]:
    text = text.strip()
    if not text:
        return []
    parts = _SENTENCE_RE.split(text)
    return [p.strip() for p in parts if p.strip()]


def _find_seed_sentence(
    wiki_pages: list[str],
    answer: str,
    aliases: list[str],
    query: str,
    embedder,
    *,
    min_len: int = 30,
    max_len: int = 250,
) -> str | None:
    """Find the sentence in any wiki_page that contains the answer
    (or alias) AND is most semantically relevant to the query.

    Two-stage selection avoids false matches where the answer string
    appears as a substring inside an unrelated sentence:

    1. **Substring filter**: keep sentences that contain answer or
       any alias (case-insensitive, but word-bounded so '8%' inside
       '70.8%' is rejected).
    2. **Embedding rerank**: score remaining candidates by cosine
       similarity to the query. Pick the highest-scoring sentence —
       the one most likely to be the actual answer-bearing context.

    Returns None when no candidate survives the substring filter.
    """
    import re as _re
    import numpy as np

    targets = [t for t in [answer, *(aliases or [])] if t]
    # Word-bounded regex per target avoids the '8%' / '70.8%' false
    # match. Escape regex specials, then enforce non-alphanumeric (or
    # string boundary) on both sides of the match.
    patterns = [
        _re.compile(rf"(?:^|[^\w]){_re.escape(t)}(?:[^\w]|$)", _re.IGNORECASE)
        for t in targets
    ]

    candidates: list[str] = []
    for page in wiki_pages:
        for sentence in _split_sentences(page):
            if not (min_len <= len(sentence) <= max_len):
                continue
            if any(p.search(sentence) for p in patterns):
                candidates.append(sentence)

    if not candidates:
        return None
    if len(candidates) == 1:
        return candidates[0]

    # Embedding rerank: cosine similarity to query (embedder already
    # L2-normalises). Higher score wins.
    q_emb = embedder.encode([query])
    c_emb = embedder.encode(candidates)
    sims = (c_emb @ q_emb.T).flatten()
    best_idx = int(np.argmax(sims))
    return candidates[best_idx]


def convert(
    *,
    n: int,
    seed: int,
    out_path: Path,
    split: str = "validation",
) -> int:
    """Convert N TriviaQA samples to our corpus JSON. Returns number
    actually written (may be less than N if filters eliminate samples).
    """
    from datasets import load_dataset

    from .rag_pipeline import Embedder

    log.info("Loading TriviaQA rc:%s", split)
    ds = load_dataset("trivia_qa", "rc", split=split)
    log.info("Loaded %d samples", len(ds))

    log.info("Loading sentence embedder for relevance reranking")
    embedder = Embedder()

    # Reproducible random sampling.
    rng = random.Random(seed)
    indices = list(range(len(ds)))
    rng.shuffle(indices)

    written: list[dict] = []
    rejected_no_pages = 0
    rejected_no_sentence = 0

    for idx in indices:
        if len(written) >= n:
            break
        sample = ds[idx]
        pages = sample["entity_pages"].get("wiki_context", []) or []
        if not pages:
            rejected_no_pages += 1
            continue

        answer = sample["answer"]["value"]
        aliases = sample["answer"].get("aliases", []) or []
        query_text = sample["question"].strip()
        seed_body = _find_seed_sentence(pages, answer, aliases, query_text, embedder)
        if seed_body is None:
            rejected_no_sentence += 1
            continue

        query_obj = {
            "id": f"triviaqa-{sample['question_id']}",
            "query": sample["question"].strip(),
            "intent": "Dialogue",
            "seeds": [
                {
                    "body": seed_body,
                    "valence": 0.0,
                    "confabulation_risk": 0.1,
                    "from_cascade": True,
                    "domain_tags": [],
                }
            ],
            "expectations": {
                "schema": "DialogueReply",
                "must_fire": [],
                "must_not_fire": ["empty_working_set"],
                "acknowledgment_only": False,
                "verbatim_quotation": True,
            },
            # ADR-0031 traceability — provenance back to TriviaQA.
            "_source": {
                "dataset": "trivia_qa",
                "config": "rc",
                "split": split,
                "question_id": sample["question_id"],
                "answer_value": answer,
                "answer_aliases": aliases,
            },
        }
        written.append(query_obj)

    log.info(
        "Wrote %d queries; rejected no_pages=%d, no_sentence=%d",
        len(written),
        rejected_no_pages,
        rejected_no_sentence,
    )

    out_path.parent.mkdir(parents=True, exist_ok=True)
    with out_path.open("w", encoding="utf-8") as f:
        json.dump(written, f, indent=2, ensure_ascii=False)
    return len(written)


@click.command()
@click.option("--n", type=int, default=150, show_default=True, help="Target query count.")
@click.option("--seed", type=int, default=42, show_default=True, help="Sampling seed.")
@click.option(
    "--output",
    type=click.Path(dir_okay=False, path_type=Path),
    default=Path("corpus-triviaqa-150.json"),
    show_default=True,
)
@click.option("--split", type=str, default="validation", show_default=True)
def main(n: int, seed: int, output: Path, split: str) -> None:
    """Convert TriviaQA samples to our corpus JSON format."""
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
    written = convert(n=n, seed=seed, out_path=output, split=split)
    print(f"Wrote {written} queries to {output}")


if __name__ == "__main__":
    main()
