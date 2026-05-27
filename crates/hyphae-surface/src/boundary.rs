// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Boundary signals and smoothing filters — per ADR-0007.
//!
//! Light heuristic extraction of the boundary characteristics of a
//! quoted body (initial / final content tokens, determiner
//! presence). The realizer reads these signals for each adjacent
//! fragment pair and filters its connective candidate set to avoid
//! known redundancy patterns:
//!
//! - **Rule 1** — anaphor + definite-determiner stack
//!   (`"Building on it, The migration..."`): exclude connectives
//!   ending with `it,` / `this,` / `that,` when the next body
//!   starts with a definite determiner.
//! - **Rule 2** — same-determiner repetition: prefer
//!   continuation-of-same-subject connectives when both bodies
//!   open with the same determiner-led NP.
//! - **Rule 3** — token-overlap repetition: exclude connectives
//!   whose phrase contains the exact content token that bridges
//!   the previous body's final token and the next body's initial
//!   token.
//!
//! The fragment bodies are **never modified**. Smoothing is purely
//! a filter over which connective phrase the picker emits.

use crate::connective::{Connective, ConnectiveRole};

/// Definite determiners detected at body start.
const DEFINITE_DETERMINERS: &[&str] = &["the", "this", "that", "these", "those"];

/// Indefinite determiners detected at body start.
const INDEFINITE_DETERMINERS: &[&str] = &["a", "an"];

/// Singular-pronoun anaphor tails the picker considers "ending
/// with a pronoun". The trailing comma matches the lexicon's
/// punctuation convention.
const ANAPHOR_TAILS: &[&str] = &["it,", "this,", "that,", "it", "this", "that"];

/// Stop-word set used by token extraction. Same shape as the
/// `hyphae_core::StopwordFilteringExtractor` + the lexicon's
/// hashing tokeniser: common English function words.
const STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "if", "of", "in", "on", "at", "to", "for", "with", "is",
    "are", "was", "were", "be", "been", "being", "this", "that", "these", "those", "i", "you",
    "he", "she", "it", "we", "they", "do", "does", "did", "have", "has", "had", "as", "by", "from",
    "into", "than", "then", "so", "such", "not", "no",
];

/// Per-body signal extracted at picker time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundarySignal {
    /// First content token of the body, lowercased.
    /// `None` for empty or stopword-only bodies.
    pub initial_token: Option<String>,
    /// `true` when the body begins with a definite determiner
    /// ("the", "this", "that", "these", "those").
    pub starts_with_definite_determiner: bool,
    /// `true` when the body begins with an indefinite determiner
    /// ("a", "an").
    pub starts_with_indefinite_determiner: bool,
    /// Last content token of the body, lowercased.
    pub final_token: Option<String>,
}

impl BoundarySignal {
    /// Extract a signal from a body string. Pure heuristic
    /// tokenisation — splits on non-alphanumerics, lowercases,
    /// inspects the first and last raw tokens for determiner
    /// markers and content-token identity.
    #[must_use]
    pub fn extract(body: &str) -> Self {
        let raw: Vec<String> = body
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .map(str::to_lowercase)
            .collect();

        let starts_with_definite_determiner = raw
            .first()
            .is_some_and(|t| DEFINITE_DETERMINERS.contains(&t.as_str()));
        let starts_with_indefinite_determiner = raw
            .first()
            .is_some_and(|t| INDEFINITE_DETERMINERS.contains(&t.as_str()));

        let content: Vec<&String> = raw
            .iter()
            .filter(|t| !STOPWORDS.contains(&t.as_str()))
            .collect();
        let initial_token = content.first().map(|s| (*s).clone());
        let final_token = content.last().map(|s| (*s).clone());

        Self {
            initial_token,
            starts_with_definite_determiner,
            starts_with_indefinite_determiner,
            final_token,
        }
    }
}

