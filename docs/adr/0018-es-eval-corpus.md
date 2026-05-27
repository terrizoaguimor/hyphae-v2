<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0018
title: ES eval corpus — native-Spanish queries against the ES lexicon
status: accepted
date: 2026-05-27
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (Fase C item-3 review)]
---

# 0018 — ES eval corpus: native-Spanish queries against the ES lexicon

## Context

ADR-0017 re-entered the Spanish lexicon at architectural-proof
scale (~60 entries). The lexicon compiles, loads, and produces
well-formed ES output when handed to a realizer. But the
substrate has **no eval coverage for the ES path** — the
existing `seed_corpus_en()` is EN-only, and ADR-0017
explicitly deferred the corpus extension.

The architectural risk: ES output that *looks* coherent in a
spot-check (the realizer test from ADR-0017) is not the same
as ES output that survives the eval harness's nine dimensions
(verbatim, schema, limitation recall/precision, hygiene,
acknowledgment-only, lexical diversity, role coverage,
boundary smoothness). Without an ES corpus, no measurement
exists — exactly the v1 wave-1 pattern: claim coverage,
verify nothing.

ADR-0017 §"What this ADR explicitly does not do" reserved this
slot. ADR-0018 fills it with the smallest honest contribution:
a small ES corpus that exercises the existing harness against
the ES realizer.

## Decision

**Add `seed_corpus_es()` returning ~5 native-Spanish
`EvalQuery` entries. No changes to `EvalQuery`, `EvalSeed`, or
`EvalHarness` structs — they are already language-agnostic by
construction (the harness was given `&Lexicon` per
`score_query` since ADR-0008/0010).**

An ES harness is built by passing the ES realizer + the ES
corpus to the existing constructor:

```rust
let es_harness = EvalHarness::new(
    SurfaceRealizer::with_lexicon(Lexicon::baseline_es()),
    seed_corpus_es(),
);
let es_report = es_harness.run();
```

The eval harness consults the realizer's own lexicon (via
`self.realizer.lexicon()`) when calling `score_query`, so the
fluency scorers detect ES phrases in ES output naturally. No
multi-language registry, no per-query lookup, no breaking
changes.

### Why no struct changes

Three reasons the struct surface stays untouched:

1. **`EvalQuery` is content-shaped, not language-tagged.** The
   query string, seed bodies, expectations, and intent are
   already the only fields the harness consumes. Adding a
   `language: LanguageTag` field would force every existing
   call site to update without yielding behaviour the harness
   couldn't already do via separate corpora.

2. **`EvalHarness` already accepts arbitrary realizers.** The
   harness's `score_one` passes `self.realizer.lexicon()` to
   the scorer. Whichever lexicon the realizer was constructed
   with is what gets used for phrase detection. ES realizer →
   ES lexicon → ES phrase detection. Cross-language scoring
   is impossible by construction — exactly what we want for
   v0.2 (one harness instance per language).

3. **Multi-language scoring in one report is a larger ADR.**
   Aggregating EN + ES scores into a single
   `DimensionMeans` would require deciding how the canary
   fires across languages, how the sensitivity audit
   interprets cross-language baselines, and how
   `corpus_exercises_multiple_register_buckets` behaves on a
   mixed corpus. Each is a real decision; v0.2 keeps the
   answer "one harness per language, separate reports."

### Corpus composition

Five queries, native-Spanish bodies + seeds, English
`domain_tags` per ADR-0017:

| id | intent | shape |
|---|---|---|
| `es-dialogue-001` | Dialogue | healthy multi-fragment status check (engineering) |
| `es-empty-001` | Dialogue | empty working set → acknowledgment-only |
| `es-risk-001` | Dialogue | high-confab-risk seed → must fire `HighConfabRisk` |
| `es-contrast-001` | Dialogue | opposed-valence pair → exercises Contrast role |
| `es-summary-001` | Summarize | three-fragment synthesis → exercises ADR-0016 Summary in ES |

The bodies are written natively in Spanish — no machine
translation, no friendly-EN-on-foreign-seeds artefacts (the
v1 wave-1 pattern this v0.2 corpus is the inverse of).
`domain_tags` stay English (`"engineering"`, etc.) per
ADR-0017's deliberate simplification.

### Known limitation — `boundary_smoothness`

The boundary smoothing rules in
`hyphae_surface::boundary` were calibrated for English
(`DEFINITE_DETERMINERS = ["the", "this", "that", "these",
"those"]`, `ANAPHOR_TAILS = ["it,", "this,", "that,", …]`).
They do not apply to ES bodies, where the equivalent
determiners are `el/la/los/las/un/una` and the anaphor surface
forms are `lo, eso, ello`.

For ES queries the `boundary_smoothness` dimension
**always reports 1.0** because the EN rules don't fire on ES
bodies. This is **inflated**, not informative — the same
trap ADR-0008 designed the v1-pattern canary against.

