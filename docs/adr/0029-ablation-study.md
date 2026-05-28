<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0029
title: Ablation study — isolating per-component contribution
status: accepted
date: 2026-05-28
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (design phase)]
---

# 0029 — Ablation study

## Context

The head-to-head in
[`docs/perf/baseline-comparison.md`](../perf/baseline-comparison.md)
established that Hyphae meets and beats the vanilla LLM+RAG baseline
on the comparable subset (verbatim_pass: 1.000 vs 0.176,
unsupported_claim_rate filtered: 0.219 vs 0.367–0.490, latency:
~48,000× speedup). That establishes the system-level claim but
leaves a question the writeup itself flags:

> "Without ablations the comparison establishes that Hyphae beats
> vanilla RAG on the comparable subset, but not which Hyphae
> component is doing the work."
> — `baseline-comparison.md` §"What's next"

A paper-grade comparator requires that ablation. Four Hyphae
components plausibly carry the result:

1. **Cascade-shape composition** (ADR-0006) — derives a shape from
   the working set's cascade tree and walks it instead of a flat
   linear emission.
2. **Ethics gate at the Compose coverage point** (ADR-0003) — the
   `EthicallySensitive` limitation trigger and the audit metadata it
   produces.
3. **Connective lexicon scale** (ADR-0005) — the ~250-entry baseline
   EN lexicon (vs a minimal ~10-entry alternative).
4. **Boundary smoothing** (ADR-0007) — the picker's rule-based
   avoidance of doubled determiners, anaphor surface forms, and
   stopword stutters at the quote-connective boundary.

If we disable each in turn and re-measure, the metric deltas isolate
each component's contribution. If a component's ablation produces
no measurable change, that is **also** signal — the component is
either over-engineered for the corpus or its load shows up only at
larger scales than v0.1 tests.

## Decision

**Run four ablation conditions over the same 34-query EN corpus
used by ADR-0027's head-to-head, score each with the same Python
pipeline, and publish the deltas as
`docs/perf/ablation-study.md`. Conditions:**

| ID | Name | What is disabled | Mechanism |
|---|---|---|---|
| A0 | `full` | Nothing — control | Default `SurfaceRealizer::new()`, baseline lexicon, smoothing on |
| A1 | `no-shape` | Cascade-shape composition (ADR-0006) | `RealizationRequest.shape = Some(linear_sequence_for(working_set))` always — bypasses `shape_from_working_set` derivation that picks `Causation`/`Contrast`/etc. based on cascade tree |
| A2 | `no-ethics` | Ethics report at Compose (ADR-0003) | `RealizationRequest.ethics = None` always; `EthicallySensitive` trigger never fires; `HighConfabRisk`/`ShallowCascade`/`EmptyWorkingSet` still fire because they evaluate the working set directly |
| A3 | `minimal-lexicon` | Lexicon scale (ADR-0005) | `SurfaceRealizer::with_lexicon(Lexicon::minimal_en())` — ~10 entries (one per role × Neutral register × Neutral polarity × Mid formality) instead of the ~250-entry baseline; picker's 4-level fallback chain handles the sparseness |
| A4 | `no-smoothing` | Boundary smoothing (ADR-0007) | `SurfaceRealizer` constructed with `smoothing_enabled = false` so the realize loop calls `lexicon.pick_in_context(...)` instead of `lexicon.pick_with_smoothing(role, ctx, idx, Some(prev_signal), Some(next_signal))` |

### Predicted effect per ablation

These predictions are recorded **before** running so the writeup can
honestly mark each as "predicted" vs "observed" and report
surprises.

| Metric | A1 no-shape | A2 no-ethics | A3 minimal-lex | A4 no-smoothing |
|---|---|---|---|---|
| `schema_match_rate` | falls — `Compare`/`Reflect`/`Narrate` queries lose shape-specific roles | unchanged | unchanged | unchanged |
| `verbatim_pass_rate` | unchanged (quoting is preserved) | unchanged | unchanged | unchanged |
| `lexical_diversity` | slight fall | unchanged | **strong fall** — fewer phrases means more repetition | slight fall — smoothing was the reason for variety at boundaries |
| `role_coverage` | strong fall — shape-specific roles never invoked | unchanged | unchanged | unchanged |
| `boundary_smoothness` | unchanged | unchanged | unchanged | **strong fall** — by construction |
| `connective_hygiene` | unchanged | unchanged | possibly falls — repeated single phrase could trip the doubled-connective detector | possibly falls |
| `ngram_overlap_4` | unchanged | unchanged | **rises** — less connective tissue means quotes contribute a larger share | unchanged |
| `unsupported_claim_rate` filtered | unchanged | slight rise on `risk-*` queries (no ethics acknowledgment) | strong rise — repeated connective phrase pushes NLI to neutral | slight rise |
| `unsupported_claim_rate` raw | unchanged | slight rise | strong fall — less connective scaffolding gives NLI fewer "neutral" sentences | unchanged |
| `quoted_content_supported_rate` | unchanged (1.000) | unchanged | unchanged | unchanged |
| Latency | unchanged | slight fall (no ethics evaluation) | unchanged | slight fall |

### What is NOT ablated

