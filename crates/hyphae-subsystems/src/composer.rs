// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Composer — bounded working memory and conversational metacognition.
//!
//! Collapses v1's `FrontalCortex + ACC`. Three responsibilities:
//!
//! - **Bounded working memory** (cap = 7 fragments per Miller 1956).
//!   FIFO eviction by default; the surface realizer reads the
//!   working set from here.
//! - **Conversation thread tracker** with Jaccard topic-switch
//!   detection. Persistent across substrate ticks; the metacognitive
//!   prelude (from `hyphae-core::thread`) is composed against this
//!   tracker.
//! - **Conflict signal** — emits a `confidence` value cabled to the
//!   ethics path from commit zero (corrects v1's
//!   `documented-pending` wire).
//!
//! Per ADR-0001 §"Subsystems collapsed", the composer owns
//! substrate-level coordination state (working memory + thread
//! table). v1's M3c review flagged that the executive composer is
//! the natural home for working memory; v2 makes that the
//! intentional collapse.

use hyphae_core::{
    CognitiveFragment, FragmentContent, KeywordExtractor, PayloadKind, Result, State,
    StopwordFilteringExtractor, Subsystem, SubsystemId, ThreadId, ThreadState,
};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;

/// Working-memory capacity (Miller 1956's `7 ± 2`). Fragments
/// beyond this are evicted in FIFO order — the integrator that
/// needs a different policy (LRU, by-saliency) can implement it
/// against the public API.
pub const WORKING_MEMORY_CAP: usize = 7;

/// Jaccard threshold for topic-switch detection. Inputs whose
/// keyword set shares fewer than this fraction with an active
/// thread's keywords are considered topic switches. v1's M3c chose
/// `0.30` empirically; v2 inherits.
pub const TOPIC_SWITCH_JACCARD: f32 = 0.30;

/// Snapshot of [`Composer`] state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ComposerSnapshot {
    working_memory: Vec<CognitiveFragment>,
    threads: HashMap<ThreadId, ThreadSnapshot>,
    active_thread: Option<ThreadId>,
    last_confidence: f32,
    compositions: u64,
}

/// Snapshot shape of one [`ConversationThread`] entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ThreadSnapshot {
    topic: String,
    state: ThreadState,
    keywords: Vec<String>,
}

/// Composer subsystem.
pub struct Composer {
    working_memory: Vec<CognitiveFragment>,
    threads: HashMap<ThreadId, ThreadSnapshot>,
    active_thread: Option<ThreadId>,
    /// Confidence of the most recent composition decision in
    /// `[0.0, 1.0]`. Cabled into the substrate's ethics path —
    /// `1.0 - last_confidence` is the conflict signal the ethics
    /// engine consumes.
    last_confidence: f32,
    /// Lifetime composition count.
    compositions: u64,
    /// Keyword extractor used by topic-switch detection.
    extractor: Box<dyn KeywordExtractor>,
}

impl std::fmt::Debug for Composer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Composer")
            .field("working_memory_size", &self.working_memory.len())
            .field("thread_count", &self.threads.len())
            .field("active_thread", &self.active_thread)
            .field("last_confidence", &self.last_confidence)
            .field("compositions", &self.compositions)
            .finish_non_exhaustive()
    }
}

impl Default for Composer {
    fn default() -> Self {
        Self {
            working_memory: Vec::with_capacity(WORKING_MEMORY_CAP),
            threads: HashMap::new(),
            active_thread: None,
            last_confidence: 1.0,
            compositions: 0,
            extractor: Box::new(StopwordFilteringExtractor),
        }
    }
}

impl Composer {
    /// Construct a composer with the default
    /// [`StopwordFilteringExtractor`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a composer with a custom keyword extractor.
    #[must_use]
    pub fn with_extractor(extractor: Box<dyn KeywordExtractor>) -> Self {
        Self {
            extractor,
            ..Self::default()
        }
    }

    /// Read-only view of the working memory in insertion order
    /// (oldest first).
    #[must_use]
    pub fn working_memory(&self) -> &[CognitiveFragment] {
        &self.working_memory
    }

