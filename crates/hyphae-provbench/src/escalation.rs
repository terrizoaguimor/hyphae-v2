// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Defense-escalation experiment (provbench/v2).
//!
//! The tamper-taxonomy matrix ([`crate::harness`]) scores the journal
//! layer's detection and localisation. This experiment scores the
//! *full provenance stack* — bare hash chain → external head anchor
//! (ADR-0032) → append-only ledger (ADR-0033) → external witness
//! (ADR-0034) — against four attacks, each designed so that exactly the
//! next defense layer up is the one that catches it:
//!
//! | attack | caught first by |
//! |---|---|
//! | in-place edit | bare chain |
//! | chain-aware head rewrite | head anchor |
//! | rollback + stale-anchor replay | append-only ledger |
//! | withholding (truncated ledger) | external witness |
//!
//! The `bare` and `anchor` verdicts for the journal-level attacks come
//! from running the real [`VerbatimJournal`] system (`verify()` and a
//! single-head anchor); the `ledger` and `witness` verdicts are the
//! shipped `hyphae-storage` checks over the presented heads/ledger.
//! Fully reproducible from `(n, seed)`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hyphae_storage::{
    AnchorLedger, HeadAnchor, JournalEntry, Witness, verify_against_witness, verify_anchored_head,
    verify_fresh_head,
};
use serde::Serialize;

use crate::PROTOCOL_VERSION;
use crate::fragment::{Fragment, corpus};
use crate::prng::seed32;
use crate::system::{ProvenanceSystem, VerifyOutcome};
use crate::systems::VerbatimJournal;
use crate::tamper::TamperMode;

const EVENT_KIND: &str = "audit_memory_op";

/// One row of the escalation matrix: an attack and whether each defense
/// level detects it.
#[derive(Debug, Clone, Serialize)]
pub struct EscalationRow {
    /// Attack name.
    pub attack: String,
    /// Bare hash-chain `verify()` detects it.
    pub bare: bool,
    /// External single-head anchor detects it (as presented by the
    /// attacker — a genuine but possibly stale anchor).
    pub anchor: bool,
    /// Append-only ledger freshness detects it.
    pub ledger: bool,
    /// Ledger + external witness detects it.
    pub ledger_witness: bool,
    /// The first (weakest) level that catches it, for readability.
    pub caught_first_by: String,
}

/// The escalation result.
#[derive(Debug, Clone, Serialize)]
pub struct EscalationEnvelope {
    /// Protocol identifier + version.
    pub protocol: String,
    /// Corpus size.
    pub n_fragments: u64,
    /// Seed.
    pub seed: u64,
    /// One row per attack.
    pub rows: Vec<EscalationRow>,
}

static UNIQ: AtomicU64 = AtomicU64::new(0);

fn tmpdir(tag: &str) -> PathBuf {
    let u = UNIQ.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("provbench-esc-{}-{u}-{tag}", std::process::id()))
}

/// Recompute the journal chain heads in memory, deterministically and
/// byte-identically to [`VerbatimJournal`]'s ingest (seq-derived
/// timestamps). `heads[k]` is the head after `k+1` fragments.
fn chain_heads(fragments: &[Fragment]) -> Vec<[u8; 32]> {
    let mut prev = [0u8; 32];
    let mut heads = Vec::with_capacity(fragments.len());
    for (i, f) in fragments.iter().enumerate() {
        let e = JournalEntry {
            seq: i as u64,
            prev_hash: prev,
            timestamp_ns: i as u128,
            event_kind: EVENT_KIND.to_string(),
            payload: f.body.clone(),
        };
        prev = e.content_hash();
        heads.push(prev);
    }
    heads
}

fn caught_first(bare: bool, anchor: bool, ledger: bool, witness: bool) -> String {
    if bare {
        "bare"
    } else if anchor {
        "anchor"
    } else if ledger {
        "ledger"
    } else if witness {
        "witness"
    } else {
        "NONE"
    }
    .to_string()
}

