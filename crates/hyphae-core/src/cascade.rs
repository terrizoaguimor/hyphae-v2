// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Cascade activation primitives — see `docs/rfc/v1-living.md` §3.
//!
//! Spreading activation (Collins & Loftus 1975) over the causal
//! fragment network. The mathematical formalism, per Siew 2019
//! (`spreadr` R package, DOI 10.3758/s13428-018-1186-5):
//!
//! ```text
//! A_j(t+1) += [(1 - retention) * A_i(t)] * [w_ij / Σ_k w_ik]
//! A_i(t+1) = A_i(t+1) * (1 - decay_rate)
//! if A_i < θ:        stop propagating from i
//! if hop_count > max_hops: stop
//! ```
//!
//! This module ships the **type vocabulary** the algorithm uses.
//! The propagation engine itself lives in the `predictive` subsystem
//! (per Marko et al. 2026, DOI 10.1162/nol.a.235 — cerebellar /
//! prefrontal complementarity). The `episodic` subsystem produces
//! the initial activation sources and the conductivity weights
//! `w_ij`; `predictive` iterates; `composer` filters and monitors
//! conflict.
//!
//! Per ADR-0002, cascade parameters are refinable by the learning
//! loop within the bounds documented in `docs/rfc/v1-living.md` §3.2.

