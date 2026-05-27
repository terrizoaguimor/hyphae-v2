// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Typed refinable parameters and their bounds.
//!
//! The [`ParameterStore`] is the **separate mutable surface** the
//! learning loop operates on per
//! `docs/adr/0002-learning-loop-firstclass.md` §"Bounds
//! enforcement". The substrate's grammar, state machine, pathway
//! topology, schemas, hash-chain protocol, and `PayloadKind`
//! taxonomy are immutable references; the store here holds the
//! parameters that the learning loop is *permitted* to refine.

use hyphae_substrate::LearningTarget;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A parameter value carried in the [`ParameterStore`]. Discriminated
/// by the shape of the refinable parameter — Layer A's
/// `salience_weights` is per-category and lives as a `Categorical`;
/// cascade thresholds are scalar floats; conductivity edge weights
/// are scalar floats keyed by edge id.
///
/// The byte serialisation of a `ParameterValue` is what rides the
/// substrate's `audit_learning_update` journal entries — so this
/// type's `Serialize` impl is load-bearing for rollback replay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParameterValue {
    /// Scalar floating-point parameter (cascade threshold, hop decay
    /// factor, conductivity edge weight, schema prior).
    Scalar(f32),
    /// Per-category map of floats. Used by Layer A's
    /// `salience_weights` and by the composer's
    /// `schema_selection_priors` when the priors are keyed by
    /// taxonomy category.
    Categorical(HashMap<String, f32>),
}

impl ParameterValue {
    /// Serialise to bytes for journaling. Uses `bincode` — the same
    /// serialiser the audit payload uses internally.
    ///
    /// # Errors
    ///
    /// Returns an error if the value cannot be serialised. In
    /// practice the variants here are `bincode`-clean, so this is a
    /// defence-in-depth path.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ParameterError> {
        bincode::serialize(self).map_err(|e| ParameterError::Serialisation(e.to_string()))
    }

    /// Deserialise from bytes produced by [`Self::to_bytes`] or by a
    /// previous journaled value.
    ///
    /// # Errors
    ///
    /// Returns an error if the bytes are malformed or were not
    /// produced by a matching serialiser.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ParameterError> {
        bincode::deserialize(bytes).map_err(|e| ParameterError::Serialisation(e.to_string()))
    }

    /// Try to read this value as a scalar. `None` if it is the
    /// categorical variant.
    #[must_use]
    pub fn as_scalar(&self) -> Option<f32> {
        match self {
            Self::Scalar(v) => Some(*v),
            Self::Categorical(_) => None,
        }
    }

    /// Try to read this value as a categorical map.
    #[must_use]
    pub fn as_categorical(&self) -> Option<&HashMap<String, f32>> {
        match self {
            Self::Scalar(_) => None,
            Self::Categorical(m) => Some(m),
        }
    }
}

/// Bounds for a refinable parameter. The learning loop enforces
/// these on every proposal; updates that would push a value outside
/// its bounds are rejected at [`ParameterStore::propose`] time, not
/// at apply time, so the substrate's audit chain never carries a
/// rejected update.
///
/// The default bound is `[0.0, 1.0]` because most refinable
/// parameters (saliency weights, schema priors, normalised
/// thresholds) live in that range. The Composer's
/// `honest_limitation_thresholds` carry a per-trigger lower bound
/// per RFC §7.3 — the floor is the type's domain, not zero.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParameterBounds {
    /// Inclusive lower bound.
    pub min: f32,
    /// Inclusive upper bound.
    pub max: f32,
}

impl ParameterBounds {
    /// Construct a bounds pair, swapping if min > max so the bounds
    /// are well-formed.
    #[must_use]
    pub fn new(min: f32, max: f32) -> Self {
        if min <= max {
            Self { min, max }
        } else {
            Self { min: max, max: min }
        }
    }

    /// Standard `[0.0, 1.0]` bound.
    pub const UNIT: Self = Self { min: 0.0, max: 1.0 };

    /// Is `value` within these bounds (inclusive)?
    #[must_use]
    pub fn contains(&self, value: f32) -> bool {
        value >= self.min && value <= self.max
    }

