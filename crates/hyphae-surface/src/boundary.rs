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

// ── English rule constants (default; back-compat) ─────────────

/// English definite determiners.
const DEFINITE_DETERMINERS_EN: &[&str] = &["the", "this", "that", "these", "those"];

/// English indefinite determiners.
const INDEFINITE_DETERMINERS_EN: &[&str] = &["a", "an"];

/// English anaphor tails (with optional trailing comma).
const ANAPHOR_TAILS_EN: &[&str] = &["it,", "this,", "that,", "it", "this", "that"];

/// English stop-word set used by token extraction.
const STOPWORDS_EN: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "if", "of", "in", "on", "at", "to", "for", "with", "is",
    "are", "was", "were", "be", "been", "being", "this", "that", "these", "those", "i", "you",
    "he", "she", "it", "we", "they", "do", "does", "did", "have", "has", "had", "as", "by", "from",
    "into", "than", "then", "so", "such", "not", "no",
];

/// English markers that identify a connective phrase as
/// "continuation of same subject" — Rule 2 preference.
const SAME_SUBJECT_MARKERS_EN: &[&str] = &[
    "likewise",
    "continuing",
    "in the same direction",
    "along those lines",
    "following from that",
];

// ── ADR-0019 Spanish rule constants ───────────────────────────

/// Spanish definite determiners.
const DEFINITE_DETERMINERS_ES: &[&str] = &[
    "el", "la", "los", "las", "lo", "este", "esta", "estos", "estas", "ese", "esa", "esos", "esas",
    "aquel", "aquella", "aquellos", "aquellas",
];

/// Spanish indefinite determiners.
const INDEFINITE_DETERMINERS_ES: &[&str] = &["un", "una", "unos", "unas"];

/// Spanish anaphor tails (with optional trailing comma).
const ANAPHOR_TAILS_ES: &[&str] = &[
    "lo,", "lo", "eso,", "eso", "esto,", "esto", "ello,", "ello", "aquello,", "aquello",
];

/// Spanish stop-word set (LATAM-leaning register).
const STOPWORDS_ES: &[&str] = &[
    "el", "la", "los", "las", "un", "una", "unos", "unas", "lo", "le", "les", "de", "del", "a",
    "al", "en", "con", "por", "para", "sobre", "entre", "hasta", "hacia", "desde", "según", "sin",
    "y", "o", "pero", "aunque", "si", "no", "ni", "que", "como", "cuando", "donde", "este", "esta",
    "estos", "estas", "ese", "esa", "esos", "esas", "aquel", "aquella", "aquellos", "aquellas",
    "yo", "tú", "él", "ella", "nosotros", "nosotras", "vosotros", "ustedes", "ellos", "ellas",
    "me", "te", "se", "nos", "os", "es", "son", "era", "fue", "ser", "estar", "haber", "ha", "han",
    "había", "habían",
];

/// Spanish same-subject markers — substrings the picker uses to
/// identify a connective as "continuation of same subject".
const SAME_SUBJECT_MARKERS_ES: &[&str] = &[
    "igualmente",
    "asimismo",
    "de igual modo",
    "en la misma línea",
    "continuando con la línea",
    "sumando a esto",
];

/// Language-specific boundary rules. ADR-0019: the same
/// `BoundarySignal` extraction + smoothing rule logic operates
/// over either set; the lexicon supplies which one applies.
#[derive(Debug, Clone, Copy)]
pub struct BoundaryRules {
    /// Definite determiners detected at body start (e.g. "the",
    /// "el", "la").
    pub definite_determiners: &'static [&'static str],
    /// Indefinite determiners (e.g. "a", "an", "un", "una").
    /// Informational; Rule 1 keys on the definite set.
    pub indefinite_determiners: &'static [&'static str],
    /// Anaphor tails matched against a candidate connective's
    /// phrase suffix (e.g. "it,", "lo,").
    pub anaphor_tails: &'static [&'static str],
    /// Common function words filtered out before identifying the
    /// `initial_token` / `final_token` content tokens of a body.
    pub stopwords: &'static [&'static str],
    /// Substring markers the picker scans connective phrases for
    /// to recognise "continuation of same subject" wording — Rule
    /// 2 preference. Language-specific because the connective
    /// phrases are language-specific.
    pub same_subject_markers: &'static [&'static str],
}

