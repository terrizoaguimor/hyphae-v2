// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! # hyphae-chat
//!
//! Interactive chat REPL for Hyphae v2 — per ADR-0014.
//!
//! One process, one `Substrate`, one `LearningOrchestrator`. The
//! substrate persists in a `tempdir` for the session and is dropped
//! cleanly on exit. State transitions `Encoding ↔ Recall` per turn
//! based on the operation.
//!
//! ## Turn semantics
//!
//! - Input ending with `?` → **recall**: pass as cue to
//!   `substrate.recall_signal`, then compose against the resulting
//!   working set.
//! - Otherwise → **ingest**: route through the encoding pathways
//!   so `Reward` produces an RPE that the orchestrator records.
//! - Slash commands (`/help`, `/quit`, `/remember`, `/recall`,
//!   `/stats`, `/learn`) override the heuristic.
//!
//! ## Run
//!
//! ```bash
//! cargo run -p hyphae-chat
//! ```

#![warn(clippy::pedantic)]

use anyhow::Result;
use hyphae_core::{
    ActorContext, CognitiveFragment, DirectPathway, ExternalInputPayload, FragmentContent, Pathway,
    PathwayId, PayloadKind, State, SubsystemId,
};
use hyphae_learning::LearningOrchestrator;
use hyphae_substrate::Substrate;
use hyphae_subsystems::{Composer, Episodic, InputGate, Predictive, Reward, Valence};
use hyphae_surface::{Intent, RealizationRequest, SurfaceRealizer};
use std::io::{BufRead, Write};
use std::time::Instant;
use tempfile::tempdir;

/// Per-session counters surfaced by `/stats`.
#[derive(Default)]
struct SessionStats {
    ingestions: usize,
    recalls: usize,
    last_audit_seq: Option<u64>,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    print_banner();

    let dir = tempdir()?;
    let mut substrate = Substrate::new(dir.path())?;
    register_subsystems(&mut substrate)?;
    register_pathways(&mut substrate);

    let mut orchestrator = LearningOrchestrator::new();
    let realizer = SurfaceRealizer::new();
    let actor = ActorContext::new("chat:user", "memory:write");
    let mut stats = SessionStats::default();

    println!("hyphae v0.1.0 chat — type `/help` for commands, `/quit` (or Ctrl-D) to exit\n");

    let stdin = std::io::stdin();
    let mut line = String::new();
    let stdin_lock = stdin.lock();
    let mut reader = std::io::BufReader::new(stdin_lock);

    loop {
        prompt()?;
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            // EOF (Ctrl-D)
            println!();
            break;
        }
        let input = line.trim().to_string();
        if input.is_empty() {
            continue;
        }

        let action = classify(&input);
        match dispatch(
            action,
            &mut substrate,
            &mut orchestrator,
            &realizer,
            &actor,
            &mut stats,
        )
        .await?
        {
            TurnOutcome::Continue => {}
            TurnOutcome::Quit => break,
        }
    }

    println!(
        "hyphae > session ended ({} ingestions, {} recalls)",
        stats.ingestions, stats.recalls
    );
    drop(substrate);
    Ok(())
}

/// What the user wants this turn.
#[derive(Debug, Clone)]
enum Action {
    Quit,
    Help,
    Stats,
    Learn,
    Remember(String),
    Recall(String),
}

enum TurnOutcome {
    Continue,
    Quit,
}

/// Classify a raw input line into an [`Action`]. Slash commands win
/// over the question-mark heuristic.
fn classify(input: &str) -> Action {
    if let Some(rest) = input.strip_prefix('/') {
        let mut parts = rest.splitn(2, char::is_whitespace);
        let cmd = parts.next().unwrap_or("").to_lowercase();
        let arg = parts.next().unwrap_or("").trim().to_string();
        return match cmd.as_str() {
            "quit" | "exit" => Action::Quit,
            "stats" => Action::Stats,
            "learn" => Action::Learn,
            "remember" => Action::Remember(arg),
            "recall" => Action::Recall(arg),
            // `/help`, `/?`, and unknown commands all print the
            // command list — minimal v0.2 surface per ADR-0014.
            _ => Action::Help,
        };
    }
    if input.trim_end().ends_with('?') {
        Action::Recall(input.to_string())
    } else {
        Action::Remember(input.to_string())
    }
}

