// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! External witness of the anchor ledger (ADR-0034).
//!
//! The append-only ledger ([`crate::ledger`], ADR-0033) gives freshness
//! and non-equivocation — *when an auditor can compare views*. It leaves
//! one gap, named in ADR-0033's threat model: a store that **withholds**
//! later entries. Such a store presents a valid *prefix* of the ledger
//! and claims it is the latest; [`crate::ledger::verify_fresh_head`]
//! accepts it, because the rolled-back journal head genuinely matches
//! that prefix's tail. Nothing the store hands the auditor reveals that
//! more entries exist.
//!
//! A **witness** closes this. An independent party — holding a key
//! *separate* from the anchor's — observes the ledger and signs an
//! attestation `(epoch, ledger_head_hash)`: "at this point I saw the
//! ledger reach at least this epoch, with this head". An auditor who
//! holds a witness attestation then requires the store's presented
//! ledger to **contain that epoch with a matching head**. A withheld
//! (truncated) ledger no longer reaches the witnessed epoch, and a fork
//! before it has a different head — either way, detected.
//!
//! In deployment the witness maps to a timestamp authority (RFC 3161),
//! a transparency-log witness (the C2SP/`sumdb` sense), gossiped
//! signed-tree-heads, or an OpenTimestamps/Bitcoin commitment of
//! [`AnchorLedger::head_hash`]. Here it is modeled as an independent
//! Ed25519 signer, exactly as the anchor is — the protocol is the
//! contribution; wiring a real-world witness network is deployment
//! (ADR-0034).
//!
//! [`AnchorLedger::head_hash`]: crate::ledger::AnchorLedger::head_hash

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use crate::ledger::{AnchorLedger, verify_ledger};

/// An independent observer's signed attestation that, at `epoch`, the
/// anchor ledger's head was `ledger_head_hash`. Signed with a key
/// **separate** from the anchor's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WitnessAttestation {
    /// The ledger epoch (entry index) the witness observed as the tail.
    pub epoch: u64,
    /// The ledger's head hash at that epoch (entry hash of that entry).
    pub ledger_head_hash: [u8; 32],
    /// Ed25519 signature over `(epoch ‖ ledger_head_hash)` by the
    /// witness key.
    pub signature: [u8; 64],
}

impl WitnessAttestation {
    /// The 40-byte message the witness signs: `epoch ‖ ledger_head_hash`.
    #[must_use]
    pub fn signing_message(epoch: u64, ledger_head_hash: &[u8; 32]) -> [u8; 40] {
        let mut msg = [0u8; 40];
        msg[..8].copy_from_slice(&epoch.to_le_bytes());
        msg[8..40].copy_from_slice(ledger_head_hash);
        msg
    }
}

/// An independent witness. Holds its own signing key — never the
/// anchor's. In deployment this is a timestamp authority / transparency
/// witness / OpenTimestamps commitment; here, a deterministic signer
/// for reproducible tests.
pub struct Witness {
    signing_key: SigningKey,
}

impl Witness {
    /// Construct from a fixed 32-byte seed (deterministic; for tests
    /// and the demonstration). Production witnesses are external
    /// services, not seeds.
    #[must_use]
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self {
            signing_key: SigningKey::from_bytes(seed),
        }
    }

    /// The witness's public verifying key. An auditor trusts this
    /// independently of the anchor key.
    #[must_use]
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Observe a ledger at its current tail and sign an attestation.
    /// Returns `None` for an empty ledger (nothing to attest).
    #[must_use]
    pub fn observe(&self, ledger: &AnchorLedger) -> Option<WitnessAttestation> {
        let latest = ledger.latest()?;
        let epoch = latest.epoch;
        let ledger_head_hash = ledger.head_hash();
        let msg = WitnessAttestation::signing_message(epoch, &ledger_head_hash);
        Some(WitnessAttestation {
            epoch,
            ledger_head_hash,
            signature: self.signing_key.sign(&msg).to_bytes(),
        })
    }
}

/// Verify a presented ledger against a witness attestation.
///
/// Returns `true` only when:
/// 1. the ledger is well-formed under the anchor key
///    ([`verify_ledger`]),
/// 2. the witness attestation's signature is valid under `witness_vk`,
/// 3. the ledger **contains** the witnessed epoch (else the store is
///    withholding entries it published), and
/// 4. the ledger's head hash *at that epoch* equals the witnessed value
///    (else the store forked before the witnessed point).
///
/// A ledger that has *grown past* the witnessed epoch still passes:
/// earlier entries are immutable in an append-only log, so the entry at
/// the witnessed epoch is unchanged.
#[must_use]
pub fn verify_against_witness(
    ledger: &AnchorLedger,
    attestation: &WitnessAttestation,
    anchor_vk: &VerifyingKey,
    witness_vk: &VerifyingKey,
) -> bool {
    if verify_ledger(ledger, anchor_vk).is_err() {
        return false;
    }
    let msg = WitnessAttestation::signing_message(attestation.epoch, &attestation.ledger_head_hash);
    let sig = Signature::from_bytes(&attestation.signature);
    if witness_vk.verify(&msg, &sig).is_err() {
        return false;
    }
    // The ledger must reach the witnessed epoch (verify_ledger already
    // checked epoch == position, so index == epoch).
    let Some(entry) = ledger.entries().get(attestation.epoch as usize) else {
        return false; // withholding: presented ledger is shorter
    };
    entry.entry_hash() == attestation.ledger_head_hash
}