use crate::{CognitiveFragment, FragmentId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

/// Activation level of a fragment in the cascade. Always in
/// `[0.0, 1.0]`; the constructor clamps and the public API preserves
/// the bound.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ActivationLevel(pub f32);

impl ActivationLevel {
    /// No activation.
    pub const ZERO: Self = Self(0.0);
    /// Fully activated.
    pub const ONE: Self = Self(1.0);

    /// New activation level, clamped to `[0.0, 1.0]`.
    #[must_use]
    pub fn new(value: f32) -> Self {
        Self(value.clamp(0.0, 1.0))
    }

    /// Is this activation at or above the supplied threshold? The
    /// threshold is also an `ActivationLevel`, preventing accidental
    /// misuse of a decay rate or retention coefficient.
    #[must_use]
    pub fn is_above_threshold(self, threshold: Self) -> bool {
        self.0 >= threshold.0
    }

    /// Apply per-iteration decay. Defends against out-of-range
    /// `rate` by clamping to `[0.0, 1.0]` before applying.
    #[must_use]
    pub fn decay(self, rate: f32) -> Self {
        Self::new(self.0 * (1.0 - rate.clamp(0.0, 1.0)))
    }

    /// Hot-path decay — skips the rate-clamp and trusts the caller.
    /// Multiplying by `(1 - rate)` where `rate ∈ [0, 1]` preserves
    /// the `[0, 1]` invariant of the activation, so the elision is
    /// safe. Use in the cascade inner loop only, behind a comment
    /// documenting the precondition.
    #[must_use]
    #[inline]
    pub fn decay_unchecked(self, rate: f32) -> Self {
        Self(self.0 * (1.0 - rate))
    }

    /// Saturating add: pushes the activation up by `delta`, clamping
    /// at `Self::ONE`. The cascade pass uses this to accumulate
    /// contributions from multiple incoming neighbours in a single
    /// iteration.
    #[must_use]
    pub fn saturating_add(self, delta: f32) -> Self {
        Self::new(self.0 + delta)
    }
}

/// Conductivity weight `w_ij` of an edge between two fragments in
/// the causal network. **Unbounded** — raw co-occurrence /
/// similarity / access-frequency counts. Normalisation happens at
/// the propagation site (`w_ij / Σ_k w_ik`), not at construction.
/// Negative weights are clamped to zero defensively.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ConductivityWeight(pub f32);

impl ConductivityWeight {
    /// Zero conductivity — fragments not connected.
    pub const ZERO: Self = Self(0.0);

    /// New weight, defensively clamped to `>= 0.0`.
    #[must_use]
    pub fn new(value: f32) -> Self {
        Self(value.max(0.0))
    }

    /// Normalised weight in `[0, 1]`, given the sum of weights from
    /// the source node `Σ_k w_ik`. Returns `0.0` when the source has
    /// no outbound conductivity at all (defensive against
    /// divide-by-zero).
    #[must_use]
    pub fn normalised(self, total_from_source: f32) -> f32 {
        if total_from_source > 0.0 {
            self.0 / total_from_source
        } else {
            0.0
        }
    }
}

/// Cascade-pass parameters. Per-deployment calibration knobs.
///
/// Defaults from the spreadr literature (Siew 2019); production
/// values are refined by the learning loop (`hyphae-learning`)
/// within bounds documented in `docs/rfc/v1-living.md` §3 and
/// `docs/adr/0002-learning-loop-firstclass.md` §"Refinable
/// parameters".
///
/// Runtime recalibration mid-propagation is a footgun — changing
/// `decay_rate` while a cascade is in flight produces inconsistent
/// activation maps. The learning loop's update mechanism is
/// between-cascade, not within-cascade.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CascadeParams {
    /// Fraction of activation that stays at the source node per
    /// iteration. `(1 - retention)` is distributed to neighbours.
    /// Range: `[0, 1]`. Default: 0.5.
    pub retention: f32,
    /// Per-iteration multiplicative decay applied to every
    /// activation. Range: `[0, 1]`. Default: 0.3.
    pub decay_rate: f32,
    /// Activation cutoff. Nodes whose activation falls below this
    /// cease propagating; the iteration terminates when no node
    /// above threshold has unpropagated activation. Default: 0.05.
    pub threshold: ActivationLevel,
    /// Hard upper bound on iteration count. Defends against
    /// pathological topologies producing unbounded propagation.
    /// Default: 4.
    pub max_hops: u8,
    /// Coefficient applied to the raw co-encoding count when
    /// composing the conductivity weight. Default: 1.0.
    pub co_encoding_weight: f32,
    /// Coefficient applied to the raw co-retrieval count when
    /// composing the conductivity weight. Default: 1.0.
    pub co_retrieval_weight: f32,
    /// Coefficient applied to the semantic-similarity term when
    /// composing the conductivity weight. Default: 0.5.
    pub semantic_weight: f32,
    /// Co-encoding window in seconds — two fragments encoded within
    /// this many seconds of each other increment their co-encoding
    /// edge weight. Default: 60.
    pub co_encoding_window_secs: u64,
    /// Coefficient on the cascade activation level term in the
    /// `composer` working-set scoring function. Default: 0.40.
    pub weight_activation: f32,
    /// Coefficient on the query-relevance term. Default: 0.30.
    pub weight_relevance: f32,
    /// Coefficient on the fragment-saliency term. Default: 0.20.
    pub weight_saliency: f32,
    /// Coefficient on the recency term. Default: 0.10.
    pub weight_recency: f32,
    /// Cardinality cap on the composition working set the `composer`
    /// extracts from the cascade output. Default: 7 (Miller's
    /// `7 ± 2`).
    pub working_set_size: u8,
    /// Maximum recursion depth for the surface realizer's slot-binding
    /// backtrack. When a required slot has no candidate satisfying
    /// its constraints, the binder unwinds a previously-bound
    /// optional slot and retries; depth caps how many such unwinds
    /// are attempted before raising a binding failure. Default: 2.
    pub backtrack_depth: u8,
}

impl CascadeParams {
    /// Defaults from the `spreadr` reference implementation
    /// (Siew 2019, DOI 10.3758/s13428-018-1186-5).
    pub const SPREADR_DEFAULTS: Self = Self {
        retention: 0.5,
        decay_rate: 0.3,
        threshold: ActivationLevel(0.05),
        max_hops: 4,
        co_encoding_weight: 1.0,
        co_retrieval_weight: 1.0,
        semantic_weight: 0.5,
        co_encoding_window_secs: 60,
        weight_activation: 0.40,
        weight_relevance: 0.30,
        weight_saliency: 0.20,
        weight_recency: 0.10,
        working_set_size: 7,
        backtrack_depth: 2,
    };
}

