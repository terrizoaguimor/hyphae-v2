// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! The verbatim-journal system: `hyphae_storage::Journal` over raw
//! fragment bodies.
//!
//! This is the provenance layer the paper's thesis rests on, and it is
//! **realizer-independent** — nothing here invokes a lexicon, cascade,
//! or any cognition machinery. The detection result is therefore
//! shared by *any* system that (a) emits retrieved fragments verbatim
//! and (b) keeps this journal: by Hyphae and by an `echo+journal`
//! baseline equally. That is the contribution being measured: verifiable
//! provenance is an addable layer, not a Hyphae-specific feature.
//!
//! Performance note: ingest and tamper each open the fjall keyspace
//! exactly once and persist with a single fsync; integrity is checked
//! by the *real* shipped [`Journal::verify`] (one more open). Keeping
//! the open count low matters because keyspace recovery is fsync-heavy.

use std::path::Path;

use fjall::{Config, PartitionCreateOptions, PersistMode};
use hyphae_storage::{Journal, JournalEntry, JournalError};

use crate::fragment::Fragment;
use crate::system::{GroundTruth, ProvenanceSystem, VerifyOutcome};
use crate::tamper::TamperMode;

const ENTRIES: &str = "entries";
const META: &str = "meta";
const HEAD_KEY: &[u8] = b"head";
const EVENT_KIND: &str = "audit_memory_op";

/// The verbatim-journal system under test.
pub struct VerbatimJournal;

impl ProvenanceSystem for VerbatimJournal {
    fn name(&self) -> &'static str {
        "verbatim-journal"
    }

    fn ingest(&self, dir: &Path, fragments: &[Fragment]) -> Option<[u8; 32]> {
        // Build the whole hash chain in memory and persist with a
        // SINGLE fsync, instead of `Journal::append`'s sync-per-entry.
        // Byte-compatible with `Journal::open`/`verify` (same
        // partitions, key scheme, content_hash) and deterministic:
        // timestamps come from the seq, not the wall clock.
        let ks = Config::new(dir).open().expect("open ks");
        let part = ks
            .open_partition(ENTRIES, PartitionCreateOptions::default())
            .expect("open entries");
        let meta = ks
            .open_partition(META, PartitionCreateOptions::default())
            .expect("open meta");
        let mut prev = [0u8; 32];
        for (i, f) in fragments.iter().enumerate() {
            let e = JournalEntry {
                seq: i as u64,
                prev_hash: prev,
                timestamp_ns: i as u128,
                event_kind: EVENT_KIND.to_string(),
                payload: f.body.clone(),
            };
            prev = e.content_hash();
            let enc = bincode::serialize(&e).expect("encode");
            part.insert(e.seq.to_be_bytes(), &enc).expect("insert");
        }
        meta.insert(HEAD_KEY, prev).expect("head");
        ks.persist(PersistMode::SyncAll).expect("persist");
        Some(prev)
    }

    fn verify(&self, dir: &Path) -> VerifyOutcome {
        let journal = Journal::open(dir).expect("reopen for verify");
        match journal.verify() {
            Ok(()) => VerifyOutcome::Clean,
            Err(JournalError::IntegrityViolation { seq, .. }) => VerifyOutcome::Violation { seq },
            Err(e) => panic!("unexpected verify error: {e}"),
        }
    }

    fn head(&self, dir: &Path) -> Option<[u8; 32]> {
        let journal = Journal::open(dir).expect("reopen for head");
        Some(journal.head())
    }

    fn tamper(
        &self,
        dir: &Path,
        mode: TamperMode,
        target: u64,
        n: u64,
        chain_aware: bool,
    ) -> Option<GroundTruth> {
        let ks = Config::new(dir).open().expect("open ks");
        let part = ks
            .open_partition(ENTRIES, PartitionCreateOptions::default())
            .expect("open entries");
        let meta = ks
            .open_partition(META, PartitionCreateOptions::default())
            .expect("open meta");

        // Read the current chain into memory (one pass).
        let mut es: Vec<JournalEntry> = part
            .iter()
            .map(|kv| {
                let (_k, v) = kv.expect("iter");
                bincode::deserialize::<JournalEntry>(&v).expect("decode")
            })
            .collect();
        let t = target as usize;

        let result = if chain_aware {
            // Apply the logical mutation, recompute the chain forward,
            // and rewrite the whole store + head. Bare verify will pass.
            let from = apply_consistent(&mut es, mode, t, n);
            let head = recompute_from(&mut es, from);
            full_rewrite(&part, &meta, &es, head);
            GroundTruth {
                expected_break_seq: None,
                head_after: Some(head),
            }
        } else {
            // Surface mutation only: leave chain links stale so the
            // break is detectable and localisable.
            apply_store_only(&part, &meta, &mut es, mode, t, n)
        };

        ks.persist(PersistMode::SyncAll).expect("persist");
        Some(result)
    }
}

