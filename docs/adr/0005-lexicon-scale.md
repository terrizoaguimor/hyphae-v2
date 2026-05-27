<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0005
title: Lexicon scale — ~300 EN connectives with register + polarity metadata
status: accepted
date: 2026-05-26
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (v0.1 implementation review)]
---

# 0005 — Lexicon scale: ~300 EN connectives with register + polarity metadata

## Context

The v0.1 baseline shipped 20 connective phrases in 5 roles (Opening,
Continuation, Contrast, Attribution, Closing). The smoke output:

> *Drawing from working memory, "fragment a" Building on it,
> "fragment b" Adding to the picture, "fragment c" That is what
> working memory holds on this.*

reads as **rigidly template-driven**. The user notes the system is
reciting connectives, not composing. Mario flagged this directly:
"esto funciona pero es rígido." That rigidity is **not** an
architectural defect — it is a calibration gap. The realizer
correctly emits fragment quotation + connective tissue; the
connective tissue is just very thin.

The fix is **data**, not architecture: scale the connective
lexicon by an order of magnitude (20 → ~300), index entries by
**register** and **polarity** so the picker can choose
context-appropriate phrasing instead of cycling `seq % n` through
a flat list, and extend the role taxonomy to capture relations
the current 5 roles cannot express (concession, causation,
elaboration, sequence, summary).

The architectural commitment is preserved: no LLM in the cognition
path. The output still quotes fragment bodies verbatim. The lexicon
expansion is in the **infrastructure path** — curated data the
realizer selects from, not language synthesised from a model.

## Decision

**Expand the connective lexicon to ~300 English entries organised
by `(role, register, polarity, formality)`. Upgrade the picker to
context-aware selection. Extend the role taxonomy from 5 to 10.**

### New `Connective` struct

```rust
pub struct Connective {
    pub phrase: String,
    pub role: ConnectiveRole,
    pub register: Register,
    pub polarity: Polarity,
    pub formality: Formality,
}
```

Each entry carries the metadata the picker filters on. Phrases
remain plain strings — no embedded templates, no slot syntax. The
realizer concatenates the phrase between quotes without
transformation.

### `Register` enum (4 variants)

- `Neutral` — works in any context. The bulk of the v0.1 baseline.
- `Formal` — declarative, sober, distant. "Furthermore," "It
  follows that," "Consequently,".
- `Conversational` — informal, direct, second-person-aware.
  "Right, and..." "Anyway," "On top of that,".
- `Technical` — engineering / scientific register. "In addition,"
  "Subsequently," "It can be observed that,".

### `Polarity` enum (5 variants)

- `Continuation` — second fragment extends the first in same
  direction.
- `ContrastSoft` — second fragment qualifies or hedges the first.
- `ContrastHard` — second fragment opposes the first.
- `Concession` — acknowledges counter-evidence ("although,"
  "granted,").
- `Neutral` — polarity not relevant (openings, closings,
  attributions).

### `Formality` enum (3 variants)

- `Low` — colloquial. "And..." "Plus,".
- `Mid` — everyday written register. The baseline default.
- `High` — formal written. "Moreover," "Notwithstanding,".

### Role taxonomy: 5 → 10

The five baseline roles stay; the picker gains five additional
roles for relations the current realizer cannot express:

- `Concession` — "Although," "Granted," "Admittedly,". Distinct
  from `Contrast`: concession yields ground, contrast asserts
  opposition.
- `Causation` — "Because," "Therefore," "Consequently,".
  Realizer picks this when the cascade shows a parent_id chain
  between adjacent fragments (the v0.1 cascade-shape-driven
  composition that ADR-0006 will formalise).
- `Elaboration` — "Specifically," "In particular," "To wit,".
  Same valence + same source_subsystem → elaboration.
- `Sequence` — "First," "Then," "Finally,". Three or more
  fragments with similar saliency and the realizer wants to
  enumerate.
- `Summary` — "In summary," "Overall," "On balance,". A second
  closing role for compositions with three or more fragments where
  a final synthesis line reads less rigid than the baseline
  "That is the substance available."

### Context-aware picker

The current `Lexicon::pick(role, seq)` stays as the default
(backward-compatible). A new method:

```rust
pub fn pick_in_context(
    &self,
    role: ConnectiveRole,
    context: &PickContext,
    seq: usize,
) -> &str
```