    /// Clamp `value` to these bounds.
    #[must_use]
    pub fn clamp(&self, value: f32) -> f32 {
        value.clamp(self.min, self.max)
    }
}

impl Default for ParameterBounds {
    fn default() -> Self {
        Self::UNIT
    }
}

/// Errors raised by the parameter store.
#[derive(Debug, thiserror::Error)]
pub enum ParameterError {
    /// A proposed value falls outside the parameter's declared
    /// bounds. The proposal is rejected at propose-time so the
    /// chain stays clean of rejected updates.
    #[error("parameter {target} value {value:.6} outside bounds [{min:.6}, {max:.6}]")]
    OutOfBounds {
        /// `LearningTarget::tag()` of the parameter.
        target: &'static str,
        /// The proposed value.
        value: f32,
        /// Bound lower limit.
        min: f32,
        /// Bound upper limit.
        max: f32,
    },
    /// A categorical proposal contained a value outside bounds for
    /// at least one key. Carries the first offender.
    #[error("parameter {target}[{key}] value {value:.6} outside bounds [{min:.6}, {max:.6}]")]
    CategoricalOutOfBounds {
        /// `LearningTarget::tag()` of the parameter.
        target: &'static str,
        /// The category whose value was out of range.
        key: String,
        /// The proposed value.
        value: f32,
        /// Bound lower limit.
        min: f32,
        /// Bound upper limit.
        max: f32,
    },
    /// The proposal's new-value bytes could not be deserialised
    /// into a `ParameterValue`.
    #[error("serialisation error: {0}")]
    Serialisation(String),
    /// The proposal's variant shape does not match what was previously
    /// stored for this target. (e.g. proposing a `Categorical` for a
    /// target previously stored as `Scalar`).
    #[error("parameter {target} variant mismatch: expected {expected}, got {actual}")]
    VariantMismatch {
        /// `LearningTarget::tag()` of the parameter.
        target: &'static str,
        /// Expected variant.
        expected: &'static str,
        /// Actual variant supplied.
        actual: &'static str,
    },
}

/// The store. Holds the **authoritative state** of every refinable
/// parameter the learning loop has touched, plus declared bounds per
/// target. Read by subsystems at runtime via the integrator's
/// `Arc<RwLock<ParameterStore>>` handle; written by the learning
/// loop after the substrate's [`Substrate::propose_learning_update`]
/// has audited.
///
/// Per ADR-0002 §7.6 the store is **separate** from any immutable
/// substrate surface — it is the mutable parameter slot the learning
/// loop owns; the substrate's immutable references stay immutable.
#[derive(Debug, Default)]
pub struct ParameterStore {
    values: HashMap<String, ParameterValue>,
    bounds: HashMap<String, ParameterBounds>,
}

