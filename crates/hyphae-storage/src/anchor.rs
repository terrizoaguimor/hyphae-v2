// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! External head anchoring (ADR-0032).
//!
//! The journal's hash chain (see [`crate::journal`]) gives
//! tamper-evidence against a store-level attacker who edits a record
//! but does not reimplement the chain logic: any in-place edit breaks
//! the recomputed chain. It does *not* defend against a *chain-aware*
//! attacker who recomputes every hash forward from the edit and
//! rewrites the persisted chain head, because in the bare scheme the
//! head lives in the same store as the entries. The entire security
//! of the scheme then reduces to the integrity of one value: the
//! head.
//!
//! This module anchors that one value outside the store's write
//! scope. A [`HeadAnchor`] holds an Ed25519 signing key --- in a real
//! deployment an offline signer, HSM, or audit-service key the store
//! process never sees --- and signs the chain head. Verification
//! ([`verify_anchored_head`]) checks the signature against the
//! anchor's public [`VerifyingKey`]. A chain-aware attacker who
//! rewrites the head to a recomputed-but-forged value cannot produce
//! a signature that verifies, because they do not hold the signing
//! key. The guarantee thus strengthens from *"tamper-evident against
//! an attacker who cannot write the head"* (vacuous when the head
//! shares the store) to *"tamper-evident against an attacker who does
//! not hold the anchor signing key"* (realistic: the key genuinely
//! lives elsewhere, whereas the head necessarily lives in the store).
//!
//! For reproducible experiments and tests the anchor is constructed
//! from a fixed 32-byte seed; production callers derive the key from
//! a KMS/HSM and never materialise it in the store process.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

/// A signed attestation of a chain head: the 64-byte Ed25519
/// signature over the 32-byte head digest. Publish this (and the
/// [`VerifyingKey`]) to an append-only external location; an auditor
/// re-derives the head from the journal and checks it here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnchoredHead {
    /// The head digest that was signed.
    pub head: [u8; 32],
    /// Ed25519 signature over `head`, serialised.
    pub signature: [u8; 64],
}

/// Holds the anchor signing key. In deployment this lives outside the
/// store process (offline signer / HSM / audit service); the store
/// only ever receives [`AnchoredHead`]s, never the key.
pub struct HeadAnchor {
    signing_key: SigningKey,
}

impl HeadAnchor {
    /// Construct from a fixed 32-byte seed. Deterministic --- used for
    /// reproducible experiments and tests. Production callers obtain
    /// the key material from a KMS/HSM instead.
    #[must_use]
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self {
            signing_key: SigningKey::from_bytes(seed),
        }
    }

    /// The public verifying key. Distribute this to auditors; it does
    /// not permit forging signatures.
    #[must_use]
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Sign a chain head, producing an [`AnchoredHead`] to publish
    /// outside the store's write scope.
    #[must_use]
    pub fn anchor(&self, head: [u8; 32]) -> AnchoredHead {
        let signature = self.signing_key.sign(&head);
        AnchoredHead {
            head,
            signature: signature.to_bytes(),
        }
    }
}

/// Verify that `current_head` (re-derived from the journal) matches an
/// [`AnchoredHead`] that was signed by the holder of `verifying_key`.
///
/// Returns `true` only when (a) the anchored signature is valid under
/// `verifying_key` over the head it claims, and (b) that head equals
/// the journal's current head. A chain-aware attacker who recomputed
/// the chain and rewrote the persisted head to `current_head'` fails
/// (b) against any anchor they did not re-sign, and cannot satisfy
/// (a) for `current_head'` without the signing key.
#[must_use]
pub fn verify_anchored_head(
    current_head: &[u8; 32],
    anchored: &AnchoredHead,
    verifying_key: &VerifyingKey,
) -> bool {
    if &anchored.head != current_head {
        return false;
    }
    let sig = Signature::from_bytes(&anchored.signature);
    verifying_key.verify(&anchored.head, &sig).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: [u8; 32] = [7u8; 32];

    #[test]
    fn valid_anchor_verifies() {
        let anchor = HeadAnchor::from_seed(&SEED);
        let head = [42u8; 32];
        let anchored = anchor.anchor(head);
        assert!(verify_anchored_head(
            &head,
            &anchored,
            &anchor.verifying_key()
        ));
    }

    #[test]
    fn rewritten_head_fails_verification() {
        // Attacker recomputes the chain and rewrites the persisted
        // head to a different value; the anchor (signed over the real
        // head) no longer matches.
        let anchor = HeadAnchor::from_seed(&SEED);
        let real_head = [42u8; 32];
        let anchored = anchor.anchor(real_head);
        let forged_head = [99u8; 32];
        assert!(!verify_anchored_head(
            &forged_head,
            &anchored,
            &anchor.verifying_key()
        ));
    }

    #[test]
    fn attacker_cannot_forge_signature_without_key() {
        // The attacker holds a DIFFERENT key (they do not have the
        // audit key) and re-signs their forged head. Verification
        // under the legitimate verifying key fails.
        let audit = HeadAnchor::from_seed(&SEED);
        let attacker = HeadAnchor::from_seed(&[1u8; 32]);
        let forged_head = [99u8; 32];
        let attacker_anchor = attacker.anchor(forged_head);
        // Attacker's signature is valid under the attacker's key...
        assert!(verify_anchored_head(
            &forged_head,
            &attacker_anchor,
            &attacker.verifying_key()
        ));
        // ...but not under the legitimate audit key the auditor trusts.
        assert!(!verify_anchored_head(
            &forged_head,
            &attacker_anchor,
            &audit.verifying_key()
        ));
    }
}
