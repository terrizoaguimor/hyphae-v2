// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! Cascade-shape-driven composition — per ADR-0006.
//!
//! Projects a [`CascadeRetrieval`] (the cascade engine's output) to
//! a [`CompositionShape`] — an ordered sequence of
//! [`CompositionStep`]s, each pairing a fragment with the connective
//! role that should precede it. The realizer walks the shape
//! instead of the raw working set so all ten lexicon roles
//! (`Causation`, `Elaboration`, `Sequence`, `Concession`, etc.) are
//! reachable from real cascade input — not just the linear
//! `Continuation` / `Contrast` binary the v0.1.1 realizer emitted.
//!
//! The projection is **deterministic** and structurally simple
//! (per ADR-0006 §"Why the algorithm stops here"). It uses no LLM,
//! no learned model, no statistical inference — it reads cascade
//! topology and assigns roles by pattern match.

use crate::connective::ConnectiveRole;
use hyphae_core::{CascadeRetrieval, CognitiveFragment, FragmentId};
use std::collections::HashSet;

/// One step in a composition — a fragment with the role that
/// precedes its quote. The first step's role is ignored by the
/// realizer (which emits an `Opening` connective at position 0
/// regardless).
#[derive(Debug, Clone)]
pub struct CompositionStep {
    /// Connective role to emit BEFORE this step's fragment quote.
    pub role: ConnectiveRole,
    /// The fragment to quote.
    pub fragment: CognitiveFragment,
    /// Distance from the cascade source. 0 = direct seed; 1+ =
    /// cascade-derived activation.
    pub depth: u8,
}

/// An ordered shape the realizer walks to produce a composition.
#[derive(Debug, Clone, Default)]
pub struct CompositionShape {
    /// Steps in emission order.
    pub steps: Vec<CompositionStep>,
}

impl CompositionShape {
    /// `true` when the shape has no steps. The realizer then emits
    /// the `EmptyWorkingSet` acknowledgment.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Number of steps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }
}

/// Project a `CascadeRetrieval` to a composition shape per the
/// algorithm in ADR-0006 §"Projection algorithm (v0.1)".
///
/// Returns an empty shape when the retrieval has no direct seeds
/// (the cascade started from nothing). The realizer surfaces this
/// as `EmptyWorkingSet`.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn shape_from_cascade(retrieval: &CascadeRetrieval) -> CompositionShape {
    let mut steps: Vec<CompositionStep> = Vec::new();

    // ── Step 1 — Anchor ──────────────────────────────────────
    //
    // The lowest-distance direct hit is the anchor. Its preceding
    // role does not matter (the realizer emits the Opening
    // connective at position 0). We mark it Continuation here so
    // a downstream consumer that does NOT use the realizer's
    // position-zero special-casing still produces meaningful prose.

    let mut direct_sorted: Vec<&(f32, CognitiveFragment)> = retrieval.direct.iter().collect();
    direct_sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    if direct_sorted.is_empty() {
        return CompositionShape::default();
    }

    let anchor = direct_sorted[0].1.clone();
    let anchor_id = anchor.id;
    steps.push(CompositionStep {
        role: ConnectiveRole::Continuation,
        fragment: anchor,
        depth: 0,
    });

    let mut emitted: HashSet<FragmentId> = HashSet::new();
    emitted.insert(anchor_id);

    // ── Step 2 — First-hop supports of the anchor ────────────
    //
    // Fragments whose parent_id is the anchor AND whose
    // hops_from_source == 1. Sort by descending activation level.
    let mut first_hop_supports: Vec<(f32, CognitiveFragment)> = retrieval
        .cascade
        .iter()
        .filter_map(|(_, (act, frag))| {
            if act.hops_from_source == 1 && act.parent_id == Some(anchor_id) {
                Some((act.activation.0, frag.clone()))
            } else {
                None
            }
        })
        .collect();
    first_hop_supports.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let support_count = first_hop_supports.len();
    let support_role = if support_count >= 2 {
        ConnectiveRole::Causation
    } else {
        ConnectiveRole::Continuation
    };

    for (_, frag) in first_hop_supports {
        if emitted.insert(frag.id) {
            steps.push(CompositionStep {
                role: support_role,
                fragment: frag,
                depth: 1,
            });
        }
    }

    // ── Step 3 — Deeper activations (Elaboration) ────────────
    //
    // hops_from_source >= 2. Sort by ascending hops (general →
    // specific).
    let mut deeper: Vec<(u8, f32, CognitiveFragment)> = retrieval
        .cascade
        .iter()
        .filter_map(|(_, (act, frag))| {
            if act.hops_from_source >= 2 {
                Some((act.hops_from_source, act.activation.0, frag.clone()))
            } else {
                None
            }
        })
        .collect();
    deeper.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
    });
    for (hops, _, frag) in deeper {
        if emitted.insert(frag.id) {
            steps.push(CompositionStep {
                role: ConnectiveRole::Elaboration,
                fragment: frag,
                depth: hops,
            });
        }
    }

    // ── Step 4 — Other direct seeds (Sequence) ───────────────
    //
    // Any direct hit beyond the anchor that was not already
    // emitted. Sequence reads as enumeration.
    for (_, frag) in direct_sorted.iter().skip(1) {
        if emitted.insert(frag.id) {
            steps.push(CompositionStep {
                role: ConnectiveRole::Sequence,
                fragment: frag.clone(),
                depth: 0,
            });
        }
    }

    // ── Step 5 — Contrast injection ──────────────────────────
    //
    // Walk adjacent pairs; if their valence delta crosses
    // thresholds with opposing sign, override the role.
    inject_contrasts(&mut steps);

    CompositionShape { steps }
}

