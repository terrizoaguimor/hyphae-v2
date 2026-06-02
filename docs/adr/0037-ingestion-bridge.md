<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0037
title: Ingestion bridge — signed-at-source provenance into the journal
status: accepted
date: 2026-06-02
decision-makers: [mario]
triangulated-by: [claude-opus-4-8 (design + implementation + demonstration), deep-research workflow (prior-art positioning)]
followup-of: [0034]
---

# 0037 — Ingestion bridge

## Context

The integrity chain — hash chain (ADR-0003), head anchor (0032),
append-only ledger (0033), external witness (0034), signed keyring
(0035) — attests integrity *from the journal forward*: a stored
fragment was not altered after it was written. ADR-0034 named the one
genuinely-open boundary, and a deep-research pass (research lens,
adversarial verification) confirmed it is where the remaining
contribution lives: **provenance *into* the journal**. A fragment's
payload can be fabricated at admission, and the chain will faithfully
prove the fabrication was never touched. Tamper-evident ≠ truthfully
sourced.

The same research established the positioning: transparency-log
structures are already applied to AI provenance, but only at **artifact
granularity** — Sigstore model-transparency signs whole model files;
Intel Atlas (EuroS&PW 2025) attests dataset/pipeline lineage and already
uses **C2PA Content Credentials**. None attests how an *individual
record* entered a store. Per-fragment ingestion provenance is the open,
differentiated gap.

## Decision

Add an **ingestion-side peer** of the integrity chain: at admission
time the ingestion source signs a credential binding a fragment's exact
bytes to a claimed origin. New `hyphae-storage::ingestion`:

- `IngestionCredential { fragment_hash, source_hash, locator, asserter,
  asserted_at, source_uri, signature }` — a self-describing record with
  a canonical encoding (`to_bytes`/`from_bytes`) whose pre-signature
  prefix (`signed_bytes`) is exactly what the asserter signs (domain-tag
  prefixed, length-prefixed URI). No serde dependency; the canonical
  form doubles as the journal payload and the signed message.
- `IngestionAsserter` — holds a signing key distinct from the
  anchor/witness keys. `assert_ingestion(fragment, source, uri, locator)`
  **refuses to sign** (returns `None`) unless `source[locator] ==
  fragment`, so an honest asserter cannot mint a credential for a
  fabricated excerpt.
- `verify_credential` (attribution) — validly signed by the named
  asserter and bound to the exact fragment bytes; **no source needed**.
- `verify_faithful_excerpt` (faithful excerpt) — the above PLUS the
  source hashes to the claimed value and the fragment is byte-for-byte
  `source[locator]`; **requires the source**.

### Binding to the journal (no format change)

The credential is appended as a typed event
(`event_kind = "ingestion_credential"`, `payload = credential.to_bytes()`).
It therefore inherits the entire integrity chain for free — it is
hash-chained, anchored, in the ledger, witnessed, and under the keyring,
exactly like any entry — and adds attributable, signed-at-source
provenance on top. The binding to its fragment is `credential.fragment_hash
== sha256(fragment_entry.payload)`. `JournalEntry`'s format is unchanged.

### Faithfulness notion (v1)

Byte-range substring: the fragment must equal `source[start..end)`
exactly. This is concrete, verifiable, and fits the extractive case
that is Hyphae's whole point. Transformations (normalisation, OCR,
translation, RAG re-chunking) are out of scope for v1.

## What it closes — and the honesty (provenance ≠ truth)

- **Closes:** *attributable origin* (who asserted, from where) — always;
  and *faithful excerpt* (the fragment is a real substring of the named
  source) — given the source.
- **Moves the trust boundary** from "trust the store" to "trust the
  named asserter" — the standard C2PA posture.
- **Does NOT close content validity.** The named source may itself be
  false or poisoned; that is an orthogonal axis (e.g. RAGShield's
  numerical-manipulation detection, RAG-poisoning benchmarks) explicitly
  out of scope. A malicious asserter holding a trusted key can still
  assert a fabricated fragment with a fabricated source — the bridge
  makes that **attributable** (you know whom to blame) and, if the real
  source is checkable, **detectable** (the excerpt will not match).

## C2PA alignment (the standards bridge)

| C2PA | Hyphae |
|---|---|
| Manifest | `IngestionCredential` |
| Assertion | the origin claim (`source_hash` + `source_uri` + `locator`) |
| Claim generator | `IngestionAsserter` |
| Hard binding (asset hash) | `fragment_hash` |

The in-repo form is a minimal Ed25519 primitive *aligned* to C2PA. Full
C2PA manifest serialisation (JUMBF/CBOR via the `c2pa` crate) and a real
asserter PKI are deployment, not this ADR. Asserter-key rotation reuses
the keyring (ADR-0035).

### Demonstration

`crates/hyphae-storage/examples/ingestion_bridge.rs` ->
`papers/arxiv-preprint/tables/ingestion-bridge.txt`. The load-bearing
result — integrity and provenance-into are orthogonal:

| check | genuine fragment | injected fragment |
|---|---|---|
| integrity chain `verify()` | PASS | **PASS** (faithfully stored) |
| ingestion credential (attribution) | PASS | **FAIL** (no signed source) |

Plus: attribution and faithful-excerpt pass for a genuine credential; a
forged credential (impostor asserter) is rejected under the trusted key;
and faithful-excerpt fails against a wrong source. Seven unit tests in
`ingestion.rs` cover attribution, wrong-key, tampered-fragment,
honest-asserter-refuses-fabrication, faithful-excerpt-vs-wrong-source,
canonical round-trip (with trailing-garbage/bad-tag rejection), and a
journaling round-trip (integrity + attribution compose).

## Threat model after this ADR

- **Defended** (attacker holding none of the anchor/witness/keyring keys
  AND not a trusted asserter): everything from 0003/0032/0033/0034/0035,
  plus a fragment injected without a valid signed-at-source credential
  is now detectable, and (given the source) any credential whose claimed
  origin does not contain the fragment is caught.
- **Out of scope:** content validity / truth of the source (orthogonal
  axis); a compromised *asserter* key (makes fabrication attributable +
  source-checkable, not impossible — same reduction as a compromised
  C2PA signer); transformations beyond verbatim excerpt; full C2PA
  manifest wire format and asserter PKI (deployment).

## Consequences

**Positive:**
- Closes ADR-0034's last named open boundary at the protocol level; the
  provenance story is now end-to-end *into* and *within* the store.
- No `JournalEntry` format change, no new dependencies (reuses
  `ed25519-dalek` + `sha2`). ~210 LOC + 7 tests + demonstration.

**Negative:**
- Introduces a second trusted role (the asserter) and a key to manage.
  Inherent to source provenance — provenance into a store cannot be
  self-certified by the store. Documented; key rotation deferred to the
  keyring.

## Followups

- Full C2PA manifest serialisation + asserter PKI / rotation.
- Transformation-aware faithfulness (normalisation, chunking, OCR) for
  non-verbatim ingestion pipelines.
- A `provbench` ingestion axis (v3) scoring credential-less / forged /
  fabricated-source injection across adversary profiles.
- Compose with a content-validity check (RAGShield-style) for the
  orthogonal truth axis.
