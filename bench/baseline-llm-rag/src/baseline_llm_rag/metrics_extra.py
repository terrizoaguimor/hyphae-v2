# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Celiums Solutions LLC
"""Extra metrics that diferentiate the two architectures.

Two metrics are introduced here and described in ADR-0027
§"Two extra metrics":

  1. `ngram_overlap(response, context, n)` — fraction of n-grams in
     the response that appear in the retrieved context.
  2. `unsupported_claim_rate(response, context, nli)` — NLI-based
     fraction of factual sentences in the response not entailed by
     the context.

Plus three small helpers that the comparator needs and that mirror
the Hyphae harness scorers verbatim where applicable:

  3. `verbatim_pass(response, seeds)` — port of
     `hyphae_eval::scorers::verbatim_pass` (3 lines; identical
     semantics).
  4. `has_doubled_connectives(text)` — port of
     `hyphae_eval::scorers::has_doubled_connectives` (loop over the
     same `DOUBLED_CHECK_PHRASES` list).
  5. `is_connective_sentence(s)` — heuristic that excludes Hyphae's
     connective tissue ("Drawing from working memory,", "Therefore,")
     from the unsupported-claim denominator. Documented as a
     deliberate heuristic that benefits Hyphae; ADR-0027 records that
     the writeup reports BOTH with-heuristic and without-heuristic
     rates for honesty.
"""

from __future__ import annotations

import re
from collections.abc import Sequence
from typing import Protocol


# ── Verbatim ──────────────────────────────────────────────────

def verbatim_pass(response: str, seed_bodies: Sequence[str]) -> bool:
    """Port of `hyphae_eval::scorers::verbatim_pass`.

    Returns True iff every seed body appears verbatim in the
    response. Hyphae returns True by construction; the LLM baseline
    generally does not because it paraphrases.
    """
    return all(body in response for body in seed_bodies)


# ── Connective hygiene (Hyphae scorer parity) ─────────────────

# Ported from crates/hyphae-eval/src/scorers.rs::DOUBLED_CHECK_PHRASES.
# Keep in sync if the Rust constant changes.
DOUBLED_CHECK_PHRASES: tuple[str, ...] = (
    "however,",
    "by contrast,",
    "on the other hand,",
    "extending that,",
    "building on it,",
    "likewise,",
    "drawing from working memory,",
    "the source states:",
    "per the recorded material:",
)


def has_doubled_connectives(text: str) -> bool:
    """Port of `hyphae_eval::scorers::has_doubled_connectives`.

    Catches "However, However,"-style stutters and the multi-word
    variants the v1 single-token scorer missed.
    """
    lower = text.lower()
    return any(f"{p} {p}" in lower for p in DOUBLED_CHECK_PHRASES)


# ── n-gram overlap ────────────────────────────────────────────

_WS_RE = re.compile(r"\s+")
_PUNCT_RE = re.compile(r"[^\w\s]")


def _normalise_tokens(text: str) -> list[str]:
    """Lowercase, strip punctuation, split on whitespace.

    The metric measures token-level fidelity; stop-words are NOT
    filtered (per ADR-0027 §"ngram_overlap definition"). Punctuation
    is stripped so that "deploy." and "deploy" match.
    """
    text = text.lower()
    text = _PUNCT_RE.sub(" ", text)
    text = _WS_RE.sub(" ", text).strip()
    return text.split(" ") if text else []


def _ngrams(tokens: Sequence[str], n: int) -> set[tuple[str, ...]]:
    if n <= 0 or len(tokens) < n:
        return set()
    return {tuple(tokens[i : i + n]) for i in range(len(tokens) - n + 1)}


def ngram_overlap(response: str, context: str, n: int) -> float:
    """Fraction of n-grams in `response` that also appear in `context`.

    Returns 1.0 when the response has no n-grams of length n
    (denominator zero — degenerate; the comparator's empty response
    handling treats this as not-meaningful and the writeup excludes
    it from aggregate means).

    Hyphae's seed-body output is expected to score 1.0 on the seed
    portion; its connective tissue ("Drawing from working memory,")
    contributes mid-range overlap, dragging the per-response score
    down. The comparator reports both per-response and seed-only
    overlap for honesty.
    """
    resp_tokens = _normalise_tokens(response)
    ctx_tokens = _normalise_tokens(context)
    resp_ngrams = _ngrams(resp_tokens, n)
    if not resp_ngrams:
        return 1.0
    ctx_ngrams = _ngrams(ctx_tokens, n)
    overlap = len(resp_ngrams & ctx_ngrams)
    return overlap / len(resp_ngrams)


