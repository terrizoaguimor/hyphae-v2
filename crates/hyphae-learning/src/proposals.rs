// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Generate [`LearningUpdateProposal`]s from [`FeedbackSignal`]s.
//!
//! The generator is intentionally small in v0.1. It is the
//! extension point for richer credit assignment (eligibility
//! traces, importance weighting) per ADR-0002 §"What the learning
//! loop does NOT do in v0.1"; the v0.1 implementation is a direct
//! signal-to-proposal mapping.
//!
//! A proposal must be paired with the store's current value (via
//! [`ParameterStore::propose`]) before it is handed to the
//! substrate. The generator here produces the **intent** — what
//! parameter to nudge and by how much — without touching the
//! store; the integrator runs the bounds check, calls
//! `substrate.propose_learning_update`, and applies on audit.

use crate::feedback::FeedbackSignal;
use hyphae_ethics::TaxonomyCategory;
use hyphae_substrate::LearningTarget;

/// A learning-loop intent. The integrator combines this with the
/// [`crate::parameters::ParameterStore`] to obtain a
/// [`hyphae_substrate::LearningUpdateProposal`] (which carries the
/// serialised old / new bytes).
#[derive(Debug, Clone)]
pub struct LearningIntent {
    /// Target parameter.
    pub target: LearningTarget,
    /// Signed delta to apply to the parameter's current scalar
    /// value. For categorical parameters the delta applies to the
    /// key named in `categorical_key`.
    pub delta: f32,
    /// When `target` resolves to a categorical parameter, this
    /// names the key whose entry receives `delta`. Ignored for
    /// scalar targets.
    pub categorical_key: Option<String>,
    /// Audit rationale that will ride the substrate's
    /// `audit_learning_update` entry.
    pub rationale: String,
}

/// Generate intents from a batch of feedback signals.
///
/// Mapping in v0.1:
/// - **Reward PE** → an episodic conductivity-weight delta on the
///   `edge_hint`, scaled by the signed error. Positive RPE raises
///   the edge's conductivity; negative lowers it.
/// - **Ethics signals** → per-category salience-weight deltas
///   keyed by [`TaxonomyCategory::tag()`] (so the parameter store's
///   `Categorical` map matches the audit-trail discriminator).
///
/// Returns the intents in input order so the audit trail preserves
/// causality.
#[must_use]
pub fn intents_from_signals(signals: &[FeedbackSignal]) -> Vec<LearningIntent> {
    let mut out = Vec::new();
    for signal in signals {
        match signal {
            FeedbackSignal::RewardPredictionError {
                error, edge_hint, ..
            } => {
                let Some(edge_id) = edge_hint.clone() else {
                    continue;
                };
                out.push(LearningIntent {
                    target: LearningTarget::EpisodicConductivityWeight {
                        edge_id: edge_id.clone(),
                    },
                    delta: *error,
                    categorical_key: None,
                    rationale: format!("reward prediction error {error:+.4} on edge {edge_id}"),
                });
            }
            FeedbackSignal::Ethics {
                hint,
                coverage_point,
                ..
            } => {
                for (cat, delta) in &hint.salience_weight_deltas {
                    out.push(LearningIntent {
                        target: LearningTarget::ValenceSalienceWeight {
                            category: category_tag(*cat).to_string(),
                        },
                        delta: *delta,
                        categorical_key: Some(category_tag(*cat).to_string()),
                        rationale: format!(
                            "ethics signal at {} suggested salience delta {delta:+.4} on category {}",
                            coverage_point.tag(),
                            category_tag(*cat),
                        ),
                    });
                }
                if let Some(floor_delta) = hint.confabulation_floor_delta {
                    out.push(LearningIntent {
                        target: LearningTarget::CascadeParameter {
                            name: "confabulation_floor".to_string(),
                        },
                        delta: floor_delta,
                        categorical_key: None,
                        rationale: format!(
                            "ethics signal at {} suggested confabulation floor delta {floor_delta:+.4}",
                            coverage_point.tag(),
                        ),
                    });
                }
            }
        }
    }
    out
}

/// Stable lowercase tag for a [`TaxonomyCategory`]. Mirrors
/// `TaxonomyCategory::tag()` but kept inline so the matching is
/// audit-grep visible.
#[must_use]
fn category_tag(cat: TaxonomyCategory) -> &'static str {
    cat.tag()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyphae_core::FragmentId;
    use hyphae_ethics::{CoveragePoint, ParameterDeltaHint};
    use std::time::SystemTime;

    #[test]
    fn reward_pe_without_edge_hint_produces_no_intent() {
        let signal = FeedbackSignal::RewardPredictionError {
            fragment_id: FragmentId::new(),
            error: 0.5,
            edge_hint: None,
            at: SystemTime::now(),
        };
        let intents = intents_from_signals(&[signal]);
        assert!(intents.is_empty());
    }

    #[test]
    fn reward_pe_with_edge_hint_produces_one_intent_per_signal() {
        let signal = FeedbackSignal::RewardPredictionError {
            fragment_id: FragmentId::new(),
            error: 0.4,
            edge_hint: Some("a:b".to_string()),
            at: SystemTime::now(),
        };
        let intents = intents_from_signals(&[signal]);
        assert_eq!(intents.len(), 1);
        assert!(matches!(
            intents[0].target,
            LearningTarget::EpisodicConductivityWeight { ref edge_id } if edge_id == "a:b"
        ));
        assert!((intents[0].delta - 0.4).abs() < f32::EPSILON);
    }

    #[test]
    fn ethics_signal_with_two_category_deltas_produces_two_intents() {
        let mut hint = ParameterDeltaHint::default();
        hint.salience_weight_deltas
            .push((TaxonomyCategory::Hate, 0.03));
        hint.salience_weight_deltas
            .push((TaxonomyCategory::Violence, 0.05));
        let signal = FeedbackSignal::Ethics {
            hint,
            coverage_point: CoveragePoint::Compose,
            at: SystemTime::now(),
        };
        let intents = intents_from_signals(&[signal]);
        assert_eq!(intents.len(), 2);
        assert!(
            intents
                .iter()
                .all(|i| matches!(i.target, LearningTarget::ValenceSalienceWeight { .. }))
        );
    }

    #[test]
    fn ethics_signal_with_confabulation_floor_produces_cascade_intent() {
        let hint = ParameterDeltaHint {
            confabulation_floor_delta: Some(0.1),
            ..ParameterDeltaHint::default()
        };
        let signal = FeedbackSignal::Ethics {
            hint,
            coverage_point: CoveragePoint::Compose,
            at: SystemTime::now(),
        };
        let intents = intents_from_signals(&[signal]);
        assert_eq!(intents.len(), 1);
        assert!(matches!(
            intents[0].target,
            LearningTarget::CascadeParameter { ref name } if name == "confabulation_floor"
        ));
    }
}
