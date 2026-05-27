<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0009
title: Corpus expansion — bucket coverage via domain_tags + register diversity
status: accepted
date: 2026-05-26
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (v0.1 implementation review)]
---

# 0009 — Corpus expansion: bucket coverage via `domain_tags` + register diversity

## Context

The first eval run after ADR-0008 reported:

```
verbatim_compliance     = 1.000
schema_match_rate       = 1.000
limitation_recall       = 1.000
limitation_precision    = 1.000
connective_hygiene_rate = 1.000
acknowledgment_only     = 1.000
lexical_diversity       = 1.000
role_coverage           = 0.967
boundary_smoothness     = 1.000
distinct_phrases_used   = 8

caveats:
  • every dimension reads above 0.99 — verify the corpus actually
    exercises the realizer's failure modes …
  • distinct_phrases_corpus_wide = 8 — the corpus exercises too
    narrow a slice of the ~250-phrase lexicon to be informative
```

The narrow-corpus caveat is **the load-bearing signal** from ADR-0008.
Eight distinct phrases out of ~250 means the picker is locked into
one bucket of the (role × register × polarity × formality) lattice.
Inspection of `hyphae_surface::realizer::register_for_fragment`
explains why: the realizer derives a fragment's register from its
`domain_tags`, and `EvalSeed::into_fragment` never sets them. Every
seed produces `Register::Neutral`, so the picker only ever traverses
the `(_, Neutral, _, Mid)` slice of the lexicon.

This is not an architectural defect — ADR-0005 designed the picker
to be context-sensitive, and ADR-0006 designed the cascade to
project roles. The corpus simply does not vary the context axes the
picker filters on. The fluency dimensions are therefore measuring
**single-bucket coverage**, not actual realizer expressive range.

## Decision

**Add `domain_tags` to `EvalSeed`, backfill the 15 existing queries
with register-appropriate tags, and add ~10 new queries that
explicitly target Conversational and Formal registers.** Restate the
v0.1 honest-evaluation discipline: corpus expansion is judged by
**bucket coverage**, not by raw query count.

### Schema extension

```rust
pub struct EvalSeed {
    pub body: String,
    pub valence: f32,
    pub confabulation_risk: f32,
    pub from_cascade: bool,
    /// **ADR-0009.** Domain tags propagated to the fragment's
    /// `domain_tags`. The realizer's
    /// `register_for_fragment` heuristic reads these to derive
    /// `Register` for picker context.
    #[serde(default)]
    pub domain_tags: Vec<String>,
}
```

`EvalSeed::into_fragment` forwards `domain_tags` to the
`CognitiveFragment`. No other type touches the field — the
realizer already reads `CognitiveFragment::domain_tags` natively
(unchanged from v0.1.0).

### Backfill discipline

Each existing query receives `domain_tags` that match its body's
semantic field:

| query family | example bodies | tags | derived register |
|---|---|---|---|
| `dialogue-001` … `004` | "migration", "deploy", "test coverage", "on-call" | `["engineering"]` | Technical |
| `assert-001` | "migration completed at 14:02 UTC" | `["engineering", "deploy"]` | Technical |
| `assert-002` | "either party may terminate the contract" | `["legal", "contract"]` | Formal |
| `risk-001` / `002` | "architecture would not scale", "release is safe" | `["engineering"]` | Technical |
| `shallow-001` | "sprint focuses on auth refactor" | `["engineering"]` | Technical |
| `contrast-001` | "launch vs rollback" | `["engineering", "deploy"]` | Technical |
| `fluency-*` (ADR-0008) | mixed | `["engineering"]` | Technical |
| `empty-*` | no seeds | — | (empty path) |

Existing semantics unchanged — `domain_tags` only refines the
realizer's register selection, not the substrate's behaviour.

### Added queries (10)

Three register-coverage families, each with 3-4 queries:

