<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] — between v0.1.0 and the next tag

The architectural surface continued to expand after the v0.1.0
tag with the **v0.2 work**: chat REPL, performance baseline, the
four v1-postponed schemas (Summary, ComparativeAnalysis,
IntrospectiveAssessment, NarrativeArc), the Spanish-language
path (lexicon, corpus, boundary rules, audit refit, scale, and
cross-regional Conversational expansion), and supporting fields
(`event_time` on `CognitiveFragment`).

A v0.2.0 tag will be cut when the BDFL judges the work
release-shaped; for now the v0.1.0 tag remains the canonical
release.

### Added since v0.1.0

- **ADR-0011**: Cascade wakeup — `recall_signal` now invokes
  `episodic.cascade()` post-`pattern_complete`, propagating
  activation through the conductivity graph and marking
  cascade-derived fragments with `parent_ids`.
- **ADR-0013**: Learning loop orchestration. New
  `LearningOrchestrator` in `hyphae-learning` closes the
  record → propose → audit → apply loop end-to-end.
- **ADR-0014**: Interactive chat REPL (`hyphae-chat` crate).
  Substrate persists across turns; heuristic mode selection by
  `?` punctuation; slash commands for explicit control.
- **ADR-0015**: Performance baseline harness (`hyphae-bench`
  crate). Criterion-driven ingest / recall / compose
  measurements at varying substrate populations. Numbers in
  `docs/perf/v0.2-baseline.md`.
- **ADR-0016**: `SchemaId::Summary` re-entered. Closing slot
  draws from `ConnectiveRole::Summary` ("Overall,", "On
  balance,", "Taking it together,").
- **ADR-0017**: Multilingual ES re-entered. Hand-curated
  Spanish lexicon (~60 entries) by the BDFL with `Lexicon::baseline_es()`
  constructor.
- **ADR-0018**: ES eval corpus. Five native-Spanish queries
  exercise the ES path through the existing harness.
- **ADR-0019**: ES boundary rules. `BoundaryRules` struct with
  `ENGLISH` and `SPANISH` constants; realizer + scorer wired
  to the lexicon's language-specific rules.
- **ADR-0020**: Per-lexicon sensitivity audit baselines. The
  audit's three lexicon-dependent verifications now construct
  baselines from the lexicon under test; ES audit floor
  promoted 6/9 → 9/9 sensitive.
- **ADR-0021**: ES lexicon Formal/Neutral/Technical expansion.
  68 model-drafted entries pending native-speaker review;
  Conversational left untouched.
- **ADR-0022**: ES Conversational cross-regional expansion.
  17 model-drafted entries (LATAM + Spain attested forms);
  explicit exclusion list for regional variants.
- **ADR-0023**: `SchemaId::ComparativeAnalysis` re-entered.
  Realizer forces `ConnectiveRole::Contrast` inter-fragment
  regardless of cascade-shape suggestion.
- **ADR-0024**: `SchemaId::IntrospectiveAssessment` re-entered.
  Realizer forces `ConnectiveRole::Concession` inter-fragment.
- **ADR-0025**: `SchemaId::NarrativeArc` re-entered (final
  v1-postponed schema closure). Realizer sorts fragments by
  `created_at` ascending and forces `ConnectiveRole::Sequence`
  inter-fragment.
- **ADR-0026**: `event_time: Option<SystemTime>` on
  `CognitiveFragment`. `NarrativeArc` schema reads
  `event_time.unwrap_or(created_at)` so backfilled fragments
  surface in event-order rather than ingest-order.
- README gained the **"On the prose style"** section explaining
  that template-rigid output is the architectural feature, not
  a defect.
- The smoke binary was modernized to exercise every v0.1 + v0.2
  ADR in a single pass (real `recall_signal`, real learning
  orchestration, no synthetic working sets).

### Note on ADR-0012

ADR-0012 is **deliberately vacant**. The 2026-05-27 audit found
ethics 5-point coverage already complete in v0.1; no decision
was needed at that slot. Future ADRs continue from 0013
forward.

## [0.1.0] — 2026-05-26

The first articulated release of Hyphae v2.

### Architectural commitments (from ADR-0001)

1. No LLM in the cognition path.
2. Ethics is RADAR semantics, not JAIL.
3. Ethics five-point coverage from day one.
4. Learning loop first-class.
5. Six functional subsystems, not seventeen.
6. Five native operations, no tool zoo.
7. EN-only for v0.1; multilingual when the bet validates.
8. Hash-chained journal non-negotiable.
9. Every fragment carries Provenance.
10. State-gated pathways enforced at the substrate.
11. Cascade activation is the retrieval mechanism (cabled in
    ADR-0011 post-v0.1.0).
12. Fragment quotation + connective tissue, not novel language
    synthesis.

### Substrate (ADRs 0001–0010)

- Six functional subsystems: `input-gate`, `episodic`,
  `valence`, `composer`, `predictive`, `reward`.
- Five native operations: `ingest` (Remember), `recall_signal`,
  `compose_signal`, `propose_learning_update`,
  `journal_verify_chain`.
- Hash-chained journal + state store: SHA-256 chain, one per
  substrate, shared with the ethics audit; `fjall` + `redb`
  persistence.
- Five-point ethics coverage: `Remember` / `Recall` /
  `Compose` / `LearningUpdate` active; `GroundedRetrieval`
  deferred per RFC §9.
- Surface realizer with 250+ EN connective entries, 10 roles,
  cascade-shape composition (ADR-0006), boundary smoothing
  (ADR-0007).
- Eval harness with 25-query EN corpus, 9 scoring dimensions,
  scorer sensitivity audit (ADR-0010), honest caveats inline.

### Tests

288 tests across 10 crates; clippy clean; fmt clean.

### Not yet in v0.1.0 (deferred per RFC §9)

- Multilingual beyond EN.
- External retrieval providers (Vertex AI grounding, etc).
- The four postponed subsystems (Mammillary, Septum, 5HT,
  Claustrum, Basal ganglia).
- Schemas beyond DialogueReply + GroundedAssertion.
- Plugin layer for LLM hosts.
- Multimodal perception.

[Unreleased]: ../../compare/v0.1.0...HEAD
[0.1.0]: ../../releases/tag/v0.1.0
