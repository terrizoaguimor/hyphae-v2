<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0001
title: Fresh repository for Hyphae v2 with controlled cherry-pick from v1
status: accepted
date: 2026-05-26
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (v2 chartering session review)]
---

# 0001 — Fresh repository for Hyphae v2 with controlled cherry-pick from v1

## Context

Hyphae v1 (preserved at the v1 archive) shipped 145+ commits across M0
through Phase C wave 1. The substrate primitives validated:
`CognitiveFragment` with provenance, typed pathways, five-state machine,
cascade activation, conversational metacognition, hash-chained journal,
honest limitation triggers, embedded storage on CPU. The architectural
bet — that a category of useful AI capability can be decoupled from
hyperscale GPU infrastructure — remains undisputed by the v1 evidence.

What did not survive was the solo-founder track. The 2026-05-24 audit
and the 2026-05-26 chartering review identified four cumulative
sources of overhead that, together, made continuation in v1
unsustainable:

  1. **Triangulation discipline pre-commit.** Every foundation
     milestone required review by `a primary LLM triangulator` (Atlas primary) and
     `a secondary LLM triangulator` (Atlas secondary) before merging, plus charter
     writing in the same PR, plus audit responses. Admirable; not
     proportionate to one founder's actual capacity.

  2. **Multilingual ES + EN from day one.** Cross-lingual concordance
     rules, lexicon in two languages (430 ES connectives in wave 1,
     target 5k nouns + 2k verbs in wave 2), framing prefixes, per-chunk
     language detection, suppression rules. All of this consumed cycles
     before the architectural bet had been validated in any single
     language.

  3. **Port of 38 tools (Phase A + B + D) from the upstream sibling project.**
     Particularly Phase D's 19 research_* and write_* tools were
     lifecycle operations shaped for an MCP wrapper over LLMs; they did
     not compose with the substrate. The port produced a 895-line Phase
     B pattern catalog and months of porting work without generating
     evidence about the architectural bet.

  4. **Five built-but-not-wired pieces at Phase C close.**
     `RetrievalPolicy` (5 stubs identically delegating to a generic),
     `SchemaSelectionFailure + fallback` (built, not called),
     `evaluate_coherence` (built, not called), `compose_tools::compose`
     (ignores the policy), `ConflictReport` per-pair precision
     (heuristic). The 0.993 grammaticality baseline was obtained
     without exercising any of the modular machinery the codebase
     claimed to use. Atlas explicitly flagged the scorer as
     optimistically biased, but the modular gap was not the scorer —
     it was that the compose path used none of the optionality.

In addition, three architectural shapes in v1 ended up `decision-by-
omission`, not by deliberate design:

  - The ethics engine distributed across S4 Amygdala (Layer 1), S3
    Hippocampus (corpus baseline), S11 Medulla (audit), with
    coordination at the substrate level — never explicitly rejected
    as "a dedicated subsystem" because nobody proposed it.
  - JAIL semantics (block on threshold breach) — directly contradicts
    the upstream sibling project's verbatim *"RADAR not JAIL"* declaration.
  - Ethics gate coverage on `memory_*_secure` + `journal_*_secure` only,
    not on `PollinatePolicy::retrieve`, `DecomposePolicy::retrieve`,
    `compose()`, or curiosity firing — gaps that emerged because
    grounding and composition shipped after the gate landed and the
    gate was never extended.

These three were not bugs the team chose to live with. They were
shapes the team did not realise had been chosen. Documenting them
honestly was an act of anti-confabulation discipline (ADR-0002 v1).
Continuing on them in v2 would not be.

## Decision

Hyphae v2 is a **fresh repository** with **controlled cherry-pick**
from v1, not a refactor of the v1 tree.

The v1 codebase is preserved at the v1 archive for archival reference
and as the empirical record of which primitives validated. It is not
deleted, not deprecated by erasure. It is the substrate of v2's
articulation — the experiment that produced the lessons v2 incorporates.

