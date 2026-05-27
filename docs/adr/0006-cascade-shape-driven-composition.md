<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0006
title: Cascade-shape-driven composition — projecting retrieval topology to discourse structure
status: accepted
date: 2026-05-26
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (v0.1 implementation review)]
---

# 0006 — Cascade-shape-driven composition

## Context

After ADR-0005 the lexicon has ten roles populated with ~250 phrases
across registers. But the realizer still **walks the working set
linearly**: it picks `Continuation` between every pair of fragments
unless their valences directly oppose, in which case it picks
`Contrast`. The other five roles
(`Causation`, `Elaboration`, `Sequence`, `Summary`, `Concession`)
are populated in the lexicon but **never selected by the realizer**.

The result is that the cascade engine — which already produces rich
topological information about why each fragment is in the working
set (`parent_id`, `hops_from_source`, prediction error magnitude,
convergence) — is wasted. The realizer ignores the topology and
treats every working set as a flat list.

The fix is **structural**, not data-driven: the composer reads the
`CascadeRetrieval` and projects its topology to a discourse shape
the realizer can emit. This is **cascade-shape-driven composition**.

The architectural commitment is preserved: no LLM in the cognition
path. The shape is **derived** from retrieval structure, not
generated. The realizer still emits fragment quotation + connective
tissue; the shift is in which connective role gets selected where.

## Decision

Introduce a `CompositionShape` type — an ordered sequence of
`CompositionStep`s, each pairing a fragment with the connective role
that should precede it. The composer (or any caller) constructs the
shape from a `CascadeRetrieval` via `shape_from_cascade(...)`. The
realizer accepts the shape and walks it instead of the raw working
set.

A linear-walk fallback constructor `shape_from_working_set(...)`
preserves the v0.1.1 behaviour for callers that have a working set
but no cascade structure (the smoke runner, the eval harness's
direct-construction path). The `RealizationRequest` gains an
optional `shape` field; when `Some`, the realizer walks it; when
`None`, the realizer derives a linear shape from the working set
(backward-compatible default).

### The shape

```rust
pub struct CompositionShape {
    pub steps: Vec<CompositionStep>,
}

pub struct CompositionStep {
    /// Connective role to emit BEFORE this step's fragment.
    /// The first step's role is ignored (the realizer emits an
    /// `Opening` connective there instead).
    pub role: ConnectiveRole,
    /// The fragment to quote.
    pub fragment: CognitiveFragment,
    /// Distance from the cascade source for this fragment.
    /// 0 = direct seed; 1+ = cascade-derived.
    pub depth: u8,
}
```

The shape is a **sequence**, not a tree. The cascade is a tree (or
DAG via the conductivity graph); projecting to a sequence is the
work this ADR does — flattening structure to prose order while
choosing the right role to mark each structural transition.

### Projection algorithm (v0.1)

The algorithm is intentionally simple. It produces a shape that
reads less rigid than the linear walk, but it is not a discourse
planner. ADR-0007+ may refine it with empirical data; v0.1 ships
a deterministic baseline.

**Step 1 — Anchor.** The highest-scoring direct retrieval (lowest
distance) becomes the **anchor** fragment. It is the first step;
its preceding role does not matter (the realizer emits the
`Opening` connective there).

**Step 2 — First-hop supports.** Cascade fragments whose
`parent_id` is one of the direct seeds AND whose
`hops_from_source == 1` are **first-hop supports** of the anchor.
Two cases:

  - **Two or more supports.** The anchor reads as a "central
    claim" with supporting evidence. Each support becomes a step
    with role `Causation` (`"Therefore,"`, `"Because,"`,
    `"Consequently,"`).
  - **One support.** Standard continuation. Role `Continuation`.

Sort by descending `activation` level so the strongest support
comes first.

**Step 3 — Deeper activations.** Cascade fragments with
`hops_from_source >= 2` are **elaborations** of the structure
above. Each becomes a step with role `Elaboration`
(`"Specifically,"`, `"In particular,"`). Sort by ascending
`hops_from_source` so the shallower elaborations come first
(reading from general to specific).

**Step 4 — Other direct seeds.** Any direct retrieval beyond the
anchor that did not anchor its own subtree (no first-hop supports
in the cascade) becomes a step with role `Sequence`
(`"Then,"`, `"Next,"`, `"Subsequently,"`). The intuition: the
recall returned multiple distinct anchors and the composer is
enumerating them.

**Step 5 — Contrast injection.** A final pass walks the steps; any
adjacent pair whose **valence delta** crosses thresholds gets its
role overridden:

  - `|Δvalence| > 0.6` and opposing sign → `Contrast`
  - `|Δvalence| > 0.3` and opposing sign → `Concession`

