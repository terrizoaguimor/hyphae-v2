// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Audit-entry construction for the substrate's shared SHA-256 hash
//! chain.
//!
//! Every ethics evaluation produces one audit entry on the
//! substrate's `hyphae-storage::Journal`. The entry's `event_kind`
//! is `"audit_ethics_evaluation"` (matching the
//! [`hyphae_core::JournalEntryType::AuditEthicsEvaluation`] tag).
//! The payload is a serialised [`AuditEntryPayload`] carrying the
//! report metadata — profile id and version, classification,
//! cvar score, hashed content fingerprint of the input.
//!
//! Content fingerprint, not raw content. The audit log is meant to
//! be auditable; storing raw evaluated content would defeat any
//! downstream privacy posture. The fingerprint lets an auditor
//! correlate two evaluations of the same input without seeing it.

use crate::taxonomy::TaxonomyCategory;
use serde::{Deserialize, Serialize};

/// Audit-entry payload serialised onto the journal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEntryPayload {
    /// The profile id that produced this evaluation.
    pub profile_id: String,
    /// The profile version that produced this evaluation.
    pub profile_version: String,
    /// SHA-256 fingerprint of the input, hex-lowercase.
    pub content_fingerprint: String,
    /// Categories that surfaced with non-zero non-suppressed
    /// confidence.
    pub flagged_categories: Vec<TaxonomyCategory>,
    /// `CVaR` score at evaluation time.
    pub cvar_score: f32,
    /// `true` when a categorical hard rule fired in this evaluation.
    pub categorical_fired: bool,
    /// The actor that triggered the evaluation (from
    /// [`hyphae_core::ActorContext`]).
    pub actor_id: String,
    /// The actor's requested operation scope.
    pub actor_scope: String,
}

/// The canonical `event_kind` string for ethics audit entries.
/// Matches [`hyphae_core::JournalEntryType::AuditEthicsEvaluation`]'s
/// `snake_case` serde tag.
pub const ETHICS_AUDIT_EVENT_KIND: &str = "audit_ethics_evaluation";

/// Compute a hex-lowercase SHA-256 fingerprint of a byte slice.
/// Defined here (rather than reused from `hyphae-storage::Journal`)
/// because the journal hashes whole entries; ethics hashes input
/// content only. Different scope, same algorithm.
#[must_use]
pub fn content_fingerprint(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        // SAFETY: writes to a String never fail.
        write!(hex, "{b:02x}").expect("writing to String");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_deterministic() {
        let a = content_fingerprint(b"hello world");
        let b = content_fingerprint(b"hello world");
        assert_eq!(a, b);
    }

    #[test]
    fn fingerprint_differs_for_different_inputs() {
        let a = content_fingerprint(b"hello");
        let b = content_fingerprint(b"world");
        assert_ne!(a, b);
    }

    #[test]
    fn fingerprint_is_hex_lowercase_64_chars() {
        let f = content_fingerprint(b"any input");
        assert_eq!(f.len(), 64);
        assert!(
            f.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
    }

    #[test]
    fn audit_payload_round_trips_through_bincode() {
        let payload = AuditEntryPayload {
            profile_id: "baseline".to_string(),
            profile_version: "0.1.0".to_string(),
            content_fingerprint: content_fingerprint(b"sample"),
            flagged_categories: vec![TaxonomyCategory::Hate, TaxonomyCategory::Cbrn],
            cvar_score: 0.42,
            categorical_fired: false,
            actor_id: "user:mario".to_string(),
            actor_scope: "memory:write".to_string(),
        };
        let bytes = bincode::serialize(&payload).unwrap();
        let restored: AuditEntryPayload = bincode::deserialize(&bytes).unwrap();
        assert_eq!(payload, restored);
    }
}
