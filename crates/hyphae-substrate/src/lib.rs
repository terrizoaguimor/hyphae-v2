// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! # hyphae-substrate
//!
//! The Hyphae v2 substrate runtime.
//!
//! Cherry-picked from v1 with the v2 corrections from ADR-0001 baked
//! in from commit zero:
//!
//! - **Fan-out routing**, with the per-lineage cycle guard
//!   ([`MAX_ROUTING_DEPTH`]) and the total work budget
//!   ([`MAX_ROUTING_WORK`]) split from commit zero. v1 conflated
//!   them in a single hop cap that grew multiplicatively under
//!   fan-out (`ISSUE-M4a-2`).
//! - **Five mandatory ethics evaluation points** ([`CoveragePoint`]),
//!   wired into the substrate API. The substrate calls
//!   `hyphae-ethics` at every point that ingests, composes, or
//!   retrieves content — coverage is part of the contract, not a
//!   future refinement (corrects v1's `PollinatePolicy` /
//!   `DecomposePolicy` / compose / curiosity coverage gap).
//! - **Shared SHA-256 hash chain.** Substrate and ethics audit share
//!   one journal handle (`Arc<Mutex<Journal>>`); one chain per
//!   substrate, per ADR-0003 §8.
//! - **Learning-loop hooks** are mutability surfaces typed on the
//!   substrate API from commit zero — `propose_learning_update`
//!   evaluates the proposed update through the ethics path before
//!   journaling it, so the audit trail covers parameter updates as
//!   well as content operations (ADR-0002 §"Audit and rollback").
//!
//! The substrate deliberately ships **no Phase A / B / D tools**.
//! The v1 tool zoo (`research_*`, `write_*`, `compose_tools`,
//! `retrieval_policy`) was diagnosed as scope creep in ADR-0001 and
//! is not ported. v2 exposes ingress / egress entry points that
//! receive [`ActorContext`] and emit [`EthicsReport`]s alongside the
//! operation's output; concrete tools live in higher-level crates
//! that consume this runtime.

#![warn(missing_docs)]
#![warn(clippy::pedantic)]

use hyphae_core::{
    ActorContext, CognitiveFragment, ConsolidationGateSignal, ExternalInputPayload,
    FragmentContent, FragmentId, LanguageTag, Pathway, PayloadKind, State, Subsystem, SubsystemId,
};
use hyphae_ethics::{
    CoveragePoint, EthicsEngine, EthicsError, EthicsReport, EvaluationInput, ParameterDeltaHint,
};
use hyphae_storage::{Journal, JournalError, StateStore, StateStoreError};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// State store key, under [`SubsystemId::Composer`], of the
/// persisted global state. v1 used the Medulla subsystem for
/// substrate-level metadata; v2 collapses housekeeping into the
/// composer (which already owns substrate-level coordination state
/// via the conversation thread tracker).
const SUBSTRATE_STATE_KEY: &str = "substrate.state";
/// State store key, per subsystem, of that subsystem's checkpoint
/// blob.
const SUBSYSTEM_STATE_KEY: &str = "subsystem";

/// Maximum hop depth a single fragment lineage may travel while
/// routing — the cycle guard. A fragment that crosses this many
/// pathways has almost certainly entered a feedback loop.
pub const MAX_ROUTING_DEPTH: usize = 64;

/// Maximum total fragment deliveries permitted while routing one
/// ingest / recall / consolidation — the work budget. Bounds total
/// work and memory independently of the per-lineage cycle guard,
/// since fan-out multiplies the number of in-flight fragments.
pub const MAX_ROUTING_WORK: usize = 4096;

// ───────────────────────────────────────────────────────────
//  Error
// ───────────────────────────────────────────────────────────

/// Errors raised by the substrate. Per RADAR, no content-level
/// ethics verdict is an error — `EthicsReport`s flow alongside
/// successful operation output. Only infrastructure failures and
/// topology bugs surface here.
#[derive(Debug, Error)]
pub enum SubstrateError {
    /// A subsystem with the given ID is not registered.
    #[error("subsystem not registered: {0:?}")]
    SubsystemNotRegistered(SubsystemId),
    /// A subsystem with the given ID is already registered. The
    /// substrate rejects duplicate registration to surface topology
    /// bugs at the boundary instead of letting them silently replace
    /// a live subsystem.
    #[error("subsystem already registered: {0:?}")]
    DuplicateSubsystemId(SubsystemId),
    /// A core operation failed.
    #[error("core error: {0}")]
    Core(#[from] hyphae_core::HyphaeError),
    /// A journal operation failed.
    #[error("journal error: {0}")]
    Journal(#[from] JournalError),
    /// A state store operation failed.
    #[error("state store error: {0}")]
    StateStore(#[from] StateStoreError),
    /// An ethics infrastructure operation failed (e.g. audit append
    /// write failed). Per RADAR, a content-level verdict is NOT an
    /// error — only the infrastructure beneath it can fail this way.
    #[error("ethics infrastructure error: {0}")]
    Ethics(#[from] EthicsError),
    /// Serialisation or deserialisation of substrate state failed.
    #[error("serialization error: {0}")]
    Serialization(String),
    /// Pathway routing exceeded the per-lineage depth or total work
    /// budget. Carries which limit was hit so the caller knows
    /// whether the topology has a cycle (depth) or just an
    /// over-broad fan-out (work).
    #[error("routing limit exceeded: {0}")]
    RoutingLimitExceeded(RoutingLimit),
    /// A transition to [`State::Consolidation`] was vetoed by one or
    /// more subsystems via [`Subsystem::consolidation_gate_signal`].
    /// Carries the count and the aggregated reasons.
    #[error("consolidation gated by {0} subsystem(s): {1}")]
    ConsolidationGated(usize, String),
    /// A learning-loop parameter update was proposed against a
    /// parameter that is **immutable by construction** (grammar,
    /// state machine, pathway topology, schemas, hash-chain
    /// protocol, `PayloadKind` taxonomy, Hard Architectural
    /// Commitments — per ADR-0002 §"Bounds enforcement").
    #[error("learning update touches an immutable substrate surface: {0}")]
    LearningUpdateOutOfBounds(String),
}

/// Which routing limit was hit. Surfaced in
/// [`SubstrateError::RoutingLimitExceeded`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingLimit {
    /// Per-lineage hop depth exceeded — almost certainly a cycle in
    /// the pathway graph.
    Depth,
    /// Total work budget exceeded — the fan-out grew faster than the
    /// budget allows.
    Work,
}

impl std::fmt::Display for RoutingLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Depth => write!(f, "per-lineage depth ({MAX_ROUTING_DEPTH} hops)"),
            Self::Work => write!(f, "total work budget ({MAX_ROUTING_WORK} deliveries)"),
        }
    }
}

