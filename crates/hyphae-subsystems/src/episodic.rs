// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Episodic — store, binding, and cascade propagation.
//!
//! Collapses v1's `Hippocampus + EntorhinalCortex`. Three
//! responsibilities:
//!
//! - **Episodic store.** Fragments live in a `HashMap` keyed by
//!   `FragmentId`. Direct retrieval is brute-force cosine similarity
//!   in v0.1 (the `hnsw_rs` dependency is workspace-declared but
//!   not consumed yet; HNSW is a future optimisation that lands
//!   when the store crosses the brute-force threshold).
//! - **Conductivity graph.** Co-encoding edges accumulate per the
//!   [`hyphae_core::ConductivityGraph`] type. Two fragments
//!   encoded within `co_encoding_window_secs` of each other (per
//!   [`hyphae_core::CascadeParams`]) increment their edge weight.
//! - **Cascade propagation.** Spreading activation through the
//!   conductivity graph per Collins & Loftus 1975 / Siew 2019, with
//!   the parameters from [`hyphae_core::CascadeParams`]. The engine
//!   ships here because Hippocampus and Entorhinal are functionally
//!   inseparable per ADR-0001.

use hyphae_core::{
    ActivationLevel, CascadeActivation, CascadeParams, CascadeRetrieval, CognitiveFragment,
    ConductivityGraph, FragmentId, PayloadKind, Result, State, Subsystem, SubsystemId,
    TemporalContext, WallClockTemporalContext,
};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::{HashMap, HashSet};

/// Cosine similarity between two equal-length float vectors. Returns
/// `0.0` for any pair whose lengths differ or whose norms are zero.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot = a[i].mul_add(b[i], dot);
        na = a[i].mul_add(a[i], na);
        nb = b[i].mul_add(b[i], nb);
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Snapshot of [`Episodic`] state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct EpisodicSnapshot {
    fragments: HashMap<FragmentId, CognitiveFragment>,
    insertion_order: Vec<FragmentId>,
    graph: ConductivityGraph,
    params: CascadeParams,
    stored: u64,
    cascades: u64,
}

/// Episodic subsystem.
///
/// Holds the fragment store, the conductivity graph, and the
/// cascade-propagation engine. The temporal context is held
/// internally as [`WallClockTemporalContext`] — the v0.1 scaffold
/// path — and is **not** persisted across checkpoints. (v1's
/// Mammillary review made the same call.)
pub struct Episodic {
    fragments: HashMap<FragmentId, CognitiveFragment>,
    /// Insertion-ordered ids — used as the deterministic recency
    /// surface when no embedding is available.
    insertion_order: Vec<FragmentId>,
    graph: ConductivityGraph,
    temporal: WallClockTemporalContext,
    params: CascadeParams,
    stored: u64,
    cascades: u64,
}

impl std::fmt::Debug for Episodic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Episodic")
            .field("fragment_count", &self.fragments.len())
            .field("edge_count", &self.graph.node_count())
            .field("stored", &self.stored)
            .field("cascades", &self.cascades)
            .finish_non_exhaustive()
    }
}

impl Default for Episodic {
    fn default() -> Self {
        Self {
            fragments: HashMap::new(),
            insertion_order: Vec::new(),
            graph: ConductivityGraph::new(),
            temporal: WallClockTemporalContext::new(),
            params: CascadeParams::SPREADR_DEFAULTS,
            stored: 0,
            cascades: 0,
        }
    }
}

impl Episodic {
    /// Construct an episodic subsystem with default cascade
    /// parameters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct with explicit cascade parameters.
    #[must_use]
    pub fn with_params(params: CascadeParams) -> Self {
        Self {
            params,
            ..Self::default()
        }
    }

    /// Read-only access to the cascade parameters.
    #[must_use]
    pub fn params(&self) -> &CascadeParams {
        &self.params
    }

    /// Mutate the cascade parameters. The integrator threads
    /// learning-loop updates here AFTER the substrate's
    /// `propose_learning_update` audits them — direct mutation
    /// here bypasses the audit, only safe at startup or on a
    /// rollback path.
    pub fn params_mut(&mut self) -> &mut CascadeParams {
        &mut self.params
    }

