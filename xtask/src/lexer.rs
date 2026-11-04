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

//! A Rust lexer for the source audits. It finds tokens and comments, and
//! it never fails or panics: any text gives some tokens, valid Rust or not.
//!
//! It knows only what the audits need: where comments and literals begin
//! and end, so an `unsafe` inside one is not code; raw identifiers, so
//! `r#unsafe` is not the keyword; Rust's whitespace, so `unsafe` is found
//! however it is spaced; the shebang rule; and lines.

/// What a token is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// An identifier or keyword. `raw` for `r#name`, whose text leaves out
    /// the `r#`.
    Ident { raw: bool },
    /// A lifetime or loop label, such as `'a`.
    Lifetime,
    /// A string, character, byte or number literal.
    Literal,
    /// One byte of punctuation: `::` is two `:` tokens.
    Punct(u8),
}

/// A token: its kind, its bytes in the source, and its line (from 1).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Token {
    pub(crate) kind: Kind,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) line: usize,
}

/// A comment, from its `//` or `/*` to the end of its line or its `*/`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Comment {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) first_line: usize,
    pub(crate) last_line: usize,
    /// A `/* */` comment rather than a `//` one.
    pub(crate) block: bool,
    /// Nothing but whitespace before it on its line.
    pub(crate) own_line: bool,
}

/// A source's tokens and comments, in order.
pub(crate) struct Lexed<'a> {
    pub(crate) source: &'a str,
    pub(crate) tokens: Vec<Token>,
    pub(crate) comments: Vec<Comment>,
}

impl<'a> Lexed<'a> {
    /// The text of token `i`, or `""` past the end.
    pub(crate) fn text(&self, i: usize) -> &'a str {
        let source: &'a str = self.source;
        self.tokens
            .get(i)
            .and_then(|t| source.get(t.start..t.end))
            .unwrap_or("")
    }

    /// Whether token `i` is the keyword `word`: spelled so, and not raw.
    pub(crate) fn is_keyword(&self, i: usize, word: &str) -> bool {
        self.tokens
            .get(i)
            .is_some_and(|t| t.kind == Kind::Ident { raw: false })
            && self.text(i) == word
    }

    /// Whether token `i` is an identifier named `name`, raw or not:
    /// `r#std` is `std`.
    pub(crate) fn is_ident(&self, i: usize, name: &str) -> bool {
        self.tokens
            .get(i)
            .is_some_and(|t| matches!(t.kind, Kind::Ident { .. }))
            && self.text(i) == name
    }

    pub(crate) fn is_punct(&self, i: usize, byte: u8) -> bool {
        self.tokens
            .get(i)
            .is_some_and(|t| t.kind == Kind::Punct(byte))
    }

    /// Whether tokens `i` and `i + 1` are `::`.
    pub(crate) fn is_path_separator(&self, i: usize) -> bool {
        self.is_punct(i, b':') && self.is_punct(i + 1, b':')
    }

    /// The token that closes the `(`, `[` or `{` at `open`, counting only
    /// brackets of that kind.
    pub(crate) fn closing(&self, open: usize) -> Option<usize> {
        let (opening, close) = match self.tokens.get(open)?.kind {
            Kind::Punct(b'(') => (b'(', b')'),
            Kind::Punct(b'[') => (b'[', b']'),
            Kind::Punct(b'{') => (b'{', b'}'),
            _ => return None,
        };
        let mut depth = 0usize;
        for i in open..self.tokens.len() {
            if self.is_punct(i, opening) {
                depth += 1;
            } else if self.is_punct(i, close) {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
        None
    }

    /// A comment's text, its markers included.
    pub(crate) fn comment_text(&self, comment: &Comment) -> &'a str {
        let source: &'a str = self.source;
        source.get(comment.start..comment.end).unwrap_or("")
    }
}

