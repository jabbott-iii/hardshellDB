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

//! Error conventions shared by every crate (`docs/subsystems/core.md`,
//! "Error conventions"): the [`SqlStateError`] trait every error type
//! implements, [`ErrorContext`] for adding "while doing X" notes, and
//! [`CoreError`], this crate's own error.

use crate::sqlstate::SqlState;
use std::borrow::Cow;
use std::error::Error;
use std::fmt;

/// An error that reports a SQLSTATE.
///
/// Every workspace error type implements it. An error enum maps its variants
/// with an exhaustive `match`, so a new variant cannot compile without a
/// code; an error struct that stores its code (as `ParseError` will) returns
/// that field. A variant that wraps another error returns that error's code;
/// reclassifying one (a truncated varint is `XX001` in a page but `08P01` in
/// a wire message) is the crate's own variant, never a note.
pub trait SqlStateError: Error {
    /// The SQLSTATE this error reports.
    fn sqlstate(&self) -> SqlState;
}

/// An error with a "while doing X" note, as [`ErrorContext::with_context`]
/// returns it.
///
/// It reports the wrapped error's SQLSTATE: a note never changes the code.
/// `Display` is the wrapped error's message, then `"; "` and the note, so
/// nested notes read innermost first. `source()` is the wrapped error's
/// `source()`: its message is already in `Display`.
#[derive(Debug)]
pub struct ContextError {
    error: Box<dyn SqlStateError + Send + Sync>,
    note: Cow<'static, str>,
}

impl ContextError {
    /// The error the note was added to. Upcast it to `&(dyn Error + 'static)`
    /// to downcast it to its concrete type.
    pub fn error(&self) -> &(dyn SqlStateError + Send + Sync + 'static) {
        &*self.error
    }
}

impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}; {}", self.error, self.note)
    }
}

impl Error for ContextError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.error.source()
    }
}

impl SqlStateError for ContextError {
    fn sqlstate(&self) -> SqlState {
        self.error.sqlstate()
    }
}

/// Adds a "while doing X" note to a failed `Result`, keeping its SQLSTATE.
pub trait ErrorContext<T> {
    /// On `Err`, wraps the error in a [`ContextError`] with the note `note`
    /// returns, for example `|| format!("while reading page {no}")` or
    /// `|| "while replaying the WAL"`. `note` runs only on `Err`.
    fn with_context<N, F>(self, note: F) -> Result<T, ContextError>
    where
        F: FnOnce() -> N,
        N: Into<Cow<'static, str>>;
}

impl<T, E> ErrorContext<T> for Result<T, E>
where
    E: SqlStateError + Send + Sync + 'static,
{
    fn with_context<N, F>(self, note: F) -> Result<T, ContextError>
    where
        F: FnOnce() -> N,
        N: Into<Cow<'static, str>>,
    {
        self.map_err(|error| ContextError {
            error: Box::new(error),
            note: note().into(),
        })
    }
}

/// The error of `hardshell-core`'s decoders: today, only
/// [`varint::decode_u64`](crate::varint::decode_u64).
///
/// Its SQLSTATE is `XX001` (`data_corrupted`): varints appear only in the
/// engine's own file format (today, the row format), never in user or
/// client input, so a malformed one means a damaged or crafted file. A
/// caller reclassifies it into a
/// variant of its own error with its own code (core.md, "Reclassifying is a
/// variant"), adding the facts this error cannot know, such as the page and
/// the varint's offset in it. The variants carry no offsets: the decoder
/// sees only a slice that starts at the varint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreError {
    /// The input ended before the varint's last byte (the first byte with
    /// its high bit clear).
    VarintTruncated,
    /// The varint is not the shortest encoding of its value: its last byte
    /// is `0x00` and is not its first.
    VarintOverlong,
    /// The varint holds more than 64 bits: its tenth byte is more than
    /// `0x01`, so its value does not fit in a `u64` or it continues past
    /// ten bytes.
    VarintOverflow,
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CoreError::VarintTruncated => "invalid varint: truncated",
            CoreError::VarintOverlong => "invalid varint: overlong encoding",
            CoreError::VarintOverflow => "invalid varint: more than 64 bits",
        })
    }
}

impl Error for CoreError {}

