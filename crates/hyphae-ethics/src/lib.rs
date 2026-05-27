// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! # hyphae-ethics
//!
//! First-class Ethics Engine for the Hyphae v2 substrate.
//!
//! Per `docs/adr/0003-ethics-radar-firstclass.md`:
//!
//! - **Dedicated crate.** Not distributed across subsystems by
//!   omission as in v1. The crate has no dependency on
//!   `hyphae-substrate` — substrate consumes ethics, never the
//!   reverse.
//! - **RADAR philosophy, not JAIL.** The engine classifies,
//!   audits, and emits [`EthicsSignals`]. It **never blocks** an
//!   operation. Callers receive a [`EthicsReport`] alongside their
//!   composition; the signals feed the composer (limitation
//!   acknowledgment) and the learning loop (parameter delta
//!   hints).
//! - **Five-point cognition-path coverage.** The substrate calls
//!   `evaluate` at the five mandatory evaluation points (remember,
//!   recall / cascade, compose, grounded retrieval, learning-loop
//!   parameter updates). No path that ingests, composes, or
//!   retrieves content bypasses the engine.
//! - **Layers in v0.1.** Layer A (deterministic) and Layer B (`CVaR`
//!   plus categorical hard rules). Layer C (multi-framework
//!   philosophical) is deferred per ADR-0003 §"Layer C deferral".
//!   Layer K (precedent advisory) is deferred per ADR-0003 §"Layer K
//!   deferral".
//! - **Shared SHA-256 chain.** Audit entries land on the same
//!   `hyphae-storage::Journal` as substrate events, one chain per
//!   substrate, per ADR-0003 §8.

#![warn(missing_docs)]
#![warn(clippy::pedantic)]

pub mod audit;
pub mod disambiguation;
pub mod layer_a;
pub mod layer_b;
pub mod lexicon;
pub mod profile;
pub mod structural;
pub mod taxonomy;
pub mod thresholds;

pub use audit::{AuditEntryPayload, ETHICS_AUDIT_EVENT_KIND, content_fingerprint};
pub use disambiguation::{DisambiguationVerdict, Disambiguator};
pub use layer_a::{FlagSource, LayerA, LayerAFlag, LayerAOutput};
pub use layer_b::{CategoricalVerdict, LayerB, LayerBOutput};
pub use lexicon::{Lexicon, LexiconEntry};
pub use profile::{LayerBParams, Profile};
pub use structural::{StructuralDetector, StructuralHit};
pub use taxonomy::TaxonomyCategory;
pub use thresholds::{ThresholdPair, ThresholdSet};

use hyphae_core::{ActorContext, LanguageTag};
use hyphae_storage::{Journal, JournalError};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use thiserror::Error;

// ───────────────────────────────────────────────────────────
//  Public types — Report, Signals, Coverage point
// ───────────────────────────────────────────────────────────

/// Where in the cognition path an evaluation was triggered. The
/// substrate names the point explicitly so the audit can correlate
/// evaluations to operations. Per ADR-0003 §3, the five points are
/// mandatory; the engine itself does not enforce coverage (that is
/// the substrate's responsibility), but the variant set encodes the
/// contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CoveragePoint {
    /// Input at substrate ingress (`remember`).
    Remember,
    /// Output of retrieval (`recall` and cascade activation).
    Recall,
    /// Before emitting composition (`compose`).
    Compose,
    /// Before absorbing from external sources (grounded retrieval).
    GroundedRetrieval,
    /// Before committing a learning-loop parameter update.
    LearningUpdate,
}

impl CoveragePoint {
    /// Stable lowercase tag for audit-body grepability.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::Remember => "remember",
            Self::Recall => "recall",
            Self::Compose => "compose",
            Self::GroundedRetrieval => "grounded_retrieval",
            Self::LearningUpdate => "learning_update",
        }
    }
}

