# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Celiums Solutions LLC
"""Multi-hop evaluation harness (ADR-0036) — the OFFLINE scaffolding.

The provenance thesis rests on *extractive* answering: every answer span
is a byte-identical quotation of one stored fragment. That is exactly
why multi-hop questions — whose answer must be *synthesised* across two
or more sources — are the honest stress test. A single verbatim span
cannot contain a composed answer, so the open question (paper Future
Work, OPEN-01) is whether such a system degrades **gracefully**
(detects it cannot answer and abstains) or **silently** (emits a
confident but wrong single-fragment quote).

This module is the offline harness for that column:

  * a normalised multi-hop corpus schema (`MultiHopItem`),
  * loaders for HotpotQA / MuSiQue (lazy `datasets` import — used when
    the data + infra are available),
  * a small **bundled offline sample** so the harness runs end-to-end
    with no network,
  * an *extractive reference answerer* with an abstention heuristic that
    stands in for a single-span verbatim system, and
  * a scorer that computes the graceful-vs-silent metrics from ANY
    system's outputs (the reference here, or real Hyphae / LLM outputs
    once those are produced).

What is deliberately NOT here (it needs infra, see ADR-0036): the live
LLM+RAG comparator column (DigitalOcean Inference) and the full-dataset
download. The schema, metrics, and offline sample are complete and
runnable today; plugging in real outputs is a drop-in.

The offline path uses only the standard library, so it runs under the
system interpreter without the uv environment.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

PROTOCOL = "multihop/v1-offline"

# A tiny stopword set — enough to make token overlap meaningful without
# a dependency.
_STOP = {
    "the", "a", "an", "of", "in", "on", "at", "to", "for", "and", "or",
    "is", "was", "were", "are", "be", "by", "with", "which", "who", "what",
    "where", "when", "whom", "that", "this", "from", "as", "did", "does",
    "do", "it", "its", "his", "her", "their",
}


def _tokens(text: str) -> list[str]:
    return [t for t in re.split(r"[^a-z0-9]+", text.lower()) if t and t not in _STOP]


# ── Normalised corpus schema ──────────────────────────────────────────


@dataclass
class Fragment:
    """One stored fragment. `supporting` marks a gold supporting fact."""

    id: str
    body: str
    supporting: bool = False


@dataclass
class MultiHopItem:
    """A multi-hop question over a small fragment store.

    `n_hops` is the number of supporting fragments the gold answer needs.
    `n_hops >= 2` is a genuine multi-hop item: no single fragment can
    contain the synthesised answer verbatim.
    """

    id: str
    query: str
    gold_answer: str
    fragments: list[Fragment]
    n_hops: int

    def gold_single_span_coverable(self) -> bool:
        """True iff some single fragment contains the gold answer verbatim
        — the structural precondition for an extractive system to answer
        at all."""
        g = self.gold_answer.lower()
        return any(g in f.body.lower() for f in self.fragments)


# ── Bundled offline sample (no network) ───────────────────────────────


def offline_sample() -> list[MultiHopItem]:
    """A hand-authored sample: single-hop items (answerable by one
    fragment) interleaved with genuine 2-hop items (answer split across
    two fragments). Distractors are mixed in so retrieval is non-trivial."""
    return [
        MultiHopItem(
            id="sh-1",
            query="Who directed the film Polaris Drift?",
            gold_answer="Lena Ortiz",
            n_hops=1,
            fragments=[
                Fragment("f1", "The film Polaris Drift was directed by Lena Ortiz.", True),
                Fragment("f2", "Polaris Drift premiered at the Reykjavik festival in 2031."),
                Fragment("f3", "The Aurora Line is a subway running under the old harbor."),
            ],
        ),
        MultiHopItem(
            id="mh-1",
            query="In which city was the director of Polaris Drift born?",
            gold_answer="Valparaiso",
            n_hops=2,
            fragments=[
                Fragment("f1", "The film Polaris Drift was directed by Lena Ortiz.", True),
                Fragment("f2", "Lena Ortiz was born in Valparaiso in 1989.", True),
                Fragment("f3", "Valparaiso is a port city on the Pacific coast of Chile."),
                Fragment("f4", "Polaris Drift was shot mostly on location in Iceland."),
            ],
        ),
        MultiHopItem(
            id="sh-2",
            query="What is the capital of the Republic of Marenia?",
            gold_answer="Talvik",
            n_hops=1,
            fragments=[
                Fragment("f1", "The capital of the Republic of Marenia is Talvik.", True),
                Fragment("f2", "Marenia adopted its current constitution in 2014."),
            ],
        ),
        MultiHopItem(
            id="mh-2",
            query="What currency is used in the country whose capital is Talvik?",
            gold_answer="the maren",
            n_hops=2,
            fragments=[
                Fragment("f1", "The capital of the Republic of Marenia is Talvik.", True),
                Fragment("f2", "The official currency of Marenia is the maren.", True),
                Fragment("f3", "Talvik sits at the mouth of the Sild river."),
                Fragment("f4", "The marbehn dialect is spoken in the northern provinces."),
            ],
        ),
        MultiHopItem(
            id="mh-3",
            query="How tall is the tower designed by the architect of the Meridian Library?",
            gold_answer="312 metres",
            n_hops=2,
            fragments=[
                Fragment("f1", "The Meridian Library was designed by architect Sora Voss.", True),
                Fragment("f2", "Sora Voss also designed the Helix Tower, which is 312 metres tall.", True),
                Fragment("f3", "The Meridian Library holds over two million volumes."),
                Fragment("f4", "The Helix Tower opened to the public in 2028."),
            ],
        ),
        MultiHopItem(
            id="sh-3",
            query="How tall is the Helix Tower?",
            gold_answer="312 metres",
            n_hops=1,
            fragments=[
                Fragment("f1", "Sora Voss also designed the Helix Tower, which is 312 metres tall.", True),
                Fragment("f2", "The Helix Tower opened to the public in 2028."),
            ],
        ),
    ]


# ── Optional dataset loaders (need `datasets`; used with real infra) ───


def load_hotpotqa(n: int) -> list[MultiHopItem]:
    """Load `n` HotpotQA items (distractor setting) into the normalised
    schema. Requires the `datasets` package and network/cache; not
    exercised in the offline path. See ADR-0036."""
    from datasets import load_dataset  # lazy: only when actually used

    ds = load_dataset("hotpot_qa", "distractor", split=f"validation[:{n}]")
    items: list[MultiHopItem] = []
    for i, ex in enumerate(ds):
        titles = ex["context"]["title"]
        sentences = ex["context"]["sentences"]
        support_titles = set(ex["supporting_facts"]["title"])
        frags: list[Fragment] = []
        for j, (title, sents) in enumerate(zip(titles, sentences)):
            body = " ".join(sents).strip()
            frags.append(Fragment(f"f{j}", f"{title}: {body}", title in support_titles))
        items.append(
            MultiHopItem(
                id=f"hotpot-{i}",
                query=ex["question"],
                gold_answer=ex["answer"],
                fragments=frags,
                n_hops=max(1, len(support_titles)),
            )
        )
    return items


def load_musique(n: int) -> list[MultiHopItem]:
    """Load `n` MuSiQue answerable items into the normalised schema.
    Requires `datasets`; not exercised offline. See ADR-0036."""
    from datasets import load_dataset

    ds = load_dataset("dgslibisey/MuSiQue", split=f"validation[:{n}]")
    items: list[MultiHopItem] = []
    for i, ex in enumerate(ds):
        frags = [
            Fragment(f"f{j}", f"{p['title']}: {p['paragraph_text']}", bool(p.get("is_supporting")))
            for j, p in enumerate(ex["paragraphs"])
        ]
        n_hops = sum(1 for f in frags if f.supporting)
        items.append(
            MultiHopItem(
                id=f"musique-{i}",
                query=ex["question"],
                gold_answer=ex["answer"],
                fragments=frags,
                n_hops=max(1, n_hops),
            )
        )
    return items


# ── Extractive reference answerer (stand-in for a verbatim system) ─────


@dataclass
class SystemAnswer:
    """What a system returned for one item."""

    id: str
    abstained: bool
    answer: str
    source_fragment_id: str | None


def extractive_reference(item: MultiHopItem, coverage_threshold: float = 0.6) -> SystemAnswer:
    """Model a single-span verbatim system: retrieve the best-overlap
    fragment and quote it — but ABSTAIN when no single fragment covers
    enough of the question's content terms, the only multi-hop signal a
    verbatim system has *without* knowing the gold answer.

    This is the honest stand-in: it never peeks at `gold_answer`. The
    abstention heuristic fires precisely when the query's terms are
    spread across fragments (a multi-hop fingerprint)."""
    q = set(_tokens(item.query))
    if not q:
        return SystemAnswer(item.id, True, "", None)

    best_frag = None
    best_cov = 0.0
    for f in item.fragments:
        ftoks = set(_tokens(f.body))
        cov = len(q & ftoks) / len(q)
        if cov > best_cov:
            best_cov = cov
            best_frag = f

    if best_frag is None or best_cov < coverage_threshold:
        # No single fragment covers the question well enough -> abstain.
        return SystemAnswer(item.id, True, "", None)
    return SystemAnswer(item.id, False, best_frag.body, best_frag.id)


def naive_extractive_reference(item: MultiHopItem) -> SystemAnswer:
    """A single-span verbatim system with NO abstention signal: it always
    emits the best-overlap fragment. On multi-hop questions the best match
    is the bridge (first-hop) fragment, which lacks the composed answer —
    so this reference silently fails. It is the contrast that shows
    graceful degradation requires an *explicit* abstention rule."""
    q = set(_tokens(item.query))
    best_frag = None
    best_cov = -1.0
    for f in item.fragments:
        cov = len(q & set(_tokens(f.body))) / len(q) if q else 0.0
        if cov > best_cov:
            best_cov = cov
            best_frag = f
    if best_frag is None:
        return SystemAnswer(item.id, True, "", None)
    return SystemAnswer(item.id, False, best_frag.body, best_frag.id)


# ── Scoring (works for ANY system's outputs) ───────────────────────────


def _correct(answer: str, gold: str) -> bool:
    return gold.lower() in answer.lower()


def score(items: list[MultiHopItem], answers: dict[str, SystemAnswer]) -> dict[str, Any]:
    """Compute the graceful-vs-silent metrics, classified by the
    dataset's ground-truth hop count.

    A single-span verbatim system can, in principle, answer a *single-hop*
    item (retrieve the one fragment that holds the answer). A genuine
    *multi-hop* item (`n_hops >= 2`) requires composing two or more
    supporting facts, which no single quoted span can do — so the
    question is purely behavioural:
      * graceful_degradation_rate — abstained (the safe behaviour)
      * silent_failure_rate       — emitted a confident wrong span
      * lucky_synthesis_rate      — emitted a span that happened to hold
                                    the gold answer (no real synthesis)
    """
    single_hop = [it for it in items if it.n_hops < 2]
    multi_hop = [it for it in items if it.n_hops >= 2]

    answered_correct = sum(
        1
        for it in single_hop
        if (a := answers.get(it.id)) and not a.abstained and _correct(a.answer, it.gold_answer)
    )

    graceful = silent_failure = lucky = 0
    for it in multi_hop:
        a = answers.get(it.id)
        if a is None or a.abstained:
            graceful += 1
        elif _correct(a.answer, it.gold_answer):
            lucky += 1
        else:
            silent_failure += 1

    def rate(x: int, d: int) -> float:
        return round(x / d, 4) if d else -1.0

    return {
        "protocol": PROTOCOL,
        "n_items": len(items),
        "n_single_hop": len(single_hop),
        "n_multi_hop": len(multi_hop),
        "gold_appears_in_some_fragment_rate": rate(
            sum(1 for it in items if it.gold_single_span_coverable()), len(items)
        ),
        "answered_correct_rate_single_hop": rate(answered_correct, len(single_hop)),
        "graceful_degradation_rate": rate(graceful, len(multi_hop)),
        "silent_failure_rate": rate(silent_failure, len(multi_hop)),
        "lucky_synthesis_rate": rate(lucky, len(multi_hop)),
    }


def _summary_block(name: str, s: dict[str, Any]) -> list[str]:
    return [
        f"## system: {name}",
        f"  answered_correct (single-hop)     {s['answered_correct_rate_single_hop']}",
        f"  GRACEFUL degradation (abstain)    {s['graceful_degradation_rate']}",
        f"  SILENT failure (wrong quote)      {s['silent_failure_rate']}",
        f"  lucky synthesis                   {s['lucky_synthesis_rate']}",
        "",
    ]


def render_table(summary: dict[str, Any]) -> str:
    """Render a single system's summary."""
    lines = [
        f"# Multi-hop offline harness — {summary['protocol']}",
        "# OPEN-01: does a single-span verbatim system degrade gracefully",
        "# (abstain) or silently (wrong quote) on multi-hop questions?",
        "",
        f"  items {summary['n_items']}  single-hop {summary['n_single_hop']}  "
        f"multi-hop {summary['n_multi_hop']}",
        "",
        *_summary_block("(scored)", summary),
        "# graceful + silent + lucky = 1 over the multi-hop subset.",
        "# Drop in real Hyphae / LLM outputs (same SystemAnswer schema)",
        "# for the live column. ADR-0036.",
    ]
    return "\n".join(lines) + "\n"


