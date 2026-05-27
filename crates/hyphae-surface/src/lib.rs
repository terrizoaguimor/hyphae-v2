// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! # hyphae-surface
//!
//! Surface realizer for Hyphae v2. The realizer is the only place
//! that produces text — every other crate operates on structured
//! fragments and types.
//!
//! Per `docs/rfc/v1-living.md` §5:
//!
//! - **Two schemas in v0.1**: [`SchemaId::DialogueReply`] and
//!   [`SchemaId::GroundedAssertion`]. The other v1 schemas
//!   (`IntrospectiveAssessment`, `NarrativeArc`,
//!   `ComparativeAnalysis`, `SyntheticSummary`) are postponed per
//!   ADR-0001 §"Surface scope".
//! - **Composition by fragment quotation + connective tissue.**
//!   The realizer NEVER paraphrases a fragment body. Quotes appear
//!   verbatim; the realizer only emits the structural prose around
//!   them. This is the architectural boundary the
//!   no-LLM-in-cognition-path commitment depends on.
//! - **Honest limitation triggers** (`EmptyWorkingSet`,
//!   `HighConfabRisk`, `ShallowCascade`, `EthicallySensitive`)
//!   surface in every composition that meets their criteria. The
//!   realizer never confabulates from nothing — it acknowledges.
//! - **English only** for v0.1 per RFC §9 negative scope. The
//!   connective lexicon and limitation acknowledgment strings are
//!   EN; additional languages re-enter additively with a lexicon-
//!   expansion ADR.

#![warn(missing_docs)]
#![warn(clippy::pedantic)]

pub mod composition_shape;
pub mod connective;
pub mod connective_data;
pub mod limitation;
pub mod realizer;
pub mod schema;

pub use composition_shape::{
    CompositionShape, CompositionStep, shape_from_cascade, shape_from_retrieval,
    shape_from_working_set, working_set_of,
};
pub use connective::{
    Connective, ConnectiveRole, Formality, Lexicon, PickContext, Polarity, Register,
};
pub use limitation::{
    HIGH_CONFAB_RISK_THRESHOLD, LimitationContext, LimitationTrigger,
    evaluate as evaluate_limitations,
};
pub use realizer::{RealizationError, RealizationOutput, RealizationRequest, SurfaceRealizer};
pub use schema::{Intent, SchemaId};
