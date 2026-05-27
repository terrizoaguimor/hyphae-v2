// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Rollback via journal replay.
//!
//! Per `docs/adr/0002-learning-loop-firstclass.md` §"Audit and
//! rollback":
//!
//! > Rollback = replay the journal up to entry N, recomputing
//! > parameter state.
//!
//! The replay walks every `audit_learning_update` entry on the
//! substrate's shared SHA-256 chain up to (and including) the
//! target sequence, deserialises each entry's
//! [`LearningUpdateAuditPayload`], and applies the new value to the
//! store. Entries on the chain that are *not*
//! `audit_learning_update` are skipped — the chain holds many event
//! kinds (memory ingest, ethics evaluations, journal writes), only
//! learning entries shape parameter state.
//!
//! The replay starts from an empty store: rollback is to a state,
//! not a relative delta. The caller is responsible for not feeding
//! the substrate any parameter values produced by the store while a
//! rollback is in progress.

use crate::parameters::{ParameterStore, ParameterValue};
use hyphae_storage::{Journal, JournalError};
use hyphae_substrate::{LearningTarget, LearningUpdateAuditPayload};
use thiserror::Error;

/// Errors raised by a rollback replay.
#[derive(Debug, Error)]
pub enum RollbackError {
    /// The underlying journal read failed.
    #[error("journal read failed: {0}")]
    Journal(#[from] JournalError),
    /// An entry's payload could not be deserialised as a learning
    /// update. The chain is corrupt or carries an entry of a
    /// different shape under the `audit_learning_update`
    /// `event_kind`.
    #[error("audit payload deserialisation failed at seq {seq}: {detail}")]
    Deserialisation {
        /// The journal sequence where the failure happened.
        seq: u64,
        /// Underlying serde error message.
        detail: String,
    },
    /// An entry's `target_tag` did not match any known
    /// [`LearningTarget`] shape. Either the chain is from a future
    /// version of the substrate or the entry was hand-injected.
    #[error("unknown learning target tag {tag:?} at seq {seq}")]
    UnknownTargetTag {
        /// The unknown tag.
        tag: String,
        /// The journal sequence.
        seq: u64,
    },
    /// The new-value bytes for an entry could not be deserialised
    /// into a [`ParameterValue`].
    #[error("parameter value bytes invalid at seq {seq}: {detail}")]
    ValueDecode {
        /// The journal sequence.
        seq: u64,
        /// Underlying serde error message.
        detail: String,
    },
}

/// Replay every `audit_learning_update` entry up to and including
/// `up_to_seq`, writing the resulting state into `store`. The
/// store is **cleared** before the replay so the result is
/// deterministic in the chain — earlier values that no longer
/// appear in the up-to range are dropped, which is the rollback
/// semantics callers expect.
///
/// Returns the number of update entries applied.
///
/// # Errors
///
/// Returns a [`RollbackError`] if the journal read fails, a payload
/// cannot be deserialised, or a target tag is unknown.
pub fn replay_to(
    journal: &Journal,
    store: &mut ParameterStore,
    up_to_seq: u64,
) -> Result<usize, RollbackError> {
    // Wipe the store. Replay is to-a-state, not relative.
    *store = ParameterStore::new();

    let mut applied = 0usize;
    for seq in 0..=up_to_seq {
        let Some(entry) = journal.read(seq)? else {
            // Sparse journals are not possible today (sequence is
            // dense), but the loop guards against a future
            // sequence-skipping mode.
            continue;
        };
        if entry.event_kind != "audit_learning_update" {
            continue;
        }
        let payload: LearningUpdateAuditPayload =
            bincode::deserialize(&entry.payload).map_err(|e| RollbackError::Deserialisation {
                seq,
                detail: e.to_string(),
            })?;
        let target =
            reconstruct_target(&payload.target_tag, &payload.rationale).ok_or_else(|| {
                RollbackError::UnknownTargetTag {
                    tag: payload.target_tag.clone(),
                    seq,
                }
            })?;
        let value = ParameterValue::from_bytes(&payload.new_value).map_err(|e| {
            RollbackError::ValueDecode {
                seq,
                detail: e.to_string(),
            }
        })?;
        store.apply_audited(&target, value);
        applied += 1;
    }
    Ok(applied)
}

/// Reconstruct a [`LearningTarget`] from the `target_tag` and the
/// audit rationale.
///
/// The substrate's audit payload only carries `target_tag` (the
/// parameter family) plus the freeform `rationale`. The
/// reconstruction here parses the identifier component out of the
/// rationale's well-known prefix shapes that
/// `intents_from_signals` produces. This is a v0.1 scaffold —
/// future versions of the substrate's audit payload should carry
/// the structured target itself, not a flattened tag.
///
/// Returns `None` when the tag does not correspond to any known
/// [`LearningTarget`] variant. The caller surfaces this as a
/// [`RollbackError::UnknownTargetTag`].
fn reconstruct_target(target_tag: &str, rationale: &str) -> Option<LearningTarget> {
    match target_tag {
        "episodic.conductivity_weight" => {
            // Rationale shape produced by `intents_from_signals`:
            // "reward prediction error +0.4000 on edge a:b" — the
            // edge id follows " on edge ". For test paths that do
            // not use the generator, the rationale carries no edge
            // marker; fall back to a deterministic placeholder so
            // the rollback proceeds with a generic key.
            let edge_id = rationale
                .rsplit_once(" on edge ")
                .map_or_else(|| "unknown".to_string(), |(_, e)| e.trim().to_string());
            Some(LearningTarget::EpisodicConductivityWeight { edge_id })
        }
        "valence.salience_weight" => {
            let category = rationale
                .rsplit_once(" on category ")
                .map_or_else(|| "unknown".to_string(), |(_, c)| c.trim().to_string());
            Some(LearningTarget::ValenceSalienceWeight { category })
        }
        "cascade.parameter" => {
            // The cascade family rationales produced by the
            // generator do not carry the parameter name in a
            // grep-friendly shape; the integrator embeds the name
            // in the rationale's tail. Fall back to a deterministic
            // placeholder when missing.
            let name = if let Some((_, n)) = rationale.rsplit_once(" parameter ") {
                n.trim().to_string()
            } else if rationale.contains("confabulation floor") {
                "confabulation_floor".to_string()
            } else {
                "unknown".to_string()
            };
            Some(LearningTarget::CascadeParameter { name })
        }
        "composer.schema_prior" => Some(LearningTarget::ComposerSchemaPrior {
            schema_id: "unknown".to_string(),
        }),
        "composer.limitation_threshold" => Some(LearningTarget::ComposerLimitationThreshold {
            trigger_id: "unknown".to_string(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyphae_core::ActorContext;
    use hyphae_substrate::{LearningUpdateProposal, Substrate};
    use tempfile::tempdir;

    async fn drive_one_update(substrate: &Substrate, edge_id: &str, new_value: ParameterValue) {
        let bytes = new_value.to_bytes().unwrap();
        let proposal = LearningUpdateProposal {
            target: LearningTarget::EpisodicConductivityWeight {
                edge_id: edge_id.to_string(),
            },
            old_value: Vec::new(),
            new_value: bytes,
            triggered_by: None,
            rationale: format!("reward prediction error +0.0500 on edge {edge_id}"),
        };
        substrate
            .propose_learning_update(proposal, ActorContext::system())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn replay_reconstructs_two_sequential_updates() {
        let dir = tempdir().unwrap();
        let substrate = Substrate::new(dir.path()).unwrap();
        drive_one_update(&substrate, "a:b", ParameterValue::Scalar(0.7)).await;
        drive_one_update(&substrate, "c:d", ParameterValue::Scalar(0.3)).await;

        // Open a fresh journal handle pointing at the same path —
        // the rollback path does not require coordination with the
        // live substrate's handle.
        let journal = hyphae_storage::Journal::open(dir.path().join("journal")).unwrap();
        let mut store = ParameterStore::new();
        // Both updates land before seq 4 inclusively — Remember
        // ethics audit (0) + nothing? Actually `propose_learning_update`
        // produces TWO entries: the ethics audit for the
        // LearningUpdate point (audit_ethics_evaluation) and then
        // the audit_learning_update entry. So for two updates we
        // have seq 0..=3. Replay up to 3 should apply both.
        let applied = replay_to(&journal, &mut store, 3).unwrap();
        assert_eq!(applied, 2, "two learning updates should be applied");
        let v_ab = store
            .get(&LearningTarget::EpisodicConductivityWeight {
                edge_id: "a:b".to_string(),
            })
            .unwrap();
        assert_eq!(*v_ab, ParameterValue::Scalar(0.7));
        let v_cd = store
            .get(&LearningTarget::EpisodicConductivityWeight {
                edge_id: "c:d".to_string(),
            })
            .unwrap();
        assert_eq!(*v_cd, ParameterValue::Scalar(0.3));
    }

    #[tokio::test]
    async fn replay_to_seq_n_excludes_later_updates() {
        let dir = tempdir().unwrap();
        let substrate = Substrate::new(dir.path()).unwrap();
        drive_one_update(&substrate, "a:b", ParameterValue::Scalar(0.5)).await;
        drive_one_update(&substrate, "a:b", ParameterValue::Scalar(0.9)).await;

        // Replay only the first update: seq 0 (ethics audit) + seq
        // 1 (learning audit). The second pair is at 2+3 — we stop
        // before that.
        let journal = hyphae_storage::Journal::open(dir.path().join("journal")).unwrap();
        let mut store = ParameterStore::new();
        let applied = replay_to(&journal, &mut store, 1).unwrap();
        assert_eq!(applied, 1);
        let v = store
            .get(&LearningTarget::EpisodicConductivityWeight {
                edge_id: "a:b".to_string(),
            })
            .unwrap();
        assert_eq!(*v, ParameterValue::Scalar(0.5));
    }

    /// Test-only passthrough input-gate used by
    /// [`replay_skips_non_learning_entries`]. Defined at module
    /// scope so the clippy `items_after_statements` lint stays
    /// quiet inside the async test body.
    struct Passthrough;

    impl hyphae_core::Subsystem for Passthrough {
        fn id(&self) -> hyphae_core::SubsystemId {
            hyphae_core::SubsystemId::InputGate
        }
        fn process(
            &mut self,
            fragment: hyphae_core::CognitiveFragment,
            _: hyphae_core::PayloadKind,
            _: hyphae_core::State,
        ) -> hyphae_core::Result<Vec<hyphae_core::CognitiveFragment>> {
            Ok(vec![fragment])
        }
        fn checkpoint(&self) -> hyphae_core::Result<Vec<u8>> {
            Ok(Vec::new())
        }
        fn restore(&mut self, _: &[u8]) -> hyphae_core::Result<()> {
            Ok(())
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    #[tokio::test]
    async fn replay_skips_non_learning_entries() {
        // A pure ethics-evaluation journal: the substrate's
        // `compose_signal` writes audit_ethics_evaluation entries
        // without any learning audit entries. Replay should
        // produce an empty store.
        use hyphae_core::ExternalInputPayload;
        let dir = tempdir().unwrap();
        let mut substrate = Substrate::new(dir.path()).unwrap();
        // Register a passthrough input-gate so the ingest doesn't
        // crash on a missing subsystem.
        substrate.register(Box::new(Passthrough)).unwrap();
        substrate
            .ingest(ExternalInputPayload::new("hello"), ActorContext::system())
            .await
            .unwrap();

        let journal = hyphae_storage::Journal::open(dir.path().join("journal")).unwrap();
        let mut store = ParameterStore::new();
        let applied = replay_to(&journal, &mut store, 10).unwrap();
        assert_eq!(applied, 0);
        assert!(store.is_empty());
    }
}
