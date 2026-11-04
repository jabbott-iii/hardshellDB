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

//! `cargo xtask unsafe-audit`: `unsafe` only where ADR-0013 and ADR-0017
//! allow it, each with its `// SAFETY:` comment, and `std::fs` only
//! outside the engine, whose file access goes through the VFS.
//!
//! It reads every `.rs` file under the workspace root as tokens, so
//! comments and strings are not code, and judges each file by where it
//! sits. Three rules keep that sound. Every target Cargo builds has its
//! root inside its own package, and a library's or binary's never in
//! `tests/`. A `#[path]` module or an `include!` names a `.rs` file in the
//! same top-level directory of its package (`src/`, `tests/`, ...). And
//! every library root carries [`DOCTEST_FORBID`]: rustdoc compiles doc
//! examples without the workspace's lints, so only that attribute keeps
//! `unsafe` out of them, whatever form their docs take.

use crate::files;
use crate::lexer::{self, Kind, Lexed};
use crate::metadata::{Package, normalize};
use crate::{Report, Workspace};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// The ADRs that let `hardshell-os` use `unsafe`, each naming the OS call
/// `std` lacks (ADR-0013). None yet: add one only once the user accepts it.
const OS_UNSAFE_ADRS: &[&str] = &[];

const OS: &str = "hardshell-os";

/// The packages that may use `std::fs` and `std::os`, besides every
/// package's `tests/`.
const FS_PACKAGES: [&str; 4] = ["hardshell-os", "xtask", "hardshell-cli", "hardshell-server"];

/// How many lines above an `unsafe` its `// SAFETY:` comment may end.
const SAFETY_WINDOW: usize = 3;

/// The attribute every library root carries, so that the compiler rejects
/// `unsafe` in its doc examples, even one that allows it for itself.
const DOCTEST_FORBID: &str = "#![doc(test(attr(forbid(unsafe_code))))]";

/// Where a file sits, which decides what it may hold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Place {
    /// In `hardshell-os`.
    os: bool,
    /// `hardshell-os`'s library root, where `#![allow(unsafe_code)]` goes
    /// once an ADR allows it.
    os_root: bool,
    /// A member's library root, which must carry `DOCTEST_FORBID`.
    lib_root: bool,
    /// Directly in a package's `tests/`: an integration-test binary, which
    /// may hold an ADR-0017 counting allocator.
    test_binary: bool,
    /// May use `std::fs` and `std::os`: in `FS_PACKAGES`, or under a
    /// package's `tests/`.
    fs_allowed: bool,
}

/// What a file holds.
#[derive(Debug, Default)]
struct Audit {
    problems: Vec<String>,
    unsafe_count: usize,
    fs_count: usize,
}

pub(crate) fn run(workspace: &Workspace) -> Result<Report, String> {
    let metadata = &workspace.metadata;
    let members = metadata.members();
    let dirs: Vec<&Path> = members.iter().map(|p| p.dir()).collect();
    let mut problems = targets_outside_their_place(&members);
    // Files, `unsafe` keywords, and `std::fs` and `std::os` paths, per
    // package.
    let mut counts: BTreeMap<&str, [usize; 3]> =
        members.iter().map(|p| (p.name.as_str(), [0; 3])).collect();
    for relative in files::list(&metadata.root, &metadata.target_dir, &dirs)? {
        if relative
            .extension()
            .is_none_or(|extension| extension != "rs")
        {
            continue;
        }
        let path = metadata.root.join(&relative);
        let shown = files::display(&relative);
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(e) => {
                problems.push(format!("{shown}: cannot read it: {e}"));
                continue;
            }
        };
        let package = package_of(&path, &members);
        let dir = package.map(|p| dir_within(&path, p));
        let audit = audit(
            &shown,
            &source,
            place(&path, package),
            dir.as_deref(),
            !OS_UNSAFE_ADRS.is_empty(),
        );
        let count = counts
            .entry(package.map_or("(no package)", |p| p.name.as_str()))
            .or_default();
        count[0] += 1;
        count[1] += audit.unsafe_count;
        count[2] += audit.fs_count;
        problems.extend(audit.problems);
    }
    let mut summary = vec![format!(
        "{:<20} {:>5} {:>7} {:>8}",
        "crate", "files", "unsafe", "fs/os"
    )];
    for (name, [files, unsafes, fs_paths]) in &counts {
        summary.push(format!("{name:<20} {files:>5} {unsafes:>7} {fs_paths:>8}"));
    }
    Ok(Report { problems, summary })
}

/// The member whose directory holds `path`: the innermost, if members
/// nest.
fn package_of<'a>(path: &Path, members: &[&'a Package]) -> Option<&'a Package> {
    members
        .iter()
        .copied()
        .filter(|p| path.starts_with(p.dir()))
        .max_by_key(|p| p.dir().components().count())
}