/// Linear-walk fallback: project a raw working set to a shape per
/// the v0.1.1 semantics (Continuation by default, Contrast on
/// strongly-opposing valence). Used when the caller has a working
/// set but no `CascadeRetrieval` — the smoke runner, the eval
/// harness's direct-construction path, and any integration that
/// has yet to wire through the cascade engine.
#[must_use]
pub fn shape_from_working_set(working_set: &[CognitiveFragment]) -> CompositionShape {
    let mut steps: Vec<CompositionStep> = working_set
        .iter()
        .map(|f| CompositionStep {
            role: ConnectiveRole::Continuation,
            fragment: f.clone(),
            depth: 0,
        })
        .collect();
    inject_contrasts(&mut steps);
    CompositionShape { steps }
}

/// Project a `CascadeRetrieval` to a shape, falling back to the
/// retrieval's `direct` channel when `cascade` is empty (no
/// propagation occurred). Useful for callers that build a
/// `CascadeRetrieval` from a working set without running a real
/// cascade pass — the smoke runner does this.
#[must_use]
pub fn shape_from_retrieval(retrieval: &CascadeRetrieval) -> CompositionShape {
    if retrieval.cascade.is_empty() {
        let working_set: Vec<CognitiveFragment> =
            retrieval.direct.iter().map(|(_, f)| f.clone()).collect();
        shape_from_working_set(&working_set)
    } else {
        shape_from_cascade(retrieval)
    }
}

/// Adjust the role of any step whose valence delta against the
/// previous step crosses the contrast thresholds. The thresholds
/// match `realizer::pick_inter_fragment_role_and_polarity`:
///
/// - |Δvalence| > 0.6 with opposing sign → `Contrast`
/// - |Δvalence| > 0.3 with opposing sign → `Concession`
fn inject_contrasts(steps: &mut [CompositionStep]) {
    for i in 1..steps.len() {
        let prev_v = steps[i - 1].fragment.valence;
        let cur_v = steps[i].fragment.valence;
        let delta = cur_v - prev_v;
        let abs = delta.abs();
        let opposing =
            prev_v.signum() != cur_v.signum() && (prev_v.abs() > 0.0 || cur_v.abs() > 0.0);

        if abs > 0.6 && opposing {
            steps[i].role = ConnectiveRole::Contrast;
        } else if abs > 0.3 && opposing {
            steps[i].role = ConnectiveRole::Concession;
        }
    }
}

