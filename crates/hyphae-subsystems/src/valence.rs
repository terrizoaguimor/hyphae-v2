// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Valence — affective stamp and decay modulation.
//!
//! Collapses v1's `Amygdala + BNST`. Two output axes, separated at
//! the type level so they cannot collide:
//!
//! - **Valence** in `[-1.0, +1.0]` on `fragment.valence` — phasic.
//!   Emitted on every input via a lexicon-driven stamp.
//! - **Durability** on `fragment.decay_rate` — sustained. Increases
//!   when the running affective magnitude is high (charged context
//!   slows the decay of encoded fragments); relaxes back toward the
//!   baseline when the context cools.
//!
//! Corrects v1's M4b finding where both subsystems modified
//! `saliency` and cancelled each other by clamp. v2 splits the
//! axes — Layer 1 valence on the `valence` field, sustained
//! durability on `decay_rate`. No overlap.
//!
//! The lexicon is **English only** for v0.1 per RFC §9. The
//! `hyphae-ethics` Layer A taxonomy is a richer surface; v0.1
//! keeps this stamp deliberately small so it stays auditable —
//! the integrator typically supplements with ethics-driven
//! salience-weight learning (the loop's `ValenceSalienceWeight`
//! parameter target).

use hyphae_core::{
    CognitiveFragment, ConsolidationGateSignal, FragmentContent, PayloadKind, Result, State,
    Subsystem, SubsystemId,
};
use serde::{Deserialize, Serialize};
use std::any::Any;

/// Time constant of the sustained-magnitude low-pass. Lower = more
/// inertia; higher = faster response. v1's M4b empirically lowered
/// its BNST equivalent to `0.1`; v2 inherits that calibration as
/// the seed.
pub const SUSTAINED_ALPHA: f32 = 0.1;

/// Decay-rate multiplier applied when the sustained magnitude is
/// at its maximum (`1.0`). A decay-rate of `0.001/s` at neutral
/// becomes `0.001 / (1 + DECAY_DAMPENING * sustained)` at full
/// magnitude — so a saturated context multiplies durability by
/// roughly `(1 + DECAY_DAMPENING)`. v0.1 default `2.0` halves the
/// decay rate at saturation.
pub const DECAY_DAMPENING: f32 = 2.0;

/// Consolidation veto threshold. When the running magnitude exceeds
/// this value, the subsystem refuses the transition into
/// [`State::Consolidation`] — sustained extreme affect is not the
/// quiescent state SHY-style consolidation needs.
pub const CONSOLIDATION_BLOCK_MAGNITUDE: f32 = 0.8;

/// English-only Layer 1 lexicon for the affective stamp. Whole-word
/// matches against the lowercased fragment body. Positive terms
/// push valence toward `+1`; negative push toward `-1`.
const POSITIVE_LEXICON: &[&str] = &[
    "love",
    "joy",
    "happy",
    "win",
    "delight",
    "wonderful",
    "excellent",
    "great",
    "success",
    "celebrate",
    "kind",
    "warm",
    "good",
    "yes",
    "thanks",
];

const NEGATIVE_LEXICON: &[&str] = &[
    "hate", "fear", "angry", "sad", "loss", "fail", "terrible", "awful", "danger", "pain", "hurt",
    "cruel", "bad", "no", "panic",
];

/// Snapshot of [`Valence`] state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ValenceSnapshot {
    stamps: u64,
    sustained_magnitude: f32,
}

/// Valence subsystem.
#[derive(Debug, Default)]
pub struct Valence {
    /// Lifetime count of fragments stamped.
    stamps: u64,
    /// Running low-pass of the absolute valence — the "how charged
    /// is the current context" signal that drives the durability
    /// modulation and the consolidation gate.
    sustained_magnitude: f32,
}

impl Valence {
    /// Construct a valence subsystem with neutral sustained
    /// magnitude.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Lifetime stamps count.
    #[must_use]
    pub fn stamps(&self) -> u64 {
        self.stamps
    }

    /// Current running sustained magnitude in `[0.0, 1.0]`.
    #[must_use]
    pub fn sustained_magnitude(&self) -> f32 {
        self.sustained_magnitude
    }

