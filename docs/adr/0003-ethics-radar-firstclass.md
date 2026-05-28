<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0003
title: Ethics Engine is first-class, RADAR philosophy, full cognition-path coverage
status: accepted
date: 2026-05-26
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (v2 chartering session review)]
---

# 0003 — Ethics Engine is first-class, RADAR philosophy, full cognition-path coverage

## Context

The v1 honest documentation of the ethics engine
(`../hyphae/docs/decisions/0002-ethics-engine-current-state.md`) recorded
four decisions that had been taken **implicitly**, not by deliberate
design:

  1. The ethics engine was distributed across three subsystems (Layer
     1 in S4 Amygdala, corpus baseline in S3 Hippocampus, audit in
     S11 Medulla) with coordination at the substrate level. The
     alternative — a dedicated ethics subsystem or crate — was never
     explicitly rejected. The shape was adopted by omission.

  2. The ethics engine implemented **JAIL** semantics: block on
     threshold breach. 18 call sites in `hyphae-substrate/src/lib.rs`
     refused the calling operation when `conflict_signal > 0.6`. This
     **directly contradicts** the verbatim declaration of celiums-
     memory v2's ethics engine
     (`packages/core/src/ethics-dispatcher.ts`): *"The ethics engine
     is a RADAR, not a JAIL. It classifies and logs for audit. It does
     NOT censor user expression."* Two organisations under the same
     entity (Celiums) ran two motors with contradictory philosophies.

  3. The ethics gate scope covered `memory_*_secure` and
     `journal_*_secure` only. It did NOT cover `PollinatePolicy::
     retrieve` (Vertex AI grounding bypass), `DecomposePolicy::
     retrieve` (grounded fragments backfilling concept gaps), the
     `compose()` composition path (responses returned to the caller
     bypassed evaluation), or curiosity firing decisions (no
     pre-check on outbound query text). This was decision-by-omission:
     grounding and composition shipped after the gate landed, and the
     gate was not extended.

  4. The v1 implementation covered approximately 10–15% of celiums-
     memory v2's ethics engine capabilities (LOC ratio and capability
     inventory). v1 shipped a binary lexicon of 30 EN tokens with a
     single threshold; celiums-memory v2 ships 10 languages, 12
     taxonomy categories, structural hate detector, context
     disambiguation, CVaR risk quantification with asymmetric
     reversibility weighting, philosophical multi-framework
     evaluation, precedent advisory, taxonomy / thresholds / XAI /
     bias / calibration / rate-limit infrastructure — 21 TypeScript
     files, approximately 5545 LOC.

In addition, the v1 ADR documented **deuda visible** that was
ledger-acknowledged but not closed:

  - The `S8b ACC.conflict_signal()` method existed in v1 but was NOT
    cabled into the ethics path. Substrate comment verbatim: *"a
    future refinement may route it through ACC.process for the real
    integrator."*
  - The Plasticity Charter draft P-3 (`dual-input learning: reward PE
    + Ethics Engine signals`) was not implemented; charter remained
    in draft status.

The v1 documentation was honest about all of this. The v2 chartering
session reached the conclusion that an honest documentation of
shortcomings is not an architecture — fixing them is. v2 is the
fixing.

## Decision

The Ethics Engine in Hyphae v2 is:

  - A **dedicated crate** (`hyphae-ethics`), not distributed by
    omission. Substrate integration is explicit through typed
    interfaces, not through coincidental anatomical assignment.

  - **RADAR philosophy**. The engine classifies, audits, and emits
    signals. It **does not block** operations. Callers receive
    composition + structured ethics report. v1's JAIL is corrected.

  - **Wired into the full cognition path from commit zero**. Five
    mandatory evaluation points. No path that can ingest, compose, or
    retrieve content bypasses the engine.

  - **Layers A + B + Audit** in v0.1 with multilingual posture at
    the crate API level (even if EN ships first per RFC §9's negative
    scope). Layers C and K are `deferred` with explicit ADR-level
    rationale.

  - **One hash chain** shared with the substrate journal, not a
    separate audit log.

### 1. Crate structure