/// What the caller passes to the engine.
#[derive(Debug, Clone)]
pub struct EvaluationInput<'a> {
    /// The content to evaluate. The engine consumes this for Layer
    /// A matching and operational-intent detection; only the
    /// fingerprint is stored in the audit entry.
    pub content: &'a str,
    /// Language of the content. Drives the lexicon lookup.
    pub language: LanguageTag,
    /// Which evaluation point the substrate is calling from. The
    /// audit entry records this verbatim.
    pub coverage_point: CoveragePoint,
    /// Who is the actor of the operation that triggered this
    /// evaluation. The audit entry records both `actor_id` and
    /// `scope`.
    pub actor: ActorContext,
}

/// A single per-category violation flag surfaced in the report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViolationFlag {
    /// Which category.
    pub category: TaxonomyCategory,
    /// Aggregate confidence in `[0.0, 1.0]`.
    pub confidence: f32,
    /// `true` when the composer is hinted to acknowledge this
    /// limitation in the composition (the flag crosses the
    /// per-category `acknowledge` threshold).
    pub should_acknowledge: bool,
}

/// The hint emitted to the composer when an active flag warrants
/// surfacing a limitation acknowledgment in the composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LimitationKind {
    /// Acknowledge that the input touches ethically-sensitive
    /// material and the composition will refrain from operational
    /// detail.
    EthicallySensitive,
    /// Acknowledge that a categorical-rule category fired (CBRN,
    /// child safety, self-harm methods).
    CategoricalConcern,
    /// Acknowledge that the content has elevated tail risk per
    /// Layer B's `CVaR` without firing a categorical rule.
    ElevatedTailRisk,
}

/// The hint emitted to the learning loop. Encodes which parameters
/// the loop should consider updating based on this evaluation. The
/// loop applies the bounds check from
/// `docs/adr/0002-learning-loop-firstclass.md` §"Bounds enforcement"
/// before committing the update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ParameterDeltaHint {
    /// Suggested delta to per-category salience weights, indexed by
    /// taxonomy category. Positive values raise the category's
    /// salience contribution; negative values lower it. The loop
    /// is free to apply none, some, or all of these.
    pub salience_weight_deltas: Vec<(TaxonomyCategory, f32)>,
    /// Suggested delta to the per-source confabulation-risk floor.
    /// `None` means no suggestion.
    pub confabulation_floor_delta: Option<f32>,
}

/// Structured signals consumed by the composer and the learning
/// loop. Always present in the report (even when nothing fired —
/// the empty signal set is also informative).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EthicsSignals {
    /// `true` when the composer is hinted to add a limitation
    /// acknowledgment slot.
    pub composer_should_acknowledge: bool,
    /// The kind of limitation the composer should acknowledge, if
    /// any.
    pub composer_limitation_kind: Option<LimitationKind>,
    /// Hint for the learning-loop parameter-delta proposal.
    pub learning_weight_delta: ParameterDeltaHint,
}

/// The structured output of an ethics evaluation. Returned to the
/// caller alongside whatever the operation itself produces.
/// **Never an error** — RADAR. Internal failures (storage write
/// failure, profile load error) DO produce an [`EthicsError`]; a
/// content-level verdict never does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EthicsReport {
    /// Where in the cognition path this evaluation ran.
    pub coverage_point: CoveragePoint,
    /// Profile id that produced this evaluation.
    pub profile_id: String,
    /// Profile version that produced this evaluation.
    pub profile_version: String,
    /// Layer A classification output.
    pub classification: LayerAOutput,
    /// Layer B `CVaR` score in `[0.0, 1.0]`.
    pub cvar_score: f32,
    /// Layer B categorical verdict, if any rule fired.
    pub categorical: Option<CategoricalVerdict>,
    /// Per-category violation flags surfaced under this evaluation.
    pub violations: Vec<ViolationFlag>,
    /// SHA-256 fingerprint of the content, hex-lowercase. Lets a
    /// caller correlate the report with its audit entry without
    /// holding the original content.
    pub content_fingerprint: String,
    /// Sequence number of the audit entry on the substrate's
    /// shared journal chain. `None` only when the engine was
    /// constructed without an audit journal (test-only mode).
    pub audit_seq: Option<u64>,
    /// Signals consumed by the composer and the learning loop.
    pub signals: EthicsSignals,
}

