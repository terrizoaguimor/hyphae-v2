// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! A signed-entries log: each entry independently Ed25519-signed by the
//! store, with NO chain linking entries.
//!
//! This is a *contrast* system. Signing each entry catches in-place
//! content edits (the signature breaks) — but, lacking any inter-entry
//! link or commitment to the set, it cannot see deletion, reordering,
//! replay, or truncation: the surviving entries still carry valid
//! signatures. It shows the benchmark discriminates *designs* — that
//! provenance detection rewards the chain, not merely the signature.
//! It maintains no head (an external anchor has nothing to anchor), and
//! it offers no membership proof at all (see proof-cost in
//! [`crate::scoring`]).

use std::path::Path;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier};

use crate::fragment::Fragment;
use crate::system::{GroundTruth, ProvenanceSystem, VerifyOutcome};
use crate::tamper::TamperMode;

const STORE_FILE: &str = "signed.store";
/// The store's signing key. Fixed for reproducibility; the adversary
/// does not hold it (re-signing a forged entry needs this key).
const STORE_SEED: [u8; 32] = [0x51u8; 32];

/// A record: a payload and the store's signature over it.
type Record = (Vec<u8>, [u8; 64]);

/// The signed-entries contrast system.
pub struct SignedEntries;

fn store_key() -> SigningKey {
    SigningKey::from_bytes(&STORE_SEED)
}

fn write_records(dir: &Path, records: &[Record]) {
    std::fs::create_dir_all(dir).expect("mkdir");
    let mut buf = Vec::new();
    for (payload, sig) in records {
        buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        buf.extend_from_slice(payload);
        buf.extend_from_slice(sig);
    }
    std::fs::write(dir.join(STORE_FILE), buf).expect("write signed store");
}

fn read_records(dir: &Path) -> Vec<Record> {
    let buf = std::fs::read(dir.join(STORE_FILE)).unwrap_or_default();
    let mut out = Vec::new();
    let mut p = 0usize;
    while p + 4 <= buf.len() {
        let len = u32::from_le_bytes(buf[p..p + 4].try_into().unwrap()) as usize;
        p += 4;
        if p + len + 64 > buf.len() {
            break; // truncated record — stop (verify will see fewer entries)
        }
        let payload = buf[p..p + len].to_vec();
        p += len;
        let sig: [u8; 64] = buf[p..p + 64].try_into().unwrap();
        p += 64;
        out.push((payload, sig));
    }
    out
}

impl ProvenanceSystem for SignedEntries {
    fn name(&self) -> &'static str {
        "signed-entries"
    }

    fn ingest(&self, dir: &Path, fragments: &[Fragment]) -> Option<[u8; 32]> {
        let key = store_key();
        let records: Vec<Record> = fragments
            .iter()
            .map(|f| (f.body.clone(), key.sign(&f.body).to_bytes()))
            .collect();
        write_records(dir, &records);
        None // no chain, no head
    }

    fn verify(&self, dir: &Path) -> VerifyOutcome {
        let vk = store_key().verifying_key();
        for (i, (payload, sig)) in read_records(dir).iter().enumerate() {
            if vk.verify(payload, &Signature::from_bytes(sig)).is_err() {
                return VerifyOutcome::Violation { seq: i as u64 };
            }
        }
        VerifyOutcome::Clean
    }

    fn head(&self, _dir: &Path) -> Option<[u8; 32]> {
        None
    }

    fn tamper(
        &self,
        dir: &Path,
        mode: TamperMode,
        target: u64,
        n: u64,
        _chain_aware: bool,
    ) -> Option<GroundTruth> {
        // The adversary cannot re-sign (no store key), so chain-aware
        // recompute does not apply: a content edit always breaks the
        // signature; the misses come from the absence of a chain.
        let mut recs = read_records(dir);
        let t = target as usize;
        let gt = match mode {
            TamperMode::Edit | TamperMode::BitFlip | TamperMode::Truncate => {
                mutate(&mut recs[t].0, mode); // payload changes; signature now invalid
                break_at(target)
            }
            TamperMode::TimestampSkew => {
                // Signed-entries store no timestamp; the skew is invisible.
                miss()
            }
            TamperMode::Delete => {
                recs.remove(t); // survivors still verify
                miss()
            }
            TamperMode::Insert => {
                // Forged entry: the attacker cannot produce a valid store
                // signature, so it carries a garbage signature.
                recs.push((b"a fabricated fact never ingested".to_vec(), [0u8; 64]));
                break_at(recs.len() as u64 - 1)
            }
            TamperMode::Duplicate => {
                // Replay an existing entry WITH its valid signature.
                recs.push(recs[t].clone());
                miss()
            }
            TamperMode::Reorder => {
                let other = ((target + 2).min(n - 1)) as usize;
                recs.swap(t, other); // each still carries a valid signature
                miss()
            }
            TamperMode::Batch => {
                let t2 = ((target + 1).min(n - 1)) as usize;
                mutate(&mut recs[t].0, TamperMode::Edit);
                mutate(&mut recs[t2].0, TamperMode::Edit);
                break_at(target.min(t2 as u64))
            }
            TamperMode::HeadRollback => {
                let drop = 3usize.min(recs.len().saturating_sub(1)).max(1);
                recs.truncate(recs.len() - drop); // survivors verify
                miss()
            }
        };
        write_records(dir, &recs);
        Some(gt)
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

fn break_at(seq: u64) -> GroundTruth {
    GroundTruth {
        expected_break_seq: Some(seq),
        head_after: None,
    }
}

fn miss() -> GroundTruth {
    GroundTruth {
        expected_break_seq: None,
        head_after: None,
    }
}
