<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0010
title: Scorer sensitivity audit — prove each dimension detects its failure mode
status: accepted
date: 2026-05-26
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (v0.1 implementation review)]
---

# 0010 — Scorer sensitivity audit: prove each dimension detects its failure mode

## Context

After ADR-0009 the eval report reads:

```
verbatim_compliance     = 1.000
schema_match_rate       = 1.000
limitation_recall       = 1.000
limitation_precision    = 1.000
connective_hygiene_rate = 1.000
acknowledgment_only     = 1.000
lexical_diversity       = 1.000
role_coverage           = 0.980
boundary_smoothness     = 1.000

caveat:
  • every dimension reads above 0.99 — v1's wave-1 baseline
    (0.993) exhibited the same shape because the scorer could
    not see realiser-class violations; verify the corpus
    actually exercises the realizer's failure modes …
```

The narrow-corpus caveat retired correctly with ADR-0009. The
v1-pattern correctness canary is **still firing** because the
realizer is deterministically correct over a well-designed corpus,
so every dimension reads 1.000.

A canary that always fires loses signal value. The integrator
cannot tell from the caveat alone whether:

- the corpus genuinely passes (healthy realizer, well-designed
  corpus, scorers correctly silent), **or**
- the scorers are blind to violations the corpus does produce
  but they fail to detect — the v1 wave-1 pattern verbatim.

The canary in its current form refuses to distinguish those two
states. The v1 lesson the canary was designed against is that
the second state went undetected for an entire wave.

## Decision

**Add a deterministic sensitivity audit that mutates a baseline
output along each scoring dimension and verifies the scorer
detects the mutation. The eval report carries the audit
inline; the v1-pattern canary downgrades to informational when
the audit confirms scorers are sensitive, and escalates when any
dimension fails to detect its mutation.**

This converts the canary from "every dimension at 0.99 is
suspect" into "every dimension at 0.99 plus a clean sensitivity
audit means the run is healthy by construction; every dimension
at 0.99 plus a failed sensitivity audit is a critical scorer
regression."

### Sensitivity audit

A new module `crates/hyphae-eval/src/sensitivity.rs` exposes
`run_sensitivity_audit(lexicon: &Lexicon) -> SensitivityReport`.
The audit:

1. Constructs a baseline `EvalQuery` + `RealizationOutput` pair
   where every dimension passes.
2. For each scored dimension, applies a controlled mutation
   that should flip that dimension from pass to fail.
3. Scores baseline and mutated outputs with `score_query`.
4. Records whether the scorer detected the mutation
   (`baseline_pass && !mutated_pass`).

The audit is **deterministic** — same baseline, same mutations,
same lexicon, same results. Reproducible across runs.

### Dimensions covered (9)

Each dimension has one mutation that should drop it:

| Dimension | Baseline | Mutation | Expected effect |
|---|---|---|---|
| `verbatim_compliance` | text contains seed body verbatim | replace body in text | `verbatim_pass` true → false |
| `schema_match_rate` | `schema_used = expected` | swap schema | `schema_pass` true → false |
| `limitation_recall` | required limitation fires | remove limitation | recall 1.0 → 0.0 |
| `limitation_precision` | no spurious limitations | inject spurious limitation | precision 1.0 → 0.0 |
| `connective_hygiene` | clean inter-fragment phrasing | inject `"However, However,"` | hygiene true → false |
| `acknowledgment_only` | flag matches expectation | flip flag | ack_pass true → false |
| `lexical_diversity` | three distinct phrases | three repeats of one phrase | diversity 1.0 → < 0.5 |
| `role_coverage` | three distinct roles | three of same role | coverage 1.0 → < 1.0 |
| `boundary_smoothness` | clean boundary | anaphor-before-determiner | smoothness 1.0 → < 1.0 |

### Report integration

`EvalReport` gains a `sensitivity_audit: Option<SensitivityReport>`
field. `EvalHarness::run()` invokes the audit and attaches the
result. The audit is `Option` because tests that construct
reports from synthetic scores (without a harness) can skip it.

