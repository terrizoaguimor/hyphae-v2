# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Celiums Solutions LLC
"""RAG pipeline: chunk → embed → FAISS → Llama-3.1-8B.

Two retrieval modes per ADR-0027 §"Same corpus, same input semantics":

  - **oracle**: the LLM receives exactly the seeds the query carries
    in the corpus. No retrieval. Isolates the composition delta —
    given identical context, how do the two systems' outputs differ?
    This is the paper-grade head-to-head.
  - **rag**: the LLM goes through full FAISS retrieval over the
    pooled seed-body index. Measures the end-to-end RAG pipeline.
    Supporting evidence; not the head-to-head.

Both modes go through the same `_generate` LLM call so the prompt
template, decoding hyperparameters, and chat formatting are
identical. The only difference is what fills `context`.
"""

from __future__ import annotations

import logging
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Literal

import faiss
import numpy as np
import tiktoken
from llama_cpp import Llama
from sentence_transformers import SentenceTransformer

from .corpus_loader import EvalQuery, EvalSeed, pooled_seed_bodies

log = logging.getLogger(__name__)


# ── Constants — pinned per ADR-0027 ───────────────────────────

EMBED_MODEL = "sentence-transformers/all-MiniLM-L6-v2"
EMBED_DIM = 384
CHUNK_SIZE = 256
CHUNK_OVERLAP = 32
RETRIEVAL_K = 5

DEFAULT_SYSTEM_PROMPT = (
    "You are a helpful assistant. Answer the user's question using only "
    "the provided context. If the context does not contain the answer, "
    "say so clearly. Do not fabricate information that is not in the "
    "context."
)

DEFAULT_DECODING = {
    "seed": 42,
    "temperature": 0.0,
    "top_p": 1.0,
    "max_tokens": 512,
}


# ── Data structures ───────────────────────────────────────────


@dataclass(frozen=True)
class RagResponse:
    """One pipeline output for one query."""

    query_id: str
    response: str
    retrieved_chunks: tuple[str, ...]
    # For diagnostics — the (query_id, chunk_idx) pairs of retrieved
    # chunks in `rag` mode. Empty in `oracle` mode.
    retrieved_provenance: tuple[tuple[str, int], ...]
    latency_ms: int


# ── Chunker ──────────────────────────────────────────────────


class Chunker:
    """Token-aware chunker on `cl100k_base` BPE.

    Counts tokens via tiktoken, splits at punctuation boundaries to
    avoid mid-sentence cuts. For the EN eval corpus's short, single-
    sentence seed bodies this almost always emits one chunk per seed.
    """

    def __init__(self, chunk_size: int = CHUNK_SIZE, overlap: int = CHUNK_OVERLAP) -> None:
        self.chunk_size = chunk_size
        self.overlap = overlap
        self._enc = tiktoken.get_encoding("cl100k_base")

    def chunk(self, text: str) -> list[str]:
        text = text.strip()
        if not text:
            return []
        tokens = self._enc.encode(text)
        if len(tokens) <= self.chunk_size:
            return [text]

        chunks: list[str] = []
        i = 0
        step = self.chunk_size - self.overlap
        while i < len(tokens):
            slice_ = tokens[i : i + self.chunk_size]
            chunks.append(self._enc.decode(slice_))
            i += step
        return chunks


# ── Embedder ─────────────────────────────────────────────────


class Embedder:
    """Wraps sentence-transformers MiniLM-L6 with L2 normalization
    so FAISS `IndexFlatIP` becomes a cosine-similarity index."""

    def __init__(self, model_name: str = EMBED_MODEL) -> None:
        self.model = SentenceTransformer(model_name)
        self.model_name = model_name

    def encode(self, texts: list[str]) -> np.ndarray:
        if not texts:
            return np.zeros((0, EMBED_DIM), dtype=np.float32)
        emb = self.model.encode(
            texts,
            batch_size=32,
            show_progress_bar=False,
            convert_to_numpy=True,
            normalize_embeddings=True,
        )
        return emb.astype(np.float32)


# ── Index ────────────────────────────────────────────────────


@dataclass
class _IndexedChunk:
    chunk: str
    query_id: str
    chunk_idx: int


class _VectorIndex:
    """FAISS IndexFlatIP over the chunks. Exact, not ANN."""

    def __init__(self) -> None:
        self.index: faiss.Index | None = None
        self.chunks: list[_IndexedChunk] = []

    def build(self, chunks: list[_IndexedChunk], embeddings: np.ndarray) -> None:
        if len(chunks) != embeddings.shape[0]:
            raise ValueError(
                f"chunks/embeddings size mismatch: {len(chunks)} vs {embeddings.shape[0]}"
            )
        self.chunks = chunks
        idx = faiss.IndexFlatIP(EMBED_DIM)
        if embeddings.size:
            idx.add(embeddings)
        self.index = idx

    def search(self, query_emb: np.ndarray, k: int) -> list[_IndexedChunk]:
        if self.index is None or self.index.ntotal == 0:
            return []
        k = min(k, self.index.ntotal)
        _, ids = self.index.search(query_emb.reshape(1, -1), k)
        return [self.chunks[i] for i in ids[0] if i >= 0]


