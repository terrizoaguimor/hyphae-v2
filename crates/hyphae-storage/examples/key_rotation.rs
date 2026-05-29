// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Anchor key rotation demonstration (ADR-0035).
//!
//! The anchor/ledger/witness all sign with a long-lived key. This shows
//! rotating it safely: a successor key is authorized by the predecessor,
//! rooted in a genesis key trusted out-of-band, and a ledger that spans
//! the rotation verifies under the keyring — while a single retired key
//! can neither verify post-rotation history nor (if compromised) forge
//! new epochs.
//!
//! Run: `cargo run -p hyphae-storage --example key_rotation`

use hyphae_storage::{
    AnchorLedger, HeadAnchor, Keyring, verify_keyring, verify_ledger, verify_ledger_with_keyring,
};

fn main() {
    // Two key generations: A (genesis/root) rotates to B at epoch 3.
    let a = HeadAnchor::from_seed(&[1u8; 32]);
    let b = HeadAnchor::from_seed(&[2u8; 32]);
    let rotate_at = 3u64;
    let total = 6u64;

    // Build a ledger: epochs 0..3 signed by A, epochs 3..6 by B.
    let mut ledger = AnchorLedger::new();
    for i in 0..total {
        let signer = if i < rotate_at { &a } else { &b };
        signer.append_to_ledger(&mut ledger, [i as u8; 32]);
    }

    // The keyring: root A, one rotation to B authorized by A.
    let mut keyring = Keyring::new(a.verifying_key());
    keyring.push(a.authorize_rotation(&b.verifying_key(), rotate_at));

    println!("# Anchor key rotation — ADR-0035");
    println!(
        "# Ledger of {total} epochs; key rotated A -> B at epoch {rotate_at}.\n\
         # Root key A is trusted out-of-band; B is authorized by A.\n"
    );

    // 1. The keyring lineage verifies.
    println!(
        "{:<42} {}",
        "keyring lineage (A authorizes B)",
        ok(verify_keyring(&keyring).is_ok(), true)
    );

    // 2. The spanning ledger verifies under the keyring.
    println!(
        "{:<42} {}",
        "ledger spanning rotation, under keyring",
        ok(verify_ledger_with_keyring(&ledger, &keyring).is_ok(), true)
    );

    // 3. The same ledger does NOT verify under A alone.
    println!(
        "{:<42} {}",
        "same ledger under retired key A alone",
        ok(verify_ledger(&ledger, &a.verifying_key()).is_ok(), false)
    );

    // 4. A forged successor not authorized by A is rejected.
    let c = HeadAnchor::from_seed(&[9u8; 32]);
    let mut forged = Keyring::new(a.verifying_key());
    forged.push(c.authorize_rotation(&c.verifying_key(), rotate_at)); // C self-authorizes
    println!(
        "{:<42} {}",
        "forged successor (self-authorized)",
        ok(verify_keyring(&forged).is_ok(), false)
    );

    // 5. Compromise containment: attacker holds retired A and signs ALL
    //    epochs with A. Under the keyring, epochs >= rotate_at must be B.
    let mut attacker = AnchorLedger::new();
    for i in 0..total {
        a.append_to_ledger(&mut attacker, [i as u8; 32]);
    }
    println!(
        "{:<42} {}",
        "retired A forging post-rotation epochs",
        ok(
            verify_ledger_with_keyring(&attacker, &keyring).is_ok(),
            false
        )
    );

    println!("\n# Summary:");
    println!("#   property                                  result");
    println!("#   successor authorized by predecessor       REQUIRED (root-of-trust chain)");
    println!("#   ledger verifies across rotation           YES (per-epoch active key)");
    println!("#   retired key verifies new history          NO");
    println!("#   stolen new key inserts itself             NO (needs predecessor's signature)");
    println!("#   compromised retired key forges new epochs NO (epochs bound to active key)");
    println!("#");
    println!("# Out of scope (deployment): KMS/HSM key sourcing and the");
    println!("# revocation-timing policy (how fast a compromised key's window");
    println!("# is closed). See ADR-0035.");
}

fn ok(actual: bool, expected: bool) -> String {
    let tag = if actual { "PASS" } else { "REJECT" };
    let mark = if actual == expected {
        ""
    } else {
        "  <-- unexpected"
    };
    format!("{tag}{mark}")
}