    /// Number of fragments currently in working memory.
    #[must_use]
    pub fn working_memory_len(&self) -> usize {
        self.working_memory.len()
    }

    /// Most recent composition confidence in `[0.0, 1.0]`.
    /// `1.0 - last_confidence` is the conflict signal feeding the
    /// ethics path.
    #[must_use]
    pub fn last_confidence(&self) -> f32 {
        self.last_confidence
    }

    /// Conflict signal — `1.0 - last_confidence`. Cabled into the
    /// substrate's ethics path (the v1 `documented-pending` wire,
    /// now wired from commit zero).
    #[must_use]
    pub fn conflict_signal(&self) -> f32 {
        1.0 - self.last_confidence
    }

    /// Lifetime composition count.
    #[must_use]
    pub fn compositions(&self) -> u64 {
        self.compositions
    }

    /// Currently active thread, if any.
    #[must_use]
    pub fn active_thread(&self) -> Option<ThreadId> {
        self.active_thread
    }

    /// Snapshot of all known threads.
    #[must_use]
    pub fn threads(&self) -> Vec<(ThreadId, &str, ThreadState)> {
        self.threads
            .iter()
            .map(|(id, snap)| (*id, snap.topic.as_str(), snap.state))
            .collect()
    }

    /// Push a fragment into working memory. FIFO eviction when the
    /// cap is exceeded.
    fn push_working(&mut self, fragment: CognitiveFragment) {
        if self.working_memory.len() >= WORKING_MEMORY_CAP {
            self.working_memory.remove(0);
        }
        self.working_memory.push(fragment);
    }

    /// Match `keywords` against active threads. Returns the
    /// best-matching thread id and its Jaccard score, or `None`
    /// when no active thread is in the table.
    fn best_active_thread_match(&self, keywords: &[String]) -> Option<(ThreadId, f32)> {
        let mut best: Option<(ThreadId, f32)> = None;
        for (id, snap) in &self.threads {
            if snap.state != ThreadState::Active {
                continue;
            }
            let score = jaccard(keywords, &snap.keywords);
            if best.is_none_or(|(_, b)| score > b) {
                best = Some((*id, score));
            }
        }
        best
    }

    /// Open a new thread with the supplied keywords. Returns the
    /// fresh thread id.
    fn open_thread(&mut self, topic: String, keywords: Vec<String>) -> ThreadId {
        let id = ThreadId::new();
        self.threads.insert(
            id,
            ThreadSnapshot {
                topic,
                state: ThreadState::Active,
                keywords,
            },
        );
        self.active_thread = Some(id);
        id
    }

    /// Process an input through the conversational-metacognition
    /// pipeline. Updates the thread table and returns the topic
    /// switch outcome.
    fn route_to_thread(&mut self, body: &str) -> TopicRouting {
        let keywords = self.extractor.extract(body);
        if let Some((id, score)) = self.best_active_thread_match(&keywords) {
            if score >= TOPIC_SWITCH_JACCARD {
                self.active_thread = Some(id);
                return TopicRouting::ContinuesExisting { thread: id, score };
            }
        }
        let topic = body.chars().take(80).collect();
        let id = self.open_thread(topic, keywords);
        TopicRouting::OpensNewThread { thread: id }
    }

    /// Score the composer's confidence in the working set. v0.1
    /// uses a simple heuristic: confidence falls when the working
    /// set holds fragments of opposing valence (sign change between
    /// any two fragments). The integrator can replace this with a
    /// richer score by reading the working memory directly.
    fn score_confidence(&self) -> f32 {
        if self.working_memory.len() < 2 {
            return 1.0;
        }
        let mut has_pos = false;
        let mut has_neg = false;
        for f in &self.working_memory {
            if f.valence > 0.2 {
                has_pos = true;
            }
            if f.valence < -0.2 {
                has_neg = true;
            }
        }
        if has_pos && has_neg { 0.4 } else { 1.0 }
    }
}

