<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0011
title: Cascade wakeup — invoke episodic.cascade() on the recall path
status: accepted
date: 2026-05-27
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (audit #32, v0.1 implementation review)]
---

# 0011 — Cascade wakeup: invoke `episodic.cascade()` on the recall path

## Context

The 2026-05-27 audit (task #32) confirmed a structural gap between
the RFC §3 commitment and the implementation:

> **RFC §3 / ADR-0001 §"Hard Commitment 11":** Cascade activation
> is the retrieval mechanism, not optional enhancement.

But:

> **`crates/hyphae-subsystems/src/episodic.rs:372-374`:**
> "the cascade is driven externally via `Self::cascade` so the
> process boundary stays simple."

Concretely:

- `substrate.recall_signal()` evaluates ethics, embeds the cue,
  and routes a `BottomUpPredictionError` to `Episodic`.
- `Episodic::process()` in the Recall branch calls
  `pattern_complete(query_embedding, top_k)` and emits the
  matches.
- `Episodic::cascade(seeds)` exists (264 LOC method + 3 unit
  tests in `episodic.rs:264`) and is correct.
- **Nothing calls `cascade()` on the recall path.** No substrate
  method, no subsystem, no smoke binary. The cascade engine is
  structurally isolated from the recall flow.

The eval corpus has been masking this by setting
`from_cascade: true` on synthetic seeds, populating
`provenance.parent_ids` directly. Real recall against a
populated `Episodic` graph produces working sets that are pure
direct-hit (`pattern_complete` clones, with whatever
`parent_ids` they had at store time). The `ShallowCascade`
limitation trigger fires on real recall, never on the synthetic
corpus, which is exactly inverted from the architecture's
intent.

This is the load-bearing v0.1 closure gap.

## Decision

**Wire `Episodic::cascade()` into `Episodic::process()` on the
Recall + `BottomUpPredictionError` branch.** The cascade
activation runs once per recall, seeded by the direct-hit IDs
from `pattern_complete`. Cascade-derived fragments are emitted
alongside direct hits, tagged with `provenance.parent_ids` set
to the immediate predecessor in the propagation chain.

### Implementation shape

```rust
(PayloadKind::BottomUpPredictionError, State::Recall) => {
    let query = fragment.embedding.as_deref();
    let direct = self.pattern_complete(query, self.params.working_set_size as usize);

    // ADR-0011: seed the cascade with direct-hit ids and merge
    // propagation results into the emission set.
    let direct_ids: Vec<FragmentId> = direct.iter().map(|(_, f)| f.id).collect();
    let retrieval = self.cascade(&direct_ids);

    let mut emissions: Vec<CognitiveFragment> =
        direct.iter().map(|(_, f)| f.clone()).collect();

    let direct_set: HashSet<FragmentId> = direct_ids.iter().copied().collect();
    for (id, (activation, frag)) in &retrieval.cascade {
        if direct_set.contains(id) {
            continue;
        }
        let mut tagged = frag.clone();
        if let Some(parent) = activation.parent_id {
            tagged.provenance.parent_ids = vec![parent];
        }
        emissions.push(tagged);
    }

    emissions.push(fragment);
    Ok(emissions)
}
```

### Why mutate `parent_ids` on the cascade-derived clones, not the
stored originals

The stored `CognitiveFragment` in `Episodic::fragments` records
the fragment's provenance at the time it was remembered. Cascade
propagation does NOT create new fragments — it activates
existing ones via the conductivity graph. Mutating the stored
provenance would lie about how the fragment was originally
created.

Instead, the propagation channel is communicated by
**emission-time tagging**: the clone that flows downstream from
`process()` carries `parent_ids = vec![activation.parent_id]`,
signalling "this fragment reached the working set via cascade
from that parent." The stored copy keeps its remember-time
provenance untouched.

### Discrimination of channels

The `CascadeRetrieval` struct
(`crates/hyphae-core/src/cascade.rs:442`) preserves the
distinction the composer documentation already anticipates
(`"direct and cascade-activated fragments are weighted
differently in the working-set selection"`). v0.1's substrate
output flattens to `Vec<CognitiveFragment>` for routing
simplicity. The `parent_ids` tag is sufficient to discriminate
at the realizer's `ShallowCascade` check — a richer return type
that exposes `(direct: Vec<_>, cascade: Vec<_>)` separately is a
future ADR if the composer needs distance + activation
metadata.

### Ethics over cascade results — sub-decision from audit #33

ADR-0003 §"Coverage" specifies that the Recall coverage point
covers "recall AND cascade activation results." Two options
considered:

1. **Re-evaluate cascade-derived fragments at Recall.** Each
   propagation-derived clone runs through `ethics.evaluate`
   before emission. Cost: O(k×n) ethics evaluations per recall.
2. **Trust the Remember-time gate.** Cascade can only reach
   fragments that previously passed `CoveragePoint::Remember`.
   The Recall ethics evaluation on the cue is sufficient — it
   checks the intent of the retrieval, not the validity of
   already-vetted stored content.

**Choose (2).** Cascade activation is a graph traversal over
content the substrate has already gated. The marginal value of
re-evaluating is low; the marginal cost is per-recall multiplied
by graph fan-out. RADAR posture: the retrieval intent matters
more than the retrieved-content validity, because the retrieved
content's validity was already audited at Remember time.

If a future requirement is "ethics should re-evaluate cascade
fan-out under specific conditions" (e.g. fragments older than
T, fragments from a tenant that has since been redacted), that
warrants a separate ADR with concrete trigger criteria.

### Working-set ranking

`pattern_complete` returns by ascending distance; cascade-derived
fragments are appended in `HashMap` iteration order
(non-deterministic across runs).

v0.1 minimum: emission order is `[direct…, cascade…,
cue_fragment]`. The substrate routes the full Vec; the composer
discriminates by `parent_ids` presence; downstream working-set
selection happens in the composer subsystem (not modified by
this ADR).

A future ADR can add deterministic ordering of cascade-derived
emissions (e.g. sort by `activation.activation` descending) when
empirical results show the order matters. v0.1 ships with
non-deterministic order documented as known.

### What this ADR explicitly does **not** do

- **Does not** change the substrate's `RecallOutput` shape. Still
  `{ terminals: Vec<CognitiveFragment>, ethics: EthicsReport }`.
- **Does not** modify the composer to weight direct vs cascade
  differently. The composer receives the full Vec; v0.1
  realizer/composer already handle whatever order.
- **Does not** update the smoke binary to use real
  `recall_signal()`. Smoke still constructs a synthetic working
  set. Modernising smoke is part of the v0.1 closure ceremony,
  not this ADR.
- **Does not** introduce a `Provenance.via_cascade: bool`
  discriminator. Using `parent_ids` is sufficient for v0.1 and
  matches the existing `ShallowCascade` heuristic.

## Sources

- **RFC §3** (cascade activation), `stable` — the architectural
  commitment this ADR operationalises.
- **ADR-0001 §"Hard Commitment 11"** — the explicit prohibition
  against treating cascade as optional.
- **ADR-0003 §"Coverage"** — the source for the ethics
  sub-decision.
- **`hyphae_core::cascade::CascadeRetrieval`** — the struct that
  preserves direct vs propagation distinction.
- **Audit #32 (2026-05-27)** — the findings document gap.

## Consequences

- Real `recall_signal()` calls now produce working sets that
  reflect both direct and propagation channels. The
  `ShallowCascade` limitation trigger fires correctly on
  recall paths that produce no propagation, not just on
  synthetic corpus seeds.
- Cascade engine is no longer structurally isolated; the v0.1
  spec commitment is satisfied.
- Episodic `process()` now does meaningful work in the Recall
  branch beyond pattern-completion. Test coverage expands by 1
  unit test (cascade fires post-pattern-complete) and 1
  integration test (remember → recall → fan-out observable).
- The eval corpus's `from_cascade: true` convention now agrees
  with real recall behaviour rather than masking it. ADR-0008
  fluency dimensions are evaluated against runs that exercise
  the cascade activation channel.
- `audit #33`'s arrastrada nota retires: ethics over cascade
  results is resolved as "trust the Remember-time gate."
- Performance: one extra `cascade()` call per `recall_signal`.
  The cascade is hop-bounded and threshold-gated; cost scales
  with graph fan-out near the seed nodes, not with total
  episodic size.

## Cross-references

- **Audit #32** (in-session work, 2026-05-27) — findings.
- **ADR-0001 §"Hard Commitment 11"** — the commitment this
  closure honours.
- **ADR-0003 §"Coverage"** — the ethics sub-decision authority.
- **ADR-0008** — the fluency dimensions whose interpretation
  shifts once real cascade lands.
- **`hyphae_subsystems::episodic::Episodic`** — the type
  modified by this ADR.
