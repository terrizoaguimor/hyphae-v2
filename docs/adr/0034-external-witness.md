<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0034
title: External witness of the anchor ledger — closing withholding
status: accepted
date: 2026-05-29
decision-makers: [mario]
triangulated-by: [claude-opus-4-8 (implementation + adversarial demonstration)]
followup-of: 0033
---

# 0034 — External witness of the anchor ledger

## Context

ADR-0033 published anchored heads to an append-only, hash-chained
ledger, adding **freshness** (`verify_fresh_head` rejects a rolled-back
head, since it is not the ledger tail) and **non-equivocation**
(`ledgers_consistent` rejects forked views). Its threat model named the
one residual gap explicitly:

> `ledgers_consistent` catches equivocation only when an auditor can
> compare the store's view against an independent one. A store that
> *withholds* later entries (presents a valid prefix and claims it is
> the latest) is caught only once the auditor obtains the true tail
> from a witness.

This is the withholding attack. A store rolls the journal back to an
earlier head **and** truncates the ledger to the matching prefix. The
prefix is internally valid; its tail equals the rolled-back head; so
`verify_fresh_head` accepts it. Nothing the store hands the auditor
reveals that more entries ever existed. Mario picked closing this as
the next deliverable.

## Decision

**Introduce an external witness: an independent party that observes the
ledger tail and signs an attestation, against which the auditor pins how
far the ledger really went.** A new `hyphae-storage::witness` module
provides:

- `WitnessAttestation { epoch, ledger_head_hash, signature }` — a
  signed statement "at this point the ledger reached `epoch` with this
  head", over `(epoch ‖ ledger_head_hash)`.
- `Witness` — holds a signing key **separate from the anchor's**
  (`from_seed` for reproducible tests; in deployment an external
  service). `observe(&ledger)` signs the current tail.
- `verify_against_witness(&ledger, &attestation, anchor_vk, witness_vk)`
  — true only when the ledger is well-formed under the anchor key, the
  attestation verifies under the witness key, the ledger **contains the
  witnessed epoch**, and its head *at that epoch* matches the witnessed
  value.
- `verify_fresh_against_witness(...)` — the full auditor check:
  `verify_fresh_head` **and** `verify_against_witness`.

### Why this closes withholding

The witness attestation is a **floor** on the ledger the store cannot
lower. To pass, the presented ledger must reach the witnessed epoch:

- a **withheld / truncated** ledger is shorter than the witnessed epoch
  → `entries().get(epoch)` is `None` → rejected;
- a **fork before** the witnessed epoch reaches it with a different head
  → entry hash mismatch → rejected;
- a ledger that **grew past** the witnessed epoch still passes — earlier
  entries are immutable in an append-only log.

The independence of the witness key is load-bearing: a store that held
both the anchor key and the witness key could forge a consistent history
and witness it. That is the standard two-party reduction for any
witnessed transparency log, and it is why the witness is a *separate*
party.

### Demonstration

`crates/hyphae-storage/examples/anchor_ledger.rs` gains a fourth
scenario. Output committed to
`papers/arxiv-preprint/tables/anchor-ledger.txt`:

| threat | single-head | ledger | ledger+witness |
|---|---|---|---|
| chain-aware head rewrite | DETECTED | DETECTED | DETECTED |
| rollback + stale-anchor replay | MISSED | DETECTED | DETECTED |
| equivocation across views | n/a | DETECTED | DETECTED |
| forged published anchor | n/a | DETECTED | DETECTED |
| **withholding (truncated ledger)** | n/a | **MISSED** | **DETECTED** |

Six unit tests in `witness.rs` cover: a witnessed ledger passes; a
withheld ledger is caught; freshness-alone accepts withholding while the
combined check does not; a ledger grown past the witness still passes; a
fork before the witnessed epoch is caught; a wrong witness key is
rejected.

## Threat model after this ADR

- **Defended** (for an attacker holding neither the anchor signing key
  nor the witness key): store-only edits (bare chain, 0003); chain-aware
  head rewrite (anchor, 0032); rollback-with-stale-anchor and
  equivocation (ledger, 0033); **entry withholding** (witness, this
  ADR).
- **Out of scope:**
  - **Anchor-key / witness-key rotation and KMS sourcing** — production
    key management.
  - **Real-world witness wiring.** Here the witness is modeled as an
    independent Ed25519 signer; deployment maps it to a timestamp
    authority (RFC 3161), a transparency-log witness, gossiped
    signed-tree-heads, or an OpenTimestamps/Bitcoin commitment of
    `AnchorLedger::head_hash`. The protocol is implemented; the network
    is deployment.
  - **Source-ingestion trust boundary** — who attests that fragments
    were ingested faithfully in the first place. Provenance from the
    journal onward is now end-to-end tamper-evident; provenance *into*
    the journal is a separate, genuinely open problem.

## Consequences

**Positive:**
- ADR-0033's last named gap is closed at the protocol level; the
  tamper-evidence story is now complete from a stored fragment to an
  audited answer, modulo deployment key management and the ingestion
  boundary.
- No new dependencies (reuses `ed25519-dalek`). ~120 LOC + 6 tests.

**Negative:**
- The guarantee now rests on a *second* independent party (the witness).
  That is inherent to defeating withholding — a single party that
  controls the store cannot witness itself — but it is operational
  surface a deployment must actually stand up.

## Followups

- **Anchor/witness key rotation** semantics (carried from 0032/0033).
- **Concrete witness binding** to OpenTimestamps / RFC 3161 / a
  transparency witness, wired into the substrate checkpoint path.
- **Source-ingestion attestation** — the open boundary above.
