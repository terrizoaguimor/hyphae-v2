<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0017
title: Multilingual ES re-entry — Spanish lexicon (architectural proof)
status: accepted
date: 2026-05-27
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (Fase C item-2 review)]
---

# 0017 — Multilingual ES re-entry: Spanish lexicon (architectural proof)

## Context

RFC §9 + ADR-0001 §"Multilingual lexicon beyond EN" reserved
ES, PT, FR, DE, IT, RU, TR, JA, ZH for post-v0.1 re-entry. The
v1 wave-1 stalled on multilingual because the corpus shipped
friendly-ES queries against EN-only seeds, producing the
0.993 grammaticality artefact Atlas later flagged as
unreliable.

v0.2 has cleared the re-entry bar for ES specifically:

- v0.1 substrate spec implemented end-to-end (tag `v0.1.0`,
  commit `84a69b9`).
- Honest eval harness with sensitivity audit and bucket
  coverage (ADR-0008/0009/0010).
- Performance baseline (ADR-0015).
- Architecture has not changed since chartering.

The integrator and the BDFL are native Spanish speakers
(Mario, LATAM market). ES is the natural first multilingual
extension. The other postponed languages stay postponed; each
needs its own ADR.

Two non-trivial design questions need resolving:

1. **How does the lexicon represent two languages?**
2. **How does the realizer know which lexicon to use?**

## Decision

**Add `connective_data_es.rs` with a hand-curated Spanish
connective set (~45–60 entries — architectural proof, not full
coverage). Add `Lexicon::baseline_es()` parallel to
`baseline_en()`. The realizer picks ES when it is constructed
with `SurfaceRealizer::with_lexicon(Lexicon::baseline_es())`.
No source changes to `hyphae-core::LanguageTag`, no
auto-detection, no cross-lingual composition.**

### Lexicon representation

Two **separate** baseline functions:

```rust
impl Lexicon {
    pub fn empty() -> Self { … }
    pub fn baseline_en() -> Self { … }   // existing
    pub fn baseline_es() -> Self { … }   // NEW (ADR-0017)
    pub fn add(&mut self, c: Connective) { … }
}
```

The `Connective` struct gains **no** language field. The
`Lexicon` is the language boundary: each baseline returns a
single-language set. A future ADR can introduce mixed-language
lexica if code-switching becomes a v0.3+ concern; v0.2's ES
re-entry is monolingual.

This avoids:

- Mutating the public `Connective` shape (no breaking change
  for downstream code consuming the existing struct).
- Adding language filtering to the picker's 4-level fallback
  chain (which would compound complexity).
- Inventing a default-language fallback policy.

### Realizer integration

The realizer already exposes `SurfaceRealizer::with_lexicon`.
ES deployments construct:

```rust
let realizer = SurfaceRealizer::with_lexicon(Lexicon::baseline_es());
```

No new realizer API. No changes to `register_for_fragment`,
`pick_in_context`, `pick_with_smoothing`, `evaluate_limitations`.
The picker's 4-level fallback degrades from
`(role × register × polarity × formality)` exact match through
to `role`-only — same algorithm; ES lexicon has its own
distribution across the lattice.

### Domain-tag semantics — stay English

`register_for_fragment` (ADR-0009) reads `domain_tags` and
matches against English markers: `engineering`, `code`,
`legal`, `informal`, etc. Spanish deployments **tag fragments
with the same English markers**, regardless of the body
language. The markers are semantic identifiers, not natural-
language words.

This is a deliberate v0.2 simplification:

- Single source of truth for register routing.
- No marker-translation table to maintain.
- Cross-language deployments share register heuristics.

A future ADR can extend `register_for_fragment` to recognise
Spanish (or other) marker words if integrators demand it; the
v0.2 architectural surface stays minimal.

### Coverage — proof, not production

The first ES lexicon ships ~45–60 entries across the 10 role
buckets. EN has ~250+; ES intentionally lands at ~20% of that
for v0.2.

