<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0027
title: Baseline LLM+RAG comparator for paper-grade claim validation
status: accepted
date: 2026-05-28
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (design phase)]
---

# 0027 — Baseline LLM+RAG comparator

## Context

The v0.2 evaluation harness (ADR-0008/0009/0010, `hyphae-eval`) and
the criterion bench (ADR-0015) produce honest **internal** numbers.
Both establish that Hyphae meets its declared commitments — verbatim
compliance, schema fidelity, limitation handling, latency
distributions on a populated substrate. Neither establishes that
Hyphae is **better than the alternative the field defaults to**, and
the architectural claim at the centre of the project ("no LLM in the
cognition path is preferable when audit beats polish") cannot be
validated without that comparison.

Paper-grade evaluation requires a comparator. The natural one is
**vanilla LLM + RAG**: chunk corpus, embed, retrieve top-k, prompt a
generation LLM with the retrieved context. This is the stock pattern
every RAG library implements; the literature treats it as the
"naive RAG" baseline that more elaborate retrieval-augmented systems
must beat. If Hyphae produces measurably less unsupported-claim
content than vanilla LLM+RAG on the same corpus and queries, the
verbatim-quotation commitment is the load-bearing reason. If it does
not, the commitment is decoration.

The harness alone cannot do this. The 9 dimensions Hyphae scores
itself on (verbatim, schema, limitation recall/precision, connective
hygiene, acknowledgment-only fidelity, lexical diversity, role
coverage, boundary smoothness) are **Hyphae-specific by design**.
Some of them have no natural meaning for an LLM output:

- `schema_match_rate` — the LLM produces free prose, not a schema
  discriminator. Forcing it through a post-hoc classifier introduces
  classifier error into the baseline score.
- `limitation_recall/precision` — Hyphae emits typed
  `LimitationTrigger` tags; an LLM acknowledges limitations (or fails
  to) in prose. Detection via keyword or NLI is noisy.
- `lexical_diversity / role_coverage / boundary_smoothness` — defined
  over Hyphae's curated lexicon (ADR-0005/0008). LLM prose does not
  draw from that lexicon. The metric does not apply.

Only **`verbatim_pass`** and **`connective_hygiene_pass`** are
directly comparable across both systems.

## Decision

**Add `bench/baseline-llm-rag/`: a sibling subdirectory in this
repository that implements a stock LLM+RAG pipeline (Llama-3.1-8B-
Instruct via llama.cpp, sentence-transformers MiniLM-L6 + FAISS
retrieval), runs it against the same EN corpus that `hyphae-eval`
uses, and reports the comparable-subset metrics side-by-side with
Hyphae. Code is open source (Apache 2.0), reproducible with one
script, and lives in the same repository as the system under
evaluation.**

### What the comparator measures

Comparison is restricted to **dimensions that mean the same thing
for both architectures**. Forcing one system into the other's metric
shape would import that shape's errors as baseline noise.

**Comparable subset:**

| Metric | Source | Hyphae expected | LLM+RAG expected |
|---|---|---|---|
| `verbatim_pass_rate` | hyphae-eval, applied to LLM output | 1.0 by construction | < 1.0 (paraphrase) |
| `ngram_overlap_at_n` (n=4, 5, 8) | new in `metrics_extra.py` | 1.0 by construction | partial |
| `unsupported_claim_rate` | NLI-based, new | ≈ 0 by construction | > 0 |
| `connective_hygiene_pass_rate` | hyphae-eval | high | high |
| `latency_p50, p95` | criterion-comparable wall clock | ms | seconds |
| `peak_memory_mb` | RSS sampling | low | high (model weights) |

**Hyphae-specific (NOT in comparator table — reported separately for
context):**

`schema_match_rate`, `limitation_recall`, `limitation_precision`,
`acknowledgment_only_pass_rate`, `lexical_diversity`,
`role_coverage`, `boundary_smoothness`. These are reported in the
Hyphae column of the writeup but the LLM+RAG column is marked
**"not applicable — see ADR-0027 §rationale"**. The integrator is
prevented from silently claiming Hyphae wins on a dimension the
baseline never had a chance to compete on.

### Stack — exact versions, pinned

- **Generator LLM**: `meta-llama/Llama-3.1-8B-Instruct`, GGUF
  Q4_K_M quantization from the HuggingFace `bartowski` mirror.
  ~5 GB on disk, fits in laptop RAM. Loaded via `llama-cpp-python`
  (which vendors a pinned llama.cpp). Seed fixed (`seed=42`),
  temperature 0.0, top_p 1.0 — deterministic decoding for
  reproducibility.
- **Embedder**: `sentence-transformers/all-MiniLM-L6-v2` — 22M
  parameters, 384-dim embeddings, the most-cited RAG paper baseline.
  CPU-only, no quantization.
- **Vector index**: FAISS `IndexFlatIP` (inner product after L2
  normalization == cosine). Exact, no approximation. 34-query corpus
  is small enough that ANN brings no measurable speedup and would
  add a stochasticity nuisance.
- **Chunking**: 256-token chunks with 32-token overlap on
  `tiktoken` `cl100k_base` count. One seed body per chunk when it
  fits; never split mid-sentence (chunker respects punctuation
  boundaries).
- **Retrieval**: top-k = 5. The corpus is small enough that k=5
  often returns ALL seed bodies; this is a feature, not a bug — it
  means the comparator is evaluating the LLM's composition quality
  on a near-oracle context, not its retrieval. Hyphae also sees
  near-oracle context (the corpus pre-supplies the working set).

### Hardware — single-configuration baseline

Same laptop hardware used for `docs/perf/v0.2-baseline.md`. The
v0.2 perf doc records cpu model, RAM, OS — the comparator inherits.
A second hardware configuration (server-class, multi-core) is
deferred to a follow-up ADR. The honest caveat: **N=1 hardware →
the head-to-head holds for this laptop class; generalization
requires the hardware matrix work that future ADR**.

### Same corpus, same input semantics

The comparator's corpus is exported from `seed_corpus_en()`
(`crates/hyphae-eval/src/corpus.rs`) via a tiny binary
`crates/hyphae-eval/examples/export_corpus.rs` that emits the
corpus as JSON. The Python pipeline reads that JSON, never
reproduces the corpus in Python source. Single source of truth.

**Two retrieval modes**, both reported:

1. **`oracle` mode**: the LLM receives the same seeds Hyphae receives.
   No retrieval — the seeds for query Q are the context for Q. This
   measures the composition delta: given identical context, how do
   the two systems' outputs compare?
2. **`rag` mode**: the LLM goes through full retrieval (FAISS over
   all corpus seed bodies pooled into one index). This measures the
   end-to-end RAG pipeline. Likely worse than `oracle` mode because
   FAISS over a 34-query mixed pool will sometimes retrieve
   cross-query seeds.

The paper-grade comparison is **`oracle` mode** — it isolates the
composition contribution. `rag` mode is reported as supporting
evidence that the LLM stack functions; it is not the head-to-head.

### Where the code lives

```
bench/baseline-llm-rag/
├── README.md           # how to reproduce, what each script does
├── pyproject.toml      # uv-managed env, deps pinned by hash
├── .python-version     # 3.11
├── .gitignore          # excludes models/, results-cache/
├── scripts/
│   └── download-model.sh        # idempotent HF download
├── src/baseline_llm_rag/
│   ├── corpus_loader.py         # reads exported JSON
│   ├── rag_pipeline.py          # chunk → embed → FAISS → LLM
│   ├── metrics_extra.py         # n-gram overlap, NLI claims
│   └── eval_runner.py           # orchestrate + emit JSON
└── results/
    ├── README.md                # describes the artifact format
    └── v0.1-laptop-{oracle,rag}.json    # generated, gitignored
```

The directory is **not** a Cargo workspace member. It runs from its
own `uv` env. CI does not exercise it — running it locally requires
downloading a 5 GB model, which is not appropriate for the
pre-merge gate.

### Two extra metrics, defined precisely

**`ngram_overlap_at_n`**: for response `R` and concatenated retrieved
context `C`, the fraction of n-grams (`n ∈ {4, 5, 8}`) of `R` that
appear in `C` after lowercasing and whitespace normalization. Hyphae
should reach 1.0 on the seed portion of its output. Stop-word n-grams
are NOT filtered — the metric measures token-level fidelity, not
content-token fidelity.

**`unsupported_claim_rate`**: response is split into sentences. Each
sentence becomes an entailment query against the retrieved context.
The NLI model `roberta-large-mnli` (HuggingFace, pinned by commit
hash) decides entailment/neutral/contradiction. The `unsupported`
rate is the fraction of sentences scored neutral or contradiction.
Hyphae's verbatim-quote bodies should score entailment; the
realizer's connective tissue ("Drawing from working memory,",
"Therefore,") scores neutral and is acceptable — those sentences are
not factual claims. A `connective_sentence` heuristic excludes them
from the denominator (documented in `metrics_extra.py`).

