// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Layer A — deterministic classifier.
//!
//! Combines:
//! - [`crate::lexicon::Lexicon`] — whole-word matches against the
//!   ethics taxonomy.
//! - [`crate::structural::StructuralDetector`] — structural
//!   patterns for indirectly-coded hate.
//! - [`crate::disambiguation::Disambiguator`] — context awareness
//!   (living target / technical / meta) to suppress over-blocks.
//!
//! Output is a [`LayerAOutput`] carrying per-category confidence
//! aggregates and the raw flag set. The aggregator hands this off
//! to Layer B and to the report builder.

use crate::disambiguation::{DisambiguationVerdict, Disambiguator};
use crate::lexicon::Lexicon;
use crate::structural::{StructuralDetector, StructuralHit};
use crate::taxonomy::TaxonomyCategory;
use hyphae_core::LanguageTag;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single Layer A flag — one term or pattern caused one category
/// to surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerAFlag {
    /// Which category was flagged.
    pub category: TaxonomyCategory,
    /// Confidence in `[0.0, 1.0]`.
    pub confidence: f32,
    /// What caused the flag. Either a lexicon term or a structural
    /// pattern identifier.
    pub source: FlagSource,
    /// `true` when disambiguation marked this flag as suppressed
    /// (the source matched but the context vetoed its surface
    /// effect). Suppressed flags still surface in the audit trail
    /// so an auditor can see what was matched and why it was
    /// withheld.
    pub suppressed: bool,
}

/// What produced a Layer A flag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlagSource {
    /// A lexicon entry whose term appeared in the input.
    Lexicon {
        /// The matched term.
        term: String,
    },
    /// A structural pattern whose conjunctive fragments all
    /// appeared in the input.
    Structural {
        /// The pattern identifier.
        pattern_id: String,
    },
}

/// Result of running Layer A over an input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerAOutput {
    /// Per-category aggregate confidence in `[0.0, 1.0]`. Computed
    /// as the maximum non-suppressed confidence across all flags in
    /// the category. Categories with no flags carry `0.0`.
    pub per_category: HashMap<TaxonomyCategory, f32>,
    /// The full flag set, including suppressed flags. Order is the
    /// order in which the flags were produced (lexicon hits first,
    /// then structural hits).
    pub flags: Vec<LayerAFlag>,
    /// The disambiguation verdict that informed suppression.
    pub disambiguation: DisambiguationVerdict,
}

impl LayerAOutput {
    /// `true` when at least one non-suppressed flag is present.
    #[must_use]
    pub fn has_active_flag(&self) -> bool {
        self.flags.iter().any(|f| !f.suppressed)
    }

    /// Highest non-suppressed confidence across all categories.
    #[must_use]
    pub fn peak_confidence(&self) -> f32 {
        self.per_category.values().copied().fold(0.0, f32::max)
    }

    /// Look up the aggregate confidence for a category. Returns
    /// `0.0` for categories with no active flag.
    #[must_use]
    pub fn confidence_for(&self, category: TaxonomyCategory) -> f32 {
        self.per_category.get(&category).copied().unwrap_or(0.0)
    }
}

/// Layer A evaluator.
#[derive(Debug)]
pub struct LayerA<'a> {
    lexicon: &'a Lexicon,
    structural: StructuralDetector,
    disambiguator: Disambiguator,
}

impl<'a> LayerA<'a> {
    /// Construct a Layer A evaluator bound to a lexicon. The
    /// structural detector and disambiguator are stateless and use
    /// their default scaffold patterns.
    #[must_use]
    pub fn new(lexicon: &'a Lexicon) -> Self {
        Self {
            lexicon,
            structural: StructuralDetector,
            disambiguator: Disambiguator,
        }
    }

    /// Evaluate the input against the lexicon and structural
    /// patterns, with disambiguation-driven suppression.
    #[must_use]
    pub fn evaluate(&self, input: &str, language: &LanguageTag) -> LayerAOutput {
        let normalised = normalise(input);
        let disambiguation = self.disambiguator.classify(&normalised);
        let suppress = disambiguation.suppresses_flags();

        let mut flags: Vec<LayerAFlag> = Vec::new();

        // Lexicon pass — whole-word matches.
        for entry in self.lexicon.entries_for(language) {
            if contains_whole(&normalised, &entry.term) {
                flags.push(LayerAFlag {
                    category: entry.category,
                    confidence: entry.confidence,
                    source: FlagSource::Lexicon {
                        term: entry.term.clone(),
                    },
                    suppressed: suppress && !entry.category.is_categorical(),
                });
            }
        }

        // Structural pass — conjunctive patterns.
        for hit in self.structural.scan(&normalised) {
            let StructuralHit {
                category,
                pattern_id,
                confidence,
            } = hit;
            flags.push(LayerAFlag {
                category,
                confidence,
                source: FlagSource::Structural { pattern_id },
                // Structural hate patterns do not suppress under
                // technical context — a vermin metaphor in a
                // database-schema register is still a vermin
                // metaphor.
                suppressed: false,
            });
        }

        // Aggregate per category: max of non-suppressed confidences.
        let mut per_category: HashMap<TaxonomyCategory, f32> = HashMap::new();
        for f in &flags {
            if f.suppressed {
                continue;
            }
            let entry = per_category.entry(f.category).or_insert(0.0);
            if f.confidence > *entry {
                *entry = f.confidence;
            }
        }

        LayerAOutput {
            per_category,
            flags,
            disambiguation,
        }
    }
}

