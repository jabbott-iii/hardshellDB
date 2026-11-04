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

//! `cargo xtask deps`: no package from outside the workspace unless the
//! dependency policy approved it (`docs/02-dependency-policy.md`,
//! ADR-0002).

use crate::metadata::Metadata;
use crate::{Report, Workspace};
use std::fs;

/// The approved third-party packages, relative to the workspace root.
const ALLOWLIST: &str = "xtask/deps-allowlist.txt";

/// A package the user approved: a line `name version ADR-NNNN`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Approved {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) adr: String,
}

pub(crate) fn run(workspace: &Workspace) -> Result<Report, String> {
    let path = workspace.metadata.root.join(ALLOWLIST);
    let text = fs::read_to_string(&path).map_err(|e| format!("cannot read {ALLOWLIST}: {e}"))?;
    let approved = parse_allowlist(&text)?;
    let problems = check(&workspace.metadata, &approved);
    let packages = workspace.metadata.packages.len();
    let members = workspace
        .metadata
        .packages
        .iter()
        .filter(|p| p.member)
        .count();
    Ok(Report {
        problems,
        summary: vec![format!(
            "{packages} packages: {members} workspace members, {} from outside the workspace",
            packages - members
        )],
    })
}

/// Reads the allowlist: one `name version ADR-NNNN` per line. Blank lines
/// and lines starting with `#` are skipped.
pub(crate) fn parse_allowlist(text: &str) -> Result<Vec<Approved>, String> {
    let mut approved = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        match fields.as_slice() {
            [name, version, adr] if is_adr(adr) => approved.push(Approved {
                name: (*name).to_owned(),
                version: (*version).to_owned(),
                adr: (*adr).to_owned(),
            }),
            _ => {
                return Err(format!(
                    "{ALLOWLIST}:{}: expected `name version ADR-NNNN`, found `{line}`",
                    index + 1
                ));
            }
        }
    }
    Ok(approved)
}

/// `ADR-` and four digits.
fn is_adr(word: &str) -> bool {
    word.strip_prefix("ADR-")
        .is_some_and(|digits| digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit()))
}

/// One problem for each package that is neither a workspace member nor an
/// approved package from a registry or a repository.
pub(crate) fn check(metadata: &Metadata, approved: &[Approved]) -> Vec<String> {
    let mut problems = Vec::new();
    for package in metadata.packages.iter().filter(|p| !p.member) {
        let (name, version) = (&package.name, &package.version);
        match &package.source {
            None => problems.push(format!(
                "{name} {version} is a path package outside the workspace ({}): vendored code needs the \
                 dependency policy's approval",
                package.manifest.display()
            )),
            Some(source) if !approved.iter().any(|a| a.name == *name && a.version == *version) => {
                problems.push(format!(
                    "{name} {version} (from {source}) is not approved: the approved set is \
                     {ALLOWLIST}, and only the user approves (docs/02-dependency-policy.md)"
                ));
            }
            Some(_) => {}
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::{Approved, check, parse_allowlist};
    use crate::fixture::Fixture;
    use crate::metadata::tests::package;
    use crate::metadata::{Metadata, Package};
    use std::path::PathBuf;

    fn third_party(name: &str, version: &str) -> Package {
        Package {
            version: version.to_owned(),
            source: Some("registry+https://github.com/rust-lang/crates.io-index".to_owned()),
            member: false,
            ..package(name, "../registry", &[])
        }
    }

    fn metadata(packages: Vec<Package>) -> Metadata {
        Metadata {
            root: PathBuf::from("/ws"),
            target_dir: PathBuf::from("/ws/target"),
            packages,
        }
    }

    fn approve(name: &str, version: &str) -> Approved {
        Approved {
            name: name.to_owned(),
            version: version.to_owned(),
            adr: "ADR-0099".to_owned(),
        }
    }

    #[test]
    fn workspace_members_alone_pass() {
        let metadata = metadata(vec![package("hardshell-core", "crates/core", &[])]);
        assert_eq!(check(&metadata, &[]), Vec::<String>::new());
    }

    #[test]
    fn a_third_party_package_fails_unless_approved_at_its_exact_version() {
        let metadata = metadata(vec![
            package("hardshell-core", "crates/core", &[]),
            third_party("rand", "0.9.0"),
        ]);
        let problems = check(&metadata, &[]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].starts_with("rand 0.9.0 (from registry+"),
            "{problems:?}"
        );
        assert_eq!(
            check(&metadata, &[approve("rand", "0.9.0")]),
            Vec::<String>::new()
        );
        assert_eq!(check(&metadata, &[approve("rand", "0.9.1")]).len(), 1);
        assert_eq!(check(&metadata, &[approve("rand-core", "0.9.0")]).len(), 1);
    }

    #[test]
    fn a_path_package_outside_the_workspace_fails_even_if_listed() {
        let vendored = Package {
            member: false,
            ..package("vendored", "../vendored", &[])
        };
        let metadata = metadata(vec![vendored]);
        let problems = check(&metadata, &[approve("vendored", "0.1.0")]);
        assert_eq!(problems.len(), 1);
        assert!(
            problems[0].contains("path package outside the workspace"),
            "{problems:?}"
        );
    }

    #[test]
    fn the_allowlist_is_parsed_strictly() {
        let text = "# comment\n\n  rand 0.9.0 ADR-0018  \n# another\nlibc\t0.2.170 ADR-0019\n";
        assert_eq!(
            parse_allowlist(text),
            Ok(vec![
                Approved {
                    name: "rand".to_owned(),
                    version: "0.9.0".to_owned(),
                    adr: "ADR-0018".to_owned()
                },
                Approved {
                    name: "libc".to_owned(),
                    version: "0.2.170".to_owned(),
                    adr: "ADR-0019".to_owned()
                },
            ])
        );
        for bad in [
            "rand",
            "rand 0.9.0",
            "rand 0.9.0 ADR-18",
            "rand 0.9.0 adr-0018",
            "rand 0.9.0 ADR-0018 x",
        ] {
            let error = parse_allowlist(&format!("# header\n{bad}\n")).unwrap_err();
            assert!(error.contains(":2:"), "{bad}: {error}");
        }
        assert_eq!(parse_allowlist(""), Ok(vec![]));
    }

    #[test]
    fn the_committed_allowlist_approves_nothing() {
        let text = include_str!("../deps-allowlist.txt");
        assert_eq!(parse_allowlist(text), Ok(vec![]));
    }

    /// Real `cargo metadata` on a throwaway workspace: a member's path
    /// dependency outside the workspace root fails, though it is optional
    /// and off by default.
    #[test]
    fn cargo_metadata_shows_a_path_package_outside_the_workspace() {
        let fixture = Fixture::new("deps");
        fixture.write(
            "ws/Cargo.toml",
            "[workspace]\nresolver = \"3\"\nmembers = [\"a\"]\n",
        );
        fixture.write(
            "ws/a/Cargo.toml",
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
             [dependencies]\noutside = { path = \"../../outside\", optional = true }\n",
        );
        fixture.write("ws/a/src/lib.rs", "");
        fixture.write(
            "outside/Cargo.toml",
            "[package]\nname = \"outside\"\nversion = \"1.2.3\"\nedition = \"2024\"\n",
        );
        fixture.write("outside/src/lib.rs", "");
        let metadata = Metadata::load(&fixture.cargo("ws")).unwrap();
        let problems = check(&metadata, &[]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].starts_with("outside 1.2.3 is a path package outside the workspace"),
            "{problems:?}"
        );
    }
}