// ───────────────────────────────────────────────────────────
//  Operation outputs
//
//  Each substrate entry point returns the operation's payload + the
//  `EthicsReport` that ran at the corresponding coverage point.
//  RADAR semantics — the report flows alongside success, never as
//  an error verdict.
// ───────────────────────────────────────────────────────────

/// Output of [`Substrate::ingest`].
#[derive(Debug, Clone)]
pub struct IngestOutput {
    /// Terminal fragments of the encoding chain — fragments that
    /// reached a subsystem with no further outgoing pathway of the
    /// `Encoding` kind.
    pub terminals: Vec<CognitiveFragment>,
    /// The ethics report from the `Remember` coverage point.
    pub ethics: EthicsReport,
}

/// Output of [`Substrate::recall_signal`].
#[derive(Debug, Clone)]
pub struct RecallOutput {
    /// Terminal fragments of the recall return path — fragments
    /// that reached a subsystem with no further outgoing pathway of
    /// the `BottomUpPredictionError` kind. In v0.1 the recall
    /// pipeline shape lives in subsystems; the substrate exposes
    /// the entry point and the ethics evaluation.
    pub terminals: Vec<CognitiveFragment>,
    /// The ethics report from the `Recall` coverage point.
    pub ethics: EthicsReport,
}

/// Output of [`Substrate::compose_signal`].
#[derive(Debug, Clone)]
pub struct ComposeOutput {
    /// The composition the composer produced (none in v0.1 until
    /// the composer subsystem lands; the substrate exposes the
    /// hook).
    pub composition: Option<CognitiveFragment>,
    /// The ethics report from the `Compose` coverage point.
    pub ethics: EthicsReport,
}

/// Output of [`Substrate::propose_learning_update`].
#[derive(Debug, Clone)]
pub struct LearningUpdateOutput {
    /// Sequence number of the audit journal entry for this update,
    /// if the update was accepted and audited. `None` when the
    /// proposal was rejected at bounds-check time.
    pub audit_seq: Option<u64>,
    /// The ethics report from the `LearningUpdate` coverage point.
    pub ethics: EthicsReport,
}

/// What kind of parameter a learning-loop update targets. The
/// substrate validates that the proposed target is in the
/// refinable-parameter set per ADR-0002 §"Refinable parameters".
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LearningTarget {
    /// `episodic.conductivity_weights[edge_id]`.
    EpisodicConductivityWeight {
        /// Identifier of the edge in the conductivity graph.
        edge_id: String,
    },
    /// `valence.salience_weights[category]`.
    ValenceSalienceWeight {
        /// The taxonomy-or-domain identifier.
        category: String,
    },
    /// `cascade.threshold` or `cascade.hop_decay_factor`.
    CascadeParameter {
        /// Name of the cascade parameter — e.g. `"threshold"`,
        /// `"hop_decay_factor"`, `"max_hops"`.
        name: String,
    },
    /// `composer.schema_selection_priors[schema_id]`.
    ComposerSchemaPrior {
        /// Identifier of the schema.
        schema_id: String,
    },
    /// `composer.honest_limitation_thresholds[trigger_id]`.
    ComposerLimitationThreshold {
        /// Identifier of the limitation trigger.
        trigger_id: String,
    },
}

impl LearningTarget {
    /// Stable lowercase tag for audit-body grepability.
    #[must_use]
    pub fn tag(&self) -> &'static str {
        match self {
            Self::EpisodicConductivityWeight { .. } => "episodic.conductivity_weight",
            Self::ValenceSalienceWeight { .. } => "valence.salience_weight",
            Self::CascadeParameter { .. } => "cascade.parameter",
            Self::ComposerSchemaPrior { .. } => "composer.schema_prior",
            Self::ComposerLimitationThreshold { .. } => "composer.limitation_threshold",
        }
    }
}