1. **Conversational (3 queries)** — informal team check-ins,
   casual feedback requests, chat-style summaries. Tags:
   `["informal", "conversation"]`. Exercises the picker's
   `(_, Conversational, _, _)` bucket — a slice of the lexicon
   that ADR-0005 populated but no corpus query reaches today.

2. **Formal (3 queries)** — policy interpretations, compliance
   attestations, contract clause readings. Tags:
   `["formal", "policy"]` or `["legal", "contract"]`. Exercises
   the `(_, Formal, _, _)` bucket.

3. **Mixed-register / Neutral (4 queries)** — general planning
   questions, cross-functional discussions, ambiguous-domain
   queries that deliberately keep `Register::Neutral`. Tests
   that the realizer does not degenerate when no register
   marker is present — the v0.1 default path must stay healthy.

Corpus total: 15 → 25 queries. Below the v1-style 255-query bar
deliberately — the discipline is bucket coverage, not query
count.

### What this ADR explicitly does **not** do

- **Does not** introduce explicit `formality_override` or
  `polarity_override` on the query. Formality is derived from
  the working set's domain at realizer time
  (`working_set_context_refs` hardcodes `Formality::Mid`);
  varying formality requires a substrate-side change, deferred
  to a future ADR.
- **Does not** pretend that 25 queries is enough to declare
  fluency dimensions load-bearing. The narrow-corpus caveat
  will still fire when `distinct_phrases_corpus_wide < 15` —
  the threshold itself stays untouched. The integrator (Mario)
  reviews whether the expanded run actually clears the caveat.
- **Does not** introduce a TOML/JSON corpus loader. v0.1
  corpus stays in Rust source where reviewer can see seed +
  expectation + tags in one diff.

### Why now and not at v0.1 chartering

Because the narrow-corpus axis was invisible until ADR-0008's
`distinct_phrases_corpus_wide` instrumentation surfaced it. The
v0.1 chartering session (`docs/adr/0001-fresh-from-v1.md`)
prioritised honest correctness scoring over fluency scoring; the
fluency signal arrived later in the timeline. Closing the loop —
"the instrumentation revealed a gap; the next ADR addresses the
gap" — is exactly the v0.1 → v0.2 cycle the chartering session
designed for.

## Sources

The new query bodies are written natively in EN per ADR-0001
§"Multilingual lexicon beyond EN" and the v0.1 EN-only
discipline. No external corpus is imported; bodies are
hand-authored against the v0.1 register taxonomy from ADR-0005.

## Consequences

- `EvalSeed` gains one optional field (`domain_tags`); its
  `Serialize`/`Deserialize` keep `#[serde(default)]`, so older
  serialised corpora deserialise unchanged.
- `EvalSeed::into_fragment` writes through to
  `CognitiveFragment::domain_tags`, which the realizer reads.
  No surface API changes.
- The eval report's `distinct_phrases_used` should rise
  measurably (target ≥ 15; honest result reported regardless).
- The narrow-corpus caveat may stop firing — but its threshold
  (15) was a calibration heuristic, not a release gate. The
  integrator decides whether the new number means anything.
- New query count (25) is below the v1 corpus size by design.
  Bucket coverage > raw count.

## Cross-references

- **ADR-0001 §"Built-but-not-wired"** — the v1 bucket-1 lesson
  this ADR scales to corpus design (corpus must exercise the
  failure modes the scorer can see).
- **ADR-0005 §"Context-aware picker"** — the
  (role × register × polarity × formality) lattice this ADR
  partitions over.
- **ADR-0006** — the cascade-shape projection that interacts
  with register: cascade shape picks the role, register picks
  the phrasing within the role.
- **ADR-0008 §"Corpus-wide phrase exposure caveat"** — the
  instrumentation that surfaced the gap this ADR closes.
- **`hyphae_surface::realizer::register_for_fragment`** — the
  heuristic this ADR feeds with appropriate `domain_tags`.
