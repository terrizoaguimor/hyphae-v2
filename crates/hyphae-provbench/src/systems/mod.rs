// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Systems under test. Add a variant here to plug a third party's
//! verifiable-generation system into the same protocol.

pub mod echo;
pub mod journal;

pub use echo::EchoNoJournal;
pub use journal::VerbatimJournal;