The render() output gains a section:

```
── Scorer sensitivity audit (ADR-0010) ──
  ✓ verbatim_compliance     — paraphrase detected
  ✓ schema_match_rate       — schema swap detected
  ✓ limitation_recall       — missing acknowledgment detected
  ✓ limitation_precision    — spurious acknowledgment detected
  ✓ connective_hygiene_rate — doubled connective detected
  ✓ acknowledgment_only     — flag mismatch detected
  ✓ lexical_diversity       — phrase repetition detected
  ✓ role_coverage           — role repetition detected
  ✓ boundary_smoothness     — Rule-1 violation detected
  status: 9/9 dimensions sensitive
```

### Canary downgrade

`build_caveats` consults the audit status:

- **All dimensions sensitive + canary firing** → informational
  caveat: "every dimension reads above 0.99; sensitivity audit
  confirms scorers detect their failure modes; this run reflects
  a healthy realizer plus a well-designed corpus."
- **Any dimension not sensitive** → critical caveat: "scorer
  for X dimension does not detect its failure mode; the
  N.NNN reading is unreliable; investigate before publishing."
- **No audit attached** → existing v1-pattern caveat fires
  unchanged (backward compatible with synthetic-score reports).

This is the **correct** v2 evolution of the canary: it now
distinguishes the two states the v1 wave-1 pattern conflated.

### What this ADR explicitly does **not** do

- **Does not** mutate the realizer or introduce "broken realizer"
  modes. Mutations apply to the `RealizationOutput` struct
  post-hoc, never to the realizer's behaviour.
- **Does not** turn negative cases into corpus entries. The
  corpus stays positive-case-only; mutations live in the audit
  module, not in `seed_corpus_en()`.
- **Does not** add ANY new dimension. The audit covers the nine
  dimensions already present.

### Why this and not adversarial corpus queries

Considered: adding queries that the realizer is expected to fail
on. Rejected because the realizer is deterministic and correct
by construction. Manufacturing "failures" by writing
expectations the realizer cannot meet would test the corpus
designer's ability to write contradictions, not the realizer or
scorers.

The sensitivity audit is the **disciplined** alternative: it
proves the scorers' sensitivity by direct mutation, not by
indirect adversarial corpus design. Each mutation is a single
line of code with a clear expected outcome — auditable in one
glance.

## Sources

- **ADR-0001 §"Honest evaluation"** — the discipline this ADR
  upgrades. v1 wave-1 shipped 0.993 because the scorer was
  blind to violations; ADR-0010 proves the v2 scorer is **not**
  blind, by construction.
- **ADR-0008** — defines the nine scoring dimensions this audit
  validates. Sensitivity table aligns 1:1 with that ADR's
  dimension list.
- **`hyphae_eval::scorers::score_query`** — the function under
  test. The audit calls it with baseline + mutation pairs.

## Consequences

- The v1-pattern canary becomes useful again: it distinguishes
  "healthy run" from "unreliable run." A run with all-1.000 and
  a clean audit is publishable; a run with all-1.000 and any
  audit failure is a load-bearing regression.
- New module `sensitivity.rs` (~200 LOC) with one public
  function and one report struct. No new dependencies.
- `EvalReport` gains one Option field; backward compatible via
  `#[serde(default)]`.
- The eval harness now does deterministic mutation testing on
  every `run()`. Cost: 9 extra `score_query` invocations per
  run, negligible.
- The render() output gains a 11-line section. The integrator
  reads pass status at a glance.
- The smoke binary's eval section now also surfaces the audit
  status alongside the dimension means.

## Cross-references

- **ADR-0001 §"Honest evaluation"** — the discipline this ADR
  closes a loop on.
- **ADR-0008 §"v1-pattern canary extension"** — the canary
  this ADR upgrades.
- **`hyphae_eval::scorers`** — the scorer module under audit.
- **RFC v1-living §6 (Honest evaluation)** — the principle the
  audit operationalises.
