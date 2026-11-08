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

//! `cargo xtask`: the project's own checks and harness runners.
//!
//! Workspace checks, fuzz smoke, crash campaigns, benchmarks and `diff-pg`.
//! Design: `docs/04-quality.md`.
//!
//! It never uploads, publishes, signs, reads secrets or calls the network:
//! the Cargo it runs is `--offline`.

/// Writes a line to standard output, as `println!` would, but a closed
/// pipe (`cargo xtask unsafe-audit | head`) is not a panic: the check's
/// result alone decides the exit status.
macro_rules! say {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stdout(), $($arg)*);
    }};
}

/// `say!` for standard error.
macro_rules! complain {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

mod ci;
mod deps;
mod files;
#[cfg(test)]
mod fixture;
mod fuzz_smoke;
mod fuzz_targets;
mod headers;
mod json;
mod layers;
mod lexer;
mod metadata;
mod mutate;
mod release_no_testing;
mod unsafe_audit;

use metadata::{Cargo, Metadata};
use std::env;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
usage: cargo xtask <command>

commands:
  ci                   fmt, clippy and the tests, then each check below,
                       stopping at the first failure
  deps                 no package from outside the workspace that
                       xtask/deps-allowlist.txt does not approve
  layers               every workspace dependency within its row of the
                       dependency matrix (docs/01-architecture.md)
  unsafe-audit         `unsafe` only where ADR-0013 and ADR-0017 allow it,
                       each with `// SAFETY:`; `std::fs` only outside the engine
  headers              the Apache-2.0 header on every source file (ADR-0015)
  release-no-testing   the `testing` feature off in the CLI's and the
                       server's release builds
  fuzz-smoke [--secs N] [--target T]
                       the mutation fuzzer: every target, or T, for N
                       seconds each (default 60), its seeds always in
                       full; findings go to fuzz/crashes/ and fail it
  help                 this message

exit status: 0 pass, 1 fail, 2 command line not understood
";

/// A check's result: the problems it found, none if it passed, and a
/// summary printed either way.
pub(crate) struct Report {
    pub(crate) problems: Vec<String>,
    pub(crate) summary: Vec<String>,
}

/// A project check. `Err` means it could not run, which fails it too.
pub(crate) type Check = fn(&Workspace) -> Result<Report, String>;

/// The project checks, in the order `ci` runs them.
pub(crate) const CHECKS: [(&str, Check); 5] = [
    ("deps", deps::run),
    ("layers", layers::run),
    ("unsafe-audit", unsafe_audit::run),
    ("headers", headers::run),
    ("release-no-testing", release_no_testing::run),
];

/// What the checks read: Cargo, and the workspace as `cargo metadata`
/// describes it.
pub(crate) struct Workspace {
    pub(crate) cargo: Cargo,
    pub(crate) metadata: Metadata,
}

impl Workspace {
    pub(crate) fn load(root: &Path) -> Result<Workspace, String> {
        let cargo = Cargo::new(root.join("Cargo.toml"));
        let metadata = Metadata::load(&cargo)?;
        Ok(Workspace { cargo, metadata })
    }
}

/// Prints a check's result and says whether it passed.
pub(crate) fn report(name: &str, result: Result<Report, String>) -> bool {
    report_to(&mut io::stdout(), &mut io::stderr(), name, result)
}

/// `report`, to `out` and `err`. Write errors are ignored, as in `say!`.
fn report_to(
    out: &mut dyn Write,
    err: &mut dyn Write,
    name: &str,
    result: Result<Report, String>,
) -> bool {
    match result {
        Ok(report) => {
            for line in report.summary.iter().chain(&report.problems) {
                let _ = writeln!(out, "{name}: {line}");
            }
            let _ = match report.problems.len() {
                0 => writeln!(out, "{name}: ok"),
                1 => writeln!(out, "{name}: FAILED, 1 problem"),
                n => writeln!(out, "{name}: FAILED, {n} problems"),
            };
            report.problems.is_empty()
        }
        Err(message) => {
            let _ = writeln!(err, "{name}: could not run: {message}");
            false
        }
    }
}

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    Help,
    Ci,
    /// The check at this index of `CHECKS`.
    Check(usize),
    FuzzSmoke(fuzz_smoke::Options),
}

fn parse(args: &[OsString]) -> Result<Command, String> {
    let words: Vec<&str> = args
        .iter()
        .map(|arg| {
            arg.to_str()
                .ok_or_else(|| format!("the argument {arg:?} is not valid Unicode"))
        })
        .collect::<Result<_, _>>()?;
    let Some((&name, rest)) = words.split_first() else {
        return Err("no command given".to_owned());
    };
    let command = match name {
        "help" | "-h" | "--help" => Command::Help,
        "ci" => Command::Ci,
        "fuzz-smoke" => return fuzz_smoke::parse(rest).map(Command::FuzzSmoke),
        _ => match CHECKS.iter().position(|(check, _)| *check == name) {
            Some(index) => Command::Check(index),
            None => return Err(format!("unknown command `{name}`")),
        },
    };
    match rest.first() {
        Some(extra) => Err(format!(
            "`{name}` takes no arguments, but was given `{extra}`"
        )),
        None => Ok(command),
    }
}

