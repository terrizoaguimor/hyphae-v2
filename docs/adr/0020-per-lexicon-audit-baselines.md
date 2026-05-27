<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0020
title: Per-lexicon sensitivity-audit baselines
status: accepted
date: 2026-05-27
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (Fase C item-5 review)]
---

# 0020 — Per-lexicon sensitivity-audit baselines

## Context

ADR-0010 introduced the scorer sensitivity audit — a
deterministic harness that mutates baseline outputs and
verifies each scorer dimension detects its failure mode. The
audit's nine baseline+mutation pairs are **hardcoded EN
strings** in `crates/hyphae-eval/src/sensitivity.rs`:

```rust
let baseline = sample_output(
    "Drawing from working memory, \"a\". However, \"b\". \
     That is the substance available.",
    …
);
```

ADR-0018's ES corpus exposed the limitation: under an ES
lexicon, three dimensions (`lexical_diversity`,
`role_coverage`, `boundary_smoothness`) score 1.0 on BOTH
baseline AND mutated outputs. The ES lexicon detects zero
phrases in EN text — the mutations become invisible — and
the audit reports "6/9 sensitive" with the 3 lexicon-bound
dimensions failing.

ADR-0018 documented this as a v0.2 limitation and reserved
this slot. ADR-0019 added ES boundary rules to the realizer +
scorer path but explicitly did NOT touch the audit, because
the audit's baselines are themselves EN-shaped strings — even
with ES boundary rules, ES anaphor patterns don't fire on EN
baseline text.

ADR-0020 fixes the audit itself.

## Decision

**Refactor the three lexicon-dependent verifications in
`run_sensitivity_audit` so they construct baselines + mutations
from the lexicon under test, not from hardcoded EN strings.
With ES lexicon, baselines now use ES phrases; with EN lexicon,
they use EN phrases. The audit becomes per-lexicon.**

### Helpers

Three pure helpers exposed `crate`-internally:

```rust
/// Pick `n` connectives whose roles are pairwise distinct,
/// in lexicon-traversal order. Returns fewer than `n` if the
/// lexicon does not have enough role variety.
fn pick_n_phrases_distinct_roles(
    lex: &Lexicon,
    n: usize,
) -> Vec<(ConnectiveRole, String)>;

/// Find a connective whose phrase ends with one of the
/// lexicon's boundary-rule anaphor tails. Returns the phrase
/// (owned String).
fn find_anaphor_connective(lex: &Lexicon) -> Option<String>;

/// Return a connective whose phrase does NOT end with anaphor
/// (suitable as the "clean" boundary in the baseline of the
/// boundary_smoothness verification).
fn find_non_anaphor_connective(lex: &Lexicon) -> Option<String>;
```

### Refactored verifications

#### `lexical_diversity`

```rust
let phrases: Vec<String> = pick_n_phrases_distinct_roles(lexicon, 3)
    .into_iter().map(|(_, p)| p).collect();
// Baseline: three distinct phrases.
let baseline_text = format!("{} \"alpha\". {} \"beta\". {} \"gamma\".",
    phrases[0], phrases[1], phrases[2]);
// Mutated: one phrase repeated three times.
let mutated_text = format!("{} \"alpha\". {} \"beta\". {} \"gamma\".",
    phrases[0], phrases[0], phrases[0]);
```

With EN lexicon: `phrases[0..3]` will include EN openings,
contrasts, closings. With ES lexicon: ES equivalents. Both
yield "3 distinct phrases" baseline and "1 phrase repeated"
mutated.

#### `role_coverage`

Same `phrases` set (each from a distinct role) drives this
verification too. The lexical_diversity baseline IS the
role_coverage baseline structurally — three distinct phrases
from three distinct roles. The mutation collapses to one
phrase ⇒ one role.

#### `boundary_smoothness`

