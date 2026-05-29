// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Append-only anchor ledger (ADR-0033).
//!
//! [`crate::anchor`] (ADR-0032) signs a *single* chain head, which
//! catches a chain-aware attacker who rewrites the head: the signature
//! over the legitimate head no longer matches the forged one. But a
//! single signature pins only *a* valid head, not *the latest* one. It
//! leaves two gaps:
//!
//! - **Freshness.** Every head the journal ever had was, at its time, a
//!   legitimately signed head. An attacker who rolls the store back to
//!   an earlier state and replays the matching *stale-but-valid* anchor
//!   passes single-signature verification — the signature is genuine
//!   and matches the (rolled-back) head. Nothing in a lone signature
//!   says "this head is superseded".
//! - **Non-equivocation.** A single signature cannot stop the store
//!   from presenting *different* consistent histories to different
//!   auditors; each history can carry its own validly-signed head.
//!
//! This module closes both by publishing anchors to an **append-only,
//! itself-hash-chained ledger** (the standard transparency-log shape:
//! Haber–Stornetta 1991, Certificate Transparency / RFC 6962). Each
//! [`LedgerEntry`] carries a monotonically increasing `epoch`, the
//! journal head at that epoch, the hash of the previous ledger entry
//! (chaining the ledger itself), and an Ed25519 signature over the
//! three. An auditor then checks the journal's current head against the
//! **latest** entry of a verified ledger ([`verify_fresh_head`]), so a
//! rolled-back head — even with a genuine stale anchor — is rejected;
//! and two ledger views are required to be prefix-consistent
//! ([`ledgers_consistent`]), so equivocation is detectable.
//!
//! The signing key still lives outside the store (see [`HeadAnchor`]);
//! the ledger is what gets published to the external append-only
//! medium. What remains out of scope is an *external witness* of the
//! ledger head (a timestamp authority / gossiped tree head) that would
//! also stop a store from withholding entries it never published — a
//! deployment concern, noted in ADR-0033.
//!
//! [`HeadAnchor`]: crate::anchor::HeadAnchor

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// One entry in the append-only anchor ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedgerEntry {
    /// Monotonic position in the ledger (0 for genesis).
    pub epoch: u64,
    /// The journal chain head anchored at this epoch.
    pub head: [u8; 32],
    /// Hash of the previous ledger entry, or zeros for genesis. Chains
    /// the ledger so it is itself append-only and tamper-evident.
    pub prev_ledger_hash: [u8; 32],
    /// Ed25519 signature over `(epoch ‖ head ‖ prev_ledger_hash)`.
    pub signature: [u8; 64],
}

impl LedgerEntry {
    /// The 72-byte message that is signed: `epoch ‖ head ‖
    /// prev_ledger_hash`. The signature deliberately does **not** cover
    /// itself; the ledger chain (below) binds the signature in.
    #[must_use]
    pub fn signing_message(epoch: u64, head: &[u8; 32], prev_ledger_hash: &[u8; 32]) -> [u8; 72] {
        let mut msg = [0u8; 72];
        msg[..8].copy_from_slice(&epoch.to_le_bytes());
        msg[8..40].copy_from_slice(head);
        msg[40..72].copy_from_slice(prev_ledger_hash);
        msg
    }

    /// This entry's hash, used as the next entry's `prev_ledger_hash`.
    /// Covers the signed message *and* the signature, so the whole
    /// entry is bound into the chain.
    #[must_use]
    pub fn entry_hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(LedgerEntry::signing_message(
            self.epoch,
            &self.head,
            &self.prev_ledger_hash,
        ));
        hasher.update(self.signature);
        hasher.finalize().into()
    }
}

/// An append-only ledger of signed chain heads. Held and published
/// outside the store's write scope; the store only ever receives
/// entries, never the signing key.
#[derive(Debug, Clone, Default)]
pub struct AnchorLedger {
    entries: Vec<LedgerEntry>,
}

impl AnchorLedger {
    /// An empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reconstruct a ledger from published entries. An auditor fetches
    /// the external append-only log and rebuilds it here to verify with
    /// [`verify_ledger`] / [`verify_fresh_head`]. Order is preserved as
    /// given; validity is not assumed — verify it.
    #[must_use]
    pub fn from_entries(entries: Vec<LedgerEntry>) -> Self {
        Self { entries }
    }

