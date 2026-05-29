// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! The standard scoring protocol and the result envelope.
//!
//! Every cell `(system × tamper-mode × adversary)` is scored on five
//! axes over `trials` independent seeds. Rates are in `[0, 1]`; a
//! `-1.0` sentinel marks "not applicable" (e.g. localisation when the
//! tamper is consistent-by-construction, or anchored detection for a
//! system with no head). The envelope embeds only metrics — never
//! timestamps or hashes — so it is byte-stable across runs with the
//! same `(n, trials, seed)`.

use serde::Serialize;

/// Sentinel for a metric that does not apply to a cell.
pub const NA: f64 = -1.0;

/// One scored cell of the matrix.
#[derive(Debug, Clone, Serialize)]
pub struct CellResult {
    /// System name.
    pub system: String,
    /// Tampering mode.
    pub tamper_mode: String,
    /// Adversary profile.
    pub adversary: String,
    /// Corpus size.
    pub n_fragments: u64,
    /// Trials aggregated.
    pub trials: u64,
    /// Whether the tamper applied to this system at all.
    pub applicable: bool,
    /// Fraction of trials the bare chain flagged a violation.
    pub bare_detection_rate: f64,
    /// Of trials with an inconsistent-by-construction tamper that were
    /// detected, the fraction localised to the exact expected seq.
    /// `NA` when no such trials exist.
    pub bare_localisation_rate: f64,
    /// Fraction of trials the external Ed25519 head anchor flagged.
    /// `NA` when the system has no head.
    pub anchored_detection_rate: f64,
    /// False-positive rate: fraction of untampered control trials the
    /// bare chain wrongly flagged. Should be 0.
    pub false_positive_rate: f64,
    /// Mean fraction of entries scanned before the bare chain reported
    /// (a violation seq+1)/n, or 1.0 when undetected. A latency proxy.
    pub mean_scan_fraction: f64,
}

/// The full benchmark result.
#[derive(Debug, Clone, Serialize)]
pub struct Envelope {
    /// Protocol identifier + version.
    pub protocol: String,
    /// Corpus size used.
    pub n_fragments: u64,
    /// Trials per cell.
    pub trials_per_cell: u64,
    /// Base seed (trial `t` uses `seed_base + t`).
    pub seed_base: u64,
    /// All scored cells, in deterministic order.
    pub cells: Vec<CellResult>,
}

/// Accumulator for a single cell across trials. Internal tallies the
/// harness increments per trial before [`CellAcc::finish`].
#[derive(Default)]
#[allow(missing_docs)]
pub struct CellAcc {
    pub trials: u64,
    pub applicable: bool,
    pub bare_detected: u64,
    pub localised_correct: u64,
    pub localisation_denom: u64,
    pub anchored_detected: u64,
    pub anchored_denom: u64,
    pub false_positives: u64,
    pub scan_fraction_sum: f64,
}

impl CellAcc {
    /// Finalise into a [`CellResult`].
    #[must_use]
    pub fn finish(&self, system: &str, tamper_mode: &str, adversary: &str, n: u64) -> CellResult {
        let trials = self.trials.max(1);
        CellResult {
            system: system.to_string(),
            tamper_mode: tamper_mode.to_string(),
            adversary: adversary.to_string(),
            n_fragments: n,
            trials: self.trials,
            applicable: self.applicable,
            bare_detection_rate: self.bare_detected as f64 / trials as f64,
            bare_localisation_rate: if self.localisation_denom > 0 {
                self.localised_correct as f64 / self.localisation_denom as f64
            } else {
                NA
            },
            anchored_detection_rate: if self.anchored_denom > 0 {
                self.anchored_detected as f64 / self.anchored_denom as f64
            } else {
                NA
            },
            false_positive_rate: self.false_positives as f64 / trials as f64,
            mean_scan_fraction: self.scan_fraction_sum / trials as f64,
        }
    }
}
