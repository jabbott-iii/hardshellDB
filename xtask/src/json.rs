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

//! A small JSON reader for `cargo metadata` (dependency policy, item 11).
//!
//! Strict RFC 8259: no trailing commas, comments or leading zeros, and
//! nothing after the value. It never panics, and nesting deeper than
//! [`MAX_DEPTH`] is an error rather than a deep recursion.

/// The deepest nesting accepted. `cargo metadata` nests about six deep.
const MAX_DEPTH: usize = 128;

/// A JSON value. Numbers keep their text: nothing here does arithmetic.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Json>),
    /// Members in document order; a repeated name keeps every member.
    Object(Vec<(String, Json)>),
}

impl Json {
    /// Parses a JSON text: one value, with whitespace around it allowed.
    pub(crate) fn parse(text: &str) -> Result<Json, String> {
        let mut parser = Parser { text, pos: 0 };
        parser.skip_whitespace();
        let value = parser.value(1)?;
        parser.skip_whitespace();
        if parser.pos != text.len() {
            return Err(parser.error("text after the value"));
        }
        Ok(value)
    }

    /// The first member named `name`, if this is an object that has one.
    pub(crate) fn get(&self, name: &str) -> Option<&Json> {
        self.as_object()?
            .iter()
            .find(|(member, _)| member == name)
            .map(|(_, value)| value)
    }

    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(text) => Some(text),
            _ => None,
        }
    }

    pub(crate) fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }

    pub(crate) fn as_object(&self) -> Option<&[(String, Json)]> {
        match self {
            Json::Object(members) => Some(members),
            _ => None,
        }
    }
}

