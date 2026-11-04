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

//! `cargo xtask release-no-testing`: the `testing` feature (fault
//! injection, crash points, test hooks) is off in every package of the
//! CLI's and the server's release builds (`docs/01-architecture.md`,
//! "Feature flags").
//!
//! It asks `cargo tree` for each binary alone, with its default features
//! and without dev-dependencies, on every target: the features a `cargo
//! build --release -p <binary>` turns on. A dev-dependency may turn
//! `testing` on for tests; a normal or build dependency may not.

use crate::metadata::{Cargo, output};
use crate::{Report, Workspace};
use std::collections::BTreeSet;

/// The binaries whose release builds must not hold the feature.
const BINARIES: [&str; 2] = ["hardshell-cli", "hardshell-server"];

const FEATURE: &str = "testing";

pub(crate) fn run(workspace: &Workspace) -> Result<Report, String> {
    check(&workspace.cargo, &BINARIES)
}

pub(crate) fn check(cargo: &Cargo, binaries: &[&str]) -> Result<Report, String> {
    let mut problems = BTreeSet::new();
    let mut packages = 0;
    for binary in binaries {
        let mut command = cargo.command("tree");
        command.args([
            "--package",
            binary,
            "--edges",
            "no-dev",
            "--target",
            "all",
            "--prefix",
            "none",
            "--format",
            "[{f}] {p}",
        ]);
        let text = output(command)?;
        let tree = parse_tree(&text)?;
        packages += tree
            .iter()
            .map(|(_, package)| package)
            .collect::<BTreeSet<_>>()
            .len();
        for (features, package) in tree {
            if features.contains(FEATURE) {
                problems.insert(format!(
                    "{binary}'s release build turns on `{FEATURE}` in {package}: only a dev-dependency may"
                ));
            }
        }
    }
    let summary = format!(
        "release builds of {}: {packages} packages",
        binaries.join(", ")
    );
    Ok(Report {
        problems: problems.into_iter().collect(),
        summary: vec![summary],
    })
}