/// Normalise input for matching: lowercase and collapse whitespace.
/// Punctuation is preserved so multi-token terms remain matchable;
/// the matcher uses whole-word boundaries that are punctuation-aware.
fn normalise(input: &str) -> String {
    let lower = input.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut prev_space = true;
    for c in lower.chars() {
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

/// Check whether `needle` appears in `haystack` at a whole-word
/// boundary. A whole-word match requires the surrounding characters
/// to be non-alphanumeric (or string ends).
fn contains_whole(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let needle_bytes = needle.as_bytes();
    let haystack_bytes = haystack.as_bytes();
    if needle_bytes.len() > haystack_bytes.len() {
        return false;
    }
    let mut i = 0;
    while i + needle_bytes.len() <= haystack_bytes.len() {
        if &haystack_bytes[i..i + needle_bytes.len()] == needle_bytes {
            let before_ok = i == 0 || !is_alnum_byte(haystack_bytes[i - 1]);
            let after_pos = i + needle_bytes.len();
            let after_ok =
                after_pos == haystack_bytes.len() || !is_alnum_byte(haystack_bytes[after_pos]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

const fn is_alnum_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_whole_matches_whole_words_only() {
        assert!(contains_whole("the nerve agent file", "nerve agent"));
        assert!(!contains_whole(
            "preliminary nerve-agentless test",
            "nerve agent"
        ));
        assert!(contains_whole("anthrax.", "anthrax"));
        assert!(!contains_whole("xanthrax", "anthrax"));
    }

    #[test]
    fn evaluate_flags_lexicon_term_in_neutral_context() {
        let lex = Lexicon::baseline_en();
        let layer = LayerA::new(&lex);
        let out = layer.evaluate(
            "the cookbook describes nerve agent precursors for the 1995 attack",
            &LanguageTag::English,
        );
        // Categorical category flags even in non-operational
        // contexts — Layer B's hard rule is what decides the verdict.
        assert!(out.confidence_for(TaxonomyCategory::Cbrn) > 0.0);
    }

    #[test]
    fn evaluate_suppresses_non_categorical_in_meta_context() {
        let lex = Lexicon::baseline_en();
        let layer = LayerA::new(&lex);
        let out = layer.evaluate(
            "to illustrate, the ethics engine treats ignore previous instructions as a system override marker",
            &LanguageTag::English,
        );
        // Meta context: the system_override flag should be present
        // but suppressed.
        let so_flag = out
            .flags
            .iter()
            .find(|f| f.category == TaxonomyCategory::SystemOverride);
        assert!(
            so_flag.is_some(),
            "system_override should appear as a flag even when suppressed",
        );
        assert!(
            so_flag.unwrap().suppressed,
            "meta context must suppress non-categorical flags",
        );
        // And the aggregate confidence for the suppressed category
        // is zero (suppressed flags do not contribute to the
        // aggregate).
        assert!((out.confidence_for(TaxonomyCategory::SystemOverride) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn evaluate_does_not_suppress_categorical_in_meta_context() {
        let lex = Lexicon::baseline_en();
        let layer = LayerA::new(&lex);
        let out = layer.evaluate(
            "for example, the lexicon includes nerve agent as a CBRN anchor",
            &LanguageTag::English,
        );
        let cbrn_flag = out
            .flags
            .iter()
            .find(|f| f.category == TaxonomyCategory::Cbrn);
        assert!(cbrn_flag.is_some());
        assert!(
            !cbrn_flag.unwrap().suppressed,
            "categorical CBRN must not suppress under meta context — the audit must always see it",
        );
    }

    #[test]
    fn evaluate_neutral_text_yields_no_flags() {
        let lex = Lexicon::baseline_en();
        let layer = LayerA::new(&lex);
        let out = layer.evaluate(
            "the weather has been pleasant this week in medellin",
            &LanguageTag::English,
        );
        assert!(!out.has_active_flag());
        assert!(out.flags.is_empty());
    }

    #[test]
    fn evaluate_picks_up_structural_hate() {
        let lex = Lexicon::baseline_en();
        let layer = LayerA::new(&lex);
        let out = layer.evaluate(
            "these people are nothing but trouble, they act like animals every day",
            &LanguageTag::English,
        );
        assert!(out.confidence_for(TaxonomyCategory::Hate) > 0.0);
        assert!(
            out.flags
                .iter()
                .any(|f| matches!(&f.source, FlagSource::Structural { .. })),
            "expected at least one structural source",
        );
    }
}