/// Head = content hash of the last entry (or zeros if empty).
fn head_of(es: &[JournalEntry]) -> [u8; 32] {
    es.last().map(JournalEntry::content_hash).unwrap_or([0u8; 32])
}

/// Apply a store-only (naive) tamper directly to the partition,
/// leaving chain links stale, and return the ground truth.
fn apply_store_only(
    part: &fjall::PartitionHandle,
    meta: &fjall::PartitionHandle,
    es: &mut [JournalEntry],
    mode: TamperMode,
    t: usize,
    n: u64,
) -> GroundTruth {
    let put = |seq: u64, e: &JournalEntry| {
        let enc = bincode::serialize(e).expect("encode");
        part.insert(seq.to_be_bytes(), &enc).expect("insert");
    };
    match mode {
        TamperMode::Edit | TamperMode::BitFlip | TamperMode::Truncate | TamperMode::TimestampSkew => {
            mutate_one(&mut es[t], mode);
            put(t as u64, &es[t]);
            break_gt(target_break(t as u64, n), head_of(es))
        }
        TamperMode::Delete => {
            part.remove((t as u64).to_be_bytes()).expect("remove");
            // last entry unchanged → head unchanged
            break_gt(t as u64 + 1, head_of(es))
        }
        TamperMode::Insert => {
            let forged = forged_entry(es, n, b"a fabricated fact never ingested".to_vec());
            put(n, &forged);
            // new trailing entry becomes the apparent head
            break_gt(n, forged.content_hash())
        }
        TamperMode::Duplicate => {
            let body = es[t].payload.clone();
            let forged = forged_entry(es, n, body);
            put(n, &forged);
            break_gt(n, forged.content_hash())
        }
        TamperMode::Reorder => {
            // Swap the BODIES only (seq/prev_hash stay put) so the
            // store stays seq-keyed; the first changed body breaks the
            // link at its successor.
            let other = (target_other(t as u64, n)) as usize;
            swap_payloads(es, t, other);
            put(t as u64, &es[t]);
            put(other as u64, &es[other]);
            break_gt(t.min(other) as u64 + 1, head_of(es))
        }
        TamperMode::Batch => {
            let t2 = ((t as u64 + 1).min(n - 1)) as usize;
            mutate_one(&mut es[t], TamperMode::Edit);
            mutate_one(&mut es[t2], TamperMode::Edit);
            put(t as u64, &es[t]);
            put(t2 as u64, &es[t2]);
            break_gt(t.min(t2) as u64 + 1, head_of(es))
        }
        TamperMode::HeadRollback => {
            // Drop the tail to a valid prefix AND repoint the persisted
            // head to that prefix (a store-writing attacker controls
            // both). The bare chain then stays internally consistent —
            // caught only by the external anchor over the old, longer
            // head.
            let drop = 3usize.min(es.len().saturating_sub(1)).max(1);
            for e in es.iter().skip(es.len() - drop) {
                part.remove(e.seq.to_be_bytes()).expect("remove");
            }
            let kept = &es[..es.len() - drop];
            let head = head_of(kept);
            meta.insert(HEAD_KEY, head).expect("repoint head");
            consistent_gt(head)
        }
    }
}

