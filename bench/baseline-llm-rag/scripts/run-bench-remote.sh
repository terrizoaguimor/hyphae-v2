#!/usr/bin/env bash
# Run inside the droplet. Idempotent-ish: assumes /root/hyphae-v2/ exists with the source tree.
set -euo pipefail

cd /root

# ── Install system deps ───────────────────────────────────────
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq build-essential curl pkg-config libssl-dev clang cmake git 2>&1 | tail -5
# Note: uv downloads its own Python 3.11 (no system python3.11 needed on Ubuntu 24.04)

# ── Install rustup if missing ────────────────────────────────
if ! command -v cargo >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal
fi
export PATH="$HOME/.cargo/bin:$PATH"
rustc --version

# ── Install uv if missing ────────────────────────────────────
if ! command -v uv >/dev/null 2>&1; then
    curl -LsSf https://astral.sh/uv/install.sh | sh
fi
export PATH="$HOME/.local/bin:$PATH"
uv --version

# ── Extract source tree ──────────────────────────────────────
cd /root
if [[ ! -d hyphae-v2 ]]; then
    mkdir hyphae-v2
    tar xzf /root/hyphae-v2-HEAD.tar.gz -C hyphae-v2
fi
cd hyphae-v2

# ── Cargo build release ──────────────────────────────────────
echo "=== cargo build --release ==="
cargo build --release --workspace --examples 2>&1 | tail -5

# ── uv sync Python deps ──────────────────────────────────────
cd bench/baseline-llm-rag
echo "=== uv sync ==="
uv sync 2>&1 | tail -5

# ── Download model ───────────────────────────────────────────
echo "=== model download ==="
uv run ./scripts/download-model.sh 2>&1 | tail -5

# ── Export EN corpus ─────────────────────────────────────────
cd /root/hyphae-v2
echo "=== export corpus ==="
cargo run --quiet --release -p hyphae-eval --example export_corpus > bench/baseline-llm-rag/corpus-en.json
echo "queries: $(python3 -c "import json; print(len(json.load(open('bench/baseline-llm-rag/corpus-en.json'))))")"

# ── Run 5 Hyphae ablations + score ───────────────────────────
cd bench/baseline-llm-rag
mkdir -p results
HW_TAG="c16-do-xeon"
for a in none no-shape no-ethics minimal-lexicon no-smoothing; do
    echo "=== Hyphae ablation: $a ==="
    cd /root/hyphae-v2
    cargo run --quiet --release -p hyphae-eval --example export_results_ablation -- --ablation "$a" > "bench/baseline-llm-rag/hyphae-results-${a}.json"
    cd bench/baseline-llm-rag
    uv run python -m baseline_llm_rag.score_hyphae \
        --hyphae-output "hyphae-results-${a}.json" \
        --output "results/v0.1-${HW_TAG}-hyphae-${a}.json" 2>&1 | tail -1
done

# ── Run LLM oracle + rag ─────────────────────────────────────
cd /root/hyphae-v2/bench/baseline-llm-rag
echo "=== LLM oracle ==="
uv run baseline-llm-rag --mode oracle --corpus corpus-en.json --output "results/v0.1-${HW_TAG}-oracle.json" 2>&1 | tail -3
echo "=== LLM rag ==="
uv run baseline-llm-rag --mode rag --corpus corpus-en.json --output "results/v0.1-${HW_TAG}-rag.json" 2>&1 | tail -3
echo "=== LLM strong-rag (HyDE + bge-reranker, ADR-0030) ==="
uv run baseline-llm-rag --mode strong-rag --corpus corpus-en.json --output "results/v0.1-${HW_TAG}-strong-rag.json" 2>&1 | tail -3

# ── List final result files ──────────────────────────────────
echo "=== Done. Result files: ==="
ls -lh results/v0.1-${HW_TAG}-*.json
