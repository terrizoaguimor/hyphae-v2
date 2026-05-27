<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0023
title: ComparativeAnalysis schema re-entry
status: accepted
date: 2026-05-27
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (Fase C item-8 review)]
---

# 0023 — ComparativeAnalysis schema re-entry

## Context

ADR-0001 §"Postponed surface schemas" reserved four schemas
for post-v0.1 ADRs. ADR-0016 redeemed `Summary`; ADR-0023
redeems `ComparativeAnalysis`. The remaining two
(`IntrospectiveAssessment`, `NarrativeArc`) stay postponed.

The motivating shape: the existing schemas all produce
"single-direction" prose (a dialogue reply, an assertion, a
summary). None of them surface **contrast as the structural
intent of the composition**. When the caller wants to compare
two services / two metrics / two strategies, the cascade-shape
projection picks `Contrast` only when valences are opposed
strongly enough — but the caller may want comparative framing
even when the fragments aren't strongly opposed.

The existing infrastructure already supports this with zero
new lexicon work:

- `ConnectiveRole::Contrast` exists with ample EN entries
  (ADR-0005's hard/soft contrast set) and ES entries
  (ADR-0017 + ADR-0021 + ADR-0022 contrast set).
- `ConnectiveRole::Summary` provides the closing slot (the
  comparative-judgment shape) — already shared with ADR-0016.

ADR-0023 is structural: a new schema variant + intent variant
+ a two-line override in the realizer's role-picking logic.

## Decision

**Add `SchemaId::ComparativeAnalysis` and `Intent::Compare`.
The realizer overrides the cascade-shape's per-step role to
`ConnectiveRole::Contrast` for every inter-fragment connective
when the schema is `ComparativeAnalysis`. The closing slot is
shared with `SchemaId::Summary` (draws from
`ConnectiveRole::Summary`).**

### Slot structure

```
[Opening connective] "[body of fragment 1]"
  [forced Contrast connective] "[body of fragment 2]"
    …
  [forced Contrast connective] "[body of fragment N]"
[Summary closing — e.g. "Overall," / "On balance,"]
```

Compared to the other schemas:

| Slot | Dialogue | Assertion | Summary | **Compare** |
|---|---|---|---|---|
| Opening | `Opening` | `Opening` | `Opening` | `Opening` |
| Inter-fragment | shape-derived | shape-derived | shape-derived | **forced `Contrast`** |
| Per-quote prefix | none | `Attribution` | none | none |
| Closing | `Closing` | `Closing` | `Summary` | `Summary` |

### Why force Contrast regardless of valence delta

The cascade-shape projection picks `Contrast` role only when
adjacent fragments' valences differ by more than the threshold
(per ADR-0006's polarity rules). For a comparative
composition, the **schema** carries the intent ("compare these
two things"), not the **content**. Two services that both
succeeded but along different axes still warrant comparative
framing if the caller asks for one.

The realizer respects that schema-level intent. Cascade-shape
projection's role suggestion is overridden; the lexicon picker
gets `Contrast` even when valence delta is small. Picker's
context still adapts (polarity defaults to `ContrastSoft` for
aligned valences, `ContrastHard` for opposed) — so the
phrasing is contextually correct within the Contrast slot.

### Small working set behaviour

For 1-fragment working sets there is no inter-fragment slot to
fill. The realizer emits the Opening + the single quote + the
Summary closing. Functionally equivalent to a Summary schema
with one fragment. **Honest passthrough** per ADR-0016's same
rule — no silent downgrade to DialogueReply.

For 2-fragment working sets the comparative form fires
naturally: opening + quote_1 + Contrast + quote_2 + Summary.
This is the canonical case for ADR-0023.

For 3+ fragments the schema generalises: every inter-fragment
slot is Contrast. The reader gets a chained-contrast prose
that reads as "X. But Y. By contrast Z. Overall …". The
chain can feel forced at higher fragment counts; the caller
chose `Intent::Compare`, the schema honours that.

### What this ADR explicitly does **not** do

- **Does not** add new lexicon entries. `Contrast` (EN ~30+,
  ES ~17+) and `Summary` (EN ~11, ES ~7+) are both already
  populated.
- **Does not** introduce pair-aware composition (e.g. "first
  half vs second half" structural pairing). The chained-
  contrast linear walk is v0.2 minimum; future ADR can add
  pairing logic if empirical use demands.
- **Does not** add an `IntrospectiveAssessment` or
  `NarrativeArc` schema. Those stay postponed.
- **Does not** change the `ComposerSchemaPrior` learning
  surface beyond gaining a fourth slot it can shape priors
  over. ADR-0002's refinable-parameter contract is preserved.
- **Does not** affect the sensitivity audit. The audit
  baselines + mutations are schema-agnostic past
  `schema_match_rate`, and the schema_match arm already
  handles any `SchemaId` variant via swap.

## Sources

- **ADR-0001 §"Postponed surface schemas"** — the postponement
  this ADR partially redeems.
- **ADR-0005 §"Role taxonomy"** — the source of
  `ConnectiveRole::Contrast` + its hard/soft polarity split.
- **ADR-0006 §"Projection algorithm"** — the cascade-shape
  projection this ADR overrides at schema-level.
- **ADR-0016** — the prior schema re-entry whose
  implementation pattern ADR-0023 mirrors.
- **`hyphae_surface::SchemaId`** — the type this ADR extends.

## Consequences

- The realizer emits comparative compositions when the caller
  picks `Intent::Compare`. Contrast role is always used for
  inter-fragment connectives; closing is always Summary role.
- Two new corpus queries (`compare-001`, `compare-002`) bring
  the EN corpus to 30 queries. The schema_match_rate
  sensitivity coverage extends automatically.
- The `Intent` and `SchemaId` enums are `#[non_exhaustive]` so
  downstream non-exhaustive matchers stay valid; only
  exhaustive `match` callers need a new arm.
- Postponed-schema list shrinks by one. Remaining:
  `IntrospectiveAssessment` (self-reference shape),
  `NarrativeArc` (temporal-ordering shape). Each requires its
  own ADR + empirical motivation when filed.
- The `lexical_diversity` and `role_coverage` eval dimensions
  benefit because Compare queries exercise the Contrast +
  Summary buckets that DialogueReply queries don't.

## Cross-references

- **ADR-0001** — the postponement list this ADR redeems.
- **ADR-0005** — the Contrast role this ADR consumes
  uniformly.
- **ADR-0006** — the cascade-shape projection this ADR
  overrides at schema-level.
- **ADR-0016** — the parallel schema re-entry pattern.
- **`hyphae_surface::realizer`** — the implementation site.