/// A learning-loop parameter update proposal. The substrate
/// validates the target is in the refinable-parameter set, evaluates
/// the proposal at the [`CoveragePoint::LearningUpdate`] ethics
/// point, and journals the update as an
/// [`hyphae_core::JournalEntryType::AuditLearningUpdate`] entry.
#[derive(Debug, Clone)]
pub struct LearningUpdateProposal {
    /// Which parameter is being updated.
    pub target: LearningTarget,
    /// Old value, serialised as bytes.
    pub old_value: Vec<u8>,
    /// New value, serialised as bytes.
    pub new_value: Vec<u8>,
    /// Optional ethics-signal hint that motivated this update — the
    /// learning loop derives proposals from
    /// [`hyphae_ethics::EthicsSignals::learning_weight_delta`].
    pub triggered_by: Option<ParameterDeltaHint>,
    /// Human-readable rationale for audit reading.
    pub rationale: String,
}

// ───────────────────────────────────────────────────────────
//  Substrate
// ───────────────────────────────────────────────────────────

/// The Hyphae v2 substrate runtime.
///
/// Coordinates subsystems, routes typed pathways, owns the global
/// state machine, and threads every content-touching operation
/// through the five-point ethics evaluation. Constructed with
/// [`Substrate::new`] over a storage path; the journal handle is
/// shared with the embedded [`EthicsEngine`] so substrate events
/// and ethics audit live on the same SHA-256 chain (ADR-0003 §8).
pub struct Substrate {
    subsystems: HashMap<SubsystemId, Arc<RwLock<Box<dyn Subsystem>>>>,
    pathways: Vec<Box<dyn Pathway>>,
    state: Arc<RwLock<State>>,
    journal: Arc<std::sync::Mutex<Journal>>,
    state_store: Arc<StateStore>,
    ethics: Arc<EthicsEngine>,
}

impl std::fmt::Debug for Substrate {
    /// Custom `Debug` impl — the substrate owns lock-guarded state
    /// and a `dyn Subsystem`/`dyn Pathway` collection that cannot
    /// derive `Debug`. We surface a compact summary (counts of the
    /// two collections) so logs do not block on locks and do not
    /// leak operational content. The full state is observable via
    /// the read-only accessors (`current_state`, `ethics`).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Substrate")
            .field("subsystem_count", &self.subsystems.len())
            .field("pathway_count", &self.pathways.len())
            .finish_non_exhaustive()
    }
}

impl Substrate {
    /// Open or create a substrate at the given storage path.
    ///
    /// Creates two on-disk artifacts:
    /// - `<path>/journal/` — fjall-backed journal with SHA-256 hash
    ///   chain (shared with [`EthicsEngine`]).
    /// - `<path>/state.redb` — redb-backed subsystem state store.
    ///
    /// # Errors
    ///
    /// Returns an error if either store cannot be opened.
    pub fn new(path: impl AsRef<Path>) -> Result<Self, SubstrateError> {
        let path = path.as_ref();
        std::fs::create_dir_all(path)
            .map_err(|e| SubstrateError::Serialization(format!("create substrate dir: {e}")))?;
        let journal = Journal::open(path.join("journal"))?;
        let state_store = StateStore::open(path.join("state.redb"))?;
        let journal_arc = Arc::new(std::sync::Mutex::new(journal));
        let ethics = EthicsEngine::with_shared_audit(Arc::clone(&journal_arc));
        Ok(Self {
            subsystems: HashMap::new(),
            pathways: Vec::new(),
            state: Arc::new(RwLock::new(State::Encoding)),
            journal: journal_arc,
            state_store: Arc::new(state_store),
            ethics: Arc::new(ethics),
        })
    }

    /// Register a subsystem with the substrate.
    ///
    /// # Errors
    ///
    /// Returns [`SubstrateError::DuplicateSubsystemId`] if a
    /// subsystem with the same [`SubsystemId`] has already been
    /// registered. Duplicate registration is a topology bug — a
    /// stale subsystem with checkpointed state would otherwise be
    /// silently replaced by a fresh one mid-build.
    pub fn register(&mut self, subsystem: Box<dyn Subsystem>) -> Result<(), SubstrateError> {
        let id = subsystem.id();
        if self.subsystems.contains_key(&id) {
            return Err(SubstrateError::DuplicateSubsystemId(id));
        }
        self.subsystems.insert(id, Arc::new(RwLock::new(subsystem)));
        tracing::info!("registered subsystem {:?}", id);
        Ok(())
    }

    /// Register a pathway. Pathways are the typed conduits the
    /// substrate routes fragments along.
    pub fn register_pathway(&mut self, pathway: Box<dyn Pathway>) {
        tracing::info!(
            "registered pathway {:?}: {:?} -> {:?} ({:?})",
            pathway.id(),
            pathway.source(),
            pathway.destination(),
            pathway.kind(),
        );
        self.pathways.push(pathway);
    }

    /// Read-only access to the embedded ethics engine. Useful for
    /// tests and for the higher-level operation crates that need
    /// to inspect the active profile.
    #[must_use]
    pub fn ethics(&self) -> &EthicsEngine {
        &self.ethics
    }

    /// Get the current global state.
    pub async fn current_state(&self) -> State {
        *self.state.read().await
    }

