<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

# Hyphae v2 — Living RFC v1

**Status of this document.** This is the canonical architectural
specification of Hyphae v2. It is **append-only**. Sections are
labelled with one of: `stable`, `experimental`, `deprecated`. When a
section's status changes, the change is recorded in the section's own
History block at the bottom of the section, with a date and an ADR
reference. Sections are not rewritten — they are extended or marked
`deprecated` and a new section is appended.

This replaces the v1 RFC chain (`v0` → `v0.1` → `v0.1.1` → `v0.1.2` →
`v0.2` → `v0.3` with superseding rules). The chain was a material
contributor to v1's track stalling. One living document with explicit
section status is sufficient.

**Triangulation.** Sections promoted to `stable` SHOULD have triangulation
recorded (reviewer model + date + outcome) when the BDFL requests it.
Sections at `experimental` may be promoted to `stable` without
triangulation if the BDFL declares it; the declaration is the record.

---

## §1 Substrate primitives — `stable`

Cherry-picked from v1 `hyphae-core` with minor refinements. The set is
deliberately small.

**1.1 `CognitiveFragment`** — the atomic unit of cognitive content.
Carries content, provenance, saliency `∈ [0.0, 1.0]`, valence `∈ [-1.0,
+1.0]`, decay rate, confidence, optional embedding (dim=256, CPU-only),
optional boundary metadata for surface composition, depth level,
language tag, domain tags. Discriminated by `FragmentContent` variant:
episode, belief, goal, observation, reflection, reference.

**1.2 `FragmentId`** — 128-bit unique identifier. Low 8 bytes:
process-monotonic `AtomicU64` counter. High 8 bytes: per-process seed
derived once from the wall clock at first use. Unique within a run and
across restarts. (v1's clock-derived id had a collision bug in M0,
fixed in M4a-1; v2 ships the fix from commit zero.)

**1.3 `Provenance`** — `source_subsystem`, `source_pathway`,
`parent_ids: Vec<FragmentId>`, `confabulation_risk ∈ [0.0, 1.0]`.
Populated by the producer subsystem; never default-zero silently.

  - Measurement emitters (predictive error, ACC conflict in composer,
    valence stamp from measurement) → `confabulation_risk = 0.0`.
  - Single-input transformers (DA RPE, temporal stamp if reintroduced)
    → propagate the source's `confabulation_risk`.
  - Passthroughs → leave the input fragment's risk unchanged.

**1.4 `Pathway`** — typed, unidirectional channel between two
subsystems, labelled with a `PayloadKind`: `TopDownPrediction`,
`BottomUpPredictionError`, `Modulation`, `Encoding`, `Housekeeping`.
Optionally state-gated (a subset of the five global states under which
the pathway is active).

**1.5 `Subsystem` trait** — the contract every subsystem implements:
`process(fragment, state, incoming: PayloadKind) → Vec<CognitiveFragment>`,
`on_state_change(old, new)`, `checkpoint()`, `restore(snapshot)`.

  - The `incoming: PayloadKind` parameter is non-negotiable from commit
    zero (v1 added it via RFC-042 mid-development after three subsystems
    had worked around its absence via brittle string heuristics).

**1.6 `State`** — five global states: `Encoding`, `Recall`,
`Consolidation`, `Dormancy`, `Recovery`. State-gated pathways are
enforced at the substrate level. Violations are journal-logged
integrity errors.

**1.7 `BoundaryMetadata`** — gender, number, and head-noun signal for
surface realization concordance. Conservative: `None` when the encoder
cannot determine with high confidence. The realizer degrades to neutral
connective forms in that case rather than guessing.

---

## §2 Six functional subsystems — `stable`

The collapse from v1's 17 anatomical subsystems to v2's 6 functional
ones is recorded in ADR-0001. Each subsystem listed below is a function,
not an anatomy. The mapping to v1's anatomical labels is provided for
cross-reference only.

**2.1 `input-gate`** — input dispatch and signature-based amplification.
Collapses v1's Thalamus + Olfactory Bulb. Two entry points: general
ingest, and signature-amplified ingest (the OB-bypass case). Modality
filtering is the consumer's responsibility, externalised through the
substrate's two distinct entry points.

**2.2 `episodic`** — episodic store with binding and conductivity.
Collapses v1's Hippocampus + Entorhinal. Owns the HNSW vector index for
direct retrieval, the conductivity graph for cascade activation
(Collins & Loftus 1975; Siew 2019; Marko et al. 2026), and the binding
operation that links arriving fragments to their causal predecessors.

