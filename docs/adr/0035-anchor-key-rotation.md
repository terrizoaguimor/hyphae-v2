<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0035
title: Anchor key rotation — a signed keyring
status: accepted
date: 2026-05-29
decision-makers: [mario]
triangulated-by: [claude-opus-4-8 (implementation + adversarial demonstration)]
followup-of: [0032, 0033, 0034]
---

# 0035 — Anchor key rotation

## Context

The head anchor (ADR-0032), the append-only ledger (ADR-0033), and the
external witness (ADR-0034) all sign with a long-lived key, and each
listed **key rotation** as a deployment followup. A long-lived signing
key is a single point of compromise, and there was no way to retire it
without invalidating every signature it ever produced — the ledger
would stop verifying at the first entry signed by a new key.

## Decision

**Add rotation as a composable keyring that does not change the ledger
entry format.** A new `hyphae-storage::keyring` module:

- `KeyRotation { from_ledger_epoch, new_key, authorization }` — a
  handover effective from a ledger epoch, where `authorization` is an
  Ed25519 signature by the **predecessor** key over a domain-separated
  message `(ROTATE_TAG ‖ from_ledger_epoch ‖ new_key)`.
- `Keyring { root, rotations }` — a genesis key trusted out-of-band
  (like a root CA), followed by predecessor-authorized successors.
  `active_key_at(ledger_epoch)` resolves the key in force at an epoch.
- `HeadAnchor::authorize_rotation(new_key, from_ledger_epoch)` — the
  current key signs the handover.
- `verify_keyring` — the lineage is sound: rotation epochs strictly
  increase from a positive first epoch, and each successor is
  authorized by the key active immediately before it.
- `verify_ledger_with_keyring` — verifies the ledger chain and each
  entry's signature **under the key active at that entry's epoch**, so
  a ledger that spans rotations verifies as one log.

### Why the format is unchanged

Rotation metadata lives in the keyring, published alongside the ledger,
not in each `LedgerEntry`. The verifier maps epoch → active key from the
keyring. This keeps ADR-0033's on-disk and wire formats intact and makes
rotation an opt-in verification layer.

### Properties

- **Authenticated rotation.** A successor is legitimate only if the
  current key signed it; an attacker who steals a *new* key cannot
  splice it into the chain back to the trusted root.
- **Compromise containment.** A retired key cannot sign *new* epochs:
  entries at epochs where a later key is active are verified under the
  later key, so a retired-key signature there fails. Rotating away from
  a compromised key closes its window for future history.

### Demonstration

`crates/hyphae-storage/examples/key_rotation.rs` →
`papers/arxiv-preprint/tables/key-rotation.txt`:

| property | result |
|---|---|
| successor authorized by predecessor | REQUIRED (root-of-trust chain) |
| ledger verifies across rotation | YES (per-epoch active key) |
| retired key verifies new history | NO |
| stolen new key inserts itself | NO (needs predecessor's signature) |
| compromised retired key forges new epochs | NO (epochs bound to active key) |

Six unit tests in `keyring.rs` cover lineage + active-key resolution,
a spanning ledger verifying under the keyring, single-key verification
failing across a rotation, a forged self-authorized successor, a retired
key failing to forge post-rotation epochs, and out-of-order rotations.

## Threat model after this ADR

- **Defended** (attacker holding neither the *active* anchor key, the
  witness key, nor a valid rotation from the root): everything from
  0003/0032/0033/0034, plus key compromise is now *recoverable* — rotate
  forward and the retired key can sign no new history.
- **Out of scope:**
  - **KMS/HSM sourcing** of key material (operational).
  - **Revocation-timing policy** — how quickly a detected compromise is
    rotated out bounds the window in which the still-active compromised
    key can sign. The mechanism is here; the cadence is deployment.
  - **Root-key compromise** — the genesis key is the trust anchor; its
    compromise is the irreducible root-of-trust assumption, as in any
    CA / transparency system. Mitigated operationally (offline root,
    threshold signing), not in this layer.
  - **Source-ingestion trust boundary** — still the one genuinely open
    problem (ADR-0034).

## Consequences

**Positive:**
- The "key rotation" followup named by ADR-0032/0033/0034 is discharged
  at the protocol level; key compromise becomes recoverable rather than
  fatal.
- No dependency changes; no ledger-format change. ~150 LOC + 6 tests.

**Negative:**
- The keyring is another artifact a deployment must publish and an
  auditor must hold. This is inherent to rotation and is the standard
  transparency-log / CA posture.

## Followups

- **KMS/HSM-backed signers** and a concrete revocation-timing policy.
- **Witness-key rotation** reuses the same mechanism (the witness is
  also an Ed25519 signer); wiring it is mechanical.