    /// Transition to a new state.
    ///
    /// # Errors
    ///
    /// Returns an error if the transition is invalid per
    /// [`State::valid_transitions`], or — when entering
    /// [`State::Consolidation`] — if one or more subsystems veto via
    /// [`Subsystem::consolidation_gate_signal`].
    pub async fn transition_to(&self, new_state: State) -> Result<(), SubstrateError> {
        let mut state = self.state.write().await;
        let old_state = *state;
        if !old_state.valid_transitions().contains(&new_state) {
            return Err(SubstrateError::Core(
                hyphae_core::HyphaeError::InvalidStateTransition {
                    from: old_state,
                    to: new_state,
                },
            ));
        }
        // Consolidation is gated. Ask every registered subsystem
        // for its consolidation gate signal; if any vetoes, refuse
        // the transition with the aggregated reasons.
        if new_state == State::Consolidation {
            let mut blocks: Vec<String> = Vec::new();
            for (id, subsystem) in &self.subsystems {
                let s = subsystem.read().await;
                if let ConsolidationGateSignal::Block(reason) = s.consolidation_gate_signal() {
                    blocks.push(format!("{id:?}: {reason}"));
                }
            }
            if !blocks.is_empty() {
                let count = blocks.len();
                let joined = blocks.join("; ");
                tracing::info!(
                    "consolidation transition refused by {count} subsystem(s): {joined}"
                );
                return Err(SubstrateError::ConsolidationGated(count, joined));
            }
        }
        *state = new_state;
        // Notify all subsystems of the state change.
        for subsystem in self.subsystems.values() {
            let mut s = subsystem.write().await;
            s.on_state_change(old_state, new_state)?;
        }
        // On Recovery, verify the chain. A break halts the substrate
        // — the caller surfaces the break point.
        if new_state == State::Recovery {
            let journal = self
                .journal
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            journal.verify()?;
        }
        tracing::info!("state transition: {:?} -> {:?}", old_state, new_state);
        Ok(())
    }

    /// Persist the global state and every registered subsystem's
    /// state, then record a `checkpoint` event in the journal.
    ///
    /// # Errors
    ///
    /// Returns an error if state cannot be serialised, a subsystem
    /// checkpoint fails, or a storage write fails.
    pub async fn checkpoint(&self) -> Result<(), SubstrateError> {
        let state = *self.state.read().await;
        let state_bytes =
            bincode::serialize(&state).map_err(|e| SubstrateError::Serialization(e.to_string()))?;
        self.state_store
            .put(SubsystemId::Composer, SUBSTRATE_STATE_KEY, state_bytes)?;

        for (id, subsystem) in &self.subsystems {
            let bytes = subsystem.read().await.checkpoint()?;
            self.state_store.put(*id, SUBSYSTEM_STATE_KEY, bytes)?;
        }

        let mut journal = self
            .journal
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        journal.append("checkpoint", Vec::new())?;
        tracing::info!("checkpoint complete in state {:?}", state);
        Ok(())
    }

    /// Restore each registered subsystem from its persisted
    /// checkpoint blob. Subsystems with no persisted state are left
    /// untouched.
    ///
    /// # Errors
    ///
    /// Returns an error if a storage read fails or a subsystem
    /// rejects its persisted bytes.
    pub async fn restore_subsystems(&self) -> Result<(), SubstrateError> {
        for (id, subsystem) in &self.subsystems {
            if let Some(bytes) = self.state_store.get(*id, SUBSYSTEM_STATE_KEY)? {
                subsystem.write().await.restore(&bytes)?;
                tracing::info!("restored subsystem {:?}", id);
            }
        }
        Ok(())
    }

    // ───────────────────────────────────────────────────────────
    //  Five mandatory ethics evaluation points (ADR-0003 §3).
    //
    //  Every method that ingests, composes, or retrieves content
    //  threads through the ethics engine and returns the report
    //  alongside the operation output. The wire is part of the
    //  contract.
    // ───────────────────────────────────────────────────────────

    /// **Coverage point 1 — `Remember`.** Accept external input at
    /// the `External → input-gate` boundary. The content is
    /// evaluated through the ethics engine BEFORE encoding;
    /// fragments inherit the audit-journal sequence of their
    /// evaluation.
    ///
    /// # Errors
    ///
    /// Returns an error if the `input-gate` subsystem is not
    /// registered, the ethics audit append fails, or routing
    /// exceeds a budget.
    pub async fn ingest(
        &self,
        payload: ExternalInputPayload,
        actor: ActorContext,
    ) -> Result<IngestOutput, SubstrateError> {
        let ethics_report = self.ethics.evaluate(&EvaluationInput {
            content: &payload.content,
            language: LanguageTag::default(),
            coverage_point: CoveragePoint::Remember,
            actor: actor.clone(),
        })?;

        let mut fragment = CognitiveFragment::new(
            FragmentContent::Observation {
                body: payload.content.clone(),
            },
            "External",
        );
        fragment.saliency = payload.saliency;

        // Mirror the v1 audit: write an external_input entry on the
        // shared chain (distinct from the ethics audit entry).
        {
            let mut journal = self
                .journal
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            journal.append("external_input", payload.content.into_bytes())?;
        }

        let current_state = *self.state.read().await;
        let input_gate = self.subsystems.get(&SubsystemId::InputGate).ok_or(
            SubstrateError::SubsystemNotRegistered(SubsystemId::InputGate),
        )?;
        let emitted =
            input_gate
                .write()
                .await
                .process(fragment, PayloadKind::Encoding, current_state)?;

        let terminals = self
            .route(
                SubsystemId::InputGate,
                emitted,
                PayloadKind::Encoding,
                current_state,
            )
            .await?;

        Ok(IngestOutput {
            terminals,
            ethics: ethics_report,
        })
    }

