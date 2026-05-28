#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Celiums Solutions LLC
"""Build the Pareto-frontier figure for the arXiv preprint."""

from __future__ import annotations

import json
import sys
from pathlib import Path

try:
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
except ImportError:
    print("ERROR: matplotlib required. Install via `pip install matplotlib`.", file=sys.stderr)
    sys.exit(1)


HERE = Path(__file__).resolve().parent
PAPER_DIR = HERE.parent
REPO_ROOT = PAPER_DIR.parent.parent
RESULTS_DIR = REPO_ROOT / "bench" / "baseline-llm-rag" / "results"
OUT = PAPER_DIR / "figures" / "pareto-frontier.pdf"


def load_aggregate(
    name: str, path: Path, y_metric: str = "unsupported_claim_rate_filtered_mean"
) -> tuple[str, float, float]:
    with path.open() as f:
        agg = json.load(f)["aggregate"]
    return (
        name,
        max(agg.get("latency_p50_ms") or 0.01, 0.001),
        agg.get(y_metric) or 0.0,
    )


def is_dominated(
    point: tuple[float, float], others: list[tuple[float, float]], *, lower_better: bool = True
) -> bool:
    """Strict Pareto dominance. lower_better=True for unsup_f (minimise);
    False for gold-answer (maximise)."""
    x, y = point
    for ox, oy in others:
        if lower_better:
            if ox <= x and oy <= y and (ox < x or oy < y):
                return True
        else:
            if ox <= x and oy >= y and (ox < x or oy > y):
                return True
    return False


def collect(
    label_pairs: list[tuple[str, str]],
    y_metric: str = "unsupported_claim_rate_filtered_mean",
) -> list[tuple[str, float, float]]:
    out = []
    for label, fname in label_pairs:
        p = RESULTS_DIR / fname
        if not p.exists():
            print(f"  warn: missing {p}", file=sys.stderr)
            continue
        out.append(load_aggregate(label, p, y_metric=y_metric))
    return out


OWN_CORPUS = [
    ("echo", "v0.1-laptop-echo-own.json"),
    ("Hyphae", "v0.1-laptop-hyphae-none.json"),
    ("Llama-8B oracle", "v0.1-laptop-oracle.json"),
    ("Llama-8B rag", "v0.1-laptop-rag.json"),
    ("Llama-8B strong-rag", "v0.1-laptop-strong-rag.json"),
    ("Llama-70B oracle", "v0.1-doinf-llama3.3-70b-instruct-oracle.json"),
    ("Llama-70B rag", "v0.1-doinf-llama3.3-70b-instruct-rag.json"),
    ("Llama-70B strong-rag", "v0.1-doinf-llama3.3-70b-instruct-strong-rag.json"),
    ("Claude oracle", "v0.1-doinf-anthropic-claude-4.6-sonnet-oracle.json"),
    ("Claude rag", "v0.1-doinf-anthropic-claude-4.6-sonnet-rag.json"),
    ("Claude strong-rag", "v0.1-doinf-anthropic-claude-4.6-sonnet-strong-rag.json"),
    ("GPT-4.1 oracle", "v0.1-doinf-openai-gpt-4.1-oracle.json"),
    ("GPT-4.1 rag", "v0.1-doinf-openai-gpt-4.1-rag.json"),
    ("GPT-4.1 strong-rag", "v0.1-doinf-openai-gpt-4.1-strong-rag.json"),
    ("DeepSeek oracle", "v0.1-doinf-deepseek-v4-pro-oracle.json"),
    ("DeepSeek rag", "v0.1-doinf-deepseek-v4-pro-rag.json"),
    ("DeepSeek strong-rag", "v0.1-doinf-deepseek-v4-pro-strong-rag.json"),
    ("Atlas oracle", "v0.1-doinf-router-celiums-conversation-oracle.json"),
    ("Atlas rag", "v0.1-doinf-router-celiums-conversation-rag.json"),
    ("Atlas strong-rag", "v0.1-doinf-router-celiums-conversation-strong-rag.json"),
]