/// Splits `source` into tokens and comments.
pub(crate) fn lex(source: &str) -> Lexed<'_> {
    let bytes = source.as_bytes();
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(
            bytes
                .iter()
                .enumerate()
                .filter(|(_, b)| **b == b'\n')
                .map(|(i, _)| i + 1),
        )
        .collect();
    let line = |offset: usize| line_starts.partition_point(|&start| start <= offset);
    let mut tokens = Vec::new();
    let mut comments = Vec::new();
    let mut i = shebang_len(source);
    while let Some(&byte) = bytes.get(i) {
        if let Some(len) = space_len(source, i) {
            i += len;
            continue;
        }
        let start = i;
        let block = match (byte, bytes.get(i + 1)) {
            (b'/', Some(b'/')) => {
                i = line_end(bytes, i);
                Some(false)
            }
            (b'/', Some(b'*')) => {
                i = block_comment_end(bytes, i);
                Some(true)
            }
            _ => None,
        };
        if let Some(block) = block {
            let line_start = line_starts[line(start) - 1];
            comments.push(Comment {
                start,
                end: i,
                first_line: line(start),
                last_line: line(i.saturating_sub(1).max(start)),
                block,
                own_line: bytes[line_start..start]
                    .iter()
                    .all(|b| b.is_ascii_whitespace()),
            });
            continue;
        }
        let (end, kind, text_start) = match byte {
            b'"' => (quoted_end(bytes, i + 1), Kind::Literal, start),
            b'\'' => {
                let (end, kind) = quote(source, i);
                (end, kind, start)
            }
            b'0'..=b'9' => (word_end(source, i), Kind::Literal, start),
            b if b.is_ascii_alphabetic() || b == b'_' || b >= 0x80 => word(source, i),
            _ => (i + 1, Kind::Punct(byte), start),
        };
        tokens.push(Token {
            kind,
            start: text_start,
            end,
            line: line(start),
        });
        i = end;
    }
    Lexed {
        source,
        tokens,
        comments,
    }
}

/// The length of the shebang line at the start of `source`, or 0 if it
/// has none. As in rustc, `#!` starts a shebang unless the next token,
/// past whitespace and comments, is `[`: `#!/**/[allow(x)]` is an inner
/// attribute.
pub(crate) fn shebang_len(source: &str) -> usize {
    let Some(rest) = source.strip_prefix("#!") else {
        return 0;
    };
    let bytes = rest.as_bytes();
    let mut i = 0;
    loop {
        if let Some(len) = space_len(rest, i) {
            i += len;
            continue;
        }
        match (bytes.get(i), bytes.get(i + 1)) {
            (Some(b'/'), Some(b'/')) => i = line_end(bytes, i),
            (Some(b'/'), Some(b'*')) => i = block_comment_end(bytes, i),
            (Some(b'['), _) => return 0,
            _ => return source.find('\n').unwrap_or(source.len()),
        }
    }
}

/// Where the line holding byte `i` ends: at its `\n`, or the end.
fn line_end(bytes: &[u8], i: usize) -> usize {
    bytes
        .get(i..)
        .and_then(|rest| rest.iter().position(|&b| b == b'\n'))
        .map_or(bytes.len(), |n| i + n)
}

/// The length of the whitespace character at byte `i`, if one is there:
/// Rust's whitespace is ASCII's and five more characters.
fn space_len(source: &str, i: usize) -> Option<usize> {
    let c = source.get(i..)?.chars().next()?;
    matches!(
        c,
        ' ' | '\t'
            | '\n'
            | '\r'
            | '\u{b}'
            | '\u{c}'
            | '\u{85}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{2028}'
            | '\u{2029}'
    )
    .then(|| c.len_utf8())
}

/// The end of an identifier, keyword or number starting at `i`.
fn word_end(source: &str, mut i: usize) -> usize {
    let bytes = source.as_bytes();
    while let Some(&b) = bytes.get(i) {
        if !(b.is_ascii_alphanumeric()
            || b == b'_'
            || (b >= 0x80 && space_len(source, i).is_none()))
        {
            break;
        }
        i += 1;
    }
    i
}

/// The end of a `/* */` comment starting at `i`; they nest.
fn block_comment_end(bytes: &[u8], mut i: usize) -> usize {
    let mut depth = 0usize;
    while i < bytes.len() {
        match (bytes[i], bytes.get(i + 1)) {
            (b'/', Some(b'*')) => {
                depth += 1;
                i += 2;
            }
            (b'*', Some(b'/')) => {
                depth -= 1;
                i += 2;
                if depth == 0 {
                    return i;
                }
            }
            _ => i += 1,
        }
    }
    bytes.len()
}

