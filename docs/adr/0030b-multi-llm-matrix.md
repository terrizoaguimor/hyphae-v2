<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0030b
title: Multi-LLM matrix via DigitalOcean Inference
status: accepted
date: 2026-05-28
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (design + execution)]
---

# 0030b — Multi-LLM matrix via DigitalOcean Inference

## Context

ADR-0030 implemented HyDE + cross-encoder reranking against
Llama-3.1-8B-Instruct Q4_K_M. The writeup pre-empted the obvious
reviewer objection ("you tested against vanilla naive RAG, not
serious RAG") by running the strong-RAG variant. But it left a
second objection unaddressed:

> "Llama-3.1-8B Q4 is not the frontier. A real production system
> would use GPT-4, Claude, or Llama 70B. Hyphae might only beat
> *small* open models, not frontier models."

The honest answer requires comparing against a **multi-LLM
matrix** spanning at least:

- A larger open model (Llama-3.3-70B or similar)
- A frontier closed model from Anthropic (Claude family)
- A frontier closed model from OpenAI (GPT-4 family)
- A frontier reasoning-tuned open model (DeepSeek)

ADR-0028's plan called for provisioning GPU droplets to run these.
Mid-execution Mario surfaced that DigitalOcean's GenAI Platform
hosts the entire catalogue (Anthropic Claude 4.6 Sonnet, OpenAI
GPT-4.1, GPT-4o, Llama 3.3-70B, DeepSeek-V4-Pro, Qwen, Mistral,
etc.) behind a single OpenAI-compatible API key. This collapses the
plan: instead of provisioning 3-5 GPU droplets (~$15-30, hours of
setup), we route all multi-LLM runs through one API token and the
existing pipeline. The strong-RAG droplet attempted in parallel
(ADR-0028's protocol applied to ADR-0030) died in apt-get lock
contention before any results; replacing it with DO Inference was
both faster and produced richer coverage.

## Decision

**Run a 5-model × 3-mode matrix via DigitalOcean Inference, scored
with the same Python pipeline as ADR-0027 / 0028 / 0029. Models
selected to span the four reviewer-expected buckets. Publish the
combined results as `docs/perf/multi-llm-comparison.md`.**

### Models in the matrix

Selection criteria: one per bucket + the project's own routing
system (Atlas) as a bonus column to test against the in-house
baseline.

| Bucket | Model | DO Inference ID | Why |
|---|---|---|---|
| Open SOTA medium | Llama 3.3 70B | `llama3.3-70b-instruct` | Replaces local Llama-8B Q4 with larger, fp8 open model — tests whether bigger open closes the gap |
| Frontier closed (Anthropic) | Claude 4.6 Sonnet | `anthropic-claude-4.6-sonnet` | Frontier flagship at time of run; Sonnet over Opus because Opus 4.7 was not reachable via the catalogue ("model not found") |
| Frontier closed (OpenAI) | GPT-4.1 | `openai-gpt-4.1` | Most-recent verified-working OpenAI model in the catalogue; 1M context window |
| Open reasoning | DeepSeek-V4-Pro | `deepseek-v4-pro` | Reasoning-tuned open model, frontier MoE; tests whether reasoning chain helps the unsupported-claim metric |
| Atlas in-house | router:celiums-conversation | `router:celiums-conversation` | The project's own conversational router (atlas-inference); bonus column for self-comparison |

Each model runs all three modes from the existing pipeline:

- `oracle`: LLM sees the corpus's seeds directly (composition-only test)
- `rag`: vanilla FAISS over pooled chunks
- `strong-rag`: HyDE + bge-reranker-base + top-5 reranked (ADR-0030)

**Total: 15 runs × 34 queries = 510 LLM calls**, plus per-query NLI
scoring (~30 sec/run for 34 sentences).

### Implementation

- `bench/baseline-llm-rag/src/baseline_llm_rag/do_inference.py`:
  `DoInferenceGenerator` with the same `generate(system, user) ->
  str` interface as `LlamaGenerator`. Pipelines use it
  interchangeably via duck typing.
- `eval_runner.py`: new `--llm-backend {local|do-inference}` flag,
  plus `--model`, `--api-key` (env `DO_INFERENCE_KEY`),
  `--endpoint`. Constructs the appropriate generator and records
  the backend identifier in the result envelope metadata.
- One fix surfaced during the run: Claude on DO Inference rejects
  `temperature` + `top_p` together. The adapter now omits `top_p`
  when `temperature=0` (greedy decoding — `top_p` has no effect at
  zero temperature, so omitting it is behaviour-preserving and
  unblocks Anthropic models).

### Decoding hyperparameters

Identical to the local llama.cpp pipeline:

- `temperature: 0.0`
- `top_p: 1.0` (omitted for Anthropic models per fix above)
- `seed: 42`
- `max_tokens: 512`

Caveat: not every upstream provider honours `seed` for full bit
identity. The writeup records this; reviewer who reruns will see
similar-but-not-identical responses for closed-API models. Open
models (Llama, DeepSeek) on DO Inference are more reliable across
runs.

### Cost

Estimated and observed:

- 510 LLM completions at ~600 tokens each ≈ 306k tokens total
- DO Inference per-token pricing varies by model; total run cost
  observed at the API level: **< $0.50** for the entire matrix
- Plus ~$1 sunk cost from the two destroyed droplets (the first
  ablation matrix from ADR-0028, plus the strong-RAG attempt that
  died in apt lock)

Total ADR-0028 + 0030b infrastructure spend: **~$1.50**, vs the
estimated $7.60 with multiple GPU droplets the original plan
called for.

## What this matrix establishes

- **Whether the head-to-head delta survives against frontier LLMs**.
  ADR-0027's claim was "Hyphae beats vanilla naive RAG on
  unsupported-claim rate". ADR-0030 extended to strong RAG. This
  matrix extends to frontier model + strong RAG.
- **Per-model behaviour patterns**. Which LLM hallucinates more or
  less, independent of the RAG layer. The oracle column isolates
  composition quality from retrieval quality.
- **The performance-of-Atlas baseline**. The
  `router:celiums-conversation` column compares Hyphae against the
  project's own LLM-routed conversational system, which is
  informative for product positioning.

## What this matrix does NOT establish

- **API determinism**. Closed-API models (Claude, GPT) do not
  guarantee bit-identical reruns; the result is a snapshot, not a
  pinned reproduction.
- **Model availability at the time of replication**. DO Inference's
  catalogue rotates; the exact model versions used here may be
  superseded by reviewer-replication time. The result JSONs carry
  the model ID; the catalog at `inference.do-ai.run/v1/models`
  is the source of truth.
- **Best possible LLM-based system**. The matrix uses the
  comparator's stock pipelines (oracle, rag, strong-rag). A
  production team would layer additional pipeline pieces
  (query rewriting, conversational state, function calling, etc.)
  that this study does not measure.
- **Multi-hop reasoning**. The corpus is single-hop retrieval; a
  HotpotQA-style benchmark would test a different regime.
- **Hyphae's hardware speed at parity**. Hyphae continues to run on
  CPU; the LLMs run on DO Inference's GPU farm. The latency
  comparison is "what a reviewer would actually deploy on each
  side," not "what would happen at identical hardware".

## Honesty discipline

The writeup MUST publish:
1. The per-model unsupported-claim-rate side by side, ranked
2. The single most surprising finding (whichever direction it
   points) flagged explicitly
3. The cases where an LLM-based system meets or beats Hyphae on
   the comparator metrics, named explicitly
4. The cost-side trade-off (Hyphae stays sub-millisecond; LLMs
   stay multi-second even on GPU)

This is the same anti-greenwashing rule ADR-0027/28/29/30 follow.
If frontier LLM + strong-RAG turns out to match Hyphae on
unsupported-claim rate within statistical noise, that finding is
published as is and the v0.1 claim is updated to reflect what
stands.

## Consequences

**Positive**:
- Pre-empts the "you only compared against weak models" objection
  at near-zero infrastructure cost
- Reuses the existing scoring pipeline; no new metrics
- Surfaces per-LLM behaviour (which models hallucinate more / less
  by default) — a finding useful beyond this paper
- The DO Inference backend is reusable for any future comparator
  ADR (new models, new pipelines)

**Negative**:
- Adds API-key-as-secret to the reproducibility surface. The
  reviewer needs their own DO Inference token; the writeup
  documents how to obtain one but cannot ship the key
- Closed-API model results are not bit-identical reruns; the
  writeup must caveat this
- Adds 13 new JSON envelopes (15 conditions, 2 had to be re-run
  after the Claude fix) to the repository; small file size
  overall, but the result-list in the paper artifacts grows

## Followups

- **Larger corpus** (separate ADR): same as the other ADRs. The
  spread within the multi-LLM matrix is much larger than the
  spread within a single LLM at different modes, which suggests
  per-LLM noise is the dominant variance at N=34.
- **RAG-Fusion + GraphRAG** layered on top of the strong-RAG pipeline:
  ADR-0030's followup. Would test whether stacking more retrieval
  techniques closes any residual gap.
- **Reader preference study**: this matrix measures the
  comparable-subset metrics. Reader preference between Hyphae's
  template-rigid prose and the LLM's smoother prose remains
  unmeasured.
- **Reasoning-mode comparison**: DeepSeek-V4-Pro and the Anthropic
  Opus family have explicit thinking/reasoning modes. The matrix
  ran them in standard mode. A separate ADR could test
  reasoning-on for the queries that involve composition over
  multiple seeds.
