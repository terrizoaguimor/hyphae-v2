// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Test-only embedder that returns a fixed zero vector.
//!
//! Useful when an integration test wants to exercise the
//! substrate's flow without the lexical-hashing variability of the
//! default [`crate::HashingTokenEmbedder`] — e.g. when the test
//! asserts on cascade behaviour and wants the embedding-based
//! ranking to collapse to ties.

use crate::EmbeddingProvider;
use hyphae_core::EMBEDDING_DIM;

/// Returns a zero vector of [`EMBEDDING_DIM`] regardless of input.
#[derive(Debug, Clone, Copy, Default)]
pub struct NullEmbedder;

impl NullEmbedder {
    /// Construct a null embedder.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl EmbeddingProvider for NullEmbedder {
    fn embed(&self, _text: &str) -> Vec<f32> {
        vec![0.0; EMBEDDING_DIM]
    }

    fn dimension(&self) -> usize {
        EMBEDDING_DIM
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_embedder_returns_zero_vector() {
        let e = NullEmbedder::new();
        let v = e.embed("anything");
        assert_eq!(v.len(), EMBEDDING_DIM);
        assert!(v.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn null_embedder_is_input_invariant() {
        let e = NullEmbedder::new();
        let a = e.embed("first");
        let b = e.embed("very different second");
        assert_eq!(a, b);
    }
}
