<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0021
title: ES lexicon scale — model-drafted Formal/Neutral/Technical expansion
status: accepted
date: 2026-05-27
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (drafter; native-speaker review pending)]
---

# 0021 — ES lexicon scale: model-drafted Formal/Neutral/Technical expansion

## Context

ADR-0017 shipped the v0.2 Spanish lexicon at ~60 entries as
**architectural proof, not production coverage**. The lexicon's
4-level fallback handles the sparseness, but real ES output
hits the level-4 fallback ("any phrase in the role") for many
context combinations because the
`(role × register × polarity × formality)` lattice has empty
cells.

ADR-0017 also documented a discipline:

> Every entry is hand-curated EN-native ↔ ES-native pairing by
> Mario (the integrator and a Spanish native speaker). No
> machine translation. No automated alignment.

That discipline is load-bearing for `Register::Conversational`
(LATAM-vs-Spain register diverges sharply — *"mirá", "vale",
"che", "carnal"* are non-portable) and for nuanced register
calls in any quadrant. It is **less load-bearing** for the
canonical Formal/Neutral/Technical discourse markers RAE
documents as standard panhispanic forms.

This ADR walks the line honestly: add ~80 new entries in the
quadrants where standard Spanish has high inter-regional
consensus, do **not** touch Conversational, and document the
model-drafted nature inline so a future Mario-led ADR closes
the rest.

## Decision

**Scale `connective_data_es.rs` from ~60 to ~140 entries by
adding canonical Formal / Neutral / Technical discourse markers
across all 10 roles. The new entries are model-drafted (by
claude-opus-4-7), drawn from regionally-invariant standard
Spanish (RAE-attested forms, common in scholarly and business
prose across LATAM + Spain). `Register::Conversational` stays
at the ADR-0017 size — its LATAM-vs-Spain divergence requires
Mario's native-speaker authority, which a future ADR will
provide.**

### Scope by role + register (planned counts)

| Role | Existing | + Formal | + Neutral | + Technical | Total |
|---|---|---|---|---|---|
| Opening | 8 | +3 | +3 | +3 | 17 |
| Continuation | 10 | +3 | +4 | +3 | 20 |
| Contrast | 8 | +3 | +3 | +2 | 16 |
| Attribution | 4 | +2 | +2 | +1 | 9 |
| Closing | 5 | +2 | +2 | +1 | 10 |
| Concession | 4 | +2 | +2 | +1 | 9 |
| Causation | 6 | +3 | +3 | +2 | 14 |
| Elaboration | 4 | +2 | +3 | +1 | 10 |
| Sequence | 4 | +2 | +2 | +1 | 9 |
| Summary | 7 | +3 | +3 | +1 | 14 |
| **Total** | **60** | **+25** | **+27** | **+16** | **~128** |

(Approximate. Actual count may be slightly higher or lower as
duplicates are filtered.)

### Selection discipline

Every new entry must satisfy three criteria:

1. **RAE-attested** as standard panhispanic Spanish. The RAE
   *Diccionario de la lengua española* + *Diccionario
   panhispánico de dudas* are the authority. No regional
   slang. No code-switching surface forms.

2. **Inter-regional consensus**. The phrase reads as standard
   in formal Spanish across LATAM (MX, AR, CO, PE, ES) without
   marking the speaker's national origin. Tested mentally
   against "would this read natural in a corporate brief
   written in Bogotá vs Madrid vs Buenos Aires?"

3. **Role-register-polarity coherent**. The phrase actually
   belongs in the slot it occupies (no "Furthermore," tagged
   as Contrast).

### What this ADR explicitly does **not** do

- **Does not** touch `Register::Conversational`. Mario's
  authority. The 4 existing Conversational entries from
  ADR-0017 (`"Mirá, lo que tengo es,"`, `"A ver, según los
  datos,"`, `"Y encima,"`, `"Pero,"`, `"Eso sí,"`,
  `"Así que,"`, `"Eso es lo que hay registrado."`) stay
  untouched. A future ADR-0022 (Mario-led) adds Conversational
  expansion with LATAM register calibration.

