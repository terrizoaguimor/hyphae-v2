// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Global state machine for the Hyphae substrate.
//!
//! Five states with deterministic transitions. State-gated pathways
//! enforce which channels are active in which state at the substrate
//! level — violations are journal-logged integrity errors. See
//! `docs/rfc/v1-living.md` §1.6.

use serde::{Deserialize, Serialize};

/// The five global states of the Hyphae substrate.
///
/// State transitions are deterministic given inputs and are
/// themselves subject to journal logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum State {
    /// Default wakeful receptive state. `input-gate` tonic, encoding
    /// active.
    Encoding,
    /// Active retrieval. `episodic` in pattern-completion mode.
    Recall,
    /// Offline reorganisation. Sharp-wave-ripple-style replay active,
    /// `input-gate` in burst mode.
    Consolidation,
    /// Deep offline. Only the substrate's housekeeping path active.
    Dormancy,
    /// Integrity restoration after crash or detected inconsistency.
    /// The journal hash chain is verified during this state.
    Recovery,
}

impl State {
    /// Returns the set of valid next states from the current state.
    ///
    /// Transitions are governed by global signals and explicit
    /// triggers; this method describes the topology of allowed
    /// transitions.
    #[must_use]
    pub fn valid_transitions(self) -> &'static [State] {
        match self {
            Self::Encoding => &[
                Self::Recall,
                Self::Consolidation,
                Self::Dormancy,
                Self::Recovery,
            ],
            Self::Recall | Self::Dormancy => &[Self::Encoding, Self::Recovery],
            Self::Consolidation => &[Self::Encoding, Self::Dormancy, Self::Recovery],
            Self::Recovery => &[Self::Encoding],
        }
    }

    /// Is this an offline state (no external input processing)?
    #[must_use]
    pub const fn is_offline(self) -> bool {
        matches!(self, Self::Consolidation | Self::Dormancy | Self::Recovery)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_can_transition_to_all_other_states() {
        let next = State::Encoding.valid_transitions();
        assert!(next.contains(&State::Recall));
        assert!(next.contains(&State::Consolidation));
        assert!(next.contains(&State::Dormancy));
        assert!(next.contains(&State::Recovery));
    }

    #[test]
    fn recovery_returns_only_to_encoding() {
        assert_eq!(State::Recovery.valid_transitions(), &[State::Encoding]);
    }

    #[test]
    fn offline_states_classified_correctly() {
        assert!(!State::Encoding.is_offline());
        assert!(!State::Recall.is_offline());
        assert!(State::Consolidation.is_offline());
        assert!(State::Dormancy.is_offline());
        assert!(State::Recovery.is_offline());
    }
}
