# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Celiums Solutions LLC
"""Strong-RAG pipeline — HyDE + cross-encoder reranking.

Per ADR-0030, this is the canonical "serious RAG" stack a
production team would reach for after vanilla chunk-embed-retrieve
stops being adequate. The pipeline wraps the existing
`LlamaGenerator`, `Embedder`, and FAISS index — the only new
component is the cross-encoder reranker.

Algorithm:
    1. Generate a HyDE hypothetical answer with the LLM
    2. Embed the HyDE answer (not the query) and retrieve top-20
    3. Cross-encoder rerank (query, candidate) pairs
    4. Pass top-5 reranked chunks to the LLM for the final response

The cross-encoder is `BAAI/bge-reranker-base` (~278 MB, CPU-fast,
top of the MTEB reranking leaderboard for its parameter class).
"""

from __future__ import annotations

import logging
import time
from dataclasses import dataclass

from sentence_transformers import CrossEncoder

from .corpus_loader import EvalQuery, pooled_seed_bodies
from .rag_pipeline import (
    DEFAULT_DECODING,
    DEFAULT_SYSTEM_PROMPT,
    Chunker,
    Embedder,
    LlamaGenerator,
    RagResponse,
    _IndexedChunk,
    _VectorIndex,
    _render_context,
    _render_user_message,
)

log = logging.getLogger(__name__)


# ── Constants — pinned per ADR-0030 ───────────────────────────

RERANKER_MODEL = "BAAI/bge-reranker-base"
HYDE_OVER_RETRIEVAL_K = 20
FINAL_TOP_K = 5

HYDE_SYSTEM_PROMPT = (
    "You are a helpful assistant. Write a brief, plausible answer "
    "to the user's question. Do not say you do not know — make up "
    "a reasonable answer of one to three sentences. This answer "
    "will be used to find relevant material, so include the kind "
    "of terms a true answer would contain."
)

HYDE_USER_TEMPLATE = "Question: {query}\n\nWrite the brief plausible answer."


# ── Reranker wrapper ──────────────────────────────────────────


class Reranker:
    """Wraps a sentence-transformers CrossEncoder. Implements the
    minimal `(pairs) -> scores` interface the pipeline uses."""

    def __init__(self, model_name: str = RERANKER_MODEL) -> None:
        log.info("Loading cross-encoder reranker %s", model_name)
        self.model_name = model_name
        self.model = CrossEncoder(model_name)

    def score(self, query: str, candidates: list[str]) -> list[float]:
        if not candidates:
            return []
        pairs = [(query, c) for c in candidates]
        scores = self.model.predict(pairs, batch_size=16, show_progress_bar=False)
        return [float(s) for s in scores]


# ── Strong-RAG pipeline ───────────────────────────────────────


@dataclass(frozen=True)
class StrongRagDiagnostics:
    """Per-query diagnostics specific to strong-RAG. Carried in the
    result envelope under `metrics.strong_rag_diagnostics`."""

    hyde_answer: str
    hyde_latency_ms: int
    rerank_latency_ms: int
    final_generation_latency_ms: int
    retrieved_provenance_pre_rerank: tuple[tuple[str, int, float], tuple[str, int, float], tuple[str, int, float], tuple[str, int, float], tuple[str, int, float]] | tuple


class StrongRagPipeline:
    """End-to-end HyDE + reranker pipeline. Same `run_query` API as
    `RagPipeline` so the eval runner uses it interchangeably."""

    def __init__(
        self,
        *,
        generator: LlamaGenerator,
        embedder: Embedder | None = None,
        chunker: Chunker | None = None,
        reranker: Reranker | None = None,
        system_prompt: str = DEFAULT_SYSTEM_PROMPT,
        over_retrieval_k: int = HYDE_OVER_RETRIEVAL_K,
        final_top_k: int = FINAL_TOP_K,
    ) -> None:
        self.mode = "strong-rag"
        self.generator = generator
        self.embedder = embedder or Embedder()
        self.chunker = chunker or Chunker()
        self.reranker = reranker or Reranker()
        self.system_prompt = system_prompt
        self.over_retrieval_k = over_retrieval_k
        self.final_top_k = final_top_k
        self._index = _VectorIndex()

    def build_index(self, queries: list[EvalQuery]) -> None:
        """Build the pooled FAISS index over all corpus seed bodies —
        same shape as RagPipeline.build_index when mode='rag'."""
        chunks: list[_IndexedChunk] = []
        for qid, body in pooled_seed_bodies(queries):
            for i, chunk_text in enumerate(self.chunker.chunk(body)):
                chunks.append(_IndexedChunk(chunk=chunk_text, query_id=qid, chunk_idx=i))

        log.info("Embedding %d chunks for strong-RAG FAISS index", len(chunks))
        embeddings = self.embedder.encode([c.chunk for c in chunks])
        self._index.build(chunks, embeddings)
        log.info("Strong-RAG index built — %d chunks", len(chunks))

    def run_query(self, query: EvalQuery) -> RagResponse:
        if self._index.index is None:
            raise RuntimeError("strong-rag requires build_index() before run_query()")

        # ── Step 1: HyDE generation ──────────────────────────
        t0 = time.perf_counter()
        hyde_user = HYDE_USER_TEMPLATE.format(query=query.query)
        hyde_answer = self.generator.generate(HYDE_SYSTEM_PROMPT, hyde_user)
        hyde_latency_ms = int((time.perf_counter() - t0) * 1000)

        # ── Step 2: Embed HyDE + over-retrieve ───────────────
        hyde_emb = self.embedder.encode([hyde_answer])
        candidates = self._index.search(hyde_emb[0], self.over_retrieval_k)

        # ── Step 3: Cross-encoder rerank ─────────────────────
        t_rerank = time.perf_counter()
        candidate_chunks = [c.chunk for c in candidates]
        scores = self.reranker.score(query.query, candidate_chunks)
        rerank_latency_ms = int((time.perf_counter() - t_rerank) * 1000)

        ranked = sorted(
            zip(candidates, scores, strict=True), key=lambda x: -x[1]
        )
        top_k = ranked[: self.final_top_k]
        retrieved = [c.chunk for c, _ in top_k]
        provenance = tuple((c.query_id, c.chunk_idx) for c, _ in top_k)

        # ── Step 4: Final generation with reranked context ──
        t_gen = time.perf_counter()
        context = _render_context(retrieved) if retrieved else "(no context available)"
        user = _render_user_message(query.query, context)
        response = self.generator.generate(self.system_prompt, user)
        final_generation_latency_ms = int((time.perf_counter() - t_gen) * 1000)

        total_latency_ms = hyde_latency_ms + rerank_latency_ms + final_generation_latency_ms

        return RagResponse(
            query_id=query.id,
            response=response,
            retrieved_chunks=tuple(retrieved),
            retrieved_provenance=provenance,
            latency_ms=total_latency_ms,
        )

    # Per-query diagnostics surfaced for the eval runner to attach to
    # the result envelope. Optional — the runner accesses it only when
    # the pipeline is a StrongRagPipeline instance.
    def last_diagnostics(self) -> dict[str, object]:
        """Not implemented in v0.1 — diagnostics are folded into
        latency_ms for now. A future ADR can split them out."""
        return {}
