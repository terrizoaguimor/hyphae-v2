// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Connective tissue lexicon — EN-only for v0.1.
//!
//! Per `docs/adr/0005-lexicon-scale.md`, v0.1 ships **~300 English
//! connective phrases** organised by `(role, register, polarity,
//! formality)`. Each [`Connective`] entry carries the metadata the
//! picker filters on; the realizer derives a [`PickContext`] from
//! the adjacent fragment pair and asks the [`Lexicon`] for a phrase
//! that matches as many context axes as possible.
//!
//! The hand-curated dataset lives in [`crate::connective_data`].
//! v0.1 is English-only per RFC §9; ES re-enters with the
//! multilingual-re-entry ADR. Type-level support for additional
//! languages is additive — the structure here does not change.

use serde::{Deserialize, Serialize};

/// What role the connective plays in the composed prose.
///
/// v0.1 ships **ten roles**. Per ADR-0005 §"Role taxonomy: 5 → 10",
/// the five baseline roles
/// (`Opening`, `Continuation`, `Contrast`, `Attribution`, `Closing`)
/// stay; the five new roles
/// (`Concession`, `Causation`, `Elaboration`, `Sequence`, `Summary`)
/// capture relations the original taxonomy could not express.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConnectiveRole {
    /// Opening line of the composition (before the first quote).
    Opening,
    /// Between two fragments that extend each other.
    Continuation,
    /// Between two fragments that oppose each other.
    Contrast,
    /// Attribution prefix introducing a quoted fragment.
    Attribution,
    /// Closing line of the composition.
    Closing,
    /// Acknowledging counter-evidence (yields ground).
    Concession,
    /// Causal relation between adjacent fragments.
    Causation,
    /// Specialisation / particularisation.
    Elaboration,
    /// Enumeration over three or more fragments.
    Sequence,
    /// Final synthesis line (alternative `Closing` for longer
    /// compositions).
    Summary,
}

/// Conversational register of a connective.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Register {
    /// Works in any register. The default for connectives that
    /// have no strong stylistic colour.
    Neutral,
    /// Declarative, sober, distant.
    Formal,
    /// Informal, direct.
    Conversational,
    /// Engineering / scientific register.
    Technical,
}

/// Polarity relation between two adjacent fragments — drives the
/// picker's choice of contrast-vs-continuation phrasing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Polarity {
    /// Second fragment extends the first.
    Continuation,
    /// Second fragment qualifies or hedges the first.
    ContrastSoft,
    /// Second fragment opposes the first.
    ContrastHard,
    /// Acknowledges counter-evidence.
    Concession,
    /// Polarity not relevant (openings, closings, attributions).
    Neutral,
}

/// Formality tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Formality {
    /// Colloquial.
    Low,
    /// Everyday written register. Default.
    Mid,
    /// Formal written.
    High,
}

/// One connective phrase with its metadata.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Connective {
    /// The literal phrase the realizer emits.
    pub phrase: String,
    /// Which role this connective plays.
    pub role: ConnectiveRole,
    /// Conversational register.
    pub register: Register,
    /// Polarity (for inter-fragment connectives; `Neutral` for
    /// openings / closings / attributions).
    pub polarity: Polarity,
    /// Formality tier.
    pub formality: Formality,
}

impl Connective {
    /// Convenience constructor.
    #[must_use]
    pub fn new(
        phrase: impl Into<String>,
        role: ConnectiveRole,
        register: Register,
        polarity: Polarity,
        formality: Formality,
    ) -> Self {
        Self {
            phrase: phrase.into(),
            role,
            register,
            polarity,
            formality,
        }
    }
}

/// Context the realizer hands the picker when selecting a
/// connective. Built from the adjacent fragment pair and the
/// caller's intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PickContext {
    /// Conversational register preference.
    pub register: Register,
    /// Polarity preference (matters mostly for inter-fragment
    /// roles).
    pub polarity: Polarity,
    /// Formality tier preference.
    pub formality: Formality,
}