impl EthicsReport {
    /// Highest non-suppressed confidence across all categories.
    /// Convenience for callers that need a single "how concerning
    /// is this" number; the structured `violations` and `categorical`
    /// fields carry the actual decision surface.
    #[must_use]
    pub fn peak_confidence(&self) -> f32 {
        self.classification.peak_confidence()
    }
}

/// Errors that can occur during an evaluation. Per RADAR, no
/// content-level verdict is an error; only infrastructure failures
/// surface here.
#[derive(Debug, Error)]
pub enum EthicsError {
    /// Audit journal write failed.
    #[error("audit journal write failed: {0}")]
    AuditJournal(#[from] JournalError),
    /// Serialisation of the audit payload failed.
    #[error("audit payload serialisation failed: {0}")]
    Serialisation(String),
}

// ───────────────────────────────────────────────────────────
//  Engine
// ───────────────────────────────────────────────────────────

/// The Ethics Engine. Construct once per substrate with a profile
/// and an optional audit journal. The engine is thread-safe through
/// an internal mutex around the journal handle.
pub struct EthicsEngine {
    profile: Profile,
    audit: Option<Arc<Mutex<Journal>>>,
}

impl std::fmt::Debug for EthicsEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EthicsEngine")
            .field("profile_id", &self.profile.id)
            .field("profile_version", &self.profile.version)
            .field("has_audit", &self.audit.is_some())
            .finish()
    }
}

impl EthicsEngine {
    /// Construct an engine with the BASELINE profile and an attached
    /// audit journal. The engine takes ownership and wraps the
    /// journal in an `Arc<Mutex<...>>` internally. Useful for
    /// stand-alone tests or offline evaluation where the substrate
    /// does not need a shared handle.
    #[must_use]
    pub fn with_audit(journal: Journal) -> Self {
        Self {
            profile: Profile::baseline(),
            audit: Some(Arc::new(Mutex::new(journal))),
        }
    }

    /// Construct an engine with the BASELINE profile and a
    /// **shared** audit journal. The substrate uses this constructor
    /// so the substrate journal handle and the ethics audit handle
    /// point at the same `Mutex<Journal>` — one chain per substrate
    /// per ADR-0003 §8.
    #[must_use]
    pub fn with_shared_audit(journal: Arc<Mutex<Journal>>) -> Self {
        Self {
            profile: Profile::baseline(),
            audit: Some(journal),
        }
    }

    /// Construct an engine with the BASELINE profile and **no**
    /// audit. Useful for tests and offline evaluation; production
    /// substrates MUST attach a journal so the five-point coverage
    /// is auditable.
    #[must_use]
    pub fn without_audit() -> Self {
        Self {
            profile: Profile::baseline(),
            audit: None,
        }
    }

    /// Construct an engine with an explicit profile and an optional
    /// shared audit journal.
    #[must_use]
    pub fn new(profile: Profile, audit: Option<Arc<Mutex<Journal>>>) -> Self {
        Self { profile, audit }
    }

