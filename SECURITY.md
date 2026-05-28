<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

# Security Policy

## Supported versions

Hyphae v2 is pre-1.0 software. Security fixes are issued for
the most recent tagged release only. The current tag is
listed in the [README](README.md) and in
[`CHANGELOG.md`](CHANGELOG.md).

| Version | Supported |
|---|---|
| `v0.1.x` | ✅ Active |
| Earlier tags | ❌ Not supported |
| Pre-1.0 unreleased `main` | ⚠️ Best-effort fixes; not guaranteed |

When v1.0 lands, a backport window will be defined for the
prior minor series.

## Reporting a vulnerability

**Do not open a public GitHub issue for security reports.**

Instead, email **mario@celiums.ai** with:

1. A description of the vulnerability and its impact.
2. The version (tag or commit SHA) where the issue reproduces.
3. A minimal reproducer (input, configuration, expected vs
   observed behaviour).
4. Optionally, a proposed fix.

We will acknowledge receipt within **7 calendar days** and aim
for an initial assessment within **30 calendar days**. Severity
and timeline for fix depend on impact.

## Scope of "security"

The following are in scope:

- **Memory safety**: any path that triggers undefined behaviour
  or panics on attacker-controlled input.
- **Journal integrity**: any path that allows tampering with
  the hash-chained journal without detection.
- **Ethics-coverage bypass**: any path that allows the
  substrate to ingest, recall, compose, or apply a learning
  update **without** the ethics evaluation point firing.
- **Persistence corruption**: any path that corrupts the
  `redb` state store or the `fjall` journal in a way that
  breaks recovery.
- **Provenance manipulation**: any path that allows a fragment
  to surface with incorrect or missing
  `provenance.parent_ids`, `provenance.confabulation_risk`, or
  `provenance.source_subsystem`.
- **Verbatim-quotation violation**: any path that causes the
  realizer to emit text claiming to be a fragment body that
  differs from the stored body. This is load-bearing for the
  no-LLM-in-cognition-path commitment.

The following are out of scope:

- **Output prose quality.** The realizer's output is
  intentionally template-rigid; see the README "On the prose
  style" section. A report that "Hyphae's output sounds
  template-rigid" is not a security issue — it is the
  documented architectural feature.
- **Performance**. Slow paths are tracked as performance
  issues, not security issues, unless the slowdown enables a
  denial-of-service primitive on a known deployment.
- **Lexicon errors**. Wrong-sounding phrases, especially in
  Spanish entries marked as model-drafted (ADR-0021,
  ADR-0022), are linguistic corrections — file as regular
  issues, not security reports.
- **Disagreement with an ADR**. Architectural decisions are
  documented; disagreement is welcome but is not a security
  report. Open an issue or propose a superseding ADR.

## Disclosure timeline

Once a fix is shipped, we publish a brief advisory describing:

- The vulnerability class.
- The affected versions.
- The fix.
- A credit to the reporter (unless they prefer anonymity).

Pre-disclosure embargo windows are negotiable for
substantial issues. We do not delay disclosure to manage
public-relations narratives.

## What to expect after reporting

- We will not threaten legal action against good-faith
  security researchers.
- We will not gate disclosure on signing an NDA.
- We will not pay bug bounties (pre-1.0). When v1.0 lands and
  Hyphae has a stable deployment footprint, this may change.
- We will credit you if you want to be credited, or keep your
  report anonymous if you prefer.

## Defensive posture

Hyphae's architecture is designed with several invariants that
limit the blast radius of bugs:

- **Hash-chained journal.** Tampering with stored fragments is
  detectable by verifying the chain (ADR-0003 §8). The
  `journal_verify_chain` operation runs on every Recovery
  transition.
- **Ethics RADAR, not JAIL.** Ethics evaluations emit signals
  and audit entries; they do not block operations. This means
  an ethics regression is a logging issue, not a
  service-unavailability issue.
- **Fragment quotation, not synthesis.** The realizer never
  paraphrases what it quotes. A bug that causes paraphrasing
  is detectable by the eval harness's
  `verbatim_compliance` dimension.

These invariants reduce — but do not eliminate — the surface
for security issues. Reports remain welcome.

## Contact

- Security: **mario@celiums.ai**
- General: open an issue on the repository.
