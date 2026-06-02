<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# hyphae-provbench — a provenance benchmark (`provbench/v3`)

A realizer-independent benchmark that scores **verifiable-generation
systems** on the axis that actually distinguishes them: when stored,
quoted content is tampered with after the fact, is the tampering
**detectable**, and can it be **localised**? Correctness benchmarks
compare LLM-RAG systems on answer quality; this compares systems on
*provenance*, the property a trivial `echo` baseline shows is
**not** a function of answer quality at all.

This generalises the paper's minimal four-mode experiment
(`hyphae-storage/examples/tamper_detection.rs`) along the three axes
its Future Work section called for: a broader **tampering taxonomy**,
an **adversary-capability matrix**, and a **standard scoring
protocol**. It is the artifact that lets verifiable-generation systems
be compared the way correctness benchmarks compare LLM-RAG systems
today.

## Why it is realizer-independent

The benchmark never invokes a lexicon, a cascade, or any cognition
machinery. It measures the **storage layer** — verbatim bodies over a
hash-chained journal. That layer is shared *identically* by Hyphae and
by an `echo+journal` baseline, so the result is a property of the
addable provenance layer, not of any realizer. A system plugs in by
implementing the `ProvenanceSystem` trait (`src/system.rs`).

## What it measures

For every cell `(system × tampering-mode × adversary)`, over `trials`
independent seeded runs:

| Metric | Definition |
|---|---|
| **bare detection** | fraction of trials the shipped hash-chain `verify()` flagged a violation |
| **localisation** | of detected *inconsistent-by-construction* tampers, the fraction localised to the exact first-broken-link seq |
| **anchored detection** | fraction the external Ed25519 head anchor flagged (signature over the pre-tamper head) |
| **false-positive rate** | fraction of *untampered* control stores wrongly flagged (must be 0) |
| **mean scan fraction** | mean `(violation_seq + 1) / n` — a detection-latency proxy |

A `-1.0` ("n/a") marks a metric that does not apply to a cell (e.g.
localisation for a consistent-by-construction tamper, or anchored
detection for a system with no head).

## Systems under test (`src/systems/`, v3)

Four systems implement the `ProvenanceSystem` trait, so the matrix
compares provenance *designs*, not just one:

| system | design | detection | inclusion proof |
|---|---|---|---|
| **verbatim-journal** | SHA-256 flat hash chain (Hyphae, echo+journal) | full | `O(n)` |
| **merkle-log** | Merkle/CT transparency log (RFC 6962) | full (ties the chain) | `O(log n)` |
| **signed-entries** | per-entry Ed25519 signature, no chain | edits + forged inserts only | none |
| **echo-no-journal** | no provenance structure (negative control) | none | none |

The reading: `merkle-log` **matches** the flat chain on every detection
cell — provenance detection is a property of the append-only-log *class*
— and differs only on **proof cost** (`O(n)` vs `O(log n)`), a per-system
axis the envelope now reports. `signed-entries` **discriminates**: it
catches in-place edits and forged inserts but misses delete, reorder,
replay, and rollback — *signing is not chaining*. See ADR-0038.

## Tampering taxonomy (`src/tamper.rs`)

Ten modes spanning content mutation, structural edits, replay, and
freshness:

`edit`, `delete`, `insert`, `reorder`, `bitflip`, `truncate`,
`duplicate`, `timestamp_skew`, `head_rollback`, `batch`.

Each is applied either **store-only** (the surface operation, leaving
chain links stale) or **chain-aware** (recomputing every hash forward
and rewriting the head), per the adversary.

## Adversary-capability matrix (`src/adversary.rs`)

Capability is parameterised along orthogonal axes so systems are
compared against the *same* graded attacker, and the guarantee's
boundary is **visible in the result** rather than asserted:

- **store access** — write (every profile here); read/none tamper
  nothing and are omitted.
- **chain knowledge** — `naive` (in-place, links go stale) vs
  `chain-aware` (recompute forward + rewrite head).
