// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Cognitive fragments — the atomic unit of cognitive content in
//! Hyphae v2.
//!
//! Cherry-picked from v1 with two corrections:
//! - `LanguageTag` simplified to `English | Other(String)`. v0.1 is
//!   English-only per RFC §9; the `Other` variant accommodates a
//!   third language landing post-validation without the type
//!   evolving (re-introducing `Spanish` first-class is additive, not
//!   a refactor).
//! - `Provenance.language_detection_confidence` removed. The
//!   citation-engine path for which it was added is `deferred` per
//!   RFC §9 negative scope; the field re-enters with the grounding
//!   ADR.

use crate::FragmentId;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// The atomic unit of cognitive content in Hyphae.
///
/// A `CognitiveFragment` is a discriminated union representing one of
/// the fundamental cognitive content types: episodes, beliefs, goals,
/// observations, reflections, or references. Every fragment carries
/// provenance, saliency, decay parameters, and an optional embedding.
///
/// The fragment is the unit that flows through pathways between
/// subsystems. Subsystems may consume fragments, emit new fragments,
/// or modulate the processing of fragments by other subsystems.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveFragment {
    /// Unique identifier for this fragment.
    pub id: FragmentId,
    /// The content of the fragment, discriminated by kind.
    pub content: FragmentContent,
    /// When this fragment was created.
    pub created_at: SystemTime,
    /// When this fragment was last accessed (for decay computation).
    pub last_accessed_at: SystemTime,
    /// Saliency score in [0.0, 1.0]. Modulated by `valence` and
    /// learning-loop salience weights.
    pub saliency: f32,
    /// Affective valence in [-1.0, 1.0]. Assigned by `valence`.
    pub valence: f32,
    /// Decay rate per second. Higher = faster forgetting. Modulated
    /// by `valence` for sustained context.
    pub decay_rate: f32,
    /// Confidence score in [0.0, 1.0]. Used by `composer` for
    /// conflict monitoring.
    pub confidence: f32,
    /// Provenance metadata: where this fragment came from.
    pub provenance: Provenance,
    /// Optional dense embedding for vector retrieval (dimension =
    /// [`crate::EMBEDDING_DIM`]).
    pub embedding: Option<Vec<f32>>,
    /// Causal depth in the knowledge graph. A fragment is at depth
    /// `N` when its causal dependencies (parents + supports + the
    /// chain of fragments that ground its assertions) all sit at
    /// depths `≤ N`. Foundational fragments are level 1; substrate-
    /// built assertions are level 3; specialised research depth is
    /// level 5+. Initial assignment is heuristic from the source;
    /// recomputed during Consolidation as the causal network evolves.
    #[serde(default = "default_depth_level")]
    pub depth_level: u8,
    /// Semantic domain tags assigned at encoding time based on
    /// content clustering. Multiple tags permitted (a fragment may
    /// straddle "biology" and "computing"). Empty `Vec` means
    /// untagged — the auto-detector produced no signal strong enough
    /// to assert.
    #[serde(default)]
    pub domain_tags: Vec<String>,
    /// Language of the fragment's content. v0.1 defaults to
    /// [`LanguageTag::English`]; the `Other` variant accommodates
    /// any future-language extension. Cross-lingual knowledge
    /// transfer is mediated by the lexicon's conceptual mapping
    /// (when introduced), not by auto-translation of fragments.
    #[serde(default)]
    pub language: LanguageTag,
    /// Boundary metadata the surface realizer uses to enforce
    /// concordance at the boundary between connective tissue and
    /// the quoted fragment body. `None` when the encoder cannot
    /// determine the metadata with high confidence. The realizer
    /// falls back to neutral connective forms in that case.
    ///
    /// Conservative by design: preferring `None` over a wrong guess
    /// avoids agreement errors visible to readers.
    #[serde(default)]
    pub boundary_metadata: Option<BoundaryMetadata>,
}

/// Surface-level metadata about a fragment's body that the realizer
/// consults when emitting connective tissue adjacent to that
/// fragment.
///
/// Populated at encoding time via a conservative heuristic: when the
/// encoder cannot determine gender / number with high confidence, the
/// metadata is left `None` on the fragment and the realizer degrades
/// to neutral connective forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BoundaryMetadata {
    /// Gender of the fragment's head noun.
    pub head_gender: Gender,
    /// Number of the fragment's head noun.
    pub head_number: Number,
    /// `true` when the fragment body starts with a definite /
    /// indefinite determiner. The realizer uses this to avoid
    /// emitting a redundant connective determiner immediately before
    /// the fragment.
    pub initial_determiner: bool,
    /// Part-of-speech of the fragment's first content word.
    pub initial_pos: PoS,
}