/// The directories between `package`'s and the file at `path`, top first:
/// `["src", "sys"]` for `src/sys/unix.rs`.
fn dir_within(path: &Path, package: &Package) -> Vec<String> {
    let within = path.strip_prefix(package.dir()).unwrap_or(path);
    within
        .parent()
        .map(|dir| {
            dir.components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// A problem for each target whose root a manifest put where the audit
/// would judge it as another package's code, or as test code: outside its
/// package's directory or inside another member's, or, for a library,
/// binary or build script, in `tests/`.
fn targets_outside_their_place(members: &[&Package]) -> Vec<String> {
    let mut problems = Vec::new();
    for package in members {
        for target in &package.targets {
            let root = normalize(&target.src_path);
            let owner = package_of(&root, members).map(|p| p.name.as_str());
            let in_tests = root
                .strip_prefix(package.dir())
                .ok()
                .and_then(|within| within.components().next())
                .is_some_and(|c| c.as_os_str() == "tests");
            let wrong = if owner != Some(package.name.as_str()) {
                Some("outside its package's directory")
            } else if target.ships() && in_tests {
                Some("in tests/, though it ships")
            } else {
                None
            };
            if let Some(wrong) = wrong {
                problems.push(format!(
                    "{}: target `{}` has its root {} ({}): the audit judges each file by where it sits",
                    package.name,
                    target.name,
                    wrong,
                    root.display()
                ));
            }
        }
    }
    problems
}

/// Where the file at `path` sits in `package`.
fn place(path: &Path, package: Option<&Package>) -> Place {
    let Some(package) = package else {
        return Place::default();
    };
    let within = path.strip_prefix(package.dir()).unwrap_or(path);
    let in_tests = within
        .components()
        .next()
        .is_some_and(|c| c.as_os_str() == "tests");
    let os = package.name == OS;
    let lib_root = package.lib_root.as_deref() == Some(path);
    Place {
        os,
        os_root: os && lib_root,
        lib_root,
        test_binary: in_tests && within.components().count() == 2,
        fs_allowed: in_tests || FS_PACKAGES.contains(&package.name.as_str()),
    }
}

/// Audits one file. `path` is how messages name it; `dir` is the
/// directories between its package's and its own (`None` outside every
/// package).
fn audit(
    path: &str,
    source: &str,
    place: Place,
    dir: Option<&[String]>,
    os_unsafe_allowed: bool,
) -> Audit {
    let lexed = lexer::lex(source);
    let mut audit = Audit::default();
    let at = |i: usize| format!("{path}:{}", lexed.tokens.get(i).map_or(1, |t| t.line));
    let nesting = nesting(&lexed);
    let lowering = lowering_attributes(&lexed, &nesting);
    let allocator = if place.test_binary {
        counting_allocator(&lexed, &lowering, &at, &mut audit)
    } else {
        None
    };
    let safety = safety_comments(&lexed);
    // Where the last `unsafe` began: a `SAFETY:` comment before it belongs
    // to it, not to the next one.
    let mut previous: Option<usize> = None;
    for i in 0..lexed.tokens.len() {
        if !lexed.is_keyword(i, "unsafe") {
            continue;
        }
        audit.unsafe_count += 1;
        let after = previous.replace(lexed.tokens[i].start);
        let in_allocator = allocator
            .as_ref()
            .is_some_and(|a| (a.first..=a.last).contains(&i));
        if in_allocator || (place.os && os_unsafe_allowed) {
            if let Some(what) = needs_safety(&lexed, i)
                && !has_safety(&lexed, &safety, i, after)
            {
                audit.problems.push(format!(
                    "{}: `unsafe` {what} with no `// SAFETY:` comment ending in the {SAFETY_WINDOW} lines above it",
                    at(i)
                ));
            }
        } else if place.os {
            audit.problems.push(format!(
                "{}: `unsafe` in hardshell-os, which no accepted ADR allows yet (ADR-0013)",
                at(i)
            ));
        } else {
            audit.problems.push(format!(
                "{}: `unsafe` outside hardshell-os; the one exception is an ADR-0017 counting \
                 allocator in a crate's tests/",
                at(i)
            ));
        }
    }
    for attribute in &lowering {
        let on_allocator = allocator
            .as_ref()
            .is_some_and(|a| a.attributes.contains(&attribute.hash));
        let at_os_root =
            attribute.inner && attribute.nested == 0 && place.os_root && os_unsafe_allowed;
        if !(on_allocator || at_os_root) {
            audit.problems.push(format!(
                "{}: an attribute lowers the `unsafe_code` lint; only an ADR-0017 counting \
                 allocator's `unsafe impl` may, and hardshell-os's crate root once an ADR allows it",
                at(attribute.hash)
            ));
        }
    }
    if place.lib_root && !forbids_unsafe_in_doctests(&lexed, &nesting) {
        audit.problems.push(format!(
            "{path}:1: a library root without `{DOCTEST_FORBID}`: rustdoc compiles doc examples \
             without the workspace's lints, so only this keeps `unsafe` out of them"
        ));
    }
    for (i, problem) in module_paths(&lexed, dir) {
        audit.problems.push(format!("{}: {problem}", at(i)));
    }
    for (i, text, reach) in fs_paths(&lexed) {
        audit.fs_count += 1;
        if !place.fs_allowed {
            let why = match reach {
                Reach::Fs => "the engine's file access goes through the VFS",
                Reach::Os => "a whole OS module would hide its `fs` module from this audit",
                Reach::Hidden => "a glob or a rename of `std` would hide `std::fs` from this audit",
            };
            audit.problems.push(format!(
                "{}: `{text}` outside hardshell-os, xtask, the apps and tests: {why}",
                at(i)
            ));
        }
    }
    // In line order: each problem starts `path:line:`.
    audit.problems.sort_by_key(|problem| {
        let line = problem
            .get(path.len() + 1..)
            .and_then(|rest| rest.split(':').next());
        line.and_then(|line| line.parse::<usize>().ok())
            .unwrap_or(0)
    });
    audit
}

/// How many brackets of any kind, `(`, `[` or `{`, are open before each
/// token: an attribute at 0 is an item's or the crate's, not text inside
/// a block, a macro call or another attribute.
fn nesting(lexed: &Lexed<'_>) -> Vec<usize> {
    let mut depth = 0usize;
    lexed
        .tokens
        .iter()
        .map(|token| {
            let before = depth;
            match token.kind {
                Kind::Punct(b'(' | b'[' | b'{') => depth += 1,
                Kind::Punct(b')' | b']' | b'}') => depth = depth.saturating_sub(1),
                _ => {}
            }
            before
        })
        .collect()
}

/// Each attribute, `#[...]` or `#![...]`: the index of its `#`, of its
/// `[` and of its `]`.
fn attributes(lexed: &Lexed<'_>) -> Vec<(usize, usize, usize)> {
    (0..lexed.tokens.len())
        .filter(|&i| lexed.is_punct(i, b'#'))
        .filter_map(|i| {
            let open = if lexed.is_punct(i + 1, b'!') {
                i + 2
            } else {
                i + 1
            };
            if !lexed.is_punct(open, b'[') {
                return None;
            }
            lexed.closing(open).map(|close| (i, open, close))
        })
        .collect()
}

/// The word before each `(` around token `k`, innermost first, out to the
/// `[` at `open`: `forbid`, `attr`, `test`, `doc` for the `unsafe_code` in
/// `#![doc(test(attr(forbid(unsafe_code))))]`.
fn enclosing_words<'a>(lexed: &Lexed<'a>, open: usize, k: usize) -> Vec<&'a str> {
    let mut words = Vec::new();
    let mut depth = 0usize;
    for j in (open + 1..k).rev() {
        if lexed.is_punct(j, b')') {
            depth += 1;
        } else if lexed.is_punct(j, b'(') {
            if depth == 0 {
                words.push(lexed.text(j - 1));
            } else {
                depth -= 1;
            }
        }
    }
    words
}

/// An attribute that lowers the `unsafe_code` lint: `allow`, `expect` or
/// `warn` naming it, `cfg_attr` or not.
struct Lowering {
    /// Its `#`.
    hash: usize,
    /// `#![...]`.
    inner: bool,
    /// How many brackets are open around it.
    nested: usize,
}

fn lowering_attributes(lexed: &Lexed<'_>, nesting: &[usize]) -> Vec<Lowering> {
    attributes(lexed)
        .into_iter()
        .filter(|&(_, open, close)| {
            (open + 1..close).any(|k| {
                lexed.is_ident(k, "unsafe_code")
                    && enclosing_words(lexed, open, k)
                        .first()
                        .is_none_or(|word| !matches!(*word, "deny" | "forbid"))
            })
        })
        .map(|(hash, open, _)| Lowering {
            hash,
            inner: open == hash + 2,
            nested: nesting.get(hash).copied().unwrap_or(0),
        })
        .collect()
}

/// Whether the file holds `DOCTEST_FORBID` among its own inner attributes:
/// at the top level, outside every bracket, and exactly so (not in a
/// `cfg_attr`, which may be off).
fn forbids_unsafe_in_doctests(lexed: &Lexed<'_>, nesting: &[usize]) -> bool {
    attributes(lexed).into_iter().any(|(hash, open, close)| {
        open == hash + 2
            && nesting.get(hash) == Some(&0)
            && (open + 1..close).any(|k| {
                lexed.is_ident(k, "unsafe_code")
                    && enclosing_words(lexed, open, k) == ["forbid", "attr", "test", "doc"]
            })
    })
}

/// An ADR-0017 counting allocator: an `unsafe impl GlobalAlloc for ...`
/// whose own outer attributes lower `unsafe_code`.
struct Allocator {
    /// Its `unsafe`.
    first: usize,
    /// The brace that closes its body.
    last: usize,
    /// The `#` of each of its outer attributes.
    attributes: Vec<usize>,
}

fn counting_allocator(
    lexed: &Lexed<'_>,
    lowering: &[Lowering],
    at: &dyn Fn(usize) -> String,
    audit: &mut Audit,
) -> Option<Allocator> {
    let mut found: Option<Allocator> = None;
    for k in 0..lexed.tokens.len() {
        if !(lexed.is_keyword(k, "unsafe") && lexed.is_keyword(k + 1, "impl")) {
            continue;
        }
        let Some(body) = (k + 2..lexed.tokens.len()).find(|&j| lexed.is_punct(j, b'{')) else {
            continue;
        };
        if !(k + 2..body)
            .any(|j| lexed.is_keyword(j, "for") && lexed.is_ident(j - 1, "GlobalAlloc"))
        {
            continue;
        }
        let Some(last) = lexed.closing(body) else {
            continue;
        };
        let attributes = outer_attributes(lexed, k);
        if !attributes
            .iter()
            .any(|&a| lowering.iter().any(|l| l.hash == a))
        {
            continue;
        }
        if found.is_some() {
            audit.problems.push(format!(
                "{}: a second counting allocator; ADR-0017 allows one per test binary",
                at(k)
            ));
            continue;
        }
        found = Some(Allocator {
            first: k,
            last,
            attributes,
        });
    }
    found
}

/// The `#` of each outer attribute right before token `k`.
fn outer_attributes(lexed: &Lexed<'_>, k: usize) -> Vec<usize> {
    let mut found = Vec::new();
    let mut end = k;
    while end > 0 && lexed.is_punct(end - 1, b']') {
        let Some(open) = opening_bracket(lexed, end - 1) else {
            break;
        };
        if open == 0 || !lexed.is_punct(open - 1, b'#') {
            break;
        }
        found.push(open - 1);
        end = open - 1;
    }
    found
}

/// The `[` that the `]` at `close` closes.
fn opening_bracket(lexed: &Lexed<'_>, close: usize) -> Option<usize> {
    let mut depth = 0usize;
    for j in (0..=close).rev() {
        if lexed.is_punct(j, b']') {
            depth += 1;
        } else if lexed.is_punct(j, b'[') {
            depth -= 1;
            if depth == 0 {
                return Some(j);
            }
        }
    }
    None
}

/// What the `unsafe` at `i` is, if it is a kind that needs a `// SAFETY:`
/// comment: a block, an impl, an extern block or an unsafe attribute. An
/// `unsafe fn`, `unsafe trait` or `unsafe static` declares a contract,
/// which its docs state under `# Safety` instead.
fn needs_safety(lexed: &Lexed<'_>, i: usize) -> Option<&'static str> {
    let next = i + 1;
    if lexed.is_punct(next, b'{') {
        Some("block")
    } else if lexed.is_keyword(next, "impl") {
        Some("impl")
    } else if lexed.is_punct(next, b'(') {
        Some("attribute")
    } else if lexed.is_keyword(next, "extern") {
        let abi = lexed
            .tokens
            .get(next + 1)
            .is_some_and(|t| t.kind == Kind::Literal);
        let after = if abi { next + 2 } else { next + 1 };
        (!lexed.is_keyword(after, "fn")).then_some("extern block")
    } else if ["fn", "trait", "static"]
        .iter()
        .any(|word| lexed.is_keyword(next, word))
    {
        None
    } else {
        Some("keyword")
    }
}

/// Each comment that says `SAFETY:`, as where it ends and its last line.
/// Consecutive `//` comments, each on its own line, are one comment.
fn safety_comments(lexed: &Lexed<'_>) -> Vec<(usize, usize)> {
    let says = |text: &str| {
        text.lines().any(|line| {
            line.trim_start_matches(|c: char| c.is_whitespace() || c == '/' || c == '*' || c == '!')
                .starts_with("SAFETY:")
        })
    };
    let mut found = Vec::new();
    // The comment so far: where it ends, its last line, whether it says
    // `SAFETY:`, and whether the next line's comment can join it.
    let mut current: Option<(usize, usize, bool, bool)> = None;
    for comment in &lexed.comments {
        let said = says(lexed.comment_text(comment));
        let joinable = !comment.block && comment.own_line;
        match current.as_mut() {
            Some((end, last, says_safety, true)) if joinable && comment.first_line == *last + 1 => {
                *end = comment.end;
                *last = comment.last_line;
                *says_safety |= said;
            }
            _ => {
                if let Some((end, last, true, _)) = current {
                    found.push((end, last));
                }
                current = Some((comment.end, comment.last_line, said, joinable));
            }
        }
    }
    if let Some((end, last, true, _)) = current {
        found.push((end, last));
    }
    found
}

/// Whether a `SAFETY:` comment ends before the `unsafe` at `i`, no more
/// than `SAFETY_WINDOW` lines above it, and after the `unsafe` that began
/// at `after`, if any: each `unsafe` needs its own.
fn has_safety(
    lexed: &Lexed<'_>,
    safety: &[(usize, usize)],
    i: usize,
    after: Option<usize>,
) -> bool {
    lexed.tokens.get(i).is_some_and(|t| {
        safety.iter().any(|&(end, last)| {
            end <= t.start
                && last + SAFETY_WINDOW >= t.line
                && after.is_none_or(|start| end > start)
        })
    })
}

/// Each `#[path = "..."]` (in `cfg_attr` or not) and `include!` that could
/// bring in code the audit does not see where it is compiled: a path that
/// is computed, does not name a `.rs` file, or leads out of the includer's
/// part of its package. The index of its first token, and what is wrong.
fn module_paths(lexed: &Lexed<'_>, dir: Option<&[String]>) -> Vec<(usize, String)> {
    let literal_at = |j: usize| {
        lexed
            .tokens
            .get(j)
            .is_some_and(|t| t.kind == Kind::Literal)
            .then(|| lexed.text(j))
    };
    let mut found = Vec::new();
    let attributes = attributes(lexed);
    for i in 0..lexed.tokens.len() {
        let in_attribute = || {
            attributes
                .iter()
                .any(|&(_, open, close)| open < i && i < close)
        };
        if lexed.is_ident(i, "path") && lexed.is_punct(i + 1, b'=') && in_attribute() {
            if let Some(problem) = judge_module_path(literal_at(i + 2), dir) {
                found.push((i, format!("`#[path]` {problem}")));
            }
        } else if lexed.is_ident(i, "include") && lexed.is_punct(i + 1, b'!') {
            // `include!("x.rs")`: one literal, an optional comma, then the
            // closing bracket.
            let after = if lexed.is_punct(i + 4, b',') {
                i + 5
            } else {
                i + 4
            };
            let closes = b")]}".iter().any(|&c| lexed.is_punct(after, c));
            let literal = if closes { literal_at(i + 3) } else { None };
            if let Some(problem) = judge_module_path(literal, dir) {
                found.push((i, format!("`include!` {problem}")));
            }
        }
    }
    found
}

/// What is wrong with a module path, if anything, given the literal that
/// names it (`None` if it is computed) and the directories between the
/// package's and the including file's (`None` outside every package).
/// Rustc reads the path from the including file's directory; the file it
/// names must stay in the same top-level part of the package (`src/`,
/// `tests/`, ..., or the package's own directory), since the audit judges
/// each file by where it sits.
fn judge_module_path(literal: Option<&str>, dir: Option<&[String]>) -> Option<String> {
    let Some(path) = literal.and_then(string_value) else {
        return Some("names a computed or escaped path, which the audit cannot follow".to_owned());
    };
    if !path.ends_with(".rs") {
        return Some(format!(
            "names `{path}`, not a `.rs` file: the audit reads only `.rs` files"
        ));
    }
    let elsewhere = || {
        Some(format!(
            "names `{path}`, outside this file's part of its package: the audit judges each file \
             by where it sits"
        ))
    };
    let Some(dir) = dir else {
        return elsewhere();
    };
    if path.starts_with(['/', '\\']) || path.get(1..2) == Some(":") {
        return elsewhere();
    }
    let mut parts: Vec<&str> = dir.iter().map(String::as_str).collect();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return elsewhere();
                }
            }
            name => parts.push(name),
        }
    }
    // The top-level directory, or none for a file in the package's own.
    fn area<'a>(parts: &[&'a str]) -> Option<&'a str> {
        if parts.len() > 1 {
            parts.first().copied()
        } else {
            None
        }
    }
    let includer: Vec<&str> = dir.iter().map(String::as_str).chain(["file"]).collect();
    if area(&parts) == area(&includer) {
        None
    } else {
        elsewhere()
    }
}