**2.3 `valence`** — affective stamp and decay modulation.
Collapses v1's Amygdala + BNST. Emits valence and saliency stamps on
input fragments; modulates `decay_rate` for sustained context. Integrates
explicitly with `hyphae-ethics` (input fragments flow through ethics
evaluation; the ethics signal can refine the valence stamp).

  - Note. v1's M4b finding showed Amygdala + BNST both modifying
    `saliency` and cancelling each other by clamp. v2 separates concerns
    at the type level: valence on `valence` axis, durability on
    `decay_rate` axis. No overlap.

**2.4 `composer`** — executive composition and conversational
metacognition. Collapses v1's Frontal Cortex + ACC. Owns the bounded
working memory (cap=7 fragments; Miller 1956), the surface realizer
wire-up, the conflict signal (cabled to the ethics path from commit
zero — corrects v1's documented-pending wire), and the conversation
thread tracker with Jaccard topic-switch detection.

**2.5 `predictive`** — predictive coding and prediction error.
Cherry-picked from v1's Cerebellum. Maintains a forward model;
broadcasts a prediction error fragment when actual diverges from
prediction. Prediction↔actual pairing uses cycle-id correlation from
commit zero — corrects v1's ISSUE-M4c-2 loose-pairing deferral.

**2.6 `reward`** — reward prediction error and expected-valence
low-pass. Cherry-picked from v1's Dopaminergic Midbrain. Computes
signed RPE `δ = actual − expected`; updates `expected_valence` via
convex combination; emits the RPE as one of the two feedback channels
the learning loop consumes (the other being ethics signals, §6).

### Postponed subsystems

Each item below carries an explicit re-entry criterion. Re-entry
requires an ADR documenting that the criterion has been met.

  - **Mammillary** (temporal index). Re-entry criterion: empirical
    evidence that the journal hash-chain timestamps are insufficient for
    composer's temporal queries.
  - **Septum** (rhythm). Re-entry criterion: empirical evidence that
    deterministic per-cycle ordering is insufficient for the composer.
  - **Serotonergic midbrain** (patience). Re-entry criterion: evidence
    that engagement modulation is needed for honest-limitation triggers.
  - **Claustrum** (coincidence detection). Re-entry criterion: evidence
    that the substrate needs explicit cross-source binding beyond what
    `episodic` already provides.
  - **Basal ganglia** (action selection). Re-entry criterion: existence
    of multiple competing proposals from `composer` that need
    selection. v0.1's composer emits one composition per recall.

---

## §3 Cascade activation — `stable`

Cherry-picked from v1 RFC v0.3 §1. Spreading activation through the
causal fragment network rather than attention over tokens in a context
window.

**3.1 Mechanism.** Given a query, `episodic` performs direct retrieval
to obtain seed fragments. From each seed, activation propagates to
neighbours weighted by `ConductivityWeight`, with decay per hop and a
`max_hops` cap. Propagation terminates when activation falls below a
threshold or `max_hops` is reached.

**3.2 Parameters.** `ConductivityWeight` per edge, hop decay factor,
activation threshold, `max_hops`. **All four are refinable by the
learning loop within bounds documented in §6.** The mechanism is
substrate (immutable); the calibration is learned.

**3.3 Output.** Both direct retrieval matches and cascade-activated
fragments enter the composer's working set. Cascade origin is recorded
in `Provenance.parent_ids` so the composer can distinguish direct from
propagated retrieval.

**3.4 Bounds.** The substrate enforces two independent limits during
routing: `MAX_ROUTING_DEPTH` (per-lineage cycle guard) and
`MAX_ROUTING_WORK` (total work budget). v1's hop cap conflated these;
v2 ships them split from commit zero.

---

## §4 Conversational metacognition — `stable`

Cherry-picked from v1 RFC v0.3 §2. Persistent system property, not
add-on.

**4.1 Thread tracking.** The `composer` maintains an explicit table of
active, paused, and resolved conversation threads. Topic switches are
detected by Jaccard similarity against active threads, not by implicit
attention reweighting.

**4.2 Open questions and pending follow-ups.** First-class state in the
thread table. Survives consolidation and dormancy. Resumed on Recovery.

**4.3 Metacognitive prelude.** When the composer detects topic
resumption, the composition prepends a structured prelude
acknowledging the prior thread context. The prelude is generated by
fragment quotation + connective tissue, not by LLM synthesis.

**4.4 Property claim.** Hyphae does not inherit the structural
attention deficit that current LLMs exhibit (the behaviour where a model
follows topic switches without acknowledgment because it has no
persistent representation of conversation structure). This is a
foundational architectural property, not a behavioural tweak.

---

## §5 Surface realizer — `stable`

Cherry-picked from v1 `hyphae-surface`, drastically simplified.

**5.1 Schemas (v0.1).** Two only: `DialogueReply` and `GroundedAssertion`.
v1's `IntrospectiveAssessment`, `NarrativeArc`, `ComparativeAnalysis`,
`SyntheticSummary` are postponed until v0.1 metrics validate that the
two-schema scope is too restrictive. Re-entry criterion documented per
schema in `docs/adr/0001-fresh-from-v1.md` §"Surface scope".

**5.2 Composition.** Fragment quotation (verbatim body text) +
connective tissue generation + boundary concordance enforced through
lexicon rules. Fragments are opaque content sources.

**5.3 Honest limitation triggers.** Mandatory: `EmptyWorkingSet`,
`HighConfabRisk`, `ShallowCascade`. The realizer emits an explicit
limitation acknowledgment when any trigger fires. This is the
architectural property that distinguishes Hyphae from systems that
confabulate when material is insufficient.

**5.4 Lexicon.** EN-only for v0.1 (§7 of ADR-0001). The connective
vocabulary, irregular nouns, and stress classifier from v1 are
cherry-picked for EN; the ES counterparts are not built until ES
re-enters.

---

## §6 Ethics Engine — `stable`

Detailed specification in ADR-0003. This section is the load-bearing
summary in the canonical RFC; deviations between this section and
ADR-0003 are bugs, and ADR-0003 wins.

**6.1 Crate.** `hyphae-ethics` is a dedicated crate. NOT distributed
across subsystems (the v1 distribution by omission is corrected here).

**6.2 Philosophy.** **RADAR, not JAIL.** The ethics engine
classifies, audits, and emits signals. It does NOT block operations.
Callers receive composition + structured ethics report. This aligns
with celiums-memory v2's verbatim declaration: *"the ethics engine is
a RADAR, not a JAIL."*

**6.3 Coverage of the cognition path.** Five evaluation points are
mandatory from commit zero:

  1. `remember` — input at substrate ingress.
  2. `recall` and cascade activation results — output of retrieval.
  3. `compose` — before emitting composition.
  4. Grounded retrieval (when introduced post-v0.1) — before absorbing
     from external sources.
  5. Learning loop parameter updates — ethics signals influence what
     the loop learns.

No "we'll add coverage when that path exists." Paths are designed
with the ethics hook already wired (corrects v1's PollinatePolicy /
DecomposePolicy / compose / curiosity coverage gap).

**6.4 Layers in v0.1.**

  - **Layer A** (deterministic lexicon and taxonomy). Multilingual
    posture from day one — EN ships first, ES re-enters with the
    lexicon expansion of §7. 12-category taxonomy (SafetyBench /
    Jigsaw / DSA / OWASP). Structural hate pattern detector. Context
    disambiguation (living target / technical / meta).
  - **Layer B** (probabilistic CVaR). Native Rust implementation,
    approximately 300 LOC, no new dependency. Profile-loader system per
    celiums-memory v2 ADR-021. Asymmetric reversibility weighting.
    Categorical hard rule for CBRN with operational intent
    (deterministic rule, bypasses the probabilistic path).
  - **Audit**. Append-only journal entries with SHA-256 hash chain.
    **One chain per substrate**, shared between substrate journal and
    ethics audit (not two chains).

**6.5 Layer C deferral.** Multi-framework philosophical evaluation
requires LLM dispatcher in celiums-memory v2. Hyphae v2 commitment #1
prohibits LLM in the cognition path. Layer C is `deferred` with
explicit ADR (ADR-0003 §"Layer C deferral"). Re-entry options surveyed
there.

**6.6 Layer K (precedent advisory).** `deferred`. Requires
`ethics_knowledge` corpus separate from the source tree (same posture
as celiums-memory v2 ships).

**6.7 Signals.** Ethics evaluation emits a structured `EthicsReport`
carrying: classification (per Layer A taxonomy), CVaR score (per
Layer B), violation flags, corpus baseline deviation, audit entry
reference. The report flows to:

  - `composer` — may add limitation acknowledgment to the composition.
  - learning loop — one of the two feedback channels (§7).
  - `audit journal` — append-only.
  - Caller — receives composition + report.

---

## §7 Learning loop — `stable`

Detailed specification in ADR-0002. This section is the load-bearing
summary; deviations between this section and ADR-0002 are bugs, and
ADR-0002 wins.

**7.1 Crate.** `hyphae-learning`. First-class from RFC v0.1, not a
post-Phase-C add-on (v1 articulated it as the AlphaGo-paradigm
Observation 1 after Phase C close; v2 ships it).

**7.2 Bounds — what learning may NOT touch.**

  - The grammar.
  - The state machine.
  - The pathway topology.
  - The schemas (structural shape, not slot priors).
  - The hash chain protocol.
  - The PayloadKind taxonomy.
  - The Hard Architectural Commitments in CLAUDE.md.

**7.3 Refinable parameters.**

  - `episodic.conductivity_weights` (per-edge).
  - `valence.salience_weights` (per-category).
  - Cascade thresholds and hop-decay factor.
  - `composer.schema_selection_priors`.
  - `composer.honest_limitation_thresholds` (within RFC-bounded
    minimum sensitivity).

**7.4 Feedback signals (two channels).**

  - **Reward prediction error** from `predictive` + `reward`. Captures
    "did the composition's predicted utility match observed utility."
  - **Ethics signals** from `hyphae-ethics`. Captures classification
    deltas, violation flags, corpus baseline deviation.

The two channels are complementary, not redundant. Either alone is
insufficient for the learning loop to converge to useful behaviour.

**7.5 Audit and rollback.** Every parameter update is a journal entry
on the same hash chain as memory and ethics audit. Rollback = replay
the journal up to entry N, recomputing parameter state. The Recovery
state runs chain verification.

**7.6 Bounds enforcement.** The learning loop cannot modify any item in
§7.2 by construction: those items are exposed as immutable references
in the substrate API; the loop operates on a separate mutable parameter
store.

**7.7 Multi-user vs single-user.** v0.1 ships single-user only. The
single-user binary learns from its own operation. Multi-user
aggregation is `deferred` with the criterion that the single-user
learning loop is empirically validated first.

---

## §8 Storage and journal — `stable`

Cherry-picked from v1 `hyphae-storage` unchanged.

**8.1 KV store.** `redb` for state store. Single writer, multi-reader.

**8.2 Journal.** `fjall` with SHA-256 hash chain. Each entry is hashed
as `SHA-256(id || subsystem || content || written_at || prev_hash)` and
links to the previous entry's hash. **One chain per substrate**, shared
between substrate journal (memory ingest, journal writes,
state transitions) and ethics audit (per §6.4).

**8.3 Vector index.** `hnsw_rs` for the `episodic` HNSW. EMBEDDING_DIM =
256 (CPU-only, no GPU).

**8.4 Internal format.** Binary throughout: `postcard` or `bincode` for
fragments, lexicon, and snapshots. JSON export is a dedicated tool, not
used internally.

**8.5 Recovery.** On Recovery state, the substrate runs chain
verification. A broken chain halts the substrate and surfaces the
break point.

---

## §9 What is NOT in v0.1

This section is canonical for the negative scope. Re-entry of any item
requires an ADR.

  - Multilingual lexicon beyond EN. ES, PT, FR, DE, IT, RU, TR, JA, ZH
    re-enter post-v0.1 validation.
  - Vertex AI grounding / external retrieval providers.
  - Citation engine and cross-lingual concordance suppression.
  - The 19 Phase D tools from v1 (`research_*`, `write_*` families).
  - The Phase B tool patterns that were LLM-shape.
  - Layer C philosophical evaluation (LLM dispatcher).
  - Layer K precedent advisory (corpus dependency).
  - Mammillary, Septum, 5HT, Claustrum, Basal ganglia subsystems
    (postponed with explicit re-entry criteria in §2).
  - Surface schemas beyond `DialogueReply` and `GroundedAssertion`
    (postponed with criteria in §5).
  - Multimodal perception (image / audio).
  - Multi-user learning aggregation.
  - Plugin layer for specific LLM-host consumers (Claude Code, Codex,
    Gemini, Claude Web). Deferred per the v1 commitment until the core
    abstraction is empirically validated.

---

## §10 Governance — `stable`

**10.1 BDFL.** Mario Gutiérrez (Celiums AI).

**10.2 ADR system.** Architectural decisions and reversals are filed as
ADRs under `docs/adr/`. ADR status: `proposed`, `accepted`, `superseded
by NNNN`. Once `accepted`, ADRs are immutable; superseding requires a
new ADR that explicitly links back.

**10.3 RFC system.** This document is the canonical RFC. Append-only.
Sections labelled `stable` / `experimental` / `deprecated`.

**10.4 Pathway to broader governance.** Triggered by scale milestones
(real external contributors, production deployments by entities other
than Celiums AI). Not in scope for v0.1.

---

## History

  - **2026-05-26** — Document created at v2 chartering session.
    Substrate primitives (§1), six functional subsystems (§2), cascade
    activation (§3), conversational metacognition (§4), surface
    realizer (§5), ethics engine RADAR (§6), learning loop first-class
    (§7), storage and journal (§8), negative scope (§9), governance
    (§10) all declared `stable`. Triangulation deferred to first
    architectural change. Cross-references: ADR-0001 (fresh-from-v1),
    ADR-0002 (learning-loop-firstclass), ADR-0003
    (ethics-radar-firstclass).
