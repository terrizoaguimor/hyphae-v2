// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Structural hate-pattern detector.
//!
//! Lexicon-only matching catches direct slurs but misses
//! structurally-coded hate (dehumanising metaphors, coded
//! references, hate-frame templates). The structural detector
//! patterns the *shape* of hateful argument rather than its surface
//! vocabulary, so it can flag indirect or coded forms that a flat
//! lexicon misses.
//!
//! v0.1 ships a small seed of patterns. Each pattern is a
//! conservative match: high precision is preferred over high
//! recall — the cost of a false positive in this category is high,
//! and the RADAR posture means missed cases produce a corpus
//! baseline signal that Layer K (when introduced) can flag for
//! review.

use crate::taxonomy::TaxonomyCategory;
use serde::{Deserialize, Serialize};

/// A single structural pattern match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuralHit {
    /// The category this pattern flags (always `Hate` in v0.1).
    pub category: TaxonomyCategory,
    /// The pattern identifier — a human-readable name so audit
    /// readers can identify which rule fired without reading the
    /// rule corpus.
    pub pattern_id: String,
    /// Confidence in `[0.0, 1.0]`.
    pub confidence: f32,
}

/// The structural detector.
#[derive(Debug, Clone, Copy, Default)]
pub struct StructuralDetector;

impl StructuralDetector {
    /// Scan the normalised lowercase input for structural patterns.
    /// Returns the set of hits; an empty `Vec` means no pattern
    /// fired.
    #[must_use]
    pub fn scan(&self, normalised_input: &str) -> Vec<StructuralHit> {
        let mut hits = Vec::new();
        for (pattern_id, fragments, confidence) in PATTERNS {
            if fragments.iter().all(|f| normalised_input.contains(f)) {
                hits.push(StructuralHit {
                    category: TaxonomyCategory::Hate,
                    pattern_id: (*pattern_id).to_string(),
                    confidence: *confidence,
                });
            }
        }
        hits
    }
}

/// Each pattern fires when **every** fragment in its `fragments`
/// array appears in the input. The conjunctive shape keeps
/// precision high — a single content word in isolation cannot fire
/// a hit.
///
/// Confidence is the per-pattern confidence weight; the engine
/// composes structural hits with lexicon hits via the Layer A
/// aggregator.
type Pattern = (&'static str, &'static [&'static str], f32);

const PATTERNS: &[Pattern] = &[
    // Dehumanising metaphor — animal frame applied to a group.
    (
        "dehumanising_animal_frame",
        &["these people are", "like animals"],
        0.85,
    ),
    // Group-level threat construction.
    (
        "group_existential_frame",
        &["they are a threat to", "our way of life"],
        0.80,
    ),
    // Vermin metaphor.
    ("vermin_metaphor", &["infesting", "our country"], 0.85),
    // Disposability frame — group cast as disposable / removable.
    (
        "disposability_frame",
        &["should be removed", "from society"],
        0.80,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detector_fires_on_dehumanising_animal_frame() {
        let hits = StructuralDetector
            .scan("these people are unable to behave, they act like animals at every turn");
        assert!(
            hits.iter()
                .any(|h| h.pattern_id == "dehumanising_animal_frame"),
            "expected the dehumanising frame to fire",
        );
    }

    #[test]
    fn detector_does_not_fire_on_innocuous_phrasing() {
        // "like animals" alone does not fire; the conjunctive shape
        // requires the framing prefix.
        let hits = StructuralDetector
            .scan("dogs are wonderful, they greet you like animals greeting an old friend");
        assert!(hits.is_empty(), "single-fragment matches must not fire");
    }

    #[test]
    fn detector_fires_on_vermin_metaphor() {
        let hits = StructuralDetector.scan("they are infesting every neighbourhood in our country");
        assert!(
            hits.iter().any(|h| h.pattern_id == "vermin_metaphor"),
            "expected the vermin metaphor to fire",
        );
    }

    #[test]
    fn detector_categorises_all_hits_as_hate() {
        let hits = StructuralDetector.scan(
            "these people are infesting our country; they are like animals; they should be removed from society",
        );
        for h in &hits {
            assert_eq!(h.category, TaxonomyCategory::Hate);
        }
        assert!(hits.len() >= 3);
    }
}