impl Default for CascadeParams {
    fn default() -> Self {
        Self::SPREADR_DEFAULTS
    }
}

/// A single fragment's state in a cascade activation pass. Returned
/// by the propagation engine; consumed by `composer` (filter pass +
/// conflict monitor).
///
/// Path is stored as the **immediate parent only**
/// (`parent_id: Option<FragmentId>`). Full activation paths are
/// reconstructed on demand by [`Self::trace_path`], walking the
/// parent chain through the cascade's activation map. This keeps
/// per-activation memory at `O(8 bytes)` instead of
/// `O(8 × max_hops + Vec overhead)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CascadeActivation {
    /// The activated fragment.
    pub fragment_id: FragmentId,
    /// Its current activation level.
    pub activation: ActivationLevel,
    /// Number of hops from the nearest initial activation source.
    /// `0` means this fragment IS an initial source (direct retrieval
    /// hit).
    pub hops_from_source: u8,
    /// The immediate parent fragment that propagated activation to
    /// this one — the largest-contributing source from the iteration
    /// in which this fragment first received activation. `None` for
    /// initial activation sources.
    pub parent_id: Option<FragmentId>,
    /// Fire-once semantics: `true` once this node has propagated its
    /// activation to neighbours. The cascade pass uses this to
    /// implement single-propagation: each node propagates exactly
    /// once, in the iteration where it first crosses threshold.
    #[serde(default)]
    pub propagated: bool,
}

/// Temporal context the `episodic` subsystem consults when deciding
/// whether two fragments were "co-encoded". Co-encoding edges in the
/// [`ConductivityGraph`] increment when two fragments are encoded
/// within a configurable window of each other (see
/// [`CascadeParams::co_encoding_window_secs`]).
pub trait TemporalContext: Send + Sync {
    /// Stamp the supplied fragment id with the current temporal
    /// coordinate. Called from the encoding hook.
    fn stamp(&mut self, fragment: FragmentId);

    /// Are these two fragments within `window_secs` of each other,
    /// per this context's temporal authority? Returns `false` for
    /// any pair whose stamps are not both recorded yet.
    fn within_window(&self, a: FragmentId, b: FragmentId, window_secs: u64) -> bool;
}

/// Wall-clock-based temporal context. `SystemTime::now()` at stamp
/// time, simple delta at query time. Suitable for tests and the
/// scaffold path.
#[derive(Debug, Default)]
pub struct WallClockTemporalContext {
    stamps: HashMap<FragmentId, SystemTime>,
}

impl WallClockTemporalContext {
    /// Construct an empty wall-clock context.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl TemporalContext for WallClockTemporalContext {
    fn stamp(&mut self, fragment: FragmentId) {
        self.stamps.insert(fragment, SystemTime::now());
    }

    fn within_window(&self, a: FragmentId, b: FragmentId, window_secs: u64) -> bool {
        let (Some(ta), Some(tb)) = (self.stamps.get(&a), self.stamps.get(&b)) else {
            return false;
        };
        let delta = if ta >= tb {
            ta.duration_since(*tb).ok()
        } else {
            tb.duration_since(*ta).ok()
        };
        delta.is_some_and(|d| d.as_secs() <= window_secs)
    }
}

/// Sparse, undirected conductivity graph between fragments. Storage
/// shape is an adjacency list
/// (`HashMap<FragmentId, HashMap<FragmentId, u32>>`) rather than a
/// flat pair-keyed map: cascade propagation iterates "all neighbours
/// of fragment X" per hop, making the adjacency list O(degree) per
/// access instead of O(|edges|) of a flat scan.
///
/// Edges are symmetric: the same `increment(a, b)` operation updates
/// both `a → b` and `b → a`. Asymmetric edges, if ever needed, would
/// live in a separate directed graph component alongside this
/// symmetric one — outside the v0.1 scope.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConductivityGraph {
    edges: HashMap<FragmentId, HashMap<FragmentId, u32>>,
}

