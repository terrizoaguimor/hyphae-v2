<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0019
title: ES boundary rules — language-aware boundary smoothing
status: accepted
date: 2026-05-27
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (Fase C item-4 review)]
---

# 0019 — ES boundary rules: language-aware boundary smoothing

## Context

ADR-0018 documented an honest limitation: the boundary
smoothing rules in `hyphae_surface::boundary` are EN-calibrated
(determiners + anaphor surface forms). For ES output, the rules
do not fire, so `boundary_smoothness` reports an inflated 1.0 —
the same `0.993`-shaped pattern ADR-0008's canary was designed
to catch.

Three EN-specific constants drive the smoothing:

```rust
const DEFINITE_DETERMINERS: &[&str] = &["the", "this", "that", "these", "those"];
const INDEFINITE_DETERMINERS: &[&str] = &["a", "an"];
const ANAPHOR_TAILS: &[&str] = &["it,", "this,", "that,", "it", "this", "that"];
```

Plus a `STOPWORDS` list used to filter "content" from "function"
tokens when extracting initial/final content tokens of a body,
and a hardcoded substring set in
`is_continuation_of_same_subject` (e.g. `"likewise"`,
`"continuing"`) which scans EN connective phrases.

ADR-0019 introduces ES analogues for each, parameterises the
boundary module over a `BoundaryRules` struct, and wires the
Lexicon to carry the appropriate rules.

## Decision

**Refactor `hyphae_surface::boundary` so its rules are
parameterised by a `BoundaryRules` struct. Add `ENGLISH` and
`SPANISH` constants. The `Lexicon` carries a `&'static
BoundaryRules` pointer; `baseline_en()` wires `ENGLISH`,
`baseline_es()` wires `SPANISH`. The realizer's
`pick_with_smoothing` and the scorer's
`compute_boundary_smoothness` both consult `lexicon.
boundary_rules()` to pick the right language's rules.**

### `BoundaryRules` struct

```rust
pub struct BoundaryRules {
    pub definite_determiners:  &'static [&'static str],
    pub indefinite_determiners: &'static [&'static str],
    pub anaphor_tails:         &'static [&'static str],
    pub stopwords:             &'static [&'static str],
    pub same_subject_markers:  &'static [&'static str],
}

impl BoundaryRules {
    pub const ENGLISH: Self = Self { … };
    pub const SPANISH: Self = Self { … };
}
```

Five fields cover every language-specific surface in the
module:

- `definite_determiners` — drove Rule 1 in EN (anaphor before
  definite-determiner quote).
- `indefinite_determiners` — informational; not used by Rule 1
  but populated for future use.
- `anaphor_tails` — the tails Rule 1 matches in connective
  phrase suffixes.
- `stopwords` — the filter for "content" vs "function" tokens
  in `BoundarySignal::extract` (drives `initial_token` and
  `final_token`, which Rule 3 compares).
- `same_subject_markers` — the substring set
  `is_continuation_of_same_subject` scans connective phrases
  for (Rule 2 preference).

### `BoundarySignal` extraction

The current signature `BoundarySignal::extract(body: &str)`
becomes a backwards-compat shim that defaults to
`BoundaryRules::ENGLISH`. The new entrypoint:

```rust
impl BoundarySignal {
    pub fn extract_with_rules(body: &str, rules: &BoundaryRules) -> Self { … }
    pub fn extract(body: &str) -> Self {
        Self::extract_with_rules(body, &BoundaryRules::ENGLISH)
    }
}
```

The shim keeps every existing call site working without
modification; new code that knows the language calls
`extract_with_rules` directly.

The same shim pattern applies to `should_exclude` and
`is_continuation_of_same_subject` — old signatures preserved,
new `_with_rules` variants added.

### `Lexicon` carries the rules

```rust
pub struct Lexicon {
    entries: Vec<Connective>,
    boundary_rules: &'static BoundaryRules,
}

impl Lexicon {
    pub fn empty() -> Self {
        Self { entries: Vec::new(), boundary_rules: &BoundaryRules::ENGLISH }
    }
    pub fn baseline_en() -> Self {
        Self {
            entries: connective_data::baseline_en_data(),
            boundary_rules: &BoundaryRules::ENGLISH,
        }
    }
    pub fn baseline_es() -> Self {
        Self {
            entries: connective_data_es::baseline_es_data(),
            boundary_rules: &BoundaryRules::SPANISH,
        }
    }
    pub fn boundary_rules(&self) -> &'static BoundaryRules {
        self.boundary_rules
    }
}
```

`Lexicon::empty()` defaults to ENGLISH for the v0.2 backwards-
compatible behaviour. A future ADR can introduce a
`Lexicon::empty_with_rules(&BoundaryRules)` constructor if the
empty-lexicon use case grows beyond tests.

### Realizer + scorer wiring

The realizer's `pick_with_smoothing` already has access to
`self.lexicon`. It now passes `self.lexicon.boundary_rules()`
through to the smoothing helpers it calls. The scorer's
`compute_boundary_smoothness` receives the same: it already
takes `&Lexicon`; it now reads `lexicon.boundary_rules()` and
forwards to the rule-aware extraction.

