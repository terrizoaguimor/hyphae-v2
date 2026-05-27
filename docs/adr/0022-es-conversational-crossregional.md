<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0022
title: ES Conversational register — cross-regional expansion
status: accepted
date: 2026-05-27
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (drafter; Mario review pending for regional pass)]
---

# 0022 — ES Conversational register: cross-regional expansion

## Context

ADR-0017 shipped ~60 ES entries; ADR-0021 added ~68 entries
across Formal / Neutral / Technical, **deliberately leaving
`Register::Conversational` at ~3–4 entries**. The reasoning
(verbatim from ADR-0021):

> `Register::Conversational` stays at ADR-0017 size because
> LATAM-vs-Spain register divergence requires native-speaker
> authority that ADR-0022 will provide.

That authority is Mario's. The model writing this ADR
(`claude-opus-4-7`) is not a Spanish native speaker. ADR-0022
walks a narrow line: expand Conversational only with phrases
that are **demonstrably cross-regional** (canonical in both
LATAM and Spain), defer regionalisms to Mario's post-merge
pass.

The risk in skipping ADR-0022 entirely: every ES caller that
sets `Register::Conversational` context falls back through the
picker's 4-level chain to `role`-only matching, producing
output that reads more like Formal Spanish than informal
speech. Mario's `domain_tags = ["informal", "conversation"]`
intent gets lost in the fallback. ADR-0022 closes enough of
that gap to make the Conversational path operational while
respecting native-speaker authority for regional surface
forms.

## Decision

**Add ~17 cross-regional Conversational entries across the 10
roles, restricted to phrases that the RAE *Diccionario
panhispánico de dudas* and the Briz-Pons-Portolés
*Diccionario de partículas discursivas del español* attest as
**panhispanic informal register** (not specific to one
country). Document the exclusion criterion explicitly. Tag
each new entry with `// ADR-0022 cross-regional` markers so
Mario's pass can locate them for review.**

### Inclusion criterion

A phrase enters the Conversational expansion only if:

1. **Both authorities (RAE + Briz/Pons/Portolés) attest it as
   informal panhispanic register** — not marked as
   country-specific.
2. **Cross-checks against editorial usage** in widely-read
   pan-LATAM publications (e.g. *El País América*, *BBC
   Mundo*, *Univision*) confirm the phrase reads natural
   regardless of the reader's national origin.
3. **No marked rioplatense/peninsular/mexican voseo or
   tuteo grammar embedded** in the phrase (e.g. *"mirá"*
   stays in the existing ADR-0017 set as Mario's call;
   *"míra"* / *"mira"* variants are NOT added by ADR-0022 —
   inconsistent tuteo register).
4. **Functional category is unambiguous** — the phrase fits
   one role + polarity + the implied Conversational +
   Low-formality slot without semantic drift.

### Exclusion criterion — explicit

The following phrases were **considered and deliberately
NOT added** by ADR-0022 because they violate criterion 1 or
3:

| Phrase | Why excluded |
|---|---|
| `"En plan,"` | Spain-specific (Peninsular contemporary slang) |
| `"Tipo,"` (as hedge) | MX/AR-leaning contemporary; sounds odd in formal ES register |
| `"Y eso,"` (closing) | Rioplatense; reads slangy in MX/CO |
| `"Total que,"` | Mostly Spain + rioplatense; not panhispanic neutral |
| `"Va,"` (filler) | Spain-specific contemporary |
| `"Pues,"` (opener) | Spain-leaning informal; LATAM uses sparingly |
| `"Pos,"` (informal "pues") | MX/regional; not pan-LATAM |
| `"Che,"` | AR/UY only |
| `"Vale,"` (acknowledgment) | Spain-specific |
| `"Carnal,"` / `"Brother,"` | MX/contemporary slang |

These belong in a future native-speaker-led pass (Mario's
authority) that **adds regional variants explicitly tagged
with country-of-use** — a feature ADR-0022 does NOT
introduce in v0.2.

### Roles + new entries

| Role | Existing (ADR-0017) | + ADR-0022 |
|---|---|---|
| Opening | 2 conversational | +3 |
| Continuation | 1 | +2 |
| Contrast | 1 (`"Eso sí,"` soft) | +2 (one hard, one soft) |
| Closing | 1 | +2 |
| Causation | 1 (`"Así que,"`) | +1 |
| Elaboration | 0 | +2 |
| Summary | 0 | +2 |
| Attribution | 0 | +1 |
| Concession | 0 | +1 |
| Sequence | 0 | +1 |
| **Total new** | | **+17** |

Final lexicon target: ~128 + 17 = **~145 entries**.

### What this ADR explicitly does **not** do

- **Does not** add regional Conversational variants (Spain,
  MX, AR/UY, CO, etc.). Reserved for a future ADR (ADR-0023?)
  with country-tagged entries.
- **Does not** introduce a country-of-use field on
  `Connective`. The Conversational register stays a single
  bucket; geographic differentiation lives in a future ADR
  if it becomes load-bearing.
- **Does not** retroactively review the ADR-0021 model-drafted
  Formal/Neutral/Technical entries. That native-speaker pass
  is a separate Mario-led commit (no ADR needed — pure
  curation).
- **Does not** scale Conversational to EN-parity (~25 entries
  in EN Conversational). The cross-regional discipline caps
  v0.2 at ~10 total Conversational entries. ADR-0023 (when
  filed) takes it the rest of the way.

### Honest acknowledgment — still model-drafted

These entries are still drafted by `claude-opus-4-7`. The
cross-regional criterion is conservative; misses are likely.
Mario's post-merge review may revise or remove individual
entries. The architectural commitment (Conversational has SOME
content now) survives even if specific phrases get adjusted.

## Sources

- **RAE *Diccionario de la lengua española***.
- **RAE *Diccionario panhispánico de dudas*** — register
  + regional attestation.
- **Briz, Pons, Portolés (2008)**, *Diccionario de partículas
  discursivas del español* — informal-register discourse
  markers attested across LATAM + Spain.
- **El País América / BBC Mundo / Univision style guides** —
  editorial cross-regional usage.

## Consequences

- ES lexicon: ~128 → ~145 entries. Conversational quadrant
  approaches operational scale (~10 entries) for the first
  time in v0.2.
- ES callers tagging `Register::Conversational` now reach
  Conversational phrases at the picker's level-1 or level-2
  fallback (instead of falling through to level-4 role-only).
- Native-speaker review backlog grows by ~17 entries; Mario
  reviews when convenient. No urgency — the cross-regional
  filter is conservative enough that wrong entries are more
  likely "too formal" than "wrong register".
- The ADR-0021 + ADR-0022 expansion together brings ES from
  "architectural proof" to **architectural sufficient**:
  every (role × register × {Low,Mid,High}) cell now has at
  least one entry, even if the lattice is sparse.
- Future ADR-0023 (when needed) introduces regional variants
  with country-of-use markup.

## Cross-references

- **ADR-0017** — the lexicon this ADR completes for v0.2.
- **ADR-0021** — the Formal/Neutral/Technical expansion this
  ADR pairs with.
- **`hyphae_surface::connective_data_es::baseline_es_data`** —
  the function this ADR extends.
- **Future ADR-0023** (reserved) — country-tagged regional
  Conversational variants.
