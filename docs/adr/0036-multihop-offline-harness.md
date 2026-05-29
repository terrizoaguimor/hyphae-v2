<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0036
title: Multi-hop evaluation column — offline harness
status: accepted
date: 2026-05-29
decision-makers: [mario]
triangulated-by: [claude-opus-4-8 (offline harness + reference answerers)]
followup-of: [0027, 0031]
---

# 0036 — Multi-hop evaluation column (offline harness)

## Context

The provenance thesis rests on *extractive* answering: every answer span
is a byte-identical quotation of one stored fragment. The paper's Future
Work (OPEN-01) names the honest limit of that design — **generalisation
beyond extractive tasks**. Multi-hop questions are the sharpest probe: a
genuine multi-hop answer must be *synthesised* across two or more
supporting facts, so no single quoted span can contain it. The open
question is behavioural: does a single-span verbatim system degrade
**gracefully** (detect it cannot answer and abstain) or **silently**
(emit a confident but wrong single-fragment quote)?

Running the full column needs external datasets (HotpotQA / MuSiQue) and
the live LLM+RAG comparator (DigitalOcean Inference) for the contrast
row — infrastructure not available in this pass. Mario asked to build
the **offline harness** now and leave the live run for when the infra is
ready.

## Decision

Add `bench/baseline-llm-rag/src/baseline_llm_rag/multihop.py`: the
complete offline scaffolding for the multi-hop column.

- **Normalised schema** (`MultiHopItem`, `Fragment`): a question, a gold
  answer, a small fragment store with `supporting` flags, and a
  ground-truth `n_hops`. `n_hops >= 2` is genuine multi-hop.
- **Dataset loaders** (`load_hotpotqa`, `load_musique`): lazy `datasets`
  imports that map each dataset into the normalised schema. Used when
  the data + infra are available; not exercised offline.
- **Bundled offline sample** (`offline_sample`): six hand-authored items
  (three single-hop, three 2-hop with distractors), so the harness runs
  end-to-end with no network, under the system interpreter (stdlib only
  on the offline path; `datasets`/`click` are imported lazily).
- **Two reference answerers** standing in for a single-span verbatim
  system: `naive_extractive_reference` (always emits the best-overlap
  fragment) and `extractive_reference` (abstains when no single fragment
  covers enough of the query's content terms — the only multi-hop signal
  available *without* peeking at the gold answer).
- **Scorer** (`score`): classifies by ground-truth `n_hops` and reports,
  over the multi-hop subset, `graceful_degradation_rate`,
  `silent_failure_rate`, and `lucky_synthesis_rate`, plus
  `answered_correct_rate_single_hop`. It scores **any** system's outputs
  (the references, or real Hyphae / LLM outputs) via the `SystemAnswer`
  schema `{id, abstained, answer, source_fragment_id}`.

### Offline result (the contrast)

`papers/arxiv-preprint/tables/multihop-offline.txt` (6-item sample):

| system | single-hop correct | graceful (multi-hop) | silent failure |
|---|---|---|---|
| naive (no abstention) | 1.0 | 0.0 | **1.0** |
| abstention on low coverage | 1.0 | **1.0** | 0.0 |

The finding the harness is built to measure: a single-span verbatim
system **silently fails on multi-hop by default**; an explicit
coverage-threshold abstention rule turns those silent failures into
graceful abstentions. Graceful degradation is achievable, but it is not
free — it requires an abstention signal the realizer must implement.

## What is NOT in this ADR (needs infra)

- The **live LLM+RAG comparator column** (DigitalOcean Inference): an
  LLM can synthesise across hops and answer, at the cost of byte-level
  provenance — the comparison that makes the column's point. Drop its
  outputs into the scorer when the keys/infra are available.
- The **full-dataset run** (HotpotQA / MuSiQue): the loaders are in
  place; running them needs the dataset download/cache.
- **Real Hyphae outputs** on the multi-hop corpus, via `hyphae-eval`,
  scored with the same `SystemAnswer` schema.

## Consequences

**Positive:**
- The multi-hop column's schema, metrics, sample, and scoring are
  complete and runnable today; the live run is a drop-in.
- The offline path adds no dependencies and runs without the uv
  environment (stdlib only).

**Negative:**
- The offline numbers come from reference stand-ins, not real systems.
  They demonstrate the *measurement*, not a result about Hyphae itself —
  that waits on the live run. Labelled as such in the table and module.

## Followups

- Wire `hyphae-eval` to emit `SystemAnswer` outputs over the multi-hop
  corpus; run the LLM comparator column; report the real contrast.
- Consider an abstention signal in the realizer if the live run shows
  silent failure (the harness makes that decision measurable).
