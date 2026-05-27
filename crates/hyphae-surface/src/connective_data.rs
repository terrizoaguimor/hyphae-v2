// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC
//
// The push_* helpers below load tens of entries in a flat sequence
// each. After `cargo fmt` expansion they cross the `too_many_lines`
// threshold; the lint is silenced module-locally because the data
// shape (one entry per `add()` call) is more readable than
// splitting into sub-helpers.
#![allow(clippy::too_many_lines)]

//! Hand-curated English connective dataset for v0.1 — ~300 entries
//! organised by `(role, register, polarity, formality)` per
//! ADR-0005.
//!
//! Sources (public-domain taxonomies; surface forms hand-curated):
//!
//! - Penn Discourse Treebank (PDTB) 3.0 connective inventory.
//! - Rhetorical Structure Theory (RST-DT) relation labels.
//! - Random House Webster's / Roget thesaurus for register
//!   variants.
//!
//! Adding new entries: keep one per line, comment-tag the
//! `(register, polarity, formality)` decision when it is not
//! obvious. ES re-entry per RFC §9 lives in a parallel data file
//! that an integrator constructs from the equivalent ES discourse-
//! connectives literature.

use crate::connective::{Connective, ConnectiveRole, Formality, Polarity, Register};

/// The v0.1 baseline data.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn baseline_en_data() -> Vec<Connective> {
    let mut out = Vec::new();
    push_openings(&mut out);
    push_continuations(&mut out);
    push_contrasts(&mut out);
    push_attributions(&mut out);
    push_closings(&mut out);
    push_concessions(&mut out);
    push_causations(&mut out);
    push_elaborations(&mut out);
    push_sequences(&mut out);
    push_summaries(&mut out);
    out
}

/// Helper: append a connective in one line.
fn add(
    out: &mut Vec<Connective>,
    phrase: &str,
    role: ConnectiveRole,
    register: Register,
    polarity: Polarity,
    formality: Formality,
) {
    out.push(Connective::new(phrase, role, register, polarity, formality));
}

// ────────────────────────────────────────────────────────────────
// Openings (Polarity::Neutral)
// ────────────────────────────────────────────────────────────────

