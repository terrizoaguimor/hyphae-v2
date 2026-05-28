<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0032
title: External head anchoring — Ed25519-signed chain head
status: accepted
date: 2026-05-28
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (implementation + adversarial test)]
---

# 0032 — External head anchoring

## Context

The hash-chained journal (ADR-0003, `hyphae-storage`) gives
tamper-evidence against a **store-only** attacker who edits a record
in place: the recomputed chain breaks at the edited entry's
successor and `verify()` localises it. The provenance benchmark
(ADR-0031 writeup, paper §5.2) measured this at 100% across four
tampering modes.

But the same benchmark exposed the boundary. A **chain-aware**
attacker who knows the hash construction can, after editing a
fragment, recompute every hash forward and rewrite the persisted
chain head. The forged chain then verifies cleanly. Because the head
lives in the `meta` partition of the *same* `fjall` keyspace as the
fragment bodies, any attacker with store write access can rewrite the
head, so the store-only adversary collapses into the chain-aware one
and the bare-chain guarantee is, on its own, vacuous against a full
store compromise.

The paper declared this the single load-bearing dependency of its
positive result and named external head anchoring as the fix —
initially as future work. This ADR implements it so the guarantee is
demonstrated rather than promised.

## Decision

**Anchor the chain head with an Ed25519 signature produced by a key
the store process does not hold.** A new `hyphae-storage::anchor`
module provides:

- `HeadAnchor` — holds the signing key (in deployment: an offline
  signer / HSM / audit-service key; for reproducible tests and the
  benchmark, derived from a fixed 32-byte seed via
  `SigningKey::from_bytes`).
- `HeadAnchor::anchor(head) -> AnchoredHead` — signs the 32-byte
  chain head, returning `(head, signature)` to publish outside the
  store's write scope (an append-only external log, a timestamp
  authority, or simply write-isolated media).
- `verify_anchored_head(current_head, anchored, verifying_key)` —
  returns true only when the anchored signature is valid under the
  public verifying key *and* the signed head equals the journal's
  current head.

`Journal::head()` exposes the current head for signing.

### Why this closes the gap

A chain-aware attacker rewrites the persisted head to a
recomputed-but-forged value `head'`. The published anchor was signed
over the legitimate `head`. Verification fails on two independent
grounds: the anchored head no longer equals the journal's current
head, and the attacker cannot produce a signature over `head'` that
verifies under the audit verifying key, because they do not hold the
signing key. The guarantee therefore strengthens from

> *tamper-evident against an attacker who cannot write the head*
> (vacuous when the head shares the store)

to

> *tamper-evident against an attacker who does not hold the anchor
> signing key*

which is realistic: the key genuinely lives outside the store
(HSM/offline/audit service), whereas the head necessarily lives in
the store.

### Demonstration

`crates/hyphae-storage/examples/tamper_detection.rs` adds an
anchored chain-aware trial. The result, committed to
`papers/arxiv-preprint/tables/tamper-detection.txt`:

- bare-chain `verify()` after the chain-aware attack: **passes**
  (the forged chain is internally consistent — the attack succeeds);
- anchored verification of the same store: **fails** (DETECTED) —
  the signature over the original head does not match the rewritten
  head.

Three unit tests in `anchor.rs` cover: valid anchor verifies; a
rewritten head fails; an attacker re-signing with a *different* key
fails under the legitimate verifying key.

## Threat model after this ADR

- **Defended:** store-only edits (bare chain); chain-aware
  recompute-and-rewrite-head (anchor), for any attacker who does not
  hold the anchor signing key.
- **Out of scope:** an attacker who additionally compromises the
  anchor signing key (HSM/offline-signer compromise) can forge a
  valid anchor; key management is the deployment's responsibility and
  is the standard reduction for any signed-log scheme.
- **Not addressed here:** liveness / availability (an attacker can
  still delete the store; anchoring makes deletion *detectable* via a
  missing-or-stale anchor, not impossible), and freshness across
  multiple anchored snapshots (an external append-only ledger, vs a
  single latest signature, is the natural extension — see Followups).

## Consequences

**Positive:**
- The paper's positive result is now demonstrated end to end against
  both adversaries, not conditional on an unimplemented piece.
- The mechanism is standard (signed log head) and small (~120 LOC +
  one well-audited dependency).

**Negative:**
- Adds `ed25519-dalek` to the workspace dependency surface (pulls
  `curve25519-dalek`, `signature`, `zeroize`). Justified: the audit
  property is now a first-class, tested feature, not a claim.
- The demo derives the key from a fixed seed for reproducibility;
  a production deployment MUST source it from a KMS/HSM and never
  materialise it in the store process. Documented in the module.

## Followups

- **External append-only publication of anchors** (timestamp
  authority, transparency log, or a Bitcoin/OpenTimestamps anchor)
  so freshness and non-equivocation hold across snapshots, not just
  integrity of the latest head.
- **Periodic anchoring policy** wired into the substrate's checkpoint
  path, so the head is re-anchored on a cadence rather than only on
  demand.
- **Key rotation** semantics for the anchor verifying key.
