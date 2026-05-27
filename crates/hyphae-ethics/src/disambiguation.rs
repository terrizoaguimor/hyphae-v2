// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Context disambiguation for Layer A.
//!
//! The same surface term can carry different ethical weight in
//! different contexts. v0.1 distinguishes three contexts:
//!
//! - **Living target.** The proposition concerns a specific living
//!   person or identifiable group. PII and privacy categories
//!   weight more heavily; abstract policy discussion of the same
//!   surface term weights less.
//! - **Technical.** The term appears in a technical / engineering
//!   register (database schemas, network protocols, software
//!   processes). Surface terms that look like operational intent
//!   are commonly used metaphorically here ("kill the process",
//!   "execute the script") and should not flag.
//! - **Meta.** The term appears in a discussion *about* the term
//!   itself or about the safety apparatus that evaluates it.
//!   Suppresses Layer A flags to avoid the v1-MANIFESTO §8
//!   over-block — describing a test of the safety engine should
//!   not flag the safety engine.
//!
//! The detector is intentionally cheap: keyword heuristics on the
//! normalised input. Calibration is empirical; the seed heuristics
//! exist to validate the type contract and the layered pipeline.

use serde::{Deserialize, Serialize};

/// Result of disambiguation: which contexts are detected in the
/// input. Multiple contexts can co-occur (technical AND meta is the
/// common case for documentation about the substrate itself).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisambiguationVerdict {
    /// `true` when a specific living-person/group reference is
    /// detected.
    pub living_target: bool,
    /// `true` when a technical / engineering register is detected.
    pub technical_context: bool,
    /// `true` when the text discusses its own terms or the safety
    /// apparatus.
    pub meta_context: bool,
}

impl DisambiguationVerdict {
    /// Should Layer A flags from this input be suppressed?
    ///
    /// Suppression rule (v0.1, conservative): suppress when meta
    /// context is detected, OR when the technical context is
    /// detected without a living-target signal. The intuition is
    /// "technical-but-about-someone" stays flaggable (e.g. doxing
    /// in a database-schema register) while pure-technical loses
    /// the flag, and any meta-context (talking about the system
    /// itself) is non-operational.
    #[must_use]
    pub fn suppresses_flags(self) -> bool {
        self.meta_context || (self.technical_context && !self.living_target)
    }
}

/// Heuristic detector. Inspect lowercase normalised input and
/// produce the disambiguation verdict.
#[derive(Debug, Clone, Copy, Default)]
pub struct Disambiguator;

impl Disambiguator {
    /// Run the disambiguation heuristics over the normalised input.
    /// The input is expected to be lowercase, tokenisable on
    /// whitespace, and free of HTML / markup.
    #[must_use]
    pub fn classify(&self, normalised_input: &str) -> DisambiguationVerdict {
        DisambiguationVerdict {
            living_target: contains_any(normalised_input, LIVING_TARGET_MARKERS),
            technical_context: contains_any(normalised_input, TECHNICAL_MARKERS),
            meta_context: contains_any(normalised_input, META_MARKERS),
        }
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

// ───────────────────────────────────────────────────────────
//  Heuristic markers (v0.1 seed)
//
//  These lists are deliberately short. They validate the type
//  contract and the pipeline; production-grade discrimination is
//  an empirical-calibration task that lands with a future ADR.
// ───────────────────────────────────────────────────────────

/// Markers suggesting the proposition concerns a specific living
/// person or identifiable group.
const LIVING_TARGET_MARKERS: &[&str] = &[
    "my neighbour",
    "my neighbor",
    "this person",
    "his address",
    "her address",
    "their phone number",
    "located at",
    "lives at",
];

/// Markers suggesting a technical / engineering register.
const TECHNICAL_MARKERS: &[&str] = &[
    "process id",
    "the process",
    "the thread",
    "kill the process",
    "execute the script",
    "shell command",
    "system call",
    "database schema",
    "the daemon",
    "the service",
    "compiler error",
    "stack trace",
    "the buffer",
];

/// Markers suggesting the text is *about* its own surface — talking
/// about the substrate, the safety apparatus, the evaluator, the
/// test of the evaluator, or this very document.
const META_MARKERS: &[&str] = &[
    "this rule",
    "this lexicon",
    "the ethics engine",
    "layer a",
    "layer b",
    "the safety engine",
    "the moderator",
    "the evaluator",
    "the policy",
    "documentation of",
    "for example,",
    "as an example",
    "to illustrate",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_technical_register_suppresses() {
        let v = Disambiguator.classify(
            "the daemon refused the system call and crashed the process — see stack trace below",
        );
        assert!(v.technical_context);
        assert!(!v.living_target);
        assert!(
            v.suppresses_flags(),
            "pure technical register must suppress Layer A flags",
        );
    }

    #[test]
    fn meta_context_suppresses_even_without_technical() {
        let v = Disambiguator.classify(
            "to illustrate, the ethics engine treats the term differently in different contexts",
        );
        assert!(v.meta_context);
        assert!(v.suppresses_flags());
    }

    #[test]
    fn technical_plus_living_target_does_not_suppress() {
        let v = Disambiguator.classify(
            "the daemon will deliver the social security number to my neighbor's home address",
        );
        assert!(v.technical_context);
        assert!(v.living_target);
        assert!(
            !v.suppresses_flags(),
            "technical-but-targeting-a-person must NOT suppress — that's the doxing-in-technical-clothing case",
        );
    }

    #[test]
    fn ordinary_prose_neither_suppresses_nor_flags_technically() {
        let v = Disambiguator.classify("the weather has been pleasant this week in medellin");
        assert!(!v.technical_context);
        assert!(!v.meta_context);
        assert!(!v.suppresses_flags());
    }
}