impl BoundaryRules {
    /// The English ruleset (ADR-0007 baseline).
    pub const ENGLISH: Self = Self {
        definite_determiners: DEFINITE_DETERMINERS_EN,
        indefinite_determiners: INDEFINITE_DETERMINERS_EN,
        anaphor_tails: ANAPHOR_TAILS_EN,
        stopwords: STOPWORDS_EN,
        same_subject_markers: SAME_SUBJECT_MARKERS_EN,
    };

    /// **ADR-0019.** The Spanish ruleset. Hand-curated by a
    /// native ES speaker.
    pub const SPANISH: Self = Self {
        definite_determiners: DEFINITE_DETERMINERS_ES,
        indefinite_determiners: INDEFINITE_DETERMINERS_ES,
        anaphor_tails: ANAPHOR_TAILS_ES,
        stopwords: STOPWORDS_ES,
        same_subject_markers: SAME_SUBJECT_MARKERS_ES,
    };
}

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
    /// Extract a signal from a body string using **English** rules
    /// (back-compat shim — defaults to [`BoundaryRules::ENGLISH`]).
    /// New code that knows the body's language should call
    /// [`Self::extract_with_rules`] directly.
    #[must_use]
    pub fn extract(body: &str) -> Self {
        Self::extract_with_rules(body, &BoundaryRules::ENGLISH)
    }

    /// **ADR-0019.** Extract a signal from a body string using
    /// the supplied [`BoundaryRules`]. Pure heuristic
    /// tokenisation — splits on non-alphanumerics, lowercases,
    /// inspects the first and last raw tokens against the
    /// rules' determiner and stopword sets.
    #[must_use]
    pub fn extract_with_rules(body: &str, rules: &BoundaryRules) -> Self {
        let raw: Vec<String> = body
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .map(str::to_lowercase)
            .collect();

        let starts_with_definite_determiner = raw
            .first()
            .is_some_and(|t| rules.definite_determiners.contains(&t.as_str()));
        let starts_with_indefinite_determiner = raw
            .first()
            .is_some_and(|t| rules.indefinite_determiners.contains(&t.as_str()));

        let content: Vec<&String> = raw
            .iter()
            .filter(|t| !rules.stopwords.contains(&t.as_str()))
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

/// Should the candidate connective be excluded? Back-compat shim
/// using English rules. New code calls
/// [`should_exclude_with_rules`].
#[must_use]
pub fn should_exclude(
    connective: &Connective,
    prev: &BoundarySignal,
    next: &BoundarySignal,
) -> bool {
    should_exclude_with_rules(connective, prev, next, &BoundaryRules::ENGLISH)
}

/// **ADR-0019.** Should the candidate connective be excluded
/// under the supplied rules? Returns `true` when ANY smoothing
/// rule rejects the candidate.
#[must_use]
pub fn should_exclude_with_rules(
    connective: &Connective,
    prev: &BoundarySignal,
    next: &BoundarySignal,
    rules: &BoundaryRules,
) -> bool {
    // Rule 1 — anaphor before definite-determiner quote.
    if next.starts_with_definite_determiner && ends_with_anaphor(&connective.phrase, rules) {
        return true;
    }
    // Rule 3 — token-overlap repetition.
    if let (Some(p), Some(n)) = (prev.final_token.as_deref(), next.initial_token.as_deref())
        && p == n
        && phrase_contains_word(&connective.phrase, p)
    {
        return true;
    }
    false
}

/// Is this candidate connective the kind we'd prefer for a same-
/// determiner-repetition boundary (Rule 2)? Back-compat shim
/// using English rules.
#[must_use]
pub fn is_continuation_of_same_subject(connective: &Connective) -> bool {
    is_continuation_of_same_subject_with_rules(connective, &BoundaryRules::ENGLISH)
}

/// **ADR-0019.** Rule 2 preference under the supplied rules. The
/// `same_subject_markers` are language-specific substrings the
/// picker scans connective phrases for.
#[must_use]
pub fn is_continuation_of_same_subject_with_rules(
    connective: &Connective,
    rules: &BoundaryRules,
) -> bool {
    if connective.role != ConnectiveRole::Continuation {
        return false;
    }
    let p = connective.phrase.to_lowercase();
    rules.same_subject_markers.iter().any(|m| p.contains(m))
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

/// `true` when the phrase (after lowercasing) ends with one of
/// the language's anaphor tails (per the supplied rules). The
/// check is on the raw end of the phrase including its trailing
/// comma — matches the lexicon's punctuation convention.
fn ends_with_anaphor(phrase: &str, rules: &BoundaryRules) -> bool {
    let p = phrase.to_lowercase();
    for tail in rules.anaphor_tails {
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
        let rules = &BoundaryRules::ENGLISH;
        assert!(ends_with_anaphor("Building on it,", rules));
        assert!(ends_with_anaphor("Adding to this,", rules));
        assert!(ends_with_anaphor("Beyond that,", rules));
        assert!(!ends_with_anaphor("Extending that idea,", rules));
        // word-boundary check: "knit," should NOT trigger.
        assert!(!ends_with_anaphor("knit,", rules));
    }

    // ── ADR-0019 ES boundary rule tests ─────────────────────

    #[test]
    fn es_extract_detects_definite_determiner() {
        let s = BoundarySignal::extract_with_rules(
            "la migración completó a las 14:02 UTC",
            &BoundaryRules::SPANISH,
        );
        assert!(s.starts_with_definite_determiner);
        assert!(!s.starts_with_indefinite_determiner);
        assert_eq!(s.initial_token.as_deref(), Some("migración"));
    }

    #[test]
    fn es_extract_detects_indefinite_determiner() {
        let s = BoundarySignal::extract_with_rules(
            "una fuente anónima reportó algo",
            &BoundaryRules::SPANISH,
        );
        assert!(s.starts_with_indefinite_determiner);
        assert!(!s.starts_with_definite_determiner);
        assert_eq!(s.initial_token.as_deref(), Some("fuente"));
    }

    #[test]
    fn es_extract_skips_stopwords() {
        // "el" + "los" are ES stopwords; "despliegue" + "errores"
        // are content tokens.
        let s = BoundarySignal::extract_with_rules(
            "el despliegue terminó sin los errores",
            &BoundaryRules::SPANISH,
        );
        assert_eq!(s.initial_token.as_deref(), Some("despliegue"));
        assert_eq!(s.final_token.as_deref(), Some("errores"));
    }

    #[test]
    fn es_ends_with_anaphor_matches_es_phrases() {
        let rules = &BoundaryRules::SPANISH;
        assert!(ends_with_anaphor("Continuando con eso,", rules));
        assert!(ends_with_anaphor("Sumando a esto,", rules));
        assert!(ends_with_anaphor("Más allá de aquello,", rules));
        assert!(!ends_with_anaphor("Continuando con la línea,", rules));
        // word-boundary check
        assert!(!ends_with_anaphor("piloto,", rules));
    }

    #[test]
    fn es_rule_one_excludes_anaphor_before_definite_quote() {
        let candidate = conn("Sumando a esto,", ConnectiveRole::Continuation);
        let prev = sig(Some("migración"), true, false, Some("utc"));
        let next = sig(Some("monitores"), true, false, Some("cambio"));
        assert!(should_exclude_with_rules(
            &candidate,
            &prev,
            &next,
            &BoundaryRules::SPANISH,
        ));
    }

    #[test]
    fn es_is_continuation_of_same_subject_matches_es_markers() {
        let rules = &BoundaryRules::SPANISH;
        assert!(is_continuation_of_same_subject_with_rules(
            &conn("Igualmente,", ConnectiveRole::Continuation),
            rules,
        ));
        assert!(is_continuation_of_same_subject_with_rules(
            &conn("Asimismo,", ConnectiveRole::Continuation),
            rules,
        ));
        assert!(is_continuation_of_same_subject_with_rules(
            &conn("En la misma línea,", ConnectiveRole::Continuation),
            rules,
        ));
        // EN markers must NOT match under ES rules.
        assert!(!is_continuation_of_same_subject_with_rules(
            &conn("Likewise,", ConnectiveRole::Continuation),
            rules,
        ));
    }

    #[test]
    fn boundary_rules_constants_are_distinct_languages() {
        // Pointer-distinct ENGLISH and SPANISH.
        assert!(!std::ptr::eq(
            BoundaryRules::ENGLISH.definite_determiners,
            BoundaryRules::SPANISH.definite_determiners
        ));
        // EN contains "the"; ES does not.
        assert!(BoundaryRules::ENGLISH.definite_determiners.contains(&"the"));
        assert!(!BoundaryRules::SPANISH.definite_determiners.contains(&"the"));
        // ES contains "el"; EN does not.
        assert!(BoundaryRules::SPANISH.definite_determiners.contains(&"el"));
        assert!(!BoundaryRules::ENGLISH.definite_determiners.contains(&"el"));
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
