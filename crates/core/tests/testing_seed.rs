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

//! `testing::check` and `testing::check_sized` end to end, through the real
//! `HARDSHELL_SEED`: a child run of this test binary fails a property on
//! purpose, and the parent checks that the seed it reports replays the
//! same failure, and that the panic points at the caller.

use hardshell_core::testing::{DEFAULT_SEED, check, check_sized};
use std::env;
use std::process::Command;

/// Set only in the child runs.
const CHILD: &str = "HARDSHELL_TESTING_CHILD";

/// Each child test and the name of the property it fails.
const CHILDREN: [(&str, &str); 2] = [
    ("child_fails_a_property", "one draw in eight fails"),
    (
        "child_fails_a_sized_property",
        "sized, one draw in eight fails",
    ),
];

/// The property both children fail.
fn one_in_eight(draw: u32) -> Result<(), String> {
    if draw.is_multiple_of(8) {
        Err(format!("{draw} is a multiple of 8"))
    } else {
        Ok(())
    }
}

/// Fails `check` in a child run; outside one it passes at once.
#[test]
fn child_fails_a_property() {
    if env::var_os(CHILD).is_some() {
        check(
            CHILDREN[0].1,
            1_000,
            |rng| rng.next_u32(),
            |&draw| one_in_eight(draw),
        );
    }
}

/// Fails `check_sized` in a child run; outside one it passes at once.
#[test]
fn child_fails_a_sized_property() {
    if env::var_os(CHILD).is_some() {
        check_sized(
            CHILDREN[1].1,
            1_000,
            |rng, size| (size, rng.next_u32()),
            |&(_, draw)| one_in_eight(draw),
        );
    }
}

/// Runs the child test `test` with `HARDSHELL_SEED` set to `seed` (unset
/// for `None`), and returns its stderr.
fn run_child(test: &str, seed: Option<&str>) -> String {
    let exe = env::current_exe().expect("the test binary has a path");
    let mut command = Command::new(exe);
    command
        .args([test, "--exact", "--nocapture"])
        .env(CHILD, "1");
    match seed {
        Some(seed) => command.env("HARDSHELL_SEED", seed),
        None => command.env_remove("HARDSHELL_SEED"),
    };
    let output = command.output().expect("the child runs");
    assert!(!output.status.success(), "{test} fails");
    String::from_utf8(output.stderr).expect("the child's stderr is UTF-8")
}

/// The three lines of the report on property `name`: the header, the input
/// and the error.
fn report<'a>(stderr: &'a str, name: &str) -> [&'a str; 3] {
    let start = stderr
        .find(&format!("property \"{name}\" failed at case "))
        .unwrap_or_else(|| panic!("no report in the child's stderr:\n{stderr}"));
    let mut lines = stderr[start..].lines();
    [(); 3].map(|()| lines.next().expect("the report has three lines"))
}

/// The panic points at the `check` or `check_sized` call in this file.
fn points_here(stderr: &str) -> bool {
    stderr.contains(&format!("panicked at {}:", file!()))
}

#[test]
fn a_reported_seed_replays_the_failure() {
    for (test, name) in CHILDREN {
        let first = run_child(test, Some("12345"));
        assert!(points_here(&first), "{first}");
        let [header, input, error] = report(&first, name);
        assert!(
            header.ends_with("; HARDSHELL_SEED=12345 replays it"),
            "{header}"
        );
        assert!(input.starts_with("input: "), "{input}");
        // `input: 2089109368`, or `input: (0, 2089109368)` with the size.
        let draw = input.trim_end_matches(')').rsplit(' ').next();
        let draw = draw.expect("the input line ends with the draw");
        assert_eq!(error, format!("error: {draw} is a multiple of 8"));

        // The seed as reported replays it, in either notation.
        let reported = header.rsplit_once("HARDSHELL_SEED=").map(|(_, rest)| rest);
        let reported = reported.and_then(|rest| rest.split(' ').next());
        let reported = reported.expect("the header names the seed");
        let replay = run_child(test, Some(reported));
        assert_eq!(report(&replay, name), report(&first, name));
        let hex = run_child(test, Some("0x3039"));
        assert_eq!(report(&hex, name), report(&first, name));

        // Another seed fails elsewhere.
        let other = run_child(test, Some("54321"));
        assert_ne!(report(&other, name)[1..], report(&first, name)[1..]);
    }
}

#[test]
fn an_unset_or_empty_seed_is_the_default() {
    for (test, name) in CHILDREN {
        let unset = run_child(test, None);
        let [header, ..] = report(&unset, name);
        let replay = format!("; HARDSHELL_SEED={DEFAULT_SEED} replays it");
        assert!(header.ends_with(&replay), "{header}");
        let empty = run_child(test, Some(""));
        assert_eq!(report(&empty, name), report(&unset, name));
        let default = run_child(test, Some(&DEFAULT_SEED.to_string()));
        assert_eq!(report(&default, name), report(&unset, name));
    }
}

#[test]
fn a_malformed_seed_fails_with_a_message() {
    let message = "HARDSHELL_SEED is \"12a\": expected a u64 in decimal, or 0x and hex digits";
    for (test, _) in CHILDREN {
        let stderr = run_child(test, Some("12a"));
        assert!(stderr.contains(message), "{stderr}");
        assert!(points_here(&stderr), "{stderr}");
    }
}
