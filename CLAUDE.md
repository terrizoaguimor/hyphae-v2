<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

# CLAUDE.md — Operating Instructions for Claude Code (Hyphae v2)

This file is read by Claude Code at session start. It is the operating
discipline for assistant-driven development on Hyphae v2. It supersedes
the v1 CLAUDE.md (preserved at `../hyphae/CLAUDE.md` for reference).

## Project identity

**Hyphae v2** is a Cognitive Language Model (CLM): a cognitive substrate
that produces language-equivalent output through compositional
mechanisms operating on structured fragments and explicit grammatical
rules, without statistical token prediction over learned distributions.
Six functionally-collapsed subsystems communicate through typed pathways
under five global states. The ethics engine and the learning loop are
first-class crates wired into the substrate from RFC v0.1, not deferred.

For motivation and project context see [`README.md`](README.md). For the
architectural specification see [`docs/rfc/v1-living.md`](docs/rfc/v1-living.md).
For the decisions that shaped v2 see [`docs/adr/`](docs/adr/).

## Hard Architectural Commitments (do not re-litigate)

These were settled during the v2 chartering session. Propose changes
through an ADR, never by silently reinterpreting:

1. **No LLM in the cognition path.** Composition is deterministic over
   fragments + lexicon + grammar rules + cascade activation. External
   models (when they exist) are retrieval providers only. If a design
   decision tempts you to invoke an LLM in compose / recall / cascade /
   ethics evaluation, stop and open an ADR.

2. **Ethics engine is RADAR, not JAIL.** It classifies, audits, and emits
   signals. It does NOT block operations. Caller receives composition +
   structured ethics report. Signals feed the composer (may add
   limitation acknowledgment) and the learning loop (refines weights).
   v1 implemented JAIL by omission; that is corrected here. See
   ADR-0003.

3. **Ethics coverage of the full cognition path, from day one.** Five
   evaluation points are mandatory: `remember`, `recall` / cascade
   activation results, `compose`, grounded retrieval (when introduced),
   and learning loop parameter updates. No "we'll add coverage when that
   path exists" — paths are designed *with* the ethics hook already
   wired. ADR-0003 §"Coverage".

4. **Learning loop is first-class from RFC v0.1.** Substrate (lexicon,
   grammar, schemas, state machine, pathway topology) is immutable.
   Refinable parameters: conductivity weights, salience weights, cascade
   thresholds, schema selection priors. Feedback signals: Reward
   prediction error (predictive + reward subsystems) + Ethics signals
   (hyphae-ethics). Every update is a journal entry; rollback via
   journal replay. See ADR-0002.

5. **Six functional subsystems, not seventeen.** `input-gate`,
   `episodic`, `valence`, `composer`, `predictive`, `reward`. Subsystem
   additions require an ADR documenting the empirical evidence that the
   collapsed pair is insufficient. Postponed-with-criterion list in
   ADR-0001 §"Postponed subsystems".

6. **Five native operations, no tool zoo.** `compose`, `recall`,
   `remember`, `journal_write`, `journal_recall`, `journal_verify_chain`,
   plus `ethics_evaluate` and `ethics_audit`. v1 ported 38 tools from
   the celiums-memory MCP wrapper; most were LLM-shape and did not
   compose with the substrate. v2 does not port those.

7. **English only for v0.1.** Multilingual support was a v1 wave-2
   overhead that consumed cycles without validating the architectural
   bet. EN-first; ES re-enters when the bet validates and a coverage
   extension ADR is filed.

8. **Hash-chained journal is non-negotiable.** Every significant event
   (memory ingest, journal write, learning parameter update, ethics
   evaluation) writes a SHA-256-chained entry. The chain is verified on
   every Recovery state transition. Substrate journal and ethics audit
   share the chain — one chain per substrate, not two.

9. **Every fragment carries Provenance.** `source_subsystem`,
   `source_pathway`, `parent_ids`, `confabulation_risk` are populated by
   the producer, never default-zero silently. Measurement emitters set
   `confabulation_risk = 0.0`; single-input transformers propagate the
   source's risk; passthroughs preserve.

10. **State-gated pathways enforced at the substrate, not by
    convention.** Five states: Encoding, Recall, Consolidation,
    Dormancy, Recovery. Violations are journal-logged integrity errors.

11. **Cascade activation is the retrieval mechanism, not optional
    enhancement.** Every composition consumes both direct retrieval
    matches and propagation-activated fragments. Cascade is implemented
    in the `episodic` subsystem (binding/conductivity) and the
    `composer` subsystem (filtering).

12. **Composition uses fragment quotation + connective tissue, not
    novel language synthesis.** Fragments are opaque content sources
    whose body text is preserved verbatim. The surface realizer
    generates only the structural prose connecting them. This boundary
    is load-bearing for the no-LLM-in-cognition-path claim.

## Working conventions

### Branching and PRs
- Trunk-based. `main` always green.
- Short-lived feature branches per task.
- Conventional Commits: `feat:`, `fix:`, `docs:`, `test:`, `bench:`,
  `refactor:`, `chore:`.
- Every PR includes tests. New functionality requires new tests.
- CI must pass before merge: `cargo fmt --check`,
  `cargo clippy -- -D warnings`, `cargo build --workspace`,
  `cargo test --workspace`.

### Triangulation — opt-in for architecture, not for implementation
v1's discipline required pre-commit triangulation against deepseek-v4-pro
and gemini-3.5-flash for every foundation commit. That overhead was a
material cause of the v1 track stalling. v2's posture:

