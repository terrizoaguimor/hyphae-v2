<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

# Hyphae v2

> *A cognitive substrate that answers grounded queries by verbatim
> quotation of hash-chained memory fragments — with no LLM in the
> cognition path, and a cryptographic audit relation between every
> output and its sources.*

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Spec: CC-BY-4.0](https://img.shields.io/badge/Spec-CC--BY--4.0-orange.svg)](https://creativecommons.org/licenses/by/4.0/)
[![Paper](https://img.shields.io/badge/paper-preprint-success.svg)](papers/arxiv-preprint/main.pdf)
[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.20436643.svg)](https://doi.org/10.5281/zenodo.20436643)

**TL;DR.** Hyphae's distinguishing property is *verifiable
provenance*: every emitted span is byte-identical to a fragment in a
SHA-256 hash-chained journal, so any answer can be independently
audited back to named, unaltered sources — a guarantee no
retrieval-augmented LLM provides by construction. Our
[preprint](papers/arxiv-preprint/main.pdf) measures this against 18
LLM configurations (six models × three retrieval modes) plus a
trivial *echo* control, on TriviaQA and a project corpus. The honest
headline: on standard correctness/grounding metrics a verbatim
`print` of the retrieved sentence matches Hyphae — those metrics
measure quotation, not architecture — but on a tamper-detection
experiment Hyphae detects and localises 100% of post-ingest store
tampering while echo and LLM-RAG detect 0%. The contribution is the
audit property, at microsecond, CPU-only cost.

## What this is

Hyphae v2 is a **Cognitive Language Model (CLM)**: an architecture that
produces language-equivalent output through compositional mechanisms
operating on structured fragments and explicit grammatical rules, **not**
through statistical token prediction over learned distributions. It runs
on commodity CPU + RAM, ships as a single Rust binary, and persists state
as cryptographically auditable storage.

It is the second articulation of the project. The first attempt
(`hyphae/` v1) validated the core architectural primitives but
accumulated scope creep that an underfunded solo-founder track could not
sustain. v2 is a deliberate restart with four corrections baked in
*before* the first commit, not as post-hoc patches:

1. **Ethics engine is first-class**, as a dedicated crate, with
   RADAR semantics (classify + audit + emit signals; never censor). In
   v1 it was distributed by omission across three subsystems and
   silently became a JAIL that bypassed the grounded retrieval path.
   See [`docs/adr/0003-ethics-radar-firstclass.md`](docs/adr/0003-ethics-radar-firstclass.md).

2. **Learning loop is first-class** from RFC v0.1, not deferred. The
   substrate specifies inmutable rules of cognitive composition; the
   learning loop refines parameters within those bounds, with audit
   trail and rollback. See
   [`docs/adr/0002-learning-loop-firstclass.md`](docs/adr/0002-learning-loop-firstclass.md).

3. **Scope discipline.** Six functional subsystems, not seventeen.
   Five native operations, not thirty-eight ported from a wrapper.
   EN-only for v0.1; multilingual when the bet validates. See
   [`docs/adr/0001-fresh-from-v1.md`](docs/adr/0001-fresh-from-v1.md).

4. **Cognition-path coverage from day one.** Ethics evaluation runs on
   every operation that can ingest, compose, or retrieve content
   (remember, recall/cascade, compose, grounded retrieval, learning
   updates). Not gated only on the write path the way v1 ended up.

## What this is *not*

Not a database. Not a vector store. Not a knowledge graph. Not a
simulation of neurons. Not a spiking neural network. Not a wrapper
around an LLM. There is no LLM invocation in the cognition path —
explicit Hard Commitment, not a marketing claim.

## Empirical results & paper

The preprint — [`papers/arxiv-preprint/main.pdf`](papers/arxiv-preprint/main.pdf),
source under [`papers/arxiv-preprint/`](papers/arxiv-preprint/) — is
a verifiable-provenance systems paper. Its results are fully
reproducible from this repo; the comparator lives in
[`bench/baseline-llm-rag/`](bench/baseline-llm-rag/) and every result
envelope is committed under `bench/baseline-llm-rag/results/`.

Three findings, in order of how the paper weighs them:

1. **Provenance is the contribution, and it is measured.** A
   tamper-detection experiment against the real hash-chained journal
   ([`crates/hyphae-storage/examples/tamper_detection.rs`](crates/hyphae-storage/examples/tamper_detection.rs))
   shows Hyphae detects and localises 100% of post-ingest store
   tampering to the exact sequence; echo and LLM-RAG have no journal
   and detect 0%. See [`docs/perf/`](docs/perf/) and §5 of the paper.

2. **The echo control bounds what the correctness benchmark can
   say.** A one-line verbatim `print` of the retrieved sentence ties
   or beats Hyphae on gold-answer match, NLI unsupported-claim rate,
   and n-gram overlap, on both corpora. Measured correctness is a
   property of verbatim quotation — shared by any echo — not of
   Hyphae's composition machinery. We therefore make no
   "more-correct-than-LLMs" claim.

3. **Latency is a corollary of not generating.** Hyphae's realizer
   runs in 2–24 µs mean per query (CPU-only, ~50 MB); the LLM
   baselines run in 1.8–6.1 s, five-to-six orders of magnitude
   slower — a consequence shared with the echo baseline, not an
   independent result.

Comparison writeups: [`docs/perf/triviaqa-comparison.md`](docs/perf/triviaqa-comparison.md),
[`multi-llm-comparison.md`](docs/perf/multi-llm-comparison.md),
[`ablation-study.md`](docs/perf/ablation-study.md),
[`hardware-matrix.md`](docs/perf/hardware-matrix.md),
[`baseline-comparison.md`](docs/perf/baseline-comparison.md).

## About the name

*Hyphae* are the filaments that compose mycelium — the underground
network through which fungi exchange nutrients and signals. The
project's parent (Celiums AI) takes its name from the same root.

When pressed for a backronym:
**H.Y.P.H.A.E. — Honest Yet Probably Helpful Auditable Engine.**
The hedge is intentional. A cognitive substrate that markets itself
as *indispensable* should be regarded with the same suspicion as any
other AI product that does so.

## On the prose style

If you run the chat REPL or the smoke binary, you will notice the
output reads **template-rigid** compared to LLM prose:

> *Drawing from working memory, "the migration completed at 14:02 UTC"
> Therefore, "the deploy succeeded on the first attempt" That is what
> working memory holds on this.*

That is **the architectural feature, not a defect**. ADR-0001
§"Hard Architectural Commitment 12":

> Composition uses fragment quotation + connective tissue, not novel
> language synthesis. Fragments are opaque content sources whose body
> text is preserved verbatim. The surface realizer generates only the
> structural prose connecting them. This boundary is load-bearing for
> the no-LLM-in-cognition-path claim.

A system that paraphrases its retrieved fragments is a system that
can hallucinate them. Hyphae **chose audit over polish**. Every
quoted body in the output is byte-identical to what was stored —
verifiable against the hash-chained journal. The "connective tissue"
between quotes is the only place the realizer composes; it draws
from a hand-curated lexicon (currently ~250 EN entries, ~128 ES) and
cascade-shape-projected discourse roles.

The trade-off is conscious: if you want a system that sounds like a
human improvising answers from your data, you want an LLM with
retrieval. If you want a system that surfaces your data with a
provable audit trail and never confabulates around it, that is what
Hyphae offers. The prose feels stiff because the architecture
refuses to lie about its sources.

## Status

**v0.1 — substrate implemented end-to-end, empirically evaluated,
tag `v0.1.0`.** 31 ADRs accepted, 331 tests passing, smoke binary
runs the full cognition path in one command. The substrate meets
every commitment in the living RFC at
[`docs/rfc/v1-living.md`](docs/rfc/v1-living.md); the empirical
comparison against LLM-RAG and the echo control, plus the
tamper-detection experiment, are landed (see *Empirical results*
above). The architectural bet remains untested in production
workloads. Open follow-ups: a community-scale provenance benchmark,
a reader-preference study of the template-rigid prose, and a
multi-hop benchmark column.

## What works in v0.1

`cargo run -p hyphae-smoke` exercises the full v0.1 surface in one
invocation:

| Capability | Evidence |
|---|---|
| Six functional subsystems | `input-gate`, `episodic`, `valence`, `composer`, `predictive`, `reward` — instantiated, state-machine-aware |
| Native operations | `ingest` (Remember), `recall_signal`, `compose_signal`, `propose_learning_update`, `journal_verify_chain` |
| Hash-chained journal + state store | SHA-256 chain, one per substrate, shared with the ethics audit (ADR-0003); `fjall` + `redb` persistence |
| Five-point ethics coverage | `Remember` / `Recall` / `Compose` / `LearningUpdate` active; `GroundedRetrieval` deferred per RFC §9 |
| Cascade activation in recall | ADR-0011 — `recall_signal → episodic.process → episodic.cascade` |
| Learning loop wakeup | ADR-0013 — `record_emission → stage_pending → propose_learning_update → apply_audited` |
| Surface realizer | 250-phrase EN lexicon, cascade-shape composition (ADR-0006), boundary smoothing (ADR-0007), two schemas |
| Eval harness | 34-query EN corpus (+5 ES), 9 dimensions, scorer sensitivity audit, honest caveats (ADR-0008/0009/0010) |
| LLM+RAG comparator | `bench/baseline-llm-rag/` — 6 models × 3 modes + echo control, NLI + gold-answer + tamper-detection scoring (ADR-0027–0031) |

The sensitivity audit verifies each scoring dimension detects its
failure mode by construction. The eval harness publishes honest
numbers without target thresholds — per ADR-0001's anti-greenwashing
discipline.

## Performance

A criterion-driven baseline is committed to
[`docs/perf/v0.2-baseline.md`](docs/perf/v0.2-baseline.md).
Headline numbers from the v0.2 measurements (laptop, release
profile, single-threaded):

- `compose`: 559 ns – 5.5 µs across working-set sizes 1–7
- `ingest`: 8.7 – 9.6 ms across stored populations 10 – 1000
- `recall`: 4.0 – 6.3 ms across stored populations 10 – 1000

Reproduce with `cargo bench -p hyphae-bench --bench substrate_ops`.
The perf doc records both dev and bench profiles for honest
comparison.

## How to run

```bash
cargo build --workspace
cargo test --workspace            # 331 tests
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p hyphae-smoke         # end-to-end demonstration

# Provenance tamper-detection experiment (real hash chain):
cargo run -p hyphae-storage --example tamper_detection

# Reproduce the LLM+RAG comparison (needs Python + a DO Inference key):
#   see bench/baseline-llm-rag/README.md
```

## Repository layout

```
hyphae-v2/
├── Cargo.toml              # workspace, edition 2024, MSRV 1.85
├── README.md               # this file
├── LICENSE / LICENSE-CC-BY-4.0   # Apache-2.0 (code) / CC BY 4.0 (docs)
├── crates/                 # the pure-Rust substrate (12 crates)
│   ├── hyphae-core/        # primitives: fragments, ids, journal types, cascade activation
│   ├── hyphae-ethics/      # RADAR engine, audit, five coverage points
│   ├── hyphae-storage/     # fjall hash-chained journal + redb state store
│   │   └── examples/tamper_detection.rs   # provenance experiment (§5 of paper)
│   ├── hyphae-substrate/   # state machine, pathway routing, integration boundary
│   ├── hyphae-subsystems/  # six functional subsystems
│   ├── hyphae-surface/     # realizer, lexicon, cascade-shape, smoothing
│   ├── hyphae-learning/    # parameter store, feedback, orchestrator
│   ├── hyphae-embed/       # hashing token embedder (no learned params)
│   ├── hyphae-eval/        # harness, corpus, scorers, sensitivity audit, exporters
│   ├── hyphae-bench/       # criterion latency benches
│   ├── hyphae-chat/        # interactive REPL
│   └── hyphae-smoke/       # end-to-end runner
├── bench/
│   └── baseline-llm-rag/   # LLM+RAG comparator (Python): vanilla/HyDE+rerank/oracle
│       ├── src/baseline_llm_rag/   # pipeline, DO-Inference + local backends, echo, scorers
│       └── results/        # committed result envelopes (52 JSON, both corpora)
├── papers/
│   └── arxiv-preprint/     # the preprint (LaTeX source + compiled PDF + Pareto figure)
└── docs/
    ├── rfc/v1-living.md    # the canonical specification, append-only
    ├── adr/                # 31 architectural decision records (0001–0031)
    └── perf/               # empirical writeups (baseline, multi-LLM, ablation,
                            #   hardware-matrix, TriviaQA, v0.2 latency baseline)
```

ADRs 0027–0031 cover the empirical program (LLM+RAG comparator,
strong-RAG, multi-LLM matrix, hardware matrix, standard-benchmark
corpus). ADR-0012 was reserved during the 5-point ethics audit but
never filed — the audit found coverage complete and the slot is left
intentionally vacant rather than reused.

## Why a v2 (and not a refactor of v1)

The v1 codebase is preserved at `hyphae/` for archival reference. It is
not abandoned — it is the empirical evidence that the substrate
primitives are correct and the scope discipline is hard. v2 is a fresh
repository because three of the four corrections above require
*architectural* changes that would have cascaded across v1's 145+
commits, 17 subsystem crates, 38 ported tools, and the RFC superseding
chain (v0.1.2 → v0.2 → v0.3 with patches). The cost of unwinding that
exceeded the cost of rebuilding from primitives. See ADR-0001.

## Motivation

The current trajectory of large-language-model development requires
increasingly massive data centers, GPU clusters, and energy
consumption. Microsoft has reactivated Three Mile Island. AWS is
building small modular reactors. Several actors are seriously planning
orbital data centers. Hyphae is an architectural bet that a category
of useful AI capability — coherent conversational interaction with
persistent memory, contextual reasoning, honest limitation
acknowledgment, and curiosity-driven learning — can be decoupled from
that infrastructure dependency.

The project is built in the conviction that meaningful alternatives to
the dominant trajectory are still possible, and that small, focused,
open work can contribute to discovering them. If the architectural bet
validates, an alternative exists. If it does not, the attempt is
documented, the failure is empirical signal, and the next attempt
learns from this one.

## Citation

Archived and citable via Zenodo (DOI:
[10.5281/zenodo.20436643](https://doi.org/10.5281/zenodo.20436643) —
concept DOI, always resolves to the latest version):

```bibtex
@misc{gutierrez2026hyphae,
  author       = {Guti{\'e}rrez, Mario},
  title        = {{Hash-Chained Verbatim Quotation: A Verifiable
                  Provenance Layer for Grounded Retrieval}},
  year         = {2026},
  publisher    = {Zenodo},
  doi          = {10.5281/zenodo.20436643},
  url          = {https://doi.org/10.5281/zenodo.20436643},
  note         = {Hyphae v2. Code, corpora, result envelopes, and
                  the preprint. \url{https://github.com/terrizoaguimor/hyphae-v2}}
}
```

(arXiv version forthcoming, pending category endorsement; the same
preprint source is in [`papers/arxiv-preprint/`](papers/arxiv-preprint/).)

## License

Hyphae v2 ships under a **dual-licensing scheme**:

- **Code** (Rust source, `Cargo.toml` configuration, CI workflows):
  [Apache License 2.0](LICENSE). Every code file carries an
  `SPDX-License-Identifier: Apache-2.0` header.
- **Specification and documentation** (`docs/rfc/`, `docs/adr/`,
  `docs/perf/`, and top-level Markdown files such as this README,
  `CHANGELOG.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`,
  `SECURITY.md`): [Creative Commons Attribution 4.0 International
  (CC BY 4.0)](LICENSE-CC-BY-4.0). Every documentation file
  carries an `SPDX-License-Identifier: CC-BY-4.0` header.

The full Apache 2.0 license text is in [`LICENSE`](LICENSE).
The CC BY 4.0 notice + the canonical-text link is in
[`LICENSE-CC-BY-4.0`](LICENSE-CC-BY-4.0). Contributors agree to
these terms by submitting; the project does not require a separate
Contributor License Agreement.

## Governance

BDFL (Benevolent Dictator For Life): Mario Gutiérrez. Celiums AI is
the commercial entity behind Hyphae open source. Pathway to broader
governance is documented in the living RFC §"Governance".

---

*Hyphae is a project of [Celiums AI](https://celiums.ai).*
