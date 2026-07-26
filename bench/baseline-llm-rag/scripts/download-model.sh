#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Celiums Solutions LLC
#
# Idempotent download of Llama-3.1-8B-Instruct GGUF Q4_K_M.
# Pinned to a specific HF repo + filename so reruns are deterministic.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCH_DIR="$(dirname "${SCRIPT_DIR}")"
MODELS_DIR="${BENCH_DIR}/models"

HF_REPO="bartowski/Meta-Llama-3.1-8B-Instruct-GGUF"
MODEL_FILE="Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf"
EXPECTED_SHA256="7b064f5842bf9532c91456deda288a1b672397a54fa729aa665952863033557c"
# SHA256 pinned 2026-05-28 after first verified download against
# bartowski/Meta-Llama-3.1-8B-Instruct-GGUF on HuggingFace. Any
# mismatch on re-run signals either a tampered local cache or a
# silent upstream re-upload — both warrant investigation, not retry.

MODEL_PATH="${MODELS_DIR}/${MODEL_FILE}"

mkdir -p "${MODELS_DIR}"

if [[ -f "${MODEL_PATH}" ]]; then
    echo "✓ Model already present: ${MODEL_PATH}"
    actual_sha=$(shasum -a 256 "${MODEL_PATH}" | awk '{print $1}')
    echo "  SHA256: ${actual_sha}"
    if [[ "${EXPECTED_SHA256}" == "b1bd6e"* ]]; then
        echo "  (no hash pin yet — record this hash in download-model.sh after first verified download)"
    elif [[ "${actual_sha}" != "${EXPECTED_SHA256}" ]]; then
        echo "✗ SHA256 mismatch! expected ${EXPECTED_SHA256}, got ${actual_sha}" >&2
        echo "  Delete ${MODEL_PATH} and re-run to re-download." >&2
        exit 1
    fi
    exit 0
fi

echo "→ Downloading ${HF_REPO}/${MODEL_FILE} via huggingface-hub..."

# Use the huggingface-hub Python CLI for resumable, hashed download.
# Requires the bench env to be installed (uv sync) so hf-cli is in PATH.
if ! command -v huggingface-cli >/dev/null 2>&1; then
    echo "✗ huggingface-cli not found. Run 'uv sync' in ${BENCH_DIR} first." >&2
    exit 1
fi

huggingface-cli download \
    "${HF_REPO}" \
    "${MODEL_FILE}" \
    --local-dir "${MODELS_DIR}" \
    --local-dir-use-symlinks False

if [[ ! -f "${MODEL_PATH}" ]]; then
    echo "✗ Download finished but ${MODEL_PATH} not found" >&2
    exit 1
fi

echo "✓ Downloaded ${MODEL_PATH}"
actual_sha=$(shasum -a 256 "${MODEL_PATH}" | awk '{print $1}')
echo "  SHA256: ${actual_sha}"
echo ""
echo "Next: record this hash by editing scripts/download-model.sh and setting"
echo "  EXPECTED_SHA256=\"${actual_sha}\""