The ES harness's report should be read with this caveat
explicit. A future ADR (ADR-0019 if filed) adds ES boundary
rules; until then, treat ES `boundary_smoothness` as
"not measured" rather than "perfect."

### Known limitation — corpus scale

Five queries is well below the v0.2 EN floor of 25. The ES
corpus is explicitly v0.2's **architectural extension proof**,
not a coverage parity claim. Bucket coverage
(`Technical`, `Conversational`, `Formal`, `Neutral`,
`Mixed`-register variation per ADR-0009) is intentionally
NOT enforced on ES yet — ADR-0017's lexicon at ~60 entries
cannot honestly support the register diversity that ADR-0009's
bucket-coverage invariant requires. Scaling both lexicon and
corpus to ES parity is the ADR-0019 candidate when the demand
becomes concrete.

### Sensitivity audit — partial coverage in ES

The ADR-0010 sensitivity audit baseline uses **EN-shaped
synthetic outputs** (hardcoded strings like `"Drawing from
working memory, \"a\". However, \"b\". That is the substance
available."`). When the audit runs with an ES lexicon, the
three dimensions that depend on **lexicon-phrase detection**
(`lexical_diversity`, `role_coverage`, `boundary_smoothness`)
score 1.0 on BOTH the baseline AND the mutated output: the
ES lexicon detects zero phrases in EN text, the diversity
calculation trivially returns 1.0 for empty input, and the
mutation isn't visible.

**Six of the nine dimensions remain sensitive** under the ES
lexicon:

| Dimension | ES sensitivity | Reason |
|---|---|---|
| `verbatim_compliance` | ✅ sensitive | Content-agnostic check |
| `schema_match_rate` | ✅ sensitive | Discriminator-only |
| `limitation_recall` | ✅ sensitive | Trigger-set comparison |
| `limitation_precision` | ✅ sensitive | Trigger-set comparison |
| `connective_hygiene_rate` | ✅ sensitive | EN doubled-pattern list still in audit |
| `acknowledgment_only_rate` | ✅ sensitive | Boolean flag check |
| `lexical_diversity` | ⚠️ NOT measurable | Audit baselines are EN text; ES lexicon detects zero phrases |
| `role_coverage` | ⚠️ NOT measurable | Same — depends on lexicon-phrase detection |
| `boundary_smoothness` | ⚠️ NOT measurable | EN-calibrated rules don't fire on EN baseline text matched against ES lexicon |

This is an **artifact of the audit's design choice**: ADR-0010
baked EN strings into the audit module rather than
parameterising over a lexicon-specific sample. v0.2 is honest
about the partial coverage; a future ADR (the same one that
adds ES boundary rules) refits the audit to draw its baselines
from the lexicon under test.

The ES harness still reports an audit; the integrator reads
the "6/9 sensitive" verdict with the known limitation in mind.
The 3 "not detected" entries are NOT signs of a scorer
regression — they are signs that the audit baselines and the
realizer's lexicon are speaking different languages.

### What this ADR explicitly does **not** do

- **Does not** modify `EvalQuery`, `EvalSeed`, or `EvalHarness`
  signatures.
- **Does not** introduce a multi-language harness. One harness
  per language.
- **Does not** add ES boundary smoothing rules. Documented as
  known limitation for v0.2; future ADR.
- **Does not** scale the ES corpus to EN parity (25 queries).
  5 is the v0.2 floor for the architectural proof.
- **Does not** translate the `domain_tags` taxonomy. Markers
  stay English per ADR-0017's contract.
- **Does not** add ES smoke binary or ES chat mode. The chat
  REPL stays English-default; future ADR if demand arises.

## Sources

- **ADR-0017 §"What this ADR explicitly does not"** — the
  postponement this ADR redeems.
- **ADR-0008/0009/0010** — the eval harness discipline this
  ADR extends to a second language without compromising.
- **ADR-0016** — the Summary schema exercised by
  `es-summary-001`.
- **`hyphae_surface::boundary`** — the EN-calibrated module
  whose ES analogue is deferred.

## Consequences

- The ES path gains measurement. `cargo test -p hyphae-eval`
  exercises both the EN and ES harnesses; both must score
  cleanly on their respective corpora.
- One new function (`seed_corpus_es()`) in `corpus.rs`.
- One new integration test in `crates/hyphae-eval/` that
  builds the ES harness, runs it, and asserts the report has
  every ES query passing on the language-agnostic dimensions
  (verbatim, schema, limitation recall/precision, hygiene,
  acknowledgment-only).
- The ES `boundary_smoothness` reporting is documented as
  inflated. The integrator reading the ES report knows.
- No regressions: the EN harness is untouched; its 28-query
  baseline still runs with the same numbers.

## Cross-references

- **ADR-0017** — the lexicon this corpus exercises.
- **ADR-0008** — the dimensions the harness reports.
- **ADR-0016** — the Summary schema one ES query reaches.
- **`hyphae_eval::seed_corpus_en`** — the EN counterpart.