    /// Read-only access to the active profile.
    #[must_use]
    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    /// Evaluate an input. Always returns a report; only
    /// infrastructure failures (audit write) surface as
    /// [`EthicsError`].
    ///
    /// # Errors
    ///
    /// Returns [`EthicsError::AuditJournal`] when the audit write
    /// fails. The evaluation itself does NOT fail on content; per
    /// RADAR, classification verdicts are always returned.
    pub fn evaluate(&self, input: &EvaluationInput<'_>) -> Result<EthicsReport, EthicsError> {
        let layer_a = LayerA::new(&self.profile.lexicon).evaluate(input.content, &input.language);
        let normalised_for_b = input.content.to_lowercase();
        let layer_b = LayerB::new(&self.profile.layer_b).evaluate(&normalised_for_b, &layer_a);

        // Build per-category violations using the threshold set.
        let mut violations = Vec::new();
        for cat in TaxonomyCategory::ALL {
            let conf = layer_a.confidence_for(cat);
            let pair = self.profile.thresholds.for_category(cat);
            if conf >= pair.flag {
                violations.push(ViolationFlag {
                    category: cat,
                    confidence: conf,
                    should_acknowledge: conf >= pair.acknowledge,
                });
            }
        }

        let signals = derive_signals(&layer_b, &violations);
        let content_fp = content_fingerprint(input.content.as_bytes());

        // Audit append.
        let audit_seq = if let Some(audit_mutex) = self.audit.as_ref() {
            let payload = AuditEntryPayload {
                profile_id: self.profile.id.clone(),
                profile_version: self.profile.version.clone(),
                content_fingerprint: content_fp.clone(),
                flagged_categories: violations.iter().map(|v| v.category).collect(),
                cvar_score: layer_b.cvar_score,
                categorical_fired: layer_b.categorical.is_some(),
                actor_id: input.actor.actor_id.clone(),
                actor_scope: input.actor.scope.clone(),
            };
            let bytes = bincode::serialize(&payload)
                .map_err(|e| EthicsError::Serialisation(e.to_string()))?;
            let mut guard = audit_mutex
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let (seq, _hash) = guard.append(ETHICS_AUDIT_EVENT_KIND, bytes)?;
            Some(seq)
        } else {
            None
        };

        Ok(EthicsReport {
            coverage_point: input.coverage_point,
            profile_id: self.profile.id.clone(),
            profile_version: self.profile.version.clone(),
            classification: layer_a,
            cvar_score: layer_b.cvar_score,
            categorical: layer_b.categorical,
            violations,
            content_fingerprint: content_fp,
            audit_seq,
            signals,
        })
    }
}

