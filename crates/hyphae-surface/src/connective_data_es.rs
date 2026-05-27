// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC
//
// Hand-curated Spanish connective dataset for v0.2 — ~60 entries
// (architectural proof, not full coverage). ADR-0017 ships this
// scale; a future ADR-0018 scales to ~250+ to match EN.
#![allow(clippy::too_many_lines)]

//! Hand-curated Spanish connective dataset for v0.2 — per
//! ADR-0017.
//!
//! Sources (public-domain):
//!
//! - Cuenca, M. J. (2013) — *Connectives and discourse markers
//!   in Spanish.*
//! - Marín, R. (2003) — *Spanish discourse markers as pragmatic
//!   functions.*
//! - Brucart, J. M. (2002) — *Spanish concessive connectives.*
//! - RAE *Diccionario panhispánico de dudas* — register
//!   calibration.
//!
//! Every entry is hand-curated. No machine translation. The
//! v0.2 scope is "prove the architecture supports a second
//! language"; ADR-0018 (when filed) takes the count to EN
//! parity.

use crate::connective::{Connective, ConnectiveRole, Formality, Polarity, Register};

/// The v0.2 Spanish baseline data. ~60 entries.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn baseline_es_data() -> Vec<Connective> {
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

    add(
        out,
        "De acuerdo a la memoria de trabajo,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Según los fragmentos registrados,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Conforme a lo conservado,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Mirá, lo que tengo es,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "A ver, según los datos,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "Desde los registros técnicos,",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Sobre la base de lo almacenado,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Partiendo de la memoria disponible,",
        r,
        Register::Neutral,
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

    add(out, "Además,", r, Register::Neutral, p, Formality::Mid);
    add(out, "Asimismo,", r, Register::Formal, p, Formality::High);
    add(out, "También,", r, Register::Neutral, p, Formality::Mid);
    add(
        out,
        "Por otra parte,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "En la misma línea,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "De igual modo,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(out, "Igualmente,", r, Register::Neutral, p, Formality::Mid);
    add(
        out,
        "Y encima,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "Sumando a esto,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Continuando con la línea,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
}

// ────────────────────────────────────────────────────────────────
// Contrasts (Polarity::ContrastSoft + ContrastHard)
// ────────────────────────────────────────────────────────────────

fn push_contrasts(out: &mut Vec<Connective>) {
    let r = ConnectiveRole::Contrast;

    // Hard contrast
    add(
        out,
        "Sin embargo,",
        r,
        Register::Neutral,
        Polarity::ContrastHard,
        Formality::Mid,
    );
    add(
        out,
        "No obstante,",
        r,
        Register::Formal,
        Polarity::ContrastHard,
        Formality::High,
    );
    add(
        out,
        "Por el contrario,",
        r,
        Register::Formal,
        Polarity::ContrastHard,
        Formality::High,
    );
    add(
        out,
        "En cambio,",
        r,
        Register::Neutral,
        Polarity::ContrastHard,
        Formality::Mid,
    );
    add(
        out,
        "Pero,",
        r,
        Register::Conversational,
        Polarity::ContrastHard,
        Formality::Low,
    );

    // Soft contrast
    add(
        out,
        "Aunque,",
        r,
        Register::Neutral,
        Polarity::ContrastSoft,
        Formality::Mid,
    );
    add(
        out,
        "Si bien,",
        r,
        Register::Formal,
        Polarity::ContrastSoft,
        Formality::High,
    );
    add(
        out,
        "Eso sí,",
        r,
        Register::Conversational,
        Polarity::ContrastSoft,
        Formality::Low,
    );
}

// ────────────────────────────────────────────────────────────────
// Attributions (Polarity::Neutral)
// ────────────────────────────────────────────────────────────────

fn push_attributions(out: &mut Vec<Connective>) {
    let p = Polarity::Neutral;
    let r = ConnectiveRole::Attribution;

    add(
        out,
        "El registro indica:",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Según lo conservado:",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "La fuente declara:",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Lo almacenado dice:",
        r,
        Register::Neutral,
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

    add(
        out,
        "Eso es lo que la memoria de trabajo conserva.",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Esa es la sustancia disponible.",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Esa es la visión actual del sustrato.",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Eso es lo que hay registrado.",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(
        out,
        "Hasta aquí lo retenido.",
        r,
        Register::Neutral,
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

    add(
        out,
        "Hay que reconocer,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Reconozcámoslo,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Hay que admitir,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Es cierto que,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
}

// ────────────────────────────────────────────────────────────────
// Causations (Polarity::Continuation, but a different role)
// ────────────────────────────────────────────────────────────────

fn push_causations(out: &mut Vec<Connective>) {
    let p = Polarity::Continuation;
    let r = ConnectiveRole::Causation;

    add(
        out,
        "Por lo tanto,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "En consecuencia,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Por consiguiente,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Como resultado,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Así que,",
        r,
        Register::Conversational,
        p,
        Formality::Low,
    );
    add(out, "De ahí que,", r, Register::Formal, p, Formality::High);
}

// ────────────────────────────────────────────────────────────────
// Elaborations (Polarity::Continuation)
// ────────────────────────────────────────────────────────────────

fn push_elaborations(out: &mut Vec<Connective>) {
    let p = Polarity::Continuation;
    let r = ConnectiveRole::Elaboration;

    add(
        out,
        "Específicamente,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Más concretamente,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "En particular,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Para precisarlo,",
        r,
        Register::Formal,
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

    add(out, "Primero,", r, Register::Neutral, p, Formality::Mid);
    add(out, "Luego,", r, Register::Neutral, p, Formality::Mid);
    add(
        out,
        "A continuación,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(out, "Finalmente,", r, Register::Neutral, p, Formality::Mid);
}

// ────────────────────────────────────────────────────────────────
// Summaries (Polarity::Neutral) — ADR-0016 role exercised in ES
// ────────────────────────────────────────────────────────────────

fn push_summaries(out: &mut Vec<Connective>) {
    let p = Polarity::Neutral;
    let r = ConnectiveRole::Summary;

    add(out, "En resumen,", r, Register::Neutral, p, Formality::Mid);
    add(out, "En síntesis,", r, Register::Formal, p, Formality::High);
    add(out, "En general,", r, Register::Neutral, p, Formality::Mid);
    add(out, "En conjunto,", r, Register::Neutral, p, Formality::Mid);
    add(
        out,
        "Para resumir,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(out, "En balance,", r, Register::Formal, p, Formality::High);
    add(
        out,
        "Sumando lo dicho,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
}