- **Does not** reach EN-parity (~250 entries). The
  Conversational gap + the natural variance in EN's
  hand-curated set means ES catches up gradually, not in a
  single jump. Target ~140 with this ADR; ~250+ when ADR-0022
  + future native-speaker work lands.

- **Does not** introduce a new role or register. The 10-role
  taxonomy from ADR-0005 stays; the existing 4 registers stay.

- **Does not** translate EN entries 1-to-1. Some EN phrases
  have no idiomatic Spanish counterpart (e.g. EN's
  `"Building on it,"` translates awkwardly — the chosen ES
  analogue is `"Sumando a esto,"` from ADR-0017, which is the
  semantic match, not the syntactic one).

### Honest acknowledgment — model-drafted

The new entries in this ADR are **drafted by claude-opus-4-7
(the model writing this ADR)**. The model is not a Spanish
native speaker. The model has read substantial Spanish
discourse-markers literature in training, can recognise
canonical forms with high confidence, but cannot fully
substitute for native-speaker review.

The discipline applied:

- Confine to RAE-canonical surface forms where the model
  has high confidence (Formal/Technical registers).
- Avoid ambiguous register calls; when in doubt, label as
  Formal.
- Skip Conversational entirely.
- Document this caveat in the data file itself + this ADR.
- Tag each new entry's source quadrant in the data so a
  future native-speaker pass can locate and revise.

Mario reviews post-merge. Anything that reads stilted or
wrong gets corrected in a follow-up commit; the ADR-0021
infrastructure (additional entries spread across the same
quadrants) survives.

This is a **deliberate scope choice**, not a methodology
shift. ADR-0017's "hand-curated by Mario" remains the
production standard; ADR-0021 is a **bootstrap step** that
gets the lexicon to a usable scale faster than a strict native-
speaker-only workflow allows, with explicit accounting of the
trade-off.

## Sources

- **RAE *Diccionario de la lengua española***.
- **RAE *Diccionario panhispánico de dudas***.
- **Briz, A., Pons, S., Portolés, J. (2008).** *Diccionario
  de partículas discursivas del español.* Standard scholarly
  reference for ES discourse markers — accessed in summary
  form via the model's training data.
- **ADR-0017 §"Sources"** — the same authorities ADR-0017
  applied at 60-entry scale.

## Consequences

- ES lexicon: ~60 → ~128 entries (numbers approximate). Real
  ES output flowing through the realizer now has more
  context-appropriate phrasing per
  `(role, register, polarity, formality)` bucket; the level-1
  exact-match path of the picker's fallback chain succeeds
  more often.
- `lexical_diversity` and `role_coverage` metrics on the ES
  eval corpus improve (more distinct phrases available).
- The ADR-0010 sensitivity audit under ES is unaffected
  (per ADR-0020, audit baselines are lexicon-derived; more
  lexicon entries means more candidate phrases for the
  helpers to pick from).
- The Conversational gap is now explicit: ES has 5 Formal,
  N Neutral, M Technical openings (plenty), and just 2-3
  Conversational. A user invoking
  `Register::Conversational` semantics (via domain_tags
  triggering it) gets a narrower phrase set than EN.
  Documented in the data file and tracked for ADR-0022.
- The lexicon size invariant `baseline_es_meets_v0_2_size_floor`
  (asserts ≥ 40 entries) becomes loose; the test stays at the
  v0.2 floor rather than raising it, because raising the
  floor here would force ADR-0022 to maintain the higher
  number even if some entries get pruned during the native-
  speaker pass.

## Cross-references

- **ADR-0017** — the v0.2 architectural-proof lexicon this
  ADR scales.
- **ADR-0020** — the audit refactor that makes this scale
  expansion automatically benefit the sensitivity audit's
  helper picks.
- **`hyphae_surface::connective_data_es::baseline_es_data`** —
  the function this ADR extends.
- **Future ADR-0022** (Mario-led, reserved) — Conversational
  register expansion + native-speaker quality pass over the
  ADR-0021 drafts.