```
crates/hyphae-ethics/
├── src/
│   ├── lib.rs            # public API: EthicsEngine, EthicsReport
│   ├── layer_a.rs        # deterministic lexicon + taxonomy + structural
│   ├── layer_b.rs        # probabilistic CVaR + asymmetric reversibility
│   ├── taxonomy.rs       # 12-category SafetyBench/Jigsaw/DSA/OWASP
│   ├── lexicon.rs        # multilingual lexicon loader (EN-loaded in v0.1)
│   ├── disambiguation.rs # living target / technical / meta context
│   ├── structural.rs     # structural hate pattern detector
│   ├── audit.rs          # journal entry types (shared chain)
│   ├── profile.rs        # profile-loader (per celiums-memory v2 ADR-021)
│   └── thresholds.rs     # configurable per-profile thresholds
└── tests/
    └── coverage_integration.rs  # validates 5-point coverage
```

The crate **does not depend on `hyphae-substrate`**. The dependency
direction is `substrate → ethics`, never the reverse. Ethics is a
library that the substrate consumes at the five evaluation points.

### 2. RADAR semantics in detail

The engine never returns "blocked." It returns an `EthicsReport`:

```rust
pub struct EthicsReport {
    pub classification: TaxonomyClassification,  // Layer A output
    pub cvar_score: f32,                          // Layer B output, ∈ [0, 1]
    pub violation_flags: Vec<ViolationFlag>,     // per Layer A + B
    pub baseline_deviation: f32,                  // corpus baseline diff
    pub audit_entry: JournalEntryId,             // for chain reference
    pub signals: EthicsSignals,                   // for composer + learning
}

pub struct EthicsSignals {
    pub composer_should_acknowledge: bool,        // → composer adds limitation
    pub composer_limitation_kind: Option<LimitationKind>,
    pub learning_weight_delta: ParameterDeltaHint, // → learning loop input
}
```

Callers receive the report alongside the composition. The composer
decides whether to add a `LimitationAcknowledgment` slot to the surface
output based on `composer_should_acknowledge`. The learning loop
consumes `learning_weight_delta` as one of the two feedback channels
(ADR-0002 §"Feedback signals").

No call site receives `Err(EthicsBlocked(...))`. The 18 v1 call sites
of `"denied by ACC ethics gate"` are not ported.

### 3. Coverage of the cognition path (five evaluation points)

The contract is enforced at the substrate level. Operations that touch
content without invoking `ethics_evaluate` are integrity errors,
journal-logged, and surfaced in CI as test failures.

**Point 1 — `remember`.** Input fragments are evaluated before
encoding. The report is attached to the fragment's `Provenance` via a
new `ethics_audit: JournalEntryId` field.

**Point 2 — `recall` and cascade activation results.** When `episodic`
returns a recall set (direct + cascade-propagated), the set passes
through ethics evaluation as a batch. Per-fragment classifications are
attached. The composer consumes the annotated set.

**Point 3 — `compose`.** Before the surface realizer emits the
composition, the candidate composition is evaluated. If the report
fires `composer_should_acknowledge`, the realizer inserts a limitation
slot. The composition is then emitted with the report attached.

**Point 4 — Grounded retrieval.** When introduced post-v0.1, every
external retrieval (Vertex AI, web fetch, whatever provider lands)
passes through ethics evaluation **before absorption**. Fragments that
flag are absorbed with their flag annotation; they are NOT discarded
(RADAR, not JAIL). The composer can decide to surface them with
limitation or skip them based on report content.

**Point 5 — Learning loop parameter updates.** Before the learning loop
commits a parameter update, the proposed update + its triggering
feedback are evaluated. Updates that would systematically shift the
composer toward outputs that flag at Layer A or B's hard rules are
journaled with the report but **not blocked** — the audit visibility is
the discipline, not the censorship.

### 4. Layers in v0.1

**Layer A — Deterministic lexicon + taxonomy.**

  - 12-category taxonomy: hate, violence, PII, self-harm, deception,
    cyber, misinformation, privacy, autonomy, system override, plus
    two additional categories from celiums-memory v2's SafetyBench /
    Jigsaw / DSA / OWASP mapping.
  - Structural hate pattern detector (for indirect / coded language).
  - Context disambiguation: living target / technical / meta. v1
    documented these patterns in `ethics-normalizer.ts` and
    `ethics-lexicon.ts`; v2 re-implements them in Rust.
  - Multilingual lexicon API. v0.1 ships EN only (per RFC §9's
    negative scope). The API accepts language tags so re-entry of ES,
    PT, etc. is additive, not a refactor.

