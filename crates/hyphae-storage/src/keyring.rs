// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Anchor key rotation (ADR-0035).
//!
//! The anchor (ADR-0032), ledger (ADR-0033), and witness (ADR-0034) all
//! sign with a long-lived key and each named **key rotation** as a
//! deployment followup. A long-lived signing key is a single point of
//! compromise and cannot be retired without breaking verification of
//! everything it ever signed.
//!
//! This module adds rotation as a *composable* layer that does not
//! change the ledger entry format. A [`Keyring`] is a chain of signing
//! keys rooted in a genesis key trusted out-of-band (like a root CA):
//! each successor is authorized by a signature from its **predecessor**
//! ([`HeadAnchor::authorize_rotation`]), and each rotation is pinned to
//! the ledger epoch from which the new key is active. An auditor then
//! verifies a ledger that spans rotations with
//! [`verify_ledger_with_keyring`], which checks each entry under
//! whichever key was active at that entry's epoch.
//!
//! Two properties follow:
//!
//! - **Authenticated rotation.** A successor key is legitimate only if
//!   the current key signed it. An attacker who steals a *new* key
//!   cannot insert it into the keyring without the predecessor, and
//!   cannot forge a rotation chain back to the trusted root.
//! - **Compromise containment.** After rotating away from a key, that
//!   retired key can no longer sign *new* ledger epochs: entries at
//!   epochs where a later key is active are verified under the later
//!   key, so a signature from the retired key fails.
//!
//! Out of scope (deployment): sourcing keys from a KMS/HSM, and the
//! revocation timing policy (how fast a compromised key's window is
//! closed). See ADR-0035.
//!
//! [`HeadAnchor::authorize_rotation`]: crate::anchor::HeadAnchor::authorize_rotation

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use thiserror::Error;

use crate::ledger::{AnchorLedger, LedgerEntry};

/// Domain-separation tag so a rotation authorization can never be
/// confused with a ledger-entry or witness signature.
const ROTATE_TAG: &[u8] = b"hyphae-keyrotate-v1";

/// A signed authorization that, from ledger epoch `from_ledger_epoch`,
/// signing passes to `new_key`. The `authorization` is an Ed25519
/// signature by the *predecessor* key over
/// `(ROTATE_TAG ‖ from_ledger_epoch ‖ new_key)`.
#[derive(Debug, Clone, Copy)]
pub struct KeyRotation {
    /// The ledger epoch from which `new_key` is the active signer.
    pub from_ledger_epoch: u64,
    /// The incoming verifying key.
    pub new_key: VerifyingKey,
    /// Predecessor's signature authorizing the handover.
    pub authorization: [u8; 64],
}

impl KeyRotation {
    /// The message a predecessor signs to authorize `new_key` from
    /// `from_ledger_epoch`.
    #[must_use]
    pub fn signing_message(from_ledger_epoch: u64, new_key: &VerifyingKey) -> Vec<u8> {
        let mut msg = Vec::with_capacity(ROTATE_TAG.len() + 8 + 32);
        msg.extend_from_slice(ROTATE_TAG);
        msg.extend_from_slice(&from_ledger_epoch.to_le_bytes());
        msg.extend_from_slice(new_key.as_bytes());
        msg
    }
}

/// A key-rotation chain: a genesis key trusted out-of-band, followed by
/// successor keys each authorized by its predecessor.
#[derive(Debug, Clone)]
pub struct Keyring {
    /// The genesis (root) verifying key, active from epoch 0.
    pub root: VerifyingKey,
    /// Successor rotations, in ascending `from_ledger_epoch` order.
    pub rotations: Vec<KeyRotation>,
}

impl Keyring {
    /// A keyring with only the root key (no rotations).
    #[must_use]
    pub fn new(root: VerifyingKey) -> Self {
        Self {
            root,
            rotations: Vec::new(),
        }
    }

    /// Append an (already authorized) rotation.
    pub fn push(&mut self, rotation: KeyRotation) {
        self.rotations.push(rotation);
    }