async fn dispatch(
    action: Action,
    substrate: &mut Substrate,
    orchestrator: &mut LearningOrchestrator,
    realizer: &SurfaceRealizer,
    actor: &ActorContext,
    stats: &mut SessionStats,
) -> Result<TurnOutcome> {
    match action {
        Action::Quit => Ok(TurnOutcome::Quit),
        Action::Help => {
            print_help();
            Ok(TurnOutcome::Continue)
        }
        Action::Stats => {
            print_stats(stats, orchestrator);
            Ok(TurnOutcome::Continue)
        }
        Action::Learn => {
            do_learn(substrate, orchestrator, actor, stats).await?;
            Ok(TurnOutcome::Continue)
        }
        Action::Remember(text) => {
            if text.is_empty() {
                println!("hyphae > /remember needs text\n");
                return Ok(TurnOutcome::Continue);
            }
            do_ingest(&text, substrate, orchestrator, actor, stats).await?;
            Ok(TurnOutcome::Continue)
        }
        Action::Recall(text) => {
            if text.is_empty() {
                println!("hyphae > /recall needs a cue\n");
                return Ok(TurnOutcome::Continue);
            }
            do_recall(&text, substrate, realizer, actor, stats).await?;
            Ok(TurnOutcome::Continue)
        }
    }
}

async fn do_ingest(
    text: &str,
    substrate: &mut Substrate,
    orchestrator: &mut LearningOrchestrator,
    actor: &ActorContext,
    stats: &mut SessionStats,
) -> Result<()> {
    ensure_state(substrate, State::Encoding).await?;
    let started = Instant::now();
    let out = substrate
        .ingest(ExternalInputPayload::new(text.to_string()), actor.clone())
        .await?;
    let dt = started.elapsed();
    for terminal in &out.terminals {
        orchestrator.record_emission(terminal, Some(&out.ethics));
    }
    stats.ingestions += 1;
    if let Some(seq) = out.ethics.audit_seq {
        stats.last_audit_seq = Some(seq);
    }
    println!(
        "hyphae > [stored | terminals={} | ethics_cvar={:.4} | {}ms]\n",
        out.terminals.len(),
        out.ethics.cvar_score,
        dt.as_millis(),
    );
    Ok(())
}

async fn do_recall(
    cue: &str,
    substrate: &mut Substrate,
    realizer: &SurfaceRealizer,
    actor: &ActorContext,
    stats: &mut SessionStats,
) -> Result<()> {
    ensure_state(substrate, State::Recall).await?;
    let started = Instant::now();
    let recall_out = substrate.recall_signal(cue, actor.clone()).await?;
    let recall_ms = started.elapsed().as_millis();

    let working_set: Vec<CognitiveFragment> = recall_out
        .terminals
        .iter()
        .filter(|f| !matches!(&f.content, FragmentContent::Observation { body } if body == cue))
        .cloned()
        .collect();
    let cascade_count = working_set
        .iter()
        .filter(|f| !f.provenance.parent_ids.is_empty())
        .count();

    let started = Instant::now();
    let realization = realizer.realize(&RealizationRequest {
        intent: Intent::Dialogue,
        query: cue,
        working_set: &working_set,
        shape: None,
        ethics: None,
    })?;
    let compose_ms = started.elapsed().as_millis();

    println!("hyphae >");
    for line in realization.text.lines() {
        println!("  {line}");
    }
    println!(
        "  (recall: {} terminals, {} with parent_ids tagged | compose: {}ms | recall: {}ms)\n",
        working_set.len(),
        cascade_count,
        compose_ms,
        recall_ms,
    );

    stats.recalls += 1;
    if let Some(seq) = recall_out.ethics.audit_seq {
        stats.last_audit_seq = Some(seq);
    }
    Ok(())
}

async fn do_learn(
    substrate: &Substrate,
    orchestrator: &mut LearningOrchestrator,
    actor: &ActorContext,
    stats: &mut SessionStats,
) -> Result<()> {
    let pending = orchestrator.pending_signal_count();
    if pending == 0 {
        println!("hyphae > [learn] no pending signals\n");
        return Ok(());
    }
    declare_bounds_for_pending_rpe(orchestrator);
    let started = Instant::now();
    let outputs = orchestrator
        .drain_and_propose(substrate, actor.clone())
        .await?;
    let dt = started.elapsed();
    println!(
        "hyphae > [learn] drained {} signal(s) into {} audited proposal(s) ({}ms)\n",
        pending,
        outputs.len(),
        dt.as_millis(),
    );
    if let Some(last) = outputs.last().and_then(|o| o.audit_seq) {
        stats.last_audit_seq = Some(last);
    }
    Ok(())
}

async fn ensure_state(substrate: &Substrate, target: State) -> Result<()> {
    let current = substrate.current_state().await;
    if current != target {
        substrate.transition_to(target).await?;
    }
    Ok(())
}

fn declare_bounds_for_pending_rpe(orch: &mut LearningOrchestrator) {
    use hyphae_learning::{FeedbackSignal, ParameterBounds, ParameterValue};
    use hyphae_substrate::LearningTarget;
    let edges: Vec<String> = orch
        .loop_ref()
        .pending_signals()
        .iter()
        .filter_map(|s| match s {
            FeedbackSignal::RewardPredictionError { edge_hint, .. } => edge_hint.clone(),
            _ => None,
        })
        .collect();
    for edge_id in edges {
        let target = LearningTarget::EpisodicConductivityWeight { edge_id };
        let store = orch.loop_mut().store_mut();
        store.set_bounds(&target, ParameterBounds::new(-1.0, 1.0));
        store.seed(&target, ParameterValue::Scalar(0.0));
    }
}