This injection runs *after* the topology assignment, so a
high-valence-delta pair that started as `Causation` becomes
`Concession`. Topology decides the **default** role; valence
decides the **rhetorical colour**.

### Why valence-delta overrides topology

Two fragments may be topologically a "support + claim" pair
(`Causation` default) yet have opposing valences — the cascade
found a counter-example in the supporting evidence. The composition
should read as a concession ("Granted, X — but Y"), not a
causation ("X, therefore Y"). The valence-delta pass catches this.

This is a small but load-bearing heuristic: it is the difference
between the realizer correctly surfacing tension in the working set
and the realizer flattening that tension into false confluence.

### Why the algorithm stops here

A richer projection — multi-level recursion, prediction-error
weighting on the cascade engine's emitted error fragments, explicit
summary insertion when the shape has more than five steps — is
deferred. The v0.1 algorithm is the **minimum projection that uses
all ten roles**. Once `Causation`, `Elaboration`, `Sequence`, and
`Concession` are reachable from real cascade input, the
"rigid template" failure mode is structurally gone. Further
refinement is calibration, not architecture.

### Backward compatibility

Callers that pass `working_set` without `shape` get the
linear-walk behaviour (Continuation by default, Contrast on
opposing valence — the v0.1.1 semantics). The eval harness uses
this path because its corpus does not yet model cascade topology;
its expectations about which limitations fire stay valid.

The smoke runner upgrades to construct a `CascadeRetrieval` from
the working set (treating each fragment as a direct seed with hop
0) and runs through `shape_from_cascade`. This makes the smoke
output exercise the new code path; with three same-valence,
same-source fragments the resulting shape is three direct seeds
producing `Sequence` connectives between them.

### Where the algorithm lives

`shape_from_cascade` and `shape_from_working_set` live in
`hyphae-surface::composition_shape`. The composer subsystem
(`hyphae-subsystems::Composer`) is a future consumer that will call
the projection during its working-memory ↔ realizer wire-up; that
wiring is **not** part of this ADR — it lands when the substrate's
end-to-end recall flow (substrate.recall_signal →
episodic.cascade → composer.working_memory → realizer) materialises
in an integration test (deferred to a substrate-integration ADR).

For v0.1, the smoke runner and the eval harness construct shapes
directly. The architecture supports the future wiring without
changing.

## What this does NOT do

  - **Boundary smoothing.** Pronoun threading and tense alignment
    between adjacent verbatim quotes remain future work. The
    realizer still emits raw verbatim bodies separated by
    connectives. ADR-0007 candidate.
  - **Summary insertion.** The `Summary` role is populated in the
    lexicon but the v0.1 projection does not emit it. Adding it
    when the shape exceeds five steps is a one-line algorithm
    change; deferred until the eval corpus has compositions long
    enough to need it.
  - **Tree-structured composition.** The shape stays a flat
    sequence. Recursive projection (a claim with supports, each
    support with its own sub-supports) is possible but produces
    nested prose that overshoots v0.1's scope.
  - **Cascade engine reading prediction-error fragments.** The
    `predictive` subsystem emits error fragments during recall;
    the v0.1 projection ignores them. ADR candidate: route
    error fragments to `Contrast` with elevated priority.

## Consequences

- The realizer's output is shape-aware. A composition over three
  direct hits with one shared parent reads differently from a
  composition over a chain of cascade-derived elaborations — same
  fragments, different shape, different rhetorical surface.
- All ten roles ADR-0005 populated are now reachable from realistic
  cascade input.
- The composer subsystem has a clear consumer surface
  (`shape_from_cascade`) to wire its working memory through when
  the substrate-integration ADR lands.
- The eval harness gains tests that assert the projection emits
  the expected role at expected positions (e.g. "two supports
  for the same anchor must produce two Causation steps").
- The smoke runner's output stops looking like a template even
  on the v0.1 corpus. Cascade structure shows in the prose.

## Cross-references

- **ADR-0005** §"Role taxonomy: 5 → 10" — populated the lexicon
  with the roles this ADR activates.
- **ADR-0004** — without embeddings, the cascade seeds are
  meaningless; this ADR is unblocked because ADR-0004 already
  landed.
- **RFC v1-living §3** — the cascade activation primitive this
  ADR projects from.
- **`hyphae_core::CascadeRetrieval`** — the input type.
- **`hyphae_core::CascadeActivation::hops_from_source`** — the
  field the depth-1 / depth-2+ classifier reads.
- **`hyphae_surface::ConnectiveRole`** — the role taxonomy the
  shape's steps refer to.
