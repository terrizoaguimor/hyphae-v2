<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

# Contributing to Hyphae v2

Thank you for considering a contribution. Hyphae is a deliberate
project with explicit architectural commitments — this file
documents the discipline contributors are expected to follow.

## Before you contribute

Read these first, in order:

1. **[`README.md`](README.md)** — what this is, what it is not,
   the prose style commitment, the H.Y.P.H.A.E. backronym.
2. **[`docs/rfc/v1-living.md`](docs/rfc/v1-living.md)** — the
   canonical specification.
3. **[`docs/adr/0001-fresh-from-v1.md`](docs/adr/0001-fresh-from-v1.md)** —
   the hard architectural commitments and why each one exists.
4. **[`CLAUDE.md`](CLAUDE.md)** — operating discipline for
   assistant-driven development. Even if you contribute by hand,
   the discipline applies.

If after reading those four documents you still want to
contribute: welcome.

## Hard architectural commitments — do not litigate informally

Hyphae's design is shaped by twelve hard commitments in ADR-0001.
The two contributors raise most often:

1. **No LLM in the cognition path.** The substrate does not
   generate language. The realizer composes via fragment
   quotation + connective tissue. A contribution that adds a
   model-based generator inside the cognition path will not be
   merged.
2. **Hash-chained journal is non-negotiable.** Every significant
   event writes a SHA-256-chained entry. Speed comes from
   optimising the journal, not skipping it.

The other ten commitments live in ADR-0001 §"Hard Architectural
Commitments". If a contribution requires changing any of them, the
contribution starts with **a new ADR** that supersedes the
relevant commitment (with reasoning, alternatives, and BDFL
sign-off).

## The ADR discipline

Every architectural change goes through an ADR (Architecture
Decision Record) in `docs/adr/NNNN-slug.md`. The ADRs already
filed (0001–0026, with 0012 deliberately vacant) are the canonical
record of how the project arrived here. Read them before
proposing changes — many "obvious" additions were considered and
explicitly rejected.

### When to file an ADR

File an ADR for:

- A new architectural commitment.
- A new public API surface that other crates will depend on.
- A new dependency (every dep needs justification — the dep list
  is curated).
- Re-entry of a postponed feature (multilingual lexicon, new
  schema, deferred subsystem, etc).
- Override or supersede of an existing ADR.

You do NOT need an ADR for:

- Bug fixes that don't change behaviour for any documented
  case.
- Documentation improvements.
- Test additions for existing behaviour.
- Performance optimisations that preserve correctness invariants.

### ADR shape

Look at the existing ADRs for the conventional shape. Each
includes:

- Front matter (adr number, title, status, date, decision-makers,
  triangulated-by).
- Context (what problem the decision addresses).
- Decision (what is being decided).
- Sources (references, prior art, citations).
- Consequences (what changes as a result).
- Cross-references (to other ADRs).
- An explicit "What this ADR does NOT do" section is encouraged.

## How a change lands

1. **Open an issue** describing the change you want to make.
   Reference the relevant ADRs / RFC sections. If the change
   needs a new ADR, mention that.
2. **Discuss before coding** when the change is non-trivial. The
   project has chosen its scope carefully; out-of-scope changes
   that were already filed as code get rejected even if the code
   is good.
3. **Branch from `main`** (trunk-based development). Short-lived
   feature branches, conventional commit messages
   (`feat:`, `fix:`, `docs:`, `test:`, `bench:`, `refactor:`,
   `chore:`).
4. **Include tests.** Every behaviour change needs a test. New
   functionality requires new tests. The project enforces this
   via the CI gate.