- **Architectural changes** (new commitments in the living RFC, new
  crates, changes to pathway topology, changes to ethics philosophy):
  triangulate **before** the ADR is filed. Atlas via the celiums-memory
  MCP if available; the BDFL otherwise.
- **Implementation milestones** (subsystem skeletons, tool ports,
  schema additions): triangulate **only if the BDFL requests it for that
  specific milestone**, not by default.

Triangulation pushback is signal to refine design, not friction to
dismiss. But the discipline is sized to the founder's actual capacity.

### Code style
- `#![warn(missing_docs)]` on every crate's `lib.rs`.
- `#![warn(clippy::pedantic)]` where it does not produce excessive
  false positives.
- Prefer explicit types for public API. Inference for internal locals.
- Error handling: `thiserror` for library, `anyhow` only in binary/
  example. Never `unwrap()` or `expect()` in library code outside
  tests.
- Async via `tokio`. Public APIs are async by default. Sync-only paths
  are documented explicitly.
- Use `tracing` for logs, not `println!` / `eprintln!`. Spans for
  operations with clear begin/end.

### Testing
- **Unit tests** in `#[cfg(test)]` blocks inside the source file.
- **Property tests** via `proptest` for invariants (hash chain
  integrity, state transition validity, pathway type safety).
- **Snapshot tests** via `insta` for stable serialization formats.
- **Integration tests** under `crates/<name>/tests/` for cross-module
  behaviour within a crate; workspace-level integration under
  `crates/hyphae-tests/tests/`.
- **Benchmarks** via `criterion` for hot-path code.
- **Eval queries** live in `docs/eval/`. The smoke corpus starts native
  EN — no friendly-query-on-foreign-seeds artefacts like v1's wave 1
  baseline (which produced an optimistically-biased 0.993 score that
  Atlas flagged as unreliable). Honesty over greenwashing.

### Documentation discipline
- Every public item: rustdoc.
- Every module: top-of-file `//!` doc explaining purpose and link to
  the relevant living-RFC section.
- When implementing a subsystem, cite the relevant RFC section in the
  module doc and (if architectural) the relevant ADR.

### Persistence format
- Internal persistence: binary throughout (`postcard` for fragments,
  `fjall` for journal with hash chain, `redb` for state store).
- External format: JSON export via a dedicated tool. NOT used for
  internal persistence.
- File extension: `.bin` for internal. No proprietary extensions.

## Specific operating rules

1. **Read the living RFC first.** When in doubt about a decision the
   docs do not cover, surface to the BDFL — do not guess. The living
   RFC is a single document; sections are labelled stable /
   experimental / deprecated.

2. **Read ADR-0001 if tempted to "bring back" something from v1.** Each
   v1 element that was dropped was dropped with reasoning, and the
   criterion for re-entry is documented per item. Check the audit trail
   first.

3. **Do not introduce new dependencies** without an ADR. The dep list
   in `Cargo.toml` is curated. Adding a dep is an architectural
   decision. Each deferred-by-omission item in `Cargo.toml` carries its
   exclusion reason inline.

4. **Hard prohibitions:**
   - Never disable state-gating "for convenience."
   - Never bypass the journal for "performance." Speed comes from
     optimising the journal, not skipping it.
   - Never `unwrap()` in library code.
   - Never let `confabulation_risk` default to 0.0 silently. Set it
     explicitly based on the subsystem's contract.
   - Never modify ADRs that are `status: accepted`. They are immutable.
     Supersede via a new ADR that explicitly links back.
   - Never invoke an LLM in the composition path. The ethics engine is
     not the composition path; see ADR-0003 for the boundary.
   - Never let an operation that can ingest, compose, or retrieve
     content bypass `ethics_evaluate`. The five-point coverage is the
     architectural contract.

5. **Anti-confabulation discipline.** When uncertain about a fact (a
   citation, a library version, an API surface, a project decision),
   check the source. If you cannot check it within reasonable effort,
   write `[unverified]` next to the claim and surface to the BDFL.
   Documented confabulation incidents from prior phases are catalogued
   in `docs/issues/` as method lessons (when the dir is created).

6. **Anti-paternalism in communication.** The BDFL has a documented
   sense of humour (dark, hyperbolic absurdism in clearly playful
   contexts). Respond to register, not surface language. Vigilance
   applies to architectural over-claim, not to general emotional
   monitoring.

## Helpful commands

```bash
# Build everything
cargo build --workspace

# Run all tests
cargo test --workspace

# Lint
cargo clippy --workspace --all-targets -- -D warnings

# Format
cargo fmt --all

# Format check (CI mode)
cargo fmt --all -- --check

# Benchmarks
cargo bench --workspace

# Build docs
cargo doc --workspace --no-deps --open

# Run with logging
RUST_LOG=hyphae=debug cargo test --workspace -- --nocapture
```

## When you are stuck

In order of preference:

1. Read the relevant section of `docs/rfc/v1-living.md`.
2. Read the relevant ADR in `docs/adr/`.
3. Read the corresponding section of the v1 CLAUDE.md
   (`../hyphae/CLAUDE.md`) to see how the same problem was framed in v1
   — but apply v2's commitments, not v1's.
4. Open a GitHub issue tagged `question` describing what you tried and
   why the docs did not answer.
5. **Do not improvise architectural decisions.** Hyphae v2 chose its
   commitments deliberately to avoid the scope creep that stalled v1.
   Improvising creates exactly the same technical debt.

## Project lead

Mario Gutiérrez. Decisions requiring BDFL approval: subsystem
additions, dependency additions, architectural pivots, license
questions, governance changes, scope expansion beyond v0.1.

---

*Last updated at v2 chartering session, 2026-05-26. The next update is
the first crate cherry-pick milestone.*
