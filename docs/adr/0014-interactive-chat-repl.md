<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright 2026 Celiums Solutions LLC -->

---
adr: 0014
title: Interactive chat REPL — conversational substrate demo
status: accepted
date: 2026-05-27
decision-makers: [mario]
triangulated-by: [claude-opus-4-7 (v0.1 → v0.2 transition review)]
---

# 0014 — Interactive chat REPL: conversational substrate demo

## Context

v0.1 (tag `v0.1.0`, commit `84a69b9`) is spec-complete: every
RFC §1–§10 commitment is implemented, the smoke binary
exercises every component in a single run, the eval harness
reports honest numbers. The bet — *useful cognitive interaction
without LLM in the cognition path* — is implementable, but is
**not yet demonstrable as a conversation**.

The smoke binary runs the substrate in a **single pass** and
exits. The closest the v0.1 surface gets to a conversation is
the eval harness, which scores 25 isolated query/seed pairs. A
person who wants to *talk to* Hyphae and see persistent memory
across turns has no entry point.

Mario's Fase B from the 2026-05-27 morning roadmap explicitly
called for: *CLI or HTTP server that exposes the 5 operations,
"conversation with Hyphae" demo reproducible, baseline of
performance on commodity hardware*.

The cleanest first step is **an interactive REPL** — single
binary, no network surface, substrate state persistent across
turns, latency observable per turn. HTTP and library-style
APIs land later when external integration becomes a concrete
ask.

## Decision

**Add `crates/hyphae-chat`: a single-process REPL that
instantiates one `Substrate` and one `LearningOrchestrator`,
runs a read-process-print loop, and exits cleanly on `/quit`
or EOF.** The substrate persists in a `tempdir` for the
session; a future ADR introduces explicit persistence opt-in
across invocations.

### Turn semantics — heuristic mode selection

The REPL chooses between **ingest** and **recall + compose**
based on a punctuation heuristic:

- Input ending with `?` → **recall**: pass as cue to
  `substrate.recall_signal`, then compose against the
  resulting working set.
- Otherwise → **ingest**: pass as observation to
  `substrate.ingest`, route through the encoding pathways
  (`InputGate → Episodic`, `InputGate → Valence`, `Valence →
  Reward`).

The heuristic is **stupid on purpose**. v0.1 has no intent
classifier; pretending to have one with NLP shape is the kind
of LLM-shape design the architecture rejects. A `?` is the
strongest available signal that the user wants retrieval. The
slash command set covers cases where the heuristic is wrong.

### Slash commands

Explicit overrides + diagnostics, listed in `/help`:

| Command | Effect |
|---|---|
| `/help` | Print available commands. |
| `/quit` (or EOF / Ctrl-D) | Exit cleanly. |
| `/remember <text>` | Force ingest, regardless of punctuation. |
| `/recall <text>` | Force recall + compose. |
| `/stats` | Show pending learning signals, last audit_seq, fragment count. |
| `/learn` | Drain the orchestrator through the substrate. |
| `/journal [N]` | Show the last `N` (default 5) journal entry tags. |

The set is deliberately minimal. Aliases (`/r`, `/q`) are
deferred until empirical use surfaces friction; v0.1
discipline preserves "no convenience shortcuts before honest
demand."

### Per-turn output discipline

Each turn prints, in order:

1. The result of the operation (composition for recall,
   `[stored]` for ingest).
2. A single-line trace tail with latency and counts —
   `(recall: N direct + M cascade | compose: Xms)` or
   `(ingest: ethics@Remember audit_seq=K)`.
3. The next prompt.

No emoji, no boxed UI, no colour. The smoke binary's narrative
banner is acceptable for a single-shot ceremony; the REPL is
operational — minimal noise per turn so latency is visible.

### Substrate lifecycle

One `Substrate::new(tempdir)` at startup. Substrate state
transitions:

- Construction → `State::Encoding` (default).
- First `?` input triggers `transition_to(State::Recall)`
  before the recall.
- After the recall, transition back to `Encoding` if the next
  input requires ingest, so the state machine reflects the
  intent.

A future ADR can introduce dedicated `Mixed` or
`Conversational` state when the state-machine design demands
it. v0.1's five-state taxonomy is sufficient; the REPL
toggles `Encoding ↔ Recall` per turn.

### Learning loop

The orchestrator records emissions from every
`substrate.ingest` and is **drained on demand** (via
`/learn`), not after every turn. This keeps each turn's
latency budget within the substrate operations themselves;
learning is a deliberate side-effect the user invokes when
they want to observe parameter changes.

If `/learn` is never invoked during a session, signals
accumulate. `/stats` reports the pending count so the user
can decide whether to drain.

### What this ADR explicitly does **not** do

- **Does not** add a new external dependency. `std::io` is
  sufficient for line-buffered REPL. Arrow-key history,
  syntax highlighting, multi-line input — all deferred to a
  future polish ADR.
- **Does not** persist across invocations. Each `hyphae-chat`
  run creates a fresh `tempdir`. Persistent-session mode
  (open + close existing substrate path) is a separate ADR
  with concrete contract for chain replay on reopen.
- **Does not** expose a network surface. HTTP server is a
  separate ADR with auth + multi-tenant decisions.
- **Does not** add NLP. The `?` heuristic is the entire
  intent classifier. The "user said something declarative or
  asked a question" distinction is a 1-bit signal, which is
  what the heuristic encodes.
- **Does not** introduce scripting / macros / configuration
  files. Pure REPL.

## Sources

- **Roadmap, 2026-05-27 morning session** — "Fase B: CLI or
  HTTP server, conversation demo, performance baseline."
- **`crates/hyphae-smoke/src/main.rs`** — single-pass
  reference for the operation sequence. The chat REPL is the
  same operations in a loop.
- **`hyphae_substrate::Substrate`** — the type the REPL
  drives.
- **`hyphae_learning::LearningOrchestrator`** — the type the
  `/learn` command drives.

## Consequences

- v0.2's first deliverable: a demo that fits in one terminal
  window. The substrate becomes visibly conversational.
- One new crate (`hyphae-chat`), one new binary target. No
  new runtime dependencies; `tokio` reused via workspace.
- The smoke binary stays as the single-pass ceremony. Chat
  REPL is the interactive complement; both live, neither
  replaces the other.
- Per-turn latency observable from day one — sets up the
  performance baseline ADR (Fase B milestone 2) with a
  concrete probe point.
- The `?` heuristic creates a documented failure mode: a
  declarative sentence that happens to end with `?` (a
  rhetorical question, an emoticon-style note) gets routed
  to recall. Acceptable for v0.2; user uses `/remember` to
  override.

## Cross-references

- **ADR-0011** — cascade activation that the REPL exercises
  on every `?` turn.
- **ADR-0013** — learning orchestration that `/learn`
  drives.
- **RFC §9 "What is NOT in v0.1"** — plugin layer for LLM
  hosts is still deferred; this ADR adds an interactive demo
  but not a hosted integration.