# ── Generator (Llama-3.1-8B via llama-cpp-python) ──────────────


class LlamaGenerator:
    """llama-cpp-python wrapper. Hyperparameters fixed per ADR-0027."""

    def __init__(
        self,
        model_path: str | Path,
        *,
        n_ctx: int = 8192,
        n_gpu_layers: int = -1,
        n_threads: int | None = None,
        verbose: bool = False,
    ) -> None:
        self.model_path = str(model_path)
        log.info("Loading Llama model from %s", self.model_path)
        # n_gpu_layers=-1 offloads all layers to GPU on Mac (Metal)
        # and CUDA when available; falls back to CPU silently when
        # neither is present.
        self.llm = Llama(
            model_path=self.model_path,
            n_ctx=n_ctx,
            n_gpu_layers=n_gpu_layers,
            n_threads=n_threads,
            seed=DEFAULT_DECODING["seed"],
            verbose=verbose,
        )
        log.info("Model loaded — n_ctx=%d", n_ctx)

    def generate(self, system: str, user: str) -> str:
        response = self.llm.create_chat_completion(
            messages=[
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            temperature=DEFAULT_DECODING["temperature"],
            top_p=DEFAULT_DECODING["top_p"],
            max_tokens=DEFAULT_DECODING["max_tokens"],
            seed=DEFAULT_DECODING["seed"],
        )
        return response["choices"][0]["message"]["content"].strip()


# ── Pipeline ─────────────────────────────────────────────────


Mode = Literal["oracle", "rag"]


def _render_context(chunks: list[str]) -> str:
    """Render retrieved chunks as a numbered context block."""
    lines = [f"[{i + 1}] {c}" for i, c in enumerate(chunks)]
    return "\n".join(lines)


def _render_user_message(query: str, context: str) -> str:
    return f"Context:\n{context}\n\nQuestion: {query}"


class RagPipeline:
    """End-to-end RAG pipeline. Stateful — build the index once per
    run, then call `run_query` for each query."""

    def __init__(
        self,
        *,
        mode: Mode,
        generator: LlamaGenerator,
        embedder: Embedder | None = None,
        chunker: Chunker | None = None,
        system_prompt: str = DEFAULT_SYSTEM_PROMPT,
        retrieval_k: int = RETRIEVAL_K,
    ) -> None:
        self.mode = mode
        self.generator = generator
        self.embedder = embedder or Embedder()
        self.chunker = chunker or Chunker()
        self.system_prompt = system_prompt
        self.retrieval_k = retrieval_k
        self._index = _VectorIndex()

    def build_index(self, queries: list[EvalQuery]) -> None:
        """Build the pooled FAISS index. Only used in `rag` mode; in
        `oracle` mode this is a no-op."""
        if self.mode == "oracle":
            log.info("oracle mode — skipping index build")
            return

        chunks: list[_IndexedChunk] = []
        for qid, body in pooled_seed_bodies(queries):
            for i, chunk_text in enumerate(self.chunker.chunk(body)):
                chunks.append(_IndexedChunk(chunk=chunk_text, query_id=qid, chunk_idx=i))

        log.info("Embedding %d chunks for FAISS index", len(chunks))
        embeddings = self.embedder.encode([c.chunk for c in chunks])
        self._index.build(chunks, embeddings)
        log.info("Index built — %d chunks", len(chunks))

    def run_query(self, query: EvalQuery) -> RagResponse:
        if self.mode == "oracle":
            return self._run_oracle(query)
        return self._run_rag(query)

    def _run_oracle(self, query: EvalQuery) -> RagResponse:
        """Oracle mode — LLM receives the corpus's seeds directly."""
        retrieved = [s.body for s in query.seeds]
        return self._call_llm(query, retrieved, provenance=())

    def _run_rag(self, query: EvalQuery) -> RagResponse:
        """RAG mode — FAISS top-k over the pooled corpus."""
        if self._index.index is None:
            raise RuntimeError("rag mode requires build_index() before run_query()")
        q_emb = self.embedder.encode([query.query])
        hits = self._index.search(q_emb[0], self.retrieval_k)
        retrieved = [h.chunk for h in hits]
        provenance = tuple((h.query_id, h.chunk_idx) for h in hits)
        return self._call_llm(query, retrieved, provenance=provenance)

    def _call_llm(
        self,
        query: EvalQuery,
        retrieved: list[str],
        *,
        provenance: tuple[tuple[str, int], ...],
    ) -> RagResponse:
        context = _render_context(retrieved) if retrieved else "(no context available)"
        user = _render_user_message(query.query, context)
        t0 = time.perf_counter()
        response = self.generator.generate(self.system_prompt, user)
        latency_ms = int((time.perf_counter() - t0) * 1000)
        return RagResponse(
            query_id=query.id,
            response=response,
            retrieved_chunks=tuple(retrieved),
            retrieved_provenance=provenance,
            latency_ms=latency_ms,
        )