impl ParameterStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare bounds for a target. Bounds default to
    /// [`ParameterBounds::UNIT`] when not declared.
    pub fn set_bounds(&mut self, target: &LearningTarget, bounds: ParameterBounds) {
        self.bounds.insert(key(target), bounds);
    }

    /// Look up the bounds declared for a target, falling back to
    /// [`ParameterBounds::UNIT`].
    #[must_use]
    pub fn bounds_for(&self, target: &LearningTarget) -> ParameterBounds {
        self.bounds
            .get(&key(target))
            .copied()
            .unwrap_or(ParameterBounds::UNIT)
    }

    /// Read the current value for a target. `None` when the store
    /// has never received an update for this target.
    #[must_use]
    pub fn get(&self, target: &LearningTarget) -> Option<&ParameterValue> {
        self.values.get(&key(target))
    }

    /// Install an initial value for a target. The value is **not**
    /// bounds-checked because initial values are deployment
    /// configuration, not learned updates — the bounds-check fires
    /// on `propose_apply`. Use this from the integrator at startup
    /// to seed the store.
    pub fn seed(&mut self, target: &LearningTarget, value: ParameterValue) {
        self.values.insert(key(target), value);
    }

    /// Propose applying a new value. Runs the bounds check at
    /// propose-time. Returns the proposal serialised to bytes
    /// (suitable for [`hyphae_substrate::LearningUpdateProposal::new_value`])
    /// together with the current bytes (suitable for `old_value`).
    ///
    /// The store is **not mutated** by this call — the substrate
    /// audits first; [`Self::apply_audited`] applies after the audit
    /// succeeds.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterError::OutOfBounds`] if the proposed value
    /// (or any value in a categorical proposal) falls outside the
    /// declared bounds, [`ParameterError::VariantMismatch`] if the
    /// proposed variant does not match what is currently stored,
    /// or [`ParameterError::Serialisation`] on a bincode failure.
    pub fn propose(
        &self,
        target: &LearningTarget,
        proposed: &ParameterValue,
    ) -> Result<ProposalBytes, ParameterError> {
        let bounds = self.bounds_for(target);
        let tag = target.tag();

        match proposed {
            ParameterValue::Scalar(v) => {
                if !bounds.contains(*v) {
                    return Err(ParameterError::OutOfBounds {
                        target: tag,
                        value: *v,
                        min: bounds.min,
                        max: bounds.max,
                    });
                }
            }
            ParameterValue::Categorical(map) => {
                for (k, v) in map {
                    if !bounds.contains(*v) {
                        return Err(ParameterError::CategoricalOutOfBounds {
                            target: tag,
                            key: k.clone(),
                            value: *v,
                            min: bounds.min,
                            max: bounds.max,
                        });
                    }
                }
            }
        }

        // Variant compatibility check — once a target has a stored
        // shape, subsequent proposals must keep that shape.
        if let Some(current) = self.values.get(&key(target)) {
            match (current, proposed) {
                (ParameterValue::Scalar(_), ParameterValue::Categorical(_)) => {
                    return Err(ParameterError::VariantMismatch {
                        target: tag,
                        expected: "Scalar",
                        actual: "Categorical",
                    });
                }
                (ParameterValue::Categorical(_), ParameterValue::Scalar(_)) => {
                    return Err(ParameterError::VariantMismatch {
                        target: tag,
                        expected: "Categorical",
                        actual: "Scalar",
                    });
                }
                _ => {}
            }
        }

        let old_value = match self.values.get(&key(target)) {
            Some(current) => current.to_bytes()?,
            None => Vec::new(),
        };
        let new_value = proposed.to_bytes()?;

        Ok(ProposalBytes {
            old_value,
            new_value,
        })
    }

    /// Apply a value that the substrate has just audited. The audit
    /// `seq` is recorded only by the caller (in their own bookkeeping)
    /// — the store itself only mutates its in-memory state.
    pub fn apply_audited(&mut self, target: &LearningTarget, value: ParameterValue) {
        self.values.insert(key(target), value);
    }

    /// Drop the stored value for a target. Useful during rollback
    /// when the journal replay shows the target was never seeded.
    pub fn clear(&mut self, target: &LearningTarget) {
        self.values.remove(&key(target));
    }

    /// Number of parameters currently stored.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// `true` if no parameters are stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// Serialised representation of a proposed update — both the
/// previous value and the new value as bytes. The substrate's
/// `LearningUpdateProposal` shape consumes these directly.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposalBytes {
    /// Current value, serialised. Empty when the target was never
    /// seeded.
    pub old_value: Vec<u8>,
    /// New value, serialised.
    pub new_value: Vec<u8>,
}

/// Key a [`LearningTarget`] for the parameter store map. Two
/// targets with the same tag + identifier components collide on the
/// same store entry — that is intentional (the conductivity weight
/// for `edge_id=A:B` lives in one slot regardless of how many times
/// it is updated).
fn key(target: &LearningTarget) -> String {
    match target {
        LearningTarget::EpisodicConductivityWeight { edge_id } => {
            format!("episodic.conductivity_weight:{edge_id}")
        }
        LearningTarget::ValenceSalienceWeight { category } => {
            format!("valence.salience_weight:{category}")
        }
        LearningTarget::CascadeParameter { name } => format!("cascade.parameter:{name}"),
        LearningTarget::ComposerSchemaPrior { schema_id } => {
            format!("composer.schema_prior:{schema_id}")
        }
        LearningTarget::ComposerLimitationThreshold { trigger_id } => {
            format!("composer.limitation_threshold:{trigger_id}")
        }
        // `LearningTarget` is `#[non_exhaustive]` so the substrate
        // can add variants without breaking downstream crates.
        // Unknown variants fall back to a deterministic placeholder
        // key — the integrator surfaces this as a future-shape
        // surface and the rollback path drops the entry rather than
        // colliding with a known key.
        _ => "unknown.target".to_string(),
    }
}

