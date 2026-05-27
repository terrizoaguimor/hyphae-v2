// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Per-category thresholds used by Layer A and Layer B to decide
//! whether a per-category signal crosses the flag bar.
//!
//! In RADAR semantics thresholds **never block** — they decide
//! whether the [`crate::EthicsReport`] surfaces the category as a
//! [`crate::ViolationFlag`] consumed by the composer's limitation
//! triggers and the learning loop's parameter-delta hints.

use crate::taxonomy::TaxonomyCategory;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-category thresholds.
///
/// Two thresholds per category:
/// - `flag` — the minimum aggregate confidence at which the
///   category surfaces as a [`crate::ViolationFlag`] in the report.
/// - `acknowledge` — the minimum aggregate confidence at which the
///   composer is hinted to add a [`crate::LimitationKind`]
///   acknowledgment to its composition.
///
/// `flag <= acknowledge` is an invariant — a flag without
/// acknowledgment is possible (low-confidence signal feeds the
/// learning loop without surfacing in the composition), but
/// acknowledgment without a flag is not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThresholdSet {
    flags: HashMap<TaxonomyCategory, ThresholdPair>,
}

/// A pair of thresholds for one category.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ThresholdPair {
    /// Confidence at which the category surfaces in the report.
    pub flag: f32,
    /// Confidence at which the composer is hinted to acknowledge a
    /// limitation derived from this category.
    pub acknowledge: f32,
}

impl ThresholdPair {
    /// Construct a threshold pair, enforcing the
    /// `flag <= acknowledge` invariant by silently raising
    /// `acknowledge` to `flag` if needed.
    #[must_use]
    pub fn new(flag: f32, acknowledge: f32) -> Self {
        let flag = flag.clamp(0.0, 1.0);
        let acknowledge = acknowledge.clamp(0.0, 1.0);
        let acknowledge = acknowledge.max(flag);
        Self { flag, acknowledge }
    }
}

impl ThresholdSet {
    /// Construct an empty threshold set. Categories without an
    /// explicit threshold fall back to the defaults from
    /// [`Self::default_pair_for`].
    #[must_use]
    pub fn empty() -> Self {
        Self {
            flags: HashMap::new(),
        }
    }

    /// Install a threshold pair for a category.
    pub fn set(&mut self, category: TaxonomyCategory, pair: ThresholdPair) {
        self.flags.insert(category, pair);
    }

    /// Look up the thresholds for a category, falling back to the
    /// category-specific default if no explicit pair has been
    /// installed.
    #[must_use]
    pub fn for_category(&self, category: TaxonomyCategory) -> ThresholdPair {
        self.flags
            .get(&category)
            .copied()
            .unwrap_or_else(|| Self::default_pair_for(category))
    }

    /// Default thresholds per category. Categorical-rule categories
    /// (CBRN, child safety) carry the lowest flag threshold — Layer
    /// B's hard rule is what decides the verdict, but the flag
    /// must surface so the audit trail records the categorical
    /// concern.
    #[must_use]
    pub fn default_pair_for(category: TaxonomyCategory) -> ThresholdPair {
        if category.is_categorical() {
            ThresholdPair::new(0.30, 0.50)
        } else {
            ThresholdPair::new(0.50, 0.70)
        }
    }
}

impl Default for ThresholdSet {
    /// Default threshold set: every category receives
    /// [`Self::default_pair_for`] explicitly so the set
    /// round-trips through serde without losing information.
    fn default() -> Self {
        let mut set = Self::empty();
        for cat in TaxonomyCategory::ALL {
            set.set(cat, Self::default_pair_for(cat));
        }
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_pair_enforces_flag_le_acknowledge() {
        let p = ThresholdPair::new(0.8, 0.3);
        assert!(
            p.acknowledge >= p.flag,
            "acknowledge must be raised to flag when invariant breaks",
        );
    }

    #[test]
    fn categorical_categories_have_lower_flag_threshold() {
        let pair_for = |cat| ThresholdSet::default_pair_for(cat);
        let cbrn_flag = pair_for(TaxonomyCategory::Cbrn).flag;
        let hate_flag = pair_for(TaxonomyCategory::Hate).flag;
        assert!(
            cbrn_flag < hate_flag,
            "categorical-rule categories must flag at lower confidence so the audit always records them",
        );
    }

    #[test]
    fn default_set_round_trips_through_bincode() {
        let set = ThresholdSet::default();
        let bytes = bincode::serialize(&set).unwrap();
        let restored: ThresholdSet = bincode::deserialize(&bytes).unwrap();
        for cat in TaxonomyCategory::ALL {
            assert_eq!(set.for_category(cat), restored.for_category(cat));
        }
    }

    #[test]
    fn unset_category_falls_back_to_default() {
        let set = ThresholdSet::empty();
        let pair = set.for_category(TaxonomyCategory::Hate);
        assert_eq!(pair, ThresholdSet::default_pair_for(TaxonomyCategory::Hate));
    }
}