**Verbatim quotation contract** (ADR-0001 Hard Commitment 12).
Ablating quotation means replacing it with paraphrase, which is the
LLM baseline's behaviour. The head-to-head against the LLM
*already* measures that delta. Re-doing it as an "ablation" would
double-count the same evidence and conflate the v2 Hyphae system
with a system that violates its central commitment.

**Hash-chained journal and substrate routing.** These do not
participate in the realizer's output for a given working set. They
are exercised by tests in `crates/hyphae-substrate` and don't
contribute to the realized text per query.

**The NLI scorer + corpus + hardware.** Same NLI model, same 34
queries, same laptop — the only varying axis between conditions is
the realizer component disabled. This is the standard ablation
discipline.

### Technical changes required

Three small code additions enable the four ablations; no existing
behaviour is altered when the new opt-in paths are not taken.

1. **`Lexicon::minimal_en()`** — a new constructor in
   `crates/hyphae-surface/src/lexicon.rs` (data in
   `crates/hyphae-surface/src/connective_data.rs`). Returns ten
   entries: one per `ConnectiveRole` variant at neutral register,
   neutral polarity, mid formality. The picker's existing 4-level
   fallback chain resolves register/polarity/formality preferences
   against the minimal entry per role.

2. **`SurfaceRealizer` smoothing flag** — add
   `smoothing_enabled: bool` to the struct (default `true`,
   preserves existing behaviour) and a `disable_smoothing()`
   setter. The realize loop's connective pick becomes a
   conditional:

   ```rust
   let connective = if self.smoothing_enabled {
       self.lexicon.pick_with_smoothing(...)
   } else {
       self.lexicon.pick_in_context(role, &ctx, idx)
   };
   ```

3. **`export_results_ablation.rs`** — example binary in
   `crates/hyphae-eval/examples/` that takes a `--ablation` flag
   (`none|no-shape|no-ethics|minimal-lexicon|no-smoothing`) and
   constructs the realizer + per-query request accordingly. Emits
   the same JSON envelope shape as `export_results.rs`, with
   `metadata.ablation` set so downstream consumers can identify the
   condition.

The Python side (`score_hyphae.py`) needs no changes — it grades
whatever JSON `--hyphae-output` points at against the same metric
suite.

## What this comparison establishes

**Establishes:**
- Per-component contribution of cascade-shape composition, ethics
  gate, lexicon scale, and boundary smoothing to each comparable-
  subset metric.
- The minimum components a reader needs to keep enabled for Hyphae
  to retain the head-to-head margin against vanilla RAG.
- Which Hyphae component, if any, fails to produce a measurable
  delta on v0.1 corpus (signal that the component is either
  over-engineered for the workload or load-bearing only at scale).

## What this comparison does NOT establish

- **Interactions between components.** A1+A2+A3+A4 disabled
  simultaneously is *not* run. A more complete ablation matrix
  (factorial: 2⁴ = 16 conditions) is deferred — the four
  single-component runs already overflow what a 34-query corpus can
  separate statistically.
- **Generalization to larger corpora.** Same N=34 caveat from
  ADR-0027. Re-running at corpus size in the hundreds or thousands
  is a separate ADR (corpus expansion follows ADR-0009 pattern).
- **The Spanish (ES) results.** A4 (no-smoothing) interacts with
  language-specific boundary rules (ADR-0019); the ES corpus is
  small enough (5 queries) that the ablation would not produce
  stable deltas. ES ablations deferred until the ES corpus reaches
  a similar size to the EN one.
- **Hardware sensitivity.** Same hardware as ADR-0027. Hardware
  matrix is ADR-0028.

## Honesty discipline

Same rule as ADR-0027 and the v0.2 baseline: every published number
travels with a caveats section. The writeup at
`docs/perf/ablation-study.md` MUST publish the predicted vs
observed delta per metric per ablation and flag surprises rather
than smoothing them out. A surprise is a signal that the
prediction's mental model is wrong; flagging it produces more value
than silently reconciling.

## Consequences

**Positive:**
- The paper claim acquires component-level resolution. Reviewers
  can see which Hyphae piece drives which metric.
- Three small code additions (one constructor, one bool flag, one
  example binary) — no rewrites, no behaviour change for default
  callers.
- The same Python scoring pipeline grades all five conditions; no
  duplicate metric implementation.

**Negative:**
- Adds a `Lexicon` constructor whose purpose is exclusively
  experimental. Risk of accidental use in production. Documented
  in the doc comment.
- The ablations are single-component; an interaction the four
  individually do not surface remains invisible.
- N=34 is small. The bootstrap CIs across conditions will overlap
  for several metrics; the writeup must be honest that "direction"
  is more meaningful than "magnitude" at this corpus size.

## Followups

- **0028** (planned, queued before this one in the original list):
  hardware matrix. Re-run **all five conditions** (full + 4
  ablations) on a server-class machine.
- **0030** (planned): "strong RAG" baseline as the inverse of this
  ADR — adds capacity to the comparator side rather than removing
  it from the Hyphae side.
- **Larger-corpus ablation** (separate ADR series) — once a
  500-query corpus exists, re-run the four conditions and check
  whether the deltas hold or whether some component's contribution
  amplifies/disappears at scale.
- **Factorial ablation matrix** — 16 conditions for the four
  components, run at scale. Hard to justify on a 34-query corpus;
  the single-component sweep is the right v0.1 scope.
