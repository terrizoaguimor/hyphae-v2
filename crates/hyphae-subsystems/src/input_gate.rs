// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Input gate — dispatch and signature-based amplification.
//!
//! Collapses v1's `Thalamus + OlfactoryBulb`. Two roles, one
//! subsystem:
//!
//! - **Dispatch (Thalamus role).** Receive a fragment and pass it
//!   onward unchanged. The substrate's pathway routing carries the
//!   fragment from here to wherever the topology directs it.
//! - **Signature-based amplification (`OlfactoryBulb` role).** When
//!   the input matches one of the configured *signatures*, the
//!   fragment's saliency is amplified before dispatch. This is the
//!   modal-specific bypass channel from v1 — input that matches a
//!   known cue gets boosted on the way in, so the composer sees a
//!   more salient signal.
//!
//! In v1 the two roles lived in distinct subsystems; the M8 review
//! confirmed the OB was effectively an "un-gated second Thalamus"
//! with modality filtering externalised to the consumer. v2 folds
//! them together. The two distinct substrate entry points
//! (`Substrate::ingest` for general; a future `ingest_with_cue` for
//! signature-amplified) externalise the choice — same subsystem.

use hyphae_core::{
    CognitiveFragment, ConsolidationGateSignal, PayloadKind, Result, State, Subsystem, SubsystemId,
};
use serde::{Deserialize, Serialize};
use std::any::Any;

/// Saliency amplification cap. The gate boosts saliency by at most
/// this fraction over its input value, so a signature hit on a
/// fragment that arrived with saliency `0.5` ends at `0.8` (delta
/// `0.3`). The cap prevents a runaway-amplification path where
/// multiple signatures could push saliency to a clamp boundary.
pub const SIGNATURE_GAIN: f32 = 0.3;

/// Snapshot of [`InputGate`] state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct InputGateSnapshot {
    dispatched: u64,
    amplified: u64,
    signatures: Vec<String>,
}

/// Input gate subsystem.
///
/// Stateless dispatch with an optional configured set of signatures.
/// A signature is a lowercased substring; an input whose body
/// contains any signature gets its saliency boosted by
/// [`SIGNATURE_GAIN`] (clamped to `[0, 1]`).
#[derive(Debug, Default)]
pub struct InputGate {
    /// Lifetime count of fragments dispatched (any kind).
    dispatched: u64,
    /// Lifetime count of fragments whose saliency was amplified by
    /// a signature hit.
    amplified: u64,
    /// Configured signatures. Lowercase substrings.
    signatures: Vec<String>,
}

impl InputGate {
    /// Construct a gate with no signatures configured. Inputs pass
    /// through unchanged.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a signature. Repeated calls accumulate; signatures are
    /// matched in insertion order. Case-insensitive — the gate
    /// lowercases at match-time so callers may register signatures
    /// in any case.
    pub fn register_signature(&mut self, signature: impl Into<String>) {
        self.signatures.push(signature.into().to_lowercase());
    }

    /// Number of dispatched fragments (lifetime).
    #[must_use]
    pub fn dispatched(&self) -> u64 {
        self.dispatched
    }

    /// Number of fragments whose saliency was amplified by a
    /// signature hit (lifetime).
    #[must_use]
    pub fn amplified(&self) -> u64 {
        self.amplified
    }

    /// Does the input body match any configured signature?
    fn matches_signature(&self, body: &str) -> bool {
        if self.signatures.is_empty() {
            return false;
        }
        let lower = body.to_lowercase();
        self.signatures.iter().any(|sig| lower.contains(sig))
    }
}

impl Subsystem for InputGate {
    fn id(&self) -> SubsystemId {
        SubsystemId::InputGate
    }

    fn process(
        &mut self,
        mut fragment: CognitiveFragment,
        _incoming: PayloadKind,
        _state: State,
    ) -> Result<Vec<CognitiveFragment>> {
        self.dispatched += 1;
        let body = fragment_body(&fragment);
        if self.matches_signature(body) {
            fragment.saliency = (fragment.saliency + SIGNATURE_GAIN).clamp(0.0, 1.0);
            self.amplified += 1;
        }
        fragment.provenance.source_subsystem = "input-gate".to_string();
        Ok(vec![fragment])
    }

