// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Conversational metacognition primitives — see
//! `docs/rfc/v1-living.md` §4.
//!
//! Hyphae maintains conversational state as a persistent system
//! property accessible to subsystems and operations throughout the
//! runtime — distinct from LLMs which reconstruct conversational
//! structure from the context window every turn. This module owns
//! the type vocabulary the `composer` subsystem composes against.
//!
//! The keyword extractor is single-language (English) per v0.1 — the
//! stop-word set is EN only. Additional languages re-enter with the
//! lexicon expansion ADR.

use crate::FragmentId;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

/// Process-monotonic counter for the low 8 bytes of a [`ThreadId`].
/// Separate from the `FragmentId` counter so threads and fragments
/// can claim the same low-bytes ranges without collision concerns.
static THREAD_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Per-process seed for the high 8 bytes of a [`ThreadId`], derived
/// once from the wall clock at first use. Distinguishes thread ids
/// minted by different runs from each other.
static THREAD_SEED: LazyLock<u64> = LazyLock::new(|| {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| {
            d.as_secs()
                .wrapping_mul(1_000_000_000)
                .wrapping_add(u64::from(d.subsec_nanos()))
        })
});

/// Unique identifier for a conversation thread. Same byte shape as
/// [`FragmentId`] — low 8 bytes a per-process monotonic counter,
/// high 8 bytes a per-process seed — so the two namespaces share
/// infrastructure but stay distinguishable through the type system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThreadId(pub [u8; 16]);

impl ThreadId {
    /// Generate a new unique thread identifier.
    #[must_use]
    pub fn new() -> Self {
        let count = THREAD_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&count.to_le_bytes());
        bytes[8..].copy_from_slice(&THREAD_SEED.to_le_bytes());
        Self(bytes)
    }
}

impl Default for ThreadId {
    fn default() -> Self {
        Self::new()
    }
}

/// Status of a thread within the conversational state. Threads open
/// `Active`, can be paused (user moves to another topic but the
/// thread is unresolved), and resolved (the topic has been answered
/// / closed). Topic-switch detection works against `Active` threads
/// only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadState {
    /// The thread is currently being discussed.
    Active,
    /// The thread was opened but the user has moved focus; it can be
    /// resumed.
    Paused,
    /// The thread has been answered or explicitly closed.
    Resolved,
}

/// A conversation thread. Holds the topic summary, status,
/// chronology, relationships, and the keyword set the `composer`'s
/// topic-switch detection algorithm matches against.
///
/// `keywords` are populated by the `composer` at thread-open time
/// via the pluggable [`KeywordExtractor`] trait so the extraction
/// algorithm can evolve without changing the thread struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationThread {
    /// Stable thread identifier.
    pub id: ThreadId,
    /// Short topic summary — the system's best one-line label for
    /// what this thread is about.
    pub topic: String,
    /// Current status.
    pub state: ThreadState,
    /// When this thread was first opened.
    pub opened_at: SystemTime,
    /// When the thread was last touched — updated on every matched
    /// user query or system response.
    pub last_touched_at: SystemTime,
    /// Parent thread for subtopic relationships. `None` for
    /// top-level threads.
    pub parent: Option<ThreadId>,
    /// Threads this one references (cross-thread reasoning).
    pub references: Vec<ThreadId>,
    /// Keyword set used by the topic-switch detection algorithm.
    /// Lowercase, deduplicated; intersection with an incoming
    /// query's keywords scores a match.
    pub keywords: Vec<String>,
}

/// A question raised in a thread that has not yet been answered.
/// Surfaced when the user returns to the thread ("we still had an
/// open question about X").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenQuestion {
    /// Thread this question belongs to.
    pub thread_id: ThreadId,
    /// The question text.
    pub question: String,
    /// When it was first raised.
    pub raised_at: SystemTime,
    /// Optional pointer to the fragment that contains the original
    /// question.
    pub raised_in_fragment: Option<FragmentId>,
}

/// A follow-up item the user explicitly asked for that has not yet
/// been addressed. Distinct from [`OpenQuestion`] in that the user
/// expects action, not just an answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingFollowup {
    /// Thread this follow-up belongs to.
    pub thread_id: ThreadId,
    /// The item description.
    pub item: String,
    /// When the user asked for it.
    pub requested_at: SystemTime,
    /// Optional pointer to the fragment that contains the original
    /// request.
    pub requested_in_fragment: Option<FragmentId>,
}

/// Source-of-truth event in the conversational log. Events are
/// retained as the log; the thread table is a derived view applied
/// from the log. Each event carries a monotonic `event_index` for
/// ordering and an optional `thread_id` for per-thread replay.
///
/// `Eq` is intentionally not derived: `AmbiguousThreadMatch` carries
/// `f32` scores, which only implement `PartialEq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversationEvent {
    /// Monotonically-increasing per-log event index. Lets a replay
    /// driver iterate events in order without sorting on timestamp.
    pub event_index: u64,
    /// When the event happened.
    pub at: SystemTime,
    /// Which thread this event belongs to. `None` for events that
    /// haven't been matched to a thread yet.
    pub thread_id: Option<ThreadId>,
    /// The event payload.
    pub kind: ConversationEventKind,
}

