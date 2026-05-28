<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

# results/

Generated JSON artifacts from comparator runs. Re-runnable; not
checked into git (see `.gitignore`).

## Artifact format

Each run emits one JSON file. Filename convention:

```
v{ADR-of-the-run}-{hardware-tag}-{mode}.json
```

For example: `v0.1-laptop-oracle.json` is the v0.1 run on the
laptop in `oracle` retrieval mode.

### Schema

```jsonc
{
  "metadata": {
    "comparator_version": "0.1.0",
    "adr": "0027",
    "mode": "oracle",
    "model": {
      "repo": "bartowski/Meta-Llama-3.1-8B-Instruct-GGUF",
      "file": "Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf",
      "sha256": "..."
    },
    "embedder": "sentence-transformers/all-MiniLM-L6-v2",
    "vector_index": "faiss.IndexFlatIP",
    "chunk_size": 256,
    "chunk_overlap": 32,
    "retrieval_k": 5,
    "decoding": { "seed": 42, "temperature": 0.0, "top_p": 1.0 },
    "hardware": {
      "os": "...",
      "cpu_model": "...",
      "cpu_cores": 0,
      "ram_gb": 0
    },
    "wall_clock_s": 0.0,
    "corpus_sha256": "..."
  },
  "per_query": [
    {
      "query_id": "dialogue-001",
      "response": "...",
      "retrieved_chunks": ["..."],
      "metrics": {
        "verbatim_pass": false,
        "connective_hygiene_pass": true,
        "ngram_overlap_4": 0.62,
        "ngram_overlap_5": 0.48,
        "ngram_overlap_8": 0.21,
        "unsupported_claim_rate": 0.33,
        "latency_ms": 1842
      }
    }
  ],
  "aggregate": {
    "verbatim_pass_rate": 0.0,
    "connective_hygiene_pass_rate": 1.0,
    "ngram_overlap_4_mean": 0.0,
    "ngram_overlap_4_ci95": [0.0, 0.0],
    "unsupported_claim_rate_mean": 0.0,
    "unsupported_claim_rate_ci95": [0.0, 0.0],
    "latency_p50_ms": 0,
    "latency_p95_ms": 0
  },
  "caveats": []
}
```

The corresponding Hyphae output (produced by
`cargo run -p hyphae-eval --example export_results`) follows the
same envelope with `metadata.system = "hyphae"` and the metrics map
restricted to the **comparable subset** (verbatim, connective
hygiene, n-gram overlap, unsupported-claim rate, latency). The
Hyphae-specific dimensions (schema, limitations, lexicon coverage)
live in a separate `hyphae_specific` block — they appear in the
writeup but NOT in the comparison table per ADR-0027 §"What the
comparator measures".