impl SqlStateError for CoreError {
    fn sqlstate(&self) -> SqlState {
        match self {
            CoreError::VarintTruncated | CoreError::VarintOverlong | CoreError::VarintOverflow => {
                SqlState::DATA_CORRUPTED
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ContextError, CoreError, ErrorContext, SqlStateError};
    use crate::sqlstate::SqlState;
    use std::cell::Cell;
    use std::error::Error;
    use std::fmt;
    use std::io;

    #[derive(Debug)]
    struct Leaf {
        source: Option<io::Error>,
    }

    impl fmt::Display for Leaf {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("invalid page 7: checksum mismatch")
        }
    }

    impl Error for Leaf {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            self.source.as_ref().map(|e| e as &(dyn Error + 'static))
        }
    }

    impl SqlStateError for Leaf {
        fn sqlstate(&self) -> SqlState {
            SqlState::DATA_CORRUPTED
        }
    }

    fn fail(source: Option<io::Error>) -> Result<(), Leaf> {
        Err(Leaf { source })
    }

    fn two_notes(source: Option<io::Error>) -> ContextError {
        fail(source)
            .with_context(|| "while reading the free list")
            .with_context(|| format!("while opening \"{}\"", "app.hsdb"))
            .unwrap_err()
    }

    #[test]
    fn context_keeps_the_sqlstate() {
        let one = fail(None)
            .with_context(|| "while reading the free list")
            .unwrap_err();
        assert_eq!(one.sqlstate(), SqlState::DATA_CORRUPTED);
        assert_eq!(two_notes(None).sqlstate(), SqlState::DATA_CORRUPTED);
    }

    #[test]
    fn notes_follow_the_message_innermost_first() {
        assert_eq!(
            two_notes(None).to_string(),
            "invalid page 7: checksum mismatch; while reading the free list; while opening \"app.hsdb\""
        );
    }

    #[test]
    fn source_is_the_wrapped_errors_source() {
        assert!(two_notes(None).source().is_none());
        let e = two_notes(Some(io::Error::other("disk on fire")));
        let src = e.source().and_then(|s| s.downcast_ref::<io::Error>());
        assert_eq!(src.map(|s| s.to_string()).as_deref(), Some("disk on fire"));
    }

    #[test]
    fn note_is_built_only_on_error() {
        let calls = Cell::new(0);
        let ok: Result<u8, Leaf> = Ok(1);
        assert_eq!(
            ok.with_context(|| {
                calls.set(calls.get() + 1);
                "never"
            })
            .ok(),
            Some(1)
        );
        assert_eq!(calls.get(), 0);
        let _ = fail(None).with_context(|| {
            calls.set(calls.get() + 1);
            "once"
        });
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn wrapped_error_is_reachable_by_downcast() {
        let e = fail(None)
            .with_context(|| "while reading the free list")
            .unwrap_err();
        let inner: &(dyn Error + 'static) = e.error();
        assert!(inner.downcast_ref::<Leaf>().is_some());
    }

    #[test]
    fn context_error_fits_a_boxed_send_sync_error() {
        fn check<T: Send + Sync + 'static>() {}
        check::<ContextError>();
        let boxed: Box<dyn Error + Send + Sync> = Box::new(two_notes(None));
        assert!(boxed.to_string().ends_with("while opening \"app.hsdb\""));
    }

    #[test]
    fn core_error_is_data_corrupted_with_postgres_style_messages() {
        fn check<T: SqlStateError + Send + Sync + 'static>() {}
        check::<CoreError>();
        for (error, text) in [
            (CoreError::VarintTruncated, "invalid varint: truncated"),
            (
                CoreError::VarintOverlong,
                "invalid varint: overlong encoding",
            ),
            (
                CoreError::VarintOverflow,
                "invalid varint: more than 64 bits",
            ),
        ] {
            assert_eq!(error.to_string(), text);
            assert_eq!(error.sqlstate(), SqlState::DATA_CORRUPTED);
            assert!(error.source().is_none());
            let noted = Err::<(), _>(error)
                .with_context(|| "while reading row 3")
                .unwrap_err();
            assert_eq!(noted.sqlstate(), SqlState::DATA_CORRUPTED);
            assert_eq!(noted.to_string(), format!("{text}; while reading row 3"));
            let inner: &(dyn Error + 'static) = noted.error();
            assert!(inner.downcast_ref::<CoreError>().is_some());
        }
    }
}
