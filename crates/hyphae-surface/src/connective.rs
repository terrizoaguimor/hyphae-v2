// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Connective tissue lexicon — EN-only for v0.1.
//!
//! The surface realizer's job is **fragment quotation + connective
//! tissue**, not language generation. The body text of each
//! fragment is emitted verbatim; the connectives between them are
//! drawn from a small fixed vocabulary keyed by role (opening,
//! continuation, contrast, attribution, closing). RFC §5.2.
//!
//! v0.1 is English-only per RFC §9 negative scope. Additional
//! languages re-enter with a lexicon-expansion ADR; the structure
//! here (per-role connective sets) is additive — the type does not
//! evolve when ES, PT, etc. land.

use serde::{Deserialize, Serialize};

/// What role the connective plays in the composed prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConnectiveRole {
    /// The opening of the composition (before the first quoted
    /// fragment).
    Opening,
    /// Between two consecutive quoted fragments that agree (the
    /// second extends or confirms the first).
    Continuation,
    /// Between two fragments where the second contrasts the first.
    Contrast,
    /// Attribution prefix introducing a quoted fragment as the
    /// source's words.
    Attribution,
    /// The closing of the composition (after the last quoted
    /// fragment).
    Closing,
}

/// English connective phrases per role. Always at least one phrase
/// per role; the realizer's `seq` % `len` picks deterministically
/// so a given composition's connectives are reproducible.
const EN_CONNECTIVES: &[(ConnectiveRole, &[&str])] = &[
    (
        ConnectiveRole::Opening,
        &[
            "Drawing from working memory,",
            "Based on what is in scope,",
            "From the fragments available,",
        ],
    ),
    (
        ConnectiveRole::Continuation,
        &[
            "Extending that,",
            "Building on it,",
            "Adding to the picture,",
            "Likewise,",
        ],
    ),
    (
        ConnectiveRole::Contrast,
        &["On the other hand,", "However,", "By contrast,", "Yet —"],
    ),
    (
        ConnectiveRole::Attribution,
        &[
            "The source states:",
            "Per the recorded material:",
            "From the fragment:",
        ],
    ),
    (
        ConnectiveRole::Closing,
        &[
            "That is what working memory holds on this.",
            "That is the substance available.",
            "That is the scope of what I can ground.",
        ],
    ),
];

/// Pluggable lexicon. The default constructor returns the v0.1 EN
/// baseline; integrators can extend with custom phrases via
/// [`Lexicon::add`] — useful for domain-specific compositions
/// (technical, legal, conversational registers).
#[derive(Debug, Clone)]
pub struct Lexicon {
    entries: Vec<(ConnectiveRole, Vec<String>)>,
}

impl Lexicon {
    /// The v0.1 English baseline. Always non-empty per role.
    #[must_use]
    pub fn baseline_en() -> Self {
        let entries = EN_CONNECTIVES
            .iter()
            .map(|(role, phrases)| (*role, phrases.iter().map(|p| (*p).to_string()).collect()))
            .collect();
        Self { entries }
    }

    /// Add a phrase to a role. Useful for register-specific
    /// extensions without rewriting the baseline.
    pub fn add(&mut self, role: ConnectiveRole, phrase: impl Into<String>) {
        if let Some(bucket) = self.entries.iter_mut().find(|(r, _)| *r == role) {
            bucket.1.push(phrase.into());
        } else {
            self.entries.push((role, vec![phrase.into()]));
        }
    }

    /// Pick a connective for a role deterministically by index.
    /// Falls back to a generic placeholder string when the role has
    /// no entries (the baseline always populates every role; this
    /// branch defends against a stripped-down custom lexicon).
    #[must_use]
    pub fn pick(&self, role: ConnectiveRole, seq: usize) -> &str {
        let Some((_, phrases)) = self.entries.iter().find(|(r, _)| *r == role) else {
            return match role {
                ConnectiveRole::Opening => "Drawing from working memory,",
                ConnectiveRole::Continuation => "Extending that,",
                ConnectiveRole::Contrast => "However,",
                ConnectiveRole::Attribution => "The source states:",
                ConnectiveRole::Closing => "That is the substance available.",
            };
        };
        if phrases.is_empty() {
            return "...";
        }
        &phrases[seq % phrases.len()]
    }

    /// Number of phrases registered for a role.
    #[must_use]
    pub fn count(&self, role: ConnectiveRole) -> usize {
        self.entries
            .iter()
            .find(|(r, _)| *r == role)
            .map_or(0, |(_, p)| p.len())
    }
}

impl Default for Lexicon {
    fn default() -> Self {
        Self::baseline_en()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_populates_every_role() {
        let lex = Lexicon::baseline_en();
        assert!(lex.count(ConnectiveRole::Opening) > 0);
        assert!(lex.count(ConnectiveRole::Continuation) > 0);
        assert!(lex.count(ConnectiveRole::Contrast) > 0);
        assert!(lex.count(ConnectiveRole::Attribution) > 0);
        assert!(lex.count(ConnectiveRole::Closing) > 0);
    }

    #[test]
    fn pick_is_deterministic_modulo_count() {
        let lex = Lexicon::baseline_en();
        let count = lex.count(ConnectiveRole::Continuation);
        // Two picks at the same seq return the same string.
        let a = lex.pick(ConnectiveRole::Continuation, 0);
        let b = lex.pick(ConnectiveRole::Continuation, count);
        assert_eq!(a, b);
    }

    #[test]
    fn add_extends_a_role_without_replacing() {
        let mut lex = Lexicon::baseline_en();
        let before = lex.count(ConnectiveRole::Contrast);
        lex.add(ConnectiveRole::Contrast, "On a separate hand,");
        assert_eq!(lex.count(ConnectiveRole::Contrast), before + 1);
    }
}