    /// **Coverage point 2 — `Recall`.** Surface a recall cue to the
    /// substrate. The cue text is evaluated through the ethics
    /// engine BEFORE entering the recall pathway. In v0.1 the
    /// pipeline shape lives in the `episodic` and `composer`
    /// subsystems — the substrate exposes the entry point with the
    /// ethics hook wired.
    ///
    /// # Errors
    ///
    /// Returns an error if the `episodic` subsystem is not
    /// registered, the ethics audit append fails, or routing
    /// exceeds a budget.
    pub async fn recall_signal(
        &self,
        cue: &str,
        actor: ActorContext,
    ) -> Result<RecallOutput, SubstrateError> {
        let ethics_report = self.ethics.evaluate(&EvaluationInput {
            content: cue,
            language: LanguageTag::default(),
            coverage_point: CoveragePoint::Recall,
            actor: actor.clone(),
        })?;

        let cue_fragment = CognitiveFragment::new(
            FragmentContent::Observation {
                body: cue.to_string(),
            },
            "External",
        );

        let current_state = *self.state.read().await;
        let episodic = self.subsystems.get(&SubsystemId::Episodic).ok_or(
            SubstrateError::SubsystemNotRegistered(SubsystemId::Episodic),
        )?;
        let emitted = episodic.write().await.process(
            cue_fragment,
            PayloadKind::BottomUpPredictionError,
            current_state,
        )?;

        let terminals = self
            .route(
                SubsystemId::Episodic,
                emitted,
                PayloadKind::BottomUpPredictionError,
                current_state,
            )
            .await?;

        Ok(RecallOutput {
            terminals,
            ethics: ethics_report,
        })
    }

    /// **Coverage point 3 — `Compose`.** Surface a candidate
    /// composition to the substrate. The candidate is evaluated
    /// through the ethics engine BEFORE being emitted to the
    /// caller. v0.1 returns the candidate unchanged alongside the
    /// report; the surface realizer (when it lands) consults the
    /// report's `signals.composer_should_acknowledge` to decide
    /// whether to add a limitation acknowledgment slot.
    ///
    /// # Errors
    ///
    /// Returns an error if the ethics audit append fails.
    ///
    /// The method is `async` for API uniformity with the rest of
    /// the substrate surface even though v0.1's implementation
    /// performs no internal `await` — future cabling (composer
    /// state lookup, working-set materialisation) will use the
    /// async runtime, and changing the signature later is a
    /// breaking change for every caller.
    #[allow(clippy::unused_async)]
    pub async fn compose_signal(
        &self,
        candidate: Option<CognitiveFragment>,
        candidate_text: &str,
        actor: ActorContext,
    ) -> Result<ComposeOutput, SubstrateError> {
        let ethics_report = self.ethics.evaluate(&EvaluationInput {
            content: candidate_text,
            language: LanguageTag::default(),
            coverage_point: CoveragePoint::Compose,
            actor,
        })?;
        Ok(ComposeOutput {
            composition: candidate,
            ethics: ethics_report,
        })
    }

    /// **Coverage point 5 — `LearningUpdate`.** Propose a
    /// learning-loop parameter update. The substrate runs three
    /// checks before journaling the update:
    ///
    /// 1. **Bounds check** — the target must be in the refinable-
    ///    parameter set per ADR-0002 §"Refinable parameters".
    ///    Anything else returns
    ///    [`SubstrateError::LearningUpdateOutOfBounds`].
    /// 2. **Ethics evaluation** — the rationale is evaluated at the
    ///    [`CoveragePoint::LearningUpdate`] point. Per RADAR the
    ///    report does NOT block the update; signals propagate to
    ///    callers so the learning loop can choose to defer.
    /// 3. **Audit append** — on acceptance the proposal is
    ///    journaled as an `audit_learning_update` entry on the
    ///    shared chain.
    ///
    /// # Errors
    ///
    /// Returns [`SubstrateError::LearningUpdateOutOfBounds`] when
    /// the target is not refinable, or an audit-journal error when
    /// the append fails.
    ///
    /// `async` for API uniformity with the rest of the substrate
    /// surface; v0.1 performs no internal `await` but future
    /// implementations (learning-loop coordination, multi-subsystem
    /// parameter broadcast) will.
    #[allow(clippy::unused_async)]
    pub async fn propose_learning_update(
        &self,
        proposal: LearningUpdateProposal,
        actor: ActorContext,
    ) -> Result<LearningUpdateOutput, SubstrateError> {
        // (1) Bounds check. The substrate exposes only refinable
        // parameter surfaces; the `LearningTarget` enum encodes the
        // bound at the type level. Any future immutable-target
        // attempt that bypasses the enum would still pass through
        // here — we keep the dynamic check so an attacker cannot
        // exploit a serialisation edge case.
        let tag = proposal.target.tag();
        if !is_refinable(&proposal.target) {
            return Err(SubstrateError::LearningUpdateOutOfBounds(tag.to_string()));
        }

        // (2) Ethics evaluation at the LearningUpdate point.
        let ethics_report = self.ethics.evaluate(&EvaluationInput {
            content: &proposal.rationale,
            language: LanguageTag::default(),
            coverage_point: CoveragePoint::LearningUpdate,
            actor,
        })?;

        // (3) Journal the update. The full old/new bytes ride the
        // chain so `hyphae-learning` can reconstruct parameter state
        // by journal replay (ADR-0002 §"Audit and rollback"). Unlike
        // the ethics audit (which fingerprints user content for
        // privacy), parameter values are internal model state and
        // safe to carry verbatim.
        let entry_payload = LearningUpdateAuditPayload {
            target_tag: tag.to_string(),
            old_value: proposal.old_value,
            new_value: proposal.new_value,
            rationale: proposal.rationale,
            triggered_by_hint: proposal.triggered_by,
        };
        let bytes = bincode::serialize(&entry_payload)
            .map_err(|e| SubstrateError::Serialization(e.to_string()))?;
        let seq = {
            let mut journal = self
                .journal
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let (seq, _hash) = journal.append("audit_learning_update", bytes)?;
            seq
        };

        Ok(LearningUpdateOutput {
            audit_seq: Some(seq),
            ethics: ethics_report,
        })
    }