/// The text a plain or raw string literal's token stands for; `None` for
/// any other literal, or one with an escape other than `\\`.
fn string_value(token: &str) -> Option<String> {
    if let Some(raw) = token.strip_prefix('r') {
        let hashes = raw.len() - raw.trim_start_matches('#').len();
        let fence = "#".repeat(hashes);
        let body = raw.get(hashes..)?.strip_suffix(fence.as_str())?;
        return Some(body.strip_prefix('"')?.strip_suffix('"')?.to_owned());
    }
    let body = token.strip_prefix('"')?.strip_suffix('"')?;
    let mut value = String::new();
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('\\') => value.push('\\'),
                _ => return None,
            },
            c => value.push(c),
        }
    }
    Some(value)
}

/// Why a path outside the engine's reach is one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reach {
    /// It names `std::fs`, or an OS's `fs` module (`std::os::unix::fs`).
    Fs,
    /// It brings in `std::os` or a whole OS module (`std::os::unix`,
    /// `std::os::unix::*`), which would hide its `fs` module.
    Os,
    /// It globs or renames `std` itself, which would hide either.
    Hidden,
}

/// Each path that reaches `std::fs` or could hide it: the index of its
/// `std`, its text, and how.
fn fs_paths(lexed: &Lexed<'_>) -> Vec<(usize, String, Reach)> {
    let text = |first: usize, last: usize| {
        let start = lexed.tokens.get(first).map_or(0, |t| t.start);
        let end = lexed.tokens.get(last).map_or(start, |t| t.end);
        let text = lexed.source.get(start..end).unwrap_or("std");
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(60)
            .collect::<String>()
    };
    let mut found = Vec::new();
    for i in 0..lexed.tokens.len() {
        if !lexed.is_ident(i, "std") {
            continue;
        }
        if lexed.is_keyword(i + 1, "as") {
            found.push((i, text(i, i + 2), Reach::Hidden));
            continue;
        }
        if !lexed.is_path_separator(i + 1) {
            continue;
        }
        let mut leaves = Vec::new();
        let last = use_tree(lexed, i + 3, &[], 0, &mut leaves);
        let reach = if leaves
            .iter()
            .any(|l| l.segments.is_empty() && (l.glob || l.renamed))
        {
            Some(Reach::Hidden)
        } else if leaves.iter().any(|l| l.segments.contains(&"fs")) {
            Some(Reach::Fs)
        } else if leaves
            .iter()
            .any(|l| l.segments.first() == Some(&"os") && l.segments.len() <= 2)
        {
            Some(Reach::Os)
        } else {
            None
        };
        if let Some(reach) = reach {
            found.push((i, text(i, last), reach));
        }
    }
    found
}