/// Public lookup helper exposing the same key computation used
/// internally — useful for crates that need to mirror the
/// store's keying convention.
#[must_use]
pub fn target_key(target: &LearningTarget) -> String {
    key(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cascade_param(name: &str) -> LearningTarget {
        LearningTarget::CascadeParameter {
            name: name.to_string(),
        }
    }

    fn weight(edge: &str) -> LearningTarget {
        LearningTarget::EpisodicConductivityWeight {
            edge_id: edge.to_string(),
        }
    }

    #[test]
    fn parameter_value_round_trips_through_bytes() {
        let scalar = ParameterValue::Scalar(0.42);
        let bytes = scalar.to_bytes().unwrap();
        let restored = ParameterValue::from_bytes(&bytes).unwrap();
        assert_eq!(scalar, restored);

        let mut map = HashMap::new();
        map.insert("hate".to_string(), 0.5);
        map.insert("violence".to_string(), 0.3);
        let cat = ParameterValue::Categorical(map);
        let bytes = cat.to_bytes().unwrap();
        let restored = ParameterValue::from_bytes(&bytes).unwrap();
        assert_eq!(cat, restored);
    }

    #[test]
    fn propose_rejects_scalar_outside_bounds() {
        let mut store = ParameterStore::new();
        store.set_bounds(&cascade_param("threshold"), ParameterBounds::UNIT);
        let err = store
            .propose(&cascade_param("threshold"), &ParameterValue::Scalar(1.5))
            .unwrap_err();
        assert!(matches!(err, ParameterError::OutOfBounds { .. }));
    }

    #[test]
    fn propose_accepts_scalar_inside_bounds() {
        let mut store = ParameterStore::new();
        store.set_bounds(&cascade_param("threshold"), ParameterBounds::UNIT);
        let bytes = store
            .propose(&cascade_param("threshold"), &ParameterValue::Scalar(0.5))
            .unwrap();
        assert!(bytes.old_value.is_empty(), "untouched target has empty old");
        assert!(!bytes.new_value.is_empty());
    }

    #[test]
    fn propose_rejects_categorical_with_out_of_range_value() {
        let mut store = ParameterStore::new();
        store.set_bounds(&weight("a:b"), ParameterBounds::new(-1.0, 1.0));
        let mut bad = HashMap::new();
        bad.insert("k".to_string(), 5.0);
        let err = store
            .propose(&weight("a:b"), &ParameterValue::Categorical(bad))
            .unwrap_err();
        assert!(matches!(err, ParameterError::CategoricalOutOfBounds { .. }));
    }

    #[test]
    fn propose_rejects_variant_mismatch() {
        let mut store = ParameterStore::new();
        store.seed(&cascade_param("decay"), ParameterValue::Scalar(0.3));
        let mut cat = HashMap::new();
        cat.insert("k".to_string(), 0.1);
        let err = store
            .propose(&cascade_param("decay"), &ParameterValue::Categorical(cat))
            .unwrap_err();
        assert!(matches!(err, ParameterError::VariantMismatch { .. }));
    }

    #[test]
    fn apply_audited_mutates_store_state() {
        let mut store = ParameterStore::new();
        store.seed(&cascade_param("alpha"), ParameterValue::Scalar(0.5));
        store.apply_audited(&cascade_param("alpha"), ParameterValue::Scalar(0.7));
        let v = store.get(&cascade_param("alpha")).unwrap();
        assert_eq!(*v, ParameterValue::Scalar(0.7));
    }

    #[test]
    fn bounds_swap_when_min_greater_than_max() {
        let b = ParameterBounds::new(1.0, 0.0);
        assert!((b.min - 0.0).abs() < f32::EPSILON);
        assert!((b.max - 1.0).abs() < f32::EPSILON);
    }
}