- **key access** — whether the attacker holds the external anchor
  signing key. Holding it is the **boundary**: they can re-sign a
  forged head, so the anchor provides no protection — exactly the
  assumption the guarantee is drawn around ("any attacker who does not
  hold the anchor signing key").

Default profiles: `store-only`, `chain-aware`, `chain-aware+key`.

## Defense escalation (v2, `src/escalation.rs`)

The taxonomy matrix above scores the *journal layer* (bare chain +
single anchor). The escalation experiment scores the **full provenance
stack** — bare chain → external head anchor (ADR-0032) → append-only
ledger (ADR-0033) → external witness (ADR-0034) — against four attacks,
each constructed so that exactly the next defense layer up is the one
that catches it:

| attack | bare | anchor | ledger | witness |
|---|---|---|---|---|
| in-place edit | ✓ | | | |
| chain-aware head rewrite | | ✓ | ✓ | ✓ |
| rollback + stale-anchor replay | | | ✓ | ✓ |
| withholding (truncated ledger) | | | | ✓ |

The `bare` and `anchor` verdicts come from running the real
verbatim-journal system; the `ledger` and `witness` verdicts are the
shipped `hyphae-storage` checks. It is emitted as a companion artifact
(`*-escalation.json` / `.txt`) and is reproducible from `(n, seed)`.
Key compromise is recoverable via rotation (ADR-0035); source ingestion
is the one open boundary (ADR-0034).

## Expected result shape

- **store-only** → the bare chain detects and localises in-place
  tampering (100%); the anchor only fires when the head itself shifts
  (insert / duplicate / rollback).
- **chain-aware** → the bare chain is defeated (0%); the external
  Ed25519 head anchor catches it (100%).
- **chain-aware+key** → anchor key compromised → no protection (the
  boundary).
- **echo-no-journal** → 0% everywhere: no journal, no provenance. (An
  LLM-RAG baseline sits here too; its paraphrased output is not even
  byte-bindable to a source.)
- **head_rollback** → consistent by construction, so the bare chain
  cannot see it; the single-head anchor catches it via head mismatch.

## Running

```sh
cargo run -p hyphae-provbench --release -- \
    --n 128 --trials 3 --seed 42 \
    --json papers/arxiv-preprint/tables/provenance-benchmark.json \
    --table papers/arxiv-preprint/tables/provenance-benchmark.txt
```

The run is **fully reproducible** from `(n, trials, seed)`: corpus
content, tamper targets, and anchor keys all derive from the seed; the
JSON envelope embeds only metrics (no timestamps or hashes), so a
re-run with the same parameters is byte-identical. Detection results
are deterministic, so small `n`/`trials` already reproduce the headline
matrix; larger `n` exercises localisation at higher sequence numbers
and the scan-fraction distribution. Note the journal store fsyncs once
per ingest and once per tamper, so wall-clock grows with
`cells × trials`, not with answer-model latency.

## Versioning

`PROTOCOL_VERSION = "provbench/v3"`. Bump on any change to the scoring
semantics or the matrix so envelopes remain comparable across versions.
(v2 added the defense-escalation experiment; v3 added the `merkle-log`
and `signed-entries` systems and the inclusion-proof-cost axis.)

## Future work

- **Append-only anchor ledger — implemented (ADR-0033).** Beyond the
  single latest-head anchor, `hyphae-storage::ledger` now publishes
  anchors to an append-only, hash-chained Ed25519 ledger, adding
  **freshness** (a rolled-back head is rejected even with a genuine
  stale anchor) and **non-equivocation** (forked views are detected).
  See `crates/hyphae-storage/examples/anchor_ledger.rs` and
  `papers/arxiv-preprint/tables/anchor-ledger.txt`. The
  ledger-vs-single-anchor axis is now folded in as the v2 defense-
  escalation experiment (see above).
- **External witness — implemented (ADR-0034).** A store that
  *withholds* later ledger entries is now caught: an independent witness
  (separate key) signs the ledger tail, and the auditor pins the
  presented ledger to it (`hyphae-storage::witness`). In deployment the
  witness maps to a timestamp authority / transparency witness /
  OpenTimestamps commitment — wiring that network is the remaining
  deployment step.
- **Key rotation — implemented (ADR-0035).** A signed keyring rotates
  the anchor key safely: successors are authorized by their predecessor
  from a trusted root, a ledger spanning rotations verifies under the
  per-epoch active key, and a compromised key is recoverable (a retired
  key can sign no new epochs). `hyphae-storage::keyring`. What remains is
  KMS/HSM sourcing of the key material.
- **More systems.** Third-party verifiable-generation systems plug in
  via the `ProvenanceSystem` trait.

## License

Code under Apache-2.0 (workspace default); this document under
CC-BY-4.0.