fn print_stats(stats: &SessionStats, orchestrator: &LearningOrchestrator) {
    println!("hyphae > [stats]");
    println!("  ingestions       : {}", stats.ingestions);
    println!("  recalls          : {}", stats.recalls);
    println!(
        "  last_audit_seq   : {}",
        stats
            .last_audit_seq
            .map_or("(none)".to_string(), |s| s.to_string()),
    );
    println!(
        "  pending_signals  : {}",
        orchestrator.pending_signal_count()
    );
    println!();
}

fn print_help() {
    println!("hyphae > [commands]");
    println!("  /help                  show this list");
    println!("  /quit  (or Ctrl-D)     exit");
    println!("  /remember <text>       force ingest");
    println!("  /recall <cue>          force recall + compose");
    println!("  /stats                 session counters");
    println!("  /learn                 drain learning orchestrator");
    println!("  (plain text)           ingest if no `?`, recall otherwise");
    println!();
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();
}

fn print_banner() {
    println!();
    println!("  ╭──────────────────────────────────────────────────────╮");
    println!("  │              h y p h a e   v 0 . 1                   │");
    println!("  │      cognitive substrate — chat REPL                 │");
    println!("  ╰──────────────────────────────────────────────────────╯");
    println!();
}

fn prompt() -> std::io::Result<()> {
    print!("you   > ");
    std::io::stdout().flush()
}

fn register_subsystems(substrate: &mut Substrate) -> Result<()> {
    substrate.register(Box::new(InputGate::new()))?;
    substrate.register(Box::new(Episodic::new()))?;
    substrate.register(Box::new(Valence::new()))?;
    substrate.register(Box::new(Composer::new()))?;
    substrate.register(Box::new(Predictive::new()))?;
    substrate.register(Box::new(Reward::new()))?;
    Ok(())
}

fn register_pathways(substrate: &mut Substrate) {
    substrate.register_pathway(direct_pathway(
        1,
        PayloadKind::Encoding,
        SubsystemId::InputGate,
        SubsystemId::Episodic,
    ));
    substrate.register_pathway(direct_pathway(
        2,
        PayloadKind::Encoding,
        SubsystemId::InputGate,
        SubsystemId::Valence,
    ));
    substrate.register_pathway(direct_pathway(
        3,
        PayloadKind::Encoding,
        SubsystemId::Valence,
        SubsystemId::Reward,
    ));
}

fn direct_pathway(
    id: u32,
    kind: PayloadKind,
    src: SubsystemId,
    dst: SubsystemId,
) -> Box<dyn Pathway> {
    Box::new(DirectPathway::new(PathwayId(id), kind, src, dst))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_routes_question_to_recall() {
        match classify("what is the status?") {
            Action::Recall(s) => assert!(s.ends_with('?')),
            other => panic!("expected Recall, got {other:?}"),
        }
    }

    #[test]
    fn classify_routes_statement_to_remember() {
        match classify("the deploy succeeded") {
            Action::Remember(s) => assert_eq!(s, "the deploy succeeded"),
            other => panic!("expected Remember, got {other:?}"),
        }
    }

    #[test]
    fn classify_slash_quit_returns_quit() {
        assert!(matches!(classify("/quit"), Action::Quit));
        assert!(matches!(classify("/exit"), Action::Quit));
    }

    #[test]
    fn classify_slash_help_returns_help() {
        assert!(matches!(classify("/help"), Action::Help));
        assert!(matches!(classify("/?"), Action::Help));
    }

    #[test]
    fn classify_slash_remember_with_text() {
        match classify("/remember the cake is a lie") {
            Action::Remember(s) => assert_eq!(s, "the cake is a lie"),
            other => panic!("expected Remember, got {other:?}"),
        }
    }

    #[test]
    fn classify_slash_remember_without_text() {
        match classify("/remember") {
            Action::Remember(s) => assert!(s.is_empty()),
            other => panic!("expected Remember(empty), got {other:?}"),
        }
    }

    #[test]
    fn classify_unknown_slash_returns_help() {
        assert!(matches!(classify("/banana"), Action::Help));
    }

    #[test]
    fn classify_slash_recall_with_text() {
        match classify("/recall what happened with the deploy") {
            Action::Recall(s) => assert_eq!(s, "what happened with the deploy"),
            other => panic!("expected Recall, got {other:?}"),
        }
    }

    #[test]
    fn classify_handles_trailing_whitespace_before_question_mark() {
        match classify("anyone home?  ") {
            Action::Recall(s) => assert!(s.contains('?')),
            other => panic!("expected Recall, got {other:?}"),
        }
    }
}