    // ───────────────────────────────────────────────────────────
    //  Routing
    // ───────────────────────────────────────────────────────────

    /// Find every registered pathway whose source is `source`, whose
    /// payload kind matches `kind`, and which is active in `state`.
    ///
    /// More than one such pathway is a legitimate fan-out: the
    /// router delivers a copy of the fragment along each.
    fn matching_pathways(
        &self,
        source: SubsystemId,
        kind: PayloadKind,
        state: State,
    ) -> Vec<&dyn Pathway> {
        self.pathways
            .iter()
            .map(|pathway| &**pathway)
            .filter(|pathway| {
                pathway.source() == source
                    && pathway.kind() == kind
                    && pathway
                        .state_gate()
                        .is_none_or(|gate| gate.contains(&state))
            })
            .collect()
    }

    /// Route fragments emitted by `origin` along the registered
    /// pathways of the given [`PayloadKind`], delivering each to its
    /// destination subsystem and following the destination's onward
    /// emissions.
    ///
    /// A subsystem with several matching outgoing pathways **fans
    /// out**: a copy of the fragment — carrying a fresh
    /// [`FragmentId`], with the original recorded in
    /// `provenance.parent_ids` — is delivered along each. Returns
    /// the fragments that reached a subsystem with no further
    /// outgoing pathway of that kind — the terminal fragments of
    /// the flow.
    ///
    /// Routing is bounded twice (split from commit zero, corrects
    /// v1's ISSUE-M4a-2): each fragment lineage may travel at most
    /// [`MAX_ROUTING_DEPTH`] hops (the cycle guard), and one
    /// `route` call may perform at most [`MAX_ROUTING_WORK`]
    /// deliveries (the work budget).
    async fn route(
        &self,
        origin: SubsystemId,
        fragments: Vec<CognitiveFragment>,
        kind: PayloadKind,
        state: State,
    ) -> Result<Vec<CognitiveFragment>, SubstrateError> {
        let mut terminal = Vec::new();
        let mut queue: VecDeque<(SubsystemId, CognitiveFragment, usize)> =
            fragments.into_iter().map(|f| (origin, f, 0usize)).collect();
        let mut work: usize = 0;

        while let Some((from, fragment, depth)) = queue.pop_front() {
            work += 1;
            if work > MAX_ROUTING_WORK {
                return Err(SubstrateError::RoutingLimitExceeded(RoutingLimit::Work));
            }
            if depth > MAX_ROUTING_DEPTH {
                return Err(SubstrateError::RoutingLimitExceeded(RoutingLimit::Depth));
            }

            // Transmit synchronously so no borrow of `self.pathways`
            // is held across the `await` on the destination
            // subsystems below.
            let mut hops_out: Vec<(SubsystemId, CognitiveFragment)> = Vec::new();
            {
                let matching = self.matching_pathways(from, kind, state);
                match matching.as_slice() {
                    [] => terminal.push(fragment),
                    [pathway] => {
                        hops_out.push((pathway.destination(), pathway.transmit(fragment)?));
                    }
                    fanout => {
                        // Fan out: each pathway receives its own
                        // copy, with a fresh id so the copies
                        // remain distinct fragments. The original
                        // id is recorded in `parent_ids` so the
                        // audit trail is complete.
                        for pathway in fanout {
                            let mut copy = fragment.clone();
                            copy.provenance.parent_ids.push(copy.id);
                            copy.id = FragmentId::new();
                            hops_out.push((pathway.destination(), pathway.transmit(copy)?));
                        }
                    }
                }
            }

            for (destination, delivered) in hops_out {
                let subsystem = self
                    .subsystems
                    .get(&destination)
                    .ok_or(SubstrateError::SubsystemNotRegistered(destination))?;
                // The destination is told which pathway kind
                // delivered the fragment (the v1 RFC-042 fix, here
                // from commit zero).
                let emissions = subsystem.write().await.process(delivered, kind, state)?;
                for emission in emissions {
                    queue.push_back((destination, emission, depth + 1));
                }
            }
        }
        Ok(terminal)
    }
}

// ───────────────────────────────────────────────────────────
//  Internal types
// ───────────────────────────────────────────────────────────

/// Whether a learning target is in the refinable-parameter set per
/// ADR-0002 §"Refinable parameters". Today every variant of
/// [`LearningTarget`] is refinable — the function exists so the
/// substrate enforces the bound dynamically as well as
/// statically (defence in depth against a future variant or
/// deserialisation edge case).
fn is_refinable(target: &LearningTarget) -> bool {
    matches!(
        target,
        LearningTarget::EpisodicConductivityWeight { .. }
            | LearningTarget::ValenceSalienceWeight { .. }
            | LearningTarget::CascadeParameter { .. }
            | LearningTarget::ComposerSchemaPrior { .. }
            | LearningTarget::ComposerLimitationThreshold { .. }
    )
}

