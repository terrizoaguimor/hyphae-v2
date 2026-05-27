<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0016
title: Summary schema re-entry — third v0.2 surface schema
status: accepted
date: 2026-05-27
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (Fase C item-1 review)]
---

# 0016 — Summary schema re-entry: third v0.2 surface schema

## Context

RFC §5.1 and `docs/adr/0001-fresh-from-v1.md`
§"Postponed surface schemas" reserved four schemas for
post-v0.1 re-entry: `IntrospectiveAssessment`, `NarrativeArc`,
`ComparativeAnalysis`, `SyntheticSummary`. Each requires an
explicit ADR demonstrating empirical need.

After v0.1 closure (tag `v0.1.0`, commit `84a69b9`) and the
Fase B baselines, three observations motivate re-entering one
schema now:

1. **The smoke + chat outputs are coherent on dialogue
   queries but feel mechanical on multi-fragment
   compositions.** The realizer can quote three observations
   and bridge them with Causation connectives ("Drawing from
   working memory, X. Therefore, Y. As a result, Z. That is
   what working memory holds on this."), but the closing
   "That is what working memory holds on this." is the same
   regardless of whether the user asked for a status check or
   a multi-fact synthesis. The closing is a DialogueReply
   slot; the Summary slot is unreached.

2. **ADR-0005 already populated the lexicon's `Summary` role.**
   `connective_data.rs:1490+` adds 11 entries: "In summary,",
   "Overall,", "On balance,", "Taking it together,",
   "Putting it together,", "Bringing the threads together,",
   "Across the working set,", "The shape of it is that,",
   "The picture overall is,", "Summing up,", "All things
   considered,". The lexicon is ready; only the **schema slot
   that selects from this role** is missing.

3. **Re-entry criterion is met.** ADR-0001 set "v0.1 bet
   validated" as the bar. The v0.2 bench (ADR-0015) +
   sensitivity audit (ADR-0010) + the chat REPL (ADR-0014)
   together constitute a validated demonstration: the
   substrate runs, the realizer composes coherently, the
   scorers detect failure modes by construction. The bar is
   not "Hyphae in production"; it is "v0.1 substrate
   functional and honestly measured." That bar is cleared.

Of the four postponed schemas, **Summary is the smallest
re-entry**:

- Lexicon already populated (no new connectives).
- Schema shape is identical to `DialogueReply` except the
  closing slot uses `ConnectiveRole::Summary` instead of
  `ConnectiveRole::Closing`.
- No new intent semantics beyond a `Summarize` variant.
- No changes to ethics, learning, or the substrate state
  machine.

The other three (`IntrospectiveAssessment`, `NarrativeArc`,
`ComparativeAnalysis`) require structural slot changes
(self-reference, temporal ordering, paired-contrast scaffolds)
that are larger ADRs. Summary is the right first re-entry.

## Decision

**Add `SchemaId::Summary` and `Intent::Summarize`. The realizer
emits the Summary schema with the same slot structure as
`DialogueReply` except the closing-line slot pulls from
`ConnectiveRole::Summary`.** No fall-back to DialogueReply for
small working sets — when the caller asks for a summary, they
get a summary regardless of input size; the closing connective
shape signals the schema, not the working-set count.

### Schema slot structure

```
[Opening connective] "[body of fragment 1]"
  [inter-fragment connective] "[body of fragment 2]"
    …
  [inter-fragment connective] "[body of fragment N]"
[Summary closing — e.g. "Overall,"]
```

Compared to `DialogueReply`:

| Slot | DialogueReply | Summary |
|---|---|---|
| Opening | `ConnectiveRole::Opening` | `ConnectiveRole::Opening` |
| Inter-fragment | role from cascade shape | role from cascade shape |
| Attribution per quote | (none) | (none) |
| Closing | `ConnectiveRole::Closing` | **`ConnectiveRole::Summary`** |

The change is **one line** in the realizer's closing-pick
call: select `ConnectiveRole::Summary` when
`schema == SchemaId::Summary`, else `ConnectiveRole::Closing`.

### Intent mapping

```rust
pub enum Intent {
    Dialogue,
    Assert,
    Summarize,   // NEW
}

impl Intent {
    pub fn default_schema(self) -> SchemaId {
        match self {
            Self::Dialogue   => SchemaId::DialogueReply,
            Self::Assert     => SchemaId::GroundedAssertion,
            Self::Summarize  => SchemaId::Summary,
        }
    }
}
```

`Intent` is `#[non_exhaustive]` so adding the variant is a
minor compatibility bump for downstream matchers, not a
breaking change. The `ComposerSchemaPrior` learning target
(ADR-0002) gains a third slot it can shape priors over.

### Small working set behaviour

The summary closing on a single-fragment working set produces
output like:

> Drawing from working memory, "the deploy succeeded."
> Overall,

This **does not** read as a useful summary. It reads as a
schema mismatch. Three honest options were considered:

1. **Silent downgrade**: fall back to `SchemaId::DialogueReply`
   when `working_set.len() < 3`. Rejected because it makes the
   `schema_used` field lie about the realizer's effective
   behaviour and breaks the eval harness contract.
2. **Hard error**: return `RealizationError` for `< 3`.
   Rejected because the caller may legitimately want a
   summary-shaped output regardless of size (e.g. for UI
   consistency in a "summarise this" button).
3. **Honest passthrough**: emit the Summary schema as
   requested. The unhelpful output is the caller's signal
   that their input was too small. **Chosen.**

The eval corpus's Summary queries supply ≥ 3 fragments; the
smoke and chat surfaces inherit the Intent the user picked.
This matches the v0.2 RADAR posture: do what the caller asked,
emit honest output, let the caller observe.

### Eval corpus extension

Add three Summary-flavoured queries to
`seed_corpus_en()`:

- `summary-001`: multi-domain status check (engineering +
  operations).
- `summary-002`: contrast wrapped in a summary closing
  (mixed valence working set).
- `summary-003`: shallow-cascade summary (single-direct seed
  with parent_ids empty — fires `ShallowCascade` limitation
  while still emitting Summary schema).

`Intent::Summarize` on each. Expected schema match:
`SchemaId::Summary`. The sensitivity audit (ADR-0010) needs no
changes — its baseline + mutation pairs are parameterised on
`SchemaId` and accept the new variant automatically because
`#[non_exhaustive]` matches its existing wildcard.

### What this ADR explicitly does **not** do

- **Does not** re-enter `IntrospectiveAssessment`,
  `NarrativeArc`, or `ComparativeAnalysis`. Each requires its
  own ADR with empirical motivation.
- **Does not** add a new lexicon role. `ConnectiveRole::Summary`
  exists from ADR-0005; this ADR consumes it.
- **Does not** add a fluency dimension specific to summary
  quality. The existing ADR-0008 dimensions cover the surface
  invariants (lexical diversity, role coverage, boundary
  smoothness).
- **Does not** change cascade-shape projection. The Summary
  schema reuses the shape from ADR-0006; only the closing
  slot's role changes.
- **Does not** add UI affordances in the chat REPL beyond what
  `/recall` already does. A future ADR can add `/summarise
  <cue>` if usage demands; for v0.2 the substrate API is the
  contract.

## Sources

- **RFC §5.1** — the two-schema scope this ADR extends to
  three.
- **ADR-0001 §"Postponed surface schemas"** — the re-entry
  criterion ("v0.1 bet validated") cleared by v0.2's
  measurements.
- **ADR-0002 §"Refinable parameters"** — the
  `ComposerSchemaPrior` learning surface gains a third entry.
- **ADR-0005 §"Role taxonomy: 5 → 10"** — the source of the
  `ConnectiveRole::Summary` variant and its phrases.
- **`hyphae_surface::schema::SchemaId`** — the type this ADR
  extends.

## Consequences

- The realizer can emit Summary-shaped compositions when the
  caller picks `Intent::Summarize`. Lexicon already populated.
- One new variant on each of `SchemaId` and `Intent`. Both
  `#[non_exhaustive]` so downstream `match` against the existing
  variants stays valid; only exhaustive matchers need an arm
  added.
- Eval corpus grows 25 → 28 queries. The
  `corpus_exercises_multiple_register_buckets` invariant
  remains satisfied.
- The `distinct_phrases_corpus_wide` count from ADR-0008
  rises slightly as the Summary closings get exercised.
- One new test in `schema.rs`
  (`intent_summarize_maps_to_summary`) and one in
  `realizer.rs`
  (`summary_schema_uses_summary_role_for_closing`).
- The ADR-0001 postponed list shrinks by one. The remaining
  three schemas keep their explicit re-entry-ADR requirement.

## Cross-references

- **ADR-0001 §"Postponed subsystems / postponed schemas"** —
  the postponement list this ADR partially redeems.
- **ADR-0005** — the lexicon expansion that pre-loaded the
  Summary role.
- **ADR-0008/0009/0010** — the eval discipline the new
  corpus entries inherit.
- **`hyphae_surface::Lexicon::baseline_en`** — the lexicon
  consumer the realizer now reaches for at the closing slot.
