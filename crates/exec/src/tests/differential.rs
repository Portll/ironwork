//! B2 (docs/codegen-runtime.md §11): mutated programs that compile run on the interpreter and on
//! the VM under a statement limit, and the two agree.

use super::{fuzz_corpus, mutate};
use crate::testing::{Executor, Harness};
use crate::{compile, unit};
use std::panic::{AssertUnwindSafe, catch_unwind};

const STATEMENT_LIMIT: u64 = 20_000;

/// Each mutated program that compiles runs through the test Harness, which runs it on both
/// executors and fails on any difference between them (docs/lir.md §12.3); one that loops ends in
/// S322 at the same statement on both. `IRONWORK_DIFFERENTIAL_ITERATIONS` raises the count for a
/// longer run, `IRONWORK_DIFFERENTIAL_SEED` mutates differently, and every failing input is written
/// to the temp directory. The runs get the stack `ironwork` gives them, which PERFORM and CALL
/// nested to their limit need.
#[test]
fn mutated_programs_run_alike_on_the_interpreter_and_the_vm() {
    std::thread::Builder::new().stack_size(64 << 20).spawn(mutated_runs).unwrap().join().unwrap();
}

fn mutated_runs() {
    let iterations: usize = std::env::var("IRONWORK_DIFFERENTIAL_ITERATIONS").ok().and_then(|v| v.parse().ok()).unwrap_or(2_000);
    let corpus = fuzz_corpus();
    let mut seed: u64 = std::env::var("IRONWORK_DIFFERENTIAL_SEED").ok().and_then(|v| v.parse().ok()).filter(|&s| s != 0).unwrap_or(0x2545_F491_4F6C_DD1D);
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut failures = Vec::new();
    for i in 0..iterations {
        let mutated = mutate(&corpus[i % corpus.len()], &mut next);
        let compiles = catch_unwind(|| {
            let main = syntax::parse_all_with(&mutated, &syntax::copy::Libraries::default()).ok().and_then(|p| p.into_iter().next());
            main.is_some_and(|p| compile(p, &[]).is_ok())
        });
        if !compiles.unwrap_or(false) {
            continue;
        }
        // Written first, so a run that aborts the process still leaves its input.
        let path = std::env::temp_dir().join(format!("ironwork-differential-{}-{i}.cbl", std::process::id()));
        std::fs::write(&path, &mutated).unwrap();
        let run = catch_unwind(AssertUnwindSafe(|| Harness::source(&mutated).statement_limit(STATEMENT_LIMIT).clock(unit::Clock::Fixed(1_790_510_400, 42)).run(Executor::Interpreter)));
        match run {
            Ok(_) => std::fs::remove_file(&path).unwrap(),
            Err(panic) => {
                let message = panic.downcast_ref::<String>().cloned().or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                failures.push(format!("{}: {}", path.display(), message.lines().take(3).collect::<Vec<_>>().join(" | ")));
            }
        }
    }
    assert!(failures.is_empty(), "{} of {iterations} mutated programs failed:\n{}", failures.len(), failures.join("\n"));
}