    /// Gate (CLM-navegado): store a fragment WITHOUT the temporal
    /// co-encoding pass. Batch corpus ingestion must not create
    /// time-based edges (everything lands in one window → a near-
    /// complete graph); the dependency graph is built explicitly
    /// via [`Self::add_edge`]. O(1) insert.
    pub fn store_raw(&mut self, fragment: CognitiveFragment) {
        let id = fragment.id;
        if !self.fragments.contains_key(&id) {
            self.insertion_order.push(id);
        }
        self.fragments.insert(id, fragment);
        self.stored += 1;
    }

    /// Gate (CLM-navegado): add a dependency edge directly to the
    /// conductivity graph (an `import` / `call` relation) — the
    /// semantic substitute for temporal co-encoding on code corpora.
    pub fn add_edge(&mut self, a: FragmentId, b: FragmentId) {
        self.graph.increment(a, b);
    }

    /// Lifetime fragments stored.
    #[must_use]
    pub fn stored(&self) -> u64 {
        self.stored
    }

    /// Lifetime cascades run.
    #[must_use]
    pub fn cascades(&self) -> u64 {
        self.cascades
    }

    /// Read-only access to the conductivity graph.
    #[must_use]
    pub fn graph(&self) -> &ConductivityGraph {
        &self.graph
    }

    /// Look up a fragment by id.
    #[must_use]
    pub fn get(&self, id: FragmentId) -> Option<&CognitiveFragment> {
        self.fragments.get(&id)
    }

    /// Number of fragments in the store.
    #[must_use]
    pub fn len(&self) -> usize {
        self.fragments.len()
    }