impl ConductivityGraph {
    /// New empty graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Increment the weight of the undirected edge between `a` and
    /// `b` by 1. Both `a → b` and `b → a` entries are updated. A
    /// self-loop (`a == b`) is a no-op — the cascade doesn't
    /// re-propagate a node to itself.
    pub fn increment(&mut self, a: FragmentId, b: FragmentId) {
        if a == b {
            return;
        }
        *self.edges.entry(a).or_default().entry(b).or_insert(0) += 1;
        *self.edges.entry(b).or_default().entry(a).or_insert(0) += 1;
    }

    /// Weight of the edge between `a` and `b`. Returns `0` when no
    /// edge exists.
    #[must_use]
    pub fn edge_weight(&self, a: FragmentId, b: FragmentId) -> u32 {
        self.edges
            .get(&a)
            .and_then(|m| m.get(&b))
            .copied()
            .unwrap_or(0)
    }

    /// Iterate the neighbours of `node` with their raw edge weights.
    /// Returns an empty iterator when `node` has no recorded edges.
    pub fn neighbours_of(&self, node: FragmentId) -> impl Iterator<Item = (FragmentId, u32)> + '_ {
        self.edges
            .get(&node)
            .into_iter()
            .flat_map(|m| m.iter().map(|(id, w)| (*id, *w)))
    }

    /// Total raw weight outbound from `node`. Used as the
    /// `Σ_k w_ik` denominator when normalising at propagation time.
    #[must_use]
    pub fn total_weight_from(&self, node: FragmentId) -> u32 {
        self.edges.get(&node).map_or(0, |m| m.values().sum())
    }

    /// Number of nodes with at least one outbound edge.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.edges.len()
    }
}

/// A single coherence conflict report between two fragments in the
/// working set. The `composer`'s coherence evaluator returns a
/// `Vec<ConflictReport>`; the realizer consumes the list and applies
/// its filtering policy (de-weight, suppress, retain-with-warning).
///
/// `composer` is report-only — it does not mutate the working set.
/// Signals, not mutates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConflictReport {
    /// Indices into the working set. The first index is always
    /// strictly less than the second so consumers can deduplicate or
    /// canonicalise.
    pub pair: (usize, usize),
    /// Fragment ids for the conflict pair. Carries
    /// `(lower_index_id, higher_index_id)` so consumers can ask "is
    /// THIS exact pair of fragments in conflict?" without needing
    /// the working-set order plumbed through.
    #[serde(default)]
    pub fragment_pair: Option<(FragmentId, FragmentId)>,
    /// Confidence the conflict is real, in `[0.0, 1.0]`. For the
    /// lexical scaffold this is binary-ish (1.0 for an antonym hit,
    /// 0.0 otherwise); the NLI upgrade produces a graded score.
    pub confidence: f32,
    /// Why the conflict was flagged — preserves the audit trail.
    pub rationale: ConflictRationale,
}

/// Discriminated explanation for a [`ConflictReport`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictRationale {
    /// A lexical antonym match — fragments contain opposing terms
    /// (e.g. "large"/"small", "true"/"false"). Shallow but
    /// audit-grep friendly: the matched terms are preserved so a
    /// reviewer can verify the call.
    LexicalAntonymy {
        /// Term that appeared in the first fragment.
        term_a: String,
        /// Term that appeared in the second fragment.
        term_b: String,
    },
    /// NLI-derived entailment contradiction. Carries the model's
    /// contradiction probability and the model identifier for
    /// reproducibility.
    EntailmentContradiction {
        /// Model contradiction probability (0..1).
        probability: f32,
        /// Identifier of the NLI model that produced the verdict.
        model: String,
    },
}

