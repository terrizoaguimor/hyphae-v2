<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0002
title: Learning loop is first-class from RFC v0.1
status: accepted
date: 2026-05-26
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (v2 chartering session review)]
---

# 0002 — Learning loop is first-class from RFC v0.1

## Context

Hyphae v1 specified the substrate's rules of cognitive composition
across three RFC versions: lexicon entries with grammatical metadata,
grammar rules enforcing concordance, schemas governing composition
shape, conductivity weights specifying network topology, threshold
parameters governing cascade propagation depth. The substrate was
declared functionally complete after RFC v0.3 (cascade activation +
conversational metacognition).

What v1 lacked was the mechanism by which strategies for navigating
this specified space are learned from operational experience.
Parameters were fixed at design time. The substrate ran with
designer-specified calibration; it never refined that calibration from
its own use.

This gap was articulated by the BDFL on 2026-05-23 after viewing the
DeepMind documentary "The Thinking Game" (Greg Kohs, 2024). The
BDFL's framing was explicit (verbatim, registered in celiums-memory
journal entry `e8392ddd-b24a-495b-be52-7915f8d23efa`):

> *"La arquitectura está bien, lo que está 'mal' entre comillas
> porque es parte de desarrollar esto, es el modelo de aprendizaje."*

The paradigm is AlphaFold 2: physical and geometric constraints were
specified by humans and integrated into the neural network
architecture; the network learned strategies for navigating the
constrained space. Hyphae substrate = rules of the game; learning
loop = strategy refinement within those rules.

In v1 this observation was filed as a post-Phase-C item — likely
RFC v0.4 — to be articulated only after Phase C closed with measured
metrics. The deferral was reasonable at the time: Phase C had a
two-week-out scope and the discipline was to not expand scope mid-
phase.

The v2 chartering session reopened the question and reached a different
conclusion.

## Decision

The learning loop is a **first-class commitment from RFC v0.1**, not a
post-validation addition. The `hyphae-learning` crate ships as part of
the v0.1 cherry-pick. Substrate APIs expose mutability surfaces for
the refinable parameters from commit zero. Every operation in the
cognition path is designed with the learning loop's audit and rollback
hooks already wired.

### Substrate vs learning loop — the bounds

**Immutable (substrate is the rules of the game):**

  - The grammar (concordance rules, boundary rules).
  - The state machine (five global states, transitions).
  - The pathway topology (which subsystem talks to which under which
    state-gate).
  - The schemas (structural shape: which slots a `DialogueReply` has;
    not the priors over which schema is selected).
  - The hash chain protocol.
  - The `PayloadKind` taxonomy.
  - The Hard Architectural Commitments in CLAUDE.md.

**Refinable (learning loop refines strategies within the rules):**

  - `episodic.conductivity_weights` — per-edge weight in the
    fragment causal graph. Initial values from heuristic at encoding
    time; refined by use.
  - `valence.salience_weights` — per-category weight for affective
    stamping. Initial values from the Plasticity Charter draft;
    refined by use.
  - Cascade activation parameters: `hop_decay_factor`,
    `activation_threshold`, per-subsystem `max_hops` cap (bounded by
    a hard substrate constant).
  - `composer.schema_selection_priors` — probability of choosing
    `DialogueReply` vs `GroundedAssertion` given query features.
  - `composer.honest_limitation_thresholds` — when to fire
    `EmptyWorkingSet`, `HighConfabRisk`, `ShallowCascade`. Refined
    within RFC-bounded minimum sensitivity (the loop cannot learn
    to never acknowledge limitations).

### Feedback signals — two channels

The Plasticity Charter draft (v1, `docs/plasticity/charter_draft.md`)
proposed two feedback channels: reward prediction error and ethics
signals. v2 ships both from commit zero.