    /// `true` when the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fragments.is_empty()
    }

    /// Store a fragment. Increments the conductivity graph against
    /// every fragment encoded within `co_encoding_window_secs`,
    /// then inserts the fragment into the store. Re-storing the
    /// same id replaces the fragment (the latest encoding wins).
    pub fn store(&mut self, fragment: CognitiveFragment) {
        let id = fragment.id;
        // Stamp FIRST so the `within_window` check below can find
        // this fragment's own timestamp. v1's M5 review documented
        // the same invariant: a stamp must exist for the new id at
        // co-encoding evaluation time.
        self.temporal.stamp(id);
        let window = self.params.co_encoding_window_secs;
        let prior_ids: Vec<FragmentId> = self
            .insertion_order
            .iter()
            .copied()
            .filter(|p| *p != id)
            .collect();
        for prior_id in prior_ids {
            if self.temporal.within_window(id, prior_id, window) {
                self.graph.increment(id, prior_id);
            }
        }
        if !self.fragments.contains_key(&id) {
            self.insertion_order.push(id);
        }
        self.fragments.insert(id, fragment);
        self.stored += 1;
    }

    /// Direct k-nearest-neighbour retrieval. Returns up to
    /// `top_k` `(distance, fragment)` pairs sorted ascending by
    /// distance. Distance is `1 - cosine_similarity` when both the
    /// query and the candidate have embeddings of the same width;
    /// candidates without embeddings (or with a different width)
    /// score `distance = f32::INFINITY` and are returned last.
    ///
    /// When the query has no embedding, falls back to **insertion-
    /// order recency** — the latest `top_k` fragments, with
    /// distance `0.0` so they sort before any embedded candidate.
    #[must_use]
    pub fn pattern_complete(
        &self,
        query_embedding: Option<&[f32]>,
        top_k: usize,
    ) -> Vec<(f32, CognitiveFragment)> {
        if top_k == 0 || self.fragments.is_empty() {
            return Vec::new();
        }
        let Some(query) = query_embedding else {
            // Recency fallback.
            return self
                .insertion_order
                .iter()
                .rev()
                .take(top_k)
                .filter_map(|id| self.fragments.get(id).map(|f| (0.0, f.clone())))
                .collect();
        };
        let mut scored: Vec<(f32, CognitiveFragment)> = self
            .fragments
            .values()
            .map(|f| {
                let dist = f
                    .embedding
                    .as_deref()
                    .map_or(f32::INFINITY, |e| 1.0 - cosine_similarity(query, e));
                (dist, f.clone())
            })
            .collect();
        scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);
        scored
    }

    /// Cascade propagation from seed ids through the conductivity
    /// graph. Implements the Siew 2019 `spreadr` formalism:
    ///
    /// ```text
    /// A_j(t+1) += [(1 - retention) * A_i(t)] * [w_ij / Σ_k w_ik]
    /// A_i(t+1) = A_i(t+1) * (1 - decay_rate)
    /// stop propagating from i when A_i < threshold or hops ≥ max_hops
    /// ```
    ///
    /// Each seed enters as an initial activation with
    /// `ActivationLevel::ONE` and `hops_from_source = 0`. The
    /// propagation runs at most [`CascadeParams::max_hops`] hops;
    /// after each hop the activation map is decayed once.
    pub fn cascade(&mut self, seeds: &[FragmentId]) -> CascadeRetrieval {
        self.cascades += 1;
        if seeds.is_empty() {
            return CascadeRetrieval::empty();
        }
        let mut map: HashMap<FragmentId, CascadeActivation> = HashMap::new();
        for seed in seeds {
            if !self.fragments.contains_key(seed) {
                continue;
            }
            map.insert(
                *seed,
                CascadeActivation::initial(*seed, ActivationLevel::ONE),
            );
        }

        for _hop in 0..self.params.max_hops {
            // Snapshot the propagators: nodes that have not yet
            // propagated and whose activation is above threshold.
            let propagators: Vec<(FragmentId, ActivationLevel, u8)> = map
                .values()
                .filter(|act| {
                    !act.propagated && act.activation.is_above_threshold(self.params.threshold)
                })
                .map(|act| (act.fragment_id, act.activation, act.hops_from_source))
                .collect();

            if propagators.is_empty() {
                break;
            }

            for (source_id, source_activation, source_hops) in propagators {
                // Mark the source propagated for this iteration.
                if let Some(act) = map.get_mut(&source_id) {
                    act.propagated = true;
                }
                let total = self.graph.total_weight_from(source_id);
                if total == 0 {
                    continue;
                }
                #[allow(clippy::cast_precision_loss)]
                let total_f = total as f32;
                let outflow = (1.0 - self.params.retention) * source_activation.0;
                let next_hops = source_hops.saturating_add(1);
                for (neighbour, weight) in self.graph.neighbours_of(source_id) {
                    #[allow(clippy::cast_precision_loss)]
                    let weight_f = weight as f32;
                    let share = outflow * (weight_f / total_f);
                    let entry = map.entry(neighbour).or_insert(CascadeActivation {
                        fragment_id: neighbour,
                        activation: ActivationLevel::ZERO,
                        hops_from_source: next_hops,
                        parent_id: Some(source_id),
                        propagated: false,
                    });
                    entry.activation = entry.activation.saturating_add(share);
                    // Track shortest-hop parentage for path tracing.
                    if next_hops < entry.hops_from_source {
                        entry.hops_from_source = next_hops;
                        entry.parent_id = Some(source_id);
                    }
                }
            }

            // Apply global decay at the end of each hop.
            for act in map.values_mut() {
                act.activation = act.activation.decay_unchecked(self.params.decay_rate);
            }
        }

        // Build the retrieval. `direct` is the seeds themselves;
        // `cascade` is the full activation map.
        let direct: Vec<(f32, CognitiveFragment)> = seeds
            .iter()
            .filter_map(|seed| self.fragments.get(seed).map(|f| (0.0, f.clone())))
            .collect();
        let mut cascade_map = HashMap::new();
        for (id, activation) in map {
            if let Some(fragment) = self.fragments.get(&id) {
                cascade_map.insert(id, (activation, fragment.clone()));
            }
        }
        CascadeRetrieval {
            direct,
            cascade: cascade_map,
        }
    }
}