/// The end of a string whose body starts at `i`: past its closing `"`,
/// stepping over escapes, or the end of the source.
fn quoted_end(bytes: &[u8], mut i: usize) -> usize {
    while let Some(&b) = bytes.get(i) {
        match b {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// A character literal or a lifetime at the `'` at `i`.
fn quote(source: &str, i: usize) -> (usize, Kind) {
    let bytes = source.as_bytes();
    match bytes.get(i + 1) {
        // `'\n'`, `'\''`, `'\u{1F600}'`: past the escaped byte, to the `'`.
        Some(b'\\') => {
            let mut j = i + 3;
            while let Some(&b) = bytes.get(j) {
                match b {
                    b'\'' => return (j + 1, Kind::Literal),
                    b'\n' => break,
                    _ => j += 1,
                }
            }
            (j.min(bytes.len()), Kind::Literal)
        }
        Some(_) => {
            let len = source
                .get(i + 1..)
                .and_then(|rest| rest.chars().next())
                .map_or(1, char::len_utf8);
            if bytes.get(i + 1 + len) == Some(&b'\'') {
                (i + 2 + len, Kind::Literal)
            } else {
                match word_end(source, i + 1) {
                    end if end > i + 1 => (end, Kind::Lifetime),
                    _ => (i + 1, Kind::Punct(b'\'')),
                }
            }
        }
        None => (i + 1, Kind::Punct(b'\'')),
    }
}

/// An identifier, keyword or prefixed literal starting at `i`: its end,
/// its kind, and where its text starts.
fn word(source: &str, i: usize) -> (usize, Kind, usize) {
    let bytes = source.as_bytes();
    let end = word_end(source, i);
    let prefix = &bytes[i..end];
    match (prefix, bytes.get(end)) {
        (b"r", Some(b'#'))
            if bytes
                .get(end + 1)
                .is_some_and(|&b| b.is_ascii_alphabetic() || b == b'_' || b >= 0x80) =>
        {
            (
                word_end(source, end + 1),
                Kind::Ident { raw: true },
                end + 1,
            )
        }
        (b"r" | b"br" | b"cr", Some(b'"' | b'#')) => match raw_string_end(bytes, end) {
            Some(end) => (end, Kind::Literal, i),
            None => (end, Kind::Ident { raw: false }, i),
        },
        (b"b" | b"c", Some(b'"')) => (quoted_end(bytes, end + 1), Kind::Literal, i),
        (b"b", Some(b'\'')) => match quote(source, end) {
            (end, Kind::Literal) => (end, Kind::Literal, i),
            _ => (end, Kind::Ident { raw: false }, i),
        },
        _ => (end, Kind::Ident { raw: false }, i),
    }
}

/// The end of a raw string whose `#`s or `"` start at `i`, or `None` if no
/// `"` follows the `#`s.
fn raw_string_end(bytes: &[u8], i: usize) -> Option<usize> {
    let hashes = bytes.get(i..)?.iter().take_while(|&&b| b == b'#').count();
    if bytes.get(i + hashes) != Some(&b'"') {
        return None;
    }
    let mut j = i + hashes + 1;
    while let Some(&b) = bytes.get(j) {
        if b == b'"'
            && bytes
                .get(j + 1..j + 1 + hashes)
                .is_some_and(|tail| tail.iter().all(|&h| h == b'#'))
        {
            return Some(j + 1 + hashes);
        }
        j += 1;
    }
    Some(bytes.len())
}

#[cfg(test)]
mod tests {
    use super::{Kind, lex, shebang_len};

    /// The identifiers and keywords in `source`, raw ones marked `r#`.
    fn idents(source: &str) -> Vec<String> {
        let lexed = lex(source);
        (0..lexed.tokens.len())
            .filter_map(|i| match lexed.tokens[i].kind {
                Kind::Ident { raw: false } => Some(lexed.text(i).to_owned()),
                Kind::Ident { raw: true } => Some(format!("r#{}", lexed.text(i))),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn comments_and_literals_hide_their_words() {
        let source = r##"
            a // unsafe line
            b /* unsafe /* nested unsafe */ still */ c
            d "unsafe \" unsafe" e
            f r#"unsafe " unsafe"# g
            h br"unsafe" i b"unsafe" j c"unsafe" k cr#"unsafe"# l
            m 'u' n '\'' o '"' p "\\" q '\\' r b'\'' s '\u{75}'
            t 0xunsafe u 1_000u32 v
        "##;
        assert_eq!(
            idents(source),
            "a b c d e f g h i j k l m n o p q r s t u v"
                .split(' ')
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn raw_identifiers_lifetimes_and_unicode_are_told_apart() {
        assert_eq!(
            idents("r#unsafe unsafe r#std r#type"),
            ["r#unsafe", "unsafe", "r#std", "r#type"]
        );
        assert_eq!(
            idents("fn f<'a>(x: &'a str) -> &'static str { 'outer: loop {} }"),
            ["fn", "f", "x", "str", "str", "loop"]
        );
        let lexed = lex("'a 'b' 'é' 'été");
        let kinds: Vec<Kind> = lexed.tokens.iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            [Kind::Lifetime, Kind::Literal, Kind::Literal, Kind::Lifetime]
        );
        assert_eq!(idents("été unsafe naïve"), ["été", "unsafe", "naïve"]);
        // Rust's non-ASCII whitespace separates words.
        assert_eq!(
            idents("unsafe\u{2028}{ x }\u{85}unsafe\u{200e}impl"),
            ["unsafe", "x", "unsafe", "impl"]
        );
    }

    #[test]
    fn lines_and_comments_are_tracked() {
        let source = "#!/usr/bin/env run\n/// outer\n//! inner\n/** block\n doc */ x /*! in */\n/**/ /***/ y // tail\n";
        let lexed = lex(source);
        let comments: Vec<(usize, usize, bool, bool)> = lexed
            .comments
            .iter()
            .map(|c| (c.first_line, c.last_line, c.block, c.own_line))
            .collect();
        assert_eq!(
            comments,
            [
                (2, 2, false, true),
                (3, 3, false, true),
                (4, 5, true, true),
                (5, 5, true, false),
                (6, 6, true, true),
                (6, 6, true, false),
                (6, 6, false, false),
            ]
        );
        let lines: Vec<(String, usize)> = (0..lexed.tokens.len())
            .map(|i| (lexed.text(i).to_owned(), lexed.tokens[i].line))
            .collect();
        assert_eq!(lines, [("x".to_owned(), 5), ("y".to_owned(), 6)]);
    }

    #[test]
    fn a_shebang_is_not_an_inner_attribute() {
        assert_eq!(shebang_len("#!/usr/bin/env run\nfn"), 18);
        assert_eq!(shebang_len("#!/usr/bin/env run"), 18);
        assert_eq!(
            shebang_len("#! /bin/x [y]\nfn"),
            13,
            "a `[` after other text"
        );
        for attribute in [
            "#![allow(x)]\nfn",
            "#!  [allow(x)]",
            "#!/**/[allow(x)]",
            "#!/* a /* nested */ b */\n[allow(x)]",
            "#!// a comment\n[allow(x)]",
            "#!\u{2028}[allow(x)]",
        ] {
            assert_eq!(shebang_len(attribute), 0, "{attribute:?}");
            assert_eq!(&idents(attribute)[..2], ["allow", "x"], "{attribute:?}");
        }
        assert_eq!(shebang_len("fn f() {}"), 0);
        assert_eq!(shebang_len("#"), 0);
    }

    #[test]
    fn brackets_close() {
        let lexed = lex("a ( b [ c { d } ] ( e ) ) {");
        assert_eq!(lexed.closing(1), Some(12));
        assert_eq!(lexed.closing(3), Some(8));
        assert_eq!(lexed.closing(5), Some(7));
        assert_eq!(lexed.closing(13), None, "never closed");
        assert_eq!(lexed.closing(0), None, "not a bracket");
    }

    #[test]
    fn any_text_lexes_without_panicking() {
        for text in [
            "",
            "'",
            "''",
            "'\\",
            "\"",
            "\"\\",
            "r#",
            "r#\"",
            "br##\"x\"#",
            "b'",
            "b'\\",
            "/*",
            "/* /* */",
            "//",
            "#!",
            "é'",
            "'é",
            "\u{2028}",
            "x\u{200f}",
            "r#é",
            "'\u{85}'",
            "c\"\\\"",
            "0x'",
            "\\",
            "#[",
            "]",
        ] {
            let lexed = lex(text);
            assert!(
                lexed
                    .tokens
                    .iter()
                    .all(|t| t.start <= t.end && t.end <= text.len()),
                "{text:?}"
            );
        }
        // Every prefix of a varied source.
        let source =
            "fn f() { let s = r#\"a\"#; /* c */ 'x'; b'\\''; \"é\\\"\"; unsafe {} } // é\n";
        for end in (0..=source.len()).filter(|&end| source.is_char_boundary(end)) {
            let _ = lex(&source[..end]);
            let _ = shebang_len(&source[..end]);
        }
    }
}
