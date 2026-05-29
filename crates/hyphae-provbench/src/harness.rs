// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! The benchmark driver: runs the full `(system × mode × adversary)`
//! matrix over `trials` seeded trials and aggregates into an
//! [`Envelope`]. Also renders the deterministic human-readable table.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hyphae_storage::{HeadAnchor, verify_anchored_head};

use crate::PROTOCOL_VERSION;
use crate::adversary::{Adversary, profiles};
use crate::fragment::corpus;
use crate::prng::{SplitMix64, seed32};
use crate::scoring::{CellAcc, CellResult, Envelope};
use crate::system::{ProvenanceSystem, VerifyOutcome};
use crate::systems::{EchoNoJournal, VerbatimJournal};
use crate::tamper::TamperMode;

/// Minimum corpus size: we tamper at an interior target in `[1, n-2]`,
/// so the smallest meaningful corpus has a head, an interior, and a
/// tail.
pub const MIN_N: u64 = 8;

/// Run the full matrix. Deterministic in `(n, trials, seed_base)`.
#[must_use]
pub fn run(n: u64, trials: u64, seed_base: u64) -> Envelope {
    let n = n.max(MIN_N);
    let systems: Vec<Box<dyn ProvenanceSystem>> =
        vec![Box::new(VerbatimJournal), Box::new(EchoNoJournal)];
    let advs = profiles();
    let modes = TamperMode::all();

    let mut cells = Vec::new();
    for sys in &systems {
        // False positives depend only on (system, corpus), not on the
        // tamper cell — measure once per system and reuse across cells.
        let fp_rate = false_positive_rate(sys.as_ref(), n, trials, seed_base);
        for &mode in &modes {
            for adv in &advs {
                cells.push(run_cell(
                    sys.as_ref(),
                    mode,
                    adv,
                    n,
                    trials,
                    seed_base,
                    fp_rate,
                ));
            }
        }
    }

    Envelope {
        protocol: PROTOCOL_VERSION.to_string(),
        n_fragments: n,
        trials_per_cell: trials,
        seed_base,
        cells,
    }
}

/// Process-global counter so concurrent trials (e.g. parallel tests)
/// never share an on-disk store directory.
static UNIQ: AtomicU64 = AtomicU64::new(0);

fn tmpdir(sys: &str, mode: TamperMode, adv: &str, seed: u64, tag: &str) -> PathBuf {
    let u = UNIQ.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!(
        "provbench-{pid}-{u}-{sys}-{}-{adv}-{seed}-{tag}",
        mode.name()
    ))
}

