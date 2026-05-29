// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! The unit under provenance: a stored memory fragment with a verbatim
//! body. A verifiable-generation system answers by emitting these
//! bodies byte-for-byte; the benchmark measures whether tampering with
//! the stored body is detectable and localisable.

use crate::prng::SplitMix64;

/// One stored fragment: an id and its verbatim body.
#[derive(Debug, Clone)]
pub struct Fragment {
    /// Stable identity within the corpus.
    pub id: u64,
    /// The verbatim body bytes — what a verbatim emitter would quote.
    pub body: Vec<u8>,
}

/// A small closed vocabulary; corpus sentences are drawn from it so the
/// content is realistic-looking yet fully determined by the seed.
const WORDS: &[&str] = &[
    "migration",
    "deploy",
    "rollback",
    "auditor",
    "indemnification",
    "clause",
    "latency",
    "percent",
    "quarter",
    "ledger",
    "fragment",
    "anchor",
    "chain",
    "hash",
    "head",
    "verify",
    "tamper",
    "evidence",
    "provenance",
    "quotation",
    "verbatim",
    "journal",
    "sequence",
    "signature",
    "key",
    "store",
    "adversary",
    "detection",
    "localised",
    "survives",
    "termination",
    "completed",
    "succeeded",
    "noted",
    "material",
    "findings",
    "grew",
    "rose",
    "UTC",
    "attempt",
];

/// Build a deterministic corpus of `n` fragments from `seed`.
///
/// The same `(n, seed)` always yields byte-identical fragments, which
/// is what makes the benchmark reproducible across machines.
#[must_use]
pub fn corpus(n: u64, seed: u64) -> Vec<Fragment> {
    let mut r = SplitMix64::new(seed ^ 0x1234_5678_9ABC_DEF0);
    (0..n)
        .map(|id| {
            let word_count = 6 + r.next_range(8) as usize; // 6..=13 words
            let mut body = String::new();
            for i in 0..word_count {
                if i > 0 {
                    body.push(' ');
                }
                body.push_str(WORDS[r.next_range(WORDS.len() as u64) as usize]);
            }
            Fragment {
                id,
                body: body.into_bytes(),
            }
        })
        .collect()
}