Three approaches were surveyed during the chartering session:

  - **Approach A (refactor in place).** Maintain `hyphae/`, branch
    `v2-simplification`, delete the LLM-shape tool ports, collapse the
    subsystem count, drop the ES path, wire the unwired pieces. Cost:
    the v1 commit history, the RFC superseding chain (`v0.1.2` → `v0.2`
    → `v0.3` with patches), and the 17-subsystem charters remain in the
    tree. Cherry-picking *out* is harder than cherry-picking *in*.

  - **Approach B (fresh with cherry-pick).** New repository
    (this one). Single living RFC. ADRs from commit zero.
    Re-introduce v1 elements only when each justifies its place
    against v0.1's commitments. Costs: temporary loss of v1's CI
    configuration and dev tooling (must be re-established); two
    repositories live side-by-side until v2 supersedes v1 by
    acceptance.

  - **Approach C (hybrid via git submodule).** v2 new, but
    `hyphae-core` and `hyphae-storage` are submodules from `hyphae/`,
    immutable. v2 builds against them. Cost: cross-repo development
    friction; the immutability constraint blocks the v0.1 corrections
    (BoundaryMetadata refinements, FragmentId fix from M4a-1 baked from
    commit zero, etc.).

**Approach B is adopted.** The chartering session BDFL decision is
recorded in the session transcript and journal entry registered in
the upstream sibling project.

## What is cherry-picked from v1 (substrate-validated, carried)

Listed in the order the cherry-pick proceeds.

  - **`hyphae-core` substrate primitives.** `CognitiveFragment`,
    `FragmentId` (with the M4a-1 fix from commit zero), `Provenance`,
    `Pathway`, `PayloadKind` (including `BottomUpPredictionError`),
    `Subsystem` trait (with `incoming: PayloadKind` parameter from
    commit zero — corrects v1's RFC-042 retrofit), `State` (five
    states), `BoundaryMetadata`, `ConductivityGraph`,
    `CascadeActivation` types.

  - **`hyphae-storage`.** `redb` state store, `fjall` journal,
    SHA-256 hash chain, chain verification on Recovery. The chain
    shared with ethics audit from commit zero (corrects v1's separate-
    audit posture by collapsing onto one chain).

  - **`hyphae-substrate`.** State machine, pathway routing with
    fan-out (the M4a fix from commit zero), `MAX_ROUTING_DEPTH` +
    `MAX_ROUTING_WORK` split from commit zero, journal write
    integration. Without the Phase A/B/D tool zoo.

  - **Six subsystems collapsed from 17.** See RFC v1-living §2 for the
    mapping. The collapse rationale per pair is in §"Subsystems
    collapsed" below.

  - **Cascade activation** (v1 RFC v0.3 §1). Mechanism is substrate;
    parameters are refinable by the learning loop (RFC §7).

  - **Conversational metacognition** (v1 RFC v0.3 §2). Thread
    tracking, Jaccard topic switch, MetacognitivePrelude. Cabled into
    `composer` directly.

  - **Surface realizer** — two schemas in v0.1 (`DialogueReply`,
    `GroundedAssertion`). Connective tissue + boundary concordance +
    honest limitation triggers (`EmptyWorkingSet`, `HighConfabRisk`,
    `ShallowCascade`).

  - **EN lexicon** subset from v1's `hyphae-lexicon`. Connectives,
    irregular nouns, stress classifier — EN only.

  - **Plasticity charter draft** as the basis for `hyphae-learning`'s
    parameter mutability bounds.

## What is NOT cherry-picked (with re-entry criterion per item)

  - **Phase A 5 tools, Phase B 14 tools, Phase D 19 tools.**
    Re-entry per tool: an empirical demonstration that the tool's
    semantics composes with the substrate (not against it). Phase D's
    `research_*` and `write_*` are unlikely to re-enter as-is; their
    lifecycle shapes belong in an MCP wrapper, not in the substrate.

  - **Multilingual lexicon beyond EN.** Re-entry: empirical validation
    of the architectural bet on EN-only, demonstrated through honest
    eval metrics (no friendly-query-on-foreign-seed artefacts).
    Re-entry order: ES, then PT, then the rest of the upstream sibling project's
    10 languages.

  - **Vertex AI grounding + citation engine + cross-lingual
    concordance.** Re-entry: post v0.1 validation, with the coverage
    extension to the ethics evaluation path (ADR-0003 §"Coverage
    extension").

  - **The RFC superseding chain (v0.1.2 → v0.2 → v0.3 with patches).**
    Replaced by a single append-only living RFC. The v1 RFCs are
    preserved at the v1 RFC archive for historical reference.

  - **The 17-subsystem topology.** Collapsed to 6 per §"Subsystems
    collapsed".

  - **Triangulation pre-commit for every foundation milestone.**
    Replaced by triangulation only when the BDFL requests it
    (per the project working conventions). Architectural changes still
    triangulate before the ADR is filed.

## Subsystems collapsed (17 → 6)

The collapse is functional, not anatomical. Each collapse pair is
justified below; mapping recorded in RFC §2.

  - **`input-gate`** = v1 S1 Thalamus + S15 Olfactory Bulb. v1's M8
    review confirmed the OB is an "un-gated second Thalamus" with
    modality filtering externalised to the consumer. The two entry
    points are kept; the subsystem is one.

  - **`episodic`** = v1 S3 Hippocampus + S3b Entorhinal. The HNSW
    store, the conductivity graph, and the binding operation are
    operationally inseparable. Splitting them in v1 produced two
    crates that always co-changed.

  - **`valence`** = v1 S4 Amygdala + S2 BNST. v1's M4b review
    documented that both modified `saliency` and cancelled by clamp.
    The fix was to move BNST to `decay_rate`, which is the same
    subsystem with two output axes — collapsed in v2 from commit zero.

  - **`composer`** = v1 S8 Frontal Cortex + S8b ACC. The composition
    operation and the conflict-monitoring metacognition share the
    working memory and the conversation thread table; splitting them
    produced the wire that v1 acknowledged as `documented-pending` for
    the ethics path (substrate `lib.rs:4921`).

  - **`predictive`** = v1 S9 Cerebellum. Kept as a distinct subsystem
    because its prediction buffer is genuinely independent state from
    `composer`'s working memory. ISSUE-M4c-2 (loose prediction↔actual
    pairing) is fixed in v2 by cycle-id correlation from commit zero.

  - **`reward`** = v1 S10a Dopaminergic Midbrain. Kept as a distinct
    subsystem because the expected-valence low-pass and the RPE
    integration are stateful in a way that does not compose with
    `valence`'s amplitude integrator (BNST-shape) or `predictive`'s
    forward model. The two feedback channels the learning loop
    consumes (RPE + ethics) require `reward` as a producer of the
    first channel.

