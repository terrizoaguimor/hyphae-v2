// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Typed pathways between subsystems.
//!
//! Pathways are unidirectional, type-labelled channels between two
//! subsystems. A subsystem cannot invoke another subsystem's logic
//! directly — it emits a payload on a defined pathway and the
//! substrate routes the result. Pathways MAY be state-gated so the
//! substrate enforces which channels are active in which global
//! state. See `docs/rfc/v1-living.md` §1.4.

use crate::CognitiveFragment;
use serde::{Deserialize, Serialize};

/// Unique identifier for a pathway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PathwayId(pub u32);

/// The kind of payload a pathway carries, per Active Inference
/// semantics. See `docs/rfc/v1-living.md` §1.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PayloadKind {
    /// Top-down prediction (e.g., `composer` → `input-gate` executive
    /// attention).
    TopDownPrediction,
    /// Bottom-up prediction error (e.g., `predictive` → limbic
    /// broadcast).
    BottomUpPredictionError,
    /// Modulation (e.g., precision weighting, reward reinforcement).
    Modulation,
    /// Encoding / storage payload (raw content moving toward storage).
    Encoding,
    /// Housekeeping (heartbeats, persistence flushes, integrity
    /// checks).
    Housekeeping,
}

/// A typed pathway between two subsystems.
///
/// Pathways are unidirectional channels with a type-constrained
/// payload. The cognitive transformation a pathway implies is
/// performed by the destination subsystem; a [`DirectPathway`] is
/// the typed conduit itself.
pub trait Pathway: Send + Sync {
    /// Unique identifier for this pathway.
    fn id(&self) -> PathwayId;

    /// The kind of payload this pathway carries.
    fn kind(&self) -> PayloadKind;

    /// The source subsystem identifier.
    fn source(&self) -> crate::SubsystemId;

    /// The destination subsystem identifier.
    fn destination(&self) -> crate::SubsystemId;

    /// Is this pathway state-gated (active only in certain global
    /// states)? Returns the set of states in which this pathway is
    /// active. `None` means active in all states.
    fn state_gate(&self) -> Option<&[crate::State]>;

    /// Transmit a fragment along this pathway. Returns the (possibly
    /// modified) fragment that the destination subsystem will receive.
    ///
    /// # Errors
    ///
    /// Returns an error if the payload kind is incompatible with the
    /// pathway or transmission is otherwise rejected.
    fn transmit(&self, fragment: CognitiveFragment) -> crate::Result<CognitiveFragment>;
}

/// A simple unidirectional pathway that relays fragments unchanged.
///
/// The cognitive transformation a pathway implies is performed by the
/// destination subsystem; a `DirectPathway` is the typed conduit
/// itself. On transmission it stamps the fragment's provenance with
/// the pathway traversed.
#[derive(Debug, Clone)]
pub struct DirectPathway {
    id: PathwayId,
    kind: PayloadKind,
    source: crate::SubsystemId,
    destination: crate::SubsystemId,
    gate: Option<Vec<crate::State>>,
}

impl DirectPathway {
    /// Create a pathway active in every global state.
    #[must_use]
    pub fn new(
        id: PathwayId,
        kind: PayloadKind,
        source: crate::SubsystemId,
        destination: crate::SubsystemId,
    ) -> Self {
        Self {
            id,
            kind,
            source,
            destination,
            gate: None,
        }
    }

    /// Create a pathway active only in the given global states.
    #[must_use]
    pub fn state_gated(
        id: PathwayId,
        kind: PayloadKind,
        source: crate::SubsystemId,
        destination: crate::SubsystemId,
        gate: Vec<crate::State>,
    ) -> Self {
        Self {
            id,
            kind,
            source,
            destination,
            gate: Some(gate),
        }
    }
}

impl Pathway for DirectPathway {
    fn id(&self) -> PathwayId {
        self.id
    }

    fn kind(&self) -> PayloadKind {
        self.kind
    }

    fn source(&self) -> crate::SubsystemId {
        self.source
    }

    fn destination(&self) -> crate::SubsystemId {
        self.destination
    }

    fn state_gate(&self) -> Option<&[crate::State]> {
        self.gate.as_deref()
    }

    fn transmit(&self, mut fragment: CognitiveFragment) -> crate::Result<CognitiveFragment> {
        fragment.provenance.source_pathway =
            Some(format!("{:?}->{:?}", self.source, self.destination));
        Ok(fragment)
    }
}
