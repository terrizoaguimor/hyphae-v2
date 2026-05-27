<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0007
title: Boundary smoothing — token-overlap-aware connective selection without body mutation
status: accepted
date: 2026-05-26
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (v0.1 implementation review)]
---

# 0007 — Boundary smoothing: token-overlap-aware connective selection

## Context

After ADR-0006 the realizer projects cascade topology to a sequence
of `(role, fragment)` steps. The picker selects a connective for
each step from the ~250-entry lexicon. But the picker is **blind
to what the quoted bodies actually say**. The connective it picks
is a function of `(role, register, polarity, formality)` — never
of the words that bracket it.

The visible failure mode is **redundancy at boundaries**:

```
Drawing from working memory, "The migration completed..."
Building on it, "The monitoring dashboards stayed green..."
Adding to the picture, "The deploy succeeded..."
```

Three things are wrong:

1. **Anaphor + determiner stacking.** "Building on **it**, **The**
   monitoring dashboards..." — the connective resolves *it* to the
   prior subject ("the migration"), but the next quote opens with
   a fresh definite-article subject. The reader's anaphor
   resolution stalls.
2. **Repeated determiner across boundaries.** Every quote starts
   with "The"; nothing in the prose acknowledges that the realizer
   is enumerating same-shape claims.
3. **Pronoun ambiguity in chained quotes.** "It" in connective 2
   could refer to the migration or to the dashboard signal — the
   reader has to disambiguate from context.

This is the **stale-template** failure mode at a level below
lexicon variety. The lexicon has 250 phrases; the picker just
doesn't know which ones fit the boundary it's about to emit
into.

The fix is **smoothing**: the picker reads light boundary signals
from the adjacent quoted bodies and filters its candidate set
accordingly. Crucially:

