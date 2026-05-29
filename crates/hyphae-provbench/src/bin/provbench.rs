// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Celiums Solutions LLC

//! `provbench` — run the provenance benchmark and emit a deterministic
//! table (stdout) plus an optional JSON envelope and table file.
//!
//! ```text
//! cargo run -p hyphae-provbench --release -- \
//!     --n 128 --trials 10 --seed 42 \
//!     --json papers/arxiv-preprint/tables/provenance-benchmark.json \
//!     --table papers/arxiv-preprint/tables/provenance-benchmark.txt
//! ```

use hyphae_provbench::harness::{render_table, run};

fn main() {
    let mut n: u64 = 128;
    let mut trials: u64 = 10;
    let mut seed: u64 = 42;
    let mut json_path: Option<String> = None;
    let mut table_path: Option<String> = None;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let take = |i: &mut usize| -> String {
            *i += 1;
            args.get(*i)
                .unwrap_or_else(|| {
                    eprintln!("missing value for {}", args[*i - 1]);
                    std::process::exit(2);
                })
                .clone()
        };
        match args[i].as_str() {
            "--n" => n = take(&mut i).parse().expect("--n integer"),
            "--trials" => trials = take(&mut i).parse().expect("--trials integer"),
            "--seed" => seed = take(&mut i).parse().expect("--seed integer"),
            "--json" => json_path = Some(take(&mut i)),
            "--table" => table_path = Some(take(&mut i)),
            "-h" | "--help" => {
                println!(
                    "provbench --n <N> --trials <T> --seed <S> [--json <path>] [--table <path>]"
                );
                return;
            }
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(2);
            }
        }
        i += 1;
    }

    eprintln!("running provenance benchmark: n={n} trials={trials} seed={seed} ...");
    let env = run(n, trials, seed);
    let table = render_table(&env);
    print!("{table}");

    if let Some(path) = json_path {
        let json = serde_json::to_string_pretty(&env).expect("serialise envelope");
        std::fs::write(&path, json).expect("write json");
        eprintln!("wrote envelope -> {path}");
    }
    if let Some(path) = table_path {
        std::fs::write(&path, &table).expect("write table");
        eprintln!("wrote table -> {path}");
    }
}