    /// All entries in append order.
    #[must_use]
    pub fn entries(&self) -> &[LedgerEntry] {
        &self.entries
    }

    /// Number of entries (the next epoch to be appended).
    #[must_use]
    pub fn len(&self) -> u64 {
        self.entries.len() as u64
    }

    /// Whether the ledger has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The most recent entry, if any.
    #[must_use]
    pub fn latest(&self) -> Option<&LedgerEntry> {
        self.entries.last()
    }

    /// The ledger's own head: the hash of the latest entry (a
    /// signed-tree-head analogue), or zeros when empty. This is the
    /// `prev_ledger_hash` the next append will chain to.
    #[must_use]
    pub fn head_hash(&self) -> [u8; 32] {
        self.entries
            .last()
            .map(LedgerEntry::entry_hash)
            .unwrap_or([0u8; 32])
    }

    /// Append a pre-built entry. Internal: [`HeadAnchor::append_to_ledger`]
    /// is the public constructor that signs.
    pub(crate) fn push(&mut self, entry: LedgerEntry) {
        self.entries.push(entry);
    }
}

/// Errors from [`verify_ledger`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum LedgerError {
    /// Epoch did not equal its position (entries must be 0,1,2,…).
    #[error("epoch out of order at position {position}: found epoch {found}")]
    BadEpoch {
        /// Position in the ledger.
        position: u64,
        /// The epoch actually stored.
        found: u64,
    },
    /// `prev_ledger_hash` did not chain the previous entry.
    #[error("broken ledger chain at epoch {epoch}")]
    BrokenChain {
        /// Epoch at which the chain link failed.
        epoch: u64,
    },
    /// The Ed25519 signature did not verify under the trusted key.
    #[error("invalid signature at epoch {epoch}")]
    BadSignature {
        /// Epoch whose signature failed.
        epoch: u64,
    },
}

/// Verify the ledger is well-formed under `verifying_key`: epochs are
/// `0,1,2,…`, each `prev_ledger_hash` chains the previous entry, and
/// every signature verifies. An empty ledger is vacuously valid.
///
/// # Errors
///
/// Returns the first [`LedgerError`] encountered.
pub fn verify_ledger(
    ledger: &AnchorLedger,
    verifying_key: &VerifyingKey,
) -> Result<(), LedgerError> {
    let mut prev = [0u8; 32];
    for (i, e) in ledger.entries().iter().enumerate() {
        let position = i as u64;
        if e.epoch != position {
            return Err(LedgerError::BadEpoch {
                position,
                found: e.epoch,
            });
        }
        if e.prev_ledger_hash != prev {
            return Err(LedgerError::BrokenChain { epoch: e.epoch });
        }
        let msg = LedgerEntry::signing_message(e.epoch, &e.head, &e.prev_ledger_hash);
        let sig = Signature::from_bytes(&e.signature);
        if verifying_key.verify(&msg, &sig).is_err() {
            return Err(LedgerError::BadSignature { epoch: e.epoch });
        }
        prev = e.entry_hash();
    }
    Ok(())
}

/// Freshness check: the journal's `current_head` must equal the head of
/// the **latest** entry of a valid ledger.
///
/// Returns `true` only when the ledger verifies under `verifying_key`
/// *and* its latest anchored head equals `current_head`. A rollback to
/// an earlier head fails even if the attacker replays that head's
/// genuine (but now superseded) anchor, because that anchor is no
/// longer the ledger's latest entry.
#[must_use]
pub fn verify_fresh_head(
    current_head: &[u8; 32],
    ledger: &AnchorLedger,
    verifying_key: &VerifyingKey,
) -> bool {
    if verify_ledger(ledger, verifying_key).is_err() {
        return false;
    }
    ledger.latest().is_some_and(|e| &e.head == current_head)
}