def render_offline_report(report: dict[str, Any]) -> str:
    """Render the two-reference contrast (naive vs abstention)."""
    any_s = next(iter(report.values()))
    lines = [
        f"# Multi-hop offline harness — {any_s['protocol']}",
        "# OPEN-01: does a single-span verbatim system degrade gracefully",
        "# (abstain) or silently (wrong quote) on multi-hop questions?",
        "",
        f"  items {any_s['n_items']}  single-hop {any_s['n_single_hop']}  "
        f"multi-hop {any_s['n_multi_hop']}",
        "",
    ]
    for name, s in report.items():
        lines += _summary_block(name, s)
    lines += [
        "# Reading: the single-span system silently fails on multi-hop",
        "# WITHOUT an abstention signal; a coverage-threshold abstention",
        "# rule turns those silent failures into graceful abstentions.",
        "# These are reference stand-ins — drop in real Hyphae / LLM",
        "# outputs (same SystemAnswer schema) for the live column. ADR-0036.",
    ]
    return "\n".join(lines) + "\n"


def run_references(items: list[MultiHopItem]) -> dict[str, Any]:
    """Score BOTH reference answerers over `items`, to show the contrast
    that answers OPEN-01: a naive single-span system silently fails on
    multi-hop, while an abstention-equipped one degrades gracefully."""
    naive = {it.id: naive_extractive_reference(it) for it in items}
    abstaining = {it.id: extractive_reference(it) for it in items}
    return {
        "naive_no_abstention": score(items, naive),
        "abstention_on_low_coverage": score(items, abstaining),
    }


