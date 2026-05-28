<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0028
title: Hardware matrix — laptop (Mac MPS) vs server-class CPU x86_64
status: accepted
date: 2026-05-28
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (design + execution)]
---

# 0028 — Hardware matrix

## Context

ADR-0027's head-to-head and ADR-0029's ablation sweep both ran on a
single hardware configuration: laptop, Apple Silicon (arm64),
10-core M-series, MPS (Metal) backend for both Llama and the NLI
scorer. Both writeups explicitly flagged this as an inferential
limit:

> "N=1 hardware (laptop, Apple Silicon, MPS). A server-class run
> could compress the latency gap; it could also widen it because
> the LLM benefits from batching that this single-query comparator
> does not exploit."
> — `baseline-comparison.md` §"What this comparison does NOT establish"

A second hardware configuration is needed to test whether the
comparable-subset metrics are **architectural** (invariant under
hardware change) or **hardware-conditional** (a happenstance of the
laptop's MPS speedup). The choice of the second machine matters: it
should be **representative of where this code would actually run in
production** for a reviewer comparing options. The standard cloud
deployment target is `Linux x86_64 CPU-only`. Almost no production
Llama deployment runs on Apple Silicon; almost every production
deployment runs on x86_64 Linux servers, and most do NOT have a
dedicated GPU per node.

## Decision

**Provision a single DigitalOcean `c-16` droplet (16 vCPU dedicated
Intel Xeon Platinum 8168 @ 2.7 GHz, 31 GB RAM, NYC1, Ubuntu 24.04),
re-run the full 7-condition matrix (5 Hyphae ablations + LLM oracle
+ LLM rag) on the same 34-query EN corpus, and publish the
laptop-vs-droplet comparison in
`docs/perf/hardware-matrix.md`.** Destroy the droplet after pulling
results.

### Hardware choice rationale

- **`c-16` AMD EPYC / Intel Xeon "CPU-Optimized" pool**: $0.50/hour,
  dedicated vCPU (not shared like the standard pool). 16 cores is
  representative of mid-sized cloud production nodes; the Intel
  Xeon Platinum 8168 is a common Skylake-SP server CPU. The pool
  assignment is opaque (DO doesn't guarantee AMD vs Intel) — the
  droplet we received ran Intel; the writeup records the
  `/proc/cpuinfo` model name for reproducibility.
- **NYC1**: the region where the project's other infrastructure
  already runs; uses available DC capacity.
- **Ubuntu 24.04 LTS**: long-term support, the same Linux base most
  reviewers will reach for first.
- **CPU-only (no GPU)**: most cloud nodes do not have a per-instance
  GPU. The expected effect is to dramatically slow the LLM
  baseline (no Metal/CUDA acceleration) and to leave Hyphae
  largely unaffected (it runs CPU-only by design). This is the
  most-likely-to-vary axis between hardware configs; flipping it
  is the test.

### Scope — what is in the matrix

Same conditions as ADR-0029, run unchanged:

- A0 full / A1 no-shape / A2 no-ethics / A3 minimal-lex /
  A4 no-smoothing (Hyphae)
- LLM oracle (vanilla seed-as-context mode)
- LLM rag (FAISS top-k retrieval)

Each condition produces a JSON envelope identical in shape to the
laptop run, with a `c16-do-xeon` hardware tag in the filename.

### Why not multiple droplets

- A second cloud configuration would add cost without
  proportionally more signal on v0.1. The contrast between
  `Mac M-series + MPS` and `Linux x86_64 + CPU` is already the
  largest one most reviewers care about.
- A third configuration (server-class CPU **plus** consumer-grade
  Nvidia GPU) is a separate decision — covered by a hypothetical
  ADR-0028b once the v0.1 paper is complete.
- A multi-droplet sweep at larger scale (e.g. 100 queries × 8
  droplets) is deferred to corpus-expansion work.

### Predicted effect per metric

These were recorded *before* running. The writeup contrasts them
with what was observed.

| Metric | Predicted droplet vs laptop |
|---|---|
| `verbatim_pass_rate` (every condition) | unchanged — quotation is deterministic |
| `connective_hygiene_pass_rate` | unchanged — output is deterministic |
| `quoted_content_supported_rate` | unchanged — extraction is deterministic |
| `ngram_overlap_4/5/8` | unchanged — same response text |
| `unsupported_claim_rate` (filtered + raw) | small NLI floating-point drift across architectures; direction preserved |
| `latency_mean` (Hyphae) | similar or slightly faster (16-core dedicated Xeon vs M-series shared compute) |
| `latency_mean` (LLM) | **slower** — no Metal acceleration; CPU-only x86_64 inference |

### Honesty discipline

The writeup MUST report the predicted-vs-observed delta per metric
per system. Surprises get flagged, not smoothed. Same rule
ADR-0027 / ADR-0029 follow.

## What this comparison establishes

- Whether Hyphae's comparable-subset metrics are robust to
  hardware change — separating *architectural* properties from
  *MPS-specific* ones.
- Whether the LLM baseline's latency degrades catastrophically
  without GPU acceleration, and by how much. (This drives the
  realistic deployment economics: a Hyphae-based system can run
  on any cheap CPU node; a vanilla-RAG system depends on GPU.)
- The Hyphae:LLM latency ratio under CPU-only conditions — the
  paper's headline claim becomes whether the gap is "thousands of
  times" or "millions of times" once the LLM's hardware
  advantage is removed.

## What this comparison does NOT establish

- **GPU-equipped server class**. CUDA Llama inference at server
  scale is dramatically faster than the CPU run here; this matrix
  does not measure that regime. A separate ADR would.
- **Batching effects**. The comparator issues one query at a time.
  LLM batching could amortize the per-query latency by 5–10×
  in throughput-oriented production; the matrix here measures
  the *latency*, not the *throughput*.
- **Memory pressure at scale**. The Llama 8B Q4 model fits
  comfortably in 31 GB; production deployments that load multiple
  models or serve concurrent users hit different bottlenecks.
- **Generalisation across cloud providers**. The matrix is
  DigitalOcean-specific; AWS / GCP / Azure equivalents (EC2 c6i,
  GCE C2, Azure Fsv2) would produce comparable but not identical
  numbers.
- **Hardware-conditional NLI drift**. The unsupported-claim rate
  shows tiny deltas across architectures; this is NLI-model
  numerical noise. A separate ADR could pin a deterministic NLI
  implementation (CPU-only torch, single-thread, fixed seed) if
  the drift becomes load-bearing.

## Consequences

**Positive:**
- The paper claim acquires hardware resolution. Reviewers see that
  the comparable-subset metrics survive a hardware shift.
- The Hyphae:LLM latency ratio under realistic CPU-only conditions
  becomes a published number, not a laptop quirk.
- The infrastructure cost (~$1) is recorded in the writeup, so
  reviewers can reproduce the run at the same cost.

**Negative:**
- One additional hardware configuration is not enough to extrapolate
  to "all hardware". The matrix is small.
- The droplet is destroyed after the run, so future reproductions
  require fresh provisioning. Not a regression — `doctl` invocation
  + `bash run-bench-remote.sh` is the same.
- DO's c-16 pool allocates Intel **or** AMD CPUs opaquely; the
  exact CPU model is recorded in the result JSON but not pinned
  across reruns.

## Followups

- **ADR-0028b** (future): GPU server class — re-run with a
  CUDA-equipped instance. Establishes whether Hyphae's lead
  survives when the LLM has its hardware advantage.
- **Throughput axis** (separate ADR): introduce batching to the LLM
  pipeline and re-measure. The latency vs throughput trade-off is
  load-bearing for production economics.
- **Multi-region matrix** (separate ADR): the same condition matrix
  on AWS / GCP / Azure equivalents to test cloud-provider
  invariance.
