<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0038
title: provbench v3 — multi-system comparison and a proof-cost axis
status: accepted
date: 2026-06-02
decision-makers: [mario]
triangulated-by: [claude-opus-4-8 (design + implementation), deep-research workflow (single-system critique)]
followup-of: [0031, 0036]
---

# 0038 — provbench v3: multi-system comparison + proof-cost

## Context

A deep-research positioning pass flagged a real weakness in provbench:
through v2 it scored only Hyphae's own storage layer (`verbatim-journal`,
shared by Hyphae and `echo+journal`) against a negative control
(`echo-no-journal`). A reviewer would call it a **single-system
benchmark** — it could not show whether its matrix *discriminates*
provenance designs or merely re-measures one. The same pass surfaced the
recurring reviewer objection that the journal is a *flat* `O(n)` hash
chain where Certificate Transparency uses a Merkle tree with `O(log n)`
proofs (the open #3 question).

## Decision

Add two further systems-under-test and a proof-cost axis (v3).

**Systems** (all via the existing `ProvenanceSystem` trait):
- **`merkle-log`** — a Merkle/CT transparency log (RFC 6962 leaf/node
  hashing) over the fragment leaves; the root is the head. A faithful
  construction over `sha2`, no new dependency.
- **`signed-entries`** — a no-chain log where each entry is
  independently Ed25519-signed by the store.

**Proof-cost axis** — a new trait method
`inclusion_proof_hashes(n) -> Option<u64>`: the number of hashes an
auditor needs (beyond the trusted head) to prove one entry's inclusion
in, and consistency with, the head *without streaming the whole log*.
Reported per system in the envelope and table.

`PROTOCOL_VERSION` → `provbench/v3`.

## What the two systems reveal

The matrix now tells a design story it could not before:

**`merkle-log` matches `verbatim-journal` on detection** — store-only
tampering detected and localised (100%); chain-aware recompute defeats
the bare check (0%) but moves the root, so the external anchor catches
it (100%); head-rollback is consistent-by-construction. This is the
intended finding: **provenance detection is a property of the
append-only-log *class*, not of Hyphae's particular chain.** Where they
differ is the proof-cost axis — `O(n)` vs `O(log n)`:

| system | inclusion-proof hashes (n=128) |
|---|---|
| verbatim-journal (flat chain) | 128 |
| merkle-log (RFC 6962) | 7 |
| signed-entries | — (no membership proof) |
| echo-no-journal | — (no membership proof) |

This discharges the Merkle objection (#3) empirically: the flat chain is
a deliberate simplification with an `O(n)` proof cost; the Merkle log is
a drop-in for deployments that need sublinear inclusion proofs, and it
*does not change any detection result*.

**`signed-entries` discriminates** — signing each entry without a chain
catches in-place content edits and forged inserts (the signature
breaks / is absent), but **misses deletion, reordering, replay
(duplicate), and rollback**: the surviving entries still carry valid
signatures, and nothing commits to the set. It has no head (no anchored
detection) and offers no membership proof. This is the contrast that
proves the benchmark is not single-system: a different design produces a
genuinely different detection profile — **signing is not chaining.**

## Demonstration

`papers/arxiv-preprint/tables/provenance-benchmark.{json,txt}` (n=128,
trials=3) now reports all four systems plus the proof-cost table. Ten
unit tests cover the new systems: `signed_entries_discriminates`
(edit/insert detected, delete/reorder/duplicate/rollback missed) and
`merkle_log_matches_chain_profile` (store-only detected+localised;
chain-aware bare-clean but root moved), plus a smoke assertion that
proof costs are `n` / `ceil(log2 n)` / none / none and that merkle ties
the journal on a representative cell.

## Consequences

**Positive:**
- provbench is no longer a single-system benchmark: it both
  *discriminates* (signed-entries) and *confirms class-robustness*
  (merkle-log), and measures a new design axis (proof cost).
- Discharges the Merkle (#3) objection with an in-repo comparator rather
  than prose.
- One new (workspace-pinned) dependency surface item: `sha2` for
  hyphae-provbench (already in the workspace; RFC 6962 hashing).

**Negative:**
- The detection matrix is now 4 systems × 10 modes × 3 adversaries; the
  journal system still dominates wall-clock via fjall fsync, but the
  file-based merkle/signed/echo systems are cheap.

## Followups

- A consistency-proof / append-only-proof cost (not just inclusion).
- An external real-system comparator (e.g. git's content-addressed DAG)
  for maximal "not-my-own-design" credibility — deferred.
