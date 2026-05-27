// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Subsystem state store via redb.
//!
//! Each subsystem owns a region of the state store keyed by its
//! [`SubsystemId`]. State is read on subsystem startup and written
//! via `checkpoint`. All operations are individually transactional:
//! a write either commits fully or not at all.

use hyphae_core::SubsystemId;
use redb::{Database, TableDefinition};
use std::path::Path;

/// redb table mapping composite `(subsystem, key)` byte keys to
/// serialised state.
const STATE_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("subsystem_state");

/// Errors that can occur during state store operations.
#[derive(Debug, thiserror::Error)]
pub enum StateStoreError {
    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(String),
    /// A serialisation error occurred.
    #[error("serialization error: {0}")]
    Serialization(String),
    /// The key was not found.
    #[error("key not found: subsystem {0:?} key {1}")]
    NotFound(SubsystemId, String),
}

/// Stable one-byte discriminant for a subsystem, used as the key
/// prefix.
///
/// Assigned by hand so the on-disk key encoding is independent of
/// the declaration order of [`SubsystemId`] — reordering the enum
/// must not invalidate persisted state. v2 reserves bytes 1–6 for
/// the six v0.1 functional subsystems; bytes 7–99 are reserved for
/// future v0.x subsystem additions. Bytes 100+ are reserved for
/// post-v0.1 re-entrants (per ADR-0001 §"Postponed subsystems") so
/// re-introducing a postponed subsystem does not collide with a v0.1
/// byte.
fn subsystem_byte(subsystem: SubsystemId) -> u8 {
    match subsystem {
        SubsystemId::InputGate => 1,
        SubsystemId::Episodic => 2,
        SubsystemId::Valence => 3,
        SubsystemId::Composer => 4,
        SubsystemId::Predictive => 5,
        SubsystemId::Reward => 6,
    }
}

/// Build the composite storage key for a `(subsystem, key)` pair:
/// the subsystem discriminant byte followed by the UTF-8 key.
fn composite_key(subsystem: SubsystemId, key: &str) -> Vec<u8> {
    let mut composite = Vec::with_capacity(1 + key.len());
    composite.push(subsystem_byte(subsystem));
    composite.extend_from_slice(key.as_bytes());
    composite
}

/// Transactional state store backed by redb.
pub struct StateStore {
    db: Database,
}

