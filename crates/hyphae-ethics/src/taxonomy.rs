// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Twelve-category ethics taxonomy.
//!
//! The categories map the surfaces of harm that an ethics evaluator
//! distinguishes per `docs/adr/0003-ethics-radar-firstclass.md`
//! §"Layers in v0.1". The mapping is anchored in the published
//! research substrate behind celiums-memory v2's Layer A
//! (`SafetyBench`, Jigsaw, EU DSA, OWASP Top 10 for LLM applications)
//! so v2's classifications can be compared across the two motors
//! when they evaluate the same input.
//!
//! The enum is `#[non_exhaustive]` so additional categories can
//! land in a minor release without breaking downstream pattern
//! matches.

use serde::{Deserialize, Serialize};

/// One of the twelve ethics taxonomy categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TaxonomyCategory {
    /// Content that attacks a person or group on the basis of a
    /// protected characteristic. Includes structurally-coded
    /// language (see [`super::structural`]).
    Hate,
    /// Violent intent, instructions, glorification, or threats.
    Violence,
    /// Personally-identifiable information disclosure (own or
    /// third-party).
    Pii,
    /// Self-harm, suicide ideation, methods, or encouragement.
    SelfHarm,
    /// Deception, manipulation, social engineering toward harm.
    Deception,
    /// Cyber-offence: exploitation, intrusion, exfiltration,
    /// malware, ransomware.
    Cyber,
    /// Misinformation: false claims presented as fact about
    /// epistemically-tractable matters.
    Misinformation,
    /// Privacy violations beyond PII (surveillance, doxing, intimate
    /// imagery).
    Privacy,
    /// Autonomy violations (coercion, manipulation, removing
    /// informed consent).
    Autonomy,
    /// System-override attempts (jailbreak, prompt-injection-style
    /// content directed at the substrate itself).
    SystemOverride,
    /// Categorical mass-harm risk: CBRN (Chemical, Biological,
    /// Radiological, Nuclear) materials and weapons. Layer B's
    /// CBRN hard rule (with operational intent) overrides the
    /// probabilistic path for this category.
    Cbrn,
    /// Child safety: any content involving minors in harmful
    /// contexts. Hard-rule category by design.
    ChildSafety,
}

impl TaxonomyCategory {
    /// Every taxonomy variant, in declaration order. Convenience for
    /// iteration over the full surface (e.g. by the profile loader
    /// when constructing per-category thresholds).
    pub const ALL: [Self; 12] = [
        Self::Hate,
        Self::Violence,
        Self::Pii,
        Self::SelfHarm,
        Self::Deception,
        Self::Cyber,
        Self::Misinformation,
        Self::Privacy,
        Self::Autonomy,
        Self::SystemOverride,
        Self::Cbrn,
        Self::ChildSafety,
    ];

    /// Stable lowercase tag for audit-body grepability.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::Hate => "hate",
            Self::Violence => "violence",
            Self::Pii => "pii",
            Self::SelfHarm => "self_harm",
            Self::Deception => "deception",
            Self::Cyber => "cyber",
            Self::Misinformation => "misinformation",
            Self::Privacy => "privacy",
            Self::Autonomy => "autonomy",
            Self::SystemOverride => "system_override",
            Self::Cbrn => "cbrn",
            Self::ChildSafety => "child_safety",
        }
    }

    /// Is this a categorical (hard-rule) category whose verdict is
    /// determined by deterministic rule rather than the `CVaR` path?
    /// See `docs/adr/0003-ethics-radar-firstclass.md` §"Layer B".
    #[must_use]
    pub fn is_categorical(self) -> bool {
        matches!(self, Self::Cbrn | Self::ChildSafety)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_variants_are_listed_in_all() {
        // The const ALL must enumerate every variant. If a new
        // category lands, this test fails until ALL is updated.
        let tags: std::collections::HashSet<&str> =
            TaxonomyCategory::ALL.iter().map(|c| c.tag()).collect();
        assert_eq!(tags.len(), 12);
    }

    #[test]
    fn categorical_categories_are_cbrn_and_child_safety() {
        for cat in TaxonomyCategory::ALL {
            let expected = matches!(cat, TaxonomyCategory::Cbrn | TaxonomyCategory::ChildSafety);
            assert_eq!(
                cat.is_categorical(),
                expected,
                "{cat:?} categorical flag mismatch",
            );
        }
    }

    #[test]
    fn tags_round_trip_through_serde() {
        for cat in TaxonomyCategory::ALL {
            let bytes = bincode::serialize(&cat).unwrap();
            let restored: TaxonomyCategory = bincode::deserialize(&bytes).unwrap();
            assert_eq!(cat, restored);
        }
    }
}
