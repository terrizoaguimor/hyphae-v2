// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC
//
// Hand-curated Spanish connective dataset for v0.2 — 128 entries
// across 10 roles. ADR-0017 shipped 60 (architectural proof);
// ADR-0021 added 68 model-drafted Formal/Neutral/Technical
// entries. ADR-0022 (reserved, Mario-led) will close
// Conversational + push toward EN parity.
#![allow(clippy::too_many_lines)]

//! Hand-curated Spanish connective dataset for v0.2 — per
//! ADR-0017 (architectural proof) + ADR-0021 (scale draft).
//!
//! Sources (public-domain):
//!
//! - Cuenca, M. J. (2013) — *Connectives and discourse markers
//!   in Spanish.*
//! - Marín, R. (2003) — *Spanish discourse markers as pragmatic
//!   functions.*
//! - Brucart, J. M. (2002) — *Spanish concessive connectives.*
//! - Briz, Pons, Portolés (2008) — *Diccionario de partículas
//!   discursivas del español.*
//! - RAE *Diccionario de la lengua española* + *Diccionario
//!   panhispánico de dudas* — register and inter-regional
//!   calibration.
//!
//! ## Provenance — important read
//!
//! - **ADR-0017 entries** (the original ~60) are hand-curated
//!   by Mario (native Spanish speaker, LATAM register).
//! - **ADR-0021 entries** (the +~80 expansion to ~140) are
//!   **model-drafted by claude-opus-4-7**, drawn from RAE-
//!   canonical regionally-invariant standard Spanish. They
//!   target Formal / Neutral / Technical quadrants only —
//!   `Register::Conversational` stays at ADR-0017 size
//!   because LATAM-vs-Spain register divergence requires
//!   native-speaker authority that ADR-0022 will provide.
//! - The model-drafted entries are flagged with `// ADR-0021`
//!   markers below so a future native-speaker pass can locate
//!   and revise them.
//!
//! Discipline: Formal/Technical phrases here are limited to
//! RAE-attested standard panhispanic surface forms. No regional
//! slang. No code-switching. When in doubt the model defaulted
//! to Formal register.

use crate::connective::{Connective, ConnectiveRole, Formality, Polarity, Register};

/// The v0.2 Spanish baseline data. 128 entries (60 from
/// ADR-0017 + 68 from ADR-0021's model-drafted
/// Formal/Neutral/Technical expansion).
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

    // ── ADR-0021 expansion: Formal / Neutral / Technical ────
    add(
        out,
        "En atención a los datos disponibles,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "A tenor de los registros,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Conforme a lo documentado,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "De los registros se desprende que,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Lo registrado indica que,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Considerando lo conservado,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Desde la traza disponible,",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Según el estado del sistema,",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
    add(
        out,
        "A partir de la telemetría,",
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

    // ── ADR-0021 expansion ──
    add(
        out,
        "Aunado a lo anterior,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Adicionalmente,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Por añadidura,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(out, "A su vez,", r, Register::Neutral, p, Formality::Mid);
    add(
        out,
        "Junto con esto,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "De manera similar,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Sin perder esa línea,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "En la misma capa,",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Siguiendo el flujo,",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
    add(
        out,
        "En este mismo punto,",
        r,
        Register::Technical,
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

    // ── ADR-0021 expansion — Hard ──
    add(
        out,
        "Contrariamente,",
        r,
        Register::Formal,
        Polarity::ContrastHard,
        Formality::High,
    );
    add(
        out,
        "En contraposición,",
        r,
        Register::Formal,
        Polarity::ContrastHard,
        Formality::High,
    );
    add(
        out,
        "Aun así,",
        r,
        Register::Neutral,
        Polarity::ContrastHard,
        Formality::Mid,
    );
    add(
        out,
        "Pese a ello,",
        r,
        Register::Neutral,
        Polarity::ContrastHard,
        Formality::Mid,
    );
    add(
        out,
        "En contraste,",
        r,
        Register::Technical,
        Polarity::ContrastHard,
        Formality::Mid,
    );

    // ── ADR-0021 expansion — Soft ──
    add(
        out,
        "Si bien es cierto que,",
        r,
        Register::Formal,
        Polarity::ContrastSoft,
        Formality::High,
    );
    add(
        out,
        "Aunque, por otro lado,",
        r,
        Register::Neutral,
        Polarity::ContrastSoft,
        Formality::Mid,
    );
    add(
        out,
        "Con la salvedad de que,",
        r,
        Register::Technical,
        Polarity::ContrastSoft,
        Formality::Mid,
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

    // ── ADR-0021 expansion ──
    add(
        out,
        "La documentación señala:",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "El expediente recoge:",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Lo registrado expresa:",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Se observa en los datos:",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "El log indica:",
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

    // ── ADR-0021 expansion ──
    add(
        out,
        "Tal es el contenido conservado.",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Esos son los datos disponibles.",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Eso es cuanto la memoria conserva.",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Hasta ahí llega la traza.",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Tal es el estado registrado.",
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

    // ── ADR-0021 expansion ──
    add(
        out,
        "Si bien se reconoce que,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Cabe admitir que,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Hay que tener presente que,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Conviene reconocer que,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Aceptando que,",
        r,
        Register::Technical,
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

    // ── ADR-0021 expansion ──
    add(
        out,
        "En virtud de ello,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Por esa razón,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Por tal motivo,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(out, "De modo que,", r, Register::Neutral, p, Formality::Mid);
    add(out, "Por eso,", r, Register::Neutral, p, Formality::Mid);
    add(
        out,
        "Lo cual lleva a que,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "En consecuencia técnica,",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Esto deriva en que,",
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

    // ── ADR-0021 expansion ──
    add(
        out,
        "Cabe precisar que,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Conviene matizar que,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Dicho de otro modo,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Para detallarlo,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Con más detalle,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Específicamente en este punto,",
        r,
        Register::Technical,
        p,
        Formality::Mid,
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

    // ── ADR-0021 expansion ──
    add(
        out,
        "En primer término,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "En último término,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Por una parte,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
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
        "En la primera etapa,",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
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

    // ── ADR-0021 expansion ──
    add(
        out,
        "En definitiva,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Recapitulando,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "En última instancia,",
        r,
        Register::Formal,
        p,
        Formality::High,
    );
    add(
        out,
        "Para cerrarlo,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Tomando todo en cuenta,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Visto en conjunto,",
        r,
        Register::Neutral,
        p,
        Formality::Mid,
    );
    add(
        out,
        "Resumiendo lo registrado,",
        r,
        Register::Technical,
        p,
        Formality::Mid,
    );
}