impl StateStore {
    /// Open or create the state store at the given path.
    ///
    /// The backing table is created eagerly so that reads against a
    /// fresh store return `Ok(None)` rather than failing.
    ///
    /// # Errors
    ///
    /// Returns an error if the redb database cannot be created or
    /// opened.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StateStoreError> {
        let db = Database::create(path).map_err(|e| StateStoreError::Io(e.to_string()))?;
        let txn = db
            .begin_write()
            .map_err(|e| StateStoreError::Io(e.to_string()))?;
        txn.open_table(STATE_TABLE)
            .map_err(|e| StateStoreError::Io(e.to_string()))?;
        txn.commit()
            .map_err(|e| StateStoreError::Io(e.to_string()))?;
        Ok(Self { db })
    }

    /// Get a value from the store under `(subsystem, key)`.
    ///
    /// Returns `Ok(None)` if no value is stored under that pair.
    ///
    /// # Errors
    ///
    /// Returns an error if the read transaction fails.
    pub fn get(
        &self,
        subsystem: SubsystemId,
        key: &str,
    ) -> Result<Option<Vec<u8>>, StateStoreError> {
        let composite = composite_key(subsystem, key);
        let txn = self
            .db
            .begin_read()
            .map_err(|e| StateStoreError::Io(e.to_string()))?;
        let table = txn
            .open_table(STATE_TABLE)
            .map_err(|e| StateStoreError::Io(e.to_string()))?;
        let value = table
            .get(composite.as_slice())
            .map_err(|e| StateStoreError::Io(e.to_string()))?
            .map(|guard| guard.value().to_vec());
        Ok(value)
    }

    /// Put a value into the store under `(subsystem, key)`. Replaces
    /// any existing value.
    ///
    /// # Errors
    ///
    /// Returns an error if the write transaction fails to commit.
    pub fn put(
        &self,
        subsystem: SubsystemId,
        key: &str,
        value: Vec<u8>,
    ) -> Result<(), StateStoreError> {
        let composite = composite_key(subsystem, key);
        let txn = self
            .db
            .begin_write()
            .map_err(|e| StateStoreError::Io(e.to_string()))?;
        {
            let mut table = txn
                .open_table(STATE_TABLE)
                .map_err(|e| StateStoreError::Io(e.to_string()))?;
            table
                .insert(composite.as_slice(), value.as_slice())
                .map_err(|e| StateStoreError::Io(e.to_string()))?;
        }
        txn.commit()
            .map_err(|e| StateStoreError::Io(e.to_string()))?;
        Ok(())
    }

    /// Delete the value stored under `(subsystem, key)`, if any.
    ///
    /// Deleting an absent key is a no-op and is not an error.
    ///
    /// # Errors
    ///
    /// Returns an error if the write transaction fails to commit.
    pub fn delete(&self, subsystem: SubsystemId, key: &str) -> Result<(), StateStoreError> {
        let composite = composite_key(subsystem, key);
        let txn = self
            .db
            .begin_write()
            .map_err(|e| StateStoreError::Io(e.to_string()))?;
        {
            let mut table = txn
                .open_table(STATE_TABLE)
                .map_err(|e| StateStoreError::Io(e.to_string()))?;
            table
                .remove(composite.as_slice())
                .map_err(|e| StateStoreError::Io(e.to_string()))?;
        }
        txn.commit()
            .map_err(|e| StateStoreError::Io(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::collections::HashMap;
    use tempfile::tempdir;

    /// Every `SubsystemId` variant, for exhaustive test coverage.
    const ALL_SUBSYSTEMS: [SubsystemId; 6] = [
        SubsystemId::InputGate,
        SubsystemId::Episodic,
        SubsystemId::Valence,
        SubsystemId::Composer,
        SubsystemId::Predictive,
        SubsystemId::Reward,
    ];

    #[test]
    fn put_get_roundtrip() {
        let dir = tempdir().unwrap();
        let store = StateStore::open(dir.path().join("state")).unwrap();
        store
            .put(SubsystemId::Episodic, "trace", b"episode".to_vec())
            .unwrap();
        assert_eq!(
            store.get(SubsystemId::Episodic, "trace").unwrap(),
            Some(b"episode".to_vec())
        );
    }

    #[test]
    fn get_missing_key_returns_none() {
        let dir = tempdir().unwrap();
        let store = StateStore::open(dir.path().join("state")).unwrap();
        assert_eq!(store.get(SubsystemId::Composer, "absent").unwrap(), None);
    }

    #[test]
    fn put_overwrites_existing_value() {
        let dir = tempdir().unwrap();
        let store = StateStore::open(dir.path().join("state")).unwrap();
        store
            .put(SubsystemId::InputGate, "mode", b"tonic".to_vec())
            .unwrap();
        store
            .put(SubsystemId::InputGate, "mode", b"burst".to_vec())
            .unwrap();
        assert_eq!(
            store.get(SubsystemId::InputGate, "mode").unwrap(),
            Some(b"burst".to_vec())
        );
    }

    #[test]
    fn delete_removes_value() {
        let dir = tempdir().unwrap();
        let store = StateStore::open(dir.path().join("state")).unwrap();
        store
            .put(SubsystemId::Valence, "stamp", b"+1".to_vec())
            .unwrap();
        store.delete(SubsystemId::Valence, "stamp").unwrap();
        assert_eq!(store.get(SubsystemId::Valence, "stamp").unwrap(), None);
        // Deleting an absent key is a no-op.
        store.delete(SubsystemId::Valence, "stamp").unwrap();
    }

    #[test]
    fn same_key_is_isolated_across_subsystems() {
        let dir = tempdir().unwrap();
        let store = StateStore::open(dir.path().join("state")).unwrap();
        store
            .put(SubsystemId::Episodic, "k", b"episodic".to_vec())
            .unwrap();
        store
            .put(SubsystemId::Composer, "k", b"composer".to_vec())
            .unwrap();
        assert_eq!(
            store.get(SubsystemId::Episodic, "k").unwrap(),
            Some(b"episodic".to_vec())
        );
        assert_eq!(
            store.get(SubsystemId::Composer, "k").unwrap(),
            Some(b"composer".to_vec())
        );
    }

    #[test]
    fn state_persists_across_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state");
        {
            let store = StateStore::open(&path).unwrap();
            store
                .put(SubsystemId::Composer, "heartbeat", b"42".to_vec())
                .unwrap();
        }
        let store = StateStore::open(&path).unwrap();
        assert_eq!(
            store.get(SubsystemId::Composer, "heartbeat").unwrap(),
            Some(b"42".to_vec())
        );
    }

    /// Every subsystem variant maps to a distinct discriminant byte.
    /// Guards against an enum-reorder bug silently collapsing two
    /// subsystems onto one key prefix.
    #[test]
    fn subsystem_bytes_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for s in ALL_SUBSYSTEMS {
            assert!(
                seen.insert(subsystem_byte(s)),
                "duplicate discriminant for {s:?}",
            );
        }
    }

    #[derive(Debug, Clone)]
    enum Op {
        Put(SubsystemId, String, Vec<u8>),
        Delete(SubsystemId, String),
        Get(SubsystemId, String),
    }

    fn subsystem_strategy() -> impl Strategy<Value = SubsystemId> {
        (0usize..ALL_SUBSYSTEMS.len()).prop_map(|i| ALL_SUBSYSTEMS[i])
    }

    fn op_strategy() -> impl Strategy<Value = Op> {
        // A small key space so puts, gets and deletes collide on the
        // same keys.
        let key = "[a-c]{1,3}";
        prop_oneof![
            (
                subsystem_strategy(),
                key,
                prop::collection::vec(any::<u8>(), 0..8)
            )
                .prop_map(|(s, k, v)| Op::Put(s, k, v)),
            (subsystem_strategy(), key).prop_map(|(s, k)| Op::Delete(s, k)),
            (subsystem_strategy(), key).prop_map(|(s, k)| Op::Get(s, k)),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// An arbitrary sequence of put / get / delete operations
        /// leaves the store consistent with an in-memory reference
        /// model.
        #[test]
        fn prop_store_matches_reference_model(
            ops in prop::collection::vec(op_strategy(), 0..50)
        ) {
            let dir = tempdir().unwrap();
            let store = StateStore::open(dir.path().join("state")).unwrap();
            let mut model: HashMap<(u8, String), Vec<u8>> = HashMap::new();

            for op in ops {
                match op {
                    Op::Put(s, k, v) => {
                        store.put(s, &k, v.clone()).unwrap();
                        model.insert((subsystem_byte(s), k), v);
                    }
                    Op::Delete(s, k) => {
                        store.delete(s, &k).unwrap();
                        model.remove(&(subsystem_byte(s), k));
                    }
                    Op::Get(s, k) => {
                        let got = store.get(s, &k).unwrap();
                        let expected = model.get(&(subsystem_byte(s), k)).cloned();
                        prop_assert_eq!(got, expected);
                    }
                }
            }
        }
    }
}
