<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0004
title: Embedding provider — trait + scaffold default, transformer upgrade path
status: accepted
date: 2026-05-26
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (v0.1 implementation review)]
---

# 0004 — Embedding provider: trait + scaffold default, transformer upgrade path

## Context

`CognitiveFragment.embedding: Option<Vec<f32>>` and the constant
`hyphae_core::EMBEDDING_DIM = 256` ship since the v2 chartering
commit, but nothing populates them. The consequence is concrete and
load-bearing:

- `hyphae_subsystems::Episodic::pattern_complete` falls back to
  **insertion-order recency** whenever the query embedding is
  `None`. The composer receives the last-encoded fragments, not
  the semantically-relevant ones.
- `Episodic::cascade` starts from seeds the substrate could not
  rank meaningfully. The seeds it propagates from are noise.
- The eval harness cannot measure "did the composer get fragments
  related to the query" because the substrate cannot supply that
  signal in the first place.

The architectural bet of Hyphae (the manifesto: a substrate that
runs on commodity CPU + RAM with no LLM in the cognition path)
includes a category of **non-generative** semantic computation —
embedding text into a fixed-width vector. Embedding is a one-way
projection (text → vector), not a language model (does not produce
text). It belongs in the substrate's *infrastructure path*, not the
*cognition path*. The cognition path commitment in CLAUDE.md
prohibits LLM **generation**; it does not prohibit semantic
representation.

This ADR therefore defines an embedding provider, ships a default
implementation that lives inside the workspace with zero external
dependencies, and documents the explicit upgrade path to a
transformer-based provider that lands behind a future ADR.

## Decision

**A new crate `hyphae-embed`** exposes the [`EmbeddingProvider`]
trait and ships **two implementations**:

1. A **`HashingTokenEmbedder`** as the v0.1 default. Pure Rust, no
   external dependencies. Deterministic. Produces a unit-normalised
   `[f32; EMBEDDING_DIM]` vector from feature-hashed token n-grams
   with TF-IDF-style weighting. Captures genuine lexical overlap;
   does NOT capture deep semantic similarity (no synonyms, no
   paraphrase invariance). It is **a scaffold, not a model** —
   sufficient to validate the embedding-driven recall path end to
   end, and to unblock the architectural bet that depends on it.

2. A **`NullEmbedder`** for tests that need a deterministic zero
   vector regardless of input. Useful for isolating non-embedding
   behaviour.

A **transformer-based provider** (BGE-M3 quantised, MiniLM-L6, or
equivalent CPU-only ONNX model) is the v0.2 upgrade. It lands behind
a separate ADR with explicit evaluation of the dependency cost
(`fastembed` or `rust-bert` pull ONNX runtime, ~30 MB compiled),
the model distribution shape (release asset, like
celiums-memory v2's `ethics_knowledge` corpus per its v0.1 release
posture), and a re-benchmark of recall quality against the v0.1
hashing baseline. v0.1 ships the trait so the upgrade is
**additive, not breaking**.

### Trait shape

```rust
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a single text into a fixed-width unit-normalised vector.
    fn embed(&self, text: &str) -> Vec<f32>;

    /// The dimension of vectors this provider emits.
    /// Must equal hyphae_core::EMBEDDING_DIM for substrate use.
    fn dimension(&self) -> usize;
}
```

Synchronous by design — the `HashingTokenEmbedder` is in-process
arithmetic; a future remote-API impl can add an async overload
without breaking the sync surface (the substrate's hot path
benefits from sync embedding).

### Where the embedder runs in the flow

Two coverage points per the cognition-path topology:

1. **At `Substrate::ingest`**, **before** routing. The
   external-input fragment receives an embedding so subsequent
   storage in `episodic` is searchable.

2. **At `Substrate::recall_signal`**, **before** the cue fragment
   reaches the `episodic` subsystem. The cue gets an embedding so
   `pattern_complete` ranks by cosine similarity rather than
   falling back to recency.

A third path lands when grounded retrieval comes online (RFC §9
deferred): grounded fragments embed before being absorbed into the
store. Out of scope for this ADR.

### Why not transformer in v0.1

Three reasons, in priority order:

1. **The architectural bet is "no LLM in the cognition path."**
   Even though an encoder is not an LLM (no generation), shipping
   a transformer in v0.1 risks the optics of "Hyphae depends on
   neural networks at build time" — true at the encoder level, but
   the messaging fight is one we postpone. v0.2's ADR makes the
   case explicit: encoder for representation is not generator for
   composition.
