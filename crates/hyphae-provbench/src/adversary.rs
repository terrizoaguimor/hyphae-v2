// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! The adversary-capability matrix.
//!
//! The paper's threat model named two adversaries (store-only and
//! chain-aware). The protocol parameterises capability along
//! orthogonal axes so verifiable-generation systems can be compared
//! against the *same* graded attacker, and so the exact boundary of
//! the guarantee is visible in the result rather than asserted in
//! prose.
//!
//! Axes:
//! - **store access** — every adversary here has write access; a
//!   read-only or no-access adversary tampers nothing and is omitted.
//! - **chain knowledge** — [`ChainKnowledge::Naive`] rewrites records
//!   in place (links go stale); [`ChainKnowledge::ChainAware`]
//!   recomputes every hash forward and rewrites the persisted head.
//! - **key access** — whether the adversary holds the external anchor
//!   signing key. [`KeyAccess::SigningKey`] is the boundary: an
//!   attacker who holds the key can re-sign a forged head, so the
//!   anchor provides no protection — exactly the assumption the paper
//!   draws the guarantee around ("any attacker who does not hold the
//!   anchor signing key").
//!
//! A `ledger access` axis (append / equivocate) is part of the v1
//! spec but not exercised here: single-head anchoring already catches
//! rollback via head mismatch; non-equivocation across observers
//! needs an external append-only ledger and is future work (see
//! `README.md` and the paper's Future Work).

/// Whether the adversary understands the hash-chain construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainKnowledge {
    /// Rewrites records in place; leaves chain links stale.
    Naive,
    /// Recomputes the chain forward and rewrites the head.
    ChainAware,
}

/// Whether the adversary holds the external anchor signing key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAccess {
    /// Does not hold the anchor key (the realistic case).
    None,
    /// Holds the anchor key — can re-sign a forged head.
    SigningKey,
}

/// An adversary profile: a point in the capability space.
#[derive(Debug, Clone, Copy)]
pub struct Adversary {
    /// Stable name for tables/envelopes.
    pub name: &'static str,
    /// Chain knowledge axis.
    pub chain: ChainKnowledge,
    /// Key access axis.
    pub key: KeyAccess,
}

impl Adversary {
    /// Whether this adversary recomputes the chain.
    #[must_use]
    pub fn chain_aware(&self) -> bool {
        matches!(self.chain, ChainKnowledge::ChainAware)
    }

    /// Whether this adversary holds the anchor signing key.
    #[must_use]
    pub fn holds_key(&self) -> bool {
        matches!(self.key, KeyAccess::SigningKey)
    }
}

/// The three profiles run by default: the two from the paper plus the
/// boundary case where the anchor key is compromised.
#[must_use]
pub fn profiles() -> [Adversary; 3] {
    [
        Adversary {
            name: "store-only",
            chain: ChainKnowledge::Naive,
            key: KeyAccess::None,
        },
        Adversary {
            name: "chain-aware",
            chain: ChainKnowledge::ChainAware,
            key: KeyAccess::None,
        },
        Adversary {
            name: "chain-aware+key",
            chain: ChainKnowledge::ChainAware,
            key: KeyAccess::SigningKey,
        },
    ]
}
