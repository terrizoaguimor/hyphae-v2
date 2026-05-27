// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Honest limitation triggers.
//!
//! Per `docs/rfc/v1-living.md` §5.3, the surface realizer MUST
//! emit an explicit limitation acknowledgment when any of three
//! triggers fires. These are the architectural property that
//! distinguishes Hyphae from systems that confabulate when material
//! is insufficient — the realizer **says so** rather than producing
//! a composition that fabricates from nothing.
//!
//! The three v0.1 triggers:
//!
//! - [`LimitationTrigger::EmptyWorkingSet`]. The composer handed
//!   the realizer no fragments to quote. The realizer cannot
//!   produce a composition from nothing.
//! - [`LimitationTrigger::HighConfabRisk`]. At least one fragment in
//!   the working set carries `confabulation_risk >= 0.5`. The
//!   composition is downgraded to an acknowledgment because the
//!   underlying material is not trustworthy.
//! - [`LimitationTrigger::ShallowCascade`]. The working set was
//!   assembled from too few cascade hops — the composer reached
//!   for material but the substrate produced only the direct
//!   retrieval, not the associative expansion. v0.1's threshold
//!   is "working set has fewer than `min_cascade_depth` fragments
//!   whose `hops_from_source >= 1`"; the integrator overrides via
//!   [`LimitationContext::min_cascade_fragments`].

use hyphae_core::CognitiveFragment;
use hyphae_ethics::EthicsReport;
use serde::{Deserialize, Serialize};

/// Threshold for [`LimitationTrigger::HighConfabRisk`]. Any fragment
/// at this risk or higher fires the trigger.
pub const HIGH_CONFAB_RISK_THRESHOLD: f32 = 0.5;

/// One of the three v0.1 limitation triggers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LimitationTrigger {
    /// The working set was empty — no fragments to compose from.
    EmptyWorkingSet,
    /// At least one fragment in the working set is high-confabulation-
    /// risk.
    HighConfabRisk,
    /// The working set carries fewer cascade-derived fragments than
    /// the integrator's minimum.
    ShallowCascade,
    /// The ethics report at the `Compose` coverage point hinted
    /// that the composer should acknowledge a limitation (the
    /// categorical-rule or elevated-tail-risk paths).
    EthicallySensitive,
}

impl LimitationTrigger {
    /// Stable lowercase tag for audit-body grepability.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::EmptyWorkingSet => "empty_working_set",
            Self::HighConfabRisk => "high_confab_risk",
            Self::ShallowCascade => "shallow_cascade",
            Self::EthicallySensitive => "ethically_sensitive",
        }
    }

    /// Human-readable acknowledgment line the realizer adds to the
    /// composition when this trigger fires. EN-only per RFC §9.
    #[must_use]
    pub fn acknowledgment(self) -> &'static str {
        match self {
            Self::EmptyWorkingSet => {
                "I do not have material in working memory to ground a response to this query."
            }
            Self::HighConfabRisk => {
                "The material I am drawing from carries elevated confabulation risk; \
                 treat the following as provisional."
            }
            Self::ShallowCascade => {
                "I am reaching directly from a small set of fragments without associative \
                 depth; the response may be narrower than the query warrants."
            }
            Self::EthicallySensitive => {
                "The query touches ethically sensitive material; the response below avoids \
                 operational detail."
            }
        }
    }
}

/// Per-call inputs to [`evaluate`]. Lets the integrator override the
/// minimum cascade-depth threshold without rebuilding the realizer.
#[derive(Debug, Clone, Copy)]
pub struct LimitationContext {
    /// Minimum count of working-set fragments whose
    /// `provenance.parent_ids` is non-empty (i.e. cascade-derived
    /// rather than direct seeds). When the working set carries
    /// fewer than this, [`LimitationTrigger::ShallowCascade`] fires.
    pub min_cascade_fragments: usize,
}

impl Default for LimitationContext {
    fn default() -> Self {
        Self {
            min_cascade_fragments: 1,
        }
    }
}