/// Apply the same logical mutation to the in-memory chain (for the
/// chain-aware adversary). Returns the index from which to recompute.
fn apply_consistent(es: &mut Vec<JournalEntry>, mode: TamperMode, t: usize, n: u64) -> usize {
    match mode {
        TamperMode::Edit | TamperMode::BitFlip | TamperMode::Truncate | TamperMode::TimestampSkew => {
            mutate_one(&mut es[t], mode);
            t
        }
        TamperMode::Delete => {
            es.remove(t);
            t
        }
        TamperMode::Insert => {
            let forged = forged_entry(es, n, b"chain-aware forged entry".to_vec());
            es.push(forged);
            es.len() - 1
        }
        TamperMode::Duplicate => {
            let body = es[t].payload.clone();
            let forged = forged_entry(es, n, body);
            es.push(forged);
            es.len() - 1
        }
        TamperMode::Reorder => {
            let other = (target_other(t as u64, n)) as usize;
            swap_payloads(es, t, other);
            t.min(other)
        }
        TamperMode::Batch => {
            let t2 = ((t as u64 + 1).min(n - 1)) as usize;
            mutate_one(&mut es[t], TamperMode::Edit);
            mutate_one(&mut es[t2], TamperMode::Edit);
            t.min(t2)
        }
        TamperMode::HeadRollback => {
            let drop = 3usize.min(es.len().saturating_sub(1)).max(1);
            es.truncate(es.len() - drop);
            es.len() // nothing to recompute; already consistent
        }
    }
}

fn break_gt(seq: u64, head_after: [u8; 32]) -> GroundTruth {
    GroundTruth {
        expected_break_seq: Some(seq),
        head_after: Some(head_after),
    }
}

fn consistent_gt(head_after: [u8; 32]) -> GroundTruth {
    GroundTruth {
        expected_break_seq: None,
        head_after: Some(head_after),
    }
}

/// Expected first-broken-link seq for an in-place single-entry edit.
fn target_break(t: u64, n: u64) -> u64 {
    (t + 1).min(n - 1)
}

/// The partner index for a reorder swap (interior, distinct from `t`).
fn target_other(t: u64, n: u64) -> u64 {
    (t + 2).min(n - 1)
}

/// Build a forged trailing entry with a stale prev_hash (the attacker
/// guesses the link).
fn forged_entry(es: &[JournalEntry], at_seq: u64, body: Vec<u8>) -> JournalEntry {
    let prev = es.last().expect("non-empty");
    JournalEntry {
        seq: at_seq,
        prev_hash: prev.prev_hash, // wrong link
        timestamp_ns: prev.timestamp_ns + 1,
        event_kind: EVENT_KIND.to_string(),
        payload: body,
    }
}

/// Swap only the bodies of two entries (leaves seq/prev_hash intact).
fn swap_payloads(es: &mut [JournalEntry], a: usize, b: usize) {
    let pa = std::mem::take(&mut es[a].payload);
    let pb = std::mem::replace(&mut es[b].payload, pa);
    es[a].payload = pb;
}

/// Apply an in-memory content mutation to a single entry.
fn mutate_one(e: &mut JournalEntry, mode: TamperMode) {
    match mode {
        TamperMode::Edit => e.payload = b"tampered: fabricated content".to_vec(),
        TamperMode::BitFlip => {
            if e.payload.is_empty() {
                e.payload.push(0xFF);
            } else {
                e.payload[0] ^= 0xFF;
            }
        }
        TamperMode::Truncate => {
            let half = e.payload.len() / 2;
            e.payload.truncate(half);
        }
        TamperMode::TimestampSkew => {
            e.timestamp_ns = e.timestamp_ns.wrapping_add(1_000_000_000);
        }
        _ => e.payload = b"tampered".to_vec(),
    }
}

/// Recompute the chain forward from `from`, returning the new head.
fn recompute_from(entries: &mut [JournalEntry], from: usize) -> [u8; 32] {
    if from >= entries.len() {
        return head_of(entries);
    }
    let mut prev = if from == 0 {
        [0u8; 32]
    } else {
        entries[from - 1].content_hash()
    };
    for e in entries.iter_mut().skip(from) {
        e.prev_hash = prev;
        prev = e.content_hash();
    }
    head_of(entries)
}

/// Clear the entries partition and re-persist `entries` + `head` (used
/// by the chain-aware adversary to install a forged-but-consistent
/// chain). Caller persists.
fn full_rewrite(
    part: &fjall::PartitionHandle,
    meta: &fjall::PartitionHandle,
    entries: &[JournalEntry],
    head: [u8; 32],
) {
    let keys: Vec<_> = part.iter().map(|kv| kv.expect("iter").0).collect();
    for k in keys {
        part.remove(&*k).expect("remove");
    }
    for e in entries {
        let enc = bincode::serialize(e).expect("encode");
        part.insert(e.seq.to_be_bytes(), &enc).expect("insert");
    }
    meta.insert(HEAD_KEY, head).expect("head");
}