/// Discriminated event payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationEventKind {
    /// The user issued a query.
    UserQuery {
        /// Query text.
        text: String,
        /// Optional pointer to the encoded fragment.
        fragment_id: Option<FragmentId>,
    },
    /// The system emitted a response.
    SystemResponse {
        /// Response text.
        text: String,
        /// Optional pointer to the encoded fragment.
        fragment_id: Option<FragmentId>,
    },
    /// External signal: the user / orchestrator explicitly resumed a
    /// previously paused thread.
    ThreadResumed,
    /// External signal: the user / orchestrator explicitly closed a
    /// thread.
    ThreadClosed,
    /// External signal: the user / orchestrator explicitly opened a
    /// new thread.
    ThreadOpened {
        /// Topic summary supplied by the opener.
        topic: String,
    },
    /// Topic-switch detection returned multiple candidate threads
    /// above the configured threshold. The `composer` emits this
    /// event so the surface realizer can surface the ambiguity ("this
    /// could be thread A or B — which were you continuing?"); the
    /// `composer` itself does not resolve.
    AmbiguousThreadMatch {
        /// Candidate threads with their match scores. Sorted
        /// descending by score so consumers iterate
        /// highest-confidence first.
        candidates: Vec<(ThreadId, f32)>,
    },
}

/// Result of running the topic-switch detection algorithm against an
/// incoming query. The surface realizer consumes this outcome during
/// composition to frame the response appropriately ("we were
/// discussing X — moving to Y?").
///
/// Ambiguous matches surface a candidate list rather than silently
/// picking the highest score — the resolution is the realizer's
/// call, not the detection algorithm's.
#[derive(Debug, Clone, PartialEq)]
pub enum TopicSwitchOutcome {
    /// The query continues an existing active thread; carries the
    /// matched thread and its Jaccard score.
    ContinuesExisting {
        /// The thread whose keywords matched.
        thread: ThreadId,
        /// Match score in `[0, 1]` (Jaccard, optionally boosted by
        /// semantic similarity).
        match_score: f32,
    },
    /// No active thread matched above threshold; the detection
    /// algorithm opened a new thread for the query.
    OpensNewThread {
        /// The freshly-minted thread id.
        thread: ThreadId,
    },
    /// Two or more active threads matched above threshold. The
    /// detection algorithm does NOT pick — the candidate list is
    /// returned so the realizer can surface the choice.
    AmbiguousMatch {
        /// Candidates with their scores, sorted descending.
        candidates: Vec<(ThreadId, f32)>,
    },
    /// No active threads exist yet, or the empty-state code path.
    NoActiveThreads,
}

/// Read-only borrowed view of the conversational state the surface
/// realizer consumes during composition. Built per composition
/// invocation; realizer reads from it without mutating.
#[derive(Debug, Clone, Copy)]
pub struct ConversationContext<'a> {
    /// The outcome of `composer`'s topic-switch detection for the
    /// incoming query.
    pub topic_switch: &'a TopicSwitchOutcome,
    /// All currently-Active threads, most-recently-touched first.
    pub active_threads: &'a [&'a ConversationThread],
    /// All open questions from the thread table.
    pub open_questions: &'a [OpenQuestion],
    /// All pending follow-ups from the thread table.
    pub pending_followups: &'a [PendingFollowup],
}

/// Pre-formatted metacognitive prelude lines the composer can attach
/// to a response when the conversational state warrants surfacing it
/// ("we were discussing X — switching to Y?"; "there are 3 open
/// threads"; "we had an unresolved question from earlier").
///
/// Computed by a separate constructor ([`Self::from_context`]) so the
/// rendering algorithm can evolve and be tested independently of the
/// composition pipeline.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetacognitivePrelude {
    /// One natural-language line per metacognitive signal worth
    /// surfacing.
    pub lines: Vec<String>,
}

impl MetacognitivePrelude {
    /// Build a prelude from the supplied [`ConversationContext`].
    /// Emits one line per surface-worthy signal:
    ///   - On `TopicSwitchOutcome::AmbiguousMatch`: a disambiguation
    ///     prompt naming the candidates count.
    ///   - On `TopicSwitchOutcome::OpensNewThread`: a "switching
    ///     topic" line.
    ///   - When `active_threads.len() >= 2`: a "we have N open
    ///     threads" line.
    ///   - When `open_questions` is non-empty: an "unresolved
    ///     question" line per item (capped at the first 3 to avoid
    ///     wall-of-text output).
    ///   - When `pending_followups` is non-empty: a "pending
    ///     follow-up" line (capped at the first 3).
    ///
    /// An empty prelude (`lines.is_empty()`) is the normal case when
    /// the conversational state has nothing surface-worthy.
    #[must_use]
    pub fn from_context(ctx: &ConversationContext<'_>) -> Self {
        let mut lines = Vec::new();
        match ctx.topic_switch {
            TopicSwitchOutcome::AmbiguousMatch { candidates } => {
                lines.push(format!(
                    "Your query could continue {} different threads — which were you referring to?",
                    candidates.len(),
                ));
            }
            TopicSwitchOutcome::OpensNewThread { .. } => {
                if !ctx.active_threads.is_empty() {
                    lines.push("This looks like a new topic — opening a fresh thread.".to_string());
                }
            }
            TopicSwitchOutcome::ContinuesExisting { .. } | TopicSwitchOutcome::NoActiveThreads => {
                // Continuation and empty-table cases are silent at
                // the prelude level; the composition speaks for
                // itself.
            }
        }
        if ctx.active_threads.len() >= 2 {
            lines.push(format!(
                "We have {} open threads currently in play.",
                ctx.active_threads.len(),
            ));
        }
        for q in ctx.open_questions.iter().take(3) {
            lines.push(format!("Unresolved question: {}", q.question));
        }
        for f in ctx.pending_followups.iter().take(3) {
            lines.push(format!("Pending follow-up: {}", f.item));
        }
        Self { lines }
    }

