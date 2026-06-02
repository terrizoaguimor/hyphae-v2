// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! The system-under-test interface.
//!
//! Any verifiable-generation system plugs into the benchmark by
//! implementing [`ProvenanceSystem`]. The benchmark is therefore
//! realizer-independent: it does not know or care whether the bodies
//! were selected by Hyphae's cascade, by a trivial `echo`, or by a
//! third party's retriever. It measures one thing — does the system's
//! storage layer make post-hoc tampering detectable and localisable —
//! the axis on which verifiable-generation systems actually differ.

use std::path::Path;

use crate::fragment::Fragment;
use crate::tamper::TamperMode;

/// The verdict a system returns from re-deriving its integrity check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// No tampering detected.
    Clean,
    /// Tampering detected; `seq` is the localised position (the first
    /// broken link).
    Violation {
        /// The sequence number reported as the integrity violation.
        seq: u64,
    },
}

/// Ground truth recorded when a tamper is applied, used to score
/// detection and localisation against what the system reports.
#[derive(Debug, Clone, Copy)]
pub struct GroundTruth {
    /// The sequence a correct localiser should report as the first
    /// broken link — when the tamper leaves the chain *inconsistent*.
    ///
    /// `None` means the tamper produced an internally consistent
    /// chain (a chain-aware recompute, or a head rollback). Such a
    /// tamper is undetectable by the bare chain *by construction*; it
    /// is expected to be caught only by an external head anchor. A
    /// bare-chain "miss" on a `None` tamper is therefore correct
    /// behaviour, not a failure — the benchmark scores it accordingly.
    pub expected_break_seq: Option<u64>,
    /// The store's head *after* the tamper, computed in memory by the
    /// tamper itself so the harness need not reopen the store. `None`
    /// for systems with no head. The external anchor (signed over the
    /// pre-tamper head) catches the tamper exactly when this differs
    /// from the anchored head.
    pub head_after: Option<[u8; 32]>,
}

/// A system under test. Implementations are stateless; each call is
/// parameterised by the on-disk store directory `dir` so the harness
/// can build a fresh store per trial.
pub trait ProvenanceSystem {
    /// Human-readable name (appears in the envelope and table).
    fn name(&self) -> &'static str;

    /// Build the store at `dir` from `fragments` (verbatim bodies).
    /// Returns the head after ingest (so the harness can anchor it
    /// without reopening), or `None` for systems with no head.
    fn ingest(&self, dir: &Path, fragments: &[Fragment]) -> Option<[u8; 32]>;

    /// Re-derive the integrity verdict from the store.
    fn verify(&self, dir: &Path) -> VerifyOutcome;

    /// The store's current head, if it maintains one. `None` for
    /// systems with no chain (the head is what an external anchor
    /// signs).
    fn head(&self, dir: &Path) -> Option<[u8; 32]>;

    /// Apply `mode` at fragment index `target` (`n` = corpus size).
    /// `chain_aware` selects whether the adversary recomputes the
    /// chain forward and rewrites the head.
    ///
    /// Returns the [`GroundTruth`], or `None` if the system has no
    /// structure this tamper could target (the cell is then N/A).
    fn tamper(
        &self,
        dir: &Path,
        mode: TamperMode,
        target: u64,
        n: u64,
        chain_aware: bool,
    ) -> Option<GroundTruth>;

    /// Number of hashes an auditor must obtain — beyond the trusted head
    /// — to prove that a single stored entry is included in, and
    /// consistent with, the committed head, *without streaming the whole
    /// log*. This is the axis on which append-only-log designs differ
    /// even when their detection profiles match: a flat hash chain needs
    /// `O(n)` (rehash to the head), a Merkle log `O(log n)` (an audit
    /// path). `None` means the system commits to no membership at all
    /// (nothing to prove inclusion against). Defaults to `None`.
    fn inclusion_proof_hashes(&self, _n: u64) -> Option<u64> {
        None
    }
}

/// `ceil(log2(n))` — the audit-path length for a Merkle log of `n`
/// leaves (0 for a single leaf).
#[must_use]
pub fn ceil_log2(n: u64) -> u64 {
    if n <= 1 {
        0
    } else {
        64 - (n - 1).leading_zeros() as u64
    }
}