/// Grammatical gender for [`BoundaryMetadata`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gender {
    /// Masculine.
    Masculine,
    /// Feminine.
    Feminine,
    /// Neuter — applies to abstract concepts and to nouns without
    /// inherent gender when the realizer does not need a binary call.
    Neuter,
}

/// Grammatical number for [`BoundaryMetadata`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Number {
    /// Singular.
    Singular,
    /// Plural.
    Plural,
}

/// Coarse part-of-speech tag the surface realizer uses for boundary
/// decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoS {
    /// A common or proper noun.
    Noun,
    /// A finite or non-finite verb.
    Verb,
    /// An adjective.
    Adjective,
    /// A determiner (article, demonstrative, possessive).
    Determiner,
    /// Anything else the heuristic could not classify confidently.
    Other,
}

/// Language tag for fragments and lexicon entries.
///
/// v0.1 is English-first (RFC §9). The `Other` variant lets the type
/// accommodate any additional language without a refactor — the
/// substrate does not validate the string; the lexicon module
/// decides whether it has rules for that language. **Unknown**
/// content (no detection signal) uses `Other("unknown")` by
/// convention.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguageTag {
    /// English. Default for v0.1.
    English,
    /// Any other language, named by its ISO 639-1 or 639-3 code
    /// (lowercase). The lexicon module decides whether it has rules
    /// for that language.
    Other(String),
}

impl Default for LanguageTag {
    /// Defaults to English (v0.1 is English-only per RFC §9).
    fn default() -> Self {
        Self::English
    }
}

impl LanguageTag {
    /// Stable lowercase tag for audit-body grepability and serde
    /// round-trips.
    #[must_use]
    pub fn tag(&self) -> String {
        match self {
            Self::English => "english".to_string(),
            Self::Other(code) => format!("other:{code}"),
        }
    }
}

/// Default for `CognitiveFragment::depth_level`. `1` is the
/// foundational level — fragments without an explicit depth
/// assignment are assumed to be foundational rather than derived,
/// matching `serde(default)` defensive deserialisation. The encoding
/// pipeline overrides this with a heuristic before storing.
#[must_use]
const fn default_depth_level() -> u8 {
    1
}

/// The discriminated content of a cognitive fragment.
///
/// Uses serde's default externally-tagged representation so the type
/// round-trips through `bincode`, the project's binary serialiser.
/// Internally-tagged enums require `deserialize_any`, which `bincode`
/// — being non-self-describing — does not support.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FragmentContent {
    /// A bound episode with spatial-temporal-contextual indices.
    Episode {
        /// The episodic content as a textual or structured
        /// representation.
        body: String,
        /// References to fragments that are part of this episode.
        bindings: Vec<crate::FragmentId>,
    },
    /// A belief or semantic abstraction consolidated from episodes.
    Belief {
        /// The belief content.
        body: String,
        /// Fragments that support this belief.
        supports: Vec<crate::FragmentId>,
    },
    /// An active goal driving current processing.
    Goal {
        /// The goal description.
        body: String,
        /// Sub-goals or related goals.
        related: Vec<crate::FragmentId>,
    },
    /// A raw observation, pre-encoding.
    Observation {
        /// The observation content.
        body: String,
    },
    /// A reflection or meta-cognitive note.
    Reflection {
        /// The reflection content.
        body: String,
        /// Fragments this reflection is about.
        about: Vec<crate::FragmentId>,
    },
    /// A reference to external content.
    Reference {
        /// The reference URI or identifier.
        uri: String,
    },
    /// A journal entry — a specialised fragment type that mirrors an
    /// entry the substrate appended to the SHA-256 hash chain.
    /// Storing the journal entry as a `CognitiveFragment` (in
    /// addition to the chain) lets `episodic` index it for semantic
    /// recall — `journal_recall` is then a thin wrapper over the
    /// recall query with a journal type filter.
    Journal {
        /// The entry payload.
        body: String,
        /// Discriminated entry kind.
        entry_type: JournalEntryType,
        /// The 1-based sequence number of the entry in the SHA
        /// chain.
        sequence: u64,
        /// The SHA-256 content hash of the entry, identical to the
        /// value stored on the chain. Lets a `journal_recall`
        /// consumer correlate the retrieved fragment with the
        /// on-chain entry.
        hash: [u8; 32],
    },
}