/// A path that a `use` tree, or an expression's path, names after `std::`.
struct Leaf<'a> {
    /// Its segments: `["io", "Write"]` for `std::io::Write`.
    segments: Vec<&'a str>,
    /// It ends in `*`.
    glob: bool,
    /// It is renamed (`as`).
    renamed: bool,
}

/// How deep `use_tree` reads nested groups and segments before it counts
/// a tree as hiding what it names.
const MAX_TREE_DEPTH: usize = 64;

/// Adds the paths that the tree starting at token `j` names under
/// `prefix` to `leaves`, and returns the index of its last token.
fn use_tree<'a>(
    lexed: &Lexed<'a>,
    j: usize,
    prefix: &[&'a str],
    depth: usize,
    leaves: &mut Vec<Leaf<'a>>,
) -> usize {
    let is_name = |j: usize| {
        lexed
            .tokens
            .get(j)
            .is_some_and(|t| matches!(t.kind, Kind::Ident { .. }))
    };
    if depth > MAX_TREE_DEPTH {
        leaves.push(Leaf {
            segments: Vec::new(),
            glob: true,
            renamed: false,
        });
        return j;
    }
    if lexed.is_punct(j, b'*') {
        leaves.push(Leaf {
            segments: prefix.to_vec(),
            glob: true,
            renamed: false,
        });
        return j;
    }
    if lexed.is_punct(j, b'{') {
        let mut k = j + 1;
        while k < lexed.tokens.len() {
            if lexed.is_punct(k, b'}') {
                return k;
            }
            let end = use_tree(lexed, k, prefix, depth + 1, leaves);
            k = end + 1;
            if lexed.is_punct(k, b',') {
                k += 1;
            } else if lexed.is_punct(k, b'}') {
                return k;
            } else {
                return end;
            }
        }
        return lexed.tokens.len().saturating_sub(1);
    }
    if lexed.is_keyword(j, "self") || is_name(j) {
        let mut segments = prefix.to_vec();
        if !lexed.is_keyword(j, "self") {
            segments.push(lexed.text(j));
            let continues = lexed.is_path_separator(j + 1)
                && (is_name(j + 3) || lexed.is_punct(j + 3, b'{') || lexed.is_punct(j + 3, b'*'));
            if continues {
                return use_tree(lexed, j + 3, &segments, depth + 1, leaves);
            }
        }
        let renamed = lexed.is_keyword(j + 1, "as");
        leaves.push(Leaf {
            segments,
            glob: false,
            renamed,
        });
        return if renamed { j + 2 } else { j };
    }
    j.saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::{
        Audit, DOCTEST_FORBID, Place, audit, dir_within, package_of, place,
        targets_outside_their_place,
    };
    use crate::metadata::tests::package;
    use crate::metadata::{Package, Target};
    use std::path::Path;
    use std::path::PathBuf;

    /// A file in a library's `src/`.
    const LIB: Place = Place {
        os: false,
        os_root: false,
        lib_root: false,
        test_binary: false,
        fs_allowed: false,
    };
    const LIB_ROOT: Place = Place {
        lib_root: true,
        ..LIB
    };
    const TEST_BINARY: Place = Place {
        test_binary: true,
        fs_allowed: true,
        ..LIB
    };
    const OS_ROOT: Place = Place {
        os: true,
        os_root: true,
        lib_root: true,
        fs_allowed: true,
        ..LIB
    };
    const OS_FILE: Place = Place {
        os: true,
        fs_allowed: true,
        ..LIB
    };

    /// The directory of a file in a package's `src/`.
    fn src() -> Vec<String> {
        vec!["src".to_owned()]
    }

    fn run(source: &str, place: Place) -> Audit {
        audit("f.rs", source, place, Some(&src()), false)
    }

    fn problems(source: &str, place: Place, os_unsafe_allowed: bool) -> Vec<String> {
        audit("f.rs", source, place, Some(&src()), os_unsafe_allowed).problems
    }

    /// The problems of a file in the package directory `dir` (`None`
    /// outside every package).
    fn problems_in(source: &str, dir: Option<&[&str]>) -> Vec<String> {
        let dir: Option<Vec<String>> = dir.map(|d| d.iter().map(|s| (*s).to_owned()).collect());
        audit("f.rs", source, LIB, dir.as_deref(), false).problems
    }

    /// The message for `unsafe` outside `hardshell-os`, after its location.
    const OUTSIDE: &str = ": `unsafe` outside hardshell-os; the one exception is an ADR-0017 \
                           counting allocator in a crate's tests/";

    /// The shape ADR-0017 allows, as in `crates/core/tests/log_alloc.rs`.
    const ALLOCATOR: &str = "\
use std::alloc::{GlobalAlloc, Layout, System};
struct Counting;
#[global_allocator]
static COUNTING: Counting = Counting;
#[allow(unsafe_code)] // ADR-0017
// SAFETY: forwards every call to System.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller's contract is System's.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller's contract is System's.
        unsafe { System.dealloc(ptr, layout) }
    }
}
";

    #[test]
    fn unsafe_in_comments_strings_and_raw_identifiers_is_not_code() {
        let source = r##"
            // unsafe { }
            /* unsafe impl */ /// unsafe prose, `unsafe { }` too
            let r#unsafe = "unsafe { }"; let s = r#"unsafe"#; let c = 'u';
            #[allow(dead_code)] // allow(unsafe_code)
            fn f() {}
        "##;
        let audit = run(source, LIB);
        assert_eq!(audit.problems, Vec::<String>::new());
        assert_eq!(audit.unsafe_count, 0);
    }

    #[test]
    fn unsafe_outside_hardshell_os_fails() {
        for source in [
            "fn f() { unsafe { g() } }",
            "unsafe fn f() {}",
            "unsafe impl Send for X {}",
            "unsafe extern \"C\" { fn g(); }",
            "#[unsafe(no_mangle)] fn f() {}",
            "fn f() { unsafe\u{2028}{ g() } }",
            "macro_rules! m { () => { unsafe { g() } } }",
        ] {
            for place in [LIB, TEST_BINARY] {
                let found = problems(source, place, false);
                assert_eq!(found.len(), 1, "{source:?}: {found:?}");
                assert!(
                    found[0].starts_with("f.rs:1: `unsafe` outside hardshell-os"),
                    "{found:?}"
                );
            }
        }
    }

    #[test]
    fn a_shebang_hides_nothing() {
        // To rustc `#!/**/[...]` is an inner attribute, not a shebang line.
        let source = "#!/**/[allow(unsafe_code)]\nfn f() { unsafe { g() } }\n";
        let found = problems(source, LIB, false);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(
            found[0].starts_with("f.rs:1: an attribute lowers"),
            "{found:?}"
        );
        assert!(
            found[1].starts_with("f.rs:2: `unsafe` outside"),
            "{found:?}"
        );
        let shebang = "#!/usr/bin/env run-cargo-script\nfn f() { unsafe { g() } }\n";
        assert_eq!(problems(shebang, LIB, false).len(), 1);
    }

    #[test]
    fn the_counting_allocator_passes_only_in_a_test_binary() {
        let audit = run(ALLOCATOR, TEST_BINARY);
        assert_eq!(audit.problems, Vec::<String>::new());
        assert_eq!(audit.unsafe_count, 5);
        // In `src/` (shipped code, or its unit tests) every part of it fails.
        let found = problems(ALLOCATOR, LIB, false);
        assert_eq!(found.len(), 6, "{found:?}");
        assert!(
            found[0].starts_with("f.rs:5: an attribute lowers the `unsafe_code` lint"),
            "in line order: {found:?}"
        );
        let lines: Vec<&str> = found[1..]
            .iter()
            .map(|p| p.strip_suffix(OUTSIDE).unwrap_or(p))
            .collect();
        assert_eq!(lines, ["f.rs:7", "f.rs:8", "f.rs:10", "f.rs:13", "f.rs:15"]);
    }

    #[test]
    fn log_alloc_passes() {
        let source = include_str!("../../crates/core/tests/log_alloc.rs");
        let audit = run(source, TEST_BINARY);
        assert_eq!(audit.problems, Vec::<String>::new());
        assert_eq!(
            audit.unsafe_count, 9,
            "an impl, four methods and four blocks"
        );
    }

    #[test]
    fn the_allocator_needs_each_safety_comment() {
        let without_block_comment = ALLOCATOR.replacen(
            "        // SAFETY: the caller's contract is System's.\n",
            "",
            1,
        );
        assert_eq!(
            problems(&without_block_comment, TEST_BINARY, false),
            ["f.rs:9: `unsafe` block with no `// SAFETY:` comment ending in the 3 lines above it"]
        );
        let without_impl_comment =
            ALLOCATOR.replace("// SAFETY: forwards every call to System.\n", "");
        assert_eq!(
            problems(&without_impl_comment, TEST_BINARY, false),
            ["f.rs:6: `unsafe` impl with no `// SAFETY:` comment ending in the 3 lines above it"]
        );
        // A comment that is not `SAFETY:`, or does not start with it.
        for wrong in ["// Safety: forwards", "// TODO: write the SAFETY: forwards"] {
            let wrong = ALLOCATOR.replace("// SAFETY: forwards", wrong);
            assert_eq!(problems(&wrong, TEST_BINARY, false).len(), 1, "{wrong}");
        }
        // One too far above, or just near enough.
        let far = ALLOCATOR.replace(
            "// SAFETY: forwards every call to System.\n",
            "// SAFETY: forwards every call to System.\n#[doc = \"a\"]\n#[doc = \"b\"]\n#[doc = \"c\"]\n",
        );
        assert_eq!(
            problems(&far, TEST_BINARY, false).len(),
            1,
            "four lines above"
        );
        let near = ALLOCATOR.replace(
            "// SAFETY: forwards every call to System.\n",
            "// SAFETY: forwards every call to System.\n#[doc = \"a\"]\n#[doc = \"b\"]\n",
        );
        assert_eq!(
            problems(&near, TEST_BINARY, false),
            Vec::<String>::new(),
            "three lines above"
        );
        // A multi-line `SAFETY:` comment counts from its last line.
        let long = ALLOCATOR.replace(
            "// SAFETY: forwards every call to System.\n",
            "// SAFETY: forwards every call to System,\n// one line\n// after\n// another.\n",
        );
        assert_eq!(problems(&long, TEST_BINARY, false), Vec::<String>::new());
        // A trailing `SAFETY:` comment does not join the comments below it.
        let trailing = ALLOCATOR.replace(
            "#[allow(unsafe_code)] // ADR-0017\n// SAFETY: forwards every call to System.\n",
            "#[allow(unsafe_code)] // SAFETY: forwards every call to System.\n// one\n// two\n// three\n",
        );
        assert_eq!(
            problems(&trailing, TEST_BINARY, false),
            ["f.rs:9: `unsafe` impl with no `// SAFETY:` comment ending in the 3 lines above it"]
        );
    }

    #[test]
    fn the_allocator_shape_is_exact() {
        // A file-wide allow.
        let file_wide = format!("#![allow(unsafe_code)]\n{ALLOCATOR}");
        let found = problems(&file_wide, TEST_BINARY, false);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with("f.rs:1: an attribute lowers"),
            "{found:?}"
        );
        // `unsafe` outside the impl.
        let outside =
            format!("{ALLOCATOR}fn f() {{\n    // SAFETY: none\n    unsafe {{ g() }}\n}}\n");
        let found = problems(&outside, TEST_BINARY, false);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with("f.rs:20: `unsafe` outside hardshell-os"),
            "{found:?}"
        );
        // Not `GlobalAlloc`.
        let other_trait = ALLOCATOR.replace("unsafe impl GlobalAlloc for", "unsafe impl Send for");
        assert_eq!(problems(&other_trait, TEST_BINARY, false).len(), 6);
        // Without its own allow.
        let no_allow = ALLOCATOR.replace("#[allow(unsafe_code)] // ADR-0017\n", "");
        assert_eq!(problems(&no_allow, TEST_BINARY, false).len(), 5);
        // In a module of a test binary rather than the binary's own file.
        assert_eq!(
            problems(
                ALLOCATOR,
                Place {
                    test_binary: false,
                    ..TEST_BINARY
                },
                false
            )
            .len(),
            6
        );
        // Two allocators.
        let two = format!(
            "{ALLOCATOR}{}",
            ALLOCATOR
                .replace("Counting", "Other")
                .replace("use std", "use core")
        );
        let found = problems(&two, TEST_BINARY, false);
        assert!(
            found
                .iter()
                .any(|p| p.contains("a second counting allocator")),
            "{found:?}"
        );
        assert!(
            found
                .iter()
                .any(|p| p.contains("`unsafe` outside hardshell-os")),
            "{found:?}"
        );
    }

    #[test]
    fn lowering_the_lint_fails_however_it_is_spelled() {
        for source in [
            "#![allow(unsafe_code)]",
            "#![allow(dead_code, unsafe_code)]",
            "#![expect(unsafe_code)]",
            "#![warn(unsafe_code)]",
            "#![cfg_attr(test, allow(unsafe_code))]",
            "#[allow(unsafe_code)]\nfn f() {}",
            "#[allow(r#unsafe_code)]\nfn f() {}",
        ] {
            let found = problems(source, LIB, false);
            assert_eq!(found.len(), 1, "{source}: {found:?}");
            assert!(
                found[0].contains("lowers the `unsafe_code` lint"),
                "{found:?}"
            );
        }
        for source in [
            "#![deny(unsafe_code)]",
            "#![forbid(unsafe_code)]",
            "#![cfg_attr(x, deny(unsafe_code))]",
            DOCTEST_FORBID,
        ] {
            assert_eq!(
                problems(source, LIB, false),
                Vec::<String>::new(),
                "{source}"
            );
        }
    }

    #[test]
    fn hardshell_os_needs_an_adr_before_any_unsafe() {
        let root = format!(
            "#![allow(unsafe_code)]\n{DOCTEST_FORBID}\n// SAFETY: the fd is open.\nfn f() {{ unsafe {{ g() }} }}\n"
        );
        let found = problems(&root, OS_ROOT, false);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(
            found[0].starts_with("f.rs:1: an attribute lowers"),
            "{found:?}"
        );
        assert!(
            found[1]
                .starts_with("f.rs:4: `unsafe` in hardshell-os, which no accepted ADR allows yet"),
            "{found:?}"
        );
        // Once an ADR allows it: the root may allow, and `unsafe` needs `SAFETY:`.
        assert_eq!(problems(&root, OS_ROOT, true), Vec::<String>::new());
        assert_eq!(
            problems("fn f() { unsafe { g() } }", OS_FILE, true).len(),
            1
        );
        assert_eq!(
            problems(&root, OS_FILE, true).len(),
            1,
            "an allow away from the crate root"
        );
        let in_module = format!("{DOCTEST_FORBID}\nmod m {{ #![allow(unsafe_code)] }}");
        assert_eq!(
            problems(&in_module, OS_ROOT, true).len(),
            1,
            "an allow in a module"
        );
        let on_item = format!("{DOCTEST_FORBID}\n#[allow(unsafe_code)]\nfn f() {{}}");
        assert_eq!(problems(&on_item, OS_ROOT, true).len(), 1, "on an item");
        // Declarations need `# Safety` docs, not a comment.
        let declarations = "pub unsafe fn f() {}\npub unsafe trait T {}\n\
                            type P = unsafe extern \"C\" fn();\npub unsafe extern fn g() {}\n";
        assert_eq!(problems(declarations, OS_FILE, true), Vec::<String>::new());
        for needs in [
            "unsafe extern \"C\" { fn g(); }",
            "unsafe extern { fn g(); }",
            "#[unsafe(no_mangle)]\nfn f() {}",
            "unsafe impl Send for X {}",
            "macro_rules! m { ($b:block) => { unsafe $b } }",
        ] {
            assert_eq!(problems(needs, OS_FILE, true).len(), 1, "{needs}");
        }
    }

    #[test]
    fn every_library_root_forbids_unsafe_in_its_doctests() {
        let found = problems("//! A crate.\npub fn f() {}\n", LIB_ROOT, false);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with(
                "f.rs:1: a library root without `#![doc(test(attr(forbid(unsafe_code))))]`"
            ),
            "{found:?}"
        );
        for passes in [
            format!("//! A crate.\n\n{DOCTEST_FORBID}\n\npub fn f() {{}}\n"),
            "#![doc(test(attr(forbid(unsafe_code), deny(warnings))))]".to_owned(),
            "#![doc(test(attr(deny(warnings), forbid(unsafe_code))))]".to_owned(),
            // After an inner attribute that holds brackets of its own.
            format!("#![doc = concat!{{\"a\", \"b\"}}]\n{DOCTEST_FORBID}\n"),
        ] {
            assert_eq!(
                problems(&passes, LIB_ROOT, false),
                Vec::<String>::new(),
                "{passes}"
            );
        }
        for fails in [
            "#![doc(test(attr(deny(unsafe_code))))]",
            "#![doc(test(attr(allow(unsafe_code))))]",
            "#![forbid(unsafe_code)]",
            "#[doc(test(attr(forbid(unsafe_code))))]\nfn f() {}",
            "mod m { #![doc(test(attr(forbid(unsafe_code))))] }",
            "// #![doc(test(attr(forbid(unsafe_code))))]",
            // Text that sets nothing, or an attribute that may be off.
            "const _: &str = stringify!(#![doc(test(attr(forbid(unsafe_code))))]);",
            "m! { #![doc(test(attr(forbid(unsafe_code))))] }",
            "#![cfg_attr(any(), doc(test(attr(forbid(unsafe_code)))))]",
        ] {
            let found = problems(fails, LIB_ROOT, false);
            assert!(
                found.iter().any(|p| p.contains("a library root without")),
                "{fails}: {found:?}"
            );
        }
        // Only library roots need it.
        assert_eq!(problems("pub fn f() {}", LIB, false), Vec::<String>::new());
        // Every library root in the workspace has it.
        for source in [
            include_str!("../../crates/core/src/lib.rs"),
            include_str!("../../crates/os/src/lib.rs"),
            include_str!("../../crates/hardshelldb/src/lib.rs"),
            include_str!("../../bench/src/lib.rs"),
        ] {
            assert_eq!(problems(source, LIB_ROOT, false), Vec::<String>::new());
        }
    }

    #[test]
    fn modules_and_includes_stay_in_their_part_of_the_package() {
        const SRC: Option<&[&str]> = Some(&["src"]);
        const SRC_SYS: Option<&[&str]> = Some(&["src", "sys"]);
        const ROOT: Option<&[&str]> = Some(&[]);
        for (source, dir) in [
            ("#[path = \"sys/unix.rs\"]\nmod sys;", SRC),
            ("#[cfg_attr(unix, path = \"sys/unix.rs\")]\nmod sys;", SRC),
            ("#[ path  =  \"sys/unix.rs\" ]\nmod sys;", SRC),
            ("#[path = \"../x.rs\"]\nmod x;", SRC_SYS),
            ("#[path = \"sub/../x.rs\"]\nmod x;", SRC),
            ("#[path = \"./x.rs\"]\nmod x;", SRC),
            ("#[path = r\"sys\\windows.rs\"]\nmod sys;", SRC),
            ("#[path = r#\"sys/unix.rs\"#]\nmod sys;", SRC),
            ("#[path = \"sys\\\\windows.rs\"]\nmod sys;", SRC),
            ("include!(\"table.rs\");", SRC),
            ("include![\"table.rs\"];", SRC),
            ("include!(\"table.rs\",);", SRC),
            ("include!(\"helpers.rs\");", ROOT),
            ("let path = \"notes.txt\";", SRC),
            ("const X: &str = include_str!(\"../../data.txt\");", SRC),
        ] {
            assert_eq!(problems_in(source, dir), Vec::<String>::new(), "{source}");
        }
        for (source, dir, says) in [
            (
                "#[path = \"evil.txt\"]\nmod evil;",
                SRC,
                "`#[path]` names `evil.txt`, not a `.rs` file",
            ),
            (
                "#[cfg_attr(unix, path = \"a.txt\")]\nmod a;",
                SRC,
                "not a `.rs` file",
            ),
            // Out of the package, or into another part of it.
            (
                "#[path = \"../../xtask/src/x.rs\"]\nmod x;",
                SRC,
                "outside this file's part",
            ),
            (
                "#[path = \"../tests/alloc.rs\"]\nmod alloc;",
                SRC,
                "outside this file's part",
            ),
            (
                "#[path = \"./../../x.rs\"]\nmod x;",
                SRC,
                "outside this file's part",
            ),
            (
                "#[path = \"sub/../../../x.rs\"]\nmod x;",
                SRC,
                "outside this file's part",
            ),
            (
                "#[path = \"../x.rs\"]\nmod x;",
                SRC,
                "outside this file's part",
            ),
            (
                "#[path = \"../../../x.rs\"]\nmod x;",
                SRC_SYS,
                "outside this file's part",
            ),
            (
                "#[path = \"src/x.rs\"]\nmod x;",
                ROOT,
                "outside this file's part",
            ),
            (
                "#[path = \"/tmp/x.rs\"]\nmod x;",
                SRC,
                "outside this file's part",
            ),
            (
                "#[path = \"C:\\\\x.rs\"]\nmod x;",
                SRC,
                "outside this file's part",
            ),
            (
                "#[path = \"sys/unix.rs\"]\nmod sys;",
                None,
                "outside this file's part",
            ),
            // Escapes rustc reads and the audit does not.
            (
                "#[path = \"../\\x2e./os/src/x.rs\"]\nmod x;",
                SRC,
                "computed or escaped path",
            ),
            (
                "#[path = \"..\\u{2f}x.rs\"]\nmod x;",
                SRC,
                "computed or escaped path",
            ),
            ("include!(\"gen.txt\");", SRC, "`include!` names `gen.txt`"),
            ("include!(\"../../x.rs\");", SRC, "outside this file's part"),
            (
                "include!(concat!(env!(\"OUT_DIR\"), \"/gen.rs\"));",
                SRC,
                "computed or escaped path",
            ),
            (
                "include!(\"a.rs\", \"b.rs\");",
                SRC,
                "computed or escaped path",
            ),
        ] {
            let found = problems_in(source, dir);
            assert_eq!(found.len(), 1, "{source}: {found:?}");
            assert!(found[0].contains(says), "{source}: {found:?}");
        }
        // Tests may not reach out either: the rule keeps files where the
        // audit judges them.
        assert_eq!(
            problems("#[path = \"x.txt\"]\nmod x;", TEST_BINARY, false).len(),
            1
        );
    }

    #[test]
    fn every_target_has_its_root_in_its_own_place() {
        let target = |name: &str, kind: &str, path: &str| Target {
            name: name.to_owned(),
            kinds: vec![kind.to_owned()],
            src_path: PathBuf::from(path),
        };
        let with = |name: &str, dir: &str, targets: Vec<Target>| Package {
            targets,
            ..package(name, dir, &[])
        };
        let os = package("hardshell-os", "crates/os", &[]);
        let fine = with(
            "hardshell-types",
            "crates/types",
            vec![
                target("hardshell_types", "lib", "/ws/crates/types/src/lib.rs"),
                target("t", "test", "/ws/crates/types/tests/t.rs"),
                target(
                    "build-script-build",
                    "custom-build",
                    "/ws/crates/types/build.rs",
                ),
                target("e", "example", "/ws/crates/types/src/../examples/e.rs"),
            ],
        );
        assert_eq!(
            targets_outside_their_place(&[&os, &fine]),
            Vec::<String>::new()
        );
        let wrong = with(
            "hardshell-types",
            "crates/types",
            vec![
                target("into_os", "lib", "/ws/crates/types/../os/src/types_lib.rs"),
                target("outside", "bin", "/elsewhere/main.rs"),
                target("lib_in_tests", "lib", "/ws/crates/types/tests/lib.rs"),
                target(
                    "build_in_tests",
                    "custom-build",
                    "/ws/crates/types/tests/build.rs",
                ),
                target("bench", "bench", "/ws/crates/types/benches/b.rs"),
            ],
        );
        let found = targets_outside_their_place(&[&os, &wrong]);
        assert_eq!(found.len(), 4, "{found:?}");
        assert!(found[0].starts_with("hardshell-types: target `into_os` has its root outside its package's directory (/ws/crates/os/src/types_lib.rs)"), "{found:?}");
        assert!(
            found[1].contains("`outside` has its root outside"),
            "{found:?}"
        );
        assert!(
            found[2].contains("`lib_in_tests` has its root in tests/, though it ships"),
            "{found:?}"
        );
        assert!(
            found[3].contains("`build_in_tests` has its root in tests/"),
            "{found:?}"
        );
    }

    #[test]
    fn std_fs_fails_in_the_engine() {
        for source in [
            "use std::fs;",
            "fn f() { std::fs::read(\"x\"); }",
            "use std::{io, fs::File};",
            "use ::std::fs as files;",
            "use std::os::unix::fs::FileExt;",
            "use std::{io::{self, Read}, os::windows::fs::OpenOptionsExt};",
            "use r#std::fs;",
            "let f = std :: fs :: File :: open(p);",
            // A glob or a rename of `std` would hide it.
            "use std::*;",
            "use std::{*};",
            "use std::{io, *};",
            "use std as s;",
            "extern crate std as s;",
            "use std::{self as s, io};",
            "use std::{{*}};",
            "use std::{io, {*}};",
            "use std::{{self as s}};",
            "use std::{io, {{self as standard}}};",
            // A whole OS module would hide its `fs` module.
            "use std::os::unix;",
            "use std::os;",
            "use std::os::*;",
            "use std::{os::unix};",
            "use std::{io, {os}};",
            "use std::os::unix::{self};",
            "use std::os::{unix::*};",
        ] {
            let found = problems(source, LIB, false);
            assert_eq!(found.len(), 1, "{source}: {found:?}");
            assert!(
                found[0].contains("outside hardshell-os, xtask, the apps and tests"),
                "{found:?}"
            );
            assert_eq!(problems(source, TEST_BINARY, false), Vec::<String>::new());
            assert_eq!(problems(source, OS_FILE, false), Vec::<String>::new());
            assert_eq!(run(source, OS_FILE).fs_count, 1);
        }
        for source in [
            "use std::io::{self, Write};",
            "use std::io::*;",
            "use std::{io::*, fmt};",
            "use std::{io as fs};",
            "use std::io::{self as stdio, Write};",
            "use std::fmt::{self as f};",
            "use std::io::{Read, *};",
            "use std::collections::{*};",
            "use std::{io::{self as stdio}, fmt};",
            "use std::{collections::{*}};",
            "use x::os;",
            // A named item of an OS module is no `fs`.
            "use std::os::unix::ffi::OsStrExt;",
            "let fd = std::os::fd::RawFd::default();",
            "use std::os::{fd::AsRawFd, raw::c_int};",
            "// std::fs",
            "let s = \"std::fs\";",
            "use std::io as fs;",
            "use x::fs;",
        ] {
            assert_eq!(
                problems(source, LIB, false),
                Vec::<String>::new(),
                "{source}"
            );
        }
        assert_eq!(
            problems("use std::fs;", LIB, false),
            [
                "f.rs:1: `std::fs` outside hardshell-os, xtask, the apps and tests: the engine's file access goes through the VFS"
            ]
        );
        assert_eq!(
            problems("use std::*;", LIB, false),
            [
                "f.rs:1: `std::*` outside hardshell-os, xtask, the apps and tests: a glob or a rename of `std` would hide `std::fs` from this audit"
            ]
        );
        assert_eq!(
            problems("use std::os::unix;", LIB, false),
            [
                "f.rs:1: `std::os::unix` outside hardshell-os, xtask, the apps and tests: a whole OS module would hide its `fs` module from this audit"
            ]
        );
    }

    #[test]
    fn deeply_nested_use_trees_end_and_count_as_hiding() {
        let deep = format!("use std::{}io{};", "{".repeat(100_000), "}".repeat(100_000));
        let found = problems(&deep, LIB, false);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("a glob or a rename of `std`"),
            "{found:?}"
        );
        let _ = problems(&"std::{".repeat(10_000), LIB, false);
    }

    #[test]
    fn places_follow_the_package_layout() {
        let os = package("hardshell-os", "crates/os", &[]);
        let core = package("hardshell-core", "crates/core", &[]);
        let xtask = package("xtask", "xtask", &[]);
        let at = |path: &str, package| place(Path::new(path), Some(package));
        assert_eq!(at("/ws/crates/os/src/lib.rs", &os), OS_ROOT);
        assert_eq!(at("/ws/crates/os/src/sys/unix.rs", &os), OS_FILE);
        assert_eq!(
            at("/ws/crates/os/tests/vfs.rs", &os),
            Place {
                test_binary: true,
                ..OS_FILE
            }
        );
        assert_eq!(at("/ws/crates/core/src/lib.rs", &core), LIB_ROOT);
        assert_eq!(at("/ws/crates/core/src/log.rs", &core), LIB);
        assert_eq!(
            at("/ws/crates/core/src/tests/helpers.rs", &core),
            LIB,
            "only a `tests/` at the package's top is tests"
        );
        assert_eq!(at("/ws/crates/core/tests/log_alloc.rs", &core), TEST_BINARY);
        assert_eq!(
            at("/ws/crates/core/tests/common/mod.rs", &core),
            Place {
                test_binary: false,
                ..TEST_BINARY
            }
        );
        assert_eq!(at("/ws/crates/core/build.rs", &core), LIB);
        assert_eq!(
            at("/ws/xtask/src/main.rs", &xtask),
            Place {
                fs_allowed: true,
                ..LIB
            }
        );
        assert_eq!(place(Path::new("/ws/stray.rs"), None), Place::default());
    }

    #[test]
    fn a_file_knows_its_directory_in_its_package() {
        let core = package("hardshell-core", "crates/core", &[]);
        let dir = |path: &str| dir_within(Path::new(path), &core);
        assert_eq!(dir("/ws/crates/core/src/lib.rs"), ["src"]);
        assert_eq!(dir("/ws/crates/core/src/sys/unix.rs"), ["src", "sys"]);
        assert_eq!(dir("/ws/crates/core/build.rs"), Vec::<String>::new());
    }

    #[test]
    fn a_file_belongs_to_the_innermost_package_holding_it() {
        let outer = package("outer", "crates/outer", &[]);
        let inner = package("inner", "crates/outer/inner", &[]);
        let other = package("other", "crates/other", &[]);
        let members = [&outer, &inner, &other];
        let name = |path: &str| package_of(Path::new(path), &members).map(|p| p.name.as_str());
        assert_eq!(name("/ws/crates/outer/src/lib.rs"), Some("outer"));
        assert_eq!(name("/ws/crates/outer/inner/src/lib.rs"), Some("inner"));
        assert_eq!(name("/ws/crates/other/src/lib.rs"), Some("other"));
        assert_eq!(
            name("/ws/crates/otherwise/src/lib.rs"),
            None,
            "by component, not by text"
        );
        assert_eq!(name("/ws/stray.rs"), None);
    }
}