## What this comparison establishes

**Establishes:**
- Whether verbatim-quotation by construction (Hyphae) produces a
  measurably lower unsupported-claim rate than retrieval + LLM
  paraphrase (baseline), on the same corpus, same hardware, same
  retrieval-quality context.
- The n-gram overlap delta — the direct, model-free signal that
  Hyphae cites and the baseline paraphrases.
- The latency / memory delta at the cost dimension — Hyphae runs in
  a single Rust binary on CPU with a few hundred MB; the baseline
  needs a 5 GB model loaded.

## What this comparison does NOT establish

**Does NOT establish:**
- That Hyphae's prose is **preferable** to readers. Reader preference
  is a separate study (likely a human eval); this ADR does not enter
  it.
- Generalization to other hardware. N=1 hardware = N=1 hardware.
- Generalization to other corpus sizes. 34 queries (EN) + 5 (ES) is
  a small benchmark by design (ADR-0009 §"corpus is intentionally
  small for v0.1"). A larger benchmark is corpus-expansion work, not
  comparator work.
- That a more sophisticated RAG pipeline (HyDE, RAG-Fusion,
  GraphRAG, MemGPT) cannot close the unsupported-claim gap. The
  baseline is **vanilla RAG**, the literature's reference point —
  not the state-of-the-art retrieval system. A follow-up ADR can
  add a "strong RAG" column.
- That Hyphae is more truthful at higher knowledge complexity. The
  corpus is short-fragment ground truth; it does not test
  multi-hop synthesis. ADR for multi-hop benchmark is separate.

## Honesty discipline

The writeup at `docs/perf/baseline-comparison.md` MUST carry a
"caveats" section that lists every item from "does not establish"
above. The format mirrors `EvalReport.caveats` from `hyphae-eval` —
the integrator cannot publish the numbers without also publishing
the caveats. This is the same anti-greenwashing rule ADR-0001
§"Triangulation pre-commit" applies to the internal eval; the
comparator inherits it.

## Consequences

**Positive:**
- Paper-grade claim validation becomes possible. The "no LLM in
  cognition path" position now has a measurable contrast.
- The repo ships as a self-contained reproduction package: clone,
  install, run, compare.
- The two extra metrics (`ngram_overlap`, `unsupported_claim_rate`)
  are reusable for any future comparator (HyDE, GraphRAG, MemGPT).
  They live in `metrics_extra.py`, not pipeline-coupled.

**Negative:**
- Python dependency tree (PyTorch, sentence-transformers, FAISS,
  llama-cpp-python, transformers, datasets) added to the project's
  reproducibility surface. Pure-Rust property of the core is
  preserved only because the dependency is in a sibling subdirectory
  with its own env, not in the workspace.
- A 5 GB model download is required to reproduce. Documented; the
  download script is idempotent.
- The comparator measures **what a vanilla RAG would do** — not
  the strongest possible LLM-based system. Reviewers may push for a
  stronger baseline. The decision here is to be **honest about which
  baseline this is** rather than overpromise.

## Followups

- **0028** (planned): hardware matrix — re-run both Hyphae and the
  comparator on a server-class machine to test generalization.
- **0029** (planned): ablation harness — disable individual Hyphae
  components (boundary smoothing, cascade-shape composition,
  lexicon, ethics gate) and re-measure. Without this the paper
  cannot isolate which component carries the verbatim-quotation
  advantage.
- **0030** (planned): "strong RAG" baseline — HyDE or RAG-Fusion
  layered on top of the vanilla baseline. Establishes whether
  retrieval sophistication closes the gap or whether the gap is
  architectural.
- Larger corpus (separate ADR series, follow ADR-0009 expansion
  pattern).
- Multi-hop benchmark on a synthesis dataset (HotpotQA-style).