2. **Heavy dependency footprint.** `fastembed` brings ONNX runtime;
   `rust-bert` brings torch bindings. Both compile slowly and bloat
   `cargo run`. The v0.1 binary should stay fast to iterate.
3. **The scaffold validates the architecture cheaply.** A hashing
   token embedder is enough to verify that the cascade actually
   propagates from semantically-related seeds rather than from
   recency. If the architecture works at all, it works at this
   resolution; the transformer upgrade then improves quality, not
   correctness.

### What the scaffold preserves vs gives up

**Preserves:**

- Deterministic, reproducible vectors (same text → same vector).
- Unit-norm so cosine similarity reads cleanly in `[-1, 1]`.
- Sensitive to lexical overlap: documents sharing vocabulary score
  higher cosine than disjoint documents.
- Sensitive to phrase order via bigram features (so
  "deploy succeeded" ≠ "succeeded deploy").
- Zero external dependencies — compiles in seconds.

**Gives up:**

- Paraphrase invariance (`"the build passes"` and `"the build is
  green"` score moderate cosine if they share no tokens, low if
  they share none).
- Synonym matching (`"big"` and `"large"` are orthogonal in this
  embedding).
- Multilingual concept mapping (the eventual reason to upgrade
  when ES re-enters per RFC §9).
- Deep semantic features (sentiment, topic, intent).

The v0.2 transformer ADR is the upgrade path for all four.

### Model storage / distribution (for v0.2)

v0.2's transformer model file will **not** ship in the git tree.
It distributes as a release asset (the same posture
celiums-memory v2 uses for `ethics_knowledge.jsonl`). The crate
provides a loader that resolves the model from:

1. An explicit `HYPHAE_EMBED_MODEL_PATH` env var.
2. A platform-standard cache dir (`~/.cache/hyphae-embed/<sha>`).
3. Optional download with SHA-256 verification.

v0.1 has no model file because the hashing embedder is pure code.

## Crate layout

```
crates/hyphae-embed/
├── Cargo.toml
└── src/
    ├── lib.rs           public API + trait + re-exports
    ├── hashing.rs       HashingTokenEmbedder (default v0.1)
    └── null.rs          NullEmbedder (test helper)
```

The crate **depends only on `hyphae-core`** — no other workspace
crate. This keeps the dependency direction strictly downward:
`hyphae-substrate` and `hyphae-subsystems` depend on
`hyphae-embed`, never the reverse.

## Wire-up to substrate

`hyphae-substrate` gains an `Arc<dyn EmbeddingProvider>` field:

- `Substrate::new` defaults to `HashingTokenEmbedder`.
- `Substrate::with_embedder(provider)` allows the integrator to
  inject a custom impl.
- `Substrate::ingest` calls `embedder.embed(&payload.content)`
  before constructing the fragment.
- `Substrate::recall_signal` embeds the cue before handing it to
  the `episodic` subsystem.

The two `process` paths in `Episodic` already consult
`fragment.embedding`; no change needed there. The recency fallback
in `pattern_complete` still fires when the embedding is absent
(test paths, ad-hoc construction), so the new contract is
**additive**: an `Option` populated by the substrate, falling back
to recency when missing.

## Consequences

- The cognition path becomes **semantic** rather than chronological.
  Cascade activation can finally propagate from a seed that
  actually relates to the query.
- The eval harness gains a meaningful test surface for recall
  quality: `must_recall_top_hit` becomes a real expectation rather
  than a coin flip.
- The "no LLM in the cognition path" commitment is preserved
  cleanly: the encoder is in the infrastructure path. The cognition
  path (compose) still uses fragment quotation + connective tissue
  only.
- The dependency surface stays small — zero new external crates in
  v0.1.
- v0.2's transformer upgrade is **additive**: same trait, swap the
  impl. No breaking change to the substrate API.

## Cross-references

- **RFC v1-living §9** ("What is NOT in v0.1") — the multilingual
  re-entry is the eventual reason the transformer-based provider
  lands. Hashing embedder is English-only by lexical accident; a
  multilingual embedder is the requirement that justifies the
  upgrade.
- **ADR-0001 §"Curated dependencies"** — workspace dependency
  curation discipline. v0.1's zero-new-dep choice for this ADR
  aligns.
- **ADR-0003 §"Layer C deferral"** — establishes the precedent for
  "deferred capability with explicit re-entry ADR". The transformer
  upgrade follows the same pattern: capability deferred, ADR-shaped
  re-entry documented.
- **`hyphae_core::EMBEDDING_DIM`** — the contract this ADR honours.
- **`hyphae_subsystems::Episodic::pattern_complete`** — the
  consumer of populated embeddings.
