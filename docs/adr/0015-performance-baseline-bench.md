<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0015
title: Performance baseline — criterion bench harness
status: accepted
date: 2026-05-27
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (v0.2 phase B review)]
---

# 0015 — Performance baseline: criterion bench harness

## Context

The chat REPL (ADR-0014) surfaces per-turn latency on every
operation: `ingest ≈ 6-8ms`, `recall ≈ 4ms`, `compose < 1ms`,
`learn ≈ 23ms / 3 proposals`. These numbers are useful as
*sanity*, but they are **single-shot** measurements with no
warmup, no iteration count, no percentile distribution, and no
sensitivity to substrate population size. A single number with
no error bar is the v1-pattern of "0.993 grammaticality" — a
claim that survives nothing.

Fase B milestone 2 from the 2026-05-27 morning roadmap called
for *baseline of performance on commodity hardware*. v0.2 needs
honest numbers: per-operation latency distributions, sensitivity
to stored-fragment count, percentile reporting. Future ADRs
(optimisation, scaling, throughput) compare against this
baseline; without it, regressions are invisible.

## Decision

**Add `crates/hyphae-bench`: a `criterion`-driven bench harness
that measures `ingest`, `recall`, and `compose` over a populated
substrate. Numbers are published as honest measurements; no
threshold gates are introduced.**

### Crate shape

`hyphae-bench` is a workspace member with one bench target
(`benches/substrate_ops.rs`). It depends on every crate it
measures, no source files beyond the bench. The crate's
`Cargo.toml` declares `[[bench]] harness = false` so Criterion's
own harness runs.

### Bench groups

Three groups in the v0.2 baseline:

1. **`ingest_at_n_stored`** — measure single `substrate.ingest`
   latency over a substrate pre-populated with N fragments,
   for `N ∈ {10, 100, 1000}`. Setup populates once; iterations
   ingest one additional fragment each (substrate state
   accumulates; the measurement is "ingest near size N", not
   "ingest at exactly N"). Acceptable v0.2 honesty caveat.

2. **`recall_at_n_stored`** — measure single
   `substrate.recall_signal` latency at the same population
   tiers. Substrate is transitioned to `State::Recall` once at
   setup; iterations reuse the cue or rotate through a small
   set so the cache (if any) is not the only thing being
   measured.

3. **`compose_at_working_set_size`** — measure single
   `realizer.realize` latency over working sets of size 1, 3,
   7. The realizer is a pure function given a working set; this
   bench isolates the surface realizer's complexity from the
   substrate's retrieval cost.

### Shared substrate, not per-iteration

Every substrate method called by the benches is `&self`
(verified: `ingest`, `recall_signal`, `realize`, `transition_to`
all take `&self`). The harness builds **one** substrate per
bench group, registers subsystems + pathways once, and shares
it across iterations. Construction is too expensive to repeat —
each one opens an `fjall` keyspace + an `redb` database + an
embedder + an ethics engine.

The trade-off: ingest mutates the store. After N iterations the
substrate carries `original_N + N` fragments. The measurement
is "ingest near population N" not "ingest at exactly N". A
future ADR can introduce iter_batched setup for stricter
isolation; v0.2 honest documentation of the shared-state caveat
is sufficient.

### What this ADR explicitly does **not** do

- **Does not** set performance thresholds. Numbers are
  measurement, not contract. ADR-0008's discipline (honest
  scores without target thresholds) applies here verbatim.
- **Does not** add CI gates. The bench is run on demand
  (`cargo bench -p hyphae-bench`); future regression-tracking
  ADR introduces CI plumbing with concrete tolerance design.
- **Does not** optimise anything. v0.2 baseline is descriptive,
  not prescriptive. The discovery of slow paths feeds future
  optimisation ADRs.
- **Does not** measure multi-threaded scaling. Single-threaded
  baseline only — multi-threaded behaviour requires a separate
  ADR with concrete contention model.
- **Does not** measure persistence overhead. The bench uses a
  `tempdir`; comparing in-memory vs on-disk requires a separate
  ADR with bind-mounted tmpfs or RAM-disk methodology.

### Reporting discipline

Criterion produces:

- Mean, median, standard deviation, slope per bench.
- Throughput in operations/second when applicable.
- HTML reports under `target/criterion/` (gitignored).

The integrator (Mario) reads numbers, judges health, decides
whether to optimise. The v0.2 baseline is committed to memory
via `memory.celiums.ai` so future sessions can compare against
the recorded snapshot.

## Sources

- **Roadmap, 2026-05-27 morning session** — "Fase B milestone
  2: performance baseline."
- **ADR-0008 §"No target thresholds"** — the
  honest-measurement-without-thresholds discipline this ADR
  carries forward to the latency axis.
- **`hyphae_substrate::Substrate`** — the type under
  measurement.
- **`criterion` crate (workspace dep, line 83)** — already
  declared in `Cargo.toml`; the harness this ADR activates.

## Consequences

- v0.2 publishes its first quantitative baseline. Numbers are
  reproducible (`cargo bench -p hyphae-bench`) and have
  percentile distributions, not just single-shot guesses.
- One new workspace member. No new runtime dependencies;
  `criterion` was reserved at v0.1 chartering for exactly this
  use.
- The `target/criterion/` HTML reports stay local (workspace
  `.gitignore` already covers `target/`). The committed
  artefact is the bench source + this ADR + the memory entry
  recording the first run's numbers.
- Future optimisation ADRs (cascade hash-graph, episodic
  pruning, embedding precompute) gain a concrete "before/
  after" baseline.

## Cross-references

- **ADR-0008** — honest evaluation without target thresholds.
- **ADR-0014** — the chat REPL whose single-shot timings this
  bench replaces with rigorous measurement.
- **RFC §1 "Hard Architectural Commitments"** — performance
  measurement does not change the immutable substrate; the
  bench is read-only against the public API.
