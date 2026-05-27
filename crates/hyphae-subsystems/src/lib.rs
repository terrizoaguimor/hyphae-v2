// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! # hyphae-subsystems
//!
//! Six functional subsystems for Hyphae v2, collapsed from v1's
//! seventeen anatomical subsystems per ADR-0001 §"Subsystems
//! collapsed". Each module here is a function, not an anatomy.
//!
//! - [`input_gate`] — `Thalamus + OlfactoryBulb`. Dispatch +
//!   signature-based amplification.
//! - [`episodic`] — `Hippocampus + EntorhinalCortex`. HNSW index +
//!   conductivity graph + cascade propagation.
//! - [`valence`] — `Amygdala + BNST`. Affective stamp + decay
//!   modulation. Concerns separated at the type level (valence on
//!   `valence` axis, durability on `decay_rate` axis — corrects
//!   v1's M4b saliency-clamp collision).
//! - [`composer`] — `FrontalCortex + ACC`. Bounded working memory
//!   (cap=7), conversation thread tracker, conflict signal cabled
//!   into the ethics path from commit zero.
//! - [`predictive`] — `Cerebellum`. Forward model + prediction error
//!   with cycle-id correlation (corrects v1's ISSUE-M4c-2 loose
//!   pairing).
//! - [`reward`] — `DopaminergicMidbrain`. Signed RPE + expected-
//!   valence low-pass + habituation emerges from the low-pass.

#![warn(missing_docs)]
#![warn(clippy::pedantic)]

pub mod composer;
pub mod episodic;
pub mod input_gate;
pub mod predictive;
pub mod reward;
pub mod valence;

pub use composer::Composer;
pub use episodic::Episodic;
pub use input_gate::InputGate;
pub use predictive::Predictive;
pub use reward::Reward;
pub use valence::Valence;