/// Run the escalation experiment. Deterministic in `(n, seed)`.
#[must_use]
pub fn run_escalation(n: u64, seed: u64) -> EscalationEnvelope {
    let n = n.max(8);
    let frags = corpus(n, seed);
    let heads = chain_heads(&frags);
    let final_head = *heads.last().unwrap();

    // The audit service: a single-head anchor over the legitimate final
    // head, plus an append-only ledger of every checkpoint head, plus an
    // independent witness of the ledger tail.
    let anchor = HeadAnchor::from_seed(&seed32(seed));
    let avk = anchor.verifying_key();
    let genuine_final_anchor = anchor.anchor(final_head);

    let mut ledger = AnchorLedger::new();
    for h in &heads {
        anchor.append_to_ledger(&mut ledger, *h);
    }
    let witness = Witness::from_seed(&seed32(seed ^ 0x7717_4E55)); // distinct key
    let wvk = witness.verifying_key();
    let attestation = witness.observe(&ledger).unwrap();

    let sys = VerbatimJournal;
    let target = (n / 2).max(1);
    let mut rows = Vec::new();

    // ── 1. in-place edit (store-only) — bare chain catches it. ──
    {
        let dir = tmpdir("edit");
        let _ = std::fs::remove_dir_all(&dir);
        sys.ingest(&dir, &frags);
        sys.tamper(&dir, TamperMode::Edit, target, n, false);
        let bare = matches!(sys.verify(&dir), VerifyOutcome::Violation { .. });
        let head_after = sys.head(&dir).unwrap_or([0u8; 32]); // unchanged (middle edit)
        let anchor_ok = verify_anchored_head(&head_after, &genuine_final_anchor, &avk);
        let ledger_fresh = verify_fresh_head(&head_after, &ledger, &avk);
        let witness_ok = ledger_fresh && verify_against_witness(&ledger, &attestation, &avk, &wvk);
        let _ = std::fs::remove_dir_all(&dir);
        rows.push(row(
            "in_place_edit",
            bare,
            !anchor_ok,
            !ledger_fresh,
            !witness_ok,
        ));
    }

    // ── 2. chain-aware rewrite — head anchor catches it. ──
    {
        let dir = tmpdir("chainaware");
        let _ = std::fs::remove_dir_all(&dir);
        sys.ingest(&dir, &frags);
        sys.tamper(&dir, TamperMode::Edit, target, n, true);
        let bare = matches!(sys.verify(&dir), VerifyOutcome::Violation { .. });
        let head_after = sys.head(&dir).unwrap_or([0u8; 32]); // recomputed, != final_head
        let anchor_ok = verify_anchored_head(&head_after, &genuine_final_anchor, &avk);
        let ledger_fresh = verify_fresh_head(&head_after, &ledger, &avk);
        let witness_ok = ledger_fresh && verify_against_witness(&ledger, &attestation, &avk, &wvk);
        let _ = std::fs::remove_dir_all(&dir);
        rows.push(row(
            "chain_aware_rewrite",
            bare,
            !anchor_ok,
            !ledger_fresh,
            !witness_ok,
        ));
    }

    // Roll a real store back to a valid prefix; the presented head is
    // that prefix's tail (an earlier checkpoint head).
    let rolled_head;
    let bare_rollback;
    {
        let dir = tmpdir("rollback");
        let _ = std::fs::remove_dir_all(&dir);
        sys.ingest(&dir, &frags);
        sys.tamper(&dir, TamperMode::HeadRollback, target, n, false);
        bare_rollback = matches!(sys.verify(&dir), VerifyOutcome::Violation { .. }); // false: valid prefix
        rolled_head = sys.head(&dir).unwrap_or([0u8; 32]);
        let _ = std::fs::remove_dir_all(&dir);
    }
    // The rolled head is an earlier checkpoint; find its ledger epoch.
    let rolled_epoch = heads.iter().position(|h| *h == rolled_head).unwrap() as u64;

    // ── 3. rollback + stale-anchor replay — the ledger catches it. ──
    {
        // The attacker presents the genuine but superseded single-head
        // anchor for the rolled-back head, and the full ledger.
        let stale_anchor = anchor.anchor(rolled_head);
        let anchor_ok = verify_anchored_head(&rolled_head, &stale_anchor, &avk); // true: genuine
        let ledger_fresh = verify_fresh_head(&rolled_head, &ledger, &avk); // false: not the tail
        let witness_ok = ledger_fresh && verify_against_witness(&ledger, &attestation, &avk, &wvk);
        rows.push(row(
            "rollback_stale_anchor",
            bare_rollback,
            !anchor_ok,
            !ledger_fresh,
            !witness_ok,
        ));
    }

    // ── 4. withholding — only the external witness catches it. ──
    {
        // The attacker presents the rolled-back head AND a truncated
        // ledger (epochs 0..=rolled_epoch) as if it were the latest.
        let truncated =
            AnchorLedger::from_entries(ledger.entries()[..=rolled_epoch as usize].to_vec());
        let stale_anchor = anchor.anchor(rolled_head);
        let anchor_ok = verify_anchored_head(&rolled_head, &stale_anchor, &avk); // true: genuine
        let ledger_fresh = verify_fresh_head(&rolled_head, &truncated, &avk); // true: matches truncated tail
        let witness_ok =
            ledger_fresh && verify_against_witness(&truncated, &attestation, &avk, &wvk); // false: truncated
        rows.push(row(
            "withholding",
            bare_rollback,
            !anchor_ok,
            !ledger_fresh,
            !witness_ok,
        ));
    }

    EscalationEnvelope {
        protocol: PROTOCOL_VERSION.to_string(),
        n_fragments: n,
        seed,
        rows,
    }
}