/// The workspace root: the directory above xtask's.
fn root() -> PathBuf {
    let xtask = Path::new(env!("CARGO_MANIFEST_DIR"));
    xtask.parent().unwrap_or(xtask).to_path_buf()
}

fn main() -> ExitCode {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let passed = match parse(&args) {
        Ok(Command::Help) => {
            say!("{}", USAGE.trim_end());
            true
        }
        Ok(Command::Ci) => ci::run(&root()),
        Ok(Command::FuzzSmoke(options)) => fuzz_smoke::run(&root(), &options),
        Ok(Command::Check(index)) => {
            let Some(&(name, check)) = CHECKS.get(index) else {
                return ExitCode::from(2);
            };
            report(
                name,
                Workspace::load(&root()).and_then(|workspace| check(&workspace)),
            )
        }
        Err(message) => {
            complain!("xtask: {message}\n\n{}", USAGE.trim_end());
            return ExitCode::from(2);
        }
    };
    if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::{CHECKS, Command, Report, parse, report_to};
    use crate::fuzz_smoke::Options;
    use std::ffi::OsString;

    fn args(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    #[test]
    fn commands_parse() {
        assert_eq!(parse(&args(&["help"])), Ok(Command::Help));
        assert_eq!(parse(&args(&["--help"])), Ok(Command::Help));
        assert_eq!(parse(&args(&["ci"])), Ok(Command::Ci));
        for (index, (name, _)) in CHECKS.iter().enumerate() {
            assert_eq!(parse(&args(&[name])), Ok(Command::Check(index)));
        }
        assert_eq!(
            parse(&args(&["fuzz-smoke"])),
            Ok(Command::FuzzSmoke(Options {
                secs: 60,
                target: None
            }))
        );
        assert_eq!(
            parse(&args(&["fuzz-smoke", "--secs", "5", "--target", "varint"])),
            Ok(Command::FuzzSmoke(Options {
                secs: 5,
                target: Some(0)
            }))
        );
    }

    #[test]
    fn a_command_line_not_understood_is_an_error() {
        for words in [
            &[][..],
            &["nope"],
            &["ci", "extra"],
            &["deps", "--verbose"],
            &["CI"],
            &["fuzz-smoke", "--secs"],
            &["fuzz-smoke", "--target", "nope"],
            &["fuzz-smoke", "extra"],
        ] {
            assert!(parse(&args(words)).is_err(), "{words:?}");
        }
    }

    #[test]
    fn a_check_passes_only_with_no_problems() {
        // What `report_to` printed, to standard output and to standard
        // error, and whether it said the check passed.
        let reported = |result: Result<Report, String>| {
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let passed = report_to(&mut out, &mut err, "t", result);
            (
                String::from_utf8(out).unwrap(),
                String::from_utf8(err).unwrap(),
                passed,
            )
        };
        let with = |problems: &[&str]| {
            Ok(Report {
                problems: problems.iter().map(|p| (*p).to_owned()).collect(),
                summary: vec!["a summary".to_owned()],
            })
        };
        assert_eq!(
            reported(with(&[])),
            ("t: a summary\nt: ok\n".to_owned(), String::new(), true)
        );
        assert_eq!(
            reported(with(&["one"])),
            (
                "t: a summary\nt: one\nt: FAILED, 1 problem\n".to_owned(),
                String::new(),
                false
            )
        );
        assert_eq!(
            reported(with(&["one", "two"])).0,
            "t: a summary\nt: one\nt: two\nt: FAILED, 2 problems\n"
        );
        assert_eq!(
            reported(Err("cannot run".to_owned())),
            (
                String::new(),
                "t: could not run: cannot run\n".to_owned(),
                false
            ),
            "a check that cannot run fails"
        );
    }

    /// A writer that always fails, as a closed pipe does.
    struct Closed;

    impl std::io::Write for Closed {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
    }

    #[test]
    fn a_failing_output_changes_no_result() {
        let report = |problems: Vec<String>| {
            Ok(Report {
                problems,
                summary: vec!["a summary".to_owned()],
            })
        };
        assert!(report_to(&mut Closed, &mut Closed, "t", report(vec![])));
        assert!(!report_to(
            &mut Closed,
            &mut Closed,
            "t",
            report(vec!["one".to_owned()])
        ));
        assert!(!report_to(
            &mut Closed,
            &mut Closed,
            "t",
            Err("cannot run".to_owned())
        ));
    }

    #[test]
    fn the_checks_are_in_ci_order() {
        let names: Vec<&str> = CHECKS.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            [
                "deps",
                "layers",
                "unsafe-audit",
                "headers",
                "release-no-testing"
            ]
        );
    }
}
