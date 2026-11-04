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

//! `cargo xtask headers`: every source file starts with the Apache-2.0
//! header from `LICENSE`'s appendix, in its comment syntax, after a
//! shebang line if it has one (ADR-0015).
//!
//! Source files are Rust (`.rs`), scripts (`.sh`, `.bash`, `.py`, `.ps1`)
//! and GitHub workflows (`.github/workflows/*.yml`). Manifests, configs,
//! docs, data and fixtures are not source and carry no header.

use crate::{Report, Workspace, files, lexer};
use std::fs;
use std::path::Path;

/// How a source file starts a line comment, or `None` if it is not a
/// source file.
fn comment_marker(relative: &Path) -> Option<&'static str> {
    let extension = relative.extension()?.to_str()?;
    match extension {
        "rs" => Some("//"),
        "sh" | "bash" | "py" | "ps1" => Some("#"),
        "yml" | "yaml"
            if relative
                .parent()
                .is_some_and(|dir| dir == Path::new(".github").join("workflows")) =>
        {
            Some("#")
        }
        _ => None,
    }
}

pub(crate) fn run(workspace: &Workspace) -> Result<Report, String> {
    let root = &workspace.metadata.root;
    let license = fs::read_to_string(root.join("LICENSE"))
        .map_err(|e| format!("cannot read LICENSE: {e}"))?;
    let header = license_header(&license)?;
    let mut problems = Vec::new();
    let mut checked = 0;
    let members = workspace.metadata.members();
    let dirs: Vec<&Path> = members.iter().map(|p| p.dir()).collect();
    for relative in files::list(root, &workspace.metadata.target_dir, &dirs)? {
        let Some(marker) = comment_marker(&relative) else {
            continue;
        };
        let shown = files::display(&relative);
        checked += 1;
        match fs::read_to_string(root.join(&relative)) {
            Ok(text) => {
                if let Err(problem) = check(&text, marker, &header) {
                    problems.push(format!("{shown}: {problem}"));
                }
            }
            Err(e) => problems.push(format!("{shown}: cannot read it: {e}")),
        }
    }
    Ok(Report {
        problems,
        summary: vec![format!("{checked} source files checked")],
    })
}

/// The header in `LICENSE`'s appendix: from its `Copyright` line to
/// `limitations under the License.`, without the appendix's indentation.
pub(crate) fn license_header(license: &str) -> Result<Vec<String>, String> {
    let appendix = license
        .split_once("APPENDIX:")
        .map(|(_, rest)| rest)
        .ok_or("LICENSE has no APPENDIX")?;
    let lines: Vec<&str> = appendix
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("Copyright "))
        .collect();
    let end = lines
        .iter()
        .position(|line| line.trim() == "limitations under the License.")
        .ok_or("LICENSE's appendix has no header ending `limitations under the License.`")?;
    let header = &lines[..=end];
    let indent = header
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    Ok(header
        .iter()
        .map(|line| line.get(indent..).unwrap_or("").trim_end().to_owned())
        .collect())
}

/// Whether `text` starts with `header` as comments, after a shebang line
/// if there is one: a `#!` line, which in Rust must not open an inner
/// attribute (`lexer::shebang_len`). Line endings may be `\n` or `\r\n`.
pub(crate) fn check(text: &str, marker: &str, header: &[String]) -> Result<(), String> {
    let mut lines = text.lines().enumerate();
    let shebang = if marker == "//" {
        lexer::shebang_len(text) > 0
    } else {
        text.starts_with("#!")
    };
    if shebang {
        lines.next();
    }
    for expected in header {
        let expected = if expected.is_empty() {
            marker.to_owned()
        } else {
            format!("{marker} {expected}")
        };
        match lines.next() {
            Some((_, line)) if line == expected => {}
            Some((index, line)) => {
                return Err(format!(
                    "no Apache-2.0 header (ADR-0015): line {} is `{line}`, not `{expected}`",
                    index + 1
                ));
            }
            None => return Err("no Apache-2.0 header (ADR-0015): the file ends first".to_owned()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{check, comment_marker, license_header};
    use std::path::Path;

    fn header() -> Vec<String> {
        license_header(include_str!("../../LICENSE")).unwrap()
    }

    fn commented(marker: &str) -> String {
        header()
            .iter()
            .map(|line| {
                if line.is_empty() {
                    marker.to_owned()
                } else {
                    format!("{marker} {line}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_header_is_licenses_appendix() {
        let header = header();
        assert_eq!(header.len(), 13);
        assert_eq!(header[0], "Copyright 2026 Joseph Anthony Abbott III");
        assert_eq!(header[6], "    http://www.apache.org/licenses/LICENSE-2.0");
        assert_eq!(header[12], "limitations under the License.");
        // This file shows it as every Rust file does.
        let this = include_str!("headers.rs").replace("\r\n", "\n");
        assert!(this.starts_with(&(commented("//") + "\n\n//! `cargo xtask headers`")));
        assert!(license_header("no appendix").is_err());
        assert!(license_header("APPENDIX: Copyright 2026 X\n no end").is_err());
    }

    #[test]
    fn files_with_the_header_pass() {
        let header = header();
        let rust = commented("//") + "\n\n//! A crate.\n";
        assert_eq!(check(&rust, "//", &header), Ok(()));
        assert_eq!(
            check(&rust.replace('\n', "\r\n"), "//", &header),
            Ok(()),
            "CRLF"
        );
        let script = format!("#!/bin/sh\n{}\nset -eu\n", commented("#"));
        assert_eq!(check(&script, "#", &header), Ok(()));
        let rust_script = format!("#!/usr/bin/env -S cargo +nightly -Zscript\n{rust}");
        assert_eq!(check(&rust_script, "//", &header), Ok(()));
    }

    #[test]
    fn files_without_it_fail() {
        let header = header();
        let rust = commented("//") + "\n";
        for broken in [
            String::new(),
            "//! A crate.\n".to_owned(),
            rust.replace("Version 2.0", "Version 3.0"),
            rust.replace("// limitations under the License.\n", ""),
            rust.replace("//\n", "// \n"),
            format!("\n{rust}"),
            format!("\u{feff}{rust}"),
            format!("#![allow(x)]\n{rust}"),
            format!("#!/**/[allow(x)]\n{rust}"),
            commented("#"),
        ] {
            assert!(check(&broken, "//", &header).is_err(), "{broken:?}");
        }
        let error = check(&rust.replace("Version 2.0", "Version 3.0"), "//", &header).unwrap_err();
        assert!(
            error.contains("line 3 is `// Licensed under the Apache License, Version 3.0"),
            "{error}"
        );
    }

    #[test]
    fn source_files_are_known_by_extension_and_place() {
        for (path, marker) in [
            ("crates/core/src/lib.rs", Some("//")),
            ("scripts/release.sh", Some("#")),
            ("tools/gen.py", Some("#")),
            ("tools/setup.ps1", Some("#")),
            (".github/workflows/ci.yml", Some("#")),
            (".github/workflows/nightly.yaml", Some("#")),
            (".github/dependabot.yml", None),
            ("Cargo.toml", None),
            ("docs/roadmap.md", None),
            ("xtask/deps-allowlist.txt", None),
            ("LICENSE", None),
        ] {
            assert_eq!(comment_marker(Path::new(path)), marker, "{path}");
        }
    }
}
