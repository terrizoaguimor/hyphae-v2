// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Ingestion bridge demonstration (ADR-0037).
//!
//! The integrity chain (chain -> anchor -> ledger -> witness -> keyring)
//! proves a stored fragment was not altered *after* admission. It says
//! nothing about *how it got there*. This experiment shows the gap and
//! how a signed-at-source ingestion credential closes it:
//!
//!   1. Attribution — a credential is validly signed by the named
//!      asserter and binds to the exact fragment bytes (no source
//!      needed).
//!   2. Faithful excerpt — given the source, the fragment is
//!      byte-for-byte source[locator].
//!   3. A forged credential (signed by an impostor) is rejected under
//!      the trusted asserter key.
//!   4. THE GAP — a fabricated fragment injected straight into the
//!      journal with NO credential: the integrity chain (verify())
//!      PASSES (it is faithfully journaled), but the ingestion check
//!      FAILS (no valid signed-at-source credential). Integrity and
//!      provenance-into-the-store are orthogonal.
//!
//! Run: `cargo run -p hyphae-storage --example ingestion_bridge`

use hyphae_storage::{
    ByteRange, IngestionAsserter, IngestionCredential, Journal, verify_credential,
    verify_faithful_excerpt,
};

const SOURCE: &[u8] = b"The IRS standard deduction for 2026 is 15000 dollars for single filers.";

/// Does the journal hold a valid ingestion credential attributing
/// `fragment` to `asserter_vk`?
fn has_valid_credential(
    journal: &Journal,
    n: u64,
    fragment: &[u8],
    asserter_vk: &ed25519_dalek::VerifyingKey,
) -> bool {
    for seq in 0..n {
        if let Ok(Some(entry)) = journal.read(seq) {
            if entry.event_kind == "ingestion_credential" {
                if let Some(cred) = IngestionCredential::from_bytes(&entry.payload) {
                    if verify_credential(&cred, fragment, asserter_vk) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn verdict(ok: bool, expected: bool) -> String {
    let tag = if ok { "PASS" } else { "FAIL" };
    let mark = if ok == expected {
        ""
    } else {
        "  <-- unexpected"
    };
    format!("{tag}{mark}")
}

fn main() {
    let asserter = IngestionAsserter::from_seed(&[5u8; 32]);
    let vk = asserter.verifying_key();

    // A genuine fragment extracted verbatim from the source.
    let locator = ByteRange { start: 4, end: 40 };
    let good_fragment = SOURCE[locator.start as usize..locator.end as usize].to_vec();
    let cred = asserter
        .assert_ingestion(
            &good_fragment,
            SOURCE,
            "doi:irs/2026/std-deduction",
            locator,
            1_750_000_000,
        )
        .expect("honest excerpt");

    println!("# Ingestion bridge — ADR-0037");
    println!(
        "# Signed-at-source provenance INTO the journal. Asserter holds a\n\
         # key of its own; a credential binds a fragment to its origin.\n"
    );

    // 1. Attribution (no source needed).
    println!("## 1. Attribution (credential signed by named asserter, binds exact bytes)");
    let attr = verify_credential(&cred, &good_fragment, &vk);
    println!(
        "{:<46} {}",
        "verify_credential(good fragment)",
        verdict(attr, true)
    );

    // 2. Faithful excerpt (source available).
    let faithful = verify_faithful_excerpt(&cred, &good_fragment, SOURCE, &vk);
    println!(
        "{:<46} {}",
        "verify_faithful_excerpt(vs real source)",
        verdict(faithful, true)
    );

    // 3. Forged credential: an impostor signs a credential for the same
    //    fragment; it must fail under the TRUSTED asserter key.
    println!("\n## 3. Forged credential (impostor asserter)");
    let impostor = IngestionAsserter::from_seed(&[0x99u8; 32]);
    let forged = impostor
        .assert_ingestion(
            &good_fragment,
            SOURCE,
            "doi:irs/2026/std-deduction",
            locator,
            1,
        )
        .unwrap();
    let forged_ok = verify_credential(&forged, &good_fragment, &vk); // checked vs TRUSTED vk
    println!(
        "{:<46} {}",
        "forged credential under trusted key",
        verdict(forged_ok, false)
    );

    // 3b. Fabricated-source claim caught by faithful-excerpt: the
    //     impostor's credential does not match a DIFFERENT real source.
    let other_source = b"An unrelated document whose text shares no excerpt here at all.";
    let fab_ok = verify_faithful_excerpt(&cred, &good_fragment, other_source, &vk);
    println!(
        "{:<46} {}",
        "faithful-excerpt vs a wrong source",
        verdict(fab_ok, false)
    );

    // 4. THE GAP — inject a fabricated fragment into a real journal with
    //    NO credential. Integrity chain passes; ingestion check fails.
    println!("\n## 4. The gap — fabricated fragment injected, no credential");
    let dir = std::env::temp_dir().join("hyphae-ingestion-bridge");
    let _ = std::fs::remove_dir_all(&dir);
    let injected = b"the deduction is 50000 dollars".to_vec();
    let n;
    {
        let mut j = Journal::open(&dir).expect("open");
        // Properly-ingested fragment + its credential.
        j.append("audit_memory_op", good_fragment.clone()).unwrap();
        j.append("ingestion_credential", cred.to_bytes()).unwrap();
        // Attacker injects a fabricated fragment — no credential.
        j.append("audit_memory_op", injected.clone()).unwrap();
        n = j.len();
    }
    let j = Journal::open(&dir).expect("reopen");

    let chain_ok = j.verify().is_ok();
    let good_has_cred = has_valid_credential(&j, n, &good_fragment, &vk);
    let injected_has_cred = has_valid_credential(&j, n, &injected, &vk);

    println!(
        "{:<46} {}",
        "integrity chain verify() (whole journal)",
        verdict(chain_ok, true)
    );
    println!(
        "{:<46} {}",
        "ingestion check: genuine fragment",
        verdict(good_has_cred, true)
    );
    println!(
        "{:<46} {}",
        "ingestion check: injected fragment",
        verdict(injected_has_cred, false)
    );
    let _ = std::fs::remove_dir_all(&dir);

    println!("\n# Summary:");
    println!("#   check                              genuine   injected");
    println!("#   integrity chain (verify)            PASS       PASS   <- both faithfully stored");
    println!(
        "#   ingestion credential (attribution)  PASS       FAIL   <- the gap the bridge closes"
    );
    println!("#");
    println!("# Integrity (within the store) and provenance (into the store) are");
    println!("# orthogonal: the chain proves the injected fragment was not altered,");
    println!("# NOT that it came from anywhere. The ingestion credential is what");
    println!("# attributes a fragment to a signed source. Provenance is not truth:");
    println!("# it moves trust from the store to the named asserter, and (given the");
    println!("# source) proves a faithful excerpt — content validity is orthogonal");
    println!("# (RAGShield-style), out of scope. See ADR-0037.");
}
