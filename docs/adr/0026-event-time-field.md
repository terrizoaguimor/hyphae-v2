<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0026
title: event_time field on CognitiveFragment
status: accepted
date: 2026-05-27
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (post-Fase-C cleanup)]
---

# 0026 — `event_time` field on `CognitiveFragment`

## Context

ADR-0025 added the `NarrativeArc` schema, which sorts fragments
chronologically by `created_at`. The ADR documented an honest
v0.2 limitation:

> `CognitiveFragment::created_at` is the **substrate ingestion
> timestamp**, not necessarily the underlying event time. For
> most callers the two correlate (events are ingested as they
> happen). For backfill / replay / batch-import scenarios they
> diverge.

ADR-0026 closes that gap with the smallest honest API: an
optional `event_time` field on `CognitiveFragment`. When the
caller knows the event happened at a time distinct from
ingestion, they set it; the `NarrativeArc` schema reads it.
The common case (ingest-time = event-time) stays unchanged.

## Decision

**Add `event_time: Option<SystemTime>` to `CognitiveFragment`.
Add `CognitiveFragment::with_event_time` builder and
`CognitiveFragment::narrative_time()` resolver method. The
`NarrativeArc` schema sorts by `narrative_time()` —
`event_time` when set, falling back to `created_at`.**

### API additions

```rust
impl CognitiveFragment {
    /// Set the event time the body describes.
    pub const fn with_event_time(mut self, event_time: SystemTime) -> Self;

    /// Resolve the effective time for narrative ordering:
    /// `event_time` when set, otherwise `created_at`.
    pub fn narrative_time(&self) -> SystemTime;
}
```

The realizer's `NarrativeArc` sort now reads
`narrative_time()` instead of `created_at` directly. No other
schema is affected.

### Backward compatibility

- `event_time: Option<SystemTime>` with `#[serde(default)]` —
  existing serialized fragments deserialize unchanged (the
  default is `None`).
- `CognitiveFragment::new` initialises `event_time = None`.
- Existing callers who construct via the struct literal
  (`Predictive::process` and `Reward::process` in
  `hyphae-subsystems`) propagate the source fragment's
  `event_time` to derived fragments — the same pattern they
  use for `depth_level` and `language`.

### Non-event_time path stays identical

For callers who never set `event_time`:
`narrative_time()` returns `created_at` unchanged, the
realizer's sort uses the same key as ADR-0025 specified. The
existing `narrative_arc_sorts_by_created_at_and_uses_sequence_role`
test still passes — the new field is purely additive.

### What this ADR explicitly does **not** do

- **Does not** populate `event_time` automatically from
  fragment body content. Body-parsing for date extraction is
  a future ADR if empirical demand justifies it (the model
  required would be LLM-shape — at odds with v0.2's no-LLM-
  in-cognition-path commitment).
- **Does not** modify ingest / recall paths to expose
  `event_time` in their public API. Callers who want to set
  it use the `with_event_time` builder before passing the
  fragment to the substrate.
- **Does not** validate `event_time` against `created_at`. A
  future event-time later than ingest-time is valid (e.g.
  scheduled / planned events); a past event-time is the
  common backfill case. No constraint, no validation.
- **Does not** affect the cascade graph, ethics evaluation,
  learning loop, or any other subsystem. The field is
  consumed only by the realizer's `NarrativeArc` sort.

## Sources

- **ADR-0025 §"v0.2 caveat — `created_at` vs event-time"** —
  the documented limitation this ADR closes.
- **`hyphae_core::CognitiveFragment`** — the struct extended.

## Consequences

- `CognitiveFragment` gains one optional field. No API
  breakage; existing struct-literal call sites that don't
  forward `event_time` continue to work, except for the
  `Predictive` and `Reward` subsystems which DO construct
  fragments via struct literal — those are updated to
  propagate the source's `event_time`.
- The `NarrativeArc` schema's emission order now reflects
  event-time when callers supply it.
- Two new APIs: `with_event_time` builder + `narrative_time`
  resolver.
- One new test (`narrative_arc_prefers_event_time_when_set`)
  exercises the backfill scenario.
- Workspace 325 → 326 tests. The other 325 stay green.

## Cross-references

- **ADR-0025** — the schema this ADR refines.
- **`hyphae_core::CognitiveFragment`** — the type extended.
- **`hyphae_surface::realizer`** — consumer of the new
  resolver.
