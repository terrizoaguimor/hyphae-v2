<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0025
title: NarrativeArc schema re-entry — final v1-postponed schema
status: accepted
date: 2026-05-27
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (Fase C item-10 review)]
---

# 0025 — NarrativeArc schema re-entry: final v1-postponed schema

## Context

ADR-0001 §"Postponed surface schemas" reserved four schemas
for post-v0.1 ADRs. ADRs 0016, 0023, and 0024 redeemed
`Summary`, `ComparativeAnalysis`, and `IntrospectiveAssessment`
respectively. ADR-0025 redeems the final one: `NarrativeArc`.

The motivating shape: callers want to **tell a story** —
surface fragments in chronological order with connective
tissue that marks temporal progression ("First …, then …,
finally …"). The four existing schemas all rely on either the
caller's working-set order (DialogueReply / Assertion /
Summary) or override the role without re-ordering (Compare /
Reflect). None of them touches **fragment order**.

NarrativeArc is the first schema that overrides BOTH:

- **Re-orders the shape's steps** by `fragment.created_at`
  ascending — the realizer emits fragments chronologically
  regardless of what order the caller passed them.
- **Overrides the inter-fragment role** to
  `ConnectiveRole::Sequence` — every transition carries
  temporal-progression phrasing ("First,", "Then,",
  "Subsequently,", "Finally,").

The lexicon's Sequence role + the Summary role for closing
were already populated by ADR-0005 + ADR-0017/0021. No new
data work required.

## Decision

**Add `SchemaId::NarrativeArc` and `Intent::Narrate`. When
this schema fires, the realizer:**

1. **Clones the shape's steps and sorts them by
   `fragment.created_at` ascending** before the emission walk.
2. **Overrides the per-step role to `ConnectiveRole::Sequence`**
   for every inter-fragment connective.
3. **Uses the shared `ConnectiveRole::Summary` closing slot**
   (same as Summary / Compare / Reflect).

### Slot structure

```
[Opening connective] "[earliest fragment by created_at]"
  [forced Sequence connective] "[second-earliest]"
    …
  [forced Sequence connective] "[latest]"
[Summary closing — e.g. "All in all," / "Overall,"]
```

Compared to the other schemas:

| Slot | Dialogue | Assertion | Summary | Compare | Reflect | **Narrate** |
|---|---|---|---|---|---|---|
| Fragment order | working-set | working-set | working-set | working-set | working-set | **created_at ascending** |
| Opening | `Opening` | `Opening` | `Opening` | `Opening` | `Opening` | `Opening` |
| Inter-fragment | shape-derived | shape-derived | shape-derived | forced `Contrast` | forced `Concession` | **forced `Sequence`** |
| Per-quote prefix | none | `Attribution` | none | none | none | none |
| Closing | `Closing` | `Closing` | `Summary` | `Summary` | `Summary` | `Summary` |

### v0.2 caveat — `created_at` vs event-time

`CognitiveFragment::created_at` is the **substrate ingestion
timestamp**, not necessarily the underlying event time. For
most callers the two correlate (events are ingested as they
happen). For backfill / replay / batch-import scenarios they
diverge.

This is a documented v0.2 limitation. A future ADR can
introduce an explicit `event_time: Option<SystemTime>` field
on `CognitiveFragment` that the NarrativeArc schema reads with
fallback to `created_at`. v0.2 ships with `created_at` only —
the field exists on every fragment, the sort is unambiguous,
and the caller who needs event-time ordering can pre-sort
their working set before passing it (the schema will then
preserve the pre-sorted order because `created_at` is broken
as tie-breaker only).

### Why re-order in the realizer (not the caller)

Two reasons:

1. **Schema semantics**: NarrativeArc *promises* chronological
   emission. Pushing the responsibility to the caller breaks
   that promise — the schema becomes "Sequence connectives in
   working-set order", which is a weaker contract.

2. **Cascade-shape compatibility**: the cascade-shape projection
   (ADR-0006) orders by topology (anchor first, supports next).
   For most schemas that order is correct; for NarrativeArc it
   actively conflicts with the intent. The realizer must
   override to make the schema meaningful.

The override cost is `O(N log N)` for N = working_set size.
N is bounded by the v0.2 `working_set_size = 7` default in
`CascadeParams`, so the sort is trivial.

### What this ADR explicitly does **not** do

- **Does not** add an `event_time` field to
  `CognitiveFragment`. Documented future ADR.
- **Does not** introduce multi-stream interleaving (e.g.
  parallel narrative threads merging). The shape walks one
  chronological sequence; a future ADR can introduce
  pair-aware narrative if empirical use demands.
- **Does not** add new lexicon entries. Sequence (EN + ES)
  and Summary (EN + ES) are populated.
- **Does not** modify the cascade-shape projection. The
  re-order happens AFTER `shape_from_working_set` /
  `shape_from_cascade` produces the shape — the override is
  schema-level, not projection-level.
- **Does not** affect any other schema. Only NarrativeArc
  triggers the temporal sort.

## Sources

- **ADR-0001 §"Postponed surface schemas"** — the postponement
  this ADR finally closes.
- **ADR-0005 §"Role taxonomy"** — the source of
  `ConnectiveRole::Sequence`.
- **ADR-0016 / ADR-0023 / ADR-0024** — the prior schema
  re-entry patterns this ADR mirrors (with the addition of
  fragment re-ordering, novel to ADR-0025).
- **`hyphae_core::CognitiveFragment::created_at`** — the
  timestamp field this ADR reads.

## Consequences

- The realizer emits chronological narratives when the caller
  picks `Intent::Narrate`. Fragments are sorted by
  `created_at` regardless of working-set order.
- Two new corpus queries (`narrate-001`, `narrate-002`) bring
  the EN corpus to 34 queries.
- `SchemaId` and `Intent` enums stay `#[non_exhaustive]`; new
  variants don't break downstream non-exhaustive matchers.
- **Postponed-schema list reaches zero**. All four v1-postponed
  schemas (Summary, Compare, Reflect, Narrate) are now
  re-entered. Further schemas require their own ADRs with
  empirical motivation.
- ADR-0020's audit covers the new schema automatically (audit's
  schema_match arm swaps any pair of `SchemaId` variants).
- Performance: `O(N log N)` sort per NarrativeArc realization,
  where N ≤ 7 in v0.2 default. Trivial.

## Cross-references

- **ADR-0001** — postponement list this ADR closes.
- **ADR-0005** — Sequence role.
- **ADR-0016 / ADR-0023 / ADR-0024** — re-entry pattern.
- **`hyphae_surface::realizer`** — implementation site.
- **Future ADR (reserved)** — `event_time` field on
  CognitiveFragment for callers whose ingestion time diverges
  from event time.