    /// Compute the Layer 1 valence stamp for a body text. Whole-word
    /// matches against the positive / negative lexicons, signed
    /// difference normalised by hit count, clamped to `[-1, +1]`.
    fn stamp_for(body: &str) -> f32 {
        let lower = body.to_lowercase();
        let pos = count_whole_word_matches(&lower, POSITIVE_LEXICON);
        let neg = count_whole_word_matches(&lower, NEGATIVE_LEXICON);
        if pos == 0 && neg == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        let pos_f = pos as f32;
        #[allow(clippy::cast_precision_loss)]
        let neg_f = neg as f32;
        let total = pos_f + neg_f;
        ((pos_f - neg_f) / total).clamp(-1.0, 1.0)
    }
}

impl Subsystem for Valence {
    fn id(&self) -> SubsystemId {
        SubsystemId::Valence
    }

    fn process(
        &mut self,
        mut fragment: CognitiveFragment,
        _incoming: PayloadKind,
        _state: State,
    ) -> Result<Vec<CognitiveFragment>> {
        self.stamps += 1;

        let body = match &fragment.content {
            FragmentContent::Episode { body, .. }
            | FragmentContent::Belief { body, .. }
            | FragmentContent::Goal { body, .. }
            | FragmentContent::Observation { body }
            | FragmentContent::Reflection { body, .. }
            | FragmentContent::Journal { body, .. } => body.clone(),
            FragmentContent::Reference { uri } => uri.clone(),
        };

        // 1. Phasic valence stamp on the `valence` axis.
        let stamp = Self::stamp_for(&body);
        fragment.valence = stamp;

        // 2. Update the running sustained magnitude (EMA of |valence|).
        self.sustained_magnitude = SUSTAINED_ALPHA.mul_add(
            stamp.abs(),
            (1.0 - SUSTAINED_ALPHA) * self.sustained_magnitude,
        );
        self.sustained_magnitude = self.sustained_magnitude.clamp(0.0, 1.0);

        // 3. Modulate durability on the `decay_rate` axis (no
        // overlap with saliency). Higher sustained magnitude →
        // slower decay.
        let dampening = 1.0 + DECAY_DAMPENING * self.sustained_magnitude;
        fragment.decay_rate /= dampening;

        // 4. Provenance stamp.
        fragment.provenance.source_subsystem = "valence".to_string();

        Ok(vec![fragment])
    }

