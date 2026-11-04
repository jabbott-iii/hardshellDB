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

//! Running Cargo, and the workspace as `cargo metadata` describes it.

use crate::json::Json;
use std::env;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

/// Cargo, pointed at one workspace.
pub(crate) struct Cargo {
    program: OsString,
    manifest: PathBuf,
}

impl Cargo {
    /// Cargo for the workspace whose root manifest is `manifest`. `cargo
    /// run` sets `CARGO` to the Cargo that started it, which is the pinned
    /// toolchain's.
    pub(crate) fn new(manifest: PathBuf) -> Cargo {
        let program = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
        Cargo { program, manifest }
    }

    /// The Cargo program, for commands run in the workspace's directory.
    pub(crate) fn program(&self) -> &OsString {
        &self.program
    }

    /// `cargo <subcommand>` on this workspace, `--locked` and `--offline`:
    /// it never rewrites `Cargo.lock` or calls the network.
    pub(crate) fn command(&self, subcommand: &str) -> Command {
        let mut command = Command::new(&self.program);
        command
            .arg(subcommand)
            .arg("--manifest-path")
            .arg(&self.manifest)
            .args(["--locked", "--offline"]);
        command
    }
}

/// Runs `command` and returns what it printed, or an error carrying its
/// standard error if it failed.
pub(crate) fn output(mut command: Command) -> Result<String, String> {
    let shown = format!("{command:?}");
    let output = command
        .output()
        .map_err(|e| format!("cannot run {shown}: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "{shown} failed ({}):\n{}",
            output.status,
            stderr.trim_end()
        ));
    }
    String::from_utf8(output.stdout).map_err(|_| format!("{shown} printed text that is not UTF-8"))
}

/// The kind of a dependency: a manifest's `[dependencies]`,
/// `[dev-dependencies]` or `[build-dependencies]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DepKind {
    Normal,
    Dev,
    Build,
}

impl DepKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            DepKind::Normal => "normal",
            DepKind::Dev => "dev",
            DepKind::Build => "build",
        }
    }
}

/// A dependency as a manifest declares it, on any target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Dependency {
    /// The package's name, not a rename.
    pub(crate) name: String,
    /// Where it comes from: `None` for a path dependency.
    pub(crate) source: Option<String>,
    pub(crate) kind: DepKind,
}

/// A package in the resolved dependency graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Package {
    pub(crate) name: String,
    pub(crate) version: String,
    /// Where it comes from: `None` for a path package, such as a member.
    pub(crate) source: Option<String>,
    pub(crate) manifest: PathBuf,
    /// A member of the workspace.
    pub(crate) member: bool,
    /// The root file of its library, if it has one.
    pub(crate) lib_root: Option<PathBuf>,
    /// Its targets: library, binaries, tests, benches, examples, build
    /// script.
    pub(crate) targets: Vec<Target>,
    pub(crate) dependencies: Vec<Dependency>,
}

/// A package's target, as Cargo builds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) name: String,
    /// `lib`, `bin`, `test`, `custom-build`, ...
    pub(crate) kinds: Vec<String>,
    /// Its root file, with `.` and `..` resolved: Cargo keeps a manifest's
    /// `path = "../x.rs"` as written.
    pub(crate) src_path: PathBuf,
}

impl Target {
    /// Whether Cargo builds it into what the project ships: a library, a
    /// binary or a build script.
    pub(crate) fn ships(&self) -> bool {
        self.kinds.iter().any(|kind| {
            matches!(
                kind.as_str(),
                "lib"
                    | "rlib"
                    | "dylib"
                    | "cdylib"
                    | "staticlib"
                    | "proc-macro"
                    | "bin"
                    | "custom-build"
            )
        })
    }
}

/// `path` with its `.` and `..` resolved, without asking the file system.
pub(crate) fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

impl Package {
    /// The directory that holds its manifest.
    pub(crate) fn dir(&self) -> &Path {
        self.manifest.parent().unwrap_or(&self.manifest)
    }
}

/// What the checks need from `cargo metadata`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Metadata {
    pub(crate) root: PathBuf,
    pub(crate) target_dir: PathBuf,
    /// Every package in the graph, members included, for every target and
    /// with every feature on.
    pub(crate) packages: Vec<Package>,
}

impl Metadata {
    /// Runs `cargo metadata` on the workspace.
    pub(crate) fn load(cargo: &Cargo) -> Result<Metadata, String> {
        let mut command = cargo.command("metadata");
        command.args(["--format-version", "1", "--all-features"]);
        let text = output(command)?;
        Metadata::from_json(&Json::parse(&text)?)
    }

