// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! The surface realizer — the only place that produces text.
//!
//! The realizer:
//!
//! 1. **Selects a schema** from the caller's [`Intent`].
//! 2. **Evaluates honest limitation triggers** against the working
//!    set + the ethics report (per RFC §5.3 and ADR-0003 §3, the
//!    composer's ethics signal flows in here).
//! 3. **Composes the prose** by emitting the chosen schema's slots:
//!    opening connective, fragment quotes interleaved with
//!    continuation / contrast connectives, closing connective,
//!    and limitation acknowledgments when any trigger fired.
//!
//! Per RFC §5.2: the **fragment body is verbatim**. The realizer
//! never rewrites a quoted fragment — it only generates the prose
//! that surrounds the quotes. This is the architectural property
//! that distinguishes Hyphae from a system that synthesises
//! language; the realizer is the boundary the
//! no-LLM-in-cognition-path commitment depends on.

use crate::boundary::BoundarySignal;
use crate::composition_shape::{CompositionShape, shape_from_working_set};
use crate::connective::{ConnectiveRole, Formality, Lexicon, PickContext, Polarity, Register};
use crate::limitation::{LimitationContext, LimitationTrigger, evaluate as evaluate_limitations};
use crate::schema::{Intent, SchemaId};
use hyphae_core::{CognitiveFragment, FragmentContent, FragmentId};
use hyphae_ethics::EthicsReport;
use serde::{Deserialize, Serialize};

/// Errors raised by realization.
#[derive(Debug, thiserror::Error)]
pub enum RealizationError {
    /// The caller's intent has no schema mapping. Reserved for
    /// future intent variants that the realizer cannot yet
    /// produce a composition for; v0.1 maps every variant.
    #[error("no schema available for intent {0:?}")]
    NoSchemaForIntent(Intent),
}

/// Per-call request to the realizer.
#[derive(Debug, Clone)]
pub struct RealizationRequest<'a> {
    /// What the caller wants (drives schema selection).
    pub intent: Intent,
    /// The caller's query / cue. Embedded in the audit metadata
    /// but not currently surfaced in the prose — future versions
    /// will use it for query-relevance scoring of fragments.
    pub query: &'a str,
    /// Working set the composer has assembled. Fragments are
    /// quoted verbatim in `working_set` order. When `shape` is
    /// `Some`, the realizer uses the shape and ignores
    /// `working_set` for ordering — the working set is still used
    /// for limitation evaluation (`HighConfabRisk`, `ShallowCascade`).
    pub working_set: &'a [CognitiveFragment],
    /// Optional [`CompositionShape`] from cascade-shape-driven
    /// composition (ADR-0006). When `Some`, the realizer walks the
    /// shape's steps and emits the role each step carries. When
    /// `None`, the realizer falls back to the linear walk
    /// (v0.1.1 behaviour) by deriving a shape from `working_set`
    /// via [`shape_from_working_set`].
    pub shape: Option<&'a CompositionShape>,
    /// Ethics report at the `Compose` coverage point. Drives the
    /// `EthicallySensitive` limitation trigger and the
    /// composition's audit metadata.
    pub ethics: Option<&'a EthicsReport>,
}

/// Output of one realization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealizationOutput {
    /// The composed prose, ready to emit to the caller.
    pub text: String,
    /// Which schema produced this composition.
    pub schema_used: SchemaId,
    /// Fragment ids that were quoted, in emission order.
    pub fragments_quoted: Vec<FragmentId>,
    /// Limitation triggers that fired and were acknowledged.
    pub limitations: Vec<LimitationTrigger>,
    /// `true` when the realization emitted no quoted fragments
    /// because the limitation triggers covered the whole working
    /// set (or the working set was empty). In that case `text`
    /// carries only the limitation acknowledgments.
    pub is_acknowledgment_only: bool,
}

/// The realizer. Holds the connective-tissue lexicon and the
/// limitation-context defaults. Construct once per substrate; use
/// many times via [`Self::realize`].
#[derive(Debug, Clone, Default)]
pub struct SurfaceRealizer {
    lexicon: Lexicon,
    limitation_context: LimitationContext,
}

impl SurfaceRealizer {
    /// Construct a realizer with the v0.1 EN baseline lexicon and
    /// default limitation context.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct with a custom lexicon. Useful for register-specific
    /// composition (technical, legal, conversational).
    #[must_use]
    pub fn with_lexicon(lexicon: Lexicon) -> Self {
        Self {
            lexicon,
            ..Self::default()
        }
    }

