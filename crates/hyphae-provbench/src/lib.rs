// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! # hyphae-provbench
//!
//! A community-scale **provenance benchmark**: a realizer-independent
//! protocol that scores any verifiable-generation system on the axis
//! that actually distinguishes such systems — whether post-hoc
//! tampering with stored, quoted content is *detectable* and
//! *localisable* — the way correctness benchmarks compare LLM-RAG
//! systems on the axis that distinguishes them.
//!
//! It generalises the paper's minimal four-mode experiment along three
//! axes the Future Work section called for:
//! - a **tampering taxonomy** ([`tamper`]) of ten modes,
//! - an **adversary-capability matrix** ([`adversary`]), and
//! - a **standard scoring protocol** ([`scoring`]) — detection,
//!   localisation, false-positive rate, and a latency proxy.
//!
//! Systems plug in via the [`system::ProvenanceSystem`] trait. The two
//! shipped here are the `verbatim-journal` layer (shared identically
//! by Hyphae and an `echo+journal` baseline) and a no-journal `echo`
//! control. The whole run is reproducible from `(n, trials, seed)`.

#![warn(missing_docs)]

pub mod adversary;
pub mod fragment;
pub mod harness;
pub mod prng;
pub mod scoring;
pub mod system;
pub mod systems;
pub mod tamper;

/// Protocol identifier and version. Bump on any change to the scoring
/// semantics or the matrix so envelopes remain comparable.
pub const PROTOCOL_VERSION: &str = "provbench/v1";

#[cfg(test)]
mod tests {
    use crate::fragment::corpus;
    use crate::harness::run;
    use crate::prng::seed32;
    use crate::system::{ProvenanceSystem, VerifyOutcome};
    use crate::systems::{EchoNoJournal, VerbatimJournal};
    use crate::tamper::TamperMode;
    use hyphae_storage::{verify_anchored_head, HeadAnchor};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static C: AtomicU64 = AtomicU64::new(0);

    fn fresh() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "provbench-test-{}-{}",
            std::process::id(),
            C.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    // ── Unit-level: the verbatim-journal store-only path detects and
    //    localises an in-place edit to the exact successor seq. ──
    #[test]
    fn store_only_edit_detected_and_localised() {
        let sys = VerbatimJournal;
        let dir = fresh();
        sys.ingest(&dir, &corpus(12, 1));
        let gt = sys.tamper(&dir, TamperMode::Edit, 5, 12, false).unwrap();
        assert_eq!(gt.expected_break_seq, Some(6));
        assert_eq!(sys.verify(&dir), VerifyOutcome::Violation { seq: 6 });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_false_positive_on_clean_store() {
        let sys = VerbatimJournal;
        let dir = fresh();
        sys.ingest(&dir, &corpus(12, 2));
        assert_eq!(sys.verify(&dir), VerifyOutcome::Clean);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Chain-aware recompute defeats the bare chain, but moves the
    //    head — so an external anchor over the old head catches it. ──
    #[test]
    fn chain_aware_defeats_bare_chain_but_anchor_catches_it() {
        let sys = VerbatimJournal;
        let dir = fresh();
        sys.ingest(&dir, &corpus(12, 3));
        let head_before = sys.head(&dir).unwrap();
        let anchor = HeadAnchor::from_seed(&seed32(3));
        let anchored = anchor.anchor(head_before);

        let gt = sys.tamper(&dir, TamperMode::Edit, 5, 12, true).unwrap();
        assert_eq!(gt.expected_break_seq, None, "consistent by construction");
        assert_eq!(sys.verify(&dir), VerifyOutcome::Clean, "bare chain defeated");

        let head_after = sys.head(&dir).unwrap();
        assert_ne!(head_after, head_before, "the head necessarily shifted");
        assert!(
            !verify_anchored_head(&head_after, &anchored, &anchor.verifying_key()),
            "the anchor over the old head no longer matches"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Head rollback to a valid prefix is invisible to the bare chain
    //    but the anchored (longer) head no longer matches. ──
    #[test]
    fn head_rollback_evades_bare_chain_but_not_anchor() {
        let sys = VerbatimJournal;
        let dir = fresh();
        sys.ingest(&dir, &corpus(12, 4));
        let head_before = sys.head(&dir).unwrap();
        let anchor = HeadAnchor::from_seed(&seed32(4));
        let anchored = anchor.anchor(head_before);

        sys.tamper(&dir, TamperMode::HeadRollback, 5, 12, false).unwrap();
        assert_eq!(sys.verify(&dir), VerifyOutcome::Clean, "prefix is consistent");
        let head_after = sys.head(&dir).unwrap();
        assert!(!verify_anchored_head(&head_after, &anchored, &anchor.verifying_key()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── The echo control has no provenance structure: it detects
    //    nothing and has no head to anchor. ──
    #[test]
    fn echo_control_detects_nothing() {
        let sys = EchoNoJournal;
        let dir = fresh();
        sys.ingest(&dir, &corpus(12, 5));
        assert!(sys.head(&dir).is_none());
        for mode in [TamperMode::Edit, TamperMode::Delete, TamperMode::HeadRollback] {
            sys.tamper(&dir, mode, 5, 12, false);
            assert_eq!(sys.verify(&dir), VerifyOutcome::Clean, "echo cannot detect");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Aggregate smoke + reproducibility (tiny so the suite stays
    //    cheap). Asserts the headline cells and byte-stable envelope. ──
    #[test]
    fn matrix_smoke_and_determinism() {
        let a = run(8, 1, 7);
        let b = run(8, 1, 7);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap(),
            "same (n, trials, seed) must produce a byte-identical envelope"
        );
        let edit = a
            .cells
            .iter()
            .find(|c| {
                c.system == "verbatim-journal"
                    && c.adversary == "chain-aware+key"
                    && c.tamper_mode == "edit"
            })
            .unwrap();
        assert_eq!(edit.bare_detection_rate, 0.0);
        assert_eq!(
            edit.anchored_detection_rate, 0.0,
            "compromised key is the guarantee's boundary"
        );
    }
}
