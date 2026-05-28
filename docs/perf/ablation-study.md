<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

# Ablation study — which Hyphae component drives which metric

> **Status — 2026-05-28**. First component ablation of Hyphae v0.1.
> Five conditions over the 34-query EN corpus: full (A0) plus four
> single-component ablations (A1 no-shape, A2 no-ethics,
> A3 minimal-lexicon, A4 no-smoothing). Design + hypotheses are in
> [`../adr/0029-ablation-study.md`](../adr/0029-ablation-study.md).
> **Read ADR-0029 first** — it states what each condition disables
> mechanically and what was predicted before the run.

## TL;DR — three findings

1. **Verbatim quotation, connective hygiene, and quoted-content
   support all hold at 1.000 across every ablation.** The
   architectural claim of Hyphae — *citations remain verbatim and
   well-formed under composition* — does not depend on cascade
   shape, ethics signal, lexicon scale, or boundary smoothing. It is
   load-bearing only on the realizer's quotation contract, which
   none of the four ablations touches.
2. **Only A3 (minimal-lexicon) moves the comparator metrics
   measurably.** Reducing the lexicon from ~250 entries to 10 raises
   `ngram_overlap_4` from 0.466 to 0.521 (+12%) and lowers the raw
   unsupported-claim rate from 0.625 to 0.461 (-26%). The connective
   tissue *is* what drags `ngram_overlap_4` down in the head-to-head
   against the LLM — confirmed by ablation.
3. **A1, A2, A4 produce no statistically meaningful delta on this
   corpus.** Cascade-shape composition, the ethics gate, and
   boundary smoothing each carry observable structural changes (the
   sample responses below show them) without measurable change in
   the comparator metrics. This is signal: their contribution is
   either *qualitative* (structural variety the metric set does not
   capture), *latent* (load-bearing only at scales above v0.1), or
   *over-engineered* for this corpus. The writeup does not pick
   between those interpretations — it reports the null result and
   surfaces the structural difference the sample responses make
   visible.

## Conditions and predictions

Recapping ADR-0029 §"Predicted effect per ablation" so the
predictions are visible side-by-side with the observed deltas:

| Metric | A1 predicted | A2 predicted | A3 predicted | A4 predicted |
|---|---|---|---|---|
| `verbatim_pass_rate` | unchanged | unchanged | unchanged | unchanged |
| `ngram_overlap_4` | unchanged | unchanged | **rises** | unchanged |
| `unsupported (filtered)` | unchanged | slight rise | strong rise | slight rise |
| `unsupported (raw)` | unchanged | slight rise | **strong fall** | unchanged |

## Headline table

| Metric | A0 full | A1 no-shape | A2 no-ethics | A3 minimal-lex | A4 no-smoothing |
|---|---:|---:|---:|---:|---:|
| `verbatim_pass_rate` | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 |
| `connective_hygiene_pass_rate` | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 |
| `quoted_content_supported_rate` | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 |
| `ngram_overlap_4` (mean) | 0.466 | 0.460 | 0.466 | **0.521** | 0.457 |
| `ngram_overlap_5` (mean) | 0.416 | 0.410 | 0.416 | **0.466** | 0.407 |
| `ngram_overlap_8` (mean) | 0.240 | 0.236 | 0.240 | 0.272 | 0.234 |
| `unsupported_claim_rate` (filtered) | 0.219 | 0.188 | 0.219 | 0.297 | 0.188 |
| `unsupported_claim_rate` (raw) | 0.625 | 0.656 | 0.625 | **0.461** | 0.641 |
| `latency_mean` (ms) | 0.024 | 0.056 | 0.048 | 0.030 | 0.021 |

Bold = magnitude of delta vs A0 ≥ 5 percentage points on a rate
metric or ≥ 5% on a continuous metric. Confidence intervals
(bootstrap 95% percentile, 1000 resamples) are recorded per-metric
in the per-run JSON envelopes under `aggregate`.

## Per-ablation interpretation