# ── NLI-based unsupported-claim rate ──────────────────────────


class NliClassifier(Protocol):
    """Minimal contract the runner relies on. The real implementation
    is a transformers pipeline wrapping `roberta-large-mnli`. The
    Protocol lets tests pass a stub that returns canned labels."""

    def __call__(
        self, premise: str, hypothesis: str
    ) -> tuple[str, float]:  # ("entailment"|"neutral"|"contradiction", score)
        ...


_SENTENCE_BOUNDARY_RE = re.compile(r"(?<=[.!?])\s+")


def _split_sentences(text: str) -> list[str]:
    """Naive sentence split on `.`/`!`/`?` followed by whitespace.

    Good enough for the corpus's short, well-formed outputs. A more
    robust splitter (spaCy, NLTK) is an unnecessary dependency for
    this comparator; if the corpus grows to multi-paragraph
    technical text the runner switches to one of those.
    """
    text = text.strip()
    if not text:
        return []
    parts = _SENTENCE_BOUNDARY_RE.split(text)
    return [p.strip() for p in parts if p.strip()]


# Heuristic: a sentence "is connective" if it starts with one of
# Hyphae's known connective phrases. Documented in ADR-0027 as a
# choice that benefits Hyphae; the writeup reports unsupported-claim
# rate BOTH with and without this filter.
_CONNECTIVE_PREFIXES: tuple[str, ...] = (
    "drawing from working memory,",
    "the source states:",
    "per the recorded material:",
    "therefore,",
    "however,",
    "by contrast,",
    "on the other hand,",
    "likewise,",
    "extending that,",
    "building on it,",
    "overall,",
    "on balance,",
    "taking it together,",
)


def is_connective_sentence(sentence: str) -> bool:
    """True if the sentence starts with a known Hyphae connective.

    Used to exclude composition-glue sentences from the
    unsupported-claim denominator. They are not factual claims; NLI
    will rate them `neutral` against the context, which would
    spuriously inflate Hyphae's unsupported-claim rate without this
    filter. The writeup publishes both filtered and unfiltered
    rates so reviewers can see the heuristic's effect.
    """
    lower = sentence.lower().lstrip()
    return any(lower.startswith(p) for p in _CONNECTIVE_PREFIXES)


def unsupported_claim_rate(
    response: str,
    context: str,
    nli: NliClassifier,
    *,
    exclude_connectives: bool = True,
) -> tuple[float, int, int]:
    """NLI-based unsupported-claim rate.

    Returns `(rate, n_unsupported, n_total_factual)`.

    `n_total_factual` is the denominator: sentences in the response
    that are treated as factual claims (i.e., not excluded by the
    `is_connective_sentence` filter when `exclude_connectives` is
    True).

    For each factual sentence S, NLI scores `(context → S)`. Labels
    `neutral` and `contradiction` count as unsupported. The
    comparator runs this twice — once with `exclude_connectives=True`
    (filtered) and once with `=False` (raw) — and reports both in
    the result JSON. Reviewers can decide which to read.
    """
    sentences = _split_sentences(response)
    factual = [s for s in sentences if not (exclude_connectives and is_connective_sentence(s))]
    if not factual:
        return 0.0, 0, 0
    unsupported = 0
    for s in factual:
        label, _score = nli(premise=context, hypothesis=s)
        if label != "entailment":
            unsupported += 1
    return unsupported / len(factual), unsupported, len(factual)


# ── Bootstrap CI helper ───────────────────────────────────────


def bootstrap_ci(values: Sequence[float], *, confidence: float = 0.95, n_resamples: int = 1000, seed: int = 42) -> tuple[float, float]:
    """Bootstrap percentile confidence interval.

    Used by the aggregator to report `(mean, ci_low, ci_high)` for
    each metric. Deterministic given the seed.
    """
    import random
    import statistics

    if not values:
        return (0.0, 0.0)
    if len(values) == 1:
        return (values[0], values[0])

    rng = random.Random(seed)
    n = len(values)
    means: list[float] = []
    for _ in range(n_resamples):
        sample = [values[rng.randrange(n)] for _ in range(n)]
        means.append(statistics.fmean(sample))
    means.sort()
    alpha = (1.0 - confidence) / 2.0
    lo_idx = int(alpha * n_resamples)
    hi_idx = int((1.0 - alpha) * n_resamples) - 1
    return (means[lo_idx], means[hi_idx])