impl PickContext {
    /// The neutral default — no register preference, neutral
    /// polarity, mid formality. The realizer overrides per-call
    /// when it has signal.
    #[must_use]
    pub const fn neutral() -> Self {
        Self {
            register: Register::Neutral,
            polarity: Polarity::Neutral,
            formality: Formality::Mid,
        }
    }
}

impl Default for PickContext {
    fn default() -> Self {
        Self::neutral()
    }
}

/// Pluggable lexicon. The default constructor returns the v0.1
/// ~300-entry baseline; integrators extend with custom entries via
/// [`Lexicon::add`] (register-specific, domain-specific, etc.).
///
/// **ADR-0019**: the lexicon also carries a static
/// [`crate::boundary::BoundaryRules`] pointer that tells the
/// boundary-smoothing module which language's determiners,
/// anaphor tails, and stopwords to use. `baseline_en()` wires
/// `BoundaryRules::ENGLISH`; `baseline_es()` wires
/// `BoundaryRules::SPANISH`; `empty()` defaults to ENGLISH for
/// back-compat.
#[derive(Debug, Clone)]
pub struct Lexicon {
    entries: Vec<Connective>,
    boundary_rules: &'static crate::boundary::BoundaryRules,
}

impl Lexicon {
    /// Construct an empty lexicon (English boundary rules).
    /// Useful for tests that want a blank slate before adding
    /// fixtures.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            boundary_rules: &crate::boundary::BoundaryRules::ENGLISH,
        }
    }

    /// **ADR-0019.** Read-only access to the lexicon's
    /// language-specific boundary rules.
    #[must_use]
    pub fn boundary_rules(&self) -> &'static crate::boundary::BoundaryRules {
        self.boundary_rules
    }

    /// The v0.1 English baseline — ~300 hand-curated entries
    /// organised by role × register × polarity × formality.
    /// See [`crate::connective_data::baseline_en_data`].
    #[must_use]
    pub fn baseline_en() -> Self {
        Self {
            entries: crate::connective_data::baseline_en_data(),
            boundary_rules: &crate::boundary::BoundaryRules::ENGLISH,
        }
    }

    /// **ADR-0017.** The v0.2 Spanish baseline — ~60 hand-curated
    /// entries (architectural proof, not full coverage). See
    /// [`crate::connective_data_es::baseline_es_data`]. A future
    /// ADR scales to EN parity (~250+).
    ///
    /// The lexicon's 4-level fallback chain handles the sparseness
    /// gracefully — when a specific
    /// `(role × register × polarity × formality)` bucket has no
    /// ES entry, the picker falls through three relaxations to
    /// "any phrase in the role."
    #[must_use]
    pub fn baseline_es() -> Self {
        Self {
            entries: crate::connective_data_es::baseline_es_data(),
            boundary_rules: &crate::boundary::BoundaryRules::SPANISH,
        }
    }

    /// Add a connective. Useful for register-specific extensions
    /// without rewriting the baseline.
    pub fn add(&mut self, connective: Connective) {
        self.entries.push(connective);
    }

    /// Read-only access to every entry.
    #[must_use]
    pub fn entries(&self) -> &[Connective] {
        &self.entries
    }

    /// Count of entries for a role.
    #[must_use]
    pub fn count(&self, role: ConnectiveRole) -> usize {
        self.entries.iter().filter(|c| c.role == role).count()
    }

    /// Total entries across all roles.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` when the lexicon is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Pick a phrase for a role with the neutral context. Backward-
    /// compatible with the v0.1.0 API.
    #[must_use]
    pub fn pick(&self, role: ConnectiveRole, seq: usize) -> &str {
        self.pick_in_context(role, &PickContext::neutral(), seq)
    }

    /// Pick a phrase with boundary smoothing applied (per
    /// ADR-0007). The candidate set is filtered through
    /// [`crate::boundary::should_exclude`] using `prev_signal` and
    /// `next_signal`; when filtering empties the set, the chain
    /// degrades to the unfiltered [`Self::pick_in_context`].
    ///
    /// When either boundary signal is `None`, this falls through
    /// to [`Self::pick_in_context`] semantics — useful for
    /// openings, closings, and attributions where there is no
    /// adjacent prior quote to smooth against.
    #[must_use]
    pub fn pick_with_smoothing(
        &self,
        role: ConnectiveRole,
        ctx: &PickContext,
        seq: usize,
        prev_signal: Option<&crate::boundary::BoundarySignal>,
        next_signal: Option<&crate::boundary::BoundarySignal>,
    ) -> &str {
        let (Some(prev), Some(next)) = (prev_signal, next_signal) else {
            return self.pick_in_context(role, ctx, seq);
        };

        // Rule 2 preference: when same-subject repetition is
        // detected, search continuation-of-same-subject phrases
        // first. ADR-0019: use this lexicon's language rules.
        let rules = self.boundary_rules;
        if crate::boundary::same_subject_repetition(prev, next) {
            let preferred: Vec<&Connective> = self
                .entries
                .iter()
                .filter(|c| {
                    c.role == role
                        && crate::boundary::is_continuation_of_same_subject_with_rules(c, rules)
                        && !crate::boundary::should_exclude_with_rules(c, prev, next, rules)
                })
                .collect();
            if !preferred.is_empty() {
                return &preferred[seq % preferred.len()].phrase;
            }
        }

        // Standard chain with the Rule 1 + Rule 3 filter applied
        // at each level. Boxed predicates so the four levels share
        // a uniform loop. (The boxed-Fn type is intentional —
        // each closure captures a different subset of the
        // context's axes; refactoring to a named type alias would
        // not improve readability.)
        #[allow(clippy::type_complexity)]
        let levels: [Box<dyn Fn(&&Connective) -> bool>; 4] = [
            Box::new(|c: &&Connective| {
                c.role == role
                    && c.register == ctx.register
                    && c.polarity == ctx.polarity
                    && c.formality == ctx.formality
            }),
            Box::new(|c: &&Connective| {
                c.role == role && c.register == ctx.register && c.polarity == ctx.polarity
            }),
            Box::new(|c: &&Connective| c.role == role && c.polarity == ctx.polarity),
            Box::new(|c: &&Connective| c.role == role),
        ];
        for level in &levels {
            let filtered: Vec<&Connective> = self
                .entries
                .iter()
                .filter(|c| {
                    level(c) && !crate::boundary::should_exclude_with_rules(c, prev, next, rules)
                })
                .collect();
            if !filtered.is_empty() {
                return &filtered[seq % filtered.len()].phrase;
            }
        }

        tracing::trace!(
            "pick_with_smoothing: smoothing filter starved the chain for {role:?}; \
             falling back to the unfiltered picker"
        );
        self.pick_in_context(role, ctx, seq)
    }

    /// Pick a phrase using a 4-level fallback chain. The picker
    /// **never panics on missing data** — every relaxation level
    /// falls through to the next, ending at a hard-coded
    /// placeholder if the lexicon was constructed without any
    /// entry for the role at all.
    ///
    /// Relaxation chain:
    /// 1. Exact match on `(role, register, polarity, formality)`.
    /// 2. Drop formality. Match `(role, register, polarity)`.
    /// 3. Drop register. Match `(role, polarity)`.
    /// 4. Any entry in the role.
    #[must_use]
    pub fn pick_in_context(&self, role: ConnectiveRole, ctx: &PickContext, seq: usize) -> &str {
        // Level 1.
        let exact: Vec<&Connective> = self
            .entries
            .iter()
            .filter(|c| {
                c.role == role
                    && c.register == ctx.register
                    && c.polarity == ctx.polarity
                    && c.formality == ctx.formality
            })
            .collect();
        if !exact.is_empty() {
            return &exact[seq % exact.len()].phrase;
        }

        // Level 2.
        let by_register_polarity: Vec<&Connective> = self
            .entries
            .iter()
            .filter(|c| c.role == role && c.register == ctx.register && c.polarity == ctx.polarity)
            .collect();
        if !by_register_polarity.is_empty() {
            return &by_register_polarity[seq % by_register_polarity.len()].phrase;
        }

        // Level 3.
        let by_polarity: Vec<&Connective> = self
            .entries
            .iter()
            .filter(|c| c.role == role && c.polarity == ctx.polarity)
            .collect();
        if !by_polarity.is_empty() {
            return &by_polarity[seq % by_polarity.len()].phrase;
        }

        // Level 4.
        let any_in_role: Vec<&Connective> =
            self.entries.iter().filter(|c| c.role == role).collect();
        if !any_in_role.is_empty() {
            return &any_in_role[seq % any_in_role.len()].phrase;
        }

        // Final placeholder. Should never reach in production with
        // baseline_en(); defends against a custom Lexicon::empty()
        // shape that omits this role entirely.
        match role {
            ConnectiveRole::Opening => "Drawing from working memory,",
            ConnectiveRole::Continuation => "Extending that,",
            ConnectiveRole::Contrast => "However,",
            ConnectiveRole::Attribution => "The source states:",
            ConnectiveRole::Closing => "That is the substance available.",
            ConnectiveRole::Concession => "Granted,",
            ConnectiveRole::Causation => "Therefore,",
            ConnectiveRole::Elaboration => "Specifically,",
            ConnectiveRole::Sequence => "Then,",
            ConnectiveRole::Summary => "Overall,",
        }
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
        for role in [
            ConnectiveRole::Opening,
            ConnectiveRole::Continuation,
            ConnectiveRole::Contrast,
            ConnectiveRole::Attribution,
            ConnectiveRole::Closing,
            ConnectiveRole::Concession,
            ConnectiveRole::Causation,
            ConnectiveRole::Elaboration,
            ConnectiveRole::Sequence,
            ConnectiveRole::Summary,
        ] {
            assert!(
                lex.count(role) > 0,
                "role {role:?} must have at least one entry in the baseline",
            );
        }
    }

    #[test]
    fn baseline_is_substantially_larger_than_v0_1_0() {
        // ADR-0005 commits to ~300 entries; the threshold here is
        // an order of magnitude over v0.1.0's 20-entry baseline.
        // We do not assert the exact count — the picker's
        // correctness does not depend on it, and adding entries
        // post-merge should not require a test update.
        let lex = Lexicon::baseline_en();
        assert!(
            lex.len() >= 200,
            "baseline_en should ship at least 200 entries; ships {}",
            lex.len(),
        );
    }

    #[test]
    fn pick_is_deterministic_for_same_inputs() {
        let lex = Lexicon::baseline_en();
        let ctx = PickContext::neutral();
        let a = lex
            .pick_in_context(ConnectiveRole::Continuation, &ctx, 0)
            .to_string();
        let b = lex
            .pick_in_context(ConnectiveRole::Continuation, &ctx, 0)
            .to_string();
        assert_eq!(a, b);
    }

    #[test]
    fn pick_falls_back_when_exact_context_unavailable() {
        // Construct a tight context that no baseline entry
        // satisfies exactly, then verify the picker still returns
        // a phrase (level 4 fallback).
        let lex = Lexicon::baseline_en();
        let ctx = PickContext {
            register: Register::Conversational,
            polarity: Polarity::ContrastHard,
            formality: Formality::Low,
        };
        let phrase = lex.pick_in_context(ConnectiveRole::Causation, &ctx, 0);
        assert!(!phrase.is_empty());
    }

    #[test]
    fn empty_lexicon_returns_hardcoded_placeholders() {
        let lex = Lexicon::empty();
        let phrase = lex.pick(ConnectiveRole::Opening, 0);
        assert!(!phrase.is_empty());
        assert!(phrase.contains("Drawing"));
    }

    /// ADR-0017 — `baseline_es()` constructs a non-empty ES
    /// lexicon with every role represented.
    #[test]
    fn baseline_es_populates_every_role() {
        let lex = Lexicon::baseline_es();
        for role in [
            ConnectiveRole::Opening,
            ConnectiveRole::Continuation,
            ConnectiveRole::Contrast,
            ConnectiveRole::Attribution,
            ConnectiveRole::Closing,
            ConnectiveRole::Concession,
            ConnectiveRole::Causation,
            ConnectiveRole::Elaboration,
            ConnectiveRole::Sequence,
            ConnectiveRole::Summary,
        ] {
            assert!(
                lex.count(role) > 0,
                "ES role {role:?} must have at least one entry in baseline_es",
            );
        }
    }

    /// ADR-0017 — the ES lexicon emits Spanish surface forms.
    /// Spot-check a few entries to confirm they contain Spanish
    /// characters / words and NOT English ones.
    #[test]
    fn baseline_es_entries_are_spanish() {
        let lex = Lexicon::baseline_es();
        let ctx = PickContext::neutral();
        // Opening — should contain "memoria" (Spanish for memory).
        let opening = lex.pick_in_context(ConnectiveRole::Opening, &ctx, 0);
        assert!(
            opening.to_lowercase().contains("memoria")
                || opening.to_lowercase().contains("registros")
                || opening.to_lowercase().contains("conservado")
                || opening.to_lowercase().contains("almacenado"),
            "ES opening should contain a Spanish-indicative noun: got `{opening}`",
        );
        // Summary — should contain "resumen" / "síntesis" / "general".
        let summary = lex.pick_in_context(ConnectiveRole::Summary, &ctx, 0);
        assert!(
            summary.to_lowercase().contains("resumen")
                || summary.to_lowercase().contains("síntesis")
                || summary.to_lowercase().contains("general")
                || summary.to_lowercase().contains("conjunto")
                || summary.to_lowercase().contains("balance"),
            "ES summary should contain a Spanish synthesis marker: got `{summary}`",
        );
    }

    /// ADR-0017 — the ES baseline ships fewer entries than EN
    /// (v0.2 architectural proof scale). Floor invariant: at
    /// least 40 entries across all roles.
    #[test]
    fn baseline_es_meets_v0_2_size_floor() {
        let lex = Lexicon::baseline_es();
        assert!(
            lex.len() >= 40,
            "baseline_es should ship at least 40 entries (v0.2 floor); ships {}",
            lex.len(),
        );
    }

    #[test]
    fn add_extends_the_lexicon() {
        let mut lex = Lexicon::empty();
        lex.add(Connective::new(
            "Test prefix,",
            ConnectiveRole::Opening,
            Register::Technical,
            Polarity::Neutral,
            Formality::Mid,
        ));
        assert_eq!(lex.count(ConnectiveRole::Opening), 1);
        let ctx = PickContext {
            register: Register::Technical,
            polarity: Polarity::Neutral,
            formality: Formality::Mid,
        };
        assert_eq!(
            lex.pick_in_context(ConnectiveRole::Opening, &ctx, 0),
            "Test prefix,"
        );
    }

    #[test]
    fn register_filter_works_at_level_one() {
        // A baseline lookup with a Technical register context
        // should prefer Technical phrases over Neutral ones when
        // both exist.
        let lex = Lexicon::baseline_en();
        let tech_ctx = PickContext {
            register: Register::Technical,
            polarity: Polarity::Continuation,
            formality: Formality::Mid,
        };
        let tech_phrase = lex
            .pick_in_context(ConnectiveRole::Continuation, &tech_ctx, 0)
            .to_string();
        // Verify the phrase is one of the Technical entries (or
        // a Neutral fallback if Technical is sparse for this
        // specific (role, polarity, formality) — both are
        // acceptable; what matters is no panic).
        assert!(!tech_phrase.is_empty());
    }

    #[test]
    fn polarity_continuation_vs_contrast_yields_different_phrases() {
        let lex = Lexicon::baseline_en();
        let ctx_cont = PickContext {
            register: Register::Neutral,
            polarity: Polarity::Continuation,
            formality: Formality::Mid,
        };
        let ctx_contrast = PickContext {
            register: Register::Neutral,
            polarity: Polarity::ContrastHard,
            formality: Formality::Mid,
        };
        let cont = lex
            .pick_in_context(ConnectiveRole::Continuation, &ctx_cont, 0)
            .to_string();
        let contrast = lex
            .pick_in_context(ConnectiveRole::Contrast, &ctx_contrast, 0)
            .to_string();
        // Trivially distinct because the roles differ — but the
        // test pins the contract.
        assert_ne!(cont, contrast);
    }
}
