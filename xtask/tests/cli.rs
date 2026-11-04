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

//! The `xtask` binary's exit statuses: 0 pass, 1 fail, 2 command line not
//! understood, whatever happens to its output.

use std::process::{Command, Stdio};

fn xtask(args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
    command.args(args);
    command
}

#[test]
fn help_exits_0_and_prints_the_usage() {
    let output = xtask(&["help"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.starts_with("usage: cargo xtask <command>"),
        "{stdout}"
    );
    assert!(stdout.contains("exit status: 0 pass, 1 fail, 2 command line not understood"));
}

#[test]
fn a_command_line_not_understood_exits_2() {
    for args in [&[][..], &["nope"], &["ci", "extra"], &["deps", "--verbose"]] {
        let output = xtask(args).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.starts_with("xtask: "), "{args:?}: {stderr}");
        assert!(stderr.contains("usage: cargo xtask <command>"), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
    }
}

/// `cargo xtask help | head -1`: the pipe closes before xtask writes, and
/// writing must not panic (exit 101).
#[test]
fn a_closed_pipe_does_not_change_the_exit_status() {
    for _ in 0..20 {
        let mut child = xtask(&["help"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        drop(child.stdout.take());
        assert_eq!(child.wait().unwrap().code(), Some(0));
    }
}