/// Result of routing an input through the thread table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TopicRouting {
    /// The input continues an existing active thread.
    ContinuesExisting {
        /// The matched thread.
        thread: ThreadId,
        /// Jaccard score against the matched thread.
        score: f32,
    },
    /// The input is a topic switch — a new thread was opened.
    OpensNewThread {
        /// The fresh thread id.
        thread: ThreadId,
    },
}

impl Subsystem for Composer {
    fn id(&self) -> SubsystemId {
        SubsystemId::Composer
    }

    fn process(
        &mut self,
        mut fragment: CognitiveFragment,
        _incoming: PayloadKind,
        _state: State,
    ) -> Result<Vec<CognitiveFragment>> {
        // Extract the body once for thread routing AND working
        // memory ingestion.
        let body = match &fragment.content {
            FragmentContent::Episode { body, .. }
            | FragmentContent::Belief { body, .. }
            | FragmentContent::Goal { body, .. }
            | FragmentContent::Observation { body }
            | FragmentContent::Reflection { body, .. }
            | FragmentContent::Journal { body, .. } => body.clone(),
            FragmentContent::Reference { uri } => uri.clone(),
        };
        let _ = self.route_to_thread(&body);

        fragment.provenance.source_subsystem = "composer".to_string();
        self.push_working(fragment.clone());
        self.compositions += 1;
        self.last_confidence = self.score_confidence();

        Ok(vec![fragment])
    }

