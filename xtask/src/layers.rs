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

//! `cargo xtask layers`: every member depends only on the workspace crates
//! its row of the dependency matrix allows (`docs/01-architecture.md`).
//! Third-party packages are `deps`'s concern.

use crate::metadata::Metadata;
use crate::{Report, Workspace};

/// The workspace crates a crate may depend on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Allowed {
    Only(&'static [&'static str]),
    /// Any workspace crate.
    Any,
}

/// The dependency matrix of `docs/01-architecture.md`, by package name.
/// A test compares the two. Changing a row is the user's decision, never a
/// way to make code compile (`AGENTS.md`, rule 2).
pub(crate) const MATRIX: &[(&str, Allowed)] = &[
    ("hardshell-core", Allowed::Only(&[])),
    ("hardshell-os", Allowed::Only(&["hardshell-core"])),
    ("hardshell-types", Allowed::Only(&["hardshell-core"])),
    ("hardshell-wire", Allowed::Only(&["hardshell-core"])),
    (
        "hardshell-storage",
        Allowed::Only(&["hardshell-core", "hardshell-os"]),
    ),
    ("hardshell-sql", Allowed::Only(&["hardshell-core"])),
    (
        "hardshell-catalog",
        Allowed::Only(&["hardshell-core", "hardshell-types", "hardshell-storage"]),
    ),
    (
        "hardshell-plan",
        Allowed::Only(&[
            "hardshell-core",
            "hardshell-types",
            "hardshell-sql",
            "hardshell-catalog",
        ]),
    ),
    (
        "hardshell-exec",
        Allowed::Only(&[
            "hardshell-core",
            "hardshell-os",
            "hardshell-types",
            "hardshell-storage",
            "hardshell-catalog",
            "hardshell-plan",
        ]),
    ),
    (
        "hardshelldb",
        Allowed::Only(&[
            "hardshell-core",
            "hardshell-os",
            "hardshell-types",
            "hardshell-storage",
            "hardshell-sql",
            "hardshell-catalog",
            "hardshell-plan",
            "hardshell-exec",
        ]),
    ),
    (
        "hardshell-cli",
        Allowed::Only(&["hardshell-core", "hardshelldb"]),
    ),
    (
        "hardshell-server",
        Allowed::Only(&["hardshell-core", "hardshell-wire", "hardshelldb"]),
    ),
    (
        "xtask",
        Allowed::Only(&[
            "hardshell-core",
            "hardshell-os",
            "hardshell-types",
            "hardshell-storage",
            "hardshell-sql",
            "hardshell-wire",
        ]),
    ),
    ("hardshell-bench", Allowed::Any),
];

pub(crate) fn run(workspace: &Workspace) -> Result<Report, String> {
    let problems = check(&workspace.metadata, MATRIX);
    let members = workspace.metadata.members().len();
    Ok(Report {
        problems,
        summary: vec![format!(
            "{members} members checked against the dependency matrix"
        )],
    })
}

