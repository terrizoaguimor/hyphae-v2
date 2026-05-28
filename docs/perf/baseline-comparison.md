<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

# Baseline comparison — Hyphae v2 vs vanilla LLM+RAG

> **Status — 2026-05-28**. First head-to-head between Hyphae's
> realizer and a vanilla LLM+RAG pipeline (Llama-3.1-8B-Instruct +
> MiniLM-L6 + FAISS). All numbers and the runnable artifacts that
> produced them are committed; the design choices and limits of the
> comparison are codified in
> [`../adr/0027-baseline-llm-rag-comparator.md`](../adr/0027-baseline-llm-rag-comparator.md).
> **Read ADR-0027 first** — it explains why only a small subset of
> metrics is directly comparable and what each column does and does
> not establish.

## TL;DR — five lines

1. **Hyphae cites the substrate verbatim in 100% of queries.** The
   LLM cites verbatim in 15–18%; the rest is paraphrased even when
   it receives the exact same context.
2. **Hyphae's unsupported-claim rate (NLI-filtered) is 0.22** vs the
   LLM at 0.37 (oracle context) and 0.49 (full RAG retrieval). On
   the metric that proxies hallucination, Hyphae is **40–55%
   better** than the baseline at the same context quality.
3. **Hyphae runs ~48,000× faster** than the LLM baseline (0.048 ms
   vs 2,299 ms mean per query). The substrate is CPU-only, 50 MB
   RSS, sub-millisecond. The baseline requires a 4.6 GB model and
   ~5 seconds at p95.