/// Evaluate the three triggers against a working set and (optionally)
/// an ethics report from the `Compose` coverage point. Returns the
/// set of triggers that fired, in detection order.
///
/// The realizer fires every trigger it detects — they are not
/// mutually exclusive. An empty working set with high confabulation
/// risk on a phantom fragment, for example, fires both triggers and
/// the realizer surfaces both acknowledgments.
#[must_use]
pub fn evaluate(
    working_set: &[CognitiveFragment],
    ethics: Option<&EthicsReport>,
    context: LimitationContext,
) -> Vec<LimitationTrigger> {
    let mut out = Vec::new();

    if working_set.is_empty() {
        out.push(LimitationTrigger::EmptyWorkingSet);
    }

    if working_set
        .iter()
        .any(|f| f.provenance.confabulation_risk >= HIGH_CONFAB_RISK_THRESHOLD)
    {
        out.push(LimitationTrigger::HighConfabRisk);
    }

    if !working_set.is_empty() {
        let cascade_fragments = working_set
            .iter()
            .filter(|f| !f.provenance.parent_ids.is_empty())
            .count();
        if cascade_fragments < context.min_cascade_fragments {
            out.push(LimitationTrigger::ShallowCascade);
        }
    }

    if let Some(report) = ethics
        && report.signals.composer_should_acknowledge
    {
        out.push(LimitationTrigger::EthicallySensitive);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyphae_core::{FragmentContent, FragmentId};
    use hyphae_ethics::{CoveragePoint, EthicsReport, EthicsSignals, LayerAOutput, LimitationKind};
    use std::collections::HashMap;

    fn obs(body: &str) -> CognitiveFragment {
        CognitiveFragment::new(
            FragmentContent::Observation {
                body: body.to_string(),
            },
            "test",
        )
    }

    fn cascade_derived(body: &str) -> CognitiveFragment {
        let mut f = obs(body);
        f.provenance.parent_ids = vec![FragmentId::new()];
        f
    }

    fn empty_report(point: CoveragePoint) -> EthicsReport {
        EthicsReport {
            coverage_point: point,
            profile_id: "test".to_string(),
            profile_version: "0.0.1".to_string(),
            classification: LayerAOutput {
                per_category: HashMap::new(),
                flags: Vec::new(),
                disambiguation: hyphae_ethics::DisambiguationVerdict::default(),
            },
            cvar_score: 0.0,
            categorical: None,
            violations: Vec::new(),
            content_fingerprint: String::new(),
            audit_seq: None,
            signals: EthicsSignals::default(),
        }
    }

    #[test]
    fn empty_working_set_fires_the_trigger() {
        let out = evaluate(&[], None, LimitationContext::default());
        assert!(out.contains(&LimitationTrigger::EmptyWorkingSet));
    }

    #[test]
    fn high_confab_risk_fires_when_any_fragment_above_threshold() {
        let mut f = obs("risky");
        f.provenance.confabulation_risk = 0.7;
        let out = evaluate(&[f], None, LimitationContext::default());
        assert!(out.contains(&LimitationTrigger::HighConfabRisk));
    }

    #[test]
    fn low_confab_risk_does_not_fire_high_confab_trigger() {
        let mut f = obs("safe");
        f.provenance.confabulation_risk = 0.1;
        // Make it cascade-derived so the shallow-cascade trigger
        // does not fire either.
        f.provenance.parent_ids = vec![FragmentId::new()];
        let out = evaluate(&[f], None, LimitationContext::default());
        assert!(out.is_empty(), "no triggers should fire on safe input");
    }

    #[test]
    fn shallow_cascade_fires_when_no_cascade_derived_fragments() {
        let out = evaluate(&[obs("direct")], None, LimitationContext::default());
        assert!(out.contains(&LimitationTrigger::ShallowCascade));
    }

    #[test]
    fn shallow_cascade_does_not_fire_with_cascade_derived_fragments() {
        let out = evaluate(
            &[cascade_derived("from cascade")],
            None,
            LimitationContext::default(),
        );
        assert!(!out.contains(&LimitationTrigger::ShallowCascade));
    }

    #[test]
    fn ethically_sensitive_fires_when_ethics_hints_acknowledge() {
        let mut report = empty_report(CoveragePoint::Compose);
        report.signals.composer_should_acknowledge = true;
        report.signals.composer_limitation_kind = Some(LimitationKind::CategoricalConcern);
        let out = evaluate(
            &[cascade_derived("content")],
            Some(&report),
            LimitationContext::default(),
        );
        assert!(out.contains(&LimitationTrigger::EthicallySensitive));
    }

    #[test]
    fn multiple_triggers_fire_simultaneously() {
        let mut risky = obs("risky direct");
        risky.provenance.confabulation_risk = 0.8;
        let mut report = empty_report(CoveragePoint::Compose);
        report.signals.composer_should_acknowledge = true;
        let out = evaluate(&[risky], Some(&report), LimitationContext::default());
        // High confab + shallow cascade + ethically sensitive.
        assert!(out.contains(&LimitationTrigger::HighConfabRisk));
        assert!(out.contains(&LimitationTrigger::ShallowCascade));
        assert!(out.contains(&LimitationTrigger::EthicallySensitive));
        assert!(!out.contains(&LimitationTrigger::EmptyWorkingSet));
    }

    #[test]
    fn acknowledgment_strings_are_non_empty() {
        for trigger in [
            LimitationTrigger::EmptyWorkingSet,
            LimitationTrigger::HighConfabRisk,
            LimitationTrigger::ShallowCascade,
            LimitationTrigger::EthicallySensitive,
        ] {
            assert!(
                !trigger.acknowledgment().is_empty(),
                "trigger {trigger:?} must have an acknowledgment string",
            );
        }
    }
}