**Layer B — Probabilistic CVaR.**

  - Conditional Value-at-Risk over flagged risks, 5%-tail.
  - Asymmetric weighting for irreversibility (higher weight on
    irreversible harm).
  - Profile-loader system per celiums-memory v2 ADR-021. The
    `BASELINE_PROFILE` ships in-tree, fully functional standalone.
  - **Categorical hard rule for CBRN with operational intent.**
    Deterministic rule, bypasses the probabilistic path. Calibrated
    to require both a CBRN term and operational intent (the
    historical/educational mention bypass). This is the lesson v1's
    celiums-memory v2 ethics learned from the under-block incident
    documented in the celiums-memory MANIFESTO §8.
  - Native Rust implementation. Approximately 300 LOC. **No new
    dependency** — implementable from `std` math.

**Audit.**

  - Every evaluation writes a journal entry on the shared chain. The
    entry carries: the input content hash, the report, the evaluation
    timestamp, and the chain prev-hash.
  - Chain verification on Recovery state covers ethics entries
    identically to memory and journal entries.

### 5. Layer C deferral (multi-framework philosophical evaluation)

celiums-memory v2 implements Layer C via an LLM dispatcher with frame
isolation across five ethical frameworks (utilitarian, deontological,
virtue, care, justice). The LLM call is in the ethics path.

Hyphae v2 commitment #1 prohibits LLM in the cognition path. The
ethics path is **adjacent to** the cognition path — it evaluates the
content that flows through cognition, but it is not the composition
path itself. Whether Layer C's LLM invocation would violate
commitment #12 ("composition uses fragment quotation, not novel
language synthesis") is the framing question that decides Layer C's
re-entry.

Three re-entry options are surveyed:

  - **Option α — Skip Layer C indefinitely.** Accept ~30-40% capability
    coverage of celiums-memory v2. Preserves commitment #1 without
    asterisk.
  - **Option β — Reformulate Layer C compositional.** The five
    frameworks as schemas applied compositionally over the candidate
    output. Cita verbatim of ethics fragments, not LLM synthesis.
    More ambitious; preserves compositional posture; requires a
    framework corpus of ethical fragments (analogous to Layer K's
    `ethics_knowledge`).
  - **Option γ — Formalise "ethics path" separately from "cognition
    path".** Layer C uses LLM dispatcher but only in the ethics path,
    which is declared as adjacent infrastructure, not cognition.
    Opens a boundary v1 never formalised; potential slippery slope.

**v0.1 chooses Option α (skip).** Re-entry decision is post-v0.1, with
the BDFL choosing between β and γ based on observed need from the
v0.1 metrics. The skip is documented; it is not a coverage gap by
omission.

### 6. Layer K deferral (precedent advisory)

celiums-memory v2's Layer K consults an `ethics_knowledge` corpus
(~31 MB JSONL with precomputed embeddings) distributed as a release
asset (not in the git tree). The corpus enables a precedent-flag-only
mechanism that catches systematic over-blocking by Layer A or B.

Layer K is `deferred` for v0.1 with the same posture celiums-memory v2
ships: the corpus is a separate distribution; the engine runs without
it and the layer abstains cleanly when the corpus is absent.

Re-entry: when v0.1 metrics indicate Layer A's over-block rate is
detectable, the corpus loader is built and the layer is wired. The
mechanism is `flag-only`, never silently overrides.

### 7. Capability coverage estimate (v0.1)

Approximately **30–40%** of celiums-memory v2's ethics engine in
v0.1, honestly declared:

  - ✅ Layer A multilingual API (EN populated)
  - ✅ 12-category taxonomy
  - ✅ Structural hate pattern detector
  - ✅ Context disambiguation (3 contexts)
  - ✅ Layer B CVaR with asymmetric reversibility
  - ✅ Categorical CBRN hard rule
  - ✅ Profile-loader with BASELINE_PROFILE
  - ✅ Hash-chained audit on shared substrate chain
  - ✅ Coverage of 5 cognition-path points from commit zero
  - ⏸️ Layer C philosophical (3 re-entry options surveyed)
  - ⏸️ Layer K precedent (corpus deferred, mechanism reserved)
  - ⏸️ Calibration / XAI / bias / rate-limit infrastructure
    (re-entry per item with criterion)
  - ⏸️ Lexicons for ES / PT / FR / DE / IT / RU / TR / JA / ZH
    (re-entry post v0.1 validation, per RFC §9)

