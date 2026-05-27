// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Schema definitions — what shape a composition takes.
//!
//! Per `docs/rfc/v1-living.md` §5.1, v0.1 shipped two schemas;
//! ADR-0016 + ADR-0023 added two more in v0.2:
//!
//! - [`SchemaId::DialogueReply`] — conversational response. Slots:
//!   opening line, body fragments quoted in order, closing line,
//!   optional limitation acknowledgments.
//! - [`SchemaId::GroundedAssertion`] — declarative statement
//!   anchored to retrieved fragments. Slots: optional attribution
//!   prefix, claim quoted from fragment(s), supporting fragments,
//!   optional limitation acknowledgments.
//! - [`SchemaId::Summary`] — multi-fragment synthesis. Same slot
//!   shape as `DialogueReply` except the closing line pulls from
//!   `ConnectiveRole::Summary` ("Overall,", "On balance,", "Taking
//!   it together,", …) — see ADR-0016.
//! - [`SchemaId::ComparativeAnalysis`] — comparative-judgment shape.
//!   Inter-fragment slots use `ConnectiveRole::Contrast` regardless
//!   of cascade-shape projection; closing slot shares the Summary
//!   role with [`SchemaId::Summary`] — see ADR-0023.
//!
//! Still postponed (RFC §9): `IntrospectiveAssessment`,
//! `NarrativeArc`. Each re-enters with an explicit ADR
//! demonstrating empirical need.

use serde::{Deserialize, Serialize};

/// Discriminator for the available v0.1 schemas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SchemaId {
    /// Conversational reply.
    DialogueReply,
    /// Declarative statement anchored to retrieved fragments.
    GroundedAssertion,
    /// **ADR-0016.** Multi-fragment synthesis. Same slot shape as
    /// `DialogueReply` with the closing line drawn from
    /// `ConnectiveRole::Summary` instead of
    /// `ConnectiveRole::Closing`. Best when the working set has
    /// three or more fragments; the realizer emits it verbatim
    /// regardless of size (no silent downgrade) per ADR-0016
    /// §"Small working set behaviour".
    Summary,
    /// **ADR-0023.** Comparative judgment shape. Inter-fragment
    /// connectives are FORCED to `ConnectiveRole::Contrast`
    /// regardless of the cascade-shape projection's suggestion;
    /// the closing slot draws from `ConnectiveRole::Summary` (the
    /// comparative synthesis line). For 1-fragment working sets
    /// the schema falls through to DialogueReply-like behaviour
    /// (no contrast to apply).
    ComparativeAnalysis,
}

impl SchemaId {
    /// Stable lowercase tag for audit-body grepability.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::DialogueReply => "dialogue_reply",
            Self::GroundedAssertion => "grounded_assertion",
            Self::Summary => "summary",
            Self::ComparativeAnalysis => "comparative_analysis",
        }
    }
}

/// What the caller's intent is. The realizer maps intents to
/// schemas; today the mapping is straightforward (dialogue intents
/// map to `DialogueReply`, assertion intents map to
/// `GroundedAssertion`). The indirection exists so a future
/// schema-selection learning loop (per ADR-0002's refinable
/// `ComposerSchemaPrior`) can re-route intents to schemas based on
/// observed utility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Intent {
    /// The caller is engaged in dialogue with the system.
    Dialogue,
    /// The caller wants a declarative statement.
    Assert,
    /// **ADR-0016.** The caller wants a multi-fragment synthesis
    /// with a summary-shaped closing line.
    Summarize,
    /// **ADR-0023.** The caller wants a comparative analysis —
    /// fragments interpreted as paired/contrasting positions with
    /// a comparative-judgment closing.
    Compare,
}

impl Intent {
    /// Default schema for this intent. The
    /// [`ComposerSchemaPrior`](hyphae_substrate::LearningTarget) is
    /// the learning surface that can refine this mapping in
    /// future versions.
    #[must_use]
    pub fn default_schema(self) -> SchemaId {
        match self {
            Self::Dialogue => SchemaId::DialogueReply,
            Self::Assert => SchemaId::GroundedAssertion,
            Self::Summarize => SchemaId::Summary,
            Self::Compare => SchemaId::ComparativeAnalysis,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_dialogue_maps_to_dialogue_reply() {
        assert_eq!(Intent::Dialogue.default_schema(), SchemaId::DialogueReply);
    }

    #[test]
    fn intent_assert_maps_to_grounded_assertion() {
        assert_eq!(Intent::Assert.default_schema(), SchemaId::GroundedAssertion);
    }

    #[test]
    fn intent_summarize_maps_to_summary() {
        assert_eq!(Intent::Summarize.default_schema(), SchemaId::Summary);
    }

    #[test]
    fn intent_compare_maps_to_comparative_analysis() {
        assert_eq!(
            Intent::Compare.default_schema(),
            SchemaId::ComparativeAnalysis
        );
    }

    #[test]
    fn schema_id_tags_are_distinct_lowercase() {
        assert_eq!(SchemaId::DialogueReply.tag(), "dialogue_reply");
        assert_eq!(SchemaId::GroundedAssertion.tag(), "grounded_assertion");
        assert_eq!(SchemaId::Summary.tag(), "summary");
        assert_eq!(SchemaId::ComparativeAnalysis.tag(), "comparative_analysis");
    }
}
