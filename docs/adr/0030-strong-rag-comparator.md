<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0030
title: Strong-RAG comparator — HyDE + cross-encoder reranking
status: accepted
date: 2026-05-28
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (design + execution)]
---

# 0030 — Strong-RAG comparator

## Context

ADR-0027's head-to-head established that Hyphae beats **vanilla
RAG** (chunk + MiniLM + FAISS top-k + Llama-3.1-8B-Instruct) by
40–55% on `unsupported_claim_rate_filtered` and ~48,000–654,000×
on latency. The writeup itself flagged the obvious objection:

> "Stronger pipelines (HyDE, RAG-Fusion, GraphRAG, MemGPT, query
> rewriting) could close the unsupported-claim gap."
> — `baseline-comparison.md` §"What this comparison does NOT establish"

A reviewer reading the paper will ask: **is the delta architectural,
or did Hyphae just beat a weak comparator?** The honest answer
requires re-running with a stronger retrieval pipeline. If a strong
RAG closes the gap, the v0.1 claim narrows. If it does not, the
claim hardens.

The literature converges on a small set of techniques that
"serious" RAG deployments use after the naive chunk-embed-retrieve
pattern stops being adequate:

- **HyDE** (Hypothetical Document Embeddings, Gao et al. 2022) —
  generate a hypothetical answer with the LLM and embed *that*
  instead of the query. The hypothetical document tends to share
  surface vocabulary with the true relevant passages even when the
  raw query does not.
- **Cross-encoder reranking** (Nogueira & Cho 2019; Reimers &
  Gurevych 2019) — over-retrieve with the embedder, then re-score
  candidates with a cross-encoder that scores `(query, candidate)`
  pairs directly. Slower per candidate but substantially more
  accurate at picking the best top-k.
- **RAG-Fusion** (Rackauckas 2024) — generate N query rewrites,
  retrieve per rewrite, fuse with reciprocal rank fusion. Strong
  but expensive (N extra LLM calls per query).
- **GraphRAG** (Microsoft Research, 2024) — offline graph
  construction of the corpus, online retrieval traversing the
  graph. Heavyweight to set up; pays off at corpus scale far above
  v0.1.

For v0.1, **HyDE + cross-encoder reranking** is the canonical
"serious RAG" stack: both techniques are widely cited, both run on
CPU, and their combination is what a production team building on
top of an LLM would reach for first.

## Decision

**Add `strong-rag` as a third mode to the comparator
(`bench/baseline-llm-rag/`), alongside `oracle` and `rag`. Implement
HyDE generation + cross-encoder reranking using the existing
LlamaGenerator + a new `BAAI/bge-reranker-base` cross-encoder. Run
on both laptop and droplet hardware (matching ADR-0028's matrix).
Publish results as `docs/perf/strong-rag-comparison.md`.**

### Pipeline — exact algorithm

```
def strong_rag(query, faiss_index, embedder, llm, reranker):
    # Step 1: HyDE — LLM-generated hypothetical answer
    hyde_prompt = "Write a brief, plausible answer to: " + query
    hyde_answer = llm.generate(system=HYDE_SYSTEM, user=hyde_prompt)

    # Step 2: Embed HyDE answer (not the query) and retrieve top-20
    hyde_embedding = embedder.encode([hyde_answer])
    candidates = faiss_index.search(hyde_embedding, k=20)

    # Step 3: Cross-encoder rerank (query, candidate) pairs
    pairs = [(query, candidate.chunk) for candidate in candidates]
    scores = reranker.predict(pairs)
    ranked = sorted(zip(candidates, scores), key=lambda x: -x[1])
    top_5 = [c for c, _ in ranked[:5]]

    # Step 4: Final generation with the (better) retrieved context
    response = llm.generate_with_context(query, top_5)
    return response
```

### Component choices

- **HyDE prompt**: kept minimal to avoid leaking task-specific
  hints. The system message is the same one the vanilla RAG uses
  ("answer using only the provided context"); the HyDE user
  message asks for a brief plausible paragraph. Decoding
  parameters identical to the final generation (seed=42, temp=0).
- **Cross-encoder model**: `BAAI/bge-reranker-base` — ~278 MB,
  cross-encoder over English, top of the MTEB reranking benchmark
  for its parameter class as of late 2024. Runs on CPU at
  ~50–200 ms per 20-candidate batch.
- **Over-retrieve k**: 20 candidates from FAISS, then rerank to
  top-5. Matches the depth a production team would use; cheap
  enough to run at v0.1 corpus size.
- **Final top-k**: 5, identical to the vanilla `rag` mode. The
  comparator measures the effect of *retrieval quality at the same
  context size*, not the effect of more context. A wider final
  context would conflate two axes.
- **Decoding for the final generation**: identical to vanilla
  (seed=42, temp=0.0, top_p=1.0). The only thing varying between
  vanilla `rag` and `strong-rag` is the retrieved chunks.

### Hypothesis under test

The paper's null hypothesis after the head-to-head is:

> **H0**: Hyphae's lower `unsupported_claim_rate_filtered` (0.219 vs
> vanilla LLM 0.367–0.490) is **architectural** — verbatim
> quotation by construction prevents unsupported claims that
> paraphrase-by-construction systems inevitably produce.

The alternative hypothesis a reviewer would raise is:

