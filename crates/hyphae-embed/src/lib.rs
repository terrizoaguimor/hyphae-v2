// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! # hyphae-embed
//!
//! Embedding provider for the Hyphae v2 substrate — per ADR-0004.
//!
//! Defines the [`EmbeddingProvider`] trait every embedder
//! implements, ships [`HashingTokenEmbedder`] as the zero-dependency
//! v0.1 default, and provides [`NullEmbedder`] for tests. The
//! transformer-based upgrade (`BGE-M3` / `MiniLM` via ONNX or
//! equivalent) is a v0.2 ADR — additive, not breaking, the same
//! trait.
//!
//! ## Where embedding fits
//!
//! Embedding is **not** generation — it projects text into a
//! fixed-width vector, never produces text. It belongs in the
//! substrate's *infrastructure path*, not the *cognition path*.
//! Per ADR-0001 §"Hard Commitment 1", the cognition path excludes
//! LLM **generation**; the encoder used here is categorically a
//! representation function, not a generator.
//!
//! ## Where it runs in the substrate
//!
//! Two coverage points per ADR-0004 §"Where the embedder runs in
//! the flow":
//!
//! 1. [`hyphae_substrate::Substrate::ingest`] embeds the input
//!    fragment **before** routing so storage in `episodic` is
//!    semantically searchable.
//! 2. [`hyphae_substrate::Substrate::recall_signal`] embeds the
//!    cue **before** handing it to `episodic` so
//!    `pattern_complete` ranks by cosine similarity rather than
//!    falling back to insertion-order recency.

#![warn(missing_docs)]
#![warn(clippy::pedantic)]

pub mod hashing;
pub mod null;

pub use hashing::HashingTokenEmbedder;
pub use null::NullEmbedder;

/// An embedding provider. Implementations project text into a
/// fixed-width unit-normalised vector.
///
/// Implementations MUST be deterministic for a given input — the
/// substrate's cascade activation and the eval harness's recall
/// scorer both depend on reproducibility. A future async
/// (remote-API) provider can layer an async batch method without
/// breaking the sync surface.
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a single text into a fixed-width unit-normalised
    /// vector. Empty strings (or strings whose tokens are all
    /// filtered out) return a zero vector, **not** an error.
    fn embed(&self, text: &str) -> Vec<f32>;

    /// The dimension of vectors this provider emits. Must equal
    /// [`hyphae_core::EMBEDDING_DIM`] for substrate use.
    fn dimension(&self) -> usize;
}