    /// The verifying key active at `ledger_epoch`: the latest rotation
    /// whose `from_ledger_epoch <= ledger_epoch`, or the root.
    #[must_use]
    pub fn active_key_at(&self, ledger_epoch: u64) -> VerifyingKey {
        let mut key = self.root;
        for r in &self.rotations {
            if r.from_ledger_epoch <= ledger_epoch {
                key = r.new_key;
            } else {
                break;
            }
        }
        key
    }
}

/// Errors from [`verify_keyring`] / [`verify_ledger_with_keyring`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum KeyringError {
    /// Rotation epochs were not strictly increasing / not positive.
    #[error("rotation epochs out of order at index {index}")]
    BadRotationOrder {
        /// Index of the offending rotation.
        index: usize,
    },
    /// A rotation was not validly signed by its predecessor key.
    #[error("rotation at epoch {from_ledger_epoch} not authorized by its predecessor")]
    UnauthorizedRotation {
        /// The `from_ledger_epoch` of the rejected rotation.
        from_ledger_epoch: u64,
    },
    /// A ledger entry's signature failed under the key active at its
    /// epoch.
    #[error("ledger entry at epoch {epoch} fails under the active key")]
    BadEntrySignature {
        /// The ledger epoch whose signature failed.
        epoch: u64,
    },
    /// Ledger structural error (epoch order / chain link).
    #[error("ledger malformed at epoch {epoch}")]
    MalformedLedger {
        /// The ledger epoch at which the structure broke.
        epoch: u64,
    },
}

/// Verify the keyring's lineage: each rotation is authorized by the key
/// active immediately before it, and rotation epochs strictly increase
/// from a positive first epoch. An empty keyring (root only) is valid.
///
/// # Errors
///
/// Returns the first [`KeyringError`] encountered.
pub fn verify_keyring(keyring: &Keyring) -> Result<(), KeyringError> {
    let mut prev_key = keyring.root;
    let mut prev_epoch = 0u64;
    for (index, r) in keyring.rotations.iter().enumerate() {
        // Strictly increasing, and the first rotation must be after the
        // root's genesis epoch (0).
        if r.from_ledger_epoch == 0 || (index > 0 && r.from_ledger_epoch <= prev_epoch) {
            return Err(KeyringError::BadRotationOrder { index });
        }
        let msg = KeyRotation::signing_message(r.from_ledger_epoch, &r.new_key);
        let sig = Signature::from_bytes(&r.authorization);
        if prev_key.verify(&msg, &sig).is_err() {
            return Err(KeyringError::UnauthorizedRotation {
                from_ledger_epoch: r.from_ledger_epoch,
            });
        }
        prev_key = r.new_key;
        prev_epoch = r.from_ledger_epoch;
    }
    Ok(())
}

