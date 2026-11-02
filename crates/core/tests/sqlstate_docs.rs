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

//! Every SQLSTATE written in `docs/` has a constant, and the table in
//! `docs/subsystems/core.md` lists exactly the constants, in `SqlState::ALL`'s
//! order, with their PostgreSQL condition names (M0.2).

use hardshell_core::sqlstate::SqlState;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const TABLE_HEADING: &str = "## SQLSTATE codes the engine raises";

/// PostgreSQL's SQLSTATE classes, from `src/backend/utils/errcodes.txt` of
/// PostgreSQL 17.4. A bare code-shaped word in one of them reads as a SQLSTATE.
const POSTGRES_CLASSES: [&str; 42] = [
    "00", "01", "02", "03", "08", "09", "0A", "0B", "0F", "0L", "0P", "0Z", "20", "21", "22", "23",
    "24", "25", "26", "27", "28", "2B", "2D", "2F", "34", "38", "39", "3B", "3D", "3F", "40", "42",
    "44", "53", "54", "55", "57", "58", "F0", "HV", "P0", "XX",
];

/// Words in `docs/` that look like a SQLSTATE but are not one, each with
/// where it appears. The test fails when an entry no longer appears.
const NOT_SQLSTATES: &[&str] = &[
    "20000", // `cargo xtask crash --iterations 20000` in docs/roadmap.md, not case_not_found
];

fn docs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Every `*.md` under `dir`, recursively, in sorted order.
fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|entry| entry.expect("directory entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            markdown_files(&path, out);
        } else if path.extension().is_some_and(|x| x == "md") {
            out.push(path);
        }
    }
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Five of `[0-9A-Z]`, at least one a digit (every SQLSTATE has one; `BEGIN` has none).
fn is_code_shaped(word: &[u8]) -> bool {
    word.len() == 5
        && word
            .iter()
            .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase())
        && word.iter().any(u8::is_ascii_digit)
}

/// One code-shaped word in a line, and whether it is a whole code span.
struct Word<'a> {
    text: &'a str,
    whole_span: bool,
}

/// The code-shaped words of `line`: maximal runs of `[0-9A-Za-z_]`.
fn code_shaped_words(line: &str) -> Vec<Word<'_>> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if !is_word_byte(bytes[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && is_word_byte(bytes[i]) {
            i += 1;
        }
        if is_code_shaped(&bytes[start..i]) {
            let whole_span =
                start > 0 && bytes[start - 1] == b'`' && i < bytes.len() && bytes[i] == b'`';
            out.push(Word {
                text: &line[start..i],
                whole_span,
            });
        }
    }
    out
}

/// The SQLSTATE mentions in one line: every code-shaped word that is (a) a
/// whole code span, or (b) in one of PostgreSQL's classes, wherever it is
/// (prose, headings, tables, fenced blocks), unless it is in `NOT_SQLSTATES`.
fn mentions(line: &str) -> Vec<&str> {
    code_shaped_words(line)
        .into_iter()
        .filter(|w| w.whole_span || POSTGRES_CLASSES.contains(&&w.text[..2]))
        .map(|w| w.text)
        .filter(|text| !NOT_SQLSTATES.contains(text))
        .collect()
}

fn all_codes() -> Vec<&'static str> {
    SqlState::ALL.iter().map(SqlState::as_str).collect()
}

#[test]
fn every_sqlstate_in_the_docs_has_a_constant() {
    let docs = docs_dir();
    let mut files = Vec::new();
    markdown_files(&docs, &mut files);
    assert!(
        files.len() >= 10,
        "read only {} files under {}",
        files.len(),
        docs.display()
    );

    let known: BTreeSet<&str> = all_codes().into_iter().collect();
    let mut found = BTreeSet::new();
    let mut not_sqlstates_seen = BTreeSet::new();
    let mut missing = Vec::new();
    for path in &files {
        let text = read(path);
        let shown = path
            .strip_prefix(&docs)
            .unwrap_or(path)
            .display()
            .to_string();
        for (n, line) in text.lines().enumerate() {
            for w in code_shaped_words(line) {
                if NOT_SQLSTATES.contains(&w.text) {
                    not_sqlstates_seen.insert(w.text.to_owned());
                }
            }
            for code in mentions(line) {
                found.insert(code.to_owned());
                if !known.contains(code) {
                    missing.push(format!("docs/{shown}:{}: {code}", n + 1));
                }
            }
        }
    }
    assert!(
        found.contains("42601"),
        "the scan found no SYNTAX_ERROR: is {} the docs?",
        docs.display()
    );
    assert!(
        missing.is_empty(),
        "SQLSTATEs in docs/ without a constant:\n{}\n\
         Add a row to the table in docs/subsystems/core.md and a constant to sqlstates! in \
         crates/core/src/sqlstate.rs. If a word is not a SQLSTATE, write it with a separator \
         (16,384), or add it to NOT_SQLSTATES in this test, saying where it appears.",
        missing.join("\n")
    );
    let stale: Vec<_> = NOT_SQLSTATES
        .iter()
        .filter(|w| !not_sqlstates_seen.contains(**w))
        .collect();
    assert!(
        stale.is_empty(),
        "NOT_SQLSTATES entries that no longer appear in docs/: {stale:?}; remove them"
    );
}