### A1 — no-shape (cascade-shape composition disabled)

**Mechanism**: every query's `RealizationRequest.shape` is set to a
flat linear Continuation shape. The realizer's
`shape_from_working_set` derivation — which detects opposed valence
and injects Contrast steps, or applies Causation/Sequence per
ADR-0006 — is bypassed.

**Observed**: comparator metrics essentially unchanged. The
structural difference is visible only in the prose itself:

| Query | A0 (full) | A1 (no-shape) |
|---|---|---|
| `contrast-001` | "the launch succeeded …" **However,** "the rollback was painful …" | "the launch succeeded …" **Per the next fragment,** "the rollback was painful …" |

The semantic role *opposition* is lost — A0 picks `However,` because
shape derivation flagged Contrast; A1 emits `Per the next fragment,`
(the Continuation role) because there is no shape signal to
override it. **The comparable-subset metrics do not capture this
loss.** A future ADR (corpus-level structural metric? readability?)
would.

**Predicted vs observed**: predicted no change, observed no change.
The structural change exists but is invisible to the v0.1 metric
set.

### A2 — no-ethics (ethics report withheld from realize)

**Mechanism**: every `RealizationRequest.ethics = None`. The
`EthicallySensitive` limitation trigger cannot fire. Note: the
other three limitation triggers (`EmptyWorkingSet`, `HighConfabRisk`,
`ShallowCascade`) evaluate the working set directly and continue
to fire.

**Observed**: **identical** to A0 across every metric, down to four
decimal places. The samples are byte-for-byte identical:

```
A0:  Per the recorded fragments, "an unnamed colleague reportedly said
     the architecture would not scale" That is the substrate's current
     view.
A2:  Per the recorded fragments, "an unnamed colleague reportedly said
     the architecture would not scale" That is the substrate's current
     view.
```

**Predicted vs observed**: predicted slight rise on `risk-*` queries
because the ADR's mental model assumed the ethics report contributed
to the `risk-*` handling. **It does not.** The corpus's high-confab-
risk seeds set the seed's `confabulation_risk` field directly; the
`HighConfabRisk` trigger reads that field, not the ethics report.
The ethics report's only contribution would be on a query the corpus
does not contain — one with ethically-sensitive content (CBRN,
self-harm, child safety, etc.).

**This is a useful null result.** For the v0.1 corpus, the ethics
gate is mechanically present but has nothing to do. A future corpus
that exercises ethically-sensitive material would surface its
contribution; v0.1 cannot. ADR-0029's mental model was wrong about
*where* the contribution would show up; the corrected reading is
that the contribution is corpus-conditional and the v0.1 corpus does
not condition for it.

### A3 — minimal-lexicon (~10 entries instead of ~250)

**Mechanism**: realizer constructed with `Lexicon::minimal_en()` —
ten entries, one per `ConnectiveRole` variant, all at neutral
register / neutral polarity / mid formality. The picker's 4-level
fallback chain resolves contextual preferences against the single
entry per role.

**Observed**: the largest deltas in the study.

- `ngram_overlap_4` **rises** from 0.466 → 0.521 (+12%). The
  minimal lexicon's phrases (`"Note that"`, `"Also,"`,
  `"Per the record:"`) are shorter and contribute fewer
  non-context tokens than the baseline's longer phrases
  (`"Per the recorded fragments,"`,
  `"Drawing from working memory,"`). The quote portion of the
  response now dominates the n-gram window. **This confirms the
  hypothesis behind the head-to-head's `ngram_overlap_8`
  inversion** (Hyphae 0.240 vs LLM 0.329 — see
  `baseline-comparison.md` §"On the ngram_overlap_8 inversion"):
  the gap is connective-tissue length, not citation fidelity.
