// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Append-only journal with SHA-256 hash chain.
//!
//! Every entry includes the SHA-256 hash of the previous entry,
//! creating a cryptographic chain. Tampering with any historical
//! entry invalidates all subsequent hashes, making the violation
//! detectable on verification.
//!
//! The hash of the most recent entry (the chain *head*) is also
//! persisted separately, so tampering with the final entry — which
//! has no successor to catch it — is detectable as well.
//!
//! **One chain per substrate** (per ADR-0003 §8). The substrate
//! journal and the ethics audit share this chain. Entries
//! distinguish themselves via the `event_kind` string, which the
//! substrate populates from the
//! [`hyphae_core::JournalEntryType`] discriminant — `audit_memory_op`,
//! `audit_ethics_evaluation`, `audit_learning_update`, etc.

use fjall::{Config, Keyspace, PartitionCreateOptions, PartitionHandle, PersistMode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use thiserror::Error;

/// fjall partition holding the journal entries, keyed by big-endian
/// sequence number.
const ENTRIES_PARTITION: &str = "entries";
/// fjall partition holding chain metadata (the head hash).
const META_PARTITION: &str = "meta";
/// Key, within the meta partition, of the persisted chain head hash.
const HEAD_KEY: &[u8] = b"head";

/// A single entry in the journal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Monotonically increasing sequence number.
    pub seq: u64,
    /// SHA-256 hash of the previous entry's content, or zeros for
    /// genesis.
    pub prev_hash: [u8; 32],
    /// Timestamp in nanoseconds since UNIX epoch.
    pub timestamp_ns: u128,
    /// The kind of event being journaled. Convention: lowercase
    /// snake_case matching the `serde(rename_all = "snake_case")`
    /// shape of [`hyphae_core::JournalEntryType`] (e.g.
    /// `"audit_memory_op"`, `"audit_ethics_evaluation"`,
    /// `"audit_learning_update"`, `"decision"`, `"reflection"`).
    pub event_kind: String,
    /// Event payload (serialised).
    pub payload: Vec<u8>,
}

impl JournalEntry {
    /// Compute the SHA-256 hash of this entry's content.
    ///
    /// Each field is fed into the digest with an explicit length
    /// prefix, so the hash is deterministic and infallible — it does
    /// not depend on any serialisation format and cannot fail.
    #[must_use]
    pub fn content_hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(self.seq.to_le_bytes());
        hasher.update(self.prev_hash);
        hasher.update(self.timestamp_ns.to_le_bytes());
        hasher.update((self.event_kind.len() as u64).to_le_bytes());
        hasher.update(self.event_kind.as_bytes());
        hasher.update((self.payload.len() as u64).to_le_bytes());
        hasher.update(&self.payload);
        hasher.finalize().into()
    }
}

/// Errors that can occur during journal operations.
#[derive(Debug, Error)]
pub enum JournalError {
    /// Hash chain integrity check failed at the given sequence
    /// number.
    #[error("hash chain integrity violation at seq {seq}: expected {expected:?}, found {found:?}")]
    IntegrityViolation {
        /// The sequence number at which the violation was detected.
        seq: u64,
        /// The expected hash.
        expected: [u8; 32],
        /// The hash actually found.
        found: [u8; 32],
    },
    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(String),
    /// A serialisation error occurred.
    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Append-only journal with hash chain.
///
/// Backed by `fjall` for the underlying storage. Each append extends
/// the hash chain and durably persists both the entry and the
/// updated chain head. Per ADR-0003 §8, a substrate operates a
/// single `Journal` instance — substrate events and ethics audit
/// share this one chain.
pub struct Journal {
    keyspace: Keyspace,
    entries: PartitionHandle,
    meta: PartitionHandle,
    last_hash: [u8; 32],
    next_seq: u64,
}

impl Journal {
    /// Open or create a journal at the given path.
    ///
    /// On reopen, the last stored entry is read to restore the chain
    /// head and the next sequence number, so appends resume
    /// seamlessly.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying storage cannot be opened
    /// or a stored entry cannot be deserialised.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, JournalError> {
        let keyspace = Config::new(path)
            .open()
            .map_err(|e| JournalError::Io(e.to_string()))?;
        let entries = keyspace
            .open_partition(ENTRIES_PARTITION, PartitionCreateOptions::default())
            .map_err(|e| JournalError::Io(e.to_string()))?;
        let meta = keyspace
            .open_partition(META_PARTITION, PartitionCreateOptions::default())
            .map_err(|e| JournalError::Io(e.to_string()))?;

