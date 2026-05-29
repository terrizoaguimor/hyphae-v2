<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0033
title: Append-only anchor ledger — freshness and non-equivocation
status: accepted
date: 2026-05-29
decision-makers: [mario]
triangulated-by: [claude-opus-4-8 (implementation + adversarial demonstration)]
supersedes-followup-of: 0032
---

# 0033 — Append-only anchor ledger

## Context

ADR-0032 anchors the journal's chain head with an Ed25519 signature
held outside the store, closing the chain-aware attacker who rewrites
the head: the signature over the legitimate head does not match the
forged one, and the attacker cannot re-sign without the key.

But a single signature pins only *a* valid head, not *the latest*
one. ADR-0032's own threat model named the residual gap, and the
provenance benchmark (`hyphae-provbench`, ADR-0031 lineage) made it
concrete in its `head_rollback` mode:

- **Freshness.** Every head the journal ever had was, at its time, a
  legitimately signed head. An attacker with store write access can
  roll the store back to an earlier consistent state and replay the
  *genuine but superseded* anchor for that earlier head. A lone
  signature check accepts it — the signature is real and matches the
  rolled-back head. Nothing in one signature says "this head is
  superseded".
- **Non-equivocation.** A single signature cannot stop the store (or a
  key holder) from presenting *different* consistent histories to
  different auditors; each history carries its own validly-signed
  head.

ADR-0032 listed "external append-only publication of anchors … so
freshness and non-equivocation hold across snapshots" as the natural
followup. This ADR implements it. It is also the paper's Future Work
item *"external anchor publication."*

## Decision

**Publish anchors to an append-only, itself-hash-chained ledger** — the
standard transparency-log shape (Haber–Stornetta 1991, Certificate
Transparency / RFC 6962). A new `hyphae-storage::ledger` module
provides:

- `LedgerEntry { epoch, head, prev_ledger_hash, signature }` — a
  monotonic `epoch`, the journal head at that epoch, the hash of the
  previous ledger entry (chaining the ledger itself), and an Ed25519
  signature over `(epoch ‖ head ‖ prev_ledger_hash)`. The entry's hash
  covers the signature too, binding the whole entry into the chain.
- `AnchorLedger` — the append-only list; `from_entries` lets an auditor
  rebuild it from the published log.
- `HeadAnchor::append_to_ledger(&mut ledger, head)` — signs and appends
  a new entry chaining the previous (the key still lives outside the
  store, per ADR-0032).
- `verify_ledger(&ledger, vk)` — epochs are `0,1,2,…`, each
  `prev_ledger_hash` chains the previous entry, every signature
  verifies.
- `verify_fresh_head(current_head, &ledger, vk)` — the journal's
  current head must equal the **latest** entry of a verified ledger.
- `ledgers_consistent(&a, &b)` — two views must be prefix-consistent;
  divergence at any epoch is equivocation.

### Why this closes the gaps

- **Freshness.** An auditor checks the journal head against the ledger
  *tail*, not against any matching signature. A rolled-back head sits
  at an earlier epoch, so `verify_fresh_head` rejects it even when the
  attacker replays that head's genuine anchor.
- **Non-equivocation.** Because the ledger is a single linear,
  hash-chained, signed log, two histories that diverge at an epoch are
  caught by `ledgers_consistent` (the shorter must be an exact prefix
  of the longer).
- **Ledger integrity.** Forging any published entry invalidates that
  entry's signature (and breaks the chain for all successors), so the
  published log is itself tamper-evident.

### Demonstration

`crates/hyphae-storage/examples/anchor_ledger.rs` builds a real journal,
anchors the head into the ledger after every append, then runs three
attacks. Output committed to
`papers/arxiv-preprint/tables/anchor-ledger.txt`:

| threat | single-head (0032) | ledger (0033) |
|---|---|---|
| chain-aware head rewrite | DETECTED | DETECTED |
| rollback + stale-anchor replay | **MISSED** | **DETECTED** |
| equivocation across views | n/a | DETECTED |
| forged published anchor | n/a | DETECTED |

Eight unit tests in `ledger.rs` cover well-formedness, freshness
(latest accepted / stale rejected), entry tampering, reordering,
truncation-as-consistent-prefix, equivocation detection, and prefix
consistency.

## Threat model after this ADR

- **Defended:** store-only edits (bare chain, ADR-0003); chain-aware
  recompute-and-rewrite-head (single anchor, ADR-0032); rollback with
  stale-anchor replay and equivocation across views (this ADR) — for
  any attacker who does not hold the anchor signing key.
- **Out of scope (deployment):**
  - **External witness of the ledger tail.** `ledgers_consistent`
    catches equivocation only when an auditor can compare the store's
    view against an independent one. A store that *withholds* later
    entries (presents a valid prefix and claims it is the latest) is
    caught only once the auditor obtains the true tail from a witness —
    a timestamp authority, gossiped signed-tree-head, or
    OpenTimestamps/Bitcoin anchor of `AnchorLedger::head_hash()`. The
    ledger is built to be witnessed; wiring the witness is deployment.
  - **Anchor-key rotation / KMS sourcing** (carried over from 0032).
  - **Liveness.** Deletion remains *detectable*, not *impossible*.

## Consequences

**Positive:**
- The paper's Future Work "external anchor publication" item is now
  implemented and demonstrated in-repo, not promised.
- No new dependencies: reuses `ed25519-dalek` and `sha2` already in the
  workspace. ~180 LOC + tests.

**Negative:**
- The witness (the piece that makes withholding detectable) is left to
  deployment; the in-repo guarantee assumes an auditor can obtain a
  second view of the tail. Documented above and in the module.

## Followups

- **External witness integration** (timestamp authority / gossiped
  tree head) of `AnchorLedger::head_hash()`.
- **Periodic anchoring policy** wired into the substrate checkpoint
  path (carried from 0032).
- **Anchor-key rotation** semantics (carried from 0032).