This is **explicit deferral**, not the 10-15% coverage v1 reached by
omission. Every ⏸️ above carries an articulated re-entry path.

### 8. Why the shared hash chain

Two options were considered for the audit log:

  - **Two chains.** A substrate journal chain and a separate ethics
    audit chain. Independent integrity; clean separation of concerns.
  - **One chain shared.** All entries (memory ingest, journal writes,
    state transitions, ethics evaluations, learning parameter
    updates) on the same SHA-256 chain.

v2 chooses **one chain** for three reasons:

  - **Causal ordering preservation.** When a recall produces a
    composition that is evaluated by ethics, all three events are on
    one totally-ordered chain. Reconstructing the causal sequence is
    `git log`-trivial.
  - **Tamper resistance.** Splitting into two chains halves the work
    of an attacker who can tamper with one of them. One chain is one
    integrity surface.
  - **Recovery simplicity.** Chain verification on Recovery state
    runs one walk, not two with reconciliation logic between them.

### 9. The composer-ethics wire (closes v1's `documented-pending`)

v1's `S8b ACC.conflict_signal()` existed but was not cabled into the
ethics path. v2 corrects this **from commit zero**: the `composer`
subsystem's conflict signal is one of the inputs to ethics evaluation
at Point 3 (compose). The wire is the substrate's responsibility, not
a future-refinement comment.

## Consequences

  - `hyphae-ethics` ships as one of the foundational crates in the
    cherry-pick order. Specifically, it lands **before**
    `hyphae-substrate`'s state machine and pathway routing, because
    the substrate's five evaluation points consume the ethics API.
  - `hyphae-substrate` exposes the five evaluation points as
    mandatory hooks. Operations that bypass them are integrity errors,
    not soft warnings.
  - The hash chain in `hyphae-storage` is shared from commit zero.
    Entries carry an `EntryKind` discriminant for grep-ability, but
    they all link via `prev_hash`.
  - The contradiction with celiums-memory v2 (JAIL vs RADAR) is
    resolved in favour of RADAR. The two Celiums ethics motors now
    agree on philosophy.
  - The 18 v1 call sites of `"denied by ACC ethics gate"` are not
    ported. The semantics is fundamentally different — the report
    flows, the operation completes.
  - v0.1's ethics capability is ~30-40% of celiums-memory v2's,
    honestly declared, with explicit re-entry paths per deferred
    capability.
  - **The four implicit decisions documented in v1's ADR-0002 are
    closed.** Decision 1 (distributed by omission) → dedicated crate.
    Decision 2 (JAIL by omission) → RADAR by design. Decision 3
    (coverage gap by omission) → five-point coverage from commit zero.
    Decision 4 (capability gap silently 10-15%) → ~30-40% with
    explicit deferrals.

## Cross-references

  - **ADR-0001** (fresh-from-v1) — companion decision.
  - **ADR-0002** (learning-loop-firstclass) — consumes the Ethics
    signals channel produced by this crate.
  - **`../hyphae/docs/decisions/0002-ethics-engine-current-state.md`**
    — the v1 honest documentation of the four implicit decisions this
    ADR closes. The retroactive articulation discipline that v1 ADR
    instituted is the model this ADR continues — but with the
    decisions made deliberately rather than by omission.
  - **The celiums-memory upstream project** (separate repo) —
    verbatim source of the "RADAR not JAIL" declaration this ADR
    aligns Hyphae with, in its ethics-dispatcher module.
  - **celiums-memory's manifesto §8** — documentation of the
    under-block incident that produced the categorical CBRN hard
    rule. v2 ships the same rule.
  - **`../hyphae/docs/plasticity/charter_draft.md` §P-3** — the v1
    Plasticity Charter draft articulation of dual-input learning
    (RPE + Ethics). v2 implements both channels per ADR-0002 §
    "Feedback signals".
