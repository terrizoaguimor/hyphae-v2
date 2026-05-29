// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Append-only anchor ledger demonstration (ADR-0033).
//!
//! ADR-0032's single-head anchor catches a chain-aware attacker who
//! rewrites the head. But a lone signature pins only *a* valid head,
//! not *the latest* one, leaving two gaps this experiment exercises:
//!
//!   1. **Freshness.** The attacker rolls the journal back to an
//!      earlier state and replays the *genuine but superseded* anchor
//!      for that earlier head. Single-signature verification ACCEPTS it
//!      (the signature is real and matches the rolled-back head). The
//!      append-only ledger REJECTS it: the rolled-back head is not the
//!      ledger's latest entry.
//!   2. **Non-equivocation.** Two ledger views that diverge at some
//!      epoch (the key holder signing two histories) are caught by a
//!      prefix-consistency check across views.
//!
//! Plus the ledger is itself tamper-evident: forging any published
//! entry fails `verify_ledger`.
//!
//! Run: `cargo run -p hyphae-storage --example anchor_ledger`

use hyphae_storage::{
    AnchorLedger, HeadAnchor, Journal, ledgers_consistent, verify_anchored_head, verify_fresh_head,
    verify_ledger,
};

fn facts() -> Vec<&'static str> {
    vec![
        "the migration completed at 14:02 UTC",
        "the deploy succeeded on the first attempt",
        "the indemnification clause survives termination",
        "weekly active users grew six percent",
        "the auditor noted no material findings",
        "the rollback lost three hours of writes",
        "the p95 latency rose from 180 to 210 ms",
        "the quarter closed with no restatements",
    ]
}

fn main() {
    let facts = facts();
    let dir = std::env::temp_dir().join("hyphae-anchor-ledger");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");

    // The audit service (key held OUTSIDE the store) anchors the head
    // after every append, publishing each to the append-only ledger.
    let anchor = HeadAnchor::from_seed(&[11u8; 32]);
    let vk = anchor.verifying_key();
    let mut ledger = AnchorLedger::new();
    let mut heads: Vec<[u8; 32]> = Vec::new();

    let mut journal = Journal::open(&dir).expect("open");
    for f in &facts {
        let (_seq, head) = journal
            .append("audit_memory_op", f.as_bytes().to_vec())
            .expect("append");
        heads.push(head);
        anchor.append_to_ledger(&mut ledger, head);
    }
    let n = facts.len();
    let latest_head = *heads.last().unwrap();
    let rollback_epoch = 3usize; // attacker rolls back to head after 4 appends

    println!("# Append-only anchor ledger — ADR-0033");
    println!(
        "# Real hyphae_storage::Journal: {n} appends, head anchored to an\n\
         # append-only Ed25519 ledger after each (epochs 0..{}). The ledger\n\
         # is itself hash-chained. Key held outside the store.\n",
        n - 1
    );

    // ── 1. Freshness: rollback + stale-anchor replay ──
    let stale_head = heads[rollback_epoch];
    let stale_anchor = anchor.anchor(stale_head); // genuine single-head anchor, epoch 3
    let single_accepts = verify_anchored_head(&stale_head, &stale_anchor, &vk);
    let ledger_accepts = verify_fresh_head(&stale_head, &ledger, &vk);

    println!("## 1. Freshness — rollback to epoch {rollback_epoch} + replay its genuine anchor");
    println!(
        "{:<34} {}",
        "single-head anchor (ADR-0032)",
        verdict(single_accepts, true)
    );
    println!(
        "{:<34} {}",
        "append-only ledger (ADR-0033)",
        verdict(ledger_accepts, false)
    );
    println!(
        "#   The stale anchor is a REAL signature over the rolled-back head,\n\
         #   so the single-head check accepts it (freshness gap). The ledger\n\
         #   rejects it: head@epoch{rollback_epoch} is not the ledger tail (epoch {}).\n",
        n - 1
    );

    // Sanity: the genuine latest head still passes the ledger.
    assert!(verify_fresh_head(&latest_head, &ledger, &vk));

    // ── 2. Non-equivocation: a forked ledger ──
    // Same first `rollback_epoch` entries, then the key holder
    // equivocates with a different head at that epoch.
    let mut forked = AnchorLedger::from_entries(ledger.entries()[..rollback_epoch].to_vec());
    let anchor2 = HeadAnchor::from_seed(&[11u8; 32]);
    anchor2.append_to_ledger(&mut forked, [0xEEu8; 32]);
    let fork_internally_valid = verify_ledger(&forked, &vk).is_ok();
    let cross_view_consistent = ledgers_consistent(&ledger, &forked);

    println!("## 2. Non-equivocation — two histories diverging at epoch {rollback_epoch}");
    println!(
        "{:<34} {}",
        "forked view verifies on its own",
        yesno(fork_internally_valid)
    );
    println!(
        "{:<34} {}",
        "cross-view consistency check",
        verdict(cross_view_consistent, false)
    );
    println!(
        "#   Each view is internally valid, but comparing them (an auditor\n\
         #   holding one, gossip/witness offering the other) detects the\n\
         #   divergence: equivocation caught.\n"
    );

    // ── 3. Ledger tamper-evidence ──
    let mut entries = ledger.entries().to_vec();
    entries[2].head = [0x42u8; 32]; // forge a published entry's head
    let tampered = AnchorLedger::from_entries(entries);
    let tampered_verifies = verify_ledger(&tampered, &vk).is_ok();

    println!("## 3. Ledger tamper-evidence — forge a published entry's head");
    println!(
        "{:<34} {}",
        "tampered ledger verifies",
        verdict(tampered_verifies, false)
    );
    println!(
        "#   The signature at that epoch no longer covers the forged head;\n\
         #   verify_ledger rejects the published log.\n"
    );

    println!("# Summary:");
    println!("#   threat                         single-head   append-only ledger");
    println!("#   chain-aware head rewrite         DETECTED        DETECTED");
    println!("#   rollback + stale-anchor replay   MISSED          DETECTED");
    println!("#   equivocation across views        n/a             DETECTED");
    println!("#   forged published anchor          n/a             DETECTED");
    println!("#");
    println!("# The ledger closes ADR-0032's freshness/non-equivocation followup.");
    println!("# Out of scope (deployment): an EXTERNAL WITNESS of the ledger tail");
    println!("# (timestamp authority / gossiped tree head) to stop a store that");
    println!("# withholds entries, and anchor-key rotation. See ADR-0033.");

    let _ = std::fs::remove_dir_all(&dir);
}

fn verdict(accepted: bool, expected_accept: bool) -> String {
    let tag = if accepted {
        "ACCEPTS (no detection)"
    } else {
        "REJECTS (DETECTED)"
    };
    let mark = if accepted == expected_accept {
        ""
    } else {
        "  <-- the gap"
    };
    format!("{tag}{mark}")
}

fn yesno(b: bool) -> &'static str {
    if b { "YES" } else { "NO" }
}