impl Subsystem for Episodic {
    fn id(&self) -> SubsystemId {
        SubsystemId::Episodic
    }

    fn process(
        &mut self,
        mut fragment: CognitiveFragment,
        incoming: PayloadKind,
        state: State,
    ) -> Result<Vec<CognitiveFragment>> {
        fragment.provenance.source_subsystem = "episodic".to_string();
        match (incoming, state) {
            // Encoding flows: store the fragment.
            (PayloadKind::Encoding, _) => {
                self.store(fragment.clone());
                Ok(vec![fragment])
            }
            // Recall flows: pattern-complete against the input,
            // then seed `Self::cascade` with the direct-hit ids
            // to activate the conductivity graph (ADR-0011).
            // Direct hits emit unchanged; cascade-derived
            // fragments emit with `provenance.parent_ids` set to
            // the immediate predecessor in the propagation chain
            // so the realizer's `ShallowCascade` check
            // discriminates correctly.
            (PayloadKind::BottomUpPredictionError, State::Recall) => {
                let query = fragment.embedding.as_deref();
                let direct = self.pattern_complete(query, self.params.working_set_size as usize);

                let direct_ids: Vec<FragmentId> = direct.iter().map(|(_, f)| f.id).collect();
                let direct_set: HashSet<FragmentId> = direct_ids.iter().copied().collect();
                let retrieval = self.cascade(&direct_ids);

                let mut emissions: Vec<CognitiveFragment> =
                    direct.into_iter().map(|(_, f)| f).collect();

                for (id, (activation, frag)) in &retrieval.cascade {
                    if direct_set.contains(id) {
                        continue;
                    }
                    let mut tagged = frag.clone();
                    if let Some(parent) = activation.parent_id {
                        tagged.provenance.parent_ids = vec![parent];
                    }
                    emissions.push(tagged);
                }

                emissions.push(fragment);
                Ok(emissions)
            }
            // Other flows: passthrough.
            _ => Ok(vec![fragment]),
        }
    }