/// Verify an append-only ledger whose entries may span key rotations.
///
/// Checks the keyring lineage, then verifies the ledger chain (epoch
/// order + `prev_ledger_hash` links) and each entry's signature under
/// the key active at that entry's epoch ([`Keyring::active_key_at`]).
///
/// # Errors
///
/// Returns the first [`KeyringError`] encountered.
pub fn verify_ledger_with_keyring(
    ledger: &AnchorLedger,
    keyring: &Keyring,
) -> Result<(), KeyringError> {
    verify_keyring(keyring)?;
    let mut prev = [0u8; 32];
    for (i, e) in ledger.entries().iter().enumerate() {
        if e.epoch != i as u64 || e.prev_ledger_hash != prev {
            return Err(KeyringError::MalformedLedger { epoch: e.epoch });
        }
        let msg = LedgerEntry::signing_message(e.epoch, &e.head, &e.prev_ledger_hash);
        let sig = Signature::from_bytes(&e.signature);
        if keyring.active_key_at(e.epoch).verify(&msg, &sig).is_err() {
            return Err(KeyringError::BadEntrySignature { epoch: e.epoch });
        }
        prev = e.entry_hash();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::HeadAnchor;

    // Build a ledger whose first `split` epochs are signed by anchor A
    // and the rest by anchor B.
    fn split_ledger(a: &HeadAnchor, b: &HeadAnchor, total: u64, split: u64) -> AnchorLedger {
        let mut l = AnchorLedger::new();
        for i in 0..total {
            let signer = if i < split { a } else { b };
            signer.append_to_ledger(&mut l, [i as u8; 32]);
        }
        l
    }

    #[test]
    fn keyring_lineage_verifies_and_resolves_active_key() {
        let a = HeadAnchor::from_seed(&[1u8; 32]);
        let b = HeadAnchor::from_seed(&[2u8; 32]);
        let mut kr = Keyring::new(a.verifying_key());
        kr.push(a.authorize_rotation(&b.verifying_key(), 3));
        assert_eq!(verify_keyring(&kr), Ok(()));
        assert_eq!(kr.active_key_at(0), a.verifying_key());
        assert_eq!(kr.active_key_at(2), a.verifying_key());
        assert_eq!(kr.active_key_at(3), b.verifying_key());
        assert_eq!(kr.active_key_at(99), b.verifying_key());
    }

    #[test]
    fn ledger_spanning_a_rotation_verifies_under_the_keyring() {
        let a = HeadAnchor::from_seed(&[1u8; 32]);
        let b = HeadAnchor::from_seed(&[2u8; 32]);
        let ledger = split_ledger(&a, &b, 6, 3); // epochs 0-2 by A, 3-5 by B
        let mut kr = Keyring::new(a.verifying_key());
        kr.push(a.authorize_rotation(&b.verifying_key(), 3));
        assert_eq!(verify_ledger_with_keyring(&ledger, &kr), Ok(()));
    }

    #[test]
    fn single_key_verification_fails_across_a_rotation() {
        // The same spanning ledger does NOT verify under A alone — the
        // post-rotation entries were signed by B. This is why a keyring
        // is needed once keys rotate.
        use crate::ledger::verify_ledger;
        let a = HeadAnchor::from_seed(&[1u8; 32]);
        let b = HeadAnchor::from_seed(&[2u8; 32]);
        let ledger = split_ledger(&a, &b, 6, 3);
        assert!(verify_ledger(&ledger, &a.verifying_key()).is_err());
    }

    #[test]
    fn forged_successor_without_predecessor_is_rejected() {
        // An attacker mints key C and tries to insert it WITHOUT the
        // predecessor authorizing it — they sign the rotation with C
        // itself (or any non-predecessor key).
        let a = HeadAnchor::from_seed(&[1u8; 32]);
        let c = HeadAnchor::from_seed(&[9u8; 32]);
        let mut kr = Keyring::new(a.verifying_key());
        // C "authorizes itself" instead of A authorizing it.
        kr.push(c.authorize_rotation(&c.verifying_key(), 3));
        assert_eq!(
            verify_keyring(&kr),
            Err(KeyringError::UnauthorizedRotation {
                from_ledger_epoch: 3
            })
        );
    }

    #[test]
    fn retired_key_cannot_sign_new_epochs() {
        // After rotating A -> B at epoch 3, A is compromised. The
        // attacker (holding A) signs epochs 3-5 with A. Under the
        // keyring those epochs must verify under B, so A's signatures
        // fail: compromise of a retired key cannot forge new history.
        let a = HeadAnchor::from_seed(&[1u8; 32]);
        let b = HeadAnchor::from_seed(&[2u8; 32]);
        let attacker_ledger = split_ledger(&a, &a, 6, 6); // ALL signed by A
        let mut kr = Keyring::new(a.verifying_key());
        kr.push(a.authorize_rotation(&b.verifying_key(), 3));
        assert_eq!(
            verify_ledger_with_keyring(&attacker_ledger, &kr),
            Err(KeyringError::BadEntrySignature { epoch: 3 })
        );
    }

    #[test]
    fn out_of_order_rotations_are_rejected() {
        let a = HeadAnchor::from_seed(&[1u8; 32]);
        let b = HeadAnchor::from_seed(&[2u8; 32]);
        let c = HeadAnchor::from_seed(&[3u8; 32]);
        let mut kr = Keyring::new(a.verifying_key());
        kr.push(a.authorize_rotation(&b.verifying_key(), 5));
        kr.push(b.authorize_rotation(&c.verifying_key(), 2)); // earlier than prev
        assert_eq!(
            verify_keyring(&kr),
            Err(KeyringError::BadRotationOrder { index: 1 })
        );
    }
}
