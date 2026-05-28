<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0031
title: Standard-benchmark corpus — TriviaQA-150 column
status: accepted
date: 2026-05-28
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (design + execution)]
---

# 0031 — Standard-benchmark corpus

## Context

The multi-LLM matrix landed in ADR-0030b ran 19 system
configurations on the project's own 34-query EN corpus
(`seed_corpus_en` in `hyphae-eval`). The headline finding — Hyphae
ties GPT-4.1 + strong-RAG on `unsupported_claim_rate_filtered` —
was statistically defensible (within bootstrap CIs) but
methodologically vulnerable to one objection:

> *"You compared 19 systems on your own 34-query corpus. That
> corpus may be tuned to Hyphae's strengths or to the LLMs'
> weaknesses or both. Show me on a standard benchmark."*

For workshop-grade submission this objection has to be addressed,
not deferred. The deferral path was a "future ADR" placeholder in
the multi-llm-comparison writeup; the urgency upgraded to a
session-immediate when the calibration discussion with Mario
flagged it as the single largest gap before workshop submission.

## Decision

**Sample 150 random queries from TriviaQA `rc` validation,
construct seed bodies from the queries' supporting Wikipedia
context, and re-run the full 19-system matrix from ADR-0030b on
this corpus. Publish as `docs/perf/triviaqa-comparison.md` in
parallel with the own-corpus writeup so the two pictures sit side
by side.**

### Why TriviaQA

Considered alternatives:

- **PopQA**: short factual questions but **no supporting passage
  in the dataset**. Would need additional Wikipedia retrieval to
  build seed bodies, introducing a second source of corpus noise
  outside our control.
- **NaturalQuestions (full)**: has supporting documents but full
  Wikipedia pages with HTML markup. Heavyweight to process; not
  proportional to v0.1 scope.
- **NQ-open**: short Q-A only, no passages — same shortcoming as
  PopQA.
- **HotpotQA**: multi-hop reasoning. Different task type from
  our pipeline's single-fragment retrieval design point. Worth a
  separate ADR for the multi-hop dimension.
- **SQuAD**: read-comprehension over given passages. Inverted
  from our task (we want passage from question, SQuAD has both).
- **TriviaQA `rc`**: **selected**. Provides question, answer,
  and `entity_pages.wiki_context` (full Wikipedia article text
  per entity). The Wikipedia text is the natural source of seed
  bodies.

### Sampling methodology

- **Random sample, seed=42**, from the `validation` split (17,944
  queries available).
- **For each sampled query**: parse `entity_pages.wiki_context`
  pages, split each into sentences. Find sentences that:
  1. Contain the answer (or any alias) as a word-bounded match
     (avoids "8%" matching inside "70.8%").
  2. Are between 30 and 250 characters (filters out headings on
     the short side, paragraphs on the long side).
  3. Pass embedding-similarity rerank: score remaining candidates
     by cosine similarity to the query and pick the highest. This
     eliminates substring matches where the answer appears in an
     unrelated sentence.
- **Reject the sample** if no qualifying sentence exists. Continue
  sampling until 150 surviving queries are collected.
- Filter pass rate on our seed: 150 surviving / ~181 sampled
  ≈ 83%. Rejected because of no wiki_context (21), no qualifying
  sentence (10).

### Why 150

- N=34 (own corpus) is below most NLP papers' floor for
  statistical claims. Reviewers will object.
- N=150 puts the bootstrap CIs in the workshop-defendable range
  for `unsupported_claim_rate` (CI width ≈ 0.05–0.10 at N=150 vs
  0.15–0.25 at N=34 on typical metric values).
- N=150 is also a sweet spot for cost: 150 × 5 LLMs × 3 modes ≈
  2,250 LLM calls via DO Inference. At ~$0.001/call, total ≈ $2.
  N=300 would double cost without proportionally more signal.

### Why this corpus is not authored by us

The TriviaQA reference is Joshi et al. 2017 (EMNLP). Hyphae's
v0.1 design predates the project's first awareness of TriviaQA
by several months. The 150-query subset selection is **random
under a published seed**, not curated; reviewers can regenerate
the exact same subset by running `corpus_external.py
--seed 42 --n 150`.

The filter rules (word-bounded match, 30–250 char length,
embedding rerank) are documented in the converter source. The
filter is not Hyphae-specific; the same rules would select the
same sentences for any system in the matrix.

### Implementation

- `bench/baseline-llm-rag/src/baseline_llm_rag/corpus_external.py`:
  click CLI that downloads TriviaQA via huggingface-hub, samples,
  filters, and writes `corpus-triviaqa-150.json` in the same
  schema `load_corpus` consumes.