    fn checkpoint(&self) -> Result<Vec<u8>> {
        let snap = EpisodicSnapshot {
            fragments: self.fragments.clone(),
            insertion_order: self.insertion_order.clone(),
            graph: self.graph.clone(),
            params: self.params,
            stored: self.stored,
            cascades: self.cascades,
        };
        bincode::serialize(&snap).map_err(|e| {
            hyphae_core::HyphaeError::Other(format!("episodic checkpoint serialise: {e}"))
        })
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<()> {
        let snap: EpisodicSnapshot = bincode::deserialize(bytes).map_err(|e| {
            hyphae_core::HyphaeError::Other(format!("episodic restore deserialise: {e}"))
        })?;
        self.fragments = snap.fragments;
        self.insertion_order = snap.insertion_order;
        self.graph = snap.graph;
        self.params = snap.params;
        self.stored = snap.stored;
        self.cascades = snap.cascades;
        // Temporal context is wall-clock and is NOT restored —
        // post-restart the timestamps are no longer authoritative
        // (the v1 Mammillary M5 invariant).
        self.temporal = WallClockTemporalContext::new();
        Ok(())
    }

    fn on_state_change(&mut self, _old: State, new: State) -> Result<()> {
        if new == State::Recovery {
            // Recovery wipes the in-process temporal context so a
            // fresh wall-clock stamp lineage starts on the next
            // store. The graph and the fragments are preserved.
            self.temporal = WallClockTemporalContext::new();
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

#[cfg(test)]
mod tests {
    use super::*;
    use hyphae_core::FragmentContent;

    fn obs_with_embedding(body: &str, emb: Vec<f32>) -> CognitiveFragment {
        let mut f = CognitiveFragment::new(
            FragmentContent::Observation {
                body: body.to_string(),
            },
            "test",
        );
        f.embedding = Some(emb);
        f
    }

    fn obs(body: &str) -> CognitiveFragment {
        CognitiveFragment::new(
            FragmentContent::Observation {
                body: body.to_string(),
            },
            "test",
        )
    }

    #[test]
    fn cosine_similarity_self_is_one_for_unit_vector() {
        let v = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_orthogonal_is_zero() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn store_inserts_and_tracks_insertion_order() {
        let mut e = Episodic::new();
        let a = obs("first");
        let b = obs("second");
        let a_id = a.id;
        let b_id = b.id;
        e.store(a);
        e.store(b);
        assert_eq!(e.len(), 2);
        assert!(e.get(a_id).is_some());
        assert!(e.get(b_id).is_some());
        assert_eq!(e.insertion_order, vec![a_id, b_id]);
    }

    #[test]
    fn co_encoding_increments_graph_edge() {
        let mut e = Episodic::new();
        let a = obs("first");
        let b = obs("second");
        let a_id = a.id;
        let b_id = b.id;
        e.store(a);
        e.store(b);
        // Both stored within the default 60s window → an edge
        // exists between them.
        assert!(e.graph.edge_weight(a_id, b_id) > 0);
    }

    #[test]
    fn pattern_complete_falls_back_to_recency_without_query_embedding() {
        let mut e = Episodic::new();
        let f1 = obs("first");
        let f2 = obs("second");
        let f3 = obs("third");
        let f3_id = f3.id;
        e.store(f1);
        e.store(f2);
        e.store(f3);
        let hits = e.pattern_complete(None, 2);
        assert_eq!(hits.len(), 2);
        // Most recent (f3) comes first.
        assert_eq!(hits[0].1.id, f3_id);
    }

    #[test]
    fn pattern_complete_ranks_by_cosine_similarity() {
        let mut e = Episodic::new();
        e.store(obs_with_embedding("close", vec![1.0, 0.0, 0.0]));
        e.store(obs_with_embedding("orthogonal", vec![0.0, 1.0, 0.0]));
        e.store(obs_with_embedding("near", vec![0.9, 0.1, 0.0]));
        let query = vec![1.0, 0.0, 0.0];
        let hits = e.pattern_complete(Some(&query), 3);
        // First hit is the perfect match (distance 0).
        assert!(hits[0].0 < 1e-5);
        // Last hit is the orthogonal candidate (distance ~1).
        assert!(hits[2].0 > 0.9);
    }

    #[test]
    fn cascade_with_no_seeds_returns_empty_retrieval() {
        let mut e = Episodic::new();
        let result = e.cascade(&[]);
        assert!(result.direct.is_empty());
        assert!(result.cascade.is_empty());
    }

    #[test]
    fn cascade_propagates_through_co_encoded_neighbours() {
        let mut e = Episodic::new();
        // Three fragments stored in quick succession all become
        // co-encoded.
        let a = obs("anchor");
        let b = obs("neighbour one");
        let c = obs("neighbour two");
        let a_id = a.id;
        let b_id = b.id;
        let c_id = c.id;
        e.store(a);
        e.store(b);
        e.store(c);
        let result = e.cascade(&[a_id]);
        // `direct` has the seed.
        assert_eq!(result.direct.len(), 1);
        // `cascade` has the seed AND its co-encoded neighbours
        // (b and c).
        assert!(result.cascade.contains_key(&a_id));
        assert!(result.cascade.contains_key(&b_id));
        assert!(result.cascade.contains_key(&c_id));
        // Each non-seed activation has hops_from_source >= 1.
        for (id, (act, _)) in &result.cascade {
            if *id != a_id {
                assert!(act.hops_from_source >= 1);
                assert_eq!(act.parent_id, Some(a_id));
            }
        }
    }

    #[test]
    fn cascade_respects_max_hops_bound() {
        let mut episodic = Episodic::with_params(CascadeParams {
            max_hops: 1,
            ..CascadeParams::SPREADR_DEFAULTS
        });
        // Build a chain alpha → bravo → charlie → delta via
        // co-encoding (all stored in sequence within the default
        // window).
        let alpha = obs("a");
        let bravo = obs("b");
        let charlie = obs("c");
        let delta = obs("d");
        let alpha_id = alpha.id;
        let bravo_id = bravo.id;
        let charlie_id = charlie.id;
        let delta_id = delta.id;
        episodic.store(alpha);
        episodic.store(bravo);
        episodic.store(charlie);
        episodic.store(delta);
        let result = episodic.cascade(&[alpha_id]);
        // With max_hops=1, only direct neighbours of `alpha_id`
        // should appear. Within the same co-encoding window all
        // four are neighbours of each other — so we expect every
        // fragment to be in the result, but each at
        // `hops_from_source ≤ 1`.
        for (id, (act, _)) in &result.cascade {
            assert!(
                act.hops_from_source <= 1,
                "fragment {id:?} reached at hops {} > 1",
                act.hops_from_source,
            );
        }
        assert!(result.cascade.contains_key(&alpha_id));
        assert!(result.cascade.contains_key(&bravo_id));
        assert!(result.cascade.contains_key(&charlie_id));
        assert!(result.cascade.contains_key(&delta_id));
    }

    #[test]
    fn checkpoint_restore_round_trips_store_and_graph() {
        let mut e = Episodic::new();
        let a = obs("a");
        let b = obs("b");
        let a_id = a.id;
        let b_id = b.id;
        e.store(a);
        e.store(b);
        let bytes = e.checkpoint().unwrap();
        let mut restored = Episodic::new();
        restored.restore(&bytes).unwrap();
        assert_eq!(restored.len(), 2);
        assert!(restored.get(a_id).is_some());
        assert!(restored.get(b_id).is_some());
        assert!(restored.graph.edge_weight(a_id, b_id) > 0);
    }

    #[test]
    fn process_with_encoding_kind_stores_fragment() {
        let mut e = Episodic::new();
        let f = obs("hello");
        let id = f.id;
        let _ = e
            .process(f, PayloadKind::Encoding, State::Encoding)
            .unwrap();
        assert_eq!(e.len(), 1);
        assert!(e.get(id).is_some());
    }

    /// ADR-0011 — the recall branch of `process` invokes
    /// `cascade()` on the direct hits and emits propagation-
    /// derived fragments alongside the direct ones. The
    /// cascade-derived emissions carry `parent_ids` set to the
    /// immediate predecessor in the propagation chain.
    #[test]
    fn process_recall_invokes_cascade_and_tags_propagation() {
        // working_set_size = 1 forces pattern_complete to return
        // just the anchor; the co-encoded neighbours must arrive
        // via cascade propagation or not at all.
        let mut e = Episodic::with_params(CascadeParams {
            working_set_size: 1,
            ..CascadeParams::SPREADR_DEFAULTS
        });
        let anchor = obs_with_embedding("anchor body", vec![1.0, 0.0, 0.0]);
        let near = obs_with_embedding("neighbour one", vec![0.0, 1.0, 0.0]);
        let far = obs_with_embedding("neighbour two", vec![0.0, 0.0, 1.0]);
        let anchor_id = anchor.id;
        let near_id = near.id;
        let far_id = far.id;
        e.store(anchor);
        e.store(near);
        e.store(far);

        // Query aligns with the anchor's embedding so
        // `pattern_complete` returns only anchor as direct hit.
        let mut cue = obs("query");
        cue.embedding = Some(vec![1.0, 0.0, 0.0]);

        let emissions = e
            .process(cue, PayloadKind::BottomUpPredictionError, State::Recall)
            .unwrap();

        let emitted_ids: HashSet<FragmentId> = emissions.iter().map(|f| f.id).collect();
        assert!(
            emitted_ids.contains(&anchor_id),
            "anchor must appear as a direct hit",
        );
        let propagation_visible = emitted_ids.contains(&near_id) || emitted_ids.contains(&far_id);
        assert!(
            propagation_visible,
            "at least one co-encoded neighbour must reach the working set via cascade",
        );

        // Cascade-derived emissions carry the parent tag.
        for em in &emissions {
            if em.id == near_id || em.id == far_id {
                assert_eq!(
                    em.provenance.parent_ids,
                    vec![anchor_id],
                    "cascade-derived fragment must carry parent_ids = [seed]",
                );
            }
        }
    }
}
