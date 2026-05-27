<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0008
title: Empirical calibration ladder — fluency dimensions + v1-pattern canary extension
status: accepted
date: 2026-05-26
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (v0.1 implementation review)]
---

# 0008 — Empirical calibration ladder: fluency dimensions + v1-pattern canary extension

## Context

ADRs 0004 → 0007 expand the substrate's expressive surface
(embeddings, lexicon scale 20 → ~250, cascade-shape-driven
composition, boundary smoothing). Each ADR ships unit + module
tests that prove the component *works*, but the eval harness
(`hyphae-eval`, established in v0.1) only scores **correctness**
dimensions:

- `verbatim_compliance` — bodies preserved.
- `schema_match_rate` — realizer picked the right schema.
- `limitation_recall` / `limitation_precision` — required
  acknowledgments fired, spurious ones did not.
- `connective_hygiene_rate` — no doubled-connective stutters.
- `acknowledgment_only_rate` — empty-working-set path flagged.

Six dimensions of **correctness**. Zero dimensions of **fluency**.

Mario's framing was: *"can we get to something that looks like an
LLM but isn't?"* The v0.1 expansion answers that on the **producer
side** — the lexicon has 250 phrases, the cascade selects from
ten roles, the picker filters by register+polarity+formality+
smoothing. But the harness has no way to confirm the consumer
actually receives that variety. A realizer that picks the same
three phrases out of 250 for every query would pass every existing
dimension and still read as template-rigid.

This is exactly the v1 wave-1 pattern: every scored dimension at
0.993, while Atlas separately flagged "the realiser never produces
native-ES output the upgraded scorer can grade." Honest evaluation
demands that **the scorer measure what the consumer experiences**,
not what the producer claims.

## Decision

**Add three fluency dimensions to the eval harness and extend the
v1-pattern canary to cover them. Publish the dimensions as honest
numbers without target thresholds — the v0.1 discipline.**

### New per-query dimensions

#### `lexical_diversity: f32`

For each query, count the distinct lexicon phrases that appear in
the output. Score:

```
lexical_diversity = distinct_phrases / total_phrase_instances
```

- `1.0` — every emitted phrase is distinct.
- `0.5` — half of the phrase instances are repetitions.
- `0.0` — every phrase is the same one repeated (degenerate).
- `1.0` when no lexicon phrases are detected (acknowledgment-only
  outputs, empty compositions).

Detection: post-hoc text matching against `Lexicon::entries()`.
The lexicon entries are hand-curated phrases, so matching is
straightforward whole-phrase containment. **No NLP model** — the
boundary that ADR-0001 §"Hard architectural commitments" demands.

#### `role_coverage: f32`

For each query, count the distinct connective roles invoked
(`Opening`, `Continuation`, `Contrast`, `Causation`,
`Concession`, `Elaboration`, `Sequence`, `Attribution`,
`Closing`, `Summary`). Score:

```
role_coverage = distinct_roles_used / min(total_phrase_instances, 10)
```

- A 3-phrase output using `Opening + Causation + Closing` →
  `3 / min(3, 10)` = `1.0`.
- A 3-phrase output using `Opening + Continuation +
  Continuation` → `2 / 3` ≈ `0.67`.
- A single-phrase output (acknowledgment) → `1 / 1` = `1.0`
  trivially.

The cap at 10 matches the role taxonomy from ADR-0005 §"Role
taxonomy: 5 → 10". A composition that uses 4 distinct roles in 8
phrase emissions scores `4 / 8` = `0.5` — fine for the harness's
purposes since the corpus's largest compositions are 3-4 fragments.

#### `boundary_smoothness: f32`

For each query, count adjacent quote-quote boundaries (`"…" X "…"`
patterns) and check whether the connective phrase `X` violates an
ADR-0007 rule given the boundary's actual signal:

- Rule 1 — anaphor (`it,` / `this,` / `that,`) before a quote
  that opens with a definite determiner (`the` / `this` / `that` /
  `these` / `those`).
- Rule 3 — phrase contains the exact content token that bridges
  the two adjacent bodies.

Score:

```
boundary_smoothness = 1 - (rule_violations / boundaries_detected)
```

- `1.0` when zero violations.
- `0.0` when every boundary violates a rule.
- `1.0` when no boundaries detected (single-fragment outputs).

This is the **regression sentinel** for ADR-0007. If the picker's
smoothing breaks, this number drops below 1.0 and the v1-pattern
canary fires.

### Corpus-wide dimension

#### `distinct_phrases_corpus_wide: usize`

Across the entire corpus run, how many distinct lexicon phrases
got used at least once? Reported as an absolute count, not a
fraction — `30 / 250` is more informative for the integrator than
`0.12`.

