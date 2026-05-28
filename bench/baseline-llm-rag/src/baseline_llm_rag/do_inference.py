# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Celiums Solutions LLC
"""DigitalOcean Inference backend — OpenAI-compatible chat completions.

Wraps the `https://inference.do-ai.run/v1` endpoint exposed by
DigitalOcean's GenAI Platform. The catalogue includes Anthropic
(Claude Sonnet/Opus), OpenAI (GPT-4o, GPT-4.1, etc.), open SOTA
(Llama 3.3 70B, DeepSeek-3.2, Qwen 3, etc.), embedders, and
rerankers — all via a single API key.

Implements the same minimal generator interface (`generate(system,
user) -> str`) as `rag_pipeline.LlamaGenerator`, so RagPipeline and
StrongRagPipeline use either backend by duck typing.

Reference: ADR-0028b (planned) + ADR-0030.
"""

from __future__ import annotations

import logging
import time

import requests

from .rag_pipeline import DEFAULT_DECODING

log = logging.getLogger(__name__)


DEFAULT_DO_INFERENCE_ENDPOINT = "https://inference.do-ai.run/v1"

# Per-request timeout. Frontier models (Claude Opus, GPT-4.1) can take
# 10+ seconds on long contexts; 90s gives margin.
DEFAULT_REQUEST_TIMEOUT_S = 90

# Backoff schedule on transient errors (rate limit, 5xx, network).
RETRY_BACKOFF_S = (1.0, 3.0, 8.0)


class DoInferenceError(RuntimeError):
    """Raised when the DO Inference API returns an unrecoverable error."""


class DoInferenceGenerator:
    """OpenAI-compatible chat completions over DO Inference.

    Same `generate(system, user) -> str` contract as
    `rag_pipeline.LlamaGenerator`. Decoding hyperparameters mirror the
    local backend (seed=42, temperature=0.0, top_p=1.0) so results
    are as close to deterministic as the upstream provider allows.

    Note: some providers (Anthropic, OpenAI) do not honor `seed` for
    full bit-identity across runs. The writeup records this when
    backend=do-inference.
    """

    def __init__(
        self,
        *,
        model: str,
        api_key: str,
        endpoint: str = DEFAULT_DO_INFERENCE_ENDPOINT,
        timeout_s: float = DEFAULT_REQUEST_TIMEOUT_S,
    ) -> None:
        if not model:
            raise ValueError("model id is required (e.g. 'llama3.3-70b-instruct')")
        if not api_key:
            raise ValueError("api_key is required (set DO_INFERENCE_KEY env)")
        self.model = model
        self.api_key = api_key
        self.endpoint = endpoint.rstrip("/")
        self.timeout_s = timeout_s
        # For envelope metadata symmetry with LlamaGenerator.model_path
        self.model_path = f"do-inference:{model}"

    def generate(self, system: str, user: str) -> str:
        url = f"{self.endpoint}/chat/completions"
        payload: dict[str, object] = {
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "temperature": DEFAULT_DECODING["temperature"],
            "max_tokens": DEFAULT_DECODING["max_tokens"],
            "seed": DEFAULT_DECODING["seed"],
        }
        # Some upstream providers (notably Anthropic Claude via DO
        # Inference) reject `temperature` + `top_p` together. With
        # temperature=0 the decoding is already greedy, so top_p has
        # no effect — omitting it preserves behaviour and unblocks
        # those providers.
        if DEFAULT_DECODING["temperature"] > 0:
            payload["top_p"] = DEFAULT_DECODING["top_p"]
        headers = {
            "Authorization": f"Bearer {self.api_key}",
            "Content-Type": "application/json",
        }

        last_error: Exception | None = None
        for attempt, backoff in enumerate(RETRY_BACKOFF_S):
            try:
                r = requests.post(url, json=payload, headers=headers, timeout=self.timeout_s)
                # Retry on rate limit + transient 5xx
                if r.status_code == 429 or 500 <= r.status_code < 600:
                    log.warning(
                        "DO Inference %s returned %d (attempt %d/%d), backing off %ss",
                        self.model,
                        r.status_code,
                        attempt + 1,
                        len(RETRY_BACKOFF_S),
                        backoff,
                    )
                    time.sleep(backoff)
                    continue
                r.raise_for_status()
                data = r.json()
                if "choices" not in data or not data["choices"]:
                    raise DoInferenceError(
                        f"DO Inference response missing 'choices': {data}"
                    )
                content = data["choices"][0]["message"]["content"]
                return content.strip() if content else ""
            except requests.exceptions.RequestException as e:
                last_error = e
                if attempt + 1 < len(RETRY_BACKOFF_S):
                    log.warning(
                        "DO Inference %s network error (attempt %d/%d): %s — backing off %ss",
                        self.model,
                        attempt + 1,
                        len(RETRY_BACKOFF_S),
                        e,
                        backoff,
                    )
                    time.sleep(backoff)
                    continue
                raise DoInferenceError(
                    f"DO Inference {self.model} failed after {len(RETRY_BACKOFF_S)} attempts: {e}"
                ) from e

        # Exhausted retries on rate limit / 5xx
        raise DoInferenceError(
            f"DO Inference {self.model} exhausted retries (rate limit / server error)"
        ) from last_error
