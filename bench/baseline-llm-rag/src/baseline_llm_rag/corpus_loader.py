# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Celiums Solutions LLC
"""Load the EN eval corpus exported from `crates/hyphae-eval`.

The corpus is the single source of truth defined by
`seed_corpus_en()` in `crates/hyphae-eval/src/corpus.rs`. The Rust
binary `cargo run -p hyphae-eval --example export_corpus` emits it
as JSON; this module reads that JSON. Never re-implement the corpus
in Python — drift between two definitions is exactly the failure
mode this loader exists to prevent.

See ADR-0027 §"Same corpus, same input semantics".
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class EvalSeed:
    """One seed memory — a fragment the substrate would have retrieved.

    Fields mirror `hyphae_eval::corpus::EvalSeed`. `body` is the
    verbatim text the realizer must quote; the comparator measures
    whether the LLM reproduces it (it usually does not).
    """

    body: str
    valence: float
    confabulation_risk: float
    from_cascade: bool
    domain_tags: tuple[str, ...]


@dataclass(frozen=True)
class Expectations:
    """Expected harness observations — mirror of the Rust struct.

    `schema` is a string identifier; the comparator does not score
    it (see ADR-0027 §"Comparable subset"). `must_fire` and
    `must_not_fire` are noted for diagnostic context only.
    """

    schema: str
    must_fire: tuple[str, ...]
    must_not_fire: tuple[str, ...]
    acknowledgment_only: bool
    verbatim_quotation: bool


@dataclass(frozen=True)
class EvalQuery:
    """One evaluation query."""

    id: str
    query: str
    intent: str
    seeds: tuple[EvalSeed, ...]
    expectations: Expectations


def load_corpus(path: str | Path) -> list[EvalQuery]:
    """Load the exported corpus JSON into a list of `EvalQuery`.

    Raises FileNotFoundError if the JSON is missing. The caller
    should run `cargo run -p hyphae-eval --example export_corpus
    > corpus-en.json` first.
    """
    path = Path(path)
    with path.open("r", encoding="utf-8") as f:
        raw = json.load(f)

    queries: list[EvalQuery] = []
    for item in raw:
        seeds = tuple(
            EvalSeed(
                body=s["body"],
                valence=float(s.get("valence", 0.0)),
                confabulation_risk=float(s.get("confabulation_risk", 0.0)),
                from_cascade=bool(s.get("from_cascade", False)),
                domain_tags=tuple(s.get("domain_tags", [])),
            )
            for s in item["seeds"]
        )
        exp = item["expectations"]
        expectations = Expectations(
            schema=str(exp["schema"]),
            must_fire=tuple(str(t) for t in exp.get("must_fire", [])),
            must_not_fire=tuple(str(t) for t in exp.get("must_not_fire", [])),
            acknowledgment_only=bool(exp.get("acknowledgment_only", False)),
            verbatim_quotation=bool(exp.get("verbatim_quotation", True)),
        )
        queries.append(
            EvalQuery(
                id=str(item["id"]),
                query=str(item["query"]),
                intent=str(item["intent"]),
                seeds=seeds,
                expectations=expectations,
            )
        )

    if not queries:
        raise ValueError(f"corpus file {path} contains no queries")

    return queries


def corpus_sha256(path: str | Path) -> str:
    """Hex SHA-256 of the corpus file — stored in the result metadata
    so the run is bound to an exact corpus snapshot."""
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def pooled_seed_bodies(queries: list[EvalQuery]) -> list[tuple[str, str]]:
    """Flatten the corpus into `(query_id, body)` pairs for the RAG
    mode's global FAISS index.

    Used by `rag_pipeline.RagPipeline.build_index` in mode `rag`. In
    `oracle` mode each query's seeds are used directly and this
    helper is not called.
    """
    out: list[tuple[str, str]] = []
    for q in queries:
        for s in q.seeds:
            out.append((q.id, s.body))
    return out