**Channel 1 — Reward prediction error.** Produced by `predictive` +
`reward`:

  - `predictive` (Cerebellum, cherry-picked from v1) maintains a
    forward model and emits a prediction error fragment when actual
    diverges from prediction. Prediction↔actual pairing uses cycle-id
    correlation from commit zero (corrects v1's ISSUE-M4c-2).
  - `reward` (DA Midbrain, cherry-picked from v1) integrates signed
    RPE `δ = actual − expected` and updates `expected_valence` via
    convex combination.

This channel captures "did the composition's predicted utility match
observed utility."

**Channel 2 — Ethics signals.** Produced by `hyphae-ethics`:

  - Classification deltas (per Layer A taxonomy) between consecutive
    compositions in a thread.
  - Violation flags (per Layer A + B).
  - Corpus baseline deviation (the recall-based reference frame from
    v1's "Layer baseline" mechanic).

This channel captures "did the composition stay aligned with the
ethics evaluation surface."

**The two channels are complementary, not redundant.** RPE alone
optimises for predicted utility, which can drift toward outputs that
satisfy reward without staying ethical. Ethics alone optimises for
classification cleanness, which can drift toward outputs that are
ethically pristine but useless. The two together is the design.

### Audit and rollback

Every parameter update is a journal entry on the same SHA-256 hash
chain as memory and ethics audit. The entry carries:

  - Parameter identifier (e.g., `episodic.conductivity_weights[edge_id]`).
  - Old value, new value.
  - Triggering feedback (the RPE or ethics signal id that caused the
    update).
  - Update timestamp.
  - Reference to the composition session that produced the feedback.

Rollback = replay the journal up to entry N, recomputing parameter
state. The Recovery state runs chain verification; a broken chain
halts the substrate and surfaces the break point.

This makes parameter learning **auditable**: at any moment the
parameter state is provable from the journal, and the journal cannot
be silently rewritten.

### Bounds enforcement

The learning loop cannot modify any item in §"Immutable" by
construction:

  - The grammar, state machine, pathway topology, schemas, and hash
    chain protocol live in `hyphae-core` and `hyphae-substrate` as
    `const` or immutable references.
  - `hyphae-learning` operates on a **separate mutable parameter
    store**, exposed through the substrate API as a typed read /
    propose-update interface.
  - Updates pass through a bounds-checker that rejects values outside
    the RFC-specified range per parameter.

A learning-loop update that attempts to modify the grammar is a type
error at compile time, not a runtime check.

### Single-user in v0.1

v0.1 ships single-user only. The single-user binary learns from its
own operation. Cross-user aggregation introduces privacy questions
(whose feedback influences whose parameters), trust questions (a
malicious user can poison the parameters), and convergence questions
(does federated aggregation converge with the available signal
volume). None of these is in scope for v0.1.

**Re-entry criterion for multi-user learning:** empirical validation
of single-user learning convergence on the v0.1 eval corpus. A
separate ADR articulates the multi-user design when that criterion is
met.

### What the learning loop does NOT do in v0.1

Listed for clarity. Each item carries an implicit re-entry criterion:
empirical evidence that the v0.1 mechanism is insufficient.

  - No meta-learning. The loop does not learn how to update its own
    hyperparameters (learning rate, update bound widths). Those are
    designer-specified.
  - No off-policy correction. The loop updates parameters from
    on-policy feedback only. The composition that produced the
    feedback is the composition the parameters are refined toward.
  - No exploration policy. v0.1 is exploitation-only. The composer's
    behaviour is deterministic given the parameter state; the
    parameter state evolves through feedback. Exploration policies
    (ε-greedy, Thompson sampling on per-parameter posteriors) are a
    post-v0.1 ADR.
  - No reward shaping. The two feedback channels are taken at face
    value; the loop does not learn a learned proxy for either.

## Consequences

  - `hyphae-learning` ships as one of the foundational crates in the
    cherry-pick order.
  - `hyphae-substrate` exposes parameter mutability surfaces from
    commit zero. No retrofit needed when learning loop "lands" — it is
    landed from the start.
  - The `predictive` + `reward` subsystems are not postponed (an
    earlier draft proposed postponing both; that draft was revised
    after this ADR was filed).
  - The journal-and-audit machinery is sized for parameter updates
    from commit zero, not retrofitted later.
  - The architectural bet of Hyphae becomes **testable**: a substrate
    that does not learn is a template engine; a substrate that learns
    within specified bounds with auditable updates is the empirical
    demonstration that compositional CLM architecture can refine itself
    on commodity hardware without GPU training.
  - The Plasticity Charter is promoted from `draft` to `proposed`
    status concurrent with this ADR. Final ratification follows the
    first crate-level implementation of the loop.

## Cross-references

  - **ADR-0001** (fresh-from-v1) — companion decision establishing the
    v2 repository.
  - **ADR-0003** (ethics-radar-firstclass) — defines the Ethics signals
    channel of the learning loop's feedback.
  - **`../hyphae/docs/plasticity/charter_draft.md`** — v1 Plasticity
    Charter draft. The dual-input learning paradigm (P-3) cited there
    is the basis for v2's two-channel design.
  - **celiums-memory journal entry `e8392ddd-b24a-495b-be52-
    7915f8d23efa`** — the BDFL's articulation of the learning-loop
    observation after the AlphaGo documentary, 2026-05-23 evening.
  - **`../hyphae/HYPHAE_PROJECT_OVERVIEW.md` §"Three Observations
    Pending"** — the v1 articulation of the observation as a
    post-Phase-C item. This ADR overrides the deferral for v2.