    /// Override the default limitation context (e.g. raise the
    /// minimum cascade-fragment count for deployments that demand
    /// associative depth).
    pub fn set_limitation_context(&mut self, context: LimitationContext) {
        self.limitation_context = context;
    }

    /// Read-only access to the connective lexicon.
    #[must_use]
    pub fn lexicon(&self) -> &Lexicon {
        &self.lexicon
    }

    /// Realize. Always returns a composition — when triggers cover
    /// the whole working set the composition is acknowledgment-only,
    /// but it is still well-formed prose. The realizer **never
    /// fabricates from nothing**; an empty working set yields the
    /// `EmptyWorkingSet` acknowledgment in place of a body.
    ///
    /// # Errors
    ///
    /// Returns an error only when the caller's intent has no schema
    /// mapping. v0.1 covers every intent — the variant is reserved
    /// for future-shape inputs.
    #[allow(clippy::too_many_lines)]
    pub fn realize(
        &self,
        request: &RealizationRequest<'_>,
    ) -> Result<RealizationOutput, RealizationError> {
        let schema = request.intent.default_schema();

        let limitations =
            evaluate_limitations(request.working_set, request.ethics, self.limitation_context);

        // If the working set is empty, the composition is
        // acknowledgment-only. The realizer never quotes a fragment
        // that does not exist.
        if request.working_set.is_empty() {
            let text = render_acknowledgments_only(&limitations);
            return Ok(RealizationOutput {
                text,
                schema_used: schema,
                fragments_quoted: Vec::new(),
                limitations,
                is_acknowledgment_only: true,
            });
        }

        // Compose per schema. Both v0.1 schemas share the same
        // structural shape (opening connective + quoted fragments
        // with interleaving connectives + closing); the difference
        // is in the attribution layer — `GroundedAssertion` always
        // prefixes each quote with an Attribution connective.
        let mut text = String::new();
        let mut fragments_quoted = Vec::new();

        // ADR-0006: if the caller supplied a CompositionShape we
        // walk it; otherwise we synthesise a linear shape from the
        // working set (the v0.1.1 behaviour). Either way the
        // realizer now operates over a uniform `steps` surface.
        let fallback_shape;
        let shape: &CompositionShape = if let Some(s) = request.shape {
            s
        } else {
            fallback_shape = shape_from_working_set(request.working_set);
            &fallback_shape
        };

        if shape.is_empty() {
            // Working set was non-empty but the shape projected
            // empty (an edge case the linear fallback cannot
            // produce, but a custom shape could). Emit the
            // acknowledgment-only path.
            let text = render_acknowledgments_only(&limitations);
            return Ok(RealizationOutput {
                text,
                schema_used: schema,
                fragments_quoted: Vec::new(),
                limitations,
                is_acknowledgment_only: true,
            });
        }

        // Derive a PickContext from the shape's working set —
        // dominant register across the fragments. Per-pair
        // refinements happen at each step below.
        let shape_fragments: Vec<&CognitiveFragment> =
            shape.steps.iter().map(|s| &s.fragment).collect();
        let opening_ctx = working_set_context_refs(&shape_fragments);
        let opening = self
            .lexicon
            .pick_in_context(ConnectiveRole::Opening, &opening_ctx, 0);
        text.push_str(opening);
        text.push(' ');

        for (idx, step) in shape.steps.iter().enumerate() {
            let fragment = &step.fragment;

            if idx > 0 {
                // ADR-0006: the step's `role` is the cascade-
                // shape-derived suggestion. Compute a polarity
                // from the adjacent valence delta so the picker's
                // context filtering stays honest. If the step's
                // role is Contrast / Concession the polarity
                // comes from the threshold logic; otherwise
                // Continuation.
                let prev_fragment = &shape.steps[idx - 1].fragment;
                let polarity = polarity_for_step(step.role, prev_fragment, fragment);
                let ctx = PickContext {
                    register: register_for_fragment(fragment),
                    polarity,
                    formality: Formality::Mid,
                };
                // ADR-0007: extract boundary signals from the two
                // adjacent quoted bodies and run the smoothing
                // filter so the picker avoids the known
                // redundancy patterns.
                let prev_signal = BoundarySignal::extract(fragment_body(prev_fragment));
                let next_signal = BoundarySignal::extract(fragment_body(fragment));
                let connective = self.lexicon.pick_with_smoothing(
                    step.role,
                    &ctx,
                    idx,
                    Some(&prev_signal),
                    Some(&next_signal),
                );
                text.push(' ');
                text.push_str(connective);
                text.push(' ');
            }

            if matches!(schema, SchemaId::GroundedAssertion) {
                let attribution_ctx = PickContext {
                    register: register_for_fragment(fragment),
                    polarity: Polarity::Neutral,
                    formality: Formality::Mid,
                };
                let attribution = self.lexicon.pick_in_context(
                    ConnectiveRole::Attribution,
                    &attribution_ctx,
                    idx,
                );
                text.push_str(attribution);
                text.push(' ');
            }

            let body = fragment_body(fragment);
            text.push('"');
            text.push_str(body);
            text.push('"');
            fragments_quoted.push(fragment.id);
        }

        // ADR-0016: Summary schema pulls the closing line from
        // `ConnectiveRole::Summary` ("Overall,", "On balance,", …)
        // instead of `ConnectiveRole::Closing`. Every other schema
        // keeps the existing closing slot.
        let closing_role = if matches!(schema, SchemaId::Summary) {
            ConnectiveRole::Summary
        } else {
            ConnectiveRole::Closing
        };
        let closing = self.lexicon.pick_in_context(closing_role, &opening_ctx, 0);
        text.push(' ');
        text.push_str(closing);

        if !limitations.is_empty() {
            text.push_str("\n\n");
            text.push_str(&render_acknowledgments(&limitations));
        }

        Ok(RealizationOutput {
            text,
            schema_used: schema,
            fragments_quoted,
            limitations,
            is_acknowledgment_only: false,
        })
    }
}