/// Conflict-detection backend the `composer` uses when scanning a
/// working set. Defaults to [`Self::Lexical`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictDetectionMode {
    /// Hand-curated antonym table. Useful as interface scaffolding;
    /// brittle for real prose. Default.
    #[default]
    Lexical,
    /// NLI-classified entailment / neutral / contradiction. Not yet
    /// implemented in v0.1.
    Nli,
}

/// Combined output of a cascade-enabled recall. Two channels:
///   - `direct`: distance-sorted saliency-weighted matches the
///     `episodic` subsystem produced for the cue.
///   - `cascade`: associatively-activated fragments propagated via
///     the conductivity graph, keyed by `FragmentId` for
///     deduplication and carrying both the [`CascadeActivation`]
///     (level / hops / parent) and the resolved
///     [`CognitiveFragment`].
///
/// The two channels can overlap — a direct hit may also appear in
/// the cascade map via co-encoding edges from another hit. The
/// `composer` discriminates between channels because direct and
/// cascade-activated fragments are weighted differently in the
/// working-set selection.
#[derive(Debug, Clone)]
pub struct CascadeRetrieval {
    /// Direct, distance-sorted matches from `episodic` pattern
    /// completion. `(distance, fragment)`.
    pub direct: Vec<(f32, CognitiveFragment)>,
    /// Cascade-activated fragments keyed by fragment id, each paired
    /// with the resolved [`CognitiveFragment`]. The map may include
    /// initial sources (those direct hits whose ids the cascade pass
    /// propagated from); consumers can join against `direct` ids to
    /// detect overlap.
    pub cascade: HashMap<FragmentId, (CascadeActivation, CognitiveFragment)>,
}

impl CascadeRetrieval {
    /// Empty retrieval — no direct hits, no cascade activations.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            direct: Vec::new(),
            cascade: HashMap::new(),
        }
    }
}

impl CascadeActivation {
    /// Construct an initial activation source — a fragment matched
    /// directly to the query. `hops_from_source` is `0`; `parent_id`
    /// is `None`.
    #[must_use]
    pub fn initial(fragment_id: FragmentId, activation: ActivationLevel) -> Self {
        Self {
            fragment_id,
            activation,
            hops_from_source: 0,
            parent_id: None,
            propagated: false,
        }
    }