| Role | Approx ES entries (v0.2) |
|---|---|
| Opening | 8 |
| Continuation | 10 |
| Contrast | 8 |
| Attribution | 4 |
| Closing | 5 |
| Concession | 4 |
| Causation | 6 |
| Elaboration | 4 |
| Sequence | 4 |
| Summary | 7 |
| **Total** | **~60** |

The lexicon's 4-level fallback handles the sparseness: when a
specific `(role, register, polarity, formality)` bucket is
empty in ES, the picker falls back through three relaxations
to "any phrase in the role." The output will be repetitive at
this scale (a future ADR-0018 scales to 250+); the v0.2 goal
is to **prove the architecture supports a second language**,
not to ship publication-quality ES output.

### Sources

ES discourse-connectives are drawn from public-domain
linguistic literature:

- **Cuenca, M. J. (2013)** — *Connectives and discourse
  markers in Spanish.* Hand-curated surface forms.
- **Marín, R. (2003)** — *Spanish discourse markers as
  pragmatic functions.* Register variants.
- **Brucart, J. M. (2002)** — *Spanish concessive
  connectives.* Concession surface forms.
- **RAE Diccionario panhispánico de dudas** — register and
  formality calibration for ambiguous cases.

Every entry is **hand-curated EN-native ↔ ES-native pairing
by Mario** (the integrator and a Spanish native speaker). No
machine translation. No automated alignment.

### What this ADR explicitly does **not** do

- **Does not** extend the eval corpus with ES queries. The
  corpus stays EN-only for v0.2; ADR-0018 is the next slot
  for ES eval.
- **Does not** add a `LanguageTag::Spanish` variant to
  `hyphae-core`. `LanguageTag::Other("es")` works today
  without a breaking change to the enum.
- **Does not** auto-detect language from input text. The
  realizer's lexicon is set at construction time. Auto-
  detection requires a classifier (LLM-shape adjacent) that
  v0.2 explicitly avoids.
- **Does not** support mixed-language compositions. One
  lexicon per realizer instance. Code-switching (a fragment
  body in ES, connective tissue in EN) is a v0.3+ concern.
- **Does not** translate `domain_tags`. ES integrators still
  tag fragments with English markers ("engineering",
  "legal", "informal", etc.).
- **Does not** scale the ES lexicon to EN parity. v0.2 ships
  the architectural pattern at ~60 entries; ADR-0018 (when
  filed) takes it to ~250+.
- **Does not** re-enter PT, FR, DE, IT, RU, TR, JA, ZH.
  Those stay postponed; each is its own ADR with its own
  curated dataset.

## Consequences

- v0.2 ships with one new module
  (`connective_data_es.rs` ~150 LOC of `add()` calls) and one
  new `Lexicon` constructor.
- ES deployments can compose end-to-end: the substrate is
  language-agnostic (fragments are opaque bodies); the
  realizer is the only language-aware component, and its
  language is set at construction.
- The lexicon's 4-level fallback gracefully handles the
  sparseness — the realizer still produces well-formed ES
  output even where the `(role, register, polarity,
  formality)` exact bucket is empty.
- The eval harness still runs against EN corpus only.
  Future ADR-0018 adds an ES corpus + ES-aware scorer
  invariants.
- ADR-0001 postponed-multilingual entry shrinks from "9
  languages" to "8 languages": PT, FR, DE, IT, RU, TR, JA,
  ZH.

## Cross-references

- **ADR-0001 §"Multilingual lexicon beyond EN"** — the
  postponement this ADR partially redeems.
- **ADR-0005** — the lexicon scale + context-aware picker
  whose architecture this ADR proves at second-language
  scale.
- **ADR-0009** — the domain-tag convention this ADR
  intentionally keeps English-only.
- **RFC §9** — the postponed-multilingual entry whose ES
  slot this ADR fills.
- **`hyphae_core::LanguageTag`** — left unmodified; future
  ADRs may add named variants.