/// Each line of `cargo tree --format "[{f}] {p}"` output: a package and the
/// features it has there. A package can appear more than once with
/// different features: as a build dependency it is built for the host,
/// apart from its build for the target.
fn parse_tree(text: &str) -> Result<Vec<(BTreeSet<&str>, &str)>, String> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (features, package) = line
                .strip_prefix('[')
                .and_then(|rest| rest.split_once("] "))
                .ok_or_else(|| format!("unexpected `cargo tree` line `{line}`"))?;
            let package = package.strip_suffix(" (*)").unwrap_or(package);
            Ok((
                features.split(',').filter(|f| !f.is_empty()).collect(),
                package,
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{check, parse_tree};
    use crate::fixture::Fixture;
    use std::collections::BTreeSet;

    #[test]
    fn tree_output_is_parsed() {
        let text = "[] cli v0.1.0 (/ws/apps/cli)\n[default,std] hardshell-core v0.1.0 (/ws/crates/core)\n\
                    [] hardshell-os v0.1.0 (/ws/crates/os)\n[testing] hardshell-os v0.1.0 (/ws/crates/os) (*)\n";
        let tree = parse_tree(text).unwrap();
        let features = |names: &[&'static str]| names.iter().copied().collect::<BTreeSet<&str>>();
        assert_eq!(
            tree,
            [
                (features(&[]), "cli v0.1.0 (/ws/apps/cli)"),
                (
                    features(&["default", "std"]),
                    "hardshell-core v0.1.0 (/ws/crates/core)"
                ),
                (features(&[]), "hardshell-os v0.1.0 (/ws/crates/os)"),
                (
                    features(&["testing"]),
                    "hardshell-os v0.1.0 (/ws/crates/os)"
                ),
            ],
            "every line, a package seen twice included"
        );
        assert!(
            parse_tree("cli v0.1.0 (/ws/apps/cli)\n").is_err(),
            "an unexpected format"
        );
    }

    /// Real `cargo tree` on a throwaway workspace: a normal dependency that
    /// turns `testing` on fails, on any target and even beside a build
    /// dependency without it, and so does a default feature of the binary;
    /// a dev-dependency or a feature that is off is fine, and so is a member
    /// outside the binary's tree.
    #[test]
    fn a_normal_dependency_turning_testing_on_fails() {
        let fixture = Fixture::new("release-no-testing");
        fixture.write(
            "Cargo.toml",
            "[workspace]\nresolver = \"3\"\nmembers = [\"os\", \"cli\", \"server\", \"mid\", \"tool\", \"other\", \"dflt\", \"opt\"]\n",
        );
        let manifest = |name: &str, extra: &str| {
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n{extra}"
            )
        };
        fixture.write(
            "os/Cargo.toml",
            &manifest("os", "[features]\ntesting = []\n"),
        );
        fixture.write("os/src/lib.rs", "");
        fixture.write(
            "cli/Cargo.toml",
            &manifest(
                "cli",
                "[dependencies]\nos = { path = \"../os\" }\n\n\
                 [dev-dependencies]\nos = { path = \"../os\", features = [\"testing\"] }\n",
            ),
        );
        fixture.write("cli/src/main.rs", "fn main() {}\n");
        fixture.write(
            "server/Cargo.toml",
            &manifest(
                "server",
                "[dependencies]\nos = { path = \"../os\", features = [\"testing\"] }\n",
            ),
        );
        fixture.write("server/src/main.rs", "fn main() {}\n");
        // `os` comes first as `mid`'s build dependency, built for the host
        // without `testing`; then for the target, with it.
        fixture.write(
            "mid/Cargo.toml",
            &manifest("mid", "[build-dependencies]\nos = { path = \"../os\" }\n"),
        );
        fixture.write("mid/src/lib.rs", "");
        fixture.write(
            "tool/Cargo.toml",
            &manifest(
                "tool",
                "[dependencies]\nmid = { path = \"../mid\" }\n\n\
                 [target.'cfg(windows)'.dependencies]\nos = { path = \"../os\", features = [\"testing\"] }\n",
            ),
        );
        fixture.write("tool/src/main.rs", "fn main() {}\n");
        fixture.write(
            "other/Cargo.toml",
            &manifest(
                "other",
                "[dependencies]\nos = { path = \"../os\", features = [\"testing\"] }\n",
            ),
        );
        fixture.write("other/src/lib.rs", "");
        // Through the binary's own features: on by default, or not.
        for (name, feature) in [("dflt", "default"), ("opt", "extra")] {
            fixture.write(
                &format!("{name}/Cargo.toml"),
                &manifest(
                    name,
                    &format!(
                        "[dependencies]\nos = {{ path = \"../os\" }}\n\n\
                         [features]\n{feature} = [\"os/testing\"]\n"
                    ),
                ),
            );
            fixture.write(&format!("{name}/src/main.rs"), "fn main() {}\n");
        }
        let cargo = fixture.cargo(".");
        let cli = check(&cargo, &["cli"]).unwrap();
        assert_eq!(cli.problems, Vec::<String>::new());
        assert_eq!(cli.summary, ["release builds of cli: 2 packages"]);
        let server = check(&cargo, &["cli", "server"]).unwrap();
        assert_eq!(server.problems.len(), 1, "{:?}", server.problems);
        assert!(
            server.problems[0]
                .starts_with("server's release build turns on `testing` in os v0.1.0")
        );
        let tool = check(&cargo, &["tool"]).unwrap();
        assert_eq!(tool.problems.len(), 1, "{:?}", tool.problems);
        assert!(
            tool.problems[0].starts_with("tool's release build turns on `testing` in os v0.1.0")
        );
        let dflt = check(&cargo, &["dflt"]).unwrap();
        assert_eq!(
            dflt.problems.len(),
            1,
            "a default feature: {:?}",
            dflt.problems
        );
        let opt = check(&cargo, &["opt"]).unwrap();
        assert_eq!(opt.problems, Vec::<String>::new(), "a feature that is off");
        assert!(check(&cargo, &["missing"]).is_err());
    }
}