/// The full auditor check: the journal's current head is the ledger's
/// tail ([`crate::ledger::verify_fresh_head`]) **and** the ledger is
/// consistent with what the witness last observed
/// ([`verify_against_witness`]). Together these reject in-place
/// tampering, chain-aware rewrite, rollback-with-stale-anchor,
/// equivocation, and entry withholding — for any attacker who holds
/// neither the anchor signing key nor the witness key.
#[must_use]
pub fn verify_fresh_against_witness(
    current_head: &[u8; 32],
    ledger: &AnchorLedger,
    attestation: &WitnessAttestation,
    anchor_vk: &VerifyingKey,
    witness_vk: &VerifyingKey,
) -> bool {
    crate::ledger::verify_fresh_head(current_head, ledger, anchor_vk)
        && verify_against_witness(ledger, attestation, anchor_vk, witness_vk)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::HeadAnchor;

    const ANCHOR_SEED: [u8; 32] = [9u8; 32];
    const WITNESS_SEED: [u8; 32] = [3u8; 32];

    fn ledger_of(anchor: &HeadAnchor, n: u64) -> AnchorLedger {
        let mut l = AnchorLedger::new();
        for i in 0..n {
            anchor.append_to_ledger(&mut l, [i as u8; 32]);
        }
        l
    }

    #[test]
    fn witnessed_ledger_passes() {
        let anchor = HeadAnchor::from_seed(&ANCHOR_SEED);
        let witness = Witness::from_seed(&WITNESS_SEED);
        let l = ledger_of(&anchor, 6);
        let att = witness.observe(&l).unwrap();
        assert_eq!(att.epoch, 5);
        assert!(verify_against_witness(
            &l,
            &att,
            &anchor.verifying_key(),
            &witness.verifying_key()
        ));
    }

    #[test]
    fn withheld_ledger_is_caught() {
        let anchor = HeadAnchor::from_seed(&ANCHOR_SEED);
        let witness = Witness::from_seed(&WITNESS_SEED);
        let full = ledger_of(&anchor, 6);
        let att = witness.observe(&full).unwrap(); // witnessed epoch 5

        // Store presents a truncated (withheld) ledger reaching only
        // epoch 2. It is internally valid and a consistent prefix …
        let truncated = AnchorLedger::from_entries(full.entries()[..3].to_vec());
        assert!(verify_ledger(&truncated, &anchor.verifying_key()).is_ok());
        // … but it does not reach the witnessed epoch, so the witness
        // check rejects it.
        assert!(!verify_against_witness(
            &truncated,
            &att,
            &anchor.verifying_key(),
            &witness.verifying_key()
        ));
    }

    #[test]
    fn freshness_alone_accepts_withholding_but_witness_does_not() {
        let anchor = HeadAnchor::from_seed(&ANCHOR_SEED);
        let witness = Witness::from_seed(&WITNESS_SEED);
        let avk = anchor.verifying_key();
        let wvk = witness.verifying_key();
        let full = ledger_of(&anchor, 6);
        let att = witness.observe(&full).unwrap();

        let truncated = AnchorLedger::from_entries(full.entries()[..3].to_vec());
        let rolled_head = truncated.latest().unwrap().head; // the head the store now presents

        // Freshness against the TRUNCATED ledger is happy (the head is
        // its tail) — the withholding gap.
        assert!(crate::ledger::verify_fresh_head(
            &rolled_head,
            &truncated,
            &avk
        ));
        // The combined check is not: the witness saw further.
        assert!(!verify_fresh_against_witness(
            &rolled_head,
            &truncated,
            &att,
            &avk,
            &wvk
        ));
        // And on the genuine full ledger, the combined check passes.
        let real_head = full.latest().unwrap().head;
        assert!(verify_fresh_against_witness(
            &real_head, &full, &att, &avk, &wvk
        ));
    }

    #[test]
    fn ledger_grown_past_witness_still_passes() {
        let anchor = HeadAnchor::from_seed(&ANCHOR_SEED);
        let witness = Witness::from_seed(&WITNESS_SEED);
        let early = ledger_of(&anchor, 4);
        let att = witness.observe(&early).unwrap(); // witnessed epoch 3
        // The ledger keeps growing after the witness observed it.
        let grown = ledger_of(&anchor, 7);
        assert!(verify_against_witness(
            &grown,
            &att,
            &anchor.verifying_key(),
            &witness.verifying_key()
        ));
    }

    #[test]
    fn fork_before_witnessed_epoch_is_caught() {
        let anchor = HeadAnchor::from_seed(&ANCHOR_SEED);
        let witness = Witness::from_seed(&WITNESS_SEED);
        let full = ledger_of(&anchor, 6);
        let att = witness.observe(&full).unwrap();
        // A fork: same first 2 epochs, divergent head at epoch 2, then
        // padded back up past the witnessed epoch.
        let mut forked = AnchorLedger::from_entries(full.entries()[..2].to_vec());
        anchor.append_to_ledger(&mut forked, [0xEEu8; 32]);
        for i in 3..6 {
            anchor.append_to_ledger(&mut forked, [i as u8; 32]);
        }
        // It reaches epoch 5, but its head at epoch 5 differs from the
        // witnessed one (the fork changed the chain).
        assert!(!verify_against_witness(
            &forked,
            &att,
            &anchor.verifying_key(),
            &witness.verifying_key()
        ));
    }

    #[test]
    fn wrong_witness_key_is_rejected() {
        let anchor = HeadAnchor::from_seed(&ANCHOR_SEED);
        let witness = Witness::from_seed(&WITNESS_SEED);
        let impostor = Witness::from_seed(&[1u8; 32]);
        let l = ledger_of(&anchor, 6);
        let att = witness.observe(&l).unwrap();
        // Attestation does not verify under a different witness key.
        assert!(!verify_against_witness(
            &l,
            &att,
            &anchor.verifying_key(),
            &impostor.verifying_key()
        ));
    }
}