### Spanish constants

```rust
const DEFINITE_DETERMINERS_ES: &[&str] = &[
    "el", "la", "los", "las", "lo",
    "este", "esta", "estos", "estas",
    "ese", "esa", "esos", "esas",
    "aquel", "aquella", "aquellos", "aquellas",
];

const INDEFINITE_DETERMINERS_ES: &[&str] = &["un", "una", "unos", "unas"];

const ANAPHOR_TAILS_ES: &[&str] = &[
    "lo,", "lo", "eso,", "eso", "esto,", "esto",
    "ello,", "ello", "aquello,", "aquello",
];

const SAME_SUBJECT_MARKERS_ES: &[&str] = &[
    "igualmente", "asimismo", "de igual modo",
    "en la misma línea", "continuando con la línea",
    "sumando a esto",
];

const STOPWORDS_ES: &[&str] = &[
    "el","la","los","las","un","una","unos","unas","lo","le","les",
    "de","del","a","al","en","con","por","para","sobre","entre",
    "hasta","hacia","desde","según","sin",
    "y","o","pero","aunque","si","no","ni","que","como","cuando","donde",
    "este","esta","estos","estas","ese","esa","esos","esas",
    "aquel","aquella","aquellos","aquellas",
    "yo","tú","él","ella","nosotros","nosotras","vosotros","ustedes",
    "ellos","ellas","me","te","se","nos","os",
    "es","son","era","fue","ser","estar","haber","ha","han","había","habían",
];
```

Curated by Mario (native ES speaker, LATAM register). Not
exhaustive — the goal is to handle the smoothing rules
(determiner / anaphor / stopword / same-subject) accurately
for typical ES discourse, not to be a comprehensive lexicon.
Coverage gaps in stopwords manifest as content tokens
including function words; the Rule 3 overlap check produces
false positives but never false negatives (an overlap that
should fire still fires; the bug is firing on overlaps that
shouldn't, which is more conservative — exactly the RADAR
posture).

### Sensitivity audit limitation — STILL HOLDS

ADR-0019 does **not** fix the sensitivity audit's EN-baseline
limitation. The audit's baseline outputs are hardcoded EN
strings; under an ES lexicon, those strings don't trigger ES
boundary rules either. The 6/9 ES audit floor from ADR-0018
stays.

Refitting the audit to draw baselines per-lexicon is a separate
ADR (probably ADR-0020 if filed). ADR-0019 helps **real ES
output** (the realizer's smoothing picker + the scorer's
boundary check on real ES corpora); the audit's lexicon
mismatch is structurally separate.

### What this ADR explicitly does **not** do

- **Does not** fix the sensitivity audit's EN-baseline
  limitation. See ADR-0018 §"Sensitivity audit — partial
  coverage in ES".
- **Does not** add boundary rules for PT/FR/DE/IT/RU/TR/JA/ZH.
  ADR-0019 ships ES because ADR-0017 + ADR-0018 already
  established the ES path; other languages re-enter when their
  lexicons land.
- **Does not** introduce a `Provenance.language` field or
  cross-lingual composition. One realizer per language; the
  realizer's lexicon dictates the boundary rules.
- **Does not** validate the Spanish stopword / determiner sets
  against a reference corpus. The lists are hand-curated; a
  future ADR can pin them against the RAE corpus or PDTB-ES
  if precision becomes load-bearing.

## Sources

- **ADR-0017** — the ES lexicon this ADR pairs with.
- **ADR-0018 §"Known limitation — boundary smoothness"** — the
  documented gap this ADR closes for real ES output.
- **`hyphae_surface::boundary` (current state)** — the EN
  rules this ADR parameterises.
- **RAE Diccionario panhispánico de dudas** — register
  calibration for ES determiner and anaphor surface forms.

## Consequences

- Real ES output flowing through the realizer now triggers ES
  boundary rules. The smoothing picker filters ES anaphor
  before ES definite-determiner correctly.
- The scorer's `boundary_smoothness` on ES corpora (when fed
  the ES lexicon) reports honest numbers, not the inflated
  1.0 from ADR-0018.
- The audit's ES floor stays 6/9 — that limitation is
  audit-baseline-side, not boundary-rule-side.
- Existing EN call sites unchanged. The
  `BoundarySignal::extract(body)` shim defaults to ENGLISH
  rules; every existing test and call site keeps its
  semantics.
- One small risk: the ES stopword list is small (~70 words)
  and may miss content-tokens-that-are-function-words for
  some bodies. Documented as "conservative false-positive on
  Rule 3" — bias toward keeping connectives, not filtering
  them. Acceptable v0.2 trade-off.

## Cross-references

- **ADR-0017** — the Spanish lexicon.
- **ADR-0018** — the gap this ADR partially closes.
- **`hyphae_surface::Lexicon`** — the type that now carries
  `boundary_rules`.
- **ADR-0007** — the original boundary-smoothing design this
  ADR makes language-aware.
