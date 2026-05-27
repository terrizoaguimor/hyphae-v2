// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Actor context for substrate-level operations.
//!
//! The substrate is single-tenant by construction. Operations that
//! ingest, compose, or retrieve content thread an [`ActorContext`]
//! into the **gate → operate → audit** pattern: the ethics path
//! (`hyphae-ethics`) reads the actor identity to scope its
//! evaluation, and the journal audit entry records who-did-what for
//! later replay.
//!
//! The shape is intentionally minimal — `actor_id` (who) and
//! `scope` (what they asked to do). Richer authentication (RBAC,
//! AAL tiers, zero-knowledge envelopes) is a deployment concern that
//! lives outside the substrate; Hyphae's tool layer trusts whatever
//! [`ActorContext`] the caller hands it.

use serde::{Deserialize, Serialize};

/// Who is asking, and what they asked to do.
///
/// Constructed by the caller and threaded into every substrate
/// operation that triggers the five-point ethics coverage. The
/// substrate never invents an [`ActorContext`] for the caller —
/// system-internal calls use [`ActorContext::system`] explicitly so
/// the audit trail records that the call was system-originated, not
/// a missing or unknown actor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorContext {
    /// Identifier for the requesting actor. Free-form — the substrate
    /// does not constrain shape; common conventions are
    /// `"agent:claude-opus-4-7"`, `"system:consolidation"`,
    /// `"user:mario"`.
    pub actor_id: String,
    /// The requested operation scope. Free-form — used by the ethics
    /// evaluation and recorded in the audit entry. Common conventions
    /// are `"memory:write"`, `"memory:read"`, `"journal:redact"`,
    /// `"learning:parameter_update"`.
    pub scope: String,
}

impl ActorContext {
    /// Construct an actor context.
    #[must_use]
    pub fn new(actor_id: impl Into<String>, scope: impl Into<String>) -> Self {
        Self {
            actor_id: actor_id.into(),
            scope: scope.into(),
        }
    }

    /// System-internal actor — used by substrate operations that
    /// originate from the runtime itself (e.g. Consolidation-triggered
    /// cleanup, learning loop parameter updates) and still want the
    /// audit trail to name the originator.
    #[must_use]
    pub fn system() -> Self {
        Self::new("system", "system:internal")
    }
}