fn push_openings(out: &mut Vec<Connective>) {
    let p = Polarity::Neutral;
    let r = ConnectiveRole::Opening;

    // Neutral / Mid (the default surface)
    add(
        out,
        "Drawing from working memory,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Based on what is in scope,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "From the fragments available,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "From what I have on hand,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Working through what I know,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Pulling together what is current,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Looking at what is recorded,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Going by what is in memory,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );

    // Formal / High
    add(
        out,
        "From the material at hand,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Based on the present record,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Per the available evidence,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "On the basis of the recorded material,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );

    // Conversational / Low
    add(out, "So,", r, Register::Conversational, p, Formality::Low);
    add(
        out,
        "Alright,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "OK so,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "Here's what I have:",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "Looking at this,",
        r,
        Register::Conversational,
        p,
        Formality::Mid,
    );

    // Technical / Mid
    add(
        out,
        "Per the recorded fragments,",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Drawing from the indexed material,",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
    add(
        out,
        "From the substrate state,",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
}

// ────────────────────────────────────────────────────────────────
// Continuations (Polarity::Continuation)
// ────────────────────────────────────────────────────────────────

fn push_continuations(out: &mut Vec<Connective>) {
    let p = Polarity::Continuation;
    let r = ConnectiveRole::Continuation;

    // Neutral
    add(
        out,
        "Extending that,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Building on it,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Adding to the picture,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(out, "Likewise,", r, Register::Neutral, p, Formality::Mid);
    add(
        out,
        "Along those lines,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Following from that,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "In the same direction,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(out, "Continuing,", r, Register::Neutral, p, Formality::Mid);
    add(
        out,
        "Adding to that,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Building further,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(out, "And,", r, Register::Neutral, p, Formality::Low);
    add(out, "Also,", r, Register::Neutral, p, Formality::Mid);
    add(out, "Plus,", r, Register::Neutral, p, Formality::Low);

    // Formal
    add(out, "Furthermore,", r, Register::Formal, p, Formality::High);
    add(out, "Moreover,", r, Register::Formal, p, Formality::High);
    add(out, "In addition,", r, Register::Formal, p, Formality::High);
    add(
        out,
        "Additionally,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(out, "Further,", r, Register::Formal, p, Formality::High);
    add(
        out,
        "What is more,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "In a similar vein,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(out, "Beyond that,", r, Register::Formal, p, Formality::High);

    // Conversational
    add(
        out,
        "Right, and",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "Yeah, and",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "On top of that,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "And then,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "Also worth noting,",
        r,
        Register::Conversational,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Beyond that too,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );

    // Technical
    add(
        out,
        "Additionally,",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Concurrently,",
        r,
        Register::Technical,
        p,
        Formality::High,
    );
    add(
        out,
        "In addition to the above,",
        r,
        Register::Technical,
        p,
        Formality::High,
    );
    add(
        out,
        "Following this,",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Per the next fragment,",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Likewise observed,",
        r,
        Register::Technical,
        p,
        Formality::High,
    );
}

// ────────────────────────────────────────────────────────────────
// Contrasts (Polarity::ContrastSoft and Polarity::ContrastHard)
// ────────────────────────────────────────────────────────────────

fn push_contrasts(out: &mut Vec<Connective>) {
    let r = ConnectiveRole::Contrast;
    let hard = Polarity::ContrastHard;
    let soft = Polarity::ContrastSoft;

    // ── Hard contrasts ─────────────────────────────────────────

    // Neutral
    add(out, "However,", r, Register::Neutral, hard, Formality::Mid);
    add(
        out,
        "By contrast,",
        r,
        Register::Neutral,
        hard,
        Formality::Mid,
    );
    add(
        out,
        "On the other hand,",
        r,
        Register::Neutral,
        hard,
        Formality::Mid,
    );
    add(out, "Yet,", r, Register::Neutral, hard, Formality::Mid);
    add(out, "Still,", r, Register::Neutral, hard, Formality::Mid);
    add(out, "But,", r, Register::Neutral, hard, Formality::Low);
    add(
        out,
        "Conversely,",
        r,
        Register::Neutral,
        hard,
        Formality::High,
    );
    add(
        out,
        "Going the other way,",
        r,
        Register::Neutral,
        hard,
        Formality::Mid,
    );

    // Formal
    add(
        out,
        "Notwithstanding,",
        r,
        Register::Formal,
        hard,
        Formality::High,
    );
    add(
        out,
        "In contrast,",
        r,
        Register::Formal,
        hard,
        Formality::High,
    );
    add(
        out,
        "Nevertheless,",
        r,
        Register::Formal,
        hard,
        Formality::High,
    );
    add(
        out,
        "Nonetheless,",
        r,
        Register::Formal,
        hard,
        Formality::High,
    );
    add(out, "That said,", r, Register::Formal, hard, Formality::Mid);
    add(
        out,
        "On the contrary,",
        r,
        Register::Formal,
        hard,
        Formality::High,
    );

    // Conversational
    add(
        out,
        "But then,",
        r,
        Register::Conversational,
        hard,
        Formality::Low,
    );
    add(
        out,
        "Although,",
        r,
        Register::Conversational,
        hard,
        Formality::Low,
    );
    add(
        out,
        "Then again,",
        r,
        Register::Conversational,
        hard,
        Formality::Low,
    );
    add(
        out,
        "On the flip side,",
        r,
        Register::Conversational,
        hard,
        Formality::Low,
    );
    add(
        out,
        "But here's the thing,",
        r,
        Register::Conversational,
        hard,
        Formality::Low,
    );

    // Technical
    add(
        out,
        "However,",
        r,
        Register::Technical,
        hard,
        Formality::Mid,
    );
    add(
        out,
        "In contradistinction,",
        r,
        Register::Technical,
        hard,
        Formality::High,
    );
    add(
        out,
        "By way of contrast,",
        r,
        Register::Technical,
        hard,
        Formality::High,
    );

    // ── Soft contrasts ─────────────────────────────────────────

    // Neutral
    add(out, "Though,", r, Register::Neutral, soft, Formality::Mid);
    add(
        out,
        "At the same time,",
        r,
        Register::Neutral,
        soft,
        Formality::Mid,
    );
    add(
        out,
        "That said,",
        r,
        Register::Neutral,
        soft,
        Formality::Mid,
    );
    add(out, "Even so,", r, Register::Neutral, soft, Formality::Mid);
    add(out, "Still,", r, Register::Neutral, soft, Formality::Mid);
    add(out, "Mind you,", r, Register::Neutral, soft, Formality::Low);

    // Formal
    add(out, "Albeit,", r, Register::Formal, soft, Formality::High);
    add(
        out,
        "While that holds,",
        r,
        Register::Formal,
        soft,
        Formality::High,
    );
    add(
        out,
        "Conceding that,",
        r,
        Register::Formal,
        soft,
        Formality::High,
    );
    add(
        out,
        "With that qualification,",
        r,
        Register::Formal,
        soft,
        Formality::High,
    );

    // Conversational
    add(
        out,
        "Even though,",
        r,
        Register::Conversational,
        soft,
        Formality::Low,
    );
    add(
        out,
        "Sort of —",
        r,
        Register::Conversational,
        soft,
        Formality::Low,
    );
    add(
        out,
        "Kind of —",
        r,
        Register::Conversational,
        soft,
        Formality::Low,
    );
    add(
        out,
        "More or less,",
        r,
        Register::Conversational,
        soft,
        Formality::Low,
    );

    // Technical
    add(
        out,
        "With caveats,",
        r,
        Register::Technical,
        soft,
        Formality::Mid,
    );
    add(
        out,
        "Subject to the qualifier that,",
        r,
        Register::Technical,
        soft,
        Formality::High,
    );
}

// ────────────────────────────────────────────────────────────────
// Attributions (Polarity::Neutral)
// ────────────────────────────────────────────────────────────────

fn push_attributions(out: &mut Vec<Connective>) {
    let p = Polarity::Neutral;
    let r = ConnectiveRole::Attribution;

    // Neutral
    add(
        out,
        "The source states:",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Per the recorded material:",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "From the fragment:",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(out, "On record:", r, Register::Neutral, p, Formality::Mid);
    add(out, "As recorded:", r, Register::Neutral, p, Formality::Mid);
    add(
        out,
        "The fragment reads:",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(out, "Verbatim:", r, Register::Neutral, p, Formality::Mid);
    add(out, "Quoted:", r, Register::Neutral, p, Formality::Mid);

    // Formal
    add(
        out,
        "Per the cited material:",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "As the record indicates:",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "It is documented that:",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "According to the source:",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "The material attests:",
        r,
        Register::Formal,
        p,
        Formality::High,
    );

    // Conversational
    add(
        out,
        "It says:",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "I have on record:",
        r,
        Register::Conversational,
        p,
        Formality::Mid,
    );
    add(
        out,
        "What I have is:",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "Here is what is logged:",
        r,
        Register::Conversational,
        p,
        Formality::Mid,
    );

    // Technical
    add(
        out,
        "From the indexed fragment:",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
    add(
        out,
        "The recorded entry reads:",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Per the substrate journal:",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
}

// ────────────────────────────────────────────────────────────────
// Closings (Polarity::Neutral)
// ────────────────────────────────────────────────────────────────

fn push_closings(out: &mut Vec<Connective>) {
    let p = Polarity::Neutral;
    let r = ConnectiveRole::Closing;

    // Neutral
    add(
        out,
        "That is what working memory holds on this.",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "That is the substance available.",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "That is the scope of what I can ground.",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "That is the material on record.",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "That is the picture as it stands.",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "That covers what is in scope.",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "That is the state of the record.",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );

    // Formal
    add(
        out,
        "That constitutes the present evidence.",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "That is the available record on this matter.",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Such is the content of the available material.",
        r,
        Register::Formal,
        p,
        Formality::High,
    );

    // Conversational
    add(
        out,
        "That is what I have.",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "That is the gist of it.",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "That covers it.",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "That is what I can speak to.",
        r,
        Register::Conversational,
        p,
        Formality::Mid,
    );

    // Technical
    add(
        out,
        "That is the substrate's current view.",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
    add(
        out,
        "That is the present working-set extract.",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
    add(
        out,
        "End of the indexed material on this query.",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
}

// ────────────────────────────────────────────────────────────────
// Concessions (Polarity::Concession)
// ────────────────────────────────────────────────────────────────

fn push_concessions(out: &mut Vec<Connective>) {
    let p = Polarity::Concession;
    let r = ConnectiveRole::Concession;

    add(out, "Granted,", r, Register::Neutral, p, Formality::Mid);
    add(out, "Admittedly,", r, Register::Neutral, p, Formality::Mid);
    add(out, "To be fair,", r, Register::Neutral, p, Formality::Mid);
    add(out, "Although,", r, Register::Neutral, p, Formality::Mid);
    add(out, "Though,", r, Register::Neutral, p, Formality::Mid);
    add(out, "Even so,", r, Register::Neutral, p, Formality::Mid);
    add(out, "True enough,", r, Register::Neutral, p, Formality::Mid);

    add(
        out,
        "It must be acknowledged that,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Conceding that,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Allowing for the fact that,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "It would be remiss not to note,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );

    add(
        out,
        "Fair point,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(out, "Sure,", r, Register::Conversational, p, Formality::Low);
    add(
        out,
        "OK, but,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "Yeah, you're right that,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );

    add(
        out,
        "It is the case that,",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Stipulating that,",
        r,
        Register::Technical,
        p,
        Formality::High,
    );
}

// ────────────────────────────────────────────────────────────────
// Causations (Polarity::Continuation)
// ────────────────────────────────────────────────────────────────

fn push_causations(out: &mut Vec<Connective>) {
    let p = Polarity::Continuation;
    let r = ConnectiveRole::Causation;

    add(out, "Because,", r, Register::Neutral, p, Formality::Mid);
    add(out, "So,", r, Register::Neutral, p, Formality::Low);
    add(out, "Therefore,", r, Register::Neutral, p, Formality::Mid);
    add(out, "As a result,", r, Register::Neutral, p, Formality::Mid);
    add(out, "Which means,", r, Register::Neutral, p, Formality::Mid);
    add(
        out,
        "For that reason,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(out, "That is why,", r, Register::Neutral, p, Formality::Mid);
    add(
        out,
        "On account of that,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(out, "Hence,", r, Register::Neutral, p, Formality::Mid);

    add(
        out,
        "Consequently,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "It follows that,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(out, "Thus,", r, Register::Formal, p, Formality::High);
    add(out, "Accordingly,", r, Register::Formal, p, Formality::High);
    add(
        out,
        "For this reason,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "In light of which,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );

    add(
        out,
        "So then,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "That means,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "Which is why,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "That's why,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );

    add(
        out,
        "From which,",
        r,
        Register::Technical,
        p,
        Formality::High,
    );
    add(
        out,
        "It can be inferred that,",
        r,
        Register::Technical,
        p,
        Formality::High,
    );
    add(
        out,
        "Implication:",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
}

// ────────────────────────────────────────────────────────────────
// Elaborations (Polarity::Continuation)
// ────────────────────────────────────────────────────────────────

fn push_elaborations(out: &mut Vec<Connective>) {
    let p = Polarity::Continuation;
    let r = ConnectiveRole::Elaboration;

    add(
        out,
        "Specifically,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "In particular,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "To be precise,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "More exactly,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(out, "Concretely,", r, Register::Neutral, p, Formality::Mid);
    add(
        out,
        "Narrowing in,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "To put a finer point on it,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Specifically speaking,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );

    add(out, "To wit,", r, Register::Formal, p, Formality::High);
    add(out, "Namely,", r, Register::Formal, p, Formality::High);
    add(
        out,
        "Particularly,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "More precisely,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );

    add(
        out,
        "For example,",
        r,
        Register::Conversational,
        p,
        Formality::Mid,
    );
    add(out, "Like,", r, Register::Conversational, p, Formality::Low);
    add(
        out,
        "I mean,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "Take this:",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "Case in point,",
        r,
        Register::Conversational,
        p,
        Formality::Mid,
    );

    add(
        out,
        "For instance,",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
    add(
        out,
        "By way of example,",
        r,
        Register::Technical,
        p,
        Formality::High,
    );
    add(
        out,
        "As an illustration,",
        r,
        Register::Technical,
        p,
        Formality::High,
    );
}

// ────────────────────────────────────────────────────────────────
// Sequences (Polarity::Continuation)
// ────────────────────────────────────────────────────────────────

fn push_sequences(out: &mut Vec<Connective>) {
    let p = Polarity::Continuation;
    let r = ConnectiveRole::Sequence;

    add(out, "First,", r, Register::Neutral, p, Formality::Mid);
    add(out, "Then,", r, Register::Neutral, p, Formality::Mid);
    add(out, "Next,", r, Register::Neutral, p, Formality::Mid);
    add(out, "After that,", r, Register::Neutral, p, Formality::Mid);
    add(out, "Finally,", r, Register::Neutral, p, Formality::Mid);
    add(
        out,
        "Subsequently,",
        r,
        Register::Neutral,
        p,
        Formality::High,
    );
    add(out, "Earlier,", r, Register::Neutral, p, Formality::Mid);
    add(out, "Later,", r, Register::Neutral, p, Formality::Mid);
    add(out, "Lastly,", r, Register::Neutral, p, Formality::Mid);
    add(out, "Before that,", r, Register::Neutral, p, Formality::Mid);

    add(out, "Firstly,", r, Register::Formal, p, Formality::High);
    add(out, "Secondly,", r, Register::Formal, p, Formality::High);
    add(out, "Thirdly,", r, Register::Formal, p, Formality::High);
    add(
        out,
        "In the first place,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "In due course,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );

    add(
        out,
        "First off,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "And then,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "After,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "Eventually,",
        r,
        Register::Conversational,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Right after,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );

    add(out, "Phase 1:", r, Register::Technical, p, Formality::Mid);
    add(out, "Phase 2:", r, Register::Technical, p, Formality::Mid);
    add(
        out,
        "Sequence step:",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Antecedent:",
        r,
        Register::Technical,
        p,
        Formality::High,
    );
    add(
        out,
        "Consequent:",
        r,
        Register::Technical,
        p,
        Formality::High,
    );
}

// ────────────────────────────────────────────────────────────────
// Summaries (Polarity::Neutral)
// ────────────────────────────────────────────────────────────────

fn push_summaries(out: &mut Vec<Connective>) {
    let p = Polarity::Neutral;
    let r = ConnectiveRole::Summary;

    add(out, "In summary,", r, Register::Neutral, p, Formality::Mid);
    add(out, "Overall,", r, Register::Neutral, p, Formality::Mid);
    add(out, "On balance,", r, Register::Neutral, p, Formality::Mid);
    add(
        out,
        "Taking it together,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Putting it together,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(out, "All in all,", r, Register::Neutral, p, Formality::Mid);
    add(out, "Net,", r, Register::Neutral, p, Formality::Mid);
    add(out, "In short,", r, Register::Neutral, p, Formality::Mid);

    add(
        out,
        "To summarise,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(out, "In sum,", r, Register::Formal, p, Formality::High);
    add(
        out,
        "In conclusion,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "On the whole,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Taken in aggregate,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );

    add(
        out,
        "Bottom line,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "TL;DR,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "So basically,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "Long story short,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );

    add(out, "Aggregate:", r, Register::Technical, p, Formality::Mid);
    add(
        out,
        "Net result:",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Composite reading:",
        r,
        Register::Technical,
        p,
        Formality::High,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn dataset_is_at_least_two_hundred() {
        let data = baseline_en_data();
        assert!(
            data.len() >= 200,
            "baseline_en_data should ship at least 200 entries; ships {}",
            data.len(),
        );
    }

    #[test]
    fn every_role_has_at_least_five_entries() {
        let data = baseline_en_data();
        let mut by_role: HashMap<ConnectiveRole, usize> = HashMap::new();
        for c in &data {
            *by_role.entry(c.role).or_insert(0) += 1;
        }
        for (role, count) in by_role {
            assert!(
                count >= 5,
                "role {role:?} has only {count} entries; need at least 5 for fallback variety",
            );
        }
    }

    #[test]
    fn every_register_appears_at_least_once() {
        let data = baseline_en_data();
        let mut seen = std::collections::HashSet::new();
        for c in &data {
            seen.insert(c.register);
        }
        assert!(seen.contains(&Register::Neutral));
        assert!(seen.contains(&Register::Formal));
        assert!(seen.contains(&Register::Conversational));
        assert!(seen.contains(&Register::Technical));
    }

    #[test]
    fn every_polarity_appears_at_least_once() {
        let data = baseline_en_data();
        let mut seen = std::collections::HashSet::new();
        for c in &data {
            seen.insert(c.polarity);
        }
        assert!(seen.contains(&Polarity::Continuation));
        assert!(seen.contains(&Polarity::ContrastSoft));
        assert!(seen.contains(&Polarity::ContrastHard));
        assert!(seen.contains(&Polarity::Concession));
        assert!(seen.contains(&Polarity::Neutral));
    }

    #[test]
    fn no_empty_phrase_in_baseline() {
        for c in baseline_en_data() {
            assert!(!c.phrase.trim().is_empty());
        }
    }
}