    fn checkpoint(&self) -> Result<Vec<u8>> {
        let snap = ComposerSnapshot {
            working_memory: self.working_memory.clone(),
            threads: self.threads.clone(),
            active_thread: self.active_thread,
            last_confidence: self.last_confidence,
            compositions: self.compositions,
        };
        bincode::serialize(&snap).map_err(|e| {
            hyphae_core::HyphaeError::Other(format!("composer checkpoint serialise: {e}"))
        })
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<()> {
        let snap: ComposerSnapshot = bincode::deserialize(bytes).map_err(|e| {
            hyphae_core::HyphaeError::Other(format!("composer restore deserialise: {e}"))
        })?;
        self.working_memory = snap.working_memory;
        self.threads = snap.threads;
        self.active_thread = snap.active_thread;
        self.last_confidence = snap.last_confidence;
        self.compositions = snap.compositions;
        Ok(())
    }

    fn on_state_change(&mut self, _old: State, new: State) -> Result<()> {
        // Recovery wipes the working memory and the active thread —
        // post-crash, neither is authoritative. Threads themselves
        // are preserved: their Active state stays, but the
        // composer's pointer to "which one is in focus right now"
        // resets.
        if new == State::Recovery {
            self.working_memory.clear();
            self.active_thread = None;
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Jaccard similarity between two keyword sets. Each set is
/// interpreted as a deduplicated unordered collection.
fn jaccard(a: &[String], b: &[String]) -> f32 {
    use std::collections::HashSet;
    let aset: HashSet<&str> = a.iter().map(String::as_str).collect();
    let bset: HashSet<&str> = b.iter().map(String::as_str).collect();
    let intersection = aset.intersection(&bset).count();
    let union = aset.union(&bset).count();
    if union == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let result = intersection as f32 / union as f32;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyphae_core::FragmentId;

    fn obs(body: &str) -> CognitiveFragment {
        CognitiveFragment::new(
            FragmentContent::Observation {
                body: body.to_string(),
            },
            "test",
        )
    }

    fn obs_with_valence(body: &str, v: f32) -> CognitiveFragment {
        let mut f = obs(body);
        f.valence = v;
        f
    }

    #[test]
    fn working_memory_caps_at_seven() {
        let mut c = Composer::new();
        for i in 0..10 {
            c.process(
                obs(&format!("input {i}")),
                PayloadKind::Encoding,
                State::Encoding,
            )
            .unwrap();
        }
        assert_eq!(c.working_memory_len(), WORKING_MEMORY_CAP);
    }

    #[test]
    fn working_memory_evicts_fifo() {
        let mut c = Composer::new();
        let mut first_id = None;
        for i in 0..(WORKING_MEMORY_CAP + 2) {
            let f = obs(&format!("input {i}"));
            if i == 0 {
                first_id = Some(f.id);
            }
            c.process(f, PayloadKind::Encoding, State::Encoding)
                .unwrap();
        }
        let ids: Vec<FragmentId> = c.working_memory().iter().map(|f| f.id).collect();
        assert!(
            !ids.contains(&first_id.unwrap()),
            "first inserted fragment must have been evicted",
        );
        assert_eq!(ids.len(), WORKING_MEMORY_CAP);
    }

    #[test]
    fn first_input_opens_a_new_thread() {
        let mut c = Composer::new();
        c.process(
            obs("rust async runtime questions"),
            PayloadKind::Encoding,
            State::Encoding,
        )
        .unwrap();
        assert_eq!(c.threads().len(), 1);
        assert!(c.active_thread().is_some());
    }

    #[test]
    fn similar_input_continues_existing_thread() {
        let mut c = Composer::new();
        c.process(
            obs("rust async runtime questions tokio"),
            PayloadKind::Encoding,
            State::Encoding,
        )
        .unwrap();
        let initial_count = c.threads().len();
        c.process(
            obs("more questions about rust async tokio runtime"),
            PayloadKind::Encoding,
            State::Encoding,
        )
        .unwrap();
        assert_eq!(
            c.threads().len(),
            initial_count,
            "should not open new thread"
        );
    }

    #[test]
    fn dissimilar_input_opens_new_thread() {
        let mut c = Composer::new();
        c.process(
            obs("rust async runtime"),
            PayloadKind::Encoding,
            State::Encoding,
        )
        .unwrap();
        c.process(
            obs("photography landscape sunset mountains"),
            PayloadKind::Encoding,
            State::Encoding,
        )
        .unwrap();
        assert_eq!(
            c.threads().len(),
            2,
            "topic switch should open a new thread"
        );
    }

    #[test]
    fn confidence_drops_when_working_set_holds_opposing_valences() {
        let mut c = Composer::new();
        c.process(
            obs_with_valence("positive", 0.8),
            PayloadKind::Encoding,
            State::Encoding,
        )
        .unwrap();
        c.process(
            obs_with_valence("negative", -0.8),
            PayloadKind::Encoding,
            State::Encoding,
        )
        .unwrap();
        assert!(
            c.last_confidence() < 1.0,
            "opposing valences must lower confidence",
        );
        assert!(c.conflict_signal() > 0.0, "conflict signal must rise");
    }

    #[test]
    fn confidence_stays_high_with_consistent_valences() {
        let mut c = Composer::new();
        c.process(
            obs_with_valence("positive a", 0.6),
            PayloadKind::Encoding,
            State::Encoding,
        )
        .unwrap();
        c.process(
            obs_with_valence("positive b", 0.5),
            PayloadKind::Encoding,
            State::Encoding,
        )
        .unwrap();
        assert!((c.last_confidence() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn recovery_clears_working_memory_but_preserves_threads() {
        let mut c = Composer::new();
        c.process(obs("a"), PayloadKind::Encoding, State::Encoding)
            .unwrap();
        c.process(obs("b"), PayloadKind::Encoding, State::Encoding)
            .unwrap();
        let thread_count_before = c.threads().len();
        c.on_state_change(State::Encoding, State::Recovery).unwrap();
        assert_eq!(c.working_memory_len(), 0);
        assert_eq!(c.threads().len(), thread_count_before);
        assert!(c.active_thread().is_none());
    }

    #[test]
    fn checkpoint_restore_round_trips() {
        let mut c = Composer::new();
        c.process(obs("hello world"), PayloadKind::Encoding, State::Encoding)
            .unwrap();
        c.process(obs("more content"), PayloadKind::Encoding, State::Encoding)
            .unwrap();
        let bytes = c.checkpoint().unwrap();
        let mut restored = Composer::new();
        restored.restore(&bytes).unwrap();
        assert_eq!(restored.working_memory_len(), c.working_memory_len());
        assert_eq!(restored.threads().len(), c.threads().len());
        assert_eq!(restored.compositions(), c.compositions());
    }
}
