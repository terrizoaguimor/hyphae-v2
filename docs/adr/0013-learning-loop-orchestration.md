<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0013
title: Learning loop orchestration — wire the feedback path that ADR-0002 mandates
status: accepted
date: 2026-05-27
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (audit #34, v0.1 implementation review)]
---

# 0013 — Learning loop orchestration: wire the feedback path ADR-0002 mandates

## Context

Audit #34 (2026-05-27) found:

- `hyphae-learning` exists as a 417-LOC crate with `LearningLoop`,
  `FeedbackSignal`, `ParameterStore`, `record`, `stage_pending`,
  `apply_audited`, `rollback_to`.
- `hyphae-substrate::propose_learning_update` exists (line 723)
  with ethics + journal + bounds check.
- The two halves are **never connected**. `hyphae-substrate`
  does not import `hyphae-learning` (correct per ADR-0002's
  dependency direction). `hyphae-smoke` lists `hyphae-learning`
  in `Cargo.toml` but never imports it. No code path converts
  Predictive/Reward emissions or `EthicsReport` signals into
  `FeedbackSignal`s and drives them through the propose pipeline.

The substrate comment at line 720-721 acknowledges the gap
verbatim: *"future implementations (learning-loop coordination,
multi-subsystem parameter broadcast) will."* That deferral was
honest in v0.1 chartering; ADR-0013 closes it.

The conflict with ADR-0002 is explicit:

> ADR-0002 §"Learning loop first-class": "Substrate (lexicon,
> grammar, schemas, state machine, pathway topology) is immutable.
> Refinable parameters: conductivity weights, salience weights,
> cascade thresholds, schema selection priors. **Feedback signals:
> Reward prediction error (predictive + reward subsystems) +
> Ethics signals (hyphae-ethics). Every update is a journal
> entry; rollback via journal replay.**"

The infrastructure is first-class. The orchestration is dormant.

## Decision

**Add `hyphae_learning::orchestrator::LearningOrchestrator` —
the missing piece that observes substrate emissions, converts
them to `FeedbackSignal`s, drains via `stage_pending`, forwards
proposals to `substrate.propose_learning_update`, and applies
audited values to the parameter store.**

The orchestrator lives in `hyphae-learning` (which already
imports `hyphae-substrate`). Substrate's dependency direction
stays one-way — substrate does not import learning. The
integrator (smoke binary, future CLI) instantiates the
orchestrator alongside the substrate and calls it explicitly.

### Public API (minimum viable)

```rust
pub struct LearningOrchestrator {
    inner: LearningLoop,
}

impl LearningOrchestrator {
    pub fn new() -> Self;
    pub fn with_loop(loop_: LearningLoop) -> Self;

    /// Read-only access to the underlying loop's parameter store.
    pub fn store(&self) -> &ParameterStore;

    /// Mutable access for startup-time bound registration.
    pub fn store_mut(&mut self) -> &mut ParameterStore;

    /// Inspect a substrate operation's emitted terminal and an
    /// optional ethics report; extract any FeedbackSignal they
    /// carry and record it in the underlying loop. Idempotent on
    /// emissions that produce no signals.
    pub fn record_emission(
        &mut self,
        fragment: &CognitiveFragment,
        report: Option<&EthicsReport>,
    );

    /// Drain the loop, forward each staged proposal to the
    /// substrate's audit pipeline, and apply audited values to
    /// the parameter store on success.
    pub async fn drain_and_propose(
        &mut self,
        substrate: &Substrate,
        actor: ActorContext,
    ) -> Result<Vec<LearningUpdateOutput>, SubstrateError>;

    /// Read pending signal count for tests / diagnostics.
    pub fn pending_signal_count(&self) -> usize;
}
```

### Conversion rules — fragment → `FeedbackSignal`

#### Reward subsystem emissions → `RewardPredictionError`

When `fragment.provenance.source_subsystem == "reward"` and
`fragment.provenance.parent_ids` is non-empty:

- `fragment_id` = `parent_ids[0]` (the actual the prediction
  was compared to, per `reward.rs:124`).
- `error` = `fragment.valence` (signed RPE δ).
- `edge_hint` = `Some(format!("{}:{}", parent_ids[0],
  fragment.id))`. v0.1 heuristic — the "edge" identifies the
  conductivity-weight target. A future ADR refines this to use
  the actual graph edge the RPE traversed; v0.1 ships with the
  synthetic identifier so the proposal pipeline fires end-to-end
  rather than being silently dropped by
  `intents_from_signals` (which filters RPE signals without
  `edge_hint`).
- `at` = `SystemTime::now()`.

#### `EthicsReport.signals.learning_weight_delta` → `Ethics`

When the report carries a non-empty hint, reuse the existing
`FeedbackSignal::from_ethics_report` convenience. When the hint
is empty, the helper returns `None` and the orchestrator records
nothing — same RADAR behaviour as the v0.1 ethics engine.

### Orchestration loop

