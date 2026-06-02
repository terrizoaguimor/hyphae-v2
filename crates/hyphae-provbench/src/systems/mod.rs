// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Systems under test. Add a variant here to plug a third party's
//! verifiable-generation system into the same protocol.

pub mod echo;
pub mod journal;
pub mod merkle_log;
pub mod signed_entries;

pub use echo::EchoNoJournal;
pub use journal::VerbatimJournal;
pub use merkle_log::MerkleLog;
pub use signed_entries::SignedEntries;