4. **The LLM wins on `ngram_overlap_8`** (0.33 vs Hyphae's 0.24).
   Honest reading: Hyphae's connective tissue ("Per the recorded
   fragments,", "Per the next fragment,") breaks 8-token windows
   around each verbatim quote, while the LLM happens to reproduce
   long verbatim runs when paraphrasing simple seed bodies.
5. **This comparison does not establish that Hyphae is preferable to
   readers.** Reader preference is a separate study. What it
   establishes is the architectural claim: verbatim quotation is a
   measurable property, unsupported claims drop, and the cost of
   composition collapses by four orders of magnitude.

## Setup

| Component | Value |
|---|---|
| Hyphae version | v0.1 + ADRs 0011, 0013–0026 (commit `9dfb67d` + working tree) |
| Comparator version | `bench/baseline-llm-rag/` v0.1.0 (this repo, ADR-0027) |
| LLM | `meta-llama/Llama-3.1-8B-Instruct` GGUF Q4_K_M (~4.6 GB) |
| LLM runtime | `llama-cpp-python` 0.3.23, seed=42, temp=0, top_p=1.0 |
| Embedder | `sentence-transformers/all-MiniLM-L6-v2` |
| Vector index | FAISS `IndexFlatIP` (exact, cosine after L2 normalise) |
| Chunking | 256-token chunks, 32 overlap, `tiktoken.cl100k_base` |
| Retrieval `k` | 5 (`rag` mode only) |
| NLI scorer | `roberta-large-mnli` |
| Corpus | EN baseline, 34 queries — exported from `crates/hyphae-eval/src/corpus.rs::seed_corpus_en` (SHA-256 stored in the per-run JSON envelope) |
| Hardware | Apple Silicon, arm64, 10 cores; macOS Darwin 25.4.0; Metal (MPS) backend for both Llama and roberta-mnli |
| Wall clock | Hyphae: ~3 s incl. NLI scoring. LLM oracle: 1 m 26 s. LLM RAG: 2 m 57 s. |

## Comparator results — the head-to-head table

Per ADR-0027 §"Comparable subset", these are the dimensions that
mean the same thing for both architectures. The seven Hyphae-
specific dimensions (schema fidelity, limitation triggers, lexicon
coverage, etc.) are not in this table — they live in
[`v0.2-baseline.md`](v0.2-baseline.md) and the harness's own
`EvalReport`.

| Metric | Hyphae | LLM (oracle) | LLM (RAG) | Notes |
|---|---:|---:|---:|---|
| `verbatim_pass_rate` | **1.000** | 0.176 | 0.147 | Per-query 0/1 — does every seed body appear verbatim in the response? |
| `connective_hygiene_pass_rate` | **1.000** | **1.000** | **1.000** | Tie — none of the three produced doubled connectives. |
| `quoted_content_supported_rate` | **1.000** (n=32) | 1.000 (n=1) | 1.000 (n=2) | When the system *does* use formal quotation, every quoted span verbatim from context. The denominator difference is the story: Hyphae quotes in 32/34 queries; the LLM almost never uses quotes. |
| `ngram_overlap_4` (mean) | 0.466 | 0.458 | 0.448 | Indistinguishable — at n=4 both systems have similar token-level fidelity. |
| `ngram_overlap_5` (mean) | 0.416 | 0.419 | 0.414 | Indistinguishable. |
| `ngram_overlap_8` (mean) | 0.240 | **0.329** | **0.329** | **LLM wins.** Hyphae's connective tissue breaks 8-token windows around quotes; the LLM reproduces longer verbatim runs when paraphrasing simple seeds. |
| `unsupported_claim_rate` (filtered) | **0.219** | 0.367 | 0.490 | Hyphae 40–55% better. NLI applied per sentence, Hyphae's connective-only sentences excluded from denominator. |
| `unsupported_claim_rate` (raw) | 0.625 | **0.376** | 0.498 | **LLM wins on raw.** Hyphae's connective sentences confuse NLI; without the filter they count as "claims" and pull the rate up. See §"On the filtered vs raw asymmetry". |
| `latency_p50` (ms) | **0.042** | 1,714 | 3,379 | ~40,000× faster at the median. |
| `latency_p95` (ms) | **0.098** | 5,113 | 11,148 | ~52,000× faster at p95. |
| `latency_mean` (ms) | **0.048** | 2,299 | 4,658 | ~48,000× faster. |

Confidence intervals (bootstrap 95% percentile, 1000 resamples) are
recorded per-metric in the JSON envelopes under each run's
`aggregate` block. Sample sizes are 34 queries (the EN corpus).

## On the filtered vs raw asymmetry

This is the table cell that deserves the most honesty.

`unsupported_claim_rate` is implemented as: split the response into
sentences, score each with NLI (`roberta-large-mnli`) against the
retrieved context, count `neutral` or `contradiction` labels as
unsupported. The **filtered** variant excludes sentences that start
with a known connective phrase; the **raw** variant counts them.

Hyphae's composition is **quote + connective glue + quote +
connective glue + … + closing**. Sentences like "Per the recorded
fragments," and "That is the substrate's current view." are not
factual claims — they are scaffolding. But to NLI they look like
text whose entailment relationship to the context is `neutral`,
which counts as "unsupported" in the raw rate. The connective
filter restores the denominator to actual claims.

Two readings of this:

- **Conservative**: report the **raw** rate. Hyphae (0.625) is
  *worse* than the LLM (0.376–0.498). The architecture's
  connective-tissue overhead inflates the unsupported count.
- **Architecture-aware**: report the **filtered** rate. Hyphae
  (0.219) is *better* than the LLM (0.367–0.490). The connective
  scaffolding is not a claim; it is composition glue.

This document publishes **both**. The connective filter list is
hard-coded in
[`bench/baseline-llm-rag/src/baseline_llm_rag/metrics_extra.py`](../../bench/baseline-llm-rag/src/baseline_llm_rag/metrics_extra.py)
and reviewers can read it; the writeup does not silently choose for
the reader. The author's read: **filtered is the more honest
comparator** because the raw number penalises Hyphae for being a
verbatim-quotation system at all — it counts the literal mechanism
of quotation as "unsupported claims." But that is an interpretation,
not the data.

The `quoted_content_supported_rate` row sidesteps this asymmetry
entirely. It measures the architectural claim directly: when the
response uses formal quotation, does every quoted span appear in the
retrieved context? Hyphae: 1.000 on the 32/34 queries with quotes.
The LLM: 1.000 in the 1–2 queries where it bothered to use
quotation. The interesting number is not the rate but the
denominator — Hyphae *uses quotation* as its compositional primitive;
the LLM almost never does.

## On the ngram_overlap_8 inversion

Hyphae loses to the LLM on 8-gram overlap (0.240 vs 0.329). This is
counterintuitive — Hyphae quotes verbatim, so its overlap should be
higher, not lower.

The mechanism: Hyphae's composition is

```
Per the recorded fragments, "quote 1" Per the next fragment,
"quote 2" That is the substrate's current view.
```

Sliding an 8-token window across that text, only a fraction of the
windows land *inside* a single quote — most cross a quote-to-
connective boundary and lose the contiguous match. The LLM,
paraphrasing freely, sometimes reproduces an 8-token span from the
seed body in one piece because the seed bodies are short and
template-like (e.g. "the deploy succeeded on the first attempt" is
8 tokens — exactly one window).

The 4-gram column corrects for this: at n=4 the windows are short
enough to fit inside individual quotes, and the two systems become
indistinguishable.

This is a measurement defect of n-gram overlap with respect to
Hyphae's compositional structure, not a fidelity defect of Hyphae.
The fix (if a future comparator pursues it) is to evaluate n-gram
overlap *per quoted span* rather than across the entire response.

## Per-query patterns

A handful of queries are worth flagging individually. The full
per-query breakdown lives in the JSON envelopes; this section
highlights the ones where the LLM behaviour is informative.

### Queries where the LLM "hallucinates everything"
(unsupported_claim_rate_filtered == 1.0 in oracle mode)

`dialogue-003`, `risk-002`, `formal-002`, `fluency-causation-001`,
`neutral-001`, `reflect-002`, `assert-002` — 7 of 34. On these the
LLM produces responses whose every factual sentence the NLI flags as
not entailed by the context. Examples:

- `dialogue-003` ("what does the team say about test coverage?"):
  LLM says *"The context does not explicitly mention the team's
  statement about test coverage. However, it does mention…"* — when
  the seed bodies DO mention coverage. The LLM is refusing to
  acknowledge the literal evidence.
- `risk-002`: LLM emits a confident yes/no about release safety
  when the seed body is itself flagged high-confab-risk in the
  corpus. Hyphae's composition acknowledges the source ("a third-
  party blog post claims …") + emits the high-confab-risk
  limitation; the LLM strips both and ventures an opinion.

### Queries where the LLM happens to quote verbatim

`assert-001`, `assert-002`, `empty-001`, `empty-002`,
`fluency-multirole-001`, `formal-003` — 6 of 34 where the LLM
reproduces the seed body verbatim in oracle mode. Two of these
(`empty-*`) are degenerate — there are no seeds, so the LLM
correctly says "I don't have that information" and `verbatim_pass`
returns True vacuously. The other four are cases where the seed
body is short and quotable enough that the LLM happens to lift it
unchanged.

### Empty-working-set queries

`empty-001`, `empty-002` are the acknowledgment-only queries. Both
systems handle them correctly: Hyphae emits its acknowledgment
template; the LLM responds with a clear "I don't have that
information." No paraphrase, no hallucination, no quotation. These
contribute to `verbatim_pass_rate` as vacuous passes and do not
influence n-gram overlap (context is empty).

## What this comparison does NOT establish

Verbatim from ADR-0027 §"What this comparison does NOT establish",
with a per-item interpretation given the actual numbers above:

1. **Reader preference**. The numbers do not say which output is
   *better to read*. Hyphae's prose is template-rigid; the LLM's is
   smoother. Reader preference is a separate human-evaluation
   study, not addressed here.
2. **Generalisation across hardware**. N=1 hardware (laptop, Apple
   Silicon, MPS). A server-class run could compress the latency
   gap; it could also widen it because the LLM benefits from
   batching that this single-query comparator does not exploit.
   See ADR-0028 (planned).
3. **Generalisation across corpus size**. 34 queries (EN). The
   unsupported-claim-rate spread (Hyphae 0.219 vs LLM 0.367) has CIs
   that overlap at the bootstrap level; the *direction* is
   consistent but the *magnitude* needs a larger corpus to harden.
4. **Comparison against a state-of-the-art retrieval system**. The
   baseline is **vanilla naive RAG** — chunk, embed, top-k. Stronger
   pipelines (HyDE, RAG-Fusion, GraphRAG, MemGPT, query rewriting)
   could close the unsupported-claim gap. ADR-0030 (planned) adds a
   "strong RAG" column.
5. **That the gap is architectural, not implementation-detail**. The
   LLM is Q4-quantised and decoded with `temperature=0` greedy; a
   larger or undecoded variant might do better. The
   `connective_hygiene_pass_rate` tie suggests no obvious quality
   floor was crossed, but this is a caveat, not a proof.
6. **Multi-hop reasoning**. The corpus tests single-fragment lookup
   and multi-fragment composition without inferential leaps. A
   HotpotQA-style multi-hop benchmark would test a different
   regime; not in scope here.

## Reproduce

The full pipeline is reproducible from the repository as committed.

```bash
# 1. Setup Python env + download model
cd bench/baseline-llm-rag
uv sync
./scripts/download-model.sh

# 2. Export the EN corpus from the Rust harness
cd ../..
cargo run --quiet -p hyphae-eval --example export_corpus \
    > bench/baseline-llm-rag/corpus-en.json

# 3. Run the baseline (both modes)
cd bench/baseline-llm-rag
uv run baseline-llm-rag --mode oracle --corpus corpus-en.json \
    --output results/v0.1-laptop-oracle.json
uv run baseline-llm-rag --mode rag --corpus corpus-en.json \
    --output results/v0.1-laptop-rag.json

# 4. Run Hyphae through the same metrics pipeline
cd ../..
cargo run --quiet -p hyphae-eval --example export_results \
    > bench/baseline-llm-rag/hyphae-results.json
cd bench/baseline-llm-rag
uv run python -m baseline_llm_rag.score_hyphae \
    --hyphae-output hyphae-results.json \
    --output results/v0.1-laptop-hyphae.json
```

Each result JSON carries its hardware metadata, model SHA-256,
corpus SHA-256, decoding hyperparameters, and per-query trace
(response + retrieved chunks + per-metric breakdown). Reviewers can
diff their own runs against the SHAs to confirm bit-for-bit setup
parity before comparing aggregates.

## What's next

Three follow-up ADRs are queued (placeholders only, not yet
written):

- **ADR-0028 — Hardware matrix.** Re-run both systems on a server-
  class machine to test whether the latency and unsupported-claim
  spreads generalise.
- **ADR-0029 — Ablation harness.** Disable individual Hyphae
  components (boundary smoothing, cascade-shape composition, lexicon,
  ethics gate) one at a time and re-measure. Without this, the paper
  cannot isolate which component carries the unsupported-claim-rate
  advantage.
- **ADR-0030 — "Strong RAG" baseline.** HyDE / RAG-Fusion / query
  rewriting on top of the vanilla pipeline. Establishes whether
  retrieval sophistication closes the gap or whether the gap is
  architectural.

The biggest single inferential gap right now is **0029 — without
ablations, the comparison establishes that Hyphae beats vanilla RAG
on the comparable subset, but not which Hyphae component is doing
the work**. That is the next milestone before the writeup is
paper-grade in the strict sense.

**Update — 2026-05-28**: ADR-0029 is now landed and its results are
in [`ablation-study.md`](ablation-study.md). The headline finding
relevant to this comparison: the `ngram_overlap_8` inversion against
the LLM (Hyphae 0.240 vs LLM 0.329) is **causally attributable to
lexicon scale**, not to citation fidelity — disabling the rich
lexicon raises Hyphae's `ngram_overlap_4` from 0.466 to 0.521. The
remaining three components (cascade-shape, ethics gate, boundary
smoothing) produced null deltas on the comparator metrics at this
corpus size; the ablation writeup discusses what that does and does
not mean.

**Update — 2026-05-28**: ADR-0028 (hardware matrix) is also landed
and its results are in [`hardware-matrix.md`](hardware-matrix.md).
On the second hardware configuration (DigitalOcean c-16 droplet,
Intel Xeon Platinum 8168, CPU-only Linux x86_64) the quality
metrics agree with the laptop run within ±0.03, and **the
Hyphae:LLM latency ratio widens to ~654,000×** (oracle) /
~904,000× (RAG). Without GPU acceleration the LLM baseline's p95
crosses 10 seconds while Hyphae stays under 100 microseconds. The
gap the laptop-only comparison exposed is not a Mac/MPS quirk — it
amplifies in the direction of every cloud deployment without a
dedicated GPU.

**Update — 2026-05-28**: ADR-0030 (strong-RAG) AND ADR-0030b
(multi-LLM matrix) are landed and their combined results are in
[`multi-llm-comparison.md`](multi-llm-comparison.md). The
**headline that supersedes this writeup's claim**: across 19
LLM-based system configurations (vanilla, strong-RAG, plus 5 LLMs
× 3 modes via DO Inference), **only GPT-4.1 with HyDE + cross-
encoder reranking matches Hyphae's unsupported-claim rate** (0.211
vs 0.219, within bootstrap CI). Every other system lands further
from Hyphae than the vanilla naive RAG this writeup measured.
Hyphae's latency advantage remains 50,000+× even against the
single LLM-based system that ties it on quality. Read the
multi-LLM writeup before quoting the numbers in this document —
the refined paper claim lives there.
