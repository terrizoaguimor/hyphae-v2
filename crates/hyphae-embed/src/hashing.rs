// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Feature-hashing token embedder — the v0.1 scaffold default.
//!
//! Pure Rust, zero external dependencies. Deterministic. Captures
//! lexical overlap (and bigram order, for free) without paraphrase
//! invariance or synonym matching. Per ADR-0004, sufficient to
//! validate the substrate's embedding-driven recall path end to
//! end; insufficient for the multilingual / synonym cases that
//! justify the v0.2 transformer upgrade.
//!
//! Algorithm:
//!
//! 1. Lowercase the input and tokenise on non-alphanumeric
//!    characters.
//! 2. Drop a small English stop-word set so common function words
//!    do not dominate the vector.
//! 3. Build the feature stream from token unigrams and bigrams —
//!    `[t0, t1, t2, (t0,t1), (t1,t2)]`. Bigrams give the embedding
//!    weak sensitivity to phrase order (`"big deal"` ≠ `"deal big"`).
//! 4. Feature-hash each feature into one of `EMBEDDING_DIM` buckets
//!    via `FxHash`. Sign is `+1` if the second-bit of the hash is set,
//!    `-1` otherwise (the classical hashing-trick sign bit, to
//!    avoid systematic correlation between collisions).
//! 5. Increment the bucket by a TF-style weight (`1 / sqrt(count)`
//!    per feature occurrence — sub-linear so a flooded token does
//!    not drown the vector).
//! 6. L2-normalise the vector. Cosine similarity now reads cleanly.
//!
//! The resulting vector is unit-norm and `EMBEDDING_DIM`-wide.

use crate::EmbeddingProvider;
use hyphae_core::EMBEDDING_DIM;
use std::collections::HashMap;

/// A short English stop-word list. Mirrors
/// `hyphae_core::StopwordFilteringExtractor`'s set so the keyword
/// extractor and the embedder agree on what counts as content.
const STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "if", "of", "in", "on", "at", "to", "for", "with", "is",
    "are", "was", "were", "be", "been", "being", "this", "that", "these", "those", "i", "you",
    "he", "she", "it", "we", "they", "do", "does", "did", "have", "has", "had", "as", "by", "from",
    "into", "than", "then", "so", "such", "not", "no",
];

/// The v0.1 default embedder.
#[derive(Debug, Clone, Copy, Default)]
pub struct HashingTokenEmbedder;

impl HashingTokenEmbedder {
    /// Construct an embedder.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    fn tokenise(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        for token in text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .map(str::to_lowercase)
        {
            if STOPWORDS.contains(&token.as_str()) {
                continue;
            }
            out.push(token);
        }
        out
    }
}

impl EmbeddingProvider for HashingTokenEmbedder {
    fn embed(&self, text: &str) -> Vec<f32> {
        let mut vec = vec![0.0f32; EMBEDDING_DIM];
        let tokens = Self::tokenise(text);
        if tokens.is_empty() {
            return vec;
        }

        // Count feature occurrences so we can apply sub-linear
        // weighting (`1 / sqrt(count)`).
        let mut counts: HashMap<u64, u32> = HashMap::new();

        // Unigrams.
        for tok in &tokens {
            let h = fx_hash(tok.as_bytes());
            *counts.entry(h).or_insert(0) += 1;
        }
        // Bigrams. Concatenated with a unit separator so
        // `"ab" + "c"` does not collide with `"a" + "bc"`.
        for window in tokens.windows(2) {
            let combined = format!("{}\u{1}{}", window[0], window[1]);
            let h = fx_hash(combined.as_bytes());
            *counts.entry(h).or_insert(0) += 1;
        }

        for (h, count) in counts {
            #[allow(clippy::cast_precision_loss)]
            let weight = 1.0 / (count as f32).sqrt();
            #[allow(clippy::cast_possible_truncation)]
            let bucket = (h as usize) % EMBEDDING_DIM;
            // Sign bit from the next-order bit of the hash so
            // collisions in different signs tend to cancel rather
            // than reinforce.
            let sign = if (h >> 1) & 1 == 0 { 1.0 } else { -1.0 };
            vec[bucket] += weight * sign;
        }

        // L2-normalise.
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut vec {
                *x /= norm;
            }
        }
        vec
    }

    fn dimension(&self) -> usize {
        EMBEDDING_DIM
    }
}

