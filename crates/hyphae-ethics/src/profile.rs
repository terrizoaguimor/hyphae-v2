// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Profile loader and BASELINE profile.
//!
//! A [`Profile`] is the per-deployment calibration artifact: it
//! couples the [`crate::thresholds::ThresholdSet`] (when does Layer
//! A flag?) with the Layer B parameters (`CVaR` alpha, irreversibility
//! weights) and the active lexicon. The `BASELINE_PROFILE` constant
//! is the in-tree calibration that ships standalone — Layer B is
//! fully functional without an external profile file (corrects
//! v1's "10–15% coverage by omission" by giving v2 a real
//! out-of-the-box profile).

use crate::lexicon::Lexicon;
use crate::taxonomy::TaxonomyCategory;
use crate::thresholds::ThresholdSet;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Layer B (`CVaR`) calibration parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerBParams {
    /// `CVaR` tail level. Standard convention is 0.05 (5%-tail).
    /// Smaller alpha = more conservative (averaging deeper in the
    /// tail).
    pub alpha: f32,
    /// Asymmetric weight applied to irreversible-harm categories
    /// when computing the `CVaR` aggregate. Default `2.0` — an
    /// irreversible-harm signal counts double a reversible-harm
    /// signal of the same point confidence.
    pub irreversibility_weight: f32,
    /// Per-category irreversibility flag. `true` for categories
    /// whose harms are not undoable once realised; the asymmetric
    /// weighting applies to these categories only.
    pub irreversible: HashMap<TaxonomyCategory, bool>,
}

impl LayerBParams {
    /// Defaults coherent with the BASELINE profile.
    #[must_use]
    pub fn baseline() -> Self {
        let mut irreversible: HashMap<TaxonomyCategory, bool> = HashMap::new();
        for cat in TaxonomyCategory::ALL {
            // Categorical-rule categories are irreversible by
            // construction. Self-harm, violence, and privacy
            // disclosures of identifying material are also
            // irreversible — content released cannot be unreleased.
            let is_irreversible = matches!(
                cat,
                TaxonomyCategory::Cbrn
                    | TaxonomyCategory::ChildSafety
                    | TaxonomyCategory::SelfHarm
                    | TaxonomyCategory::Violence
                    | TaxonomyCategory::Pii
                    | TaxonomyCategory::Privacy,
            );
            irreversible.insert(cat, is_irreversible);
        }
        Self {
            alpha: 0.05,
            irreversibility_weight: 2.0,
            irreversible,
        }
    }

    /// Returns whether the given category is configured as
    /// irreversible in this profile.
    #[must_use]
    pub fn is_irreversible(&self, category: TaxonomyCategory) -> bool {
        self.irreversible.get(&category).copied().unwrap_or(false)
    }
}

impl Default for LayerBParams {
    fn default() -> Self {
        Self::baseline()
    }
}

/// A profile: thresholds + Layer B params + lexicon. Identified by
/// stable id and version so an [`crate::EthicsReport`] can be traced
/// back to the exact calibration that produced it.
#[derive(Debug, Clone)]
pub struct Profile {
    /// Stable identifier for the profile (e.g. `"baseline"`,
    /// `"deployment:celiums-prod"`).
    pub id: String,
    /// Version string. Bumped on any change to thresholds, Layer B
    /// params, or the active lexicon.
    pub version: String,
    /// Per-category thresholds for Layer A flag / acknowledge.
    pub thresholds: ThresholdSet,
    /// Layer B (`CVaR`) parameters.
    pub layer_b: LayerBParams,
    /// Active lexicon. Constructed at profile-load time; not
    /// serialised with the profile metadata.
    pub lexicon: Lexicon,
}

impl Profile {
    /// The in-tree BASELINE profile. Layer B is fully functional
    /// with this profile and no external loader configuration.
    #[must_use]
    pub fn baseline() -> Self {
        Self {
            id: "baseline".to_string(),
            version: "0.1.0".to_string(),
            thresholds: ThresholdSet::default(),
            layer_b: LayerBParams::baseline(),
            lexicon: Lexicon::baseline_en(),
        }
    }
}

impl Default for Profile {
    fn default() -> Self {
        Self::baseline()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_profile_carries_id_and_version() {
        let p = Profile::baseline();
        assert_eq!(p.id, "baseline");
        assert_eq!(p.version, "0.1.0");
    }

    #[test]
    fn baseline_layer_b_marks_categorical_categories_irreversible() {
        let lb = LayerBParams::baseline();
        assert!(lb.is_irreversible(TaxonomyCategory::Cbrn));
        assert!(lb.is_irreversible(TaxonomyCategory::ChildSafety));
        assert!(lb.is_irreversible(TaxonomyCategory::SelfHarm));
        // Deception is harm but it is not irreversible at the
        // baseline calibration — reversible-mitigation paths exist
        // (correction, retraction, dispute).
        assert!(!lb.is_irreversible(TaxonomyCategory::Deception));
    }

    #[test]
    fn baseline_alpha_is_canonical_5pct_tail() {
        let lb = LayerBParams::baseline();
        assert!((lb.alpha - 0.05).abs() < f32::EPSILON);
    }

    #[test]
    fn baseline_lexicon_is_english_only() {
        let p = Profile::baseline();
        assert!(
            !p.lexicon
                .entries_for(&hyphae_core::LanguageTag::English)
                .is_empty()
        );
    }
}
