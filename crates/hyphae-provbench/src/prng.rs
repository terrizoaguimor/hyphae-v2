// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Deterministic, dependency-free PRNG.
//!
//! The whole benchmark is reproducible from `(n, trials, seed)`: no
//! `Math.random`, no system clock, no `rand` crate. Every random
//! choice — corpus content, tamper target, anchor key seed — comes
//! from this stream so two runs with the same parameters produce
//! identical metrics (the envelope is byte-stable; see
//! [`crate::scoring`]).

/// SplitMix64 — a small, well-distributed 64-bit generator.
pub struct SplitMix64(u64);

impl SplitMix64 {
    /// Seed the generator.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// Next 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `[0, n)` (returns 0 when `n == 0`).
    pub fn next_range(&mut self, n: u64) -> u64 {
        if n == 0 {
            return 0;
        }
        self.next_u64() % n
    }
}

/// Derive a fixed 32-byte seed (e.g. for an Ed25519 anchor key) from a
/// scalar seed, deterministically.
#[must_use]
pub fn seed32(seed: u64) -> [u8; 32] {
    let mut r = SplitMix64::new(seed ^ 0xA5A5_5A5A_DEAD_BEEF);
    let mut out = [0u8; 32];
    for chunk in out.chunks_mut(8) {
        let v = r.next_u64().to_le_bytes();
        let len = chunk.len();
        chunk.copy_from_slice(&v[..len]);
    }
    out
}