/// Payload of an `audit_learning_update` journal entry. Public so
/// `hyphae-learning` can deserialise journal entries during
/// rollback replay (ADR-0002 §"Audit and rollback"). Carries the
/// full old / new byte values — parameter state is internal model
/// state, not user content, so it is safe to journal verbatim.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LearningUpdateAuditPayload {
    /// `LearningTarget::tag()` of the parameter being updated.
    pub target_tag: String,
    /// The parameter's value before the update, as bytes.
    pub old_value: Vec<u8>,
    /// The parameter's new value, as bytes.
    pub new_value: Vec<u8>,
    /// Human-readable rationale.
    pub rationale: String,
    /// Optional ethics-signal hint that motivated the update.
    pub triggered_by_hint: Option<ParameterDeltaHint>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyphae_core::Result as CoreResult;
    use std::any::Any;
    use tempfile::tempdir;

    /// A stub subsystem used to exercise the substrate's
    /// registration guards and routing behaviour.
    enum StubMode {
        /// Subsystem emits a fixed payload regardless of the
        /// incoming fragment.
        Emit(Vec<CognitiveFragment>),
        /// Subsystem emits the delivered fragment unchanged (its
        /// emissions then route onward, or land as terminals if
        /// the subsystem has no outbound pathway).
        Passthrough,
    }

    struct StubSubsystem {
        id: SubsystemId,
        mode: StubMode,
    }

    impl StubSubsystem {
        fn new(id: SubsystemId) -> Self {
            Self {
                id,
                mode: StubMode::Emit(Vec::new()),
            }
        }

        fn emitting(id: SubsystemId, emit: Vec<CognitiveFragment>) -> Self {
            Self {
                id,
                mode: StubMode::Emit(emit),
            }
        }

        fn passthrough(id: SubsystemId) -> Self {
            Self {
                id,
                mode: StubMode::Passthrough,
            }
        }
    }

    impl Subsystem for StubSubsystem {
        fn id(&self) -> SubsystemId {
            self.id
        }

        fn process(
            &mut self,
            fragment: CognitiveFragment,
            _incoming: PayloadKind,
            _state: State,
        ) -> CoreResult<Vec<CognitiveFragment>> {
            match &self.mode {
                StubMode::Emit(payload) => Ok(payload.clone()),
                StubMode::Passthrough => Ok(vec![fragment]),
            }
        }

        fn checkpoint(&self) -> CoreResult<Vec<u8>> {
            Ok(Vec::new())
        }

        fn restore(&mut self, _bytes: &[u8]) -> CoreResult<()> {
            Ok(())
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    fn obs(body: &str) -> CognitiveFragment {
        CognitiveFragment::new(
            FragmentContent::Observation {
                body: body.to_string(),
            },
            "test",
        )
    }

    fn driver() -> ActorContext {
        ActorContext::new("test:driver", "memory:write")
    }

    #[tokio::test]
    async fn substrate_initializes_in_encoding_state() {
        let dir = tempdir().unwrap();
        let substrate = Substrate::new(dir.path()).unwrap();
        assert_eq!(substrate.current_state().await, State::Encoding);
    }

    #[tokio::test]
    async fn substrate_rejects_invalid_state_transitions() {
        let dir = tempdir().unwrap();
        let substrate = Substrate::new(dir.path()).unwrap();
        // Encoding → Recall is valid; Recall → Consolidation is NOT
        substrate.transition_to(State::Recall).await.unwrap();
        let result = substrate.transition_to(State::Consolidation).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn substrate_rejects_duplicate_subsystem_registration() {
        let dir = tempdir().unwrap();
        let mut substrate = Substrate::new(dir.path()).unwrap();
        substrate
            .register(Box::new(StubSubsystem::new(SubsystemId::Composer)))
            .unwrap();
        let duplicate = substrate.register(Box::new(StubSubsystem::new(SubsystemId::Composer)));
        assert!(matches!(
            duplicate,
            Err(SubstrateError::DuplicateSubsystemId(SubsystemId::Composer)),
        ));
    }

    #[tokio::test]
    async fn ingest_runs_ethics_at_remember_point_and_emits_report() {
        let dir = tempdir().unwrap();
        let mut substrate = Substrate::new(dir.path()).unwrap();
        substrate
            .register(Box::new(StubSubsystem::new(SubsystemId::InputGate)))
            .unwrap();
        let output = substrate
            .ingest(ExternalInputPayload::new("the weather is fine"), driver())
            .await
            .unwrap();
        assert_eq!(output.ethics.coverage_point, CoveragePoint::Remember);
        assert!(output.ethics.audit_seq.is_some());
    }

    #[tokio::test]
    async fn ingest_fails_when_input_gate_unregistered() {
        let dir = tempdir().unwrap();
        let substrate = Substrate::new(dir.path()).unwrap();
        // No input-gate registered. The ethics evaluation still
        // succeeds (its audit entry lands on the shared chain) —
        // but the routing then fails because no subsystem is
        // there to process the fragment.
        let err = substrate
            .ingest(ExternalInputPayload::new("anything"), driver())
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            SubstrateError::SubsystemNotRegistered(SubsystemId::InputGate),
        ));
    }

    #[tokio::test]
    async fn recall_runs_ethics_at_recall_point() {
        let dir = tempdir().unwrap();
        let mut substrate = Substrate::new(dir.path()).unwrap();
        substrate
            .register(Box::new(StubSubsystem::new(SubsystemId::Episodic)))
            .unwrap();
        let output = substrate.recall_signal("hello", driver()).await.unwrap();
        assert_eq!(output.ethics.coverage_point, CoveragePoint::Recall);
    }

    #[tokio::test]
    async fn compose_runs_ethics_at_compose_point() {
        let dir = tempdir().unwrap();
        let substrate = Substrate::new(dir.path()).unwrap();
        let candidate = obs("a candidate composition");
        let output = substrate
            .compose_signal(Some(candidate.clone()), "a candidate composition", driver())
            .await
            .unwrap();
        assert_eq!(output.ethics.coverage_point, CoveragePoint::Compose);
        assert!(output.composition.is_some());
    }

    #[tokio::test]
    async fn learning_update_runs_ethics_and_audits() {
        let dir = tempdir().unwrap();
        let substrate = Substrate::new(dir.path()).unwrap();
        let proposal = LearningUpdateProposal {
            target: LearningTarget::EpisodicConductivityWeight {
                edge_id: "frag_a:frag_b".to_string(),
            },
            old_value: vec![0u8; 4],
            new_value: vec![1u8; 4],
            triggered_by: None,
            rationale: "co-encoding count crossed a threshold".to_string(),
        };
        let output = substrate
            .propose_learning_update(proposal, driver())
            .await
            .unwrap();
        assert_eq!(output.ethics.coverage_point, CoveragePoint::LearningUpdate);
        assert!(output.audit_seq.is_some());
    }

    /// Routing fans a fragment to every matching pathway and assigns
    /// each fan-out copy a fresh [`FragmentId`].
    #[tokio::test]
    async fn routing_fans_out_with_unique_ids_per_copy() {
        use hyphae_core::{DirectPathway, PathwayId};
        let dir = tempdir().unwrap();
        let mut substrate = Substrate::new(dir.path()).unwrap();
        // input-gate emits one fragment; pathways to episodic and
        // valence both deliver an Encoding-kind copy.
        substrate
            .register(Box::new(StubSubsystem::emitting(
                SubsystemId::InputGate,
                vec![obs("encoded payload")],
            )))
            .unwrap();
        substrate
            .register(Box::new(StubSubsystem::passthrough(SubsystemId::Episodic)))
            .unwrap();
        substrate
            .register(Box::new(StubSubsystem::passthrough(SubsystemId::Valence)))
            .unwrap();
        substrate.register_pathway(Box::new(DirectPathway::new(
            PathwayId(1),
            PayloadKind::Encoding,
            SubsystemId::InputGate,
            SubsystemId::Episodic,
        )));
        substrate.register_pathway(Box::new(DirectPathway::new(
            PathwayId(2),
            PayloadKind::Encoding,
            SubsystemId::InputGate,
            SubsystemId::Valence,
        )));
        let out = substrate
            .ingest(ExternalInputPayload::new("payload"), driver())
            .await
            .unwrap();
        // Both Episodic and Valence are terminal sinks for the
        // Encoding kind (no outbound pathways), so two terminals
        // emerge — one per fan-out copy. Their ids differ because
        // the router assigns a fresh id per copy.
        assert_eq!(out.terminals.len(), 2);
        assert_ne!(out.terminals[0].id, out.terminals[1].id);
    }

    #[tokio::test]
    async fn ethics_audit_and_substrate_events_share_one_chain() {
        let dir = tempdir().unwrap();
        let mut substrate = Substrate::new(dir.path()).unwrap();
        substrate
            .register(Box::new(StubSubsystem::new(SubsystemId::InputGate)))
            .unwrap();
        // The ingest produces (1) an ethics audit entry for the
        // Remember coverage point, (2) an external_input entry on
        // the substrate side. Both land on the same chain — the
        // ethics entry's audit_seq must be 0, the external_input's
        // sequence must be 1.
        let out = substrate
            .ingest(ExternalInputPayload::new("hello"), driver())
            .await
            .unwrap();
        assert_eq!(out.ethics.audit_seq, Some(0));
        // The second event (external_input) holds seq 1. We can't
        // query the journal directly through the substrate API in
        // v0.1, but we observe that a subsequent ingest extends
        // monotonically.
        let out2 = substrate
            .ingest(ExternalInputPayload::new("again"), driver())
            .await
            .unwrap();
        // Out's ethics audit was 0; in_input was 1; out2's ethics
        // audit must be 2; out2's external_input must be 3.
        assert_eq!(out2.ethics.audit_seq, Some(2));
    }

    #[tokio::test]
    async fn checkpoint_roundtrips_subsystem_state() {
        let dir = tempdir().unwrap();
        {
            let mut substrate = Substrate::new(dir.path()).unwrap();
            substrate
                .register(Box::new(StubSubsystem::new(SubsystemId::Composer)))
                .unwrap();
            substrate.checkpoint().await.unwrap();
        }
        let mut substrate = Substrate::new(dir.path()).unwrap();
        substrate
            .register(Box::new(StubSubsystem::new(SubsystemId::Composer)))
            .unwrap();
        substrate.restore_subsystems().await.unwrap();
        // No assertion needed beyond not-erroring; the stub's
        // restore is a no-op and the round-trip is the contract.
    }

    #[tokio::test]
    async fn recovery_state_verifies_chain_integrity() {
        let dir = tempdir().unwrap();
        let mut substrate = Substrate::new(dir.path()).unwrap();
        substrate
            .register(Box::new(StubSubsystem::new(SubsystemId::InputGate)))
            .unwrap();
        substrate
            .ingest(ExternalInputPayload::new("event"), driver())
            .await
            .unwrap();
        // Encoding → Recovery is a valid transition; entering
        // Recovery verifies the chain. A passing chain returns Ok.
        substrate.transition_to(State::Recovery).await.unwrap();
    }
}