/// Convenience: extract the working set in emission order from a
/// shape. The realizer uses this to feed downstream consumers that
/// only want the fragments (the limitation evaluator, the audit
/// trail).
#[must_use]
pub fn working_set_of(shape: &CompositionShape) -> Vec<CognitiveFragment> {
    shape.steps.iter().map(|s| s.fragment.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyphae_core::{ActivationLevel, CascadeActivation, FragmentContent};
    use std::collections::HashMap;

    fn frag(body: &str, valence: f32) -> CognitiveFragment {
        let mut f = CognitiveFragment::new(
            FragmentContent::Observation {
                body: body.to_string(),
            },
            "test",
        );
        f.valence = valence;
        f
    }

    fn cascade_entry(
        f: CognitiveFragment,
        hops: u8,
        parent: Option<FragmentId>,
        activation: f32,
    ) -> (CascadeActivation, CognitiveFragment) {
        (
            CascadeActivation {
                fragment_id: f.id,
                activation: ActivationLevel::new(activation),
                hops_from_source: hops,
                parent_id: parent,
                propagated: false,
            },
            f,
        )
    }

    #[test]
    fn empty_retrieval_yields_empty_shape() {
        let retrieval = CascadeRetrieval::empty();
        let shape = shape_from_cascade(&retrieval);
        assert!(shape.is_empty());
    }

    #[test]
    fn single_direct_hit_yields_one_step() {
        let anchor = frag("the only fragment", 0.0);
        let retrieval = CascadeRetrieval {
            direct: vec![(0.0, anchor.clone())],
            cascade: HashMap::new(),
        };
        let shape = shape_from_cascade(&retrieval);
        assert_eq!(shape.len(), 1);
        assert_eq!(shape.steps[0].fragment.id, anchor.id);
        assert_eq!(shape.steps[0].depth, 0);
    }

    #[test]
    fn anchor_plus_two_supports_uses_causation_role() {
        let anchor = frag("central claim", 0.3);
        let support_a = frag("support a", 0.3);
        let support_b = frag("support b", 0.3);
        let mut cascade = HashMap::new();
        cascade.insert(
            support_a.id,
            cascade_entry(support_a.clone(), 1, Some(anchor.id), 0.6),
        );
        cascade.insert(
            support_b.id,
            cascade_entry(support_b.clone(), 1, Some(anchor.id), 0.5),
        );
        let retrieval = CascadeRetrieval {
            direct: vec![(0.0, anchor.clone())],
            cascade,
        };
        let shape = shape_from_cascade(&retrieval);
        assert_eq!(shape.len(), 3);
        // Anchor first.
        assert_eq!(shape.steps[0].fragment.id, anchor.id);
        // Two supports both with Causation role.
        assert_eq!(shape.steps[1].role, ConnectiveRole::Causation);
        assert_eq!(shape.steps[2].role, ConnectiveRole::Causation);
        // Sorted by descending activation — support_a (0.6) first.
        assert_eq!(shape.steps[1].fragment.id, support_a.id);
        assert_eq!(shape.steps[2].fragment.id, support_b.id);
    }

    #[test]
    fn single_support_uses_continuation_role() {
        let anchor = frag("anchor", 0.0);
        let support = frag("lone support", 0.0);
        let mut cascade = HashMap::new();
        cascade.insert(
            support.id,
            cascade_entry(support.clone(), 1, Some(anchor.id), 0.5),
        );
        let retrieval = CascadeRetrieval {
            direct: vec![(0.0, anchor)],
            cascade,
        };
        let shape = shape_from_cascade(&retrieval);
        assert_eq!(shape.steps[1].role, ConnectiveRole::Continuation);
    }

    #[test]
    fn deeper_activations_use_elaboration_role() {
        let anchor = frag("anchor", 0.0);
        let mid = frag("mid-hop", 0.0);
        let deep = frag("deeper", 0.0);
        let mut cascade = HashMap::new();
        cascade.insert(mid.id, cascade_entry(mid.clone(), 1, Some(anchor.id), 0.5));
        cascade.insert(deep.id, cascade_entry(deep.clone(), 2, Some(mid.id), 0.3));
        let retrieval = CascadeRetrieval {
            direct: vec![(0.0, anchor.clone())],
            cascade,
        };
        let shape = shape_from_cascade(&retrieval);
        assert_eq!(shape.len(), 3);
        // Anchor, then mid (Continuation), then deeper (Elaboration).
        assert_eq!(shape.steps[0].fragment.id, anchor.id);
        assert_eq!(shape.steps[1].fragment.id, mid.id);
        assert_eq!(shape.steps[1].role, ConnectiveRole::Continuation);
        assert_eq!(shape.steps[2].fragment.id, deep.id);
        assert_eq!(shape.steps[2].role, ConnectiveRole::Elaboration);
        assert_eq!(shape.steps[2].depth, 2);
    }

    #[test]
    fn additional_direct_seeds_use_sequence_role() {
        let anchor_a = frag("first anchor", 0.0);
        let anchor_b = frag("second anchor", 0.0);
        let anchor_c = frag("third anchor", 0.0);
        let retrieval = CascadeRetrieval {
            direct: vec![
                (0.0, anchor_a.clone()),
                (0.1, anchor_b.clone()),
                (0.2, anchor_c.clone()),
            ],
            cascade: HashMap::new(),
        };
        let shape = shape_from_cascade(&retrieval);
        assert_eq!(shape.len(), 3);
        assert_eq!(shape.steps[0].fragment.id, anchor_a.id);
        assert_eq!(shape.steps[1].fragment.id, anchor_b.id);
        assert_eq!(shape.steps[1].role, ConnectiveRole::Sequence);
        assert_eq!(shape.steps[2].fragment.id, anchor_c.id);
        assert_eq!(shape.steps[2].role, ConnectiveRole::Sequence);
    }

    #[test]
    fn strong_opposing_valence_injects_contrast() {
        let positive = frag("the deploy succeeded", 0.7);
        let negative = frag("the rollback was painful", -0.7);
        let retrieval = CascadeRetrieval {
            direct: vec![(0.0, positive), (0.1, negative)],
            cascade: HashMap::new(),
        };
        let shape = shape_from_cascade(&retrieval);
        assert_eq!(shape.steps[1].role, ConnectiveRole::Contrast);
    }

    #[test]
    fn mild_opposing_valence_injects_concession() {
        let positive = frag("positive thing", 0.4);
        let negative = frag("mildly negative", -0.2);
        let retrieval = CascadeRetrieval {
            direct: vec![(0.0, positive), (0.1, negative)],
            cascade: HashMap::new(),
        };
        let shape = shape_from_cascade(&retrieval);
        assert_eq!(shape.steps[1].role, ConnectiveRole::Concession);
    }

    #[test]
    fn same_sign_valence_does_not_inject_contrast() {
        let a = frag("a", 0.5);
        let b = frag("b", 0.6);
        let retrieval = CascadeRetrieval {
            direct: vec![(0.0, a), (0.1, b)],
            cascade: HashMap::new(),
        };
        let shape = shape_from_cascade(&retrieval);
        // Sequence role (additional direct seed) stays intact.
        assert_eq!(shape.steps[1].role, ConnectiveRole::Sequence);
    }

    #[test]
    fn shape_from_working_set_is_linear_continuation() {
        let a = frag("a", 0.0);
        let b = frag("b", 0.0);
        let c = frag("c", 0.0);
        let shape = shape_from_working_set(&[a.clone(), b.clone(), c.clone()]);
        assert_eq!(shape.len(), 3);
        assert_eq!(shape.steps[0].role, ConnectiveRole::Continuation);
        assert_eq!(shape.steps[1].role, ConnectiveRole::Continuation);
        assert_eq!(shape.steps[2].role, ConnectiveRole::Continuation);
    }

    #[test]
    fn shape_from_retrieval_falls_back_when_cascade_empty() {
        let a = frag("a", 0.0);
        let b = frag("b", 0.0);
        let retrieval = CascadeRetrieval {
            direct: vec![(0.0, a.clone()), (0.1, b.clone())],
            cascade: HashMap::new(),
        };
        let shape = shape_from_retrieval(&retrieval);
        // No cascade → linear walk → both Continuation.
        assert_eq!(shape.steps[1].role, ConnectiveRole::Continuation);
    }

    #[test]
    fn working_set_of_extracts_fragments_in_order() {
        let a = frag("a", 0.0);
        let b = frag("b", 0.0);
        let a_id = a.id;
        let b_id = b.id;
        let shape = shape_from_working_set(&[a, b]);
        let ws = working_set_of(&shape);
        assert_eq!(ws.len(), 2);
        assert_eq!(ws[0].id, a_id);
        assert_eq!(ws[1].id, b_id);
    }
}