```rust
let rules = lexicon.boundary_rules();
let det = rules.definite_determiners.first().copied().unwrap_or("the");
let anaphor = find_anaphor_connective(lexicon).unwrap_or_else(|| "Building on it,".to_string());
let safe = find_non_anaphor_connective(lexicon).unwrap_or_else(|| "However,".to_string());

let body_a = format!("{det} sustancia_a aquí");
let body_b = format!("{det} sustancia_b allí");
let baseline_text = format!("\"{body_a}\" {safe} \"{body_b}\"");
let mutated_text  = format!("\"{body_a}\" {anaphor} \"{body_b}\"");
```

The body's first raw token is the language's definite
determiner; the rest is placeholder content. Under EN rules:
"the" triggers `starts_with_definite_determiner`. Under ES
rules: "el"/"la" triggers it. The anaphor connective's tail
matches the language's `anaphor_tails`. Rule 1 fires in the
mutated output; not in the baseline. Audit detects the
mutation in either language.

### Graceful degradation

If a helper returns `None` (lexicon too small to satisfy the
constraint — e.g. fewer than three distinct roles, or no
anaphor connective), the verification falls back to the
**hardcoded EN baseline** used in ADR-0010. This preserves
back-compat for empty / minimal test lexica. The two production
lexica (`baseline_en`, `baseline_es`) both satisfy every
constraint, so the fallback never fires in normal usage.

### What this ADR explicitly does **not** do

- **Does not** change the six language-agnostic verifications
  (`verbatim_compliance`, `schema_match_rate`,
  `limitation_recall`, `limitation_precision`,
  `connective_hygiene_rate`, `acknowledgment_only_rate`). They
  already work cross-language.
- **Does not** introduce a per-language threshold or per-
  language report variant. The audit's verdict (9/9 sensitive
  or the failing set) is language-agnostic in expression; only
  the construction of mutations is language-aware.
- **Does not** add audit support for languages whose lexicons
  do not yet exist. Adding PT/FR/DE/IT/RU/TR/JA/ZH lexicons
  automatically extends audit coverage to them (the helpers
  read from `&Lexicon`); no audit ADR per language needed.
- **Does not** validate that the helpers' lexicon picks are
  optimal. The first-N-distinct-roles selection is
  deterministic but not curated. Future ADR can introduce
  preference-ordered selection if mutation strength becomes
  load-bearing.

## Sources

- **ADR-0010 §"Dimensions covered (9)"** — the audit
  structure this ADR parameterises.
- **ADR-0018 §"Sensitivity audit — partial coverage in ES"** —
  the documented limitation this ADR closes.
- **ADR-0019** — language-aware boundary rules whose lexicon
  side this ADR now extends to the audit.
- **`hyphae_eval::sensitivity`** — the module modified.

## Consequences

- The ES audit floor rises from 6/9 to 9/9 sensitive. The ES
  harness regression-guard test in
  `crates/hyphae-eval/src/lib.rs` is updated to assert the
  promoted floor.
- Any future language whose lexicon ships through
  `Lexicon::baseline_X()` gets audit coverage automatically;
  the audit picks phrases from the lexicon and tests them
  with the lexicon's own boundary rules.
- The hardcoded EN fallback in each helper survives for
  empty/minimal-lexicon tests. Removing the fallback is a
  future cleanup ADR if the empty-lexicon use case dies.
- The audit's deterministic output for `baseline_en` stays
  unchanged (the lexicon-derived baselines happen to pick the
  same EN phrases that were hardcoded — first-N-distinct-roles
  order). EN baseline numbers in `target/criterion/` do not
  shift.
- The ADR-0018 caveat in `seed_corpus_es()` doc-comment is
  retired (the "boundary smoothness inflated 1.0" line still
  applies to **real** ES output where the realizer doesn't
  fire on ES bodies that DON'T match a known anaphor pattern;
  ADR-0020 closes the audit-side gap specifically).

## Cross-references

- **ADR-0010** — the audit this ADR refactors.
- **ADR-0017/0018/0019** — the ES path whose audit coverage
  this ADR completes.
- **`hyphae_eval::sensitivity::run_sensitivity_audit`** — the
  entrypoint whose internals this ADR rewires.