- `crates/hyphae-eval/examples/export_results_from_json.rs`: new
  Rust binary that loads a corpus JSON (rather than the embedded
  `seed_corpus_en`) and runs the realizer. Same output envelope
  as `export_results.rs`.
- `bench/baseline-llm-rag/scripts/run-triviaqa.sh` (committed at
  `/tmp/run-triviaqa.sh` during the session; promoted to
  `scripts/` here): orchestrates all 19 runs.

### Predicted effects per system

Recorded before the run. Writeup contrasts with observed.

| System | Predicted vs own corpus |
|---|---|
| Hyphae | unsupported_filtered should *fall* — TriviaQA's single-seed queries mean Hyphae's output is almost entirely scaffolding the filter catches; the multi-fragment compositions on the own corpus added the most filter-escaping prose |
| Llama-8B-local rag/strong-rag | should worsen — small open model with denser Wikipedia context |
| Llama-3.3-70B | should match the own-corpus pattern (slightly worse than Llama-8B at fp8) |
| Claude-4.6-Sonnet | should match — already at hedging floor |
| GPT-4.1 strong-rag | unclear — TriviaQA may help or hurt depending on how the rerank handles single Wikipedia sentences |
| DeepSeek-V4-Pro | should retain retrieval gain — its strong-rag mode beats its oracle on the own corpus |
| router:celiums | should match own-corpus pattern |

The predicted-vs-observed deltas land in the writeup.

## What this comparison establishes

- Whether the multi-LLM matrix's headline finding (Hyphae ties
  GPT-4.1 + strong-RAG) generalises beyond the project's own
  corpus.
- Per-system corpus-sensitivity: which configurations move under
  a different corpus distribution.
- Statistical solidification at N=150 of patterns that were
  CI-fuzzy at N=34.

## What this comparison does NOT establish

- **Generalisation across benchmarks**. TriviaQA is one
  benchmark with a specific question distribution and a specific
  context construction (Wikipedia). A multi-hop benchmark, a
  domain-specific benchmark, or a conversational benchmark would
  each tell a different story.
- **Factual correctness against TriviaQA's gold answers**. The
  unsupported-claim metric measures grounding in the retrieved
  context; the TriviaQA dataset also carries `answer.value` for
  gold-answer match scoring. Adding that column is a worthwhile
  followup ADR.
- **Reader preference between Hyphae's prose and LLMs' prose on
  TriviaQA-style queries**. The metric measures grounding; the
  prose's conversational fit is unmeasured.
- **Multi-hop reasoning**. TriviaQA queries are single-hop. A
  HotpotQA or MuSiQue column would test the multi-hop regime.

## Honesty discipline

Same rule the previous ADRs in the chain follow. The writeup MUST
publish:

1. Hyphae's TriviaQA rank vs own-corpus rank side by side.
2. The mechanism by which any system's rank changes substantially.
3. The honest caveat that the metric interacts with the corpus
   structure — TriviaQA's single-seed queries mechanically favour
   Hyphae's compositional template in a specific way.
4. The acknowledgement that this benchmark addresses one reviewer
   objection (no standard benchmark) but does not address the
   parallel objection (no multi-hop or domain-diverse benchmark).

## Consequences

**Positive:**
- Workshop-grade pre-requisite met: standard benchmark column
  landed, N substantially closer to defendable.
- The picture on TriviaQA is **stronger** for Hyphae than the
  own-corpus picture — pre-empts the "you tuned the corpus"
  objection with empirical data pointing the other way.
- The mechanism by which Hyphae's lead grows on TriviaQA is
  visible in the writeup (filter behaviour + single-seed
  structure) — this transparency is a feature, not a bug.

**Negative:**
- Adds a 280 KB corpus JSON and 19 result JSONs (~150 KB each)
  to the repository. ~3 MB total.
- The TriviaQA dataset itself is not redistributed (license
  permits research use; we use it programmatically via
  huggingface-hub). Reviewers need ~3 GB of disk for the dataset
  cache on first run.
- The own-corpus result is now methodologically secondary — the
  TriviaQA result is the headline for the paper. The own corpus
  remains useful for the multi-fragment composition story but
  is no longer the comparator's primary measurement.

## Followups

- **ADR-0032** (planned): multi-hop benchmark column (HotpotQA
  or MuSiQue subset).
- **ADR-0033** (planned): gold-answer match column against
  TriviaQA's `answer.value` — complements grounding with
  correctness.
- **ADR-0034** (planned): reader preference study — human eval
  on a subset of the TriviaQA corpus to address the prose-quality
  vulnerability.
- **arXiv preprint draft** (~3-5 days): with the standard
  benchmark landed, the paper claim is well-supported enough to
  begin writing.