/// `FxHash` — fast, low-quality but well-distributed for our
/// hashing-trick use. Reimplemented inline to avoid a `fxhash` crate
/// dependency for one function.
fn fx_hash(bytes: &[u8]) -> u64 {
    const SEED: u64 = 0xcbf2_9ce4_8422_2325;
    const ROTATE: u32 = 5;
    const MULTIPLIER: u64 = 0x517c_c1b7_2722_0a95;
    let mut hash = SEED;
    for &b in bytes {
        hash = hash.rotate_left(ROTATE) ^ u64::from(b);
        hash = hash.wrapping_mul(MULTIPLIER);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cos(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if na == 0.0 || nb == 0.0 {
            0.0
        } else {
            dot / (na * nb)
        }
    }

    #[test]
    fn embedding_has_correct_dimension() {
        let e = HashingTokenEmbedder::new();
        let v = e.embed("hello world");
        assert_eq!(v.len(), EMBEDDING_DIM);
        assert_eq!(e.dimension(), EMBEDDING_DIM);
    }

    #[test]
    fn embedding_is_unit_normalised_for_non_empty_input() {
        let e = HashingTokenEmbedder::new();
        let v = e.embed("the migration completed at fourteen oh two utc");
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "norm was {norm}");
    }

    #[test]
    fn embedding_is_deterministic() {
        let e = HashingTokenEmbedder::new();
        let v1 = e.embed("rust async runtime questions");
        let v2 = e.embed("rust async runtime questions");
        assert_eq!(v1, v2);
    }

    #[test]
    fn shared_vocabulary_yields_higher_cosine_than_disjoint() {
        let e = HashingTokenEmbedder::new();
        let a = e.embed("the deploy succeeded and traffic ramped smoothly");
        let b = e.embed("the deploy completed successfully and traffic ramped up");
        let c = e.embed("photography landscape sunset mountains far away");
        let ab = cos(&a, &b);
        let ac = cos(&a, &c);
        assert!(
            ab > ac,
            "shared-vocab cosine ({ab}) should beat disjoint-vocab cosine ({ac})",
        );
    }

    #[test]
    fn empty_text_yields_zero_vector() {
        let e = HashingTokenEmbedder::new();
        let v = e.embed("");
        assert!(v.iter().all(|x| (*x - 0.0).abs() < f32::EPSILON));
    }

    #[test]
    fn stopword_only_text_yields_zero_vector() {
        let e = HashingTokenEmbedder::new();
        let v = e.embed("the and or but if");
        assert!(v.iter().all(|x| (*x - 0.0).abs() < f32::EPSILON));
    }

    #[test]
    fn bigram_order_changes_embedding() {
        // "deploy succeeded" should not equal "succeeded deploy"
        // because of the bigram features. (Their unigram hashes
        // collide identically, so the difference rides on the
        // bigram contribution.)
        let e = HashingTokenEmbedder::new();
        let a = e.embed("deploy succeeded");
        let b = e.embed("succeeded deploy");
        let cosab = cos(&a, &b);
        // Strong overlap from the unigrams; bigram contributes
        // some divergence. Cosine should be high but < 1.0.
        assert!(cosab < 0.999, "cosine was {cosab}");
        assert!(cosab > 0.5, "cosine was {cosab}");
    }

    #[test]
    fn case_insensitive() {
        let e = HashingTokenEmbedder::new();
        let a = e.embed("Hyphae Cognitive Substrate");
        let b = e.embed("HYPHAE COGNITIVE SUBSTRATE");
        assert_eq!(a, b);
    }
}
