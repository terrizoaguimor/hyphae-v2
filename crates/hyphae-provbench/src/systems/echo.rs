// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! The no-journal control: an `echo` baseline that stores bodies with
//! no chain, no head, no integrity structure.
//!
//! This is the negative control. It emits verbatim bodies (so on
//! correctness/grounding it ties the journal-backed systems — the
//! paper's central observation) but keeps no provenance structure, so
//! it detects **nothing**. Its row of zeros is the point: verifiable
//! provenance comes from the journal layer, which `echo` lacks and
//! `echo+journal` (== the verbatim-journal system) has. An LLM-RAG
//! baseline sits here too — it not only keeps no journal, its
//! paraphrased output is not byte-bindable to a source at all.

use std::path::Path;

use crate::fragment::Fragment;
use crate::system::{GroundTruth, ProvenanceSystem, VerifyOutcome};
use crate::tamper::TamperMode;

const STORE_FILE: &str = "echo.store";

/// The no-journal echo control.
pub struct EchoNoJournal;

impl ProvenanceSystem for EchoNoJournal {
    fn name(&self) -> &'static str {
        "echo-no-journal"
    }

    fn ingest(&self, dir: &Path, fragments: &[Fragment]) -> Option<[u8; 32]> {
        std::fs::create_dir_all(dir).expect("mkdir");
        let mut buf = Vec::new();
        for f in fragments {
            buf.extend_from_slice(&f.id.to_le_bytes());
            buf.extend_from_slice(&(f.body.len() as u64).to_le_bytes());
            buf.extend_from_slice(&f.body);
        }
        std::fs::write(dir.join(STORE_FILE), buf).expect("write echo store");
        None // no chain, no head
    }

    fn verify(&self, _dir: &Path) -> VerifyOutcome {
        // No chain, no head: there is nothing to check against, so
        // tampering is structurally undetectable.
        VerifyOutcome::Clean
    }

    fn head(&self, _dir: &Path) -> Option<[u8; 32]> {
        None
    }

    fn tamper(
        &self,
        dir: &Path,
        _mode: TamperMode,
        _target: u64,
        _n: u64,
        _chain_aware: bool,
    ) -> Option<GroundTruth> {
        // Mutate the store so the tamper genuinely happened; detection
        // is still structurally impossible, which is the result.
        let path = dir.join(STORE_FILE);
        if let Ok(mut bytes) = std::fs::read(&path) {
            if let Some(b) = bytes.last_mut() {
                *b ^= 0xFF;
            } else {
                bytes.push(0xFF);
            }
            std::fs::write(&path, bytes).expect("rewrite echo store");
        }
        // No chain to break — the bare check can never localise this.
        Some(GroundTruth {
            expected_break_seq: None,
            head_after: None,
        })
    }
}