fn row(attack: &str, bare: bool, anchor: bool, ledger: bool, witness: bool) -> EscalationRow {
    EscalationRow {
        attack: attack.to_string(),
        bare,
        anchor,
        ledger,
        ledger_witness: witness,
        caught_first_by: caught_first(bare, anchor, ledger, witness),
    }
}

fn mark(b: bool) -> &'static str {
    if b { "DETECT" } else { "  --  " }
}

/// Render the escalation matrix as a fixed-width table.
#[must_use]
pub fn render_escalation_table(env: &EscalationEnvelope) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Defense escalation — {}\n# Full provenance stack vs four attacks (n={}, seed={}).\n",
        env.protocol, env.n_fragments, env.seed
    ));
    out.push_str("# Each attack is caught by exactly the next defense layer up.\n\n");
    out.push_str(&format!(
        "{:<24} {:>7} {:>8} {:>8} {:>9}   {}\n",
        "attack", "bare", "anchor", "ledger", "witness", "caught-by"
    ));
    out.push_str(&"-".repeat(72));
    out.push('\n');
    for r in &env.rows {
        out.push_str(&format!(
            "{:<24} {:>7} {:>8} {:>8} {:>9}   {}\n",
            r.attack,
            mark(r.bare),
            mark(r.anchor),
            mark(r.ledger),
            mark(r.ledger_witness),
            r.caught_first_by,
        ));
    }
    out.push_str(
        "\n# Reading: detection escalates exactly one layer per attack —\n\
         # bare chain → head anchor (0032) → append-only ledger (0033) →\n\
         # external witness (0034). Every attack is caught by some layer for\n\
         # an adversary holding none of the signing keys. Key compromise is\n\
         # recoverable via rotation (0035); source ingestion is the open\n\
         # boundary (0034).\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(env: &'a EscalationEnvelope, attack: &str) -> &'a EscalationRow {
        env.rows.iter().find(|r| r.attack == attack).unwrap()
    }

    #[test]
    fn escalation_matrix_is_staircase() {
        let env = run_escalation(16, 5);
        let edit = find(&env, "in_place_edit");
        assert!(edit.bare && !edit.anchor, "bare catches an in-place edit");

        let ca = find(&env, "chain_aware_rewrite");
        assert!(!ca.bare && ca.anchor && ca.ledger && ca.ledger_witness);

        let rb = find(&env, "rollback_stale_anchor");
        assert!(
            !rb.bare && !rb.anchor && rb.ledger && rb.ledger_witness,
            "rollback+stale-anchor: ledger is the first catcher"
        );

        let wh = find(&env, "withholding");
        assert!(
            !wh.bare && !wh.anchor && !wh.ledger && wh.ledger_witness,
            "withholding: only the witness catches it"
        );
    }

    #[test]
    fn escalation_is_deterministic() {
        let a = run_escalation(16, 9);
        let b = run_escalation(16, 9);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }
}