### Postponed subsystems (with re-entry criteria)

  - **Mammillary** (temporal index). Re-entry: empirical evidence the
    hash-chain timestamps are insufficient for `composer`'s temporal
    queries.
  - **Septum** (rhythm). Re-entry: deterministic per-cycle ordering
    proves insufficient.
  - **Serotonergic midbrain** (patience). Re-entry: engagement
    modulation needed for honest-limitation triggers.
  - **Claustrum** (coincidence). Re-entry: explicit cross-source
    binding needed beyond what `episodic` provides.
  - **Basal ganglia** (action selection). Re-entry: `composer` emits
    multiple competing proposals.

## Surface scope

Two schemas only in v0.1: `DialogueReply` and `GroundedAssertion`.

Postponed schemas with re-entry:

  - **`IntrospectiveAssessment`** — re-entry: composer needs a schema
    distinct from `DialogueReply` for self-referential output (Phase
    C wave 2 bucket 1 in v1 shipped this; criterion is whether the
    learning loop produces self-referential outputs that
    `DialogueReply` cannot accommodate).
  - **`NarrativeArc`** — re-entry: temporal-window retrieval lands.
  - **`ComparativeAnalysis`** — re-entry: cascade activation reliably
    returns parallel-structure fragments.
  - **`SyntheticSummary`** — re-entry: citation engine lands (post
    grounding re-entry).

## Consequences

  - v1 and v2 coexist under `Documents/` until v2 reaches a
    declared-supersession milestone (BDFL-determined; not in scope
    here).
  - The cherry-pick proceeds crate-by-crate per the order in §"What
    is cherry-picked" above. Each crate's first commit references this
    ADR and the relevant living-RFC section.
  - The four sources of overhead documented in §Context are
    structurally addressed by the v2 commitments (ADR-0001), not
    by aspiration.
  - The v0.1 corrections cited above (FragmentId fix, PayloadKind
    parameter, fan-out routing, MAX_ROUTING split, ethics on shared
    chain, ethics RADAR not JAIL, ethics coverage from commit zero,
    learning loop first-class, two-schema surface, six functional
    subsystems, EN-only) are no longer corrections — they are the
    starting state.

## Cross-references

  - **ADR-0002** (learning-loop-firstclass) — companion decision.
  - **ADR-0003** (ethics-radar-firstclass) — companion decision.
  - **the v1 ethics-engine-current-state decision document**
    — the v1 honest documentation of the four ethics decisions taken
    implicitly. The retroactive articulation the team produced there
    is the model for the discipline this ADR continues.
  - **the v1 issue documentation** — the v1 implementation
    triangulation review log. Source for the "built-but-not-wired"
    catalogue cited in §Context.
  - **the v1 wave-1 close report** — Atlas's
    verbatim caveat on the grammaticality baseline.
  - **the v1 phase-C wave-2 direction note** — v1's last
    operational direction before the track stalled.