/// Derive a polarity for a step's connective lookup given its
/// role and the adjacent fragment pair. Per ADR-0006, the step's
/// role is the cascade-shape-derived default; the polarity
/// refines the lexicon picker's context.
fn polarity_for_step(
    role: ConnectiveRole,
    prev: &CognitiveFragment,
    cur: &CognitiveFragment,
) -> Polarity {
    let delta = cur.valence - prev.valence;
    let abs = delta.abs();
    let opposing = prev.valence.signum() != cur.valence.signum()
        && (prev.valence.abs() > 0.0 || cur.valence.abs() > 0.0);

    match role {
        ConnectiveRole::Contrast => {
            if abs > 0.6 && opposing {
                Polarity::ContrastHard
            } else {
                Polarity::ContrastSoft
            }
        }
        ConnectiveRole::Concession => Polarity::Concession,
        ConnectiveRole::Opening
        | ConnectiveRole::Closing
        | ConnectiveRole::Attribution
        | ConnectiveRole::Summary => Polarity::Neutral,
        _ => Polarity::Continuation,
    }
}

/// Derive a register hint for one fragment from its `domain_tags`.
/// v0.1 heuristic: presence of engineering- / code-flavoured tags
/// picks `Technical`; presence of `informal` / `conversation` picks
/// `Conversational`; default `Neutral`.
fn register_for_fragment(fragment: &CognitiveFragment) -> Register {
    let tech_markers = [
        "engineering",
        "code",
        "systems",
        "infrastructure",
        "deploy",
        "migration",
        "monitoring",
        "technical",
    ];
    let conv_markers = ["informal", "conversation", "chat", "casual"];
    let formal_markers = ["legal", "contract", "policy", "formal", "compliance"];

    for tag in &fragment.domain_tags {
        let t = tag.to_lowercase();
        if tech_markers.iter().any(|m| t.contains(m)) {
            return Register::Technical;
        }
        if conv_markers.iter().any(|m| t.contains(m)) {
            return Register::Conversational;
        }
        if formal_markers.iter().any(|m| t.contains(m)) {
            return Register::Formal;
        }
    }
    Register::Neutral
}

/// Aggregate per-fragment register hints from a slice of
/// references into a single dominant register for openings /
/// closings. The realizer prefers this entry-point because the
/// shape walker holds `&CognitiveFragment` rather than owned
/// values. v0.1 rule: the most common non-neutral register wins;
/// ties fall to `Neutral`.
fn working_set_context_refs(working_set: &[&CognitiveFragment]) -> PickContext {
    use std::collections::HashMap;
    let mut counts: HashMap<Register, usize> = HashMap::new();
    for f in working_set {
        let r = register_for_fragment(f);
        *counts.entry(r).or_insert(0) += 1;
    }
    let dominant = counts
        .iter()
        .filter(|(r, _)| **r != Register::Neutral)
        .max_by_key(|(_, c)| **c)
        .map_or(Register::Neutral, |(r, _)| *r);
    PickContext {
        register: dominant,
        polarity: Polarity::Neutral,
        formality: Formality::Mid,
    }
}