        let (last_hash, next_seq) = match entries
            .last_key_value()
            .map_err(|e| JournalError::Io(e.to_string()))?
        {
            Some((_key, value)) => {
                let entry: JournalEntry = bincode::deserialize(&value)
                    .map_err(|e| JournalError::Serialization(e.to_string()))?;
                (entry.content_hash(), entry.seq + 1)
            }
            None => ([0u8; 32], 0),
        };

        Ok(Self {
            keyspace,
            entries,
            meta,
            last_hash,
            next_seq,
        })
    }

    /// Append an event to the journal. Returns the sequence number
    /// of the new entry and the hash that extends the chain.
    ///
    /// The entry and the updated chain head are durably persisted
    /// before the in-memory chain state is advanced.
    ///
    /// # Errors
    ///
    /// Returns an error if the system clock is before the UNIX
    /// epoch, the entry cannot be serialised, or the underlying
    /// storage write fails.
    pub fn append(
        &mut self,
        event_kind: impl Into<String>,
        payload: Vec<u8>,
    ) -> Result<(u64, [u8; 32]), JournalError> {
        let timestamp_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| JournalError::Io(e.to_string()))?
            .as_nanos();
        let entry = JournalEntry {
            seq: self.next_seq,
            prev_hash: self.last_hash,
            timestamp_ns,
            event_kind: event_kind.into(),
            payload,
        };
        let hash = entry.content_hash();
        let encoded =
            bincode::serialize(&entry).map_err(|e| JournalError::Serialization(e.to_string()))?;

        self.entries
            .insert(entry.seq.to_be_bytes(), &encoded)
            .map_err(|e| JournalError::Io(e.to_string()))?;
        self.meta
            .insert(HEAD_KEY, hash)
            .map_err(|e| JournalError::Io(e.to_string()))?;
        self.keyspace
            .persist(PersistMode::SyncAll)
            .map_err(|e| JournalError::Io(e.to_string()))?;