/// Should the candidate connective be excluded for this boundary?
/// Returns `true` when ANY smoothing rule rejects the candidate.
///
/// `prev` is the signal from the previous quoted body; `next` is
/// the signal from the body the connective is about to introduce.
/// `connective` is the candidate phrase under consideration.
#[must_use]
pub fn should_exclude(
    connective: &Connective,
    prev: &BoundarySignal,
    next: &BoundarySignal,
) -> bool {
    // Rule 1 — anaphor before definite-determiner quote.
    if next.starts_with_definite_determiner && ends_with_anaphor(&connective.phrase) {
        return true;
    }
    // Rule 3 — token-overlap repetition. Skip if the connective
    // phrase mentions the overlapping content token.
    if let (Some(p), Some(n)) = (prev.final_token.as_deref(), next.initial_token.as_deref())
        && p == n
        && phrase_contains_word(&connective.phrase, p)
    {
        return true;
    }
    false
}

/// Is this candidate connective the kind we'd prefer for a
/// same-determiner-repetition boundary (Rule 2)? When both bodies
/// open with the same determiner, the picker prefers candidates
/// that return `true` here — explicit continuation-of-same-subject
/// phrasing. Falls through if no preferred candidate exists.
#[must_use]
pub fn is_continuation_of_same_subject(connective: &Connective) -> bool {
    if connective.role != ConnectiveRole::Continuation {
        return false;
    }
    let p = connective.phrase.to_lowercase();
    // Phrases that explicitly mark same-subject continuation.
    p.contains("likewise")
        || p.contains("continuing")
        || p.contains("in the same direction")
        || p.contains("along those lines")
        || p.contains("following from that")
}

/// `true` when both bodies open with a definite determiner AND
/// share the same first content token — the "same-subject
/// repetition" pattern Rule 2 targets.
#[must_use]
pub fn same_subject_repetition(prev: &BoundarySignal, next: &BoundarySignal) -> bool {
    prev.starts_with_definite_determiner
        && next.starts_with_definite_determiner
        && prev.initial_token.is_some()
        && prev.initial_token == next.initial_token
}

/// `true` when the phrase (after lowercasing) ends with one of the
/// pronoun-anaphor tails. The check is on the raw end of the
/// phrase including its trailing comma — matches the lexicon's
/// punctuation convention (`"Building on it,"`).
fn ends_with_anaphor(phrase: &str) -> bool {
    let p = phrase.to_lowercase();
    for tail in ANAPHOR_TAILS {
        // Match "ends with tail" considering word boundary on the
        // left. "Knit," should NOT match "it,".
        if p.ends_with(tail) {
            // Verify word boundary: char before the tail is not
            // alphanumeric (or the tail starts the string).
            let head_len = p.len() - tail.len();
            if head_len == 0 {
                return true;
            }
            let prev_byte = p.as_bytes()[head_len - 1];
            if !prev_byte.is_ascii_alphanumeric() {
                return true;
            }
        }
    }
    false
}

