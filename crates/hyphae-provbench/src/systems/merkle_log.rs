// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! A Merkle-tree transparency log (RFC 6962 / Certificate Transparency
//! style) over the fragment leaves.
//!
//! This is the *recognized alternative* to Hyphae's flat hash chain.
//! Leaves are hashed `H(0x00 ‖ payload)` and combined `H(0x01 ‖ l ‖ r)`
//! with the RFC 6962 split; the tree root is the head an external anchor
//! signs. On the tampering taxonomy its detection profile is expected to
//! *match* the flat chain — store-only tampering is detected and
//! localised, a chain-aware recompute defeats the bare root but the
//! external anchor over the old root catches it — which is itself the
//! finding: provenance detection is a property of the append-only-log
//! *class*, not of Hyphae's particular chain. Where it differs is
//! proof cost: a Merkle log proves inclusion in `O(log n)` hashes,
//! against the flat chain's `O(n)` (see proof-cost in
//! [`crate::scoring`]).
//!
//! No new dependency: the tree is a faithful RFC 6962 construction over
//! `sha2`.

use std::path::Path;

use sha2::{Digest, Sha256};

use crate::fragment::Fragment;
use crate::system::{GroundTruth, ProvenanceSystem, VerifyOutcome};
use crate::tamper::TamperMode;

const STORE_FILE: &str = "merkle.store";

/// The Merkle transparency-log system.
pub struct MerkleLog;

fn leaf_hash(payload: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([0x00]);
    h.update(payload);
    h.finalize().into()
}

fn node_hash(l: &[u8; 32], r: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([0x01]);
    h.update(l);
    h.update(r);
    h.finalize().into()
}

/// RFC 6962 Merkle Tree Hash over a list of leaf hashes.
fn merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    match leaves.len() {
        0 => Sha256::digest([]).into(),
        1 => leaves[0],
        n => {
            let mut k = 1usize;
            while k * 2 < n {
                k *= 2;
            } // largest power of two strictly < n
            node_hash(&merkle_root(&leaves[..k]), &merkle_root(&leaves[k..]))
        }
    }
}

/// The store: the payloads (leaves), the committed leaf hashes, and the
/// committed root.
struct MerkleStore {
    payloads: Vec<Vec<u8>>,
    leaf_hashes: Vec<[u8; 32]>,
    root: [u8; 32],
}

fn write_store(dir: &Path, s: &MerkleStore) {
    std::fs::create_dir_all(dir).expect("mkdir");
    let mut buf = Vec::new();
    // Two independent counts: a store-only tamper can leave the payload
    // list and the committed leaf-hash list at different lengths (that
    // mismatch is exactly what verify detects).
    buf.extend_from_slice(&(s.payloads.len() as u64).to_le_bytes());
    for p in &s.payloads {
        buf.extend_from_slice(&(p.len() as u32).to_le_bytes());
        buf.extend_from_slice(p);
    }
    buf.extend_from_slice(&(s.leaf_hashes.len() as u64).to_le_bytes());
    for h in &s.leaf_hashes {
        buf.extend_from_slice(h);
    }
    buf.extend_from_slice(&s.root);
    std::fs::write(dir.join(STORE_FILE), buf).expect("write merkle store");
}

fn read_store(dir: &Path) -> MerkleStore {
    let buf = std::fs::read(dir.join(STORE_FILE)).unwrap_or_default();
    let mut p = 0usize;
    let take = |p: &mut usize, k: usize| -> &[u8] {
        let s = &buf[*p..*p + k];
        *p += k;
        s
    };
    let pn = u64::from_le_bytes(take(&mut p, 8).try_into().unwrap()) as usize;
    let mut payloads = Vec::with_capacity(pn);
    for _ in 0..pn {
        let len = u32::from_le_bytes(take(&mut p, 4).try_into().unwrap()) as usize;
        payloads.push(take(&mut p, len).to_vec());
    }
    let ln = u64::from_le_bytes(take(&mut p, 8).try_into().unwrap()) as usize;
    let mut leaf_hashes = Vec::with_capacity(ln);
    for _ in 0..ln {
        leaf_hashes.push(take(&mut p, 32).try_into().unwrap());
    }
    let root: [u8; 32] = take(&mut p, 32).try_into().unwrap();
    MerkleStore {
        payloads,
        leaf_hashes,
        root,
    }
}