TRIVIAQA_CORPUS = [
    ("echo", "v0.1-laptop-triviaqa-echo.json"),
    ("Hyphae", "v0.1-laptop-triviaqa-hyphae-none.json"),
    ("Llama-8B oracle", "v0.1-laptop-triviaqa-oracle.json"),
    ("Llama-8B rag", "v0.1-laptop-triviaqa-rag.json"),
    ("Llama-8B strong-rag", "v0.1-laptop-triviaqa-strong-rag.json"),
    ("Llama-70B oracle", "v0.1-laptop-triviaqa-doinf-llama3.3-70b-instruct-oracle.json"),
    ("Llama-70B rag", "v0.1-laptop-triviaqa-doinf-llama3.3-70b-instruct-rag.json"),
    ("Llama-70B strong-rag", "v0.1-laptop-triviaqa-doinf-llama3.3-70b-instruct-strong-rag.json"),
    ("Claude oracle", "v0.1-laptop-triviaqa-doinf-anthropic-claude-4.6-sonnet-oracle.json"),
    ("Claude rag", "v0.1-laptop-triviaqa-doinf-anthropic-claude-4.6-sonnet-rag.json"),
    ("Claude strong-rag", "v0.1-laptop-triviaqa-doinf-anthropic-claude-4.6-sonnet-strong-rag.json"),
    ("GPT-4.1 oracle", "v0.1-laptop-triviaqa-doinf-openai-gpt-4.1-oracle.json"),
    ("GPT-4.1 rag", "v0.1-laptop-triviaqa-doinf-openai-gpt-4.1-rag.json"),
    ("GPT-4.1 strong-rag", "v0.1-laptop-triviaqa-doinf-openai-gpt-4.1-strong-rag.json"),
    ("DeepSeek oracle", "v0.1-laptop-triviaqa-doinf-deepseek-v4-pro-oracle.json"),
    ("DeepSeek rag", "v0.1-laptop-triviaqa-doinf-deepseek-v4-pro-rag.json"),
    ("DeepSeek strong-rag", "v0.1-laptop-triviaqa-doinf-deepseek-v4-pro-strong-rag.json"),
    ("Atlas oracle", "v0.1-laptop-triviaqa-doinf-router-celiums-conversation-oracle.json"),
    ("Atlas rag", "v0.1-laptop-triviaqa-doinf-router-celiums-conversation-rag.json"),
    ("Atlas strong-rag", "v0.1-laptop-triviaqa-doinf-router-celiums-conversation-strong-rag.json"),
]


def plot_pareto(
    ax,
    points: list[tuple[str, float, float]],
    title: str,
    *,
    y_label: str = "unsupported_claim_rate_filtered (lower better)",
    lower_better: bool = True,
):
    pts_only = [(x, y) for _, x, y in points]
    non_dom = []
    for i, (label, x, y) in enumerate(points):
        others = [pts_only[j] for j in range(len(pts_only)) if j != i]
        if not is_dominated((x, y), others, lower_better=lower_better):
            non_dom.append((label, x, y))
    non_dom.sort(key=lambda t: t[1])

    xs = [x for _, x, _ in points]
    ys = [y for _, _, y in points]
    ax.scatter(xs, ys, c="lightgray", s=24, edgecolor="gray", zorder=2, label="Dominated")

    fx = [x for _, x, _ in non_dom]
    fy = [y for _, _, y in non_dom]
    ax.scatter(fx, fy, c="C3", s=80, edgecolor="black", zorder=5, label="Pareto frontier")

    for label, x, y in points:
        if label == "Hyphae" or (label, x, y) in non_dom:
            ax.annotate(
                label, (x, y),
                xytext=(8, 5), textcoords="offset points",
                fontsize=8,
                fontweight="bold" if label == "Hyphae" else "normal",
            )

    ax.set_xscale("log")
    ax.set_xlabel("Latency p50 (ms, log scale)")
    ax.set_ylabel(y_label)
    ax.set_title(title)
    ax.grid(True, which="both", alpha=0.3, zorder=1)
    ax.set_ylim(-0.05, 1.05)
    ax.legend(loc=("lower left" if not lower_better else "upper left"), fontsize=8)


def main():
    fig, axes = plt.subplots(2, 2, figsize=(11, 8.5))
    # Top row: own corpus + TriviaQA on unsup_f (the v1 metric, kept for context)
    own = collect(OWN_CORPUS)
    plot_pareto(axes[0, 0], own, "Own corpus -- unsup_f")
    triviaqa = collect(TRIVIAQA_CORPUS)
    plot_pareto(axes[0, 1], triviaqa, "TriviaQA-150 -- unsup_f")
    # Bottom row: TriviaQA on gold-answer (higher better) -- the v2 metric.
    # Only TriviaQA has gold-answer (own corpus has no reference answers).
    triviaqa_gold = collect(TRIVIAQA_CORPUS, y_metric="gold_answer_match_rate_mean")
    plot_pareto(
        axes[1, 0], triviaqa_gold,
        "TriviaQA-150 -- gold-answer (higher better)",
        y_label="gold_answer_match_rate (higher better)",
        lower_better=False,
    )
    # Bottom-right: empty / annotation
    axes[1, 1].axis("off")
    axes[1, 1].text(
        0.05, 0.5,
        "Bottom-left panel uses gold-answer match\n"
        "as the correctness axis (higher = better);\n"
        "unsup_f is metric-degenerate on TriviaQA\n"
        "(Hyphae 0.000 by quotient).\n\n"
        "The decisive feature of every panel: the\n"
        "echo baseline is co-located with Hyphae at\n"
        "the favourable corner (and dominates it on\n"
        "the own-corpus unsup_f panel). The metric-\n"
        "space frontier does NOT separate the\n"
        "architecture from a verbatim print(). The\n"
        "distinguishing property -- tamper-evident\n"
        "provenance (Table 3) -- is not an axis any\n"
        "of these panels can plot.",
        fontsize=10, va="center",
    )
    plt.tight_layout()
    OUT.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(OUT, bbox_inches="tight")
    print(f"Wrote {OUT}")


if __name__ == "__main__":
    main()
