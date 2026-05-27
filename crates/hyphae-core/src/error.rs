// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Error types for the Hyphae core.

use thiserror::Error;

/// Result type alias for Hyphae operations.
pub type Result<T> = std::result::Result<T, HyphaeError>;

/// The unified error type for Hyphae core operations.
#[derive(Debug, Error)]
pub enum HyphaeError {
    /// A pathway transmission failed due to type or routing error.
    #[error("pathway error: {0}")]
    Pathway(String),

    /// A subsystem rejected a fragment or operation.
    #[error("subsystem error: {0}")]
    Subsystem(String),

    /// A state transition was rejected as invalid.
    #[error("invalid state transition from {from:?} to {to:?}")]
    InvalidStateTransition {
        /// The current state.
        from: crate::State,
        /// The requested next state.
        to: crate::State,
    },

    /// A storage operation failed.
    #[error("storage error: {0}")]
    Storage(String),

    /// Journal hash chain integrity check failed.
    #[error("journal integrity violation: {0}")]
    JournalIntegrity(String),

    /// A fragment failed validation (e.g., out-of-range saliency).
    #[error("invalid fragment: {0}")]
    InvalidFragment(String),

    /// An unspecified error.
    #[error("hyphae error: {0}")]
    Other(String),
}
