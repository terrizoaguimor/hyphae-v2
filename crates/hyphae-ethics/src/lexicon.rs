// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Lexicon for Layer A deterministic classification.
//!
//! A [`Lexicon`] maps lowercase terms to ethics taxonomy categories
//! with confidence weights. Multilingual at the type level —
//! v0.1 ships English-only per RFC §9 negative scope; additional
//! languages re-enter additively via a future lexicon-expansion ADR.
//!
//! The seed lexicon in [`Lexicon::baseline_en`] is intentionally
//! small. It exists to validate the type contract and the layered
//! pipeline; production calibration is an empirical task that
//! tracks the `BASELINE_PROFILE`.

use crate::taxonomy::TaxonomyCategory;
use hyphae_core::LanguageTag;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single lexicon entry — a term paired with its taxonomy
/// category and a confidence weight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LexiconEntry {
    /// The lowercase term to match. Whole-word matches in the
    /// normalised input; substring matches are intentionally NOT
    /// performed (substring matching produces the
    /// `"the X attack of 1995"` over-block that the v1 manifesto
    /// section 8 documented as a failure mode).
    pub term: String,
    /// Which taxonomy category this term flags.
    pub category: TaxonomyCategory,
    /// Confidence in `[0.0, 1.0]` that an exact match warrants the
    /// flag in isolation. Disambiguation in [`super::disambiguation`]
    /// may suppress the flag despite a positive confidence.
    pub confidence: f32,
}

impl LexiconEntry {
    /// Construct a new entry, clamping confidence to `[0.0, 1.0]`.
    #[must_use]
    pub fn new(term: impl Into<String>, category: TaxonomyCategory, confidence: f32) -> Self {
        Self {
            term: term.into(),
            category,
            confidence: confidence.clamp(0.0, 1.0),
        }
    }
}

/// A multilingual lexicon. Internally keyed by [`LanguageTag`] so
/// re-introducing additional languages is additive — Layer A
/// consults the lexicon for the request's language tag, falling back
/// to the input fragment's language when the request does not carry
/// one. v0.1 ships English-only.
#[derive(Debug, Clone, Default)]
pub struct Lexicon {
    by_language: HashMap<String, Vec<LexiconEntry>>,
}

impl Lexicon {
    /// Construct an empty lexicon.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert entries for a given language. Repeated calls extend
    /// the per-language vector rather than replacing it, so
    /// multi-source seeds compose cleanly.
    pub fn extend(
        &mut self,
        language: &LanguageTag,
        entries: impl IntoIterator<Item = LexiconEntry>,
    ) {
        let key = language.tag();
        let bucket = self.by_language.entry(key).or_default();
        for entry in entries {
            bucket.push(entry);
        }
    }

    /// Look up the entries for a language. Returns an empty slice if
    /// the language has no entries.
    #[must_use]
    pub fn entries_for(&self, language: &LanguageTag) -> &[LexiconEntry] {
        self.by_language
            .get(&language.tag())
            .map_or(&[][..], Vec::as_slice)
    }

    /// Total entry count across all languages.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_language.values().map(Vec::len).sum()
    }

    /// `true` when the lexicon has no entries in any language.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_language.values().all(Vec::is_empty)
    }

    /// English baseline seed. Small by design — these are
    /// type-contract-validating entries, not a production
    /// calibration. The strong-confidence CBRN and self-harm terms
    /// in particular are the categorical hard-rule anchors that
    /// Layer B's CBRN rule pattern-matches against; broadening them
    /// is the job of an empirical-calibration ADR, not a hand-tune.
    #[must_use]
    pub fn baseline_en() -> Self {
        let mut lex = Self::new();
        lex.extend(&LanguageTag::English, baseline_en_entries());
        lex
    }
}

fn baseline_en_entries() -> Vec<LexiconEntry> {
    use TaxonomyCategory as C;
    [
        // Categorical hard-rule anchors. Layer B's CBRN rule
        // requires BOTH a CBRN-domain term AND operational intent to
        // hard-block; a historical / educational mention alone
        // produces only the Layer A flag.
        ("nerve agent", C::Cbrn, 0.95),
        ("anthrax", C::Cbrn, 0.85),
        ("sarin", C::Cbrn, 0.90),
        ("fissile material", C::Cbrn, 0.90),
        ("pathogen weaponisation", C::Cbrn, 0.95),
        // Self-harm anchors — also a Layer B hard rule when paired
        // with operational intent (means / methods).
        ("suicide method", C::SelfHarm, 0.90),
        ("self-harm method", C::SelfHarm, 0.90),
        // Cyber-offence anchors.
        ("ransomware payload", C::Cyber, 0.85),
        ("zero-day exploit", C::Cyber, 0.80),
        ("credential dump", C::Cyber, 0.75),
        // Violence — high-confidence operational language.
        ("execute the target", C::Violence, 0.85),
        ("kill order", C::Violence, 0.75),
        // PII — anchors that are domain markers rather than
        // category-defining. The disambiguation layer suppresses
        // these in technical contexts (e.g. database schema docs).
        ("social security number", C::Pii, 0.70),
        ("home address of", C::Pii, 0.65),
        // Deception / system override / autonomy — high-confidence
        // anchors only; the broader surface is taxonomy-shaped, not
        // term-shaped, and lives in structural detection.
        ("ignore previous instructions", C::SystemOverride, 0.85),
        ("disregard your guidelines", C::SystemOverride, 0.80),
    ]
    .into_iter()
    .map(|(term, cat, conf)| LexiconEntry::new(term, cat, conf))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_en_lexicon_is_non_empty() {
        let lex = Lexicon::baseline_en();
        assert!(!lex.is_empty());
        assert!(lex.len() >= 10);
    }

    #[test]
    fn baseline_en_only_populates_english() {
        let lex = Lexicon::baseline_en();
        assert!(!lex.entries_for(&LanguageTag::English).is_empty());
        assert!(
            lex.entries_for(&LanguageTag::Other("spanish".to_string()))
                .is_empty(),
            "v0.1 baseline must NOT silently populate non-English entries",
        );
    }

    #[test]
    fn lexicon_entry_confidence_clamps() {
        let high = LexiconEntry::new("x", TaxonomyCategory::Hate, 5.0);
        assert!((high.confidence - 1.0).abs() < f32::EPSILON);
        let low = LexiconEntry::new("x", TaxonomyCategory::Hate, -3.0);
        assert!((low.confidence - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn baseline_en_includes_categorical_anchors() {
        let lex = Lexicon::baseline_en();
        let en = lex.entries_for(&LanguageTag::English);
        let has_cbrn = en.iter().any(|e| e.category == TaxonomyCategory::Cbrn);
        let has_self_harm = en.iter().any(|e| e.category == TaxonomyCategory::SelfHarm);
        assert!(
            has_cbrn,
            "baseline EN must seed at least one CBRN anchor for Layer B's hard rule",
        );
        assert!(
            has_self_harm,
            "baseline EN must seed at least one self-harm anchor for Layer B's hard rule",
        );
    }
}