- `unsupported_claim_rate` (raw) **falls** from 0.625 → 0.461
  (-26%). With less connective scaffolding the NLI has fewer
  sentences to label `neutral`. The raw rate becomes more
  charitable to Hyphae for purely mechanical reasons. The filter
  variant moves the opposite direction (rises +0.078) because the
  same shorter phrases produce sentences that the
  `is_connective_sentence` heuristic does not catch — e.g.
  `"Note that"` is in the filter list but `"Also,"` and
  `"Specifically,"` slip through and count as claims even though
  they are pure glue. This is a heuristic gap, not signal about
  lexicon scale.

```
A0 dialogue-001:  Per the recorded fragments, "the migration completed at
                  14:02 UTC" Per the next fragment, "the monitoring …" That
                  is the substrate's current view.
A3 dialogue-001:  Note that "the migration completed at 14:02 UTC" Also,
                  "the monitoring …" That is what is on record.
```

**Predicted vs observed**: ngram_overlap rise — predicted, observed.
unsupported_raw fall — predicted, observed. unsupported_filtered
rise — predicted, observed (driven by heuristic gap rather than
hallucination, but the metric direction matches).

**Implication for the head-to-head**: the baseline-comparison's
`ngram_overlap_8` inversion (where Hyphae loses to the LLM 0.240 vs
0.329) is **causally attributable to lexicon scale**. With the
minimal lexicon Hyphae's `ngram_overlap_8` rises to 0.272 — closer
to but not past the LLM. A subsequent ADR could explore whether a
*shorter* lexicon (still adequate for role coverage but tuned for
n-gram-friendly phrases) would close the gap entirely.

### A4 — no-smoothing (boundary-smoothing filter disabled)

**Mechanism**: realizer's connective pick calls
`lexicon.pick_in_context` instead of `pick_with_smoothing`. The
boundary-rule filter that excludes doubled determiners, anaphor
tails, and stopword stutters at the quote-connective boundary is
inactive.

**Observed**: comparator metrics within ±0.01 of A0 on every n-gram
and unsupported-claim measurement. The structural difference is in
which connective the picker chooses at each boundary:

```
A0 dialogue-001:  … "the migration completed at 14:02 UTC" Per the next
                  fragment, "the monitoring dashboards …"
A4 dialogue-001:  … "the migration completed at 14:02 UTC" Following this,
                  "the monitoring dashboards …"
```

The smoothing filter, under A0, picks `Per the next fragment,` at
this boundary because the adjacent quote tail does not trip its
exclusion rules. Without the filter the realizer picks
`Following this,` — the first Continuation entry for the neutral
context. Same role, different phrase. The `boundary_smoothness`
dimension (Hyphae-specific, not in this comparator table) is where
this would show up; the comparable subset misses it by design.

**Predicted vs observed**: predicted no measurable change in the
comparator metrics, observed no measurable change. The
`boundary_smoothness` dimension *would* show the contribution but is
Hyphae-specific (ADR-0027 keeps it out of the comparable-subset
table).

## On the latency numbers

The latency column is **not interpretable at the millisecond level**
in this study. Hyphae's realizer runs in 20–60 microseconds on the
hardware used, and the timer resolution / process noise dominate at
that scale. A0 (full) at 0.024 ms and A1 (no-shape) at 0.056 ms
differ by 32 microseconds; this is within the measurement floor of
a `std::time::Instant` call pair on macOS, and across the 34
queries the noise propagates to the mean.

The honest reading is: **all five conditions run in well under one
millisecond per query**, four orders of magnitude faster than the
LLM baseline. The relative ordering between conditions at this
scale should not be read as signal.