/// Non-equivocation check: two ledger views are consistent iff the
/// shorter is an exact prefix of the longer. A divergence at any epoch
/// means the store presented two different histories — equivocation.
///
/// (This compares the two views structurally; pin one of them to an
/// external witness / gossiped head to make the guarantee hold against
/// a store that withholds entries — see ADR-0033.)
#[must_use]
pub fn ledgers_consistent(a: &AnchorLedger, b: &AnchorLedger) -> bool {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    short
        .entries()
        .iter()
        .zip(long.entries().iter())
        .all(|(x, y)| x == y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::HeadAnchor;

    const SEED: [u8; 32] = [9u8; 32];

    /// Build a ledger of `n` epochs with distinct heads `[i; 32]`.
    fn ledger_of(anchor: &HeadAnchor, n: u64) -> AnchorLedger {
        let mut l = AnchorLedger::new();
        for i in 0..n {
            anchor.append_to_ledger(&mut l, [i as u8; 32]);
        }
        l
    }

    #[test]
    fn well_formed_ledger_verifies() {
        let anchor = HeadAnchor::from_seed(&SEED);
        let l = ledger_of(&anchor, 5);
        assert_eq!(verify_ledger(&l, &anchor.verifying_key()), Ok(()));
        assert_eq!(l.len(), 5);
        assert_eq!(l.latest().unwrap().epoch, 4);
    }

    #[test]
    fn fresh_head_accepts_latest_rejects_stale() {
        let anchor = HeadAnchor::from_seed(&SEED);
        let l = ledger_of(&anchor, 5);
        let vk = anchor.verifying_key();
        // Latest head accepted.
        assert!(verify_fresh_head(&[4u8; 32], &l, &vk));
        // A genuinely-anchored-but-stale earlier head is REJECTED — the
        // freshness gap a single signature leaves open.
        assert!(!verify_fresh_head(&[2u8; 32], &l, &vk));
    }

    #[test]
    fn tampering_an_entry_breaks_verification() {
        let anchor = HeadAnchor::from_seed(&SEED);
        let mut l = ledger_of(&anchor, 5);
        // Forge the head of a middle entry: its signature no longer
        // covers the new head.
        l.entries[2].head = [42u8; 32];
        assert_eq!(
            verify_ledger(&l, &anchor.verifying_key()),
            Err(LedgerError::BadSignature { epoch: 2 })
        );
    }

    #[test]
    fn reordering_breaks_the_chain() {
        let anchor = HeadAnchor::from_seed(&SEED);
        let mut l = ledger_of(&anchor, 5);
        l.entries.swap(1, 3);
        assert!(verify_ledger(&l, &anchor.verifying_key()).is_err());
    }

    #[test]
    fn truncation_is_a_consistent_prefix_but_loses_freshness() {
        let anchor = HeadAnchor::from_seed(&SEED);
        let full = ledger_of(&anchor, 5);
        // A truncated view (rollback) is still internally valid …
        let mut rolled = AnchorLedger::new();
        for e in &full.entries()[..3] {
            rolled.push(*e);
        }
        assert!(verify_ledger(&rolled, &anchor.verifying_key()).is_ok());
        // … and is a consistent prefix of the full ledger …
        assert!(ledgers_consistent(&rolled, &full));
        // … but an auditor holding the full ledger rejects the rolled
        // head as stale.
        assert!(!verify_fresh_head(
            &[2u8; 32],
            &full,
            &anchor.verifying_key()
        ));
    }

    #[test]
    fn equivocation_between_forked_ledgers_is_detected() {
        let anchor = HeadAnchor::from_seed(&SEED);
        let a = ledger_of(&anchor, 5);
        // A fork: same first two epochs, then a different head at epoch 2.
        let mut b = AnchorLedger::new();
        for e in &a.entries()[..2] {
            b.push(*e);
        }
        anchor.append_to_ledger(&mut b, [99u8; 32]); // diverges at epoch 2
        assert!(verify_ledger(&b, &anchor.verifying_key()).is_ok());
        assert!(
            !ledgers_consistent(&a, &b),
            "two histories diverging at an epoch is equivocation"
        );
    }

    #[test]
    fn prefix_views_are_consistent() {
        let anchor = HeadAnchor::from_seed(&SEED);
        let a = ledger_of(&anchor, 5);
        let mut prefix = AnchorLedger::new();
        for e in &a.entries()[..3] {
            prefix.push(*e);
        }
        assert!(ledgers_consistent(&prefix, &a));
        assert!(ledgers_consistent(&a, &prefix));
    }
}