impl ProvenanceSystem for MerkleLog {
    fn name(&self) -> &'static str {
        "merkle-log"
    }

    fn ingest(&self, dir: &Path, fragments: &[Fragment]) -> Option<[u8; 32]> {
        let payloads: Vec<Vec<u8>> = fragments.iter().map(|f| f.body.clone()).collect();
        let leaf_hashes: Vec<[u8; 32]> = payloads.iter().map(|p| leaf_hash(p)).collect();
        let root = merkle_root(&leaf_hashes);
        write_store(
            dir,
            &MerkleStore {
                payloads,
                leaf_hashes,
                root,
            },
        );
        Some(root)
    }

    fn verify(&self, dir: &Path) -> VerifyOutcome {
        let s = read_store(dir);
        // Localise: first index where the payload-derived leaf hash
        // diverges from the committed leaf hash (or a length boundary).
        let m = s.payloads.len().max(s.leaf_hashes.len());
        for i in 0..m {
            let derived = s.payloads.get(i).map(|p| leaf_hash(p));
            let committed = s.leaf_hashes.get(i).copied();
            if derived != committed {
                return VerifyOutcome::Violation { seq: i as u64 };
            }
        }
        // The committed leaf hashes must commit to the stored root.
        if merkle_root(&s.leaf_hashes) != s.root {
            return VerifyOutcome::Violation {
                seq: s.leaf_hashes.len().saturating_sub(1) as u64,
            };
        }
        VerifyOutcome::Clean
    }

    fn head(&self, dir: &Path) -> Option<[u8; 32]> {
        Some(read_store(dir).root)
    }

    fn inclusion_proof_hashes(&self, n: u64) -> Option<u64> {
        // RFC 6962 audit path: O(log n).
        Some(crate::system::ceil_log2(n))
    }

    fn tamper(
        &self,
        dir: &Path,
        mode: TamperMode,
        target: u64,
        n: u64,
        chain_aware: bool,
    ) -> Option<GroundTruth> {
        let mut s = read_store(dir);
        let t = target as usize;

        // HeadRollback is consistent-by-construction regardless of
        // chain knowledge: truncate the tail and repoint the root.
        if matches!(mode, TamperMode::HeadRollback) {
            let drop = 3usize.min(s.payloads.len().saturating_sub(1)).max(1);
            s.payloads.truncate(s.payloads.len() - drop);
            s.leaf_hashes.truncate(s.leaf_hashes.len() - drop);
            s.root = merkle_root(&s.leaf_hashes);
            write_store(dir, &s);
            return Some(consistent(s.root));
        }
        if mode == TamperMode::TimestampSkew {
            // No timestamp in a Merkle leaf: the skew is a no-op, so the
            // store is unchanged and nothing detects it (correctly).
            return Some(GroundTruth {
                expected_break_seq: None,
                head_after: Some(s.root),
            });
        }

        // Apply the surface mutation to the payloads (and, for the
        // structural modes, the payload list).
        let expected = apply_payload_mutation(&mut s.payloads, mode, t, n);

        if chain_aware {
            // Recompute the whole tree + root: the bare check passes,
            // only the anchor over the old root catches it.
            s.leaf_hashes = s.payloads.iter().map(|p| leaf_hash(p)).collect();
            s.root = merkle_root(&s.leaf_hashes);
            write_store(dir, &s);
            Some(consistent(s.root))
        } else {
            // Store-only: leaf hashes and root stay stale; the
            // recomputed-vs-committed mismatch is detected and localised.
            let head_after = s.root; // unchanged by a store-only edit
            write_store(dir, &s);
            Some(GroundTruth {
                expected_break_seq: Some(expected),
                head_after: Some(head_after),
            })
        }
    }
}

/// Apply a mode's surface mutation to the payload list; return the
/// sequence at which a store-only verify will first diverge.
fn apply_payload_mutation(payloads: &mut Vec<Vec<u8>>, mode: TamperMode, t: usize, n: u64) -> u64 {
    match mode {
        TamperMode::Edit | TamperMode::BitFlip | TamperMode::Truncate => {
            mutate(&mut payloads[t], mode);
            t as u64
        }
        TamperMode::Delete => {
            payloads.remove(t);
            t as u64
        }
        TamperMode::Insert => {
            payloads.push(b"a fabricated fact never ingested".to_vec());
            n // first divergence is the appended leaf beyond the committed set
        }
        TamperMode::Duplicate => {
            payloads.push(payloads[t].clone());
            n
        }
        TamperMode::Reorder => {
            let other = ((t as u64 + 2).min(n - 1)) as usize;
            payloads.swap(t, other);
            t.min(other) as u64
        }
        TamperMode::Batch => {
            let t2 = ((t as u64 + 1).min(n - 1)) as usize;
            mutate(&mut payloads[t], TamperMode::Edit);
            mutate(&mut payloads[t2], TamperMode::Edit);
            t.min(t2) as u64
        }
        // HeadRollback / TimestampSkew handled before this call.
        _ => t as u64,
    }
}

fn mutate(payload: &mut Vec<u8>, mode: TamperMode) {
    match mode {
        TamperMode::Edit => *payload = b"tampered: fabricated content".to_vec(),
        TamperMode::BitFlip => {
            if payload.is_empty() {
                payload.push(0xFF);
            } else {
                payload[0] ^= 0xFF;
            }
        }
        TamperMode::Truncate => {
            let half = payload.len() / 2;
            payload.truncate(half);
        }
        _ => *payload = b"tampered".to_vec(),
    }
}

fn consistent(root: [u8; 32]) -> GroundTruth {
    GroundTruth {
        expected_break_seq: None,
        head_after: Some(root),
    }
}