/// Case-insensitive whole-word containment check.
fn phrase_contains_word(phrase: &str, word: &str) -> bool {
    let p = phrase.to_lowercase();
    let w = word.to_lowercase();
    for token in p.split(|c: char| !c.is_alphanumeric()) {
        if token == w {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connective::{Formality, Polarity, Register};

    fn conn(phrase: &str, role: ConnectiveRole) -> Connective {
        Connective::new(
            phrase,
            role,
            Register::Neutral,
            Polarity::Continuation,
            Formality::Mid,
        )
    }

    fn sig(
        initial: Option<&str>,
        definite: bool,
        indefinite: bool,
        final_tok: Option<&str>,
    ) -> BoundarySignal {
        BoundarySignal {
            initial_token: initial.map(str::to_string),
            starts_with_definite_determiner: definite,
            starts_with_indefinite_determiner: indefinite,
            final_token: final_tok.map(str::to_string),
        }
    }

    #[test]
    fn extract_detects_definite_determiner() {
        let s = BoundarySignal::extract("The migration completed at 14:02 UTC");
        assert!(s.starts_with_definite_determiner);
        assert!(!s.starts_with_indefinite_determiner);
        assert_eq!(s.initial_token.as_deref(), Some("migration"));
    }

    #[test]
    fn extract_detects_indefinite_determiner() {
        let s = BoundarySignal::extract("An anonymous source said something");
        assert!(s.starts_with_indefinite_determiner);
        assert!(!s.starts_with_definite_determiner);
        assert_eq!(s.initial_token.as_deref(), Some("anonymous"));
    }

    #[test]
    fn extract_skips_stopwords_for_initial_token() {
        let s = BoundarySignal::extract("the deploy succeeded on the first attempt");
        assert_eq!(s.initial_token.as_deref(), Some("deploy"));
        assert_eq!(s.final_token.as_deref(), Some("attempt"));
    }

    #[test]
    fn extract_handles_empty_body() {
        let s = BoundarySignal::extract("");
        assert!(s.initial_token.is_none());
        assert!(s.final_token.is_none());
        assert!(!s.starts_with_definite_determiner);
    }

    #[test]
    fn ends_with_anaphor_matches_the_lexicon_baseline() {
        assert!(ends_with_anaphor("Building on it,"));
        assert!(ends_with_anaphor("Adding to this,"));
        assert!(ends_with_anaphor("Beyond that,"));
        assert!(!ends_with_anaphor("Extending that idea,"));
        // word-boundary check: "knit," should NOT trigger.
        assert!(!ends_with_anaphor("knit,"));
    }

    #[test]
    fn rule_one_excludes_anaphor_before_definite_quote() {
        let candidate = conn("Building on it,", ConnectiveRole::Continuation);
        let prev = sig(Some("migration"), true, false, Some("utc"));
        let next = sig(Some("monitoring"), true, false, Some("cutover"));
        assert!(should_exclude(&candidate, &prev, &next));
    }

    #[test]
    fn rule_one_does_not_exclude_when_next_is_not_determiner_led() {
        let candidate = conn("Building on it,", ConnectiveRole::Continuation);
        let prev = sig(Some("migration"), true, false, Some("utc"));
        let next = sig(Some("ten"), false, false, Some("hours"));
        assert!(!should_exclude(&candidate, &prev, &next));
    }

    #[test]
    fn rule_three_excludes_when_token_overlap_appears_in_phrase() {
        let candidate = conn("On the deploy front,", ConnectiveRole::Continuation);
        let prev = sig(Some("migration"), true, false, Some("deploy"));
        let next = sig(Some("deploy"), true, false, Some("succeeded"));
        assert!(should_exclude(&candidate, &prev, &next));
    }

    #[test]
    fn rule_three_does_not_exclude_when_phrase_lacks_overlapping_token() {
        let candidate = conn("Likewise,", ConnectiveRole::Continuation);
        let prev = sig(Some("migration"), true, false, Some("deploy"));
        let next = sig(Some("deploy"), true, false, Some("succeeded"));
        assert!(!should_exclude(&candidate, &prev, &next));
    }

    #[test]
    fn same_subject_repetition_detection() {
        let a = sig(Some("deploy"), true, false, Some("succeeded"));
        let b = sig(Some("deploy"), true, false, Some("rolled"));
        assert!(same_subject_repetition(&a, &b));
        let c = sig(Some("migration"), true, false, Some("done"));
        assert!(!same_subject_repetition(&a, &c));
    }

    #[test]
    fn is_continuation_of_same_subject_matches_lexicon_phrases() {
        assert!(is_continuation_of_same_subject(&conn(
            "Likewise,",
            ConnectiveRole::Continuation
        )));
        assert!(is_continuation_of_same_subject(&conn(
            "Continuing,",
            ConnectiveRole::Continuation
        )));
        assert!(is_continuation_of_same_subject(&conn(
            "Along those lines,",
            ConnectiveRole::Continuation
        )));
        // Wrong role.
        assert!(!is_continuation_of_same_subject(&conn(
            "Likewise,",
            ConnectiveRole::Contrast
        )));
        // Wrong phrase.
        assert!(!is_continuation_of_same_subject(&conn(
            "Furthermore,",
            ConnectiveRole::Continuation
        )));
    }
}