- **The fragment bodies stay verbatim.** Smoothing only changes
  which connective the picker emits. The quote remains an exact
  citation of the stored fragment per CLAUDE.md Hard Commitment #12
  ("composition uses fragment quotation, not novel language
  synthesis").
- **The smoothing logic uses no model.** It is heuristic pattern
  matching on the boundary strings of adjacent bodies — a small
  number of deterministic rules.
- **Backward compatible.** Smoothing is **additive filtering** on
  top of the existing picker. When no smoothing rule applies, the
  picker behaves as in ADR-0005.

The architectural commitment is preserved: no LLM in the cognition
path. Smoothing is a context-aware filter on the connective
lexicon, not a transformation of the cited content.

## Decision

Introduce a `BoundarySignal` extracted heuristically from each
quoted body. The picker consults the signal pair `(prev, next)` for
each inter-fragment connective slot and **excludes** connectives
that would create a known redundancy pattern. The realizer falls
back to the unfiltered picker when no candidate survives the
filter, so smoothing **never starves** the realizer of a
connective.

### `BoundarySignal`

```rust
pub struct BoundarySignal {
    /// First content token of the body, lowercased.
    /// `None` for empty or stopword-only bodies.
    pub initial_token: Option<String>,
    /// `true` when the body begins with a definite determiner
    /// ("the", "this", "that", "these", "those").
    pub starts_with_definite_determiner: bool,
    /// `true` when the body begins with an indefinite determiner
    /// ("a", "an").
    pub starts_with_indefinite_determiner: bool,
    /// Last content token of the body, lowercased. Useful for
    /// detecting v0.2 pronoun-threading signals.
    pub final_token: Option<String>,
}
```

The signal is computed at picker time from the body string. No
extension to `BoundaryMetadata` in `hyphae-core` for v0.1 — the
signal lives in `hyphae-surface::boundary` and is computed lazily.
A v0.2 ADR may move it into the encoder-populated `BoundaryMetadata`
if the encoder gains the tokeniser to do so cheaply.

### Smoothing rules (v0.1)

The v0.1 smoothing surface ships **three rules**. Each rule is a
filter on the picker's candidate set; a candidate that fails any
rule is excluded.

**Rule 1 — `anaphor before definite-determiner quote`.** When the
next body starts with a definite determiner, exclude connectives
that end with a singular pronoun anaphor (`it,`, `this,`,
`that,`). Reason: stacking *"Building on it, The migration..."*
forces the reader to bind *it* and then immediately re-bind to a
fresh definite NP. The connective's referent stalls.

**Rule 2 — `same-determiner repetition`.** When **both**
adjacent bodies start with the same definite-determiner-led NP
(both `"the deploy"`, or both `"this build"`), prefer connectives
that explicitly mark continuation-of-same-subject
(`"likewise,"`, `"in the same direction,"`, `"continuing,"`)
over generic ones. v0.1 implements this as a **preference**, not
a hard filter — if no continuation-of-same-subject connective is
in the candidate set after Rule 1, the picker falls through.

**Rule 3 — `token-overlap repetition`.** When the **initial token**
of the next body equals the **final token** of the previous body
(after lowercasing and stop-word filtering), exclude connectives
whose phrase contains that exact token. Reason: avoids
*"...the deploy. Likewise, the deploy succeeded"* where the
connective + boundary triple-prints the same content word.

### Fallback discipline

If filtering produces an **empty candidate set**, the picker falls
back to the unfiltered lexicon. The realizer never panics on
smoothing; it degrades to the ADR-0005 baseline gracefully. A
trace log records the fallback so the integrator can see when the
lexicon is too narrow for the boundary signal.

### What this does NOT do (v0.2 candidates)

- **Tense alignment.** The opening connective could inherit the
  dominant tense of the working set (`"Looking at what
  happened,"` for past-tense bodies). v0.1 skips this — it would
  require either tense detection per fragment (cheap but lossy)
  or a tense field on each `Connective` entry (data burden).
- **Pronoun threading.** Detecting that fragment[i+1]'s opening
  pronoun refers to fragment[i]'s closing NP and choosing a
  connective that smooths that anaphor. v0.1 only filters out
  the conflict; it does not attempt to bridge.
- **Subject continuity across non-adjacent steps.** A composition
  with five steps may carry a single subject through all of them.
  The realizer does not detect this and does not vary connectives
  accordingly. v0.2 candidate.
- **Punctuation smoothing.** The realizer always puts a space
  between the connective and the quote. Some connectives end with
  a colon (`"The source states:"`) and the quote follows naturally;
  others end with a comma (`"However,"`). v0.1 does not adjust
  the punctuation between connective and quote based on the
  connective's tail character. The current shape works because
  the lexicon's tail conventions are consistent.

These are all candidates for a future ADR. v0.1 ships the
minimum that visibly reduces the stale-template feel.

### Why three rules, not ten

A larger rule set scales the lexicon's data burden non-linearly:
each new rule requires a list of which connectives in the
existing ~250 satisfy it. Three rules ship with O(1) added data
(none — they read connective phrase strings directly). Adding a
fourth rule that requires per-connective tagging is a deferred
data-curation task.

This is the same shape as ADR-0005's "~300, not 500" decision —
the v0.1 surface ships the smallest change that exits the
template-rigid territory.

## Consequences

- The composition reads less repetitive at boundaries. The most
  visible v0.1.x bug (the "Building on it, The X..." pattern) is
  structurally suppressed.
- The picker's complexity grows by one filtering pass per
  inter-fragment slot. The filter is `O(candidates × constant)`;
  no impact on the realizer's hot path.
- New module `hyphae-surface::boundary` owns the signal extractor
  and the filter rules. ~200 LOC. Tests cover each rule on
  documented input pairs.
- The realizer keeps its existing API. `RealizationRequest` and
  `RealizationOutput` are unchanged.
- The eval harness's `connective_hygiene` scorer gains one
  additional check: scan the output for the specific anaphor +
  definite-determiner stack and fail if observed (rule 1
  violation should be impossible after this ADR).
- ADR-0008's substrate-integration wire-up consumes this without
  change. The boundary smoothing is internal to the realizer.

## Cross-references

- **ADR-0005** — populates the lexicon the smoothing filter
  operates on.
- **ADR-0006** — produces the step sequence the smoothing applies
  to.
- **CLAUDE.md Hard Commitment #12** — fragment bodies stay
  verbatim. This ADR honours the commitment by only filtering
  which connective phrase is emitted; bodies are unchanged.
- **`hyphae_surface::Lexicon::pick_in_context`** — the function
  the smoothing filter wraps.