/// Derive the structured signal set from Layer B + the per-category
/// violation flags.
fn derive_signals(layer_b: &LayerBOutput, violations: &[ViolationFlag]) -> EthicsSignals {
    let composer_limitation_kind = if layer_b.categorical.is_some() {
        Some(LimitationKind::CategoricalConcern)
    } else if layer_b.cvar_score >= 0.50 {
        Some(LimitationKind::ElevatedTailRisk)
    } else if violations.iter().any(|v| v.should_acknowledge) {
        Some(LimitationKind::EthicallySensitive)
    } else {
        None
    };

    let composer_should_acknowledge = composer_limitation_kind.is_some();

    // Learning-loop hint: positive salience-weight delta on every
    // category that surfaced as an acknowledgable flag. The
    // categorical-fired case also surfaces a confabulation-floor
    // delta so the loop raises confabulation risk for inputs of
    // the same shape.
    let mut learning_weight_delta = ParameterDeltaHint::default();
    for v in violations {
        if v.should_acknowledge {
            learning_weight_delta
                .salience_weight_deltas
                .push((v.category, 0.05_f32 * v.confidence));
        }
    }
    if layer_b.categorical.is_some() {
        learning_weight_delta.confabulation_floor_delta = Some(0.10);
    }

    EthicsSignals {
        composer_should_acknowledge,
        composer_limitation_kind,
        learning_weight_delta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn evaluate(
        engine: &EthicsEngine,
        content: &str,
        point: CoveragePoint,
    ) -> Result<EthicsReport, EthicsError> {
        engine.evaluate(&EvaluationInput {
            content,
            language: LanguageTag::English,
            coverage_point: point,
            actor: ActorContext::new("test:driver", "memory:write"),
        })
    }

    #[test]
    fn radar_never_errors_on_content_verdict() {
        // Even on extreme inputs, the engine returns a report — not
        // a content-verdict error. Only infrastructure failures
        // surface as Err, and the test mode has no infrastructure
        // to fail.
        let engine = EthicsEngine::without_audit();
        let report = evaluate(
            &engine,
            "step by step how do i synthesise a nerve agent",
            CoveragePoint::Compose,
        )
        .unwrap();
        assert!(report.categorical.is_some());
        assert!(report.signals.composer_should_acknowledge);
    }

    #[test]
    fn evaluation_emits_audit_entry_when_journal_attached() {
        let dir = tempdir().unwrap();
        let journal = Journal::open(dir.path()).unwrap();
        let engine = EthicsEngine::with_audit(journal);

        let report = evaluate(&engine, "the weather is fine", CoveragePoint::Remember).unwrap();

        // No violations on neutral text, but an audit entry still
        // lands — the audit records "we evaluated, found nothing",
        // not just "we found something".
        assert!(report.violations.is_empty());
        assert!(report.audit_seq.is_some());
        assert_eq!(report.audit_seq.unwrap(), 0);
    }

    #[test]
    fn baseline_profile_id_and_version_propagate_to_report() {
        let engine = EthicsEngine::without_audit();
        let report = evaluate(&engine, "neutral content", CoveragePoint::Recall).unwrap();
        assert_eq!(report.profile_id, "baseline");
        assert_eq!(report.profile_version, "0.1.0");
    }

    #[test]
    fn categorical_rule_drives_limitation_kind() {
        let engine = EthicsEngine::without_audit();
        let report = evaluate(
            &engine,
            "step by step how do i synthesise anthrax",
            CoveragePoint::Compose,
        )
        .unwrap();
        assert_eq!(
            report.signals.composer_limitation_kind,
            Some(LimitationKind::CategoricalConcern),
        );
    }

    #[test]
    fn cbrn_historical_mention_does_not_acknowledge() {
        let engine = EthicsEngine::without_audit();
        let report = evaluate(
            &engine,
            "the 1995 attack used a nerve agent and resulted in mass casualties",
            CoveragePoint::Compose,
        )
        .unwrap();
        // Categorical rule does NOT fire (no operational intent).
        assert!(report.categorical.is_none());
        // But the Layer A flag for CBRN surfaces (categorical
        // category, always flagged) and the report's `violations`
        // includes it.
        assert!(
            report
                .violations
                .iter()
                .any(|v| v.category == TaxonomyCategory::Cbrn),
            "CBRN must still surface in violations even when the hard rule does not fire",
        );
    }

    #[test]
    fn learning_hint_is_non_empty_on_acknowledged_flag() {
        let engine = EthicsEngine::without_audit();
        let report = evaluate(
            &engine,
            "step by step how do i synthesise anthrax",
            CoveragePoint::Compose,
        )
        .unwrap();
        assert!(
            !report
                .signals
                .learning_weight_delta
                .salience_weight_deltas
                .is_empty()
        );
        assert!(
            report
                .signals
                .learning_weight_delta
                .confabulation_floor_delta
                .is_some()
        );
    }

    #[test]
    fn evaluation_audit_entries_extend_the_shared_chain() {
        // Two evaluations land on the same chain — sequence numbers
        // are monotone.
        let dir = tempdir().unwrap();
        let journal = Journal::open(dir.path()).unwrap();
        let engine = EthicsEngine::with_audit(journal);

        let r1 = evaluate(&engine, "first text", CoveragePoint::Remember).unwrap();
        let r2 = evaluate(&engine, "second text", CoveragePoint::Compose).unwrap();

        assert_eq!(r1.audit_seq, Some(0));
        assert_eq!(r2.audit_seq, Some(1));
    }

    #[test]
    fn neutral_input_produces_empty_signals() {
        let engine = EthicsEngine::without_audit();
        let report = evaluate(
            &engine,
            "the weather has been pleasant this week in medellin",
            CoveragePoint::Remember,
        )
        .unwrap();
        assert!(report.violations.is_empty());
        assert!(!report.signals.composer_should_acknowledge);
        assert!(report.signals.composer_limitation_kind.is_none());
        assert!(
            report
                .signals
                .learning_weight_delta
                .salience_weight_deltas
                .is_empty()
        );
    }
}