struct Parser<'a> {
    text: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> String {
        format!("JSON at byte {}: {message}", self.pos)
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.pos).copied()
    }

    fn skip_whitespace(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.peek() {
            self.pos += 1;
        }
    }

    /// A value at nesting `depth`: 1 for the top level.
    fn value(&mut self, depth: usize) -> Result<Json, String> {
        match self.peek() {
            Some(b'{' | b'[') if depth > MAX_DEPTH => Err(self.error("nested too deeply")),
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Json::String),
            Some(b't') => self.word("true", Json::Bool(true)),
            Some(b'f') => self.word("false", Json::Bool(false)),
            Some(b'n') => self.word("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("expected a value")),
            None => Err(self.error("expected a value, found the end")),
        }
    }

    fn word(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if self
            .text
            .get(self.pos..)
            .is_some_and(|rest| rest.starts_with(word))
        {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(self.error("expected a value"))
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, String> {
        self.pos += 1;
        let mut members = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Object(members));
        }
        loop {
            self.skip_whitespace();
            if self.peek() != Some(b'"') {
                return Err(self.error("expected a member name"));
            }
            let name = self.string()?;
            self.skip_whitespace();
            if self.peek() != Some(b':') {
                return Err(self.error("expected `:`"));
            }
            self.pos += 1;
            self.skip_whitespace();
            members.push((name, self.value(depth + 1)?));
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Object(members));
                }
                _ => return Err(self.error("expected `,` or `}`")),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json, String> {
        self.pos += 1;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Array(items));
        }
        loop {
            self.skip_whitespace();
            items.push(self.value(depth + 1)?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(self.error("expected `,` or `]`")),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.pos += 1;
        let mut out = String::new();
        loop {
            let run = self.pos;
            while let Some(byte) = self.peek() {
                if byte == b'"' || byte == b'\\' || byte < 0x20 {
                    break;
                }
                self.pos += 1;
            }
            // The run stops only at ASCII bytes, so it is whole characters.
            out.push_str(self.text.get(run..self.pos).unwrap_or_default());
            match self.peek() {
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    out.push(self.escape()?);
                }
                Some(_) => return Err(self.error("control character in a string")),
                None => return Err(self.error("unterminated string")),
            }
        }
    }

    /// The character an escape after its `\` stands for.
    fn escape(&mut self) -> Result<char, String> {
        let byte = self
            .peek()
            .ok_or_else(|| self.error("unterminated string"))?;
        self.pos += 1;
        Ok(match byte {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => {
                let unit = self.hex4()?;
                let code = match unit {
                    0xD800..=0xDBFF => {
                        if self
                            .text
                            .get(self.pos..)
                            .is_some_and(|rest| rest.starts_with("\\u"))
                        {
                            self.pos += 2;
                        } else {
                            return Err(self.error("unpaired surrogate"));
                        }
                        let low = self.hex4()?;
                        if !(0xDC00..=0xDFFF).contains(&low) {
                            return Err(self.error("unpaired surrogate"));
                        }
                        0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00)
                    }
                    0xDC00..=0xDFFF => return Err(self.error("unpaired surrogate")),
                    _ => unit,
                };
                char::from_u32(code).ok_or_else(|| self.error("not a character"))?
            }
            _ => return Err(self.error("unknown escape")),
        })
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let mut value = 0;
        for _ in 0..4 {
            let digit = self
                .peek()
                .and_then(|byte| char::from(byte).to_digit(16))
                .ok_or_else(|| self.error("expected four hex digits"))?;
            value = value * 16 + digit;
            self.pos += 1;
        }
        Ok(value)
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => self.digits()?,
            _ => return Err(self.error("expected a digit")),
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            self.digits()?;
        }
        if let Some(b'e' | b'E') = self.peek() {
            self.pos += 1;
            if let Some(b'+' | b'-') = self.peek() {
                self.pos += 1;
            }
            self.digits()?;
        }
        Ok(Json::Number(
            self.text
                .get(start..self.pos)
                .unwrap_or_default()
                .to_owned(),
        ))
    }

    /// One or more digits.
    fn digits(&mut self) -> Result<(), String> {
        if !self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            return Err(self.error("expected a digit"));
        }
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.pos += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Json, MAX_DEPTH};

    fn string(text: &str) -> Json {
        Json::String(text.to_owned())
    }

    #[test]
    fn values_parse() {
        assert_eq!(Json::parse(" null "), Ok(Json::Null));
        assert_eq!(Json::parse("true"), Ok(Json::Bool(true)));
        assert_eq!(Json::parse("false"), Ok(Json::Bool(false)));
        for number in ["0", "-0", "12", "-3.25", "1e9", "2.5E-3", "7e+1"] {
            assert_eq!(Json::parse(number), Ok(Json::Number(number.to_owned())));
        }
        assert_eq!(
            Json::parse(r#"{"a": [1, "x", {}], "b": null}"#),
            Ok(Json::Object(vec![
                (
                    "a".to_owned(),
                    Json::Array(vec![
                        Json::Number("1".to_owned()),
                        string("x"),
                        Json::Object(vec![])
                    ])
                ),
                ("b".to_owned(), Json::Null),
            ]))
        );
        assert_eq!(Json::parse("[]"), Ok(Json::Array(vec![])));
    }

    #[test]
    fn escapes_decode() {
        assert_eq!(
            Json::parse(r#""a\"b\\c\/d\b\f\n\r\t""#),
            Ok(string("a\"b\\c/d\u{8}\u{c}\n\r\t"))
        );
        assert_eq!(Json::parse(r#""\u00e9\u4e2d""#), Ok(string("é中")));
        assert_eq!(Json::parse(r#""\ud83d\ude00""#), Ok(string("😀")));
        assert_eq!(Json::parse("\"é中😀\""), Ok(string("é中😀")));
        assert_eq!(
            Json::parse(r#""C:\\Users\\x\\Cargo.toml""#),
            Ok(string(r"C:\Users\x\Cargo.toml"))
        );
    }

    #[test]
    fn malformed_text_is_an_error() {
        for bad in [
            "",
            " ",
            "nul",
            "tru",
            "[1,]",
            "[1 2]",
            r#"{"a" 1}"#,
            r#"{"a": 1,}"#,
            "{1: 2}",
            "01",
            "1.",
            "-",
            ".5",
            "1e",
            "+1",
            "\"abc",
            "\"a\u{1}b\"",
            r#""\x""#,
            r#""\u12""#,
            r#""\ud800""#,
            r#""\ud800\u0041""#,
            r#""\ud800\ndc00""#,
            r#""\udc00""#,
            "[1] [2]",
            "{} x",
            "[",
            "{",
        ] {
            assert!(Json::parse(bad).is_err(), "{bad:?} parsed");
        }
    }

    #[test]
    fn nesting_is_limited() {
        let nested = |depth: usize| "[".repeat(depth) + &"]".repeat(depth);
        assert!(Json::parse(&nested(MAX_DEPTH)).is_ok());
        assert!(Json::parse(&nested(MAX_DEPTH + 1)).is_err());
        assert!(Json::parse(&nested(100_000)).is_err());
        let objects = "{\"a\":".repeat(MAX_DEPTH + 1) + "1" + &"}".repeat(MAX_DEPTH + 1);
        assert!(Json::parse(&objects).is_err());
    }

    #[test]
    fn accessors_find_members() {
        let json = Json::parse(r#"{"a": "x", "a": "y", "n": [true]}"#).unwrap();
        assert_eq!(
            json.get("a").and_then(Json::as_str),
            Some("x"),
            "the first of two"
        );
        assert_eq!(
            json.get("n").and_then(Json::as_array).map(<[Json]>::len),
            Some(1)
        );
        assert_eq!(json.get("missing"), None);
        assert_eq!(Json::Null.get("a"), None);
        assert_eq!(Json::Null.as_str(), None);
        assert_eq!(json.as_object().map(<[(String, Json)]>::len), Some(3));
    }
}