> **H1**: The gap is **retrieval-quality**. A stronger retrieval
> pipeline gives the LLM better context, the LLM hallucinates less,
> and the gap closes.

This comparator's job is to **try to refute H0**. If `strong-rag`
brings `unsupported_claim_rate_filtered` down to ≤ 0.30 (within
striking distance of Hyphae's 0.219), H0 weakens. If `strong-rag`
stays in the 0.35+ range like vanilla `rag`, H0 hardens — better
retrieval is not the bottleneck.

### Scope — what is in vs out

**In**:
- HyDE generation
- Cross-encoder reranking with `bge-reranker-base`
- Combination of the two
- Run on both laptop and droplet (matching ADR-0028)

**Out** (deferred to separate ADRs):
- **RAG-Fusion**: N query rewrites + RRF. Strong but expensive;
  ADR-0030b (hypothetical) could add it as a third column.
- **GraphRAG**: offline graph construction. Heavyweight; pays off
  at corpus scale far above 34 queries. Would need a corpus
  expansion first.
- **ColBERT / token-level embeddings**: substantial storage and
  compute cost.
- **Self-RAG**: requires fine-tuning the LLM. Out of v0.1 scope.
- **Stronger LLM** (GPT-4, Claude, etc.): different axis from
  retrieval-quality. Would conflate model capacity and retrieval
  pipeline.

### Predicted effects

Recorded **before** running. Writeup contrasts with observed.

| Metric | Predicted vanilla rag → strong-rag |
|---|---|
| `verbatim_pass_rate` | unchanged — LLM still paraphrases |
| `connective_hygiene_pass_rate` | unchanged |
| `quoted_content_supported_rate` | unchanged (LLM rarely quotes) |
| `ngram_overlap_4` | slight rise — better chunks means LLM has more relevant tokens to lift |
| `unsupported_claim_rate_filtered` | **fall** if H1 true (better retrieval → less hallucination). Magnitude is the test. Predicted: 0.49 → 0.30–0.40 if H1 partially true; 0.49 → 0.40+ if H0 holds |
| `unsupported_claim_rate_raw` | similar to filtered |
| `latency_mean` | **rise** — adds 1 LLM call (HyDE) + 1 cross-encoder batch (rerank) per query. Predicted: vanilla 4658 ms (laptop rag) → strong-rag ~6500–8000 ms |
| `latency_p95` | rise proportionally |

The latency rise is **part of the comparison story** — strong RAG
costs more compute. The hardware matrix will surface this on the
droplet where the LLM cost is already high.

## What this comparison establishes

- Whether Hyphae's `unsupported_claim_rate_filtered` advantage
  survives against the canonical "serious RAG" stack a production
  team would actually deploy.
- The compute cost premium of strong RAG vs vanilla RAG —
  whether the quality gain (if any) justifies the additional LLM
  call and reranker pass.
- Per-system production economics: the strong-RAG column adds the
  most realistic latency number for the kind of pipeline that
  reviewers will compare against.

## What this comparison does NOT establish

- **Hyphae's advantage against every possible RAG pipeline.**
  HyDE + reranker is "serious" but not "exhaustive". GraphRAG,
  RAG-Fusion, Self-RAG, ColBERT may close the gap that HyDE +
  reranker does not. Each is a separate ADR.
- **The optimum HyDE prompt.** Prompt engineering of HyDE is its
  own research direction; the comparator uses a minimal prompt
  to avoid over-tuning. A reviewer's objection that "a better HyDE
  prompt would close more of the gap" stands until ADR-0030b
  measures it.
- **The strong-RAG result with a larger context window.** This
  comparator keeps top-k=5 to match vanilla; a real production
  deployment might use top-k=10 or 20. That is a context-budget
  study, not a retrieval-quality study.
- **The reranker on languages other than English.** Same EN-only
  caveat ADR-0027 inherits.

## Honesty discipline

Same rule ADR-0027/0028/0029 follow: the writeup reports
predicted-vs-observed deltas per metric, flags surprises, and
publishes the result whether it strengthens or weakens the v0.1
claim. The whole point of ADR-0030 is to try to refute H0; if H0
falls, the paper claim is updated to reflect what stands.

## Consequences

**Positive**:
- The paper claim acquires depth against the strongest comparator
  v0.1 can run. Reviewer objection "you compared against weak RAG"
  is pre-empted.
- The `BAAI/bge-reranker-base` reranker is reusable for any future
  comparator (RAG-Fusion, multi-hop, etc.) — lives in the same
  module as the other components.

**Negative**:
- Adds `bge-reranker-base` (~278 MB) to the reproducibility
  surface. Reviewer download cost rises.
- Strong-RAG runs ~2× the LLM calls of vanilla — wall-clock for the
  experiment doubles (the LLM is the bottleneck).
- Two more JSON envelopes per hardware (laptop + droplet) — small
  but adds to repo size.

## Followups

- **ADR-0030b** (hypothetical): RAG-Fusion as a fourth column.
  Establishes whether multi-query retrieval helps where HyDE +
  reranker did not.
- **GraphRAG study** (separate ADR): requires corpus expansion +
  offline graph construction. Pays off at larger scale.
- **Larger top-k context-budget study** (separate ADR):
  independent of retrieval quality, measures how much extra context
  the LLM can usefully consume.
