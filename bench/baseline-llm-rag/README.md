<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

# baseline-llm-rag

A vanilla LLM + RAG pipeline used as the **paper-grade comparator**
for Hyphae v2. Architecture, rationale, and what this comparison
does and does not establish are in
[`../../docs/adr/0027-baseline-llm-rag-comparator.md`](../../docs/adr/0027-baseline-llm-rag-comparator.md).

Read that ADR before changing anything in this directory.

## What this is

A reference implementation of "naive RAG" — the pattern every RAG
library implements out of the box:

```
seed bodies → chunk → embed (MiniLM-L6) → FAISS index
                                              │
                              query ──────────┴──> top-k retrieval
                                              │
                              prompt template ─┴──> Llama-3.1-8B-Instruct
                                              │
                                              ▼
                                          response
```

Outputs are scored with **the same `hyphae-eval` scorers** that
score Hyphae, restricted to the dimensions that mean the same thing
for both systems (`verbatim_pass`, `connective_hygiene`), plus two
extra metrics this directory introduces: **n-gram overlap** and
**unsupported-claim rate**.

The corpus is exported from `crates/hyphae-eval` via the helper
binary `cargo run -p hyphae-eval --example export_corpus` — never
duplicated in Python source. Single source of truth.

## What this is NOT

- Not a benchmark of the best LLM-based system you can build. Vanilla
  RAG is the literature's reference point, not the state of the art.
  Stronger pipelines (HyDE, RAG-Fusion, GraphRAG, MemGPT) are
  deferred to follow-up ADRs.
- Not a CI dependency. Running this requires a 5 GB model download
  and CPU-bound generation; not appropriate for the pre-merge gate.
- Not part of the Rust workspace. The Hyphae core stays pure-Rust;
  this comparator lives in its own `uv` env in a sibling directory.

## Reproduce

### Requirements

- Python 3.11 (pinned in `.python-version`)
- `uv` (recommended) or `pip`
- ~6 GB free disk: 5 GB model + ~500 MB Python deps + small results
- macOS (Metal) or Linux (CPU or CUDA). Windows untested.

### Setup

```bash
cd bench/baseline-llm-rag

# Install Python deps (uv pins by hash via uv.lock)
uv sync

# Download Llama-3.1-8B-Instruct GGUF Q4_K_M (idempotent — re-runs are no-ops)
./scripts/download-model.sh

# Export the EN corpus from the Rust harness — never duplicate it in Python
cargo run -p hyphae-eval --example export_corpus > corpus-en.json
```

### Run the comparator

```bash
# oracle mode: LLM receives the same seeds Hyphae receives — measures composition delta
uv run baseline-llm-rag --mode oracle --corpus corpus-en.json --output results/v0.1-laptop-oracle.json

# rag mode: full FAISS retrieval over pooled corpus — measures end-to-end RAG
uv run baseline-llm-rag --mode rag --corpus corpus-en.json --output results/v0.1-laptop-rag.json
```

Both modes emit JSON with per-query metrics + aggregate means + CIs
+ hardware metadata. The writeup in
`docs/perf/baseline-comparison.md` consumes these files.

### Run Hyphae for the head-to-head

```bash
# from repo root
cargo run -p hyphae-eval --example export_results > bench/baseline-llm-rag/results/v0.1-laptop-hyphae.json
```

The writeup script reads all three JSON files and emits the
comparison table.

## Multi-hop column (offline harness, ADR-0036)

`src/baseline_llm_rag/multihop.py` is the offline scaffolding for the
multi-hop generalisation column (paper OPEN-01): does a single-span
verbatim system degrade **gracefully** (abstain) or **silently** (wrong
quote) on questions that need synthesis across two or more sources?

Runs end-to-end with no network, under the system interpreter (the
offline path is stdlib-only; `datasets`/`click` import lazily):

```sh
# bundled 6-item sample, naive-vs-abstention contrast
python3 -c "import sys; sys.path.insert(0,'src'); \
from baseline_llm_rag import multihop as m; \
print(m.render_offline_report(m.run_offline()))"

# or, with the uv env, the CLI (also loads HotpotQA/MuSiQue when ready):
uv run python -m baseline_llm_rag.multihop --offline-sample
uv run python -m baseline_llm_rag.multihop --dataset hotpotqa --n 50 --json-out results/multihop.json
```

The offline finding (`papers/arxiv-preprint/tables/multihop-offline.txt`):
a single-span system silently fails on multi-hop *without* an abstention
signal; a coverage-threshold abstention turns those into graceful
abstentions. The live LLM column + full-dataset run need infra and are
the documented followups in ADR-0036; drop real outputs into the scorer
via the `SystemAnswer` schema.

## Files

| Path | Purpose |
|---|---|
| `pyproject.toml` | Pinned deps. uv-managed. |
| `src/baseline_llm_rag/multihop.py` | Multi-hop offline harness (ADR-0036): schema, dataset loaders, bundled sample, reference answerers, scorer. |
| `.python-version` | 3.11 — exact. |
| `scripts/download-model.sh` | Idempotent Llama-3.1-8B GGUF Q4_K_M download via HF. |
| `src/baseline_llm_rag/corpus_loader.py` | Reads the JSON the Rust binary emits. |
| `src/baseline_llm_rag/rag_pipeline.py` | Chunk → embed → FAISS → LLM. Both `oracle` and `rag` modes. |
| `src/baseline_llm_rag/metrics_extra.py` | n-gram overlap + NLI unsupported-claim rate. |
| `src/baseline_llm_rag/eval_runner.py` | CLI entry. Orchestrates + writes JSON. |
| `results/` | Generated artifacts. Gitignored except for `README.md`. |

## License

This directory inherits the repository's dual-licensing scheme:

- Python source files: Apache 2.0 (each carries an
  `SPDX-License-Identifier: Apache-2.0` header)
- `README.md`, `results/README.md`: CC BY 4.0

See [`../../LICENSE`](../../LICENSE) and
[`../../LICENSE-CC-BY-4.0`](../../LICENSE-CC-BY-4.0).