5. **Run the local gate before pushing:**

   ```bash
   cargo fmt --all
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

   The CI workflow runs the same checks on every PR.
6. **Open a PR** that references the issue, summarises the
   change, and lists which ADRs (if any) were filed alongside.
7. **Wait for review.** The project's BDFL (Mario Gutiérrez)
   reviews architectural changes. Other reviewers may comment on
   code quality.

## Commit message conventions

Conventional Commits (`feat:`, `fix:`, `docs:`, …) with a clear
subject line under ~72 characters. The body explains the
**why** more than the **what** — the diff already shows the what.

If the commit closes an ADR, mention the ADR id explicitly in
the subject line: `feat(surface): cascade-shape composition (ADR-0006)`.

**Do not include `Co-Authored-By:` lines from automated tools.**
The project policy is that commits attribute the human who
authored the work; tooling-generated co-authorship lines are
declined.

## Honest scoring discipline

The eval harness in `crates/hyphae-eval/` ships with a
deliberate anti-greenwashing posture (ADR-0008/0010): honest
numbers, no target thresholds, the v1-pattern canary fires when
every dimension reads above 0.99. If you change scoring or audit
behaviour, **read ADR-0008 + ADR-0010 first**. The discipline is
load-bearing — it is the property that distinguishes v2 from v1.

## Lexicon contributions

The connective lexicon (`crates/hyphae-surface/src/connective_data*.rs`)
is hand-curated. If you contribute lexicon entries:

- **English (`connective_data.rs`):** entries should be RAE-of-
  English-equivalent quality. Avoid colloquial flourishes that
  would feel forced in formal prose; the lexicon is the source
  of truth for ALL register / formality combinations the picker
  selects from.
- **Spanish (`connective_data_es.rs`):** model-drafted entries
  (ADR-0021, ADR-0022) carry `// ADR-XXXX` markers — those are
  pending native-speaker review by the BDFL. Native speakers
  contributing review notes for those entries is welcome.
  **Conversational regional variants** (rioplatense / peninsular
  / mexicano) are explicitly deferred and require their own ADR
  with country-of-use markup.
- **Other languages:** require a new ADR (`hyphae-surface/src/
  connective_data_LANG.rs` + `Lexicon::baseline_LANG()` +
  `BoundaryRules::LANGUAGE` constant + corpus extension). See
  ADR-0017–0019 for the ES pattern.

## What to NOT contribute

These are explicit non-goals; PRs implementing them will be
politely closed:

- LLM wrappers around the substrate.
- Vector store backends that replace the cascade graph.
- Streaming token generation interfaces.
- Auto-detection of language from content (the realizer's
  language is set at construction time per ADR-0017).
- Auto-translation of fragments between languages.
- Cross-lingual fragment paraphrasing.
- A "naturalization layer" that rewrites the realizer's output
  via an LLM. ADR-0001 §"Hard Commitment 12" closes this; the
  README's "On the prose style" section explains the trade-off.

These are not bad ideas — they are different ideas. Build them
in your own project; Hyphae is not the place.

## Reporting bugs and security issues

- **Bugs**: open an issue with `[bug]` prefix. Include the
  command that reproduced the issue, the observed output, and
  the expected output.
- **Security**: see [`SECURITY.md`](SECURITY.md).

## Code of conduct

Contributors are expected to follow the
[Code of Conduct](CODE_OF_CONDUCT.md). The TL;DR is the usual:
be considerate, be honest, do not abuse the issue tracker for
unrelated arguments. Architectural disagreement is welcome and
expected; personal attacks are not.

## Governance

Hyphae v2 is a benevolent-dictator project. The BDFL is Mario
Gutiérrez (Celiums Solutions LLC). All architectural decisions
require BDFL sign-off; routine code review can be done by any
reviewer the BDFL has delegated to.

The pathway to broader governance is documented in the living
RFC §"Governance" and remains intentionally minimal at v0.2.
When the project's contributor base grows enough to require
formal governance, that change itself will be filed as an ADR.

## License

By contributing to Hyphae v2 you agree that your contributions
will be licensed under the project's existing terms:
[Apache-2.0](LICENSE) for code and [CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/)
for documentation. The project does not require a separate CLA.