/// The `(code, condition name)` pairs of the table in core.md, in row order.
/// A row lists its codes in the first cell and the same number of names, in
/// the same order, as code spans in the second.
fn core_table() -> Vec<(String, String)> {
    let path = docs_dir().join("subsystems/core.md");
    let text = read(&path);
    let mut lines = text.lines().skip_while(|l| *l != TABLE_HEADING);
    assert!(
        lines.next().is_some(),
        "{} has no line {TABLE_HEADING:?}",
        path.display()
    );
    let mut rows = Vec::new();
    for line in lines.take_while(|l| !l.starts_with("## ")) {
        let mut cells = line.split('|').skip(1);
        let (Some(code_cell), Some(name_cell)) = (cells.next(), cells.next()) else {
            continue;
        };
        let codes: Vec<&str> = code_shaped_words(code_cell)
            .into_iter()
            .filter(|w| w.whole_span)
            .map(|w| w.text)
            .collect();
        if codes.is_empty() {
            continue; // the header and alignment rows
        }
        let names: Vec<&str> = name_cell
            .split('`')
            .skip(1)
            .step_by(2)
            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'))
            .collect();
        assert_eq!(
            codes.len(),
            names.len(),
            "core.md table row {line:?}: each code needs its PostgreSQL condition name in \
             backticks in the Name cell, in the same order"
        );
        rows.extend(
            codes
                .into_iter()
                .zip(names)
                .map(|(c, n)| (c.to_owned(), n.to_owned())),
        );
    }
    rows
}

/// The `(constant, code, first doc line)` triples of the `sqlstates!` table in
/// `src/sqlstate.rs`, in order.
fn source_table() -> Vec<(String, String, String)> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/sqlstate.rs");
    let text = read(&path);
    let body = text
        .split_once("\nsqlstates! {\n")
        .map(|(_, rest)| rest)
        .unwrap_or_else(|| panic!("{} has no `sqlstates! {{` table", path.display()));
    let mut out = Vec::new();
    let mut doc: Option<String> = None;
    for line in body.lines().map(str::trim) {
        if line == "}" {
            break;
        }
        if let Some(text) = line.strip_prefix("///") {
            doc.get_or_insert_with(|| text.trim().to_owned());
        } else if let Some((name, code)) = line
            .strip_suffix("\";")
            .and_then(|l| l.split_once(" = b\""))
        {
            out.push((
                name.to_owned(),
                code.to_owned(),
                doc.take().unwrap_or_default(),
            ));
        }
    }
    out
}

#[test]
fn core_table_matches_the_constants() {
    let table = core_table();
    let codes: Vec<&str> = table.iter().map(|(c, _)| c.as_str()).collect();
    let all = all_codes();
    let row_set: BTreeSet<&str> = codes.iter().copied().collect();
    let all_set: BTreeSet<&str> = all.iter().copied().collect();
    assert_eq!(
        row_set.len(),
        codes.len(),
        "core.md table lists a code twice: {codes:?}"
    );
    let no_constant: Vec<_> = row_set.difference(&all_set).collect();
    let no_row: Vec<_> = all_set.difference(&row_set).collect();
    assert!(
        no_constant.is_empty() && no_row.is_empty(),
        "core.md table rows without a constant: {no_constant:?}; constants without a row: {no_row:?}"
    );
    assert_eq!(
        codes, all,
        "SqlState::ALL must follow the core.md table's order"
    );

    let source = source_table();
    let source_codes: Vec<&str> = source.iter().map(|(_, c, _)| c.as_str()).collect();
    assert_eq!(
        source_codes, all,
        "parsing src/sqlstate.rs must find SqlState::ALL"
    );
    for ((code, name), (constant, _, doc)) in table.iter().zip(&source) {
        assert_eq!(
            *constant,
            name.to_uppercase(),
            "{code} is {name:?} in core.md but its constant is {constant}"
        );
        let want = format!("`{code}` `{name}`:");
        assert!(
            doc.starts_with(&want),
            "{constant}'s doc comment must start with {want:?}, found {doc:?}"
        );
    }
}

#[test]
fn scanner_finds_codes_and_skips_look_alikes() {
    let cases: &[(&str, &[&str])] = &[
        ("`42601`", &["42601"]),
        ("| `22007` / `22008` | x |", &["22007", "22008"]),
        ("``42601``", &["42601"]),
        ("    SQLSTATE 55P03 on timeout", &["55P03"]),
        ("statement error 23506", &["23506"]),
        // Classes with no constant yet are still read as SQLSTATEs.
        ("statement error 2D000", &["2D000"]),
        ("fails clearly (`28P01 invalid_password`).", &["28P01"]),
        ("## Authentication fails with 28P01", &["28P01"]),
        ("BadPassword, // -> 28P01 invalid_password", &["28P01"]),
        ("raises `44000`", &["44000"]),
        // Not a PostgreSQL class, but a whole code span: a typo, or a number to write as 16,384.
        ("typo `52601`", &["52601"]),
        ("`16384`", &["16384"]),
        ("FAT32 and `BEGIN` and `UNION`", &[]),
        ("page size 16384, 32768 or 65536; $1 .. $65535", &[]),
        ("`cargo xtask crash --iterations 20000`", &[]),
        ("SQLSTATE_42601 x42601 42601x 42p01", &[]),
        ("0x82F63B78 `x86_64` `INT8`", &[]),
    ];
    for (line, want) in cases {
        assert_eq!(mentions(line), *want, "line {line:?}");
    }
}