        self.last_hash = hash;
        self.next_seq = entry.seq + 1;
        Ok((entry.seq, hash))
    }

    /// Verify the integrity of the entire journal.
    ///
    /// Iterates every entry in sequence order, recomputing the hash
    /// chain, and checks the recomputed chain tail against the
    /// persisted head hash. This is what the substrate runs on
    /// entering the [`hyphae_core::State::Recovery`] state.
    ///
    /// # Errors
    ///
    /// Returns [`JournalError::IntegrityViolation`] if the chain is
    /// broken, or an I/O / serialisation error if a stored entry
    /// cannot be read.
    pub fn verify(&self) -> Result<(), JournalError> {
        let mut expected_prev = [0u8; 32];
        let mut last_seq: Option<u64> = None;

        for kv in self.entries.iter() {
            let (_key, value) = kv.map_err(|e| JournalError::Io(e.to_string()))?;
            let entry: JournalEntry = bincode::deserialize(&value)
                .map_err(|e| JournalError::Serialization(e.to_string()))?;
            if entry.prev_hash != expected_prev {
                return Err(JournalError::IntegrityViolation {
                    seq: entry.seq,
                    expected: expected_prev,
                    found: entry.prev_hash,
                });
            }
            expected_prev = entry.content_hash();
            last_seq = Some(entry.seq);
        }

        // The recomputed chain tail must match the persisted head
        // hash. This catches tampering with the final entry, which
        // has no successor.
        let stored_head = self
            .meta
            .get(HEAD_KEY)
            .map_err(|e| JournalError::Io(e.to_string()))?;
        match stored_head {
            Some(bytes) => {
                let mut head = [0u8; 32];
                if bytes.len() != head.len() {
                    return Err(JournalError::IntegrityViolation {
                        seq: last_seq.unwrap_or(0),
                        expected: expected_prev,
                        found: [0u8; 32],
                    });
                }
                head.copy_from_slice(&bytes);
                if head != expected_prev {
                    return Err(JournalError::IntegrityViolation {
                        seq: last_seq.unwrap_or(0),
                        expected: expected_prev,
                        found: head,
                    });
                }
            }
            None => {
                // A missing head is only consistent with an empty
                // journal.
                if last_seq.is_some() {
                    return Err(JournalError::IntegrityViolation {
                        seq: last_seq.unwrap_or(0),
                        expected: expected_prev,
                        found: [0u8; 32],
                    });
                }
            }
        }
        Ok(())
    }

    /// The current chain head: the content hash of the most recent
    /// entry (or all-zeros for an empty journal). This is the single
    /// value an external anchor signs (see [`crate::anchor`]).
    #[must_use]
    pub fn head(&self) -> [u8; 32] {
        self.last_hash
    }

    /// The number of entries in the journal.
    #[must_use]
    pub fn len(&self) -> u64 {
        self.next_seq
    }

    /// Whether the journal has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.next_seq == 0
    }

    /// Read the journal entry at the given sequence number, if it
    /// exists.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage read fails or the entry
    /// cannot be deserialised.
    pub fn read(&self, seq: u64) -> Result<Option<JournalEntry>, JournalError> {
        match self
            .entries
            .get(seq.to_be_bytes())
            .map_err(|e| JournalError::Io(e.to_string()))?
        {
            Some(value) => {
                let entry = bincode::deserialize(&value)
                    .map_err(|e| JournalError::Serialization(e.to_string()))?;
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    /// Flush buffered writes durably to disk.
    ///
    /// Appends already persist synchronously; this is an explicit
    /// durability barrier for callers.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying storage flush fails.
    pub fn flush(&self) -> Result<(), JournalError> {
        self.keyspace
            .persist(PersistMode::SyncAll)
            .map_err(|e| JournalError::Io(e.to_string()))
    }

    /// Overwrite the raw stored bytes of an entry. Test-only
    /// corruption hook used to verify that [`Journal::verify`]
    /// detects tampering.
    #[cfg(test)]
    pub(crate) fn overwrite_raw_for_test(
        &self,
        seq: u64,
        raw: Vec<u8>,
    ) -> Result<(), JournalError> {
        self.entries
            .insert(seq.to_be_bytes(), &raw)
            .map_err(|e| JournalError::Io(e.to_string()))?;
        self.keyspace
            .persist(PersistMode::SyncAll)
            .map_err(|e| JournalError::Io(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use tempfile::tempdir;

    #[test]
    fn journal_append_and_hash_chain_extends() {
        let dir = tempdir().unwrap();
        let mut journal = Journal::open(dir.path()).unwrap();
        let prev = journal.last_hash;
        let (seq, hash) = journal.append("test_event", b"payload1".to_vec()).unwrap();
        assert_eq!(seq, 0);
        assert_ne!(hash, prev, "the returned hash must extend the chain");
        assert_eq!(
            journal.last_hash, hash,
            "the persisted head matches the returned hash"
        );
        assert_ne!(journal.last_hash, prev, "hash chain must extend on append");
    }

    #[test]
    fn journal_verify_on_clean_journal_succeeds() {
        let dir = tempdir().unwrap();
        let mut journal = Journal::open(dir.path()).unwrap();
        journal.append("event_a", b"a".to_vec()).unwrap();
        journal.append("event_b", b"b".to_vec()).unwrap();
        assert!(journal.verify().is_ok());
    }

    #[test]
    fn journal_reopen_restores_chain_state() {
        let dir = tempdir().unwrap();
        {
            let mut journal = Journal::open(dir.path()).unwrap();
            journal.append("a", b"1".to_vec()).unwrap();
            journal.append("b", b"2".to_vec()).unwrap();
        }
        let mut journal = Journal::open(dir.path()).unwrap();
        assert!(journal.verify().is_ok());
        let (seq, _hash) = journal.append("c", b"3".to_vec()).unwrap();
        assert_eq!(seq, 2, "next_seq must resume after reopen");
        assert!(journal.verify().is_ok());
    }

    #[test]
    fn journal_read_and_len_reflect_appends() {
        let dir = tempdir().unwrap();
        let mut journal = Journal::open(dir.path()).unwrap();
        assert!(journal.is_empty());
        journal.append("alpha", b"one".to_vec()).unwrap();
        journal.append("beta", b"two".to_vec()).unwrap();
        assert_eq!(journal.len(), 2);
        assert!(!journal.is_empty());

        let entry = journal.read(1).unwrap().expect("entry 1 exists");
        assert_eq!(entry.event_kind, "beta");
        assert_eq!(entry.payload, b"two");
        assert!(journal.read(99).unwrap().is_none());

        journal.flush().unwrap();
    }

    /// Shared-chain semantics (ADR-0003 §8): substrate events and
    /// ethics-audit events live on the same chain, distinguished
    /// only by `event_kind`. Verify the chain stays valid across
    /// interleaved kinds.
    #[test]
    fn journal_shared_chain_interleaves_substrate_and_ethics_events() {
        let dir = tempdir().unwrap();
        let mut journal = Journal::open(dir.path()).unwrap();
        journal
            .append("audit_memory_op", b"remember:fragment_a".to_vec())
            .unwrap();
        journal
            .append("audit_ethics_evaluation", b"layer_a:ok".to_vec())
            .unwrap();
        journal
            .append("audit_memory_op", b"recall:cascade".to_vec())
            .unwrap();
        journal
            .append("audit_learning_update", b"conductivity:edge_42".to_vec())
            .unwrap();
        journal
            .append("audit_ethics_evaluation", b"layer_b:cvar=0.03".to_vec())
            .unwrap();
        assert!(journal.verify().is_ok());
        assert_eq!(journal.len(), 5);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn prop_arbitrary_appends_produce_valid_chain(
            events in prop::collection::vec(
                (".{0,12}", prop::collection::vec(any::<u8>(), 0..24)),
                0..20,
            )
        ) {
            let dir = tempdir().unwrap();
            let mut journal = Journal::open(dir.path()).unwrap();
            for (kind, payload) in events {
                journal.append(kind, payload).unwrap();
            }
            prop_assert!(journal.verify().is_ok());
        }

        #[test]
        fn prop_tampering_historical_entry_fails_verify(
            events in prop::collection::vec(
                (".{0,12}", prop::collection::vec(any::<u8>(), 0..24)),
                1..20,
            ),
            target in any::<usize>(),
        ) {
            let dir = tempdir().unwrap();
            let mut journal = Journal::open(dir.path()).unwrap();
            let count = events.len() as u64;
            for (kind, payload) in events {
                journal.append(kind, payload).unwrap();
            }
            prop_assert!(journal.verify().is_ok());

            let victim = (target as u64) % count;
            let raw = journal
                .entries
                .get(victim.to_be_bytes())
                .unwrap()
                .expect("stored entry must exist");
            let mut entry: JournalEntry = bincode::deserialize(&raw).unwrap();
            entry.payload.push(0xFF); // mutate so content_hash changes
            let tampered = bincode::serialize(&entry).unwrap();
            journal.overwrite_raw_for_test(victim, tampered).unwrap();

            prop_assert!(
                journal.verify().is_err(),
                "tampering with a stored entry must be detected",
            );
        }
    }
}