/// Discriminated kind of a journal entry.
///
/// The first six variants are the reflective taxonomy a model uses
/// to keep its own journal. The last four are audit categories that
/// substrate operations emit so an auditor can replay
/// who-did-what-when from the SHA chain — including the learning
/// loop's parameter-update audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalEntryType {
    /// A committed decision.
    Decision,
    /// A reflection or meta-cognitive note.
    Reflection,
    /// A lesson learned from a failure or success.
    Lesson,
    /// A belief held — a semantic stance.
    Belief,
    /// An arc — a thread of related entries over time.
    Arc,
    /// A doubt — an open question or design uncertainty.
    Doubt,
    /// Audit: a memory operation.
    AuditMemoryOp,
    /// Audit: a journal operation.
    AuditJournalOp,
    /// Audit: an entry redaction.
    AuditRedaction,
    /// Audit: an entry supersession.
    AuditSupersession,
    /// Audit: an ethics evaluation outcome (RADAR report).
    AuditEthicsEvaluation,
    /// Audit: a learning-loop parameter update.
    AuditLearningUpdate,
}

/// Provenance metadata: where a fragment came from.
///
/// Populated by the producer subsystem; never default-zero silently.
/// `confabulation_risk` follows the discipline documented in
/// `docs/rfc/v1-living.md` §1.3:
/// - Measurement emitters → `0.0`.
/// - Single-input transformers → propagate the source's risk.
/// - Passthroughs → leave the input fragment's risk unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    /// The subsystem that produced this fragment.
    pub source_subsystem: String,
    /// The pathway it traveled, if applicable.
    pub source_pathway: Option<String>,
    /// Fragments that contributed to this fragment.
    pub parent_ids: Vec<crate::FragmentId>,
    /// Was this fragment confabulated (low confidence)?
    pub confabulation_risk: f32,
    /// Optional namespace the fragment belongs to. The operation
    /// layer uses this as a recall filter so a caller can scope a
    /// recall (e.g. `namespace = "project/hyphae-v2"`) without
    /// baking multi-tenancy into the substrate. `None` means the
    /// fragment is in the default namespace.
    #[serde(default)]
    pub namespace: Option<String>,
}

impl CognitiveFragment {
    /// Create a new fragment with default decay and confidence.
    ///
    /// Defaults: `depth_level = 1` (foundational — overridden by
    /// encoding heuristics if the caller knows better),
    /// `domain_tags = []` (untagged), `language = English`
    /// (v0.1 default per RFC §9), `boundary_metadata = None`.
    #[must_use]
    pub fn new(content: FragmentContent, source_subsystem: impl Into<String>) -> Self {
        let now = SystemTime::now();
        Self {
            id: FragmentId::new(),
            content,
            created_at: now,
            last_accessed_at: now,
            saliency: 0.5,
            valence: 0.0,
            decay_rate: 0.001,
            confidence: 1.0,
            provenance: Provenance {
                source_subsystem: source_subsystem.into(),
                source_pathway: None,
                parent_ids: Vec::new(),
                confabulation_risk: 0.0,
                namespace: None,
            },
            embedding: None,
            depth_level: default_depth_level(),
            domain_tags: Vec::new(),
            language: LanguageTag::default(),
            boundary_metadata: None,
        }
    }

    /// Set the causal depth level. Builder-style; returns `self` for
    /// chaining at fragment-construction sites.
    #[must_use]
    pub const fn with_depth_level(mut self, depth: u8) -> Self {
        self.depth_level = depth;
        self
    }

    /// Add a semantic domain tag. Repeated calls accumulate.
    #[must_use]
    pub fn with_domain_tag(mut self, tag: impl Into<String>) -> Self {
        self.domain_tags.push(tag.into());
        self
    }

    /// Set the language tag.
    #[must_use]
    pub fn with_language(mut self, language: LanguageTag) -> Self {
        self.language = language;
        self
    }

    /// Set the boundary metadata. Conservative builder: use `None`
    /// (default) when the encoder cannot determine head gender /
    /// number with high confidence. Setting wrong metadata is worse
    /// than leaving it `None` because the realizer's fallback path
    /// emits gender / number-neutral connective tissue.
    #[must_use]
    pub const fn with_boundary_metadata(mut self, metadata: BoundaryMetadata) -> Self {
        self.boundary_metadata = Some(metadata);
        self
    }
}

/// Raw input arriving at the External Interface, before any encoding.
///
/// The substrate wraps an `ExternalInputPayload` into a
/// [`CognitiveFragment`] (specifically a
/// [`FragmentContent::Observation`]) at the `External → input-gate`
/// boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalInputPayload {
    /// The raw input content.
    pub content: String,
    /// Saliency in `[0.0, 1.0]` to associate with the resulting
    /// fragment.
    pub saliency: f32,
}

impl ExternalInputPayload {
    /// Create a new external input payload with the default saliency
    /// of `0.5`.
    #[must_use]
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            saliency: 0.5,
        }
    }

    /// Associate an explicit saliency with this input.
    #[must_use]
    pub fn with_saliency(mut self, saliency: f32) -> Self {
        self.saliency = saliency;
        self
    }
}
