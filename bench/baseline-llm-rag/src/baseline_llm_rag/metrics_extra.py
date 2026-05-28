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

# Hyphae's compositional output joins verbatim quotes with inter-fragment
# connective phrases that do NOT terminate with a period — e.g.
#     Per the recorded fragments, "X" Per the next fragment, "Y" That is the substrate's current view.
# A naive period-split treats that entire string as ONE sentence, and the
# NLI then judges a multi-quote composition that no single entailment
# model is calibrated for. We split on the same inter-fragment phrases
# Hyphae's lexicon emits so each verbatim quote becomes its own
# "sentence" for the NLI denominator. This is documented behaviour, not
# a hidden adjustment.
_INTER_FRAGMENT_SPLIT_RE = re.compile(
    r"\s+(?="
    r"Per the recorded fragments?,|"
    r"Per the next fragment,|"
    r"Per the recorded material:|"
    r"The source states:|"
    r"Drawing from working memory,|"
    r"Following this,|"
    r"Additionally,|"
    r"Furthermore,|"
    r"Extending that,|"
    r"Building on it,|"
    r"Likewise,|"
    r"By contrast,|"
    r"However,|"
    r"On the other hand,|"
    r"That is the substrate's current view|"
    r"Overall,|"
    r"On balance,|"
    r"Taking it together,|"
    r"Therefore,"
    r")"
)


def _split_sentences(text: str) -> list[str]:
    """Split a response into sentence-like units for NLI scoring.

    Two passes:
      1. Standard period/!/? split.
      2. Hyphae-aware inter-fragment split — a verbatim-quotation
         composition does not place a period between adjacent quotes,
         only a connective phrase. Without this pass the entire
         response collapses to one "sentence" and the NLI loses its
         denominator entirely.

    The Hyphae-aware pass is purely additive — it never merges two
    sentences a standard splitter would have separated. Applying it
    to an LLM response that does not use Hyphae's connective vocabulary
    is a no-op.
    """
    text = text.strip()
    if not text:
        return []
    coarse = _SENTENCE_BOUNDARY_RE.split(text)
    fine: list[str] = []
    for part in coarse:
        part = part.strip()
        if not part:
            continue
        fine.extend(p.strip() for p in _INTER_FRAGMENT_SPLIT_RE.split(part) if p.strip())
    return fine


# Heuristic: a sentence "is connective" if it starts with one of
# Hyphae's known connective phrases. Documented in ADR-0027 as a
# choice that benefits Hyphae; the writeup reports unsupported-claim
# rate BOTH with and without this filter.
#
# The list extends `DOUBLED_CHECK_PHRASES` with the lexicon phrases
# Hyphae actually emits in its compositions (Per the recorded
# fragments, / Per the next fragment, / That is the substrate's
# current view, / Following this, / Additionally, / Furthermore,).
# Without these the filter under-counts and the LLM appears
# artificially better on Hyphae's compositional output.
_CONNECTIVE_PREFIXES: tuple[str, ...] = (
    "drawing from working memory,",
    "the source states:",
    "per the recorded material:",
    "per the recorded fragments,",
    "per the recorded fragment,",
    "per the next fragment,",
    "that is the substrate's current view",
    "following this,",
    "additionally,",
    "furthermore,",
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


# ── Quoted-content support rate ──────────────────────────────


_QUOTED_RE = re.compile(r'"([^"]+)"')


def extract_quoted_strings(text: str) -> list[str]:
    """Return all double-quoted substrings of `text`.

    Hyphae's compositions surface seed bodies inside `"..."` (the
    realizer's verbatim-quotation contract). This extractor enables
    a metric the LLM baseline almost never triggers — the rate at
    which a system's quoted content matches the retrieved context.
    """
    return _QUOTED_RE.findall(text)


def quoted_content_supported_rate(
    response: str, retrieved_chunks: Sequence[str]
) -> tuple[float | None, int, int]:
    """Of the quoted spans in `response`, what fraction appears
    verbatim in at least one retrieved chunk?

    Returns `(rate_or_None, supported, total_quoted)`. `None` rate
    when the response has no quoted spans — applicable to most LLM
    outputs (the baseline almost never uses formal quotation),
    making this metric architecturally diagnostic rather than
    universal.

    Hyphae's expected rate is 1.0 by construction. The LLM's rate is
    typically `None` (no quotes) or near zero (when it does, the
    quotes are paraphrased).
    """
    quoted = extract_quoted_strings(response)
    if not quoted:
        return None, 0, 0
    chunks_concat = "\n".join(retrieved_chunks)
    supported = sum(1 for q in quoted if q in chunks_concat)
    return supported / len(quoted), supported, len(quoted)


# ── Gold-answer match ─────────────────────────────────────────


def gold_answer_match(response: str, answer: str, aliases: Sequence[str] = ()) -> bool:
    """Does the response contain the gold answer (or any alias) as a
    word-bounded match?

    Word-bounded to avoid `'5'` inside `'1958'`-style false positives.
    Case-insensitive — gold answers in TriviaQA are stored mixed-case.

    This is the *correctness* axis the NLI-grounded
    `unsupported_claim_rate` does not measure. Per the review feedback
    on the v1 preprint, a verbatim-grounding system can be both
    "well-grounded" (cites the supplied context faithfully) and
    "incorrect" (cites the context but never produces the gold answer
    span). Conversely, an LLM-based system can be "correct" (says the
    gold answer) and "grounded-incomplete" (its surrounding
    elaboration is scored neutral by NLI). Reporting both axes
    separates the two properties.
    """
    if not response:
        return False
    targets = [t for t in [answer, *(aliases or [])] if t]
    if not targets:
        return False
    lower = response.lower()
    for t in targets:
        t_lower = t.lower().strip()
        if not t_lower:
            continue
        # Word-bounded regex match. Escape regex specials; anchor on
        # non-alphanumeric (or string boundary) both sides.
        pat = re.compile(
            rf"(?:^|[^\w]){re.escape(t_lower)}(?:[^\w]|$)"
        )
        if pat.search(lower):
            return True
    return False


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