/// Render limitation acknowledgments as prose. One line per
/// trigger, separated by newlines. Empty when the input is empty.
fn render_acknowledgments(triggers: &[LimitationTrigger]) -> String {
    triggers
        .iter()
        .map(|t| format!("[limitation:{}] {}", t.tag(), t.acknowledgment()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render an acknowledgment-only composition. Used when the working
/// set is empty — the realizer emits the acknowledgments and
/// nothing else.
fn render_acknowledgments_only(triggers: &[LimitationTrigger]) -> String {
    if triggers.is_empty() {
        // Should not happen — empty working set always fires
        // EmptyWorkingSet — but defends against a future evaluator
        // that suppresses the trigger.
        return "I have nothing in working memory to respond from.".to_string();
    }
    render_acknowledgments(triggers)
}

/// Extract the body text from a fragment for quoting.
fn fragment_body(fragment: &CognitiveFragment) -> &str {
    match &fragment.content {
        FragmentContent::Episode { body, .. }
        | FragmentContent::Belief { body, .. }
        | FragmentContent::Goal { body, .. }
        | FragmentContent::Observation { body }
        | FragmentContent::Reflection { body, .. }
        | FragmentContent::Journal { body, .. } => body,
        FragmentContent::Reference { uri } => uri,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyphae_ethics::{CoveragePoint, EthicsReport, EthicsSignals, LayerAOutput, LimitationKind};
    use std::collections::HashMap;

    fn obs(body: &str) -> CognitiveFragment {
        let mut f = CognitiveFragment::new(
            FragmentContent::Observation {
                body: body.to_string(),
            },
            "test",
        );
        // Mark as cascade-derived so the ShallowCascade trigger
        // does not fire by default in the tests that don't care.
        f.provenance.parent_ids = vec![FragmentId::new()];
        f
    }

    fn obs_direct(body: &str) -> CognitiveFragment {
        CognitiveFragment::new(
            FragmentContent::Observation {
                body: body.to_string(),
            },
            "test",
        )
    }

    fn empty_report() -> EthicsReport {
        EthicsReport {
            coverage_point: CoveragePoint::Compose,
            profile_id: "test".to_string(),
            profile_version: "0.0.1".to_string(),
            classification: LayerAOutput {
                per_category: HashMap::new(),
                flags: Vec::new(),
                disambiguation: hyphae_ethics::DisambiguationVerdict::default(),
            },
            cvar_score: 0.0,
            categorical: None,
            violations: Vec::new(),
            content_fingerprint: String::new(),
            audit_seq: None,
            signals: EthicsSignals::default(),
        }
    }

    #[test]
    fn empty_working_set_produces_acknowledgment_only() {
        let realizer = SurfaceRealizer::new();
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Dialogue,
                query: "what is the status of project X?",
                working_set: &[],
                ethics: None,
                shape: None,
            })
            .unwrap();
        assert!(out.is_acknowledgment_only);
        assert!(out.fragments_quoted.is_empty());
        assert!(
            out.limitations
                .contains(&LimitationTrigger::EmptyWorkingSet)
        );
        assert!(out.text.contains("[limitation:empty_working_set]"));
    }

    #[test]
    fn dialogue_reply_quotes_fragments_verbatim() {
        let realizer = SurfaceRealizer::new();
        let frag = obs("the build passes the integration suite");
        let frag_id = frag.id;
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Dialogue,
                query: "build status?",
                working_set: std::slice::from_ref(&frag),
                ethics: None,
                shape: None,
            })
            .unwrap();
        assert_eq!(out.schema_used, SchemaId::DialogueReply);
        assert!(
            out.text
                .contains("\"the build passes the integration suite\""),
            "fragment body must appear verbatim and quoted: {}",
            out.text,
        );
        assert_eq!(out.fragments_quoted, vec![frag_id]);
    }

    #[test]
    fn grounded_assertion_prefixes_quotes_with_attribution() {
        let realizer = SurfaceRealizer::new();
        let frag = obs("the migration completed at 14:02 UTC");
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Assert,
                query: "did the migration complete?",
                working_set: &[frag],
                ethics: None,
                shape: None,
            })
            .unwrap();
        assert_eq!(out.schema_used, SchemaId::GroundedAssertion);
        // Some attribution phrase should appear.
        let has_attribution = out.text.contains("source")
            || out.text.contains("material")
            || out.text.contains("fragment");
        assert!(
            has_attribution,
            "GroundedAssertion must prefix quotes with an attribution: {}",
            out.text,
        );
    }

    #[test]
    fn high_confab_risk_fires_and_acknowledgment_appears() {
        let realizer = SurfaceRealizer::new();
        let mut frag = obs("a claim of uncertain provenance");
        frag.provenance.confabulation_risk = 0.8;
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Dialogue,
                query: "tell me about it",
                working_set: &[frag],
                ethics: None,
                shape: None,
            })
            .unwrap();
        assert!(out.limitations.contains(&LimitationTrigger::HighConfabRisk));
        assert!(out.text.contains("[limitation:high_confab_risk]"));
        // The composition itself still proceeded — RADAR, not JAIL.
        // The realizer surfaces the caveat alongside the quote, not
        // instead of it.
        assert!(!out.is_acknowledgment_only);
        assert!(out.text.contains("\"a claim of uncertain provenance\""));
    }

    #[test]
    fn ethics_signal_drives_ethically_sensitive_acknowledgment() {
        let realizer = SurfaceRealizer::new();
        let mut report = empty_report();
        report.signals.composer_should_acknowledge = true;
        report.signals.composer_limitation_kind = Some(LimitationKind::CategoricalConcern);
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Dialogue,
                query: "tell me about anthrax synthesis",
                working_set: &[obs("anthrax is a bacterial disease")],
                ethics: Some(&report),
                shape: None,
            })
            .unwrap();
        assert!(
            out.limitations
                .contains(&LimitationTrigger::EthicallySensitive)
        );
    }

    #[test]
    fn shallow_cascade_fires_with_direct_only_working_set() {
        let realizer = SurfaceRealizer::new();
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Dialogue,
                query: "anything",
                working_set: &[obs_direct("a direct hit")],
                ethics: None,
                shape: None,
            })
            .unwrap();
        assert!(out.limitations.contains(&LimitationTrigger::ShallowCascade));
    }

    #[test]
    fn opposing_valences_select_contrast_connective() {
        let realizer = SurfaceRealizer::new();
        let mut positive = obs("the deploy succeeded");
        positive.valence = 0.6;
        let mut negative = obs("the monitoring alarm fired ten minutes later");
        negative.valence = -0.6;
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Dialogue,
                query: "deploy status?",
                working_set: &[positive, negative],
                ethics: None,
                shape: None,
            })
            .unwrap();
        // A contrast connective from the lexicon should appear
        // between the two quotes. The baseline contrast phrases
        // are "On the other hand,", "However,", "By contrast,",
        // "Yet —".
        let has_contrast = out.text.contains("On the other hand,")
            || out.text.contains("However,")
            || out.text.contains("By contrast,")
            || out.text.contains("Yet");
        assert!(
            has_contrast,
            "opposing valences must select a Contrast connective: {}",
            out.text,
        );
    }

    #[test]
    fn aligned_valences_select_continuation_connective() {
        let realizer = SurfaceRealizer::new();
        let mut a = obs("the deploy succeeded");
        a.valence = 0.6;
        let mut b = obs("the monitoring stayed green for two hours afterward");
        b.valence = 0.5;
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Dialogue,
                query: "deploy status?",
                working_set: &[a, b],
                ethics: None,
                shape: None,
            })
            .unwrap();
        // Should NOT contain any of the contrast phrases.
        let has_contrast = out.text.contains("On the other hand,")
            || out.text.contains("However,")
            || out.text.contains("By contrast,")
            || out.text.contains("Yet —");
        assert!(
            !has_contrast,
            "aligned valences must NOT select a Contrast connective: {}",
            out.text,
        );
    }

    #[test]
    fn multiple_limitations_all_appear_in_output() {
        let realizer = SurfaceRealizer::new();
        let mut frag = obs_direct("risky direct content");
        frag.provenance.confabulation_risk = 0.8;
        let mut report = empty_report();
        report.signals.composer_should_acknowledge = true;
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Dialogue,
                query: "tell me",
                working_set: &[frag],
                ethics: Some(&report),
                shape: None,
            })
            .unwrap();
        assert!(out.limitations.contains(&LimitationTrigger::HighConfabRisk));
        assert!(out.limitations.contains(&LimitationTrigger::ShallowCascade));
        assert!(
            out.limitations
                .contains(&LimitationTrigger::EthicallySensitive)
        );
        for trigger in &out.limitations {
            assert!(
                out.text
                    .contains(&format!("[limitation:{}]", trigger.tag())),
                "limitation {trigger:?} must appear in the output text: {}",
                out.text,
            );
        }
    }

    #[test]
    fn output_does_not_paraphrase_fragment_body() {
        let realizer = SurfaceRealizer::new();
        let frag = obs("VERBATIM_BODY_TOKEN_42 should appear unchanged");
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Dialogue,
                query: "what about the token?",
                working_set: &[frag],
                ethics: None,
                shape: None,
            })
            .unwrap();
        assert!(
            out.text
                .contains("VERBATIM_BODY_TOKEN_42 should appear unchanged"),
            "fragment body must be quoted verbatim: {}",
            out.text,
        );
    }

    /// ADR-0017 — a realizer instantiated with the Spanish
    /// lexicon emits Spanish connective tissue around the (verbatim)
    /// fragment bodies.
    #[test]
    fn realizer_with_es_lexicon_emits_spanish_tissue() {
        let realizer = SurfaceRealizer::with_lexicon(Lexicon::baseline_es());
        let frag = obs("la migración terminó a las 14:02 UTC");
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Dialogue,
                query: "¿cuál es el estado de la migración?",
                working_set: &[frag],
                ethics: None,
                shape: None,
            })
            .unwrap();
        // Body quoted verbatim — boundary the no-LLM-in-cognition-
        // path commitment depends on, regardless of lexicon
        // language.
        assert!(
            out.text.contains("la migración terminó a las 14:02 UTC"),
            "ES output must preserve fragment body verbatim: {}",
            out.text,
        );
        // Spanish opening must appear (one of the baseline_es
        // Opening entries' distinctive nouns).
        let lower = out.text.to_lowercase();
        let has_spanish_opening = lower.contains("memoria")
            || lower.contains("registros")
            || lower.contains("conservado")
            || lower.contains("almacenado")
            || lower.contains("datos");
        assert!(
            has_spanish_opening,
            "ES output must contain a Spanish opening marker: {}",
            out.text,
        );
        // English defaults must NOT appear — the realizer is
        // lexicon-locked to ES.
        assert!(
            !out.text.contains("Drawing from working memory,"),
            "ES output must not leak the EN default opening: {}",
            out.text,
        );
        assert!(
            !out.text
                .contains("That is what working memory holds on this."),
            "ES output must not leak the EN default closing: {}",
            out.text,
        );
    }

    /// ADR-0016 — `Intent::Summarize` produces `SchemaId::Summary`
    /// and the closing line is drawn from `ConnectiveRole::Summary`.
    #[test]
    fn summary_schema_uses_summary_role_for_closing() {
        let realizer = SurfaceRealizer::new();
        let frags: Vec<_> = (0..3)
            .map(|i| obs(&format!("synthesis fragment {i}")))
            .collect();
        let out = realizer
            .realize(&RealizationRequest {
                intent: Intent::Summarize,
                query: "summarise the recent activity",
                working_set: &frags,
                ethics: None,
                shape: None,
            })
            .unwrap();
        assert_eq!(out.schema_used, SchemaId::Summary);
        // The summary lexicon (see connective_data.rs:1490) starts
        // every entry with one of these tokens — at least one must
        // appear in the output text.
        let summary_markers = [
            "In summary,",
            "Overall,",
            "On balance,",
            "Taking it together,",
            "Putting it together,",
            "Bringing",
            "Across the working set,",
            "The shape of it is that,",
            "The picture overall is,",
            "Summing up,",
            "All things considered,",
        ];
        assert!(
            summary_markers.iter().any(|m| out.text.contains(m)),
            "Summary schema must use a Summary-role closing; got: {}",
            out.text,
        );
        // And `DialogueReply`'s default closing must NOT appear.
        assert!(
            !out.text
                .contains("That is what working memory holds on this."),
            "Summary closing must not be the DialogueReply default: {}",
            out.text,
        );
    }
}