def run_offline() -> dict[str, Any]:
    """Run both reference answerers over the bundled offline sample."""
    return run_references(offline_sample())


def load_external_answers(path: Path) -> dict[str, SystemAnswer]:
    """Load a system's outputs (real Hyphae / LLM) for scoring. JSON: a
    list of {id, abstained, answer, source_fragment_id}."""
    raw = json.loads(Path(path).read_text())
    return {
        r["id"]: SystemAnswer(
            r["id"], bool(r.get("abstained", False)), r.get("answer", ""), r.get("source_fragment_id")
        )
        for r in raw
    }


def _cli() -> None:
    import click  # imported here so the offline path needs only stdlib

    @click.command()
    @click.option("--offline-sample", "offline", is_flag=True, help="Run the bundled offline sample.")
    @click.option("--dataset", type=click.Choice(["hotpotqa", "musique"]), default=None)
    @click.option("--n", type=int, default=50, help="Items to load from --dataset.")
    @click.option("--answers", type=click.Path(exists=True), default=None, help="External system outputs JSON.")
    @click.option("--json-out", type=click.Path(), default=None)
    def main(offline: bool, dataset: str | None, n: int, answers: str | None, json_out: str | None) -> None:
        if dataset == "hotpotqa":
            items = load_hotpotqa(n)
        elif dataset == "musique":
            items = load_musique(n)
        else:
            items = offline_sample()

        if answers:
            # Score one real system's outputs.
            summary = score(items, load_external_answers(Path(answers)))
            out = summary
            print(render_table(summary))
        else:
            # Show the naive-vs-abstention contrast using the references.
            out = run_references(items)
            print(render_offline_report(out))
        if json_out:
            Path(json_out).write_text(json.dumps(out, indent=2) + "\n")
            click.echo(f"wrote {json_out}", err=True)

    main()


if __name__ == "__main__":
    _cli()