```rust
pub async fn drain_and_propose(...) -> Result<...> {
    let staged = self.inner.stage_pending();
    let mut outputs = Vec::new();
    for s in staged {
        let target = s.proposal.target.clone();
        let value = s.apply_value.clone();
        let output = substrate.propose_learning_update(s.proposal, actor.clone()).await?;
        // RADAR — the substrate's ethics evaluation at
        // CoveragePoint::LearningUpdate emits signals but does not
        // veto. The orchestrator applies on Ok and propagates the
        // ethics signal back through the loop so the next batch
        // can react.
        if let Some(sig) = FeedbackSignal::from_ethics_report(&output.ethics) {
            self.inner.record(sig);
        }
        self.inner.apply_audited(&target, value);
        outputs.push(output);
    }
    Ok(outputs)
}
```

### What "wakeup" means in v0.1

Three loops close:

1. **Recording loop.** Substrate operation terminals + ethics
   reports → `FeedbackSignal`s → `LearningLoop` aggregator.
2. **Proposal loop.** Aggregated signals → `LearningIntent`s →
   bounds-checked `LearningUpdateProposal`s → substrate audit.
3. **Application loop.** Substrate accepts the proposal →
   `apply_audited` mutates the parameter store. The ethics
   signal from the audit feeds back into the recording loop.

All three close without bypassing any v2 commitment: ethics
runs at `CoveragePoint::LearningUpdate`, every accepted update
is journaled, the substrate-not-import-learning direction is
preserved.

### Edge-hint synthesis — known v0.1 heuristic

The RPE-to-conductivity-edge mapping is ambiguous in v0.1:

- The `Reward` subsystem holds only `expected_valence`. It does
  not track which conductivity edge the prediction traversed.
- The `Episodic` subsystem owns the graph but doesn't see RPE
  attribution.

The v0.1 heuristic `format!("{parent_id}:{fragment_id}")` is
**syntactically valid but semantically synthetic**. It exercises
the proposal pipeline end-to-end without making a load-bearing
claim about which real graph edge gets reinforced. A future ADR
introduces:

- Either: `Reward` carries a `last_routed_edge` field tracked
  through the recall path's cascade activation.
- Or: the orchestrator inspects the cascade trace
  (`CascadeActivation.parent_id` chain from ADR-0011) to
  attribute the RPE to a real propagation edge.

Both options need substrate-side changes; deferred to a future
ADR with concrete attribution requirements.

### What this ADR explicitly does **not** do

- **Does not** import learning into substrate. Direction stays
  one-way per ADR-0002.
- **Does not** auto-trigger the orchestrator from inside
  substrate methods. The integrator drives `record_emission`
  and `drain_and_propose` explicitly. Hidden background
  threads break the v0.1 "single integrator, single substrate"
  model.
- **Does not** modify the smoke binary to use the orchestrator.
  Smoke modernisation lands in the v0.1 closure ceremony, not
  this ADR. Smoke's `Cargo.toml` already lists `hyphae-learning`
  but the orchestrator wiring is left for the closure pass.
- **Does not** introduce eligibility traces, importance
  weighting, or convergence diagnostics. The v0.1 aggregator is
  a FIFO; richer signal smoothing is a future ADR.
- **Does not** refine the RPE-to-edge attribution heuristic.
  v0.1 ships the synthetic identifier; refinement is a
  follow-up.

## Sources

- **ADR-0001 §"Hard Commitments"** — the substrate-immutability
  + learning-first-class commitments this ADR honours.
- **ADR-0002** — the canonical authority for the feedback-signal
  model, refinable parameters, and journal-replay rollback.
- **ADR-0003 §"Coverage"** — `CoveragePoint::LearningUpdate`
  ensures every proposal flows through ethics.
- **`hyphae_learning::feedback::FeedbackSignal`** — the type
  the orchestrator constructs.
- **`hyphae_substrate::Substrate::propose_learning_update`** —
  the audit endpoint.
- **Audit #34 (2026-05-27)** — the gap report.

## Consequences

- Three loops (record / propose / apply) close from v0.1
  forward. The orchestrator is the missing wiring.
- `hyphae-learning` gains one new module (`orchestrator`, ~150
  LOC). No new dependencies.
- The integration test demonstrates: substrate routes a
  valenced fragment through Reward, Reward emits an RPE
  fragment, the orchestrator observes it, drains the loop,
  forwards the proposal to the substrate, the audit succeeds,
  the parameter store mutates by the signed delta.
- Audit #34's DORMANT verdict retires. The remaining v0.1
  closure gap is smoke modernisation (use real loop, not
  synthetic working set) — a closure-ceremony task, not a
  separate ADR.
- The ethics signal feedback path (orchestrator records the
  audit's ethics report into the next batch) creates a
  self-tuning loop bounded by RADAR semantics — proposals never
  block but ethics deltas iteratively shape future proposals.

## Cross-references

- **Audit #34** — findings document.
- **ADR-0002 §"Feedback signals (two channels)"** — the source
  for the conversion rules.
- **ADR-0011** — the cascade wakeup whose `parent_id` chain
  unlocks the future RPE-edge-attribution refinement.
- **`hyphae_learning::LearningLoop`** — the type the
  orchestrator wraps.