    /// Reads the output of `cargo metadata --format-version 1`.
    pub(crate) fn from_json(json: &Json) -> Result<Metadata, String> {
        let members: Vec<&str> = array(json, "workspace_members")?
            .iter()
            .map(|id| id.as_str().ok_or("a workspace member's id is not a string"))
            .collect::<Result<_, _>>()?;
        let packages = array(json, "packages")?
            .iter()
            .map(|package| {
                let name = string(package, "name")?;
                let targets = array(package, "targets")?
                    .iter()
                    .map(|target| {
                        let kinds = array(target, "kind")?
                            .iter()
                            .map(|kind| {
                                kind.as_str()
                                    .map(str::to_owned)
                                    .ok_or("a target kind is not a string")
                            })
                            .collect::<Result<Vec<String>, _>>()?;
                        Ok(Target {
                            name: string(target, "name")?,
                            kinds,
                            src_path: normalize(Path::new(&string(target, "src_path")?)),
                        })
                    })
                    .collect::<Result<Vec<Target>, String>>()?;
                let lib_root = targets
                    .iter()
                    .find(|t| {
                        t.kinds.iter().any(|k| k != "bin" && k != "custom-build") && t.ships()
                    })
                    .map(|t| t.src_path.clone());
                let dependencies = array(package, "dependencies")?
                    .iter()
                    .map(|dependency| {
                        let kind = match field(dependency, "kind")? {
                            Json::Null => DepKind::Normal,
                            Json::String(kind) if kind == "dev" => DepKind::Dev,
                            Json::String(kind) if kind == "build" => DepKind::Build,
                            other => return Err(format!("dependency kind {other:?}")),
                        };
                        Ok(Dependency {
                            name: string(dependency, "name")?,
                            source: optional_string(dependency, "source")?,
                            kind,
                        })
                    })
                    .collect::<Result<_, String>>()
                    .map_err(|e| format!("package {name}: {e}"))?;
                Ok(Package {
                    member: members.contains(&string(package, "id")?.as_str()),
                    version: string(package, "version")?,
                    source: optional_string(package, "source")?,
                    manifest: PathBuf::from(string(package, "manifest_path")?),
                    name,
                    lib_root,
                    targets,
                    dependencies,
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(Metadata {
            root: PathBuf::from(string(json, "workspace_root")?),
            target_dir: PathBuf::from(string(json, "target_directory")?),
            packages,
        })
    }

    /// The workspace's members, sorted by name.
    pub(crate) fn members(&self) -> Vec<&Package> {
        let mut members: Vec<&Package> = self.packages.iter().filter(|p| p.member).collect();
        members.sort_by(|a, b| a.name.cmp(&b.name));
        members
    }
}

fn field<'a>(json: &'a Json, name: &str) -> Result<&'a Json, String> {
    json.get(name)
        .ok_or_else(|| format!("`cargo metadata` gave no `{name}`"))
}

fn string(json: &Json, name: &str) -> Result<String, String> {
    field(json, name)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("`{name}` is not a string"))
}

fn optional_string(json: &Json, name: &str) -> Result<Option<String>, String> {
    match field(json, name)? {
        Json::Null => Ok(None),
        Json::String(text) => Ok(Some(text.clone())),
        _ => Err(format!("`{name}` is neither a string nor null")),
    }
}

fn array<'a>(json: &'a Json, name: &str) -> Result<&'a [Json], String> {
    field(json, name)?
        .as_array()
        .ok_or_else(|| format!("`{name}` is not an array"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{Cargo, DepKind, Dependency, Metadata, Package, Target, normalize};
    use crate::json::Json;
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};

    /// A package for a test: a member at `/ws/<dir>` when `source` is
    /// `None` and `dir` is not `..`-relative, with these dependencies.
    pub(crate) fn package(name: &str, dir: &str, deps: &[(&str, DepKind)]) -> Package {
        Package {
            name: name.to_owned(),
            version: "0.1.0".to_owned(),
            source: None,
            manifest: PathBuf::from(format!("/ws/{dir}/Cargo.toml")),
            member: true,
            lib_root: Some(PathBuf::from(format!("/ws/{dir}/src/lib.rs"))),
            targets: vec![Target {
                name: name.replace('-', "_"),
                kinds: vec!["lib".to_owned()],
                src_path: PathBuf::from(format!("/ws/{dir}/src/lib.rs")),
            }],
            dependencies: deps
                .iter()
                .map(|&(name, kind)| Dependency {
                    name: name.to_owned(),
                    source: None,
                    kind,
                })
                .collect(),
        }
    }

    const METADATA: &str = r#"{
      "packages": [
        {"name": "hardshell-core", "version": "0.1.0",
         "id": "path+file:///ws/crates/core#hardshell-core@0.1.0", "source": null,
         "dependencies": [
           {"name": "hardshell-os", "source": null, "kind": "dev", "rename": null},
           {"name": "cc", "source": "registry+https://github.com/rust-lang/crates.io-index", "kind": "build"},
           {"name": "rand", "source": "registry+https://github.com/rust-lang/crates.io-index", "kind": null}
         ],
         "targets": [{"kind": ["lib"], "name": "hardshell_core", "src_path": "/ws/crates/core/src/lib.rs"},
                     {"kind": ["test"], "name": "t", "src_path": "/ws/crates/core/tests/t.rs"},
                     {"kind": ["example"], "name": "e", "src_path": "/ws/crates/core/src/../examples/e.rs"}],
         "manifest_path": "/ws/crates/core/Cargo.toml"},
        {"name": "rand", "version": "0.9.0",
         "id": "registry+https://github.com/rust-lang/crates.io-index#rand@0.9.0",
         "source": "registry+https://github.com/rust-lang/crates.io-index",
         "dependencies": [], "targets": [], "manifest_path": "/home/.cargo/registry/rand/Cargo.toml"}
      ],
      "workspace_members": ["path+file:///ws/crates/core#hardshell-core@0.1.0"],
      "workspace_root": "/ws",
      "target_directory": "/ws/target",
      "version": 1
    }"#;

    #[test]
    fn metadata_is_read() {
        let metadata = Metadata::from_json(&Json::parse(METADATA).unwrap()).unwrap();
        assert_eq!(metadata.root, PathBuf::from("/ws"));
        assert_eq!(metadata.target_dir, PathBuf::from("/ws/target"));
        let [core, rand] = metadata.packages.as_slice() else {
            panic!("two packages")
        };
        assert!(core.member);
        assert_eq!(core.source, None);
        assert_eq!(core.dir(), PathBuf::from("/ws/crates/core"));
        assert_eq!(
            core.lib_root,
            Some(PathBuf::from("/ws/crates/core/src/lib.rs"))
        );
        let kinds: Vec<(&str, DepKind, bool)> = core
            .dependencies
            .iter()
            .map(|d| (d.name.as_str(), d.kind, d.source.is_some()))
            .collect();
        assert_eq!(
            kinds,
            [
                ("hardshell-os", DepKind::Dev, false),
                ("cc", DepKind::Build, true),
                ("rand", DepKind::Normal, true)
            ]
        );
        let targets: Vec<(&str, &str, PathBuf)> = core
            .targets
            .iter()
            .map(|t| (t.name.as_str(), t.kinds[0].as_str(), t.src_path.clone()))
            .collect();
        assert_eq!(
            targets,
            [
                (
                    "hardshell_core",
                    "lib",
                    PathBuf::from("/ws/crates/core/src/lib.rs")
                ),
                ("t", "test", PathBuf::from("/ws/crates/core/tests/t.rs")),
                (
                    "e",
                    "example",
                    PathBuf::from("/ws/crates/core/examples/e.rs")
                ),
            ]
        );
        assert!(!rand.member);
        assert_eq!(rand.version, "0.9.0");
        assert_eq!(rand.lib_root, None);
        assert!(
            rand.source
                .as_deref()
                .is_some_and(|s| s.starts_with("registry+"))
        );
        assert_eq!(metadata.members().len(), 1);
    }

    #[test]
    fn paths_are_normalized_without_the_file_system() {
        assert_eq!(
            normalize(Path::new("/ws/a/../shared/./main.rs")),
            PathBuf::from("/ws/shared/main.rs")
        );
        assert_eq!(
            normalize(Path::new("/ws/a/src/lib.rs")),
            PathBuf::from("/ws/a/src/lib.rs")
        );
        assert_eq!(normalize(Path::new("/../x")), PathBuf::from("/x"));
    }

    #[test]
    fn shipped_targets_are_libraries_binaries_and_build_scripts() {
        let target = |kind: &str| Target {
            name: "t".to_owned(),
            kinds: vec![kind.to_owned()],
            src_path: PathBuf::new(),
        };
        for kind in ["lib", "rlib", "proc-macro", "bin", "custom-build"] {
            assert!(target(kind).ships(), "{kind}");
        }
        for kind in ["test", "bench", "example"] {
            assert!(!target(kind).ships(), "{kind}");
        }
    }

    #[test]
    fn cargo_runs_locked_and_offline() {
        let cargo = Cargo::new(PathBuf::from("/ws/Cargo.toml"));
        let command = cargo.command("tree");
        let args: Vec<&OsStr> = command.get_args().collect();
        assert_eq!(
            args,
            [
                "tree",
                "--manifest-path",
                "/ws/Cargo.toml",
                "--locked",
                "--offline"
            ]
        );
    }

    #[test]
    fn unexpected_metadata_is_an_error() {
        for bad in [
            METADATA.replace(r#""workspace_root": "/ws","#, ""),
            METADATA.replace(r#""kind": "dev""#, r#""kind": "weird""#),
            METADATA.replace(r#""version": "0.9.0""#, r#""version": 9"#),
            METADATA.replace(r#""source": null,"#, r#""source": 1,"#),
        ] {
            let json = Json::parse(&bad).unwrap();
            assert!(Metadata::from_json(&json).is_err(), "{bad}");
        }
    }
}
