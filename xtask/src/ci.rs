// Copyright 2026 Joseph Anthony Abbott III
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! `cargo xtask ci`: the PR gate. fmt, clippy and the tests, then each
//! project check, stopping at the first failure; then a summary table.

use crate::metadata::Cargo;
use crate::{CHECKS, Check, Workspace, report};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

/// The Cargo commands that run before the project checks
/// (`docs/04-quality.md`, "`xtask` checks"). Those that resolve the
/// dependency graph are `--locked --offline`, as every Cargo command
/// `xtask` runs is.
const CARGO_STEPS: [(&str, &[&str]); 3] = [
    ("fmt", &["fmt", "--all", "--", "--check"]),
    (
        "clippy",
        &[
            "clippy",
            "--locked",
            "--offline",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
    ),
    (
        "test",
        &[
            "test",
            "--locked",
            "--offline",
            "--workspace",
            "--all-features",
        ],
    ),
];

/// A step of the gate.
enum Step {
    Cargo(&'static [&'static str]),
    Check(Check),
}

/// Runs the gate in the workspace at `root` and says whether it passed.
pub(crate) fn run(root: &Path) -> bool {
    let cargo = Cargo::new(root.join("Cargo.toml"));
    let steps: Vec<(&str, Step)> = CARGO_STEPS
        .iter()
        .map(|&(name, args)| (name, Step::Cargo(args)))
        .chain(
            CHECKS
                .iter()
                .map(|&(name, check)| (name, Step::Check(check))),
        )
        .collect();
    let mut workspace: Option<Workspace> = None;
    let outcomes = run_until_failure(&steps, |(name, step)| {
        say!("\n==> {name}");
        match step {
            Step::Cargo(args) => {
                say!("cargo {}", args.join(" "));
                match Command::new(cargo.program())
                    .args(*args)
                    .current_dir(root)
                    .status()
                {
                    Ok(status) => status.success(),
                    Err(e) => {
                        complain!("{name}: cannot run cargo: {e}");
                        false
                    }
                }
            }
            Step::Check(check) => {
                if workspace.is_none() {
                    match Workspace::load(root) {
                        Ok(loaded) => workspace = Some(loaded),
                        Err(e) => return report(name, Err(e)),
                    }
                }
                workspace
                    .as_ref()
                    .is_some_and(|workspace| report(name, check(workspace)))
            }
        }
    });
    let names: Vec<&str> = steps.iter().map(|(name, _)| *name).collect();
    say!();
    for line in summary(&names, &outcomes) {
        say!("{line}");
    }
    all_passed(&outcomes)
}

/// Whether every step ran and passed.
fn all_passed(outcomes: &[Option<(bool, Duration)>]) -> bool {
    outcomes
        .iter()
        .all(|outcome| outcome.is_some_and(|(passed, _)| passed))
}

/// Runs `steps` in order until one fails. Each outcome is whether the step
/// passed and how long it took, or `None` for a step after a failure,
/// which does not run.
fn run_until_failure<T>(
    steps: &[T],
    mut run: impl FnMut(&T) -> bool,
) -> Vec<Option<(bool, Duration)>> {
    let mut failed = false;
    steps
        .iter()
        .map(|step| {
            if failed {
                return None;
            }
            let started = Instant::now();
            let passed = run(step);
            failed = !passed;
            Some((passed, started.elapsed()))
        })
        .collect()
}

/// The summary table.
fn summary(names: &[&str], outcomes: &[Option<(bool, Duration)>]) -> Vec<String> {
    let mut lines = vec![format!(
        "{:<20} {:<8} {:>8}",
        "ci step", "result", "seconds"
    )];
    for (name, outcome) in names.iter().zip(outcomes) {
        lines.push(match outcome {
            Some((passed, took)) => {
                let result = if *passed { "ok" } else { "FAILED" };
                format!("{name:<20} {result:<8} {:>8.1}", took.as_secs_f64())
            }
            None => format!("{name:<20} {:<8} {:>8}", "not run", "-"),
        });
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::{CARGO_STEPS, all_passed, run_until_failure, summary};
    use std::time::Duration;

    #[test]
    fn the_gate_stops_at_the_first_failure() {
        let mut ran = Vec::new();
        let outcomes = run_until_failure(&[true, true, false, true, true], |&passes| {
            ran.push(passes);
            passes
        });
        assert_eq!(ran, [true, true, false], "nothing runs after a failure");
        let results: Vec<Option<bool>> = outcomes
            .iter()
            .map(|o| o.map(|(passed, _)| passed))
            .collect();
        assert_eq!(results, [Some(true), Some(true), Some(false), None, None]);
        assert!(!all_passed(&outcomes));
        assert!(all_passed(&run_until_failure(&[true, true], |&passes| {
            passes
        })));
        // A failure at the end fails the gate too.
        assert!(!all_passed(&run_until_failure(&[true, false], |&passes| {
            passes
        })));
        assert!(all_passed(&[]));
    }

    #[test]
    fn cargo_steps_resolve_locked_and_offline() {
        for (name, args) in CARGO_STEPS {
            let resolves = name != "fmt";
            for flag in ["--locked", "--offline"] {
                assert_eq!(args.contains(&flag), resolves, "{name} {flag}");
            }
        }
        // The commands of docs/04-quality.md, "`xtask` checks", exactly.
        let commands: Vec<String> = CARGO_STEPS
            .iter()
            .map(|(name, args)| format!("{name}: cargo {}", args.join(" ")))
            .collect();
        assert_eq!(
            commands,
            [
                "fmt: cargo fmt --all -- --check",
                "clippy: cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings",
                "test: cargo test --locked --offline --workspace --all-features",
            ]
        );
    }

    #[test]
    fn the_summary_is_a_table() {
        let second = Duration::from_millis(1300);
        let lines = summary(
            &["fmt", "clippy", "deps"],
            &[Some((true, second)), Some((false, second)), None],
        );
        assert_eq!(
            lines,
            [
                "ci step              result    seconds",
                "fmt                  ok            1.3",
                "clippy               FAILED        1.3",
                "deps                 not run         -",
            ]
        );
    }
}