/// One problem for each member without a row in `matrix`, and for each
/// normal, dev or build dependency on another member that its row does not
/// allow. A dependency is on a member if it names one, wherever its
/// manifest says it comes from: a `[patch]` can turn a registry dependency
/// into the member. A crate depending on itself (a dev-dependency that
/// turns on a feature for its own tests) is not an edge.
pub(crate) fn check(metadata: &Metadata, matrix: &[(&str, Allowed)]) -> Vec<String> {
    let members = metadata.members();
    let mut problems = Vec::new();
    for member in &members {
        let Some(&(_, allowed)) = matrix.iter().find(|(name, _)| *name == member.name) else {
            problems.push(format!(
                "{} has no row in the dependency matrix (docs/01-architecture.md); adding one is the \
                 user's decision",
                member.name
            ));
            continue;
        };
        for dependency in &member.dependencies {
            let on_member = members.iter().any(|m| m.name == dependency.name);
            if !on_member || dependency.name == member.name {
                continue;
            }
            let permitted = match allowed {
                Allowed::Any => true,
                Allowed::Only(crates) => crates.contains(&dependency.name.as_str()),
            };
            if !permitted {
                problems.push(format!(
                    "{} has a {} dependency on {}, which its row of the dependency matrix does not allow \
                     (docs/01-architecture.md)",
                    member.name,
                    dependency.kind.as_str(),
                    dependency.name
                ));
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::{Allowed, MATRIX, check};
    use crate::fixture::Fixture;
    use crate::metadata::tests::package;
    use crate::metadata::{DepKind, Metadata, Package};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn metadata(packages: Vec<Package>) -> Metadata {
        Metadata {
            root: PathBuf::from("/ws"),
            target_dir: PathBuf::from("/ws/target"),
            packages,
        }
    }

    /// A row as the doc and the table both can say it: `None` for any
    /// workspace crate.
    type Row = Option<Vec<String>>;

    fn table_rows() -> BTreeMap<String, Row> {
        MATRIX
            .iter()
            .map(|&(name, allowed)| {
                let row = match allowed {
                    Allowed::Any => None,
                    Allowed::Only(crates) => {
                        let mut crates: Vec<String> =
                            crates.iter().map(|c| (*c).to_owned()).collect();
                        crates.sort();
                        Some(crates)
                    }
                };
                (name.to_owned(), row)
            })
            .collect()
    }

    /// The rows of the doc's "Dependency matrix" table. The doc names
    /// allowed crates by their short names (`core` for `hardshell-core`).
    fn doc_rows(doc: &str) -> BTreeMap<String, Row> {
        let section = doc
            .split("\n## ")
            .find(|s| s.starts_with("Dependency matrix\n"))
            .expect("the matrix section");
        let quoted = |cell: &str| -> Vec<String> {
            cell.split('`')
                .skip(1)
                .step_by(2)
                .map(str::to_owned)
                .collect()
        };
        section
            .lines()
            .filter(|line| line.starts_with("| `"))
            .map(|line| {
                let cells: Vec<&str> = line.split('|').map(str::trim).collect();
                let [_, crate_cell, allowed_cell, _] = cells.as_slice() else {
                    panic!("row {line:?}")
                };
                let name = quoted(crate_cell).into_iter().next().expect("a crate name");
                let row = match *allowed_cell {
                    "—" => Some(vec![]),
                    "any workspace crate" => None,
                    cell => {
                        let mut crates: Vec<String> = quoted(cell)
                            .into_iter()
                            .map(|c| {
                                if c.starts_with("hardshell") {
                                    c
                                } else {
                                    format!("hardshell-{c}")
                                }
                            })
                            .collect();
                        assert!(!crates.is_empty(), "row {line:?}");
                        crates.sort();
                        Some(crates)
                    }
                };
                (name, row)
            })
            .collect()
    }

    #[test]
    fn the_table_matches_the_architecture_doc() {
        let doc = doc_rows(&include_str!("../../docs/01-architecture.md").replace("\r\n", "\n"));
        assert_eq!(doc.len(), 14, "{doc:?}");
        assert_eq!(table_rows(), doc);
    }

    #[test]
    fn every_allowed_crate_has_a_row() {
        let names: Vec<&str> = MATRIX.iter().map(|(name, _)| *name).collect();
        for (name, allowed) in MATRIX {
            if let Allowed::Only(crates) = allowed {
                for c in *crates {
                    assert!(names.contains(c), "{name} allows {c}, which has no row");
                }
            }
        }
    }

    #[test]
    fn allowed_edges_pass() {
        let metadata = metadata(vec![
            package("hardshell-core", "crates/core", &[]),
            package(
                "hardshell-os",
                "crates/os",
                &[
                    ("hardshell-core", DepKind::Normal),
                    ("hardshell-os", DepKind::Dev),
                ],
            ),
            package(
                "hardshell-bench",
                "bench",
                &[
                    ("hardshell-os", DepKind::Normal),
                    ("hardshell-core", DepKind::Dev),
                ],
            ),
        ]);
        assert_eq!(check(&metadata, MATRIX), Vec::<String>::new());
    }

    #[test]
    fn a_forbidden_edge_fails_whatever_its_kind() {
        for kind in [DepKind::Normal, DepKind::Dev, DepKind::Build] {
            let metadata = metadata(vec![
                package("hardshell-core", "crates/core", &[("hardshell-os", kind)]),
                package("hardshell-os", "crates/os", &[]),
            ]);
            let problems = check(&metadata, MATRIX);
            assert_eq!(
                problems,
                [format!(
                    "hardshell-core has a {} dependency on hardshell-os, which its row of the dependency \
                     matrix does not allow (docs/01-architecture.md)",
                    kind.as_str()
                )]
            );
        }
    }

    #[test]
    fn a_member_without_a_row_fails() {
        let metadata = metadata(vec![package("hardshell-extra", "crates/extra", &[])]);
        let problems = check(&metadata, MATRIX);
        assert_eq!(problems.len(), 1);
        assert!(
            problems[0].starts_with("hardshell-extra has no row"),
            "{problems:?}"
        );
    }

    #[test]
    fn a_dependency_naming_a_member_is_an_edge_wherever_it_comes_from() {
        let registry = Some("registry+https://github.com/rust-lang/crates.io-index".to_owned());
        let mut core = package(
            "hardshell-core",
            "crates/core",
            &[
                ("hardshell-os", DepKind::Normal),
                ("rand", DepKind::Normal),
                ("vendored", DepKind::Normal),
            ],
        );
        core.dependencies[0].source = registry.clone();
        core.dependencies[1].source = registry;
        let metadata = metadata(vec![core, package("hardshell-os", "crates/os", &[])]);
        // `rand` and the path package `vendored` are not members: `deps`'s concern.
        assert_eq!(
            check(&metadata, MATRIX),
            [
                "hardshell-core has a normal dependency on hardshell-os, which its row of the dependency \
              matrix does not allow (docs/01-architecture.md)"
            ]
        );
    }

    /// Real `cargo metadata` on a throwaway workspace with a forbidden
    /// dev-dependency, a forbidden registry dependency that a `[patch]`
    /// turns into a member, and an allowed build-dependency.
    #[test]
    fn cargo_metadata_shows_a_forbidden_edge() {
        let fixture = Fixture::new("layers");
        fixture.write(
            "Cargo.toml",
            "[workspace]\nresolver = \"3\"\nmembers = [\"core\", \"os\", \"types\"]\n\n\
             [patch.crates-io]\nhardshell-os = { path = \"os\" }\n",
        );
        let manifest = |name: &str, extra: &str| {
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n{extra}"
            )
        };
        fixture.write(
            "core/Cargo.toml",
            &manifest(
                "hardshell-core",
                "[dev-dependencies]\nhardshell-os = { path = \"../os\" }\n",
            ),
        );
        fixture.write("core/src/lib.rs", "");
        fixture.write(
            "os/Cargo.toml",
            &manifest(
                "hardshell-os",
                "[build-dependencies]\nhardshell-core = { path = \"../core\" }\n",
            ),
        );
        fixture.write("os/src/lib.rs", "");
        fixture.write(
            "types/Cargo.toml",
            &manifest(
                "hardshell-types",
                "[dependencies]\nhardshell-os = \"0.1\"\n",
            ),
        );
        fixture.write("types/src/lib.rs", "");
        let metadata = Metadata::load(&fixture.cargo(".")).unwrap();
        let problems = check(&metadata, MATRIX);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(
            problems[0].starts_with("hardshell-core has a dev dependency on hardshell-os"),
            "{problems:?}"
        );
        assert!(
            problems[1].starts_with("hardshell-types has a normal dependency on hardshell-os"),
            "{problems:?}"
        );
    }
}
