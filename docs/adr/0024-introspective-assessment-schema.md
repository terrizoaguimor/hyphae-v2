<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0024
title: IntrospectiveAssessment schema re-entry
status: accepted
date: 2026-05-27
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (Fase C item-9 review)]
---

# 0024 — IntrospectiveAssessment schema re-entry

## Context

ADR-0001 §"Postponed surface schemas" reserved four schemas
for post-v0.1 ADRs. ADR-0016 redeemed `Summary`; ADR-0023
redeemed `ComparativeAnalysis`; ADR-0024 redeems
`IntrospectiveAssessment`. The remaining one
(`NarrativeArc`) requires temporal-ordering shape projection
beyond v0.2 scope; stays postponed.

The motivating shape: callers occasionally want the substrate
to **assess what it knows** — to surface stored fragments
*while hedging its own certainty about them*. The existing
schemas all assume the substrate is presenting facts; none
of them are shaped around "here is what I have, with
acknowledged limitations". The closest existing surface is
the ethics-driven `HighConfabRisk` limitation, but that fires
on confab risk threshold — not on a caller's general request
for a reflective answer.

`Intent::Reflect` is the API for "give me your assessment,
not just your data". The schema realises that by **forcing
Concession-role connectives** between fragments — phrases that
carry epistemic hedging ("Granted,", "Admittedly,", "To be
fair,", "Hay que reconocer,"). The substrate surfaces each
fragment with explicit acknowledgment that the data has
limits.

## Decision

**Add `SchemaId::IntrospectiveAssessment` and `Intent::Reflect`.
The realizer overrides the cascade-shape's per-step role to
`ConnectiveRole::Concession` for every inter-fragment slot.
The closing slot is shared with `SchemaId::Summary` and
`SchemaId::ComparativeAnalysis` (`ConnectiveRole::Summary`).**

### Slot structure

```
[Opening connective] "[body of fragment 1]"
  [forced Concession connective] "[body of fragment 2]"
    …
  [forced Concession connective] "[body of fragment N]"
[Summary closing — e.g. "Overall,"]
```

Compared to other schemas:

| Slot | Dialogue | Assertion | Summary | Compare | **Reflect** |
|---|---|---|---|---|---|
| Opening | `Opening` | `Opening` | `Opening` | `Opening` | `Opening` |
| Inter-fragment | shape-derived | shape-derived | shape-derived | forced `Contrast` | **forced `Concession`** |
| Per-quote prefix | none | `Attribution` | none | none | none |
| Closing | `Closing` | `Closing` | `Summary` | `Summary` | `Summary` |

### Why force Concession regardless of context

ADR-0023 established the pattern: schema-level intent overrides
content-level signal. For Reflect, the caller asked for
introspection — that intent should be visible in every
inter-fragment connection, not only when the cascade-shape's
polarity rules happen to suggest Concession.

The lexicon's Concession role
(`hyphae_surface::ConnectiveRole::Concession`) ships with EN
+ ES entries (`Granted,`, `Admittedly,`, `To be fair,`,
`Of course,`, `Hay que reconocer,`, `Cabe admitir,`, etc.).
No new lexicon work required.

### Small working set behaviour

For 1-fragment working sets there is no inter-fragment slot
to fill. The realizer emits Opening + the single quote +
Summary closing. Functionally equivalent to a Summary-shape
with one fragment. **Honest passthrough** per the ADR-0016 /
ADR-0023 precedent — no silent downgrade to DialogueReply.

For 2-fragment working sets the introspective form fires
naturally: opening + quote_1 + Concession + quote_2 + Summary.
This is the canonical case for ADR-0024.

For 3+ fragments the schema generalises with chained
Concession (every inter-fragment slot is Concession). The
chained form reads as repeated hedging — appropriate when the
caller's intent is genuine reflection, redundant otherwise.
The caller chose `Intent::Reflect`; the schema honours that.

### What this ADR explicitly does **not** do

- **Does not** add new lexicon entries. Concession (EN +
  ES) and Summary (EN + ES) are both populated.
- **Does not** modify the ethics-driven limitation triggers
  (`HighConfabRisk`, `EthicallySensitive`). Those fire by
  content; Reflect fires by intent. The two surfaces compose
  naturally — a Reflect query whose working set ALSO trips
  `HighConfabRisk` emits Concession-tissue prose AND the
  acknowledgment line.
- **Does not** introduce a uncertainty-quantification metric
  for fragments. v0.2 hedges by linguistic register, not by
  measured confidence interval. A future ADR can introduce
  numerical certainty alongside if empirical use demands.
- **Does not** re-enter `NarrativeArc`. That schema requires
  temporal-ordering shape projection — outside v0.2 scope.
  Stays postponed.

## Sources

- **ADR-0001 §"Postponed surface schemas"** — the postponement
  this ADR redeems.
- **ADR-0005 §"Role taxonomy"** — the source of
  `ConnectiveRole::Concession`.
- **ADR-0016 / ADR-0023** — the prior schema re-entry
  patterns this ADR mirrors.
- **`hyphae_surface::SchemaId`** — the type this ADR extends.

## Consequences

- The realizer emits introspective compositions when the
  caller picks `Intent::Reflect`. Concession role is always
  used for inter-fragment connectives; closing is always
  Summary role.
- Two new corpus queries (`reflect-001`, `reflect-002`) bring
  the EN corpus to 32 queries.
- `Intent` + `SchemaId` enums stay `#[non_exhaustive]`; new
  variants don't break downstream non-exhaustive matchers.
- Postponed-schema list shrinks by one. **Remaining**:
  `NarrativeArc` only. Requires temporal-ordering shape
  projection that is outside v0.2 scope.
- Lexicon-derived audit (ADR-0020) covers the new schema
  automatically because the audit's schema_match arm swaps
  ANY pair of `SchemaId` variants.

## Cross-references

- **ADR-0001** — postponement list this ADR redeems.
- **ADR-0005** — Concession role consumed.
- **ADR-0016 / ADR-0023** — parallel re-entry patterns.
- **`hyphae_surface::realizer`** — implementation site.