    fn checkpoint(&self) -> Result<Vec<u8>> {
        let snap = ValenceSnapshot {
            stamps: self.stamps,
            sustained_magnitude: self.sustained_magnitude,
        };
        bincode::serialize(&snap).map_err(|e| {
            hyphae_core::HyphaeError::Other(format!("valence checkpoint serialise: {e}"))
        })
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<()> {
        let snap: ValenceSnapshot = bincode::deserialize(bytes).map_err(|e| {
            hyphae_core::HyphaeError::Other(format!("valence restore deserialise: {e}"))
        })?;
        self.stamps = snap.stamps;
        self.sustained_magnitude = snap.sustained_magnitude;
        Ok(())
    }

    fn on_state_change(&mut self, _old: State, new: State) -> Result<()> {
        // Recovery wipes the running EMA — the context after a
        // crash is undefined, and resuming with a stale magnitude
        // would mis-modulate decay rates for new inputs.
        if new == State::Recovery {
            self.sustained_magnitude = 0.0;
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn consolidation_gate_signal(&self) -> ConsolidationGateSignal {
        if self.sustained_magnitude >= CONSOLIDATION_BLOCK_MAGNITUDE {
            ConsolidationGateSignal::Block(format!(
                "valence sustained magnitude {:.2} >= {:.2}",
                self.sustained_magnitude, CONSOLIDATION_BLOCK_MAGNITUDE
            ))
        } else {
            ConsolidationGateSignal::Allow
        }
    }
}

/// Count whole-word matches of any term in `lexicon` against `body`
/// (which must already be lowercased). Whole-word boundary means the
/// surrounding characters are non-alphanumeric (or string ends).
fn count_whole_word_matches(body: &str, lexicon: &[&str]) -> u32 {
    let bytes = body.as_bytes();
    let mut count = 0u32;
    for term in lexicon {
        let tbytes = term.as_bytes();
        if tbytes.is_empty() || tbytes.len() > bytes.len() {
            continue;
        }
        let mut i = 0;
        while i + tbytes.len() <= bytes.len() {
            if &bytes[i..i + tbytes.len()] == tbytes {
                let before = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
                let after_pos = i + tbytes.len();
                let after = after_pos == bytes.len() || !bytes[after_pos].is_ascii_alphanumeric();
                if before && after {
                    count += 1;
                    i += tbytes.len();
                    continue;
                }
            }
            i += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(body: &str) -> CognitiveFragment {
        CognitiveFragment::new(
            FragmentContent::Observation {
                body: body.to_string(),
            },
            "test",
        )
    }

    #[test]
    fn neutral_input_yields_zero_valence() {
        let mut v = Valence::new();
        let out = v
            .process(obs("the weather"), PayloadKind::Encoding, State::Encoding)
            .unwrap();
        assert!((out[0].valence - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn positive_input_pushes_positive_valence() {
        let mut v = Valence::new();
        let out = v
            .process(
                obs("the team had a great win today, wonderful celebration"),
                PayloadKind::Encoding,
                State::Encoding,
            )
            .unwrap();
        assert!(out[0].valence > 0.5);
    }

    #[test]
    fn negative_input_pushes_negative_valence() {
        let mut v = Valence::new();
        let out = v
            .process(
                obs("terrible loss and awful pain throughout"),
                PayloadKind::Encoding,
                State::Encoding,
            )
            .unwrap();
        assert!(out[0].valence < -0.5);
    }

    #[test]
    fn sustained_magnitude_rises_with_charged_context() {
        let mut v = Valence::new();
        let initial = v.sustained_magnitude();
        for _ in 0..10 {
            v.process(
                obs("wonderful joy delight"),
                PayloadKind::Encoding,
                State::Encoding,
            )
            .unwrap();
        }
        assert!(v.sustained_magnitude() > initial);
        assert!(v.sustained_magnitude() <= 1.0);
    }

    #[test]
    fn decay_rate_decreases_when_sustained_high() {
        let mut v = Valence::new();
        // Saturate the magnitude.
        for _ in 0..20 {
            v.process(
                obs("terrible awful fear pain"),
                PayloadKind::Encoding,
                State::Encoding,
            )
            .unwrap();
        }
        let mut frag = obs("a follow-up input");
        let initial_decay = frag.decay_rate;
        frag = v
            .process(frag, PayloadKind::Encoding, State::Encoding)
            .unwrap()
            .pop()
            .unwrap();
        assert!(
            frag.decay_rate < initial_decay,
            "charged context must dampen decay rate, got {} (was {})",
            frag.decay_rate,
            initial_decay,
        );
    }

    #[test]
    fn consolidation_gate_blocks_when_magnitude_high() {
        let mut v = Valence::new();
        for _ in 0..50 {
            v.process(
                obs("fear danger hurt terrible"),
                PayloadKind::Encoding,
                State::Encoding,
            )
            .unwrap();
        }
        assert!(matches!(
            v.consolidation_gate_signal(),
            ConsolidationGateSignal::Block(_)
        ));
    }

    #[test]
    fn consolidation_gate_allows_when_neutral() {
        let v = Valence::new();
        assert_eq!(
            v.consolidation_gate_signal(),
            ConsolidationGateSignal::Allow
        );
    }

    #[test]
    fn recovery_wipes_sustained_magnitude() {
        let mut v = Valence::new();
        for _ in 0..10 {
            v.process(obs("terrible pain"), PayloadKind::Encoding, State::Encoding)
                .unwrap();
        }
        assert!(v.sustained_magnitude() > 0.0);
        v.on_state_change(State::Encoding, State::Recovery).unwrap();
        assert!((v.sustained_magnitude() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn checkpoint_restore_round_trips() {
        let mut v = Valence::new();
        for _ in 0..3 {
            v.process(obs("great success"), PayloadKind::Encoding, State::Encoding)
                .unwrap();
        }
        let bytes = v.checkpoint().unwrap();
        let mut restored = Valence::new();
        restored.restore(&bytes).unwrap();
        assert_eq!(restored.stamps(), v.stamps());
        assert!((restored.sustained_magnitude() - v.sustained_magnitude()).abs() < f32::EPSILON);
    }
}
