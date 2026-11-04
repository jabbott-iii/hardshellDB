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

//! The files the source checks read: every file under the workspace root,
//! committed or not, but Git's and Cargo's own.

use std::fs;
use std::path::{Path, PathBuf};

/// The first line of a `CACHEDIR.TAG`, by the Cache Directory Tagging
/// specification, which Cargo follows for its build directories.
const CACHEDIR_SIGNATURE: &str = "Signature: 8a477f597d28d172789f06886806bc55";

/// Every file under `root`, as sorted paths relative to it. Skips `.git`,
/// the build directory `target_dir`, symbolic links to directories, and
/// two kinds of directory outside `packages` (the members' directories):
/// one that holds a `CACHEDIR.TAG` with the tag's signature, as Cargo's
/// build directories do, and one that holds a `.git` of its own, another
/// checkout (a Git worktree, say) with its own copy of the sources. Inside
/// a package nothing is skipped: code there is compiled.
pub(crate) fn list(
    root: &Path,
    target_dir: &Path,
    packages: &[&Path],
) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(dir) = pending.pop() {
        let path = root.join(&dir);
        let entries =
            fs::read_dir(&path).map_err(|e| format!("cannot list {}: {e}", path.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("cannot list {}: {e}", path.display()))?;
            let relative = dir.join(entry.file_name());
            let full = root.join(&relative);
            let kind = entry
                .file_type()
                .map_err(|e| format!("cannot inspect {}: {e}", full.display()))?;
            let is_dir = kind.is_dir();
            let is_file = kind.is_file() || (kind.is_symlink() && full.is_file());
            if is_dir {
                let outside = !packages.iter().any(|p| full.starts_with(p));
                let skipped = entry.file_name() == ".git"
                    || full == target_dir
                    || (outside && (tagged(&full) || full.join(".git").exists()));
                if !skipped {
                    pending.push(relative);
                }
            } else if is_file {
                files.push(relative);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Whether `dir` holds a `CACHEDIR.TAG` that starts with the signature.
fn tagged(dir: &Path) -> bool {
    fs::read(dir.join("CACHEDIR.TAG"))
        .is_ok_and(|tag| tag.starts_with(CACHEDIR_SIGNATURE.as_bytes()))
}

/// A relative path as the checks print it: `/` between its parts on every
/// OS.
pub(crate) fn display(relative: &Path) -> String {
    let parts: Vec<String> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::{CACHEDIR_SIGNATURE, display, list};
    use crate::fixture::Fixture;
    use std::path::{Path, PathBuf};

    #[test]
    fn git_and_build_directories_are_skipped() {
        let fixture = Fixture::new("files");
        let tag = format!("{CACHEDIR_SIGNATURE}\n# This file is a cache directory tag.\n");
        for (path, text) in [
            ("crates/a/src/lib.rs", ""),
            ("crates/a/src/b/c.rs", ""),
            (".github/workflows/ci.yml", ""),
            (".git/HEAD", ""),
            ("target/debug/x.rs", ""),
            ("target-ra/CACHEDIR.TAG", tag.as_str()),
            ("target-ra/out.rs", ""),
            // A tag without the signature, or inside a package, hides nothing.
            ("notes/CACHEDIR.TAG", "not a tag"),
            ("notes/n.rs", ""),
            ("crates/a/src/hidden/CACHEDIR.TAG", tag.as_str()),
            ("crates/a/src/hidden/h.rs", ""),
            // Another checkout, such as a worktree: skipped outside a package.
            (".claude/worktrees/w/.git", "gitdir: elsewhere"),
            (".claude/worktrees/w/src/lib.rs", ""),
            ("crates/a/src/checkout/.git", ""),
            ("crates/a/src/checkout/k.rs", ""),
        ] {
            fixture.write(path, text);
        }
        let package = fixture.root().join("crates").join("a");
        let files = list(
            fixture.root(),
            &fixture.root().join("target"),
            &[package.as_path()],
        )
        .unwrap();
        let shown: Vec<String> = files.iter().map(|f| display(f)).collect();
        assert_eq!(
            shown,
            [
                ".github/workflows/ci.yml",
                "crates/a/src/b/c.rs",
                "crates/a/src/checkout/.git",
                "crates/a/src/checkout/k.rs",
                "crates/a/src/hidden/CACHEDIR.TAG",
                "crates/a/src/hidden/h.rs",
                "crates/a/src/lib.rs",
                "notes/CACHEDIR.TAG",
                "notes/n.rs",
            ]
        );
    }

    #[test]
    fn paths_display_with_slashes() {
        assert_eq!(
            display(&PathBuf::from("a").join("b").join("c.rs")),
            "a/b/c.rs"
        );
        assert_eq!(display(Path::new("x.rs")), "x.rs");
    }
}