    fn checkpoint(&self) -> Result<Vec<u8>> {
        let snap = InputGateSnapshot {
            dispatched: self.dispatched,
            amplified: self.amplified,
            signatures: self.signatures.clone(),
        };
        bincode::serialize(&snap).map_err(|e| {
            hyphae_core::HyphaeError::Other(format!("input-gate checkpoint serialise: {e}"))
        })
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<()> {
        let snap: InputGateSnapshot = bincode::deserialize(bytes).map_err(|e| {
            hyphae_core::HyphaeError::Other(format!("input-gate restore deserialise: {e}"))
        })?;
        self.dispatched = snap.dispatched;
        self.amplified = snap.amplified;
        self.signatures = snap.signatures;
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn consolidation_gate_signal(&self) -> ConsolidationGateSignal {
        // The gate is purely dispatch — never blocks consolidation.
        ConsolidationGateSignal::Allow
    }
}

/// Extract the body text from a fragment for signature matching.
/// Returns the empty string for variants without a body field.
fn fragment_body(fragment: &CognitiveFragment) -> &str {
    use hyphae_core::FragmentContent;
    match &fragment.content {
        FragmentContent::Episode { body, .. }
        | FragmentContent::Belief { body, .. }
        | FragmentContent::Goal { body, .. }
        | FragmentContent::Observation { body }
        | FragmentContent::Reflection { body, .. }
        | FragmentContent::Journal { body, .. } => body,
        FragmentContent::Reference { uri } => uri,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyphae_core::{FragmentContent, FragmentId};

    fn obs(body: &str) -> CognitiveFragment {
        let mut f = CognitiveFragment::new(
            FragmentContent::Observation {
                body: body.to_string(),
            },
            "external",
        );
        f.id = FragmentId::new();
        f.saliency = 0.5;
        f
    }

    #[test]
    fn dispatches_unchanged_without_signatures() {
        let mut gate = InputGate::new();
        let emissions = gate
            .process(obs("hello world"), PayloadKind::Encoding, State::Encoding)
            .unwrap();
        assert_eq!(emissions.len(), 1);
        assert!((emissions[0].saliency - 0.5).abs() < f32::EPSILON);
        assert_eq!(gate.dispatched(), 1);
        assert_eq!(gate.amplified(), 0);
    }

    #[test]
    fn amplifies_when_signature_matches() {
        let mut gate = InputGate::new();
        gate.register_signature("EXAMPLE");
        let emissions = gate
            .process(
                obs("the example project status update"),
                PayloadKind::Encoding,
                State::Encoding,
            )
            .unwrap();
        assert_eq!(emissions.len(), 1);
        let s = emissions[0].saliency;
        assert!(s > 0.5, "saliency should be boosted by SIGNATURE_GAIN");
        assert!(s <= 1.0, "saliency must stay within [0,1]");
        assert_eq!(gate.amplified(), 1);
    }

    #[test]
    fn signature_match_is_case_insensitive() {
        let mut gate = InputGate::new();
        gate.register_signature("Hyphae");
        let emissions = gate
            .process(obs("HYPHAE"), PayloadKind::Encoding, State::Encoding)
            .unwrap();
        assert!(emissions[0].saliency > 0.5);
    }

    #[test]
    fn saliency_clamps_at_one() {
        let mut gate = InputGate::new();
        gate.register_signature("urgent");
        let mut frag = obs("urgent request");
        frag.saliency = 0.9; // boost would push to 1.2
        let emissions = gate
            .process(frag, PayloadKind::Encoding, State::Encoding)
            .unwrap();
        assert!((emissions[0].saliency - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn checkpoint_restore_round_trips_state() {
        let mut gate = InputGate::new();
        gate.register_signature("alpha");
        gate.register_signature("beta");
        for _ in 0..3 {
            gate.process(obs("plain"), PayloadKind::Encoding, State::Encoding)
                .unwrap();
        }
        gate.process(obs("alpha event"), PayloadKind::Encoding, State::Encoding)
            .unwrap();
        let bytes = gate.checkpoint().unwrap();

        let mut restored = InputGate::new();
        restored.restore(&bytes).unwrap();
        assert_eq!(restored.dispatched(), gate.dispatched());
        assert_eq!(restored.amplified(), gate.amplified());
        // Restored gate still matches the persisted signatures.
        let emissions = restored
            .process(obs("alpha"), PayloadKind::Encoding, State::Encoding)
            .unwrap();
        assert!(emissions[0].saliency > 0.5);
    }
}
