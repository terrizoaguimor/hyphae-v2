// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Feedback signals — the two channels the learning loop consumes.
//!
//! Per `docs/adr/0002-learning-loop-firstclass.md` §"Feedback
//! signals (two channels)":
//!
//! - **Reward prediction error** from the `predictive` and `reward`
//!   subsystems — captures "did the composition's predicted utility
//!   match observed utility".
//! - **Ethics signals** from `hyphae-ethics` — captures
//!   classification deltas, violation flags, corpus baseline
//!   deviation.
//!
//! Both channels feed the same aggregator. Either channel alone is
//! insufficient for the loop to converge to useful behaviour (the
//! ADR's load-bearing claim).

use hyphae_core::FragmentId;
use hyphae_ethics::{CoveragePoint, EthicsReport, ParameterDeltaHint};
use std::time::SystemTime;

/// One feedback observation.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum FeedbackSignal {
    /// Reward prediction error from the `reward` subsystem. The
    /// `error` is the signed RPE `actual - expected`, in
    /// `[-1.0, +1.0]` after the convex-combination update.
    RewardPredictionError {
        /// Fragment the error was computed against (the actual that
        /// the prediction was compared to).
        fragment_id: FragmentId,
        /// Signed RPE.
        error: f32,
        /// Optional context tag — typically the edge id in the
        /// conductivity graph the prediction traversed, so the
        /// proposal generator can target the right edge weight.
        edge_hint: Option<String>,
        /// When the signal was produced.
        at: SystemTime,
    },
    /// Ethics signal emitted by an [`EthicsReport`]'s
    /// `signals.learning_weight_delta`. Carries the hint and the
    /// coverage point the report was filed under, so the proposal
    /// generator can scope updates appropriately.
    Ethics {
        /// The hint to apply.
        hint: ParameterDeltaHint,
        /// Which cognition-path point produced the report.
        coverage_point: CoveragePoint,
        /// When the signal was produced.
        at: SystemTime,
    },
}

impl FeedbackSignal {
    /// Stable lowercase tag for audit-body grepability.
    #[must_use]
    pub fn tag(&self) -> &'static str {
        match self {
            Self::RewardPredictionError { .. } => "reward_pe",
            Self::Ethics { .. } => "ethics",
        }
    }

    /// Construct an ethics signal from an [`EthicsReport`] —
    /// convenience for the integrator's hot path. Returns `None`
    /// when the report carries no learning hint (no acknowledgable
    /// flag and no categorical verdict).
    #[must_use]
    pub fn from_ethics_report(report: &EthicsReport) -> Option<Self> {
        if report
            .signals
            .learning_weight_delta
            .salience_weight_deltas
            .is_empty()
            && report
                .signals
                .learning_weight_delta
                .confabulation_floor_delta
                .is_none()
        {
            return None;
        }
        Some(Self::Ethics {
            hint: report.signals.learning_weight_delta.clone(),
            coverage_point: report.coverage_point,
            at: SystemTime::now(),
        })
    }
}

/// Aggregates feedback observations into a flushable buffer. The
/// learning loop calls [`Self::record`] on every observation and
/// [`Self::drain`] when it is ready to generate proposals.
///
/// v0.1 is a simple FIFO; the aggregator is the extension point for
/// future smoothing (eligibility traces, importance weighting,
/// off-policy correction) per ADR-0002 §"What the learning loop
/// does NOT do in v0.1".
#[derive(Debug, Default)]
pub struct FeedbackAggregator {
    pending: Vec<FeedbackSignal>,
}

impl FeedbackAggregator {
    /// Construct an empty aggregator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one feedback observation.
    pub fn record(&mut self, signal: FeedbackSignal) {
        self.pending.push(signal);
    }

    /// Drain all pending observations and return them. The
    /// aggregator is empty after the drain.
    #[must_use]
    pub fn drain(&mut self) -> Vec<FeedbackSignal> {
        std::mem::take(&mut self.pending)
    }

    /// Peek at pending observations without consuming them.
    #[must_use]
    pub fn pending(&self) -> &[FeedbackSignal] {
        &self.pending
    }

    /// Number of pending observations.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// `true` when there are no pending observations.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyphae_ethics::{
        EthicsReport, EthicsSignals, LayerAOutput, LimitationKind, TaxonomyCategory,
    };
    use std::collections::HashMap;

    fn empty_report(coverage_point: CoveragePoint) -> EthicsReport {
        EthicsReport {
            coverage_point,
            profile_id: "test".to_string(),
            profile_version: "0.0.1".to_string(),
            classification: LayerAOutput {
                per_category: HashMap::new(),
                flags: Vec::new(),
                disambiguation: hyphae_ethics::DisambiguationVerdict::default(),
            },
            cvar_score: 0.0,
            categorical: None,
            violations: Vec::new(),
            content_fingerprint: String::new(),
            audit_seq: None,
            signals: EthicsSignals::default(),
        }
    }

    fn report_with_hint() -> EthicsReport {
        let mut r = empty_report(CoveragePoint::Compose);
        r.signals.composer_should_acknowledge = true;
        r.signals.composer_limitation_kind = Some(LimitationKind::EthicallySensitive);
        r.signals
            .learning_weight_delta
            .salience_weight_deltas
            .push((TaxonomyCategory::Hate, 0.04));
        r
    }

    #[test]
    fn from_ethics_report_returns_none_for_empty_signals() {
        let r = empty_report(CoveragePoint::Compose);
        assert!(FeedbackSignal::from_ethics_report(&r).is_none());
    }

    #[test]
    fn from_ethics_report_returns_some_when_hint_present() {
        let r = report_with_hint();
        let signal = FeedbackSignal::from_ethics_report(&r).expect("hint present");
        assert!(matches!(signal, FeedbackSignal::Ethics { .. }));
        assert_eq!(signal.tag(), "ethics");
    }

    #[test]
    fn aggregator_drains_in_fifo_order() {
        let mut agg = FeedbackAggregator::new();
        let s1 = FeedbackSignal::RewardPredictionError {
            fragment_id: FragmentId::new(),
            error: 0.1,
            edge_hint: Some("a:b".to_string()),
            at: SystemTime::now(),
        };
        let s2 = FeedbackSignal::RewardPredictionError {
            fragment_id: FragmentId::new(),
            error: -0.2,
            edge_hint: Some("c:d".to_string()),
            at: SystemTime::now(),
        };
        agg.record(s1);
        agg.record(s2);
        assert_eq!(agg.len(), 2);
        let drained = agg.drain();
        assert_eq!(drained.len(), 2);
        assert!(agg.is_empty());
        match (&drained[0], &drained[1]) {
            (
                FeedbackSignal::RewardPredictionError {
                    edge_hint: Some(e0),
                    ..
                },
                FeedbackSignal::RewardPredictionError {
                    edge_hint: Some(e1),
                    ..
                },
            ) => {
                assert_eq!(e0, "a:b");
                assert_eq!(e1, "c:d");
            }
            _ => panic!("expected two reward signals"),
        }
    }
}