/// Fraction of pristine (untampered) stores the bare check wrongly
/// flags. Depends only on `(system, corpus)`, so it is measured once
/// per system and shared across that system's cells.
fn false_positive_rate(sys: &dyn ProvenanceSystem, n: u64, trials: u64, seed_base: u64) -> f64 {
    let mut fp = 0u64;
    for t in 0..trials {
        let seed = seed_base.wrapping_add(t);
        let dir = tmpdir(sys.name(), TamperMode::Edit, "fp-control", seed, "ctrl");
        let _ = std::fs::remove_dir_all(&dir);
        sys.ingest(&dir, &corpus(n, seed));
        if let VerifyOutcome::Violation { .. } = sys.verify(&dir) {
            fp += 1;
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
    fp as f64 / trials.max(1) as f64
}

fn run_cell(
    sys: &dyn ProvenanceSystem,
    mode: TamperMode,
    adv: &Adversary,
    n: u64,
    trials: u64,
    seed_base: u64,
    fp_rate: f64,
) -> CellResult {
    let mut acc = CellAcc {
        trials,
        ..Default::default()
    };

    for t in 0..trials {
        let seed = seed_base.wrapping_add(t);
        let frags = corpus(n, seed);

        // ── Main trial: ingest (returns head) → anchor → tamper
        //    (returns head_after) → verify. Three keyspace opens. ──
        let dir = tmpdir(sys.name(), mode, adv.name, seed, "main");
        let _ = std::fs::remove_dir_all(&dir);
        let head_before = sys.ingest(&dir, &frags);

        // Anchor the legitimate head before the attack (key held
        // outside the store).
        let anchor = HeadAnchor::from_seed(&seed32(seed));
        let vk = anchor.verifying_key();
        let anchored = head_before.map(|h| anchor.anchor(h));

        // Interior target in [1, n-2].
        let mut r = SplitMix64::new(seed ^ mode.index().wrapping_mul(0x9E37_79B9));
        let target = 1 + r.next_range((n - 2).max(1));

        if let Some(gt) = sys.tamper(&dir, mode, target, n, adv.chain_aware()) {
            acc.applicable = true;
            let (bare_detected, bare_seq) = match sys.verify(&dir) {
                VerifyOutcome::Violation { seq } => (true, Some(seq)),
                VerifyOutcome::Clean => (false, None),
            };
            if bare_detected {
                acc.bare_detected += 1;
            }
            // Localisation only scores inconsistent-by-construction
            // tampers (expected_break_seq = Some).
            if let Some(expected) = gt.expected_break_seq {
                acc.localisation_denom += 1;
                if bare_detected && bare_seq == Some(expected) {
                    acc.localised_correct += 1;
                }
            }
            // Latency proxy.
            acc.scan_fraction_sum += if bare_detected {
                (bare_seq.unwrap() + 1) as f64 / n as f64
            } else {
                1.0
            };

            // Anchored detection (only when the system has a head).
            if let (Some(anc), Some(head_after)) = (anchored, gt.head_after) {
                acc.anchored_denom += 1;
                let anchor_ok = if adv.holds_key() {
                    // Compromised key: the attacker re-signs the forged
                    // head, so the anchor verifies and provides no
                    // protection — the guarantee's boundary.
                    true
                } else {
                    verify_anchored_head(&head_after, &anc, &vk)
                };
                if !anchor_ok {
                    acc.anchored_detected += 1;
                }
            }
        } else {
            acc.scan_fraction_sum += 1.0;
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    let mut cell = acc.finish(sys.name(), mode.name(), adv.name, n);
    cell.false_positive_rate = fp_rate;
    cell
}

// ── Deterministic ASCII rendering ──

fn pct(v: f64) -> String {
    if v < 0.0 {
        "  n/a".to_string()
    } else {
        format!("{:>4.0}%", v * 100.0)
    }
}

/// Render the envelope as a fixed-width report, grouped by system.
#[must_use]
pub fn render_table(env: &Envelope) -> String {
    let mut out = String::new();
    out.push_str("# Provenance benchmark — ");
    out.push_str(&env.protocol);
    out.push('\n');
    out.push_str(&format!(
        "# Realizer-independent: scores the storage layer shared by Hyphae\n\
         # AND echo+journal. Reproducible from (n={}, trials={}, seed={}).\n",
        env.n_fragments, env.trials_per_cell, env.seed_base
    ));
    out.push_str(
        "# Columns: bare = hash-chain verify; loc = localised to exact seq;\n\
         #          anchor = external Ed25519 head anchor; fp = false positives;\n\
         #          scan = mean fraction of entries read before detection.\n",
    );

    let systems = ["verbatim-journal", "echo-no-journal"];
    let advs = ["store-only", "chain-aware", "chain-aware+key"];

    for sys in systems {
        out.push_str(&format!("\n## System: {sys}\n"));
        for adv in advs {
            out.push_str(&format!("\n### Adversary: {adv}\n"));
            out.push_str(&format!(
                "{:<16} {:>6} {:>6} {:>6} {:>6} {:>6}\n",
                "mode", "bare", "loc", "anchor", "fp", "scan"
            ));
            out.push_str(&"-".repeat(50));
            out.push('\n');
            for mode in TamperMode::all() {
                if let Some(c) = env
                    .cells
                    .iter()
                    .find(|c| c.system == sys && c.adversary == adv && c.tamper_mode == mode.name())
                {
                    out.push_str(&format!(
                        "{:<16} {:>6} {:>6} {:>6} {:>6} {:>5.0}%\n",
                        c.tamper_mode,
                        pct(c.bare_detection_rate),
                        pct(c.bare_localisation_rate),
                        pct(c.anchored_detection_rate),
                        pct(c.false_positive_rate),
                        c.mean_scan_fraction * 100.0,
                    ));
                }
            }
        }
    }

    out.push_str(
        "\n# Reading:\n\
         #  * store-only adversary  → bare chain detects + localises in-place\n\
         #    tampering; anchor only fires when the head itself shifts.\n\
         #  * chain-aware adversary → bare chain is defeated (0%); the external\n\
         #    Ed25519 head anchor catches it (100%).\n\
         #  * chain-aware+key       → anchor key compromised → no protection.\n\
         #    This is the guarantee's exact boundary: tamper-evidence holds\n\
         #    against any attacker who does NOT hold the anchor signing key.\n\
         #  * echo-no-journal       → 0% everywhere: no journal, no provenance.\n\
         #    (Hyphae and echo+journal share the verbatim-journal row above;\n\
         #    the provenance property is the addable layer, not the realizer.)\n\
         #  * head_rollback         → consistent by construction, so the bare\n\
         #    chain cannot see it; the single-head anchor catches it via head\n\
         #    mismatch. Non-equivocation across observers needs an external\n\
         #    append-only ledger (see README §Future work).\n",
    );

    out
}