filters by register + polarity + formality match, then picks
`seq % matching.len()`. Falls back through three relaxations:

1. Exact `(role, register, polarity, formality)` match.
2. Drop formality match; keep role + register + polarity.
3. Drop register match; keep role + polarity.
4. Final fallback: any phrase in the role (the baseline behaviour).

The fallback chain guarantees the picker never panics on missing
data and degrades gracefully — the integrator can add narrow
custom phrases without worrying about leaving the baseline buckets
non-empty.

### `PickContext`

Built by the realizer from the adjacent fragment pair + the
caller's intent:

```rust
pub struct PickContext {
    pub register: Register,
    pub polarity: Polarity,
    pub formality: Formality,
}
```

The realizer derives `register` from the working set's
`domain_tags` heuristically (presence of "engineering" / "code" /
"systems" → `Technical`; "informal" / "conversation" →
`Conversational`; default `Neutral`). `polarity` is the valence
delta between adjacent fragments (rule lifted from the current
`pick_inter_fragment_role`). `formality` defaults to `Mid`; the
integrator overrides per deployment.

### Why ~300, not 500 or 1000

The 20 → 300 jump is the **smallest scale change that exits
template-rigid territory**. Above ~300, marginal output diversity
gains diminish quickly — the next-order improvement is not more
connectives, it is **cascade-shape-driven composition** (ADR-0006:
the composer's working-set order is informed by the cascade
topology rather than retrieval order).

300 is also a number a single curator can review in a single
session — auditable per ADR-0001 §"Curated dependencies". Pulling
in PDTB's full 100 connectives × 5-way register variants would
require an empirical-calibration ADR with sources, licensing
review, and a quality-control pass; 300 hand-curated entries
covering the discourse relations a v0.1 dialogue substrate exhibits
is the right scope.

## Sources

The connective list draws on three public-domain discourse-relation
corpora:

- **Penn Discourse Treebank (PDTB) 3.0 taxonomy** — public
  taxonomy paper (Webber et al. 2019); we use the **list of
  connective surface forms** (not the corpus annotations).
- **Rhetorical Structure Theory (RST) relations** — Mann &
  Thompson 1988 plus the RST-DT corpus's relation labels
  (Carlson, Marcu, Okurowski 2003) — the relation names map to
  our roles; the surface forms are the canonical English
  phrasings.
- **Random House Webster's Roget thesaurus** for register
  variants (formal / conversational / technical) of the same
  semantic role.

Every entry in the v0.1 baseline is **hand-curated English**.
There is no automated extraction; there is no NLP model in the
loop. Adding ES (per RFC §9) is the same job for the ES
discourse-connectives literature (Cuenca, Marín, Brucart) —
deferred to the multilingual-re-entry ADR.

## Consequences

- The smoke output stops sounding like a template. The realizer
  has variety per role × register × polarity to draw from.
- The picker's complexity grows from `seq % n` to a fallback
  chain — but the chain is bounded (4 relaxation levels) and
  deterministic.
- The realizer's `pick_inter_fragment_role` upgrades from
  `Continuation` / `Contrast` to the full 5-polarity surface.
- The lexicon data lives in a separate `connective_data.rs`
  module so `connective.rs` stays focused on types + picker.
- The eval harness's `connective_hygiene` scorer expands its
  list of doubled-connective patterns to cover the new role
  surface. The corpus's `must_not_fire` expectations stay valid.
- v0.1's clippy / fmt / test invariants stay green.
- ADR-0006 (cascade-shape-driven composition) is now unblocked:
  the role surface supports the relations the cascade topology
  can project to prose.

## Cross-references

- **ADR-0001 §"Surface scope"** — the 2-schema scope this ADR
  scales connectives WITHIN. Not adding schemas; adding
  expressive range to the existing two.
- **ADR-0004 §"What the scaffold preserves vs gives up"** — the
  embedding scaffold gave up paraphrase invariance; the lexicon
  expansion compensates by giving the realizer more surface
  variety to draw from once retrieval lands on the right
  fragments.
- **RFC v1-living §5.2** — composition uses fragment quotation +
  connective tissue. This ADR scales the connective tissue
  surface; quotation stays verbatim.
- **`hyphae_surface::Lexicon`** — the type this ADR extends.