A baseline run of the v0.1 corpus (12 queries) should exercise
~25-35 distinct phrases. If the run produces `< 15`, the picker
is cycling through too narrow a slice of the lexicon.

### v1-pattern canary extension

The existing canary fires when all six correctness dimensions
read above 0.99. ADR-0008 extends this:

> If `lexical_diversity`, `role_coverage`, AND
> `boundary_smoothness` all read above 0.99 simultaneously **and**
> the correctness canary is firing, surface a stronger caveat:
> "the corpus is not exercising the realizer's failure modes."

This is the v1-bucket-1 pattern at the fluency layer. A "perfect
score" run is suspect because the corpus's job is to expose
failure modes the architecture must defend against.

### No target thresholds

The v0.1 harness does not publish a "fluency complete" target.
Per ADR-0001 §"Honest evaluation" and the v1 wave-1 lesson,
thresholds become targets to be gamed. The harness publishes the
numbers; the integrator (Mario) reviews them and decides whether
they look healthy for the workload.

A future ADR (post-v0.1) may layer a `calibration_thresholds.toml`
on top of the report — explicitly opt-in, with a documented
contract that the thresholds are for **process control**, not
release gates.

### Per-query reporting

Per-query scores expose the new dimensions so the integrator can
drill into specific failures. The render() output adds three
lines after the existing six:

```
lexical_diversity       = 0.875
role_coverage           = 0.667
boundary_smoothness     = 1.000
distinct_phrases_used   = 31
```

### Implementation surface

The change is contained to `hyphae-eval`:

- `crates/hyphae-eval/src/scorers.rs` — three new dimension
  fields on `QueryScore`, three new computation helpers.
- `crates/hyphae-eval/src/report.rs` — three new fields on
  `DimensionMeans`, plus `distinct_phrases_corpus_wide` on
  `EvalReport`, extended canary, three new caveats, extended
  `render()`.
- `crates/hyphae-eval/src/corpus.rs` — three new corpus entries
  to exercise the fluency dimensions: a multi-role composition,
  an opposed-valence cascade triggering Contrast role, and a
  causation-shape composition exercising ADR-0006's role
  projection.
- `crates/hyphae-eval/Cargo.toml` — dep on `hyphae-surface`'s
  `Lexicon` (already present; reuses the existing path).

No changes to `hyphae-surface`, no changes to the realizer's
output struct. Detection is post-hoc on the rendered text — the
boundary the no-LLM-in-cognition-path commitment depends on.

### Cascade-shape coherence — deferred

The original ADR draft included a fourth dimension:
`cascade_shape_coherence` — does the chosen connective's role
match the cascade-shape projection from ADR-0006? Deferred to a
future ADR because:

1. The eval harness builds working sets from declarative seeds,
   not from a live cascade. The shape information lives in the
   composer, which the harness bypasses.
2. To implement, the harness would need access to the working
   set's parent-id topology and would need to reconstruct the
   cascade graph — that's substrate-level integration the v0.1
   harness was deliberately scoped to skip.

A substrate-integrated harness (in `hyphae-tests` or a future
`hyphae-eval-integration` crate) is the right place for this
dimension. Not v0.1.

## Sources

- **ADR-0001 §"Honest evaluation"** — the v1 bucket-1 lesson
  this ADR operationalises for the fluency axes.
- **ADR-0005 §"Role taxonomy"** — the 10 roles
  `role_coverage` partitions over.
- **ADR-0007** — `boundary_smoothness` is the regression
  sentinel for the smoothing rules.
- **`hyphae_eval::DimensionMeans`** — the type this ADR extends.

## Consequences

- The integrator gains three numbers that answer the "does it
  look like an LLM but isn't" question on the consumer side:
  variety of phrases, breadth of roles, hygiene of boundaries.
- The v1-pattern canary now covers nine dimensions, not six. A
  run that reads `all above 0.99` triggers the extended caveat
  about corpus failure-mode exposure.
- The eval harness's report renders three more lines plus a
  corpus-wide phrase count.
- The corpus grows from 12 to ~15 queries to exercise the new
  dimensions. Existing query semantics unchanged.
- No new dependencies; no changes to the realizer's API.
- All v0.1 invariants stay: clippy + fmt clean, tests deterministic,
  EN-only, no LLM in cognition path.

## Cross-references

- **ADR-0001 §"Honest evaluation"** — the discipline this ADR
  scales to fluency.
- **ADR-0005** — the lexicon scale `lexical_diversity` and
  `role_coverage` measure consumption of.
- **ADR-0006** — `role_coverage` will detect when cascade-shape
  projection drops back to flat `Continuation`-everywhere.
- **ADR-0007** — `boundary_smoothness` is the live regression
  test for the smoothing rules.
- **RFC v1-living §6 (Honest evaluation)** — the principle this
  ADR extends.