    /// Reconstruct the full activation path from this fragment back
    /// to the initial source. The chain is returned **last hop
    /// first** (this fragment's parent comes first, the initial
    /// source is last).
    ///
    /// Returns `Vec::new()` for initial sources (which have no
    /// parent). The caller supplies the cascade pass's full
    /// activation map so the walk can resolve parent ids.
    #[must_use]
    pub fn trace_path(
        &self,
        activation_map: &HashMap<FragmentId, CascadeActivation>,
    ) -> Vec<FragmentId> {
        let mut path = Vec::new();
        let mut cursor = self.parent_id;
        while let Some(parent_id) = cursor {
            path.push(parent_id);
            cursor = activation_map.get(&parent_id).and_then(|a| a.parent_id);
            // Defend against pathological cycles — though the
            // hop-bounded iteration should make these impossible at
            // the propagation level.
            if path.len() > usize::from(u8::MAX) {
                break;
            }
        }
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_level_clamps_to_unit_range() {
        assert!((ActivationLevel::new(1.5).0 - 1.0).abs() < f32::EPSILON);
        assert!((ActivationLevel::new(-0.3).0 - 0.0).abs() < f32::EPSILON);
        assert!((ActivationLevel::new(0.7).0 - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn activation_decay_preserves_unit_invariant() {
        let a = ActivationLevel::new(0.8);
        let b = a.decay(0.3);
        assert!(b.0 < a.0);
        assert!(b.0 >= 0.0);
        assert!(b.0 <= 1.0);
        let c = a.decay(0.0);
        assert!((c.0 - a.0).abs() < f32::EPSILON);
        let d = a.decay(1.0);
        assert!((d.0 - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn decay_unchecked_matches_safe_decay_for_in_range_rates() {
        let a = ActivationLevel::new(0.7);
        let safe = a.decay(0.25);
        let fast = a.decay_unchecked(0.25);
        assert!((safe.0 - fast.0).abs() < f32::EPSILON);
    }

    #[test]
    fn is_above_threshold_compares_correctly() {
        let a = ActivationLevel::new(0.1);
        assert!(a.is_above_threshold(ActivationLevel::new(0.05)));
        assert!(!a.is_above_threshold(ActivationLevel::new(0.2)));
        assert!(a.is_above_threshold(ActivationLevel::new(0.1)));
    }

    #[test]
    fn saturating_add_caps_at_one() {
        let a = ActivationLevel::new(0.7);
        let b = a.saturating_add(0.5);
        assert!((b.0 - 1.0).abs() < f32::EPSILON);
        let c = a.saturating_add(0.1);
        assert!((c.0 - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn conductivity_weight_clamps_negative_to_zero() {
        assert!((ConductivityWeight::new(-3.0).0 - 0.0).abs() < f32::EPSILON);
        assert!((ConductivityWeight::new(42.0).0 - 42.0).abs() < f32::EPSILON);
    }

    #[test]
    fn conductivity_normalised_handles_zero_total() {
        let w = ConductivityWeight::new(5.0);
        assert!((w.normalised(0.0) - 0.0).abs() < f32::EPSILON);
        assert!((w.normalised(20.0) - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn cascade_params_spreadr_defaults_match_literature() {
        let p = CascadeParams::SPREADR_DEFAULTS;
        assert!((p.retention - 0.5).abs() < f32::EPSILON);
        assert!((p.decay_rate - 0.3).abs() < f32::EPSILON);
        assert!((p.threshold.0 - 0.05).abs() < f32::EPSILON);
        assert_eq!(p.max_hops, 4);
    }

    #[test]
    fn cascade_activation_initial_has_no_parent() {
        let id = FragmentId::new();
        let act = CascadeActivation::initial(id, ActivationLevel::ONE);
        assert_eq!(act.hops_from_source, 0);
        assert!(act.parent_id.is_none());
    }

    #[test]
    fn trace_path_returns_empty_for_initial_source() {
        let id = FragmentId::new();
        let act = CascadeActivation::initial(id, ActivationLevel::ONE);
        let path = act.trace_path(&HashMap::new());
        assert!(path.is_empty());
    }

    #[test]
    fn trace_path_walks_back_to_source() {
        let source_id = FragmentId::new();
        let a_id = FragmentId::new();
        let b_id = FragmentId::new();
        let c_id = FragmentId::new();
        let mut map = HashMap::new();
        map.insert(
            source_id,
            CascadeActivation::initial(source_id, ActivationLevel::ONE),
        );
        map.insert(
            a_id,
            CascadeActivation {
                fragment_id: a_id,
                activation: ActivationLevel::new(0.5),
                hops_from_source: 1,
                parent_id: Some(source_id),
                propagated: true,
            },
        );
        map.insert(
            b_id,
            CascadeActivation {
                fragment_id: b_id,
                activation: ActivationLevel::new(0.3),
                hops_from_source: 2,
                parent_id: Some(a_id),
                propagated: true,
            },
        );
        let c = CascadeActivation {
            fragment_id: c_id,
            activation: ActivationLevel::new(0.15),
            hops_from_source: 3,
            parent_id: Some(b_id),
            propagated: false,
        };
        map.insert(c_id, c.clone());
        let path = c.trace_path(&map);
        assert_eq!(path, vec![b_id, a_id, source_id]);
    }
}
