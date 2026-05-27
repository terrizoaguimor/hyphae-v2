// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Schema definitions — what shape a composition takes.
//!
//! Per `docs/rfc/v1-living.md` §5.1, v0.1 ships **two schemas only**:
//!
//! - [`SchemaId::DialogueReply`] — conversational response. Slots:
//!   opening line, body fragments quoted in order, closing line,
//!   optional limitation acknowledgments.
//! - [`SchemaId::GroundedAssertion`] — declarative statement
//!   anchored to retrieved fragments. Slots: optional attribution
//!   prefix, claim quoted from fragment(s), supporting fragments,
//!   optional limitation acknowledgments.
//!
//! Postponed schemas (RFC §9): `IntrospectiveAssessment`,
//! `NarrativeArc`, `ComparativeAnalysis`, `SyntheticSummary`. Each
//! re-enters with an explicit ADR demonstrating empirical need.

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
}

impl SchemaId {
    /// Stable lowercase tag for audit-body grepability.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::DialogueReply => "dialogue_reply",
            Self::GroundedAssertion => "grounded_assertion",
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
    fn schema_id_tags_are_distinct_lowercase() {
        assert_eq!(SchemaId::DialogueReply.tag(), "dialogue_reply");
        assert_eq!(SchemaId::GroundedAssertion.tag(), "grounded_assertion");
    }
}