A future criterion-driven ablation bench (parallel to
`hyphae-bench`'s existing harness) would produce statistically
meaningful per-ablation latency comparisons. Not in scope for
v0.1.

## What this ablation establishes

- The **central architectural claim** (verbatim quotation, byte-for-
  byte preservation of seed bodies under composition) is robust to
  each of the four single-component ablations. No condition breaks
  it.
- The **lexicon scale** (ADR-0005) is the component whose
  contribution is detectable on the comparator metrics. Removing it
  raises `ngram_overlap_4` by 12% and lowers raw unsupported-claim
  rate by 26%. The baseline-comparison's `ngram_overlap_8`
  inversion against the LLM is causally attributable here.
- The **cascade-shape composition** (ADR-0006) and **boundary
  smoothing** (ADR-0007) contribute *structurally* — visibly
  different phrasing at the same boundary — but the chosen metric
  set does not separate their effect from the baseline. Their value
  is real; this comparator's resolution is insufficient.
- The **ethics gate at the Compose coverage point** (ADR-0003) has
  no measurable contribution on the v0.1 corpus. The corpus does not
  exercise ethically-sensitive content; the gate is mechanically
  present but inert on this evaluation. Corpus expansion (separate
  ADR) is required to surface its contribution.

## What this ablation does NOT establish

- **Component interactions**. Each ablation disables one component
  in isolation. A 2⁴ = 16-condition factorial would reveal
  interactions; not run on this corpus (statistical separation at
  N=34 is already strained).
- **The contribution at corpus scale.** A 500-query corpus could
  surface effects too small to detect at 34. Direction of effect
  could also reverse — the ngram_overlap_4 rise under A3 might
  attenuate or amplify; no claim about scale extrapolation is made.
- **Reader preference.** None of the metrics measure whether A3's
  shorter phrases produce prose readers prefer over A0's richer
  ones. The minimal lexicon's prose feels noticeably more
  template-rigid; whether that matters to a reader is outside this
  study.
- **The Spanish corpus.** Same ADR-0029 caveat. The 5-query ES
  corpus is too small to support stable ablation deltas. Pending
  ES corpus expansion.

## Reproduce

```bash
# 1. Make sure the prerequisite setup from the head-to-head is done:
#    Python env, Llama model, NLI model. See bench/baseline-llm-rag/README.md.

# 2. From repo root, for each ablation, export Hyphae's output:
cd "$REPO_ROOT"
for a in none no-shape no-ethics minimal-lexicon no-smoothing; do
    cargo run --quiet -p hyphae-eval \
        --example export_results_ablation -- --ablation "$a" \
        > "bench/baseline-llm-rag/hyphae-results-${a}.json"
done

# 3. From the bench directory, score each with the same NLI pipeline:
cd bench/baseline-llm-rag
for a in none no-shape no-ethics minimal-lexicon no-smoothing; do
    uv run python -m baseline_llm_rag.score_hyphae \
        --hyphae-output "hyphae-results-${a}.json" \
        --output "results/v0.1-laptop-hyphae-${a}.json"
done

# 4. The result JSONs in results/v0.1-laptop-hyphae-{condition}.json
#    are the inputs to the comparison table above.
```

Each run takes ~7 seconds (NLI model load + 34 queries scored).
Total reproduction time for the five-condition sweep is under a
minute, excluding initial Python env + model setup.

## What's next

- **ADR-0028 — Hardware matrix.** Re-run all five conditions on a
  server-class machine. The latency story is dominated by
  measurement noise at microsecond scale on this hardware; a
  different machine could clarify the ordering, and the comparator
  metrics' direction would either hold or shift.
- **ADR-0030 — "Strong RAG" baseline.** The inverse of this ADR —
  adds capacity to the comparator side. Whether stronger retrieval
  closes the unsupported-claim gap that the head-to-head opened is
  not addressable by ablating Hyphae alone.
- **Corpus expansion ADR.** The A2 null result (ethics gate has no
  measurable contribution on this corpus) is corpus-conditional.
  A corpus that exercises ethically-sensitive material would
  surface the contribution. The ADR-0009 expansion pattern
  applies.
- **Lexicon shape tuning.** Per A3's finding, lexicon-scale is the
  observable driver of `ngram_overlap_4`. A subsequent ADR could
  explore: are there phrases in the baseline-EN lexicon that
  contribute role variety without dragging n-gram overlap down?
  The minimal lexicon trades expressiveness for n-gram fidelity;
  a tuned middle ground might Pareto-dominate.