    /// `true` when the prelude has no lines to surface.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// Output of `composer`'s context-aware composition. Carries the
/// composed fragment unchanged PLUS an optional
/// [`MetacognitivePrelude`] consumers can surface alongside the
/// fragment without baking presentation into the fragment body.
#[derive(Debug, Clone)]
pub struct ContextAwareComposition {
    /// The cognitive content.
    pub composition: crate::CognitiveFragment,
    /// Optional metacognitive prelude. `None` when the conversational
    /// state had nothing surface-worthy.
    pub prelude: Option<MetacognitivePrelude>,
}

/// Pluggable keyword extractor the `composer` uses to populate a
/// [`ConversationThread::keywords`] field at thread-open time.
/// Default impl is [`StopwordFilteringExtractor`].
pub trait KeywordExtractor: Send + Sync {
    /// Extract keywords from a piece of text. Returned values must
    /// be lowercase and deduplicated.
    fn extract(&self, text: &str) -> Vec<String>;
}

/// Simple stop-word-filtering extractor. Splits on non-alphanumerics,
/// lowercases, drops a short English stop-word set, deduplicates.
/// v0.1 is English-only per RFC §9; additional languages re-enter
/// with the lexicon expansion ADR.
#[derive(Debug, Default)]
pub struct StopwordFilteringExtractor;

const STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "if", "of", "in", "on", "at", "to", "for", "with", "is",
    "are", "was", "were", "be", "been", "being", "this", "that", "these", "those", "i", "you",
    "he", "she", "it", "we", "they",
];

impl KeywordExtractor for StopwordFilteringExtractor {
    fn extract(&self, text: &str) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for token in text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .map(str::to_lowercase)
        {
            if STOPWORDS.contains(&token.as_str()) {
                continue;
            }
            if seen.insert(token.clone()) {
                out.push(token);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_ids_are_unique() {
        let a = ThreadId::new();
        let b = ThreadId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn stopword_extractor_drops_english_function_words() {
        let extractor = StopwordFilteringExtractor;
        let kw = extractor.extract("The quick brown fox jumps over the lazy dog");
        assert!(!kw.contains(&"the".to_string()));
        assert!(kw.contains(&"quick".to_string()));
        assert!(kw.contains(&"fox".to_string()));
    }

    #[test]
    fn stopword_extractor_deduplicates() {
        let extractor = StopwordFilteringExtractor;
        let kw = extractor.extract("rust rust rust async async");
        assert_eq!(kw.len(), 2);
        assert!(kw.contains(&"rust".to_string()));
        assert!(kw.contains(&"async".to_string()));
    }

    #[test]
    fn conversation_thread_round_trips_through_bincode() {
        let thread = ConversationThread {
            id: ThreadId::new(),
            topic: "rust async".to_string(),
            state: ThreadState::Active,
            opened_at: SystemTime::now(),
            last_touched_at: SystemTime::now(),
            parent: None,
            references: vec![ThreadId::new()],
            keywords: vec!["rust".to_string(), "async".to_string()],
        };
        let bytes = bincode::serialize(&thread).unwrap();
        let restored: ConversationThread = bincode::deserialize(&bytes).unwrap();
        assert_eq!(thread, restored);
    }

    #[test]
    fn conversation_event_carries_monotonic_index() {
        let e1 = ConversationEvent {
            event_index: 0,
            at: SystemTime::now(),
            thread_id: None,
            kind: ConversationEventKind::UserQuery {
                text: "hello".to_string(),
                fragment_id: None,
            },
        };
        let e2 = ConversationEvent {
            event_index: 1,
            at: SystemTime::now(),
            thread_id: Some(ThreadId::new()),
            kind: ConversationEventKind::SystemResponse {
                text: "hi".to_string(),
                fragment_id: None,
            },
        };
        assert!(e2.event_index > e1.event_index);
    }

    #[test]
    fn thread_state_serialises_as_snake_case() {
        let state = ThreadState::Resolved;
        let bytes = bincode::serialize(&state).unwrap();
        let restored: ThreadState = bincode::deserialize(&bytes).unwrap();
        assert_eq!(state, restored);
    }
}
