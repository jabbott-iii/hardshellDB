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

//! Logging (`docs/01-architecture.md`, "Errors and logging"). The macros
//! [`error!`], [`warn!`], [`info!`], [`debug!`] and [`trace!`] take
//! `format!` arguments and hand a [`LogRecord`] to the one process-wide
//! sink, which the host installs with [`set_sink`] (through the facade's
//! `set_log_sink`). The engine never prints.
//!
//! With no sink, a macro costs one atomic load: it evaluates none of its
//! arguments, formats nothing and allocates nothing. With a sink, the
//! record carries the unformatted [`fmt::Arguments`], so formatting, and
//! any allocation, is the sink's choice.
//!
//! ```
//! use hardshell_core::log::{self, Level, warn};
//!
//! // The host, once, at startup:
//! let installed = log::set_sink(Box::new(|record| {
//!     if record.level() <= Level::Warn {
//!         eprintln!("{}: {}: {}", record.level(), record.target(), record.message());
//!     }
//! }));
//! assert!(installed.is_ok());
//!
//! // Engine code:
//! let frames = 2_000;
//! warn!("the WAL holds {frames} frames, twice its checkpoint threshold");
//! ```

use std::fmt;
use std::sync::OnceLock;

/// How severe a record is. Ordered from most to least severe, so
/// `level <= Level::Warn` selects errors and warnings.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// Something failed: the engine reports the failure or works around it
    /// (a rollback that failed while a transaction was dropped).
    Error,
    /// Something the host should look at (a checkpoint held back by a long
    /// reader, a WAL past twice its checkpoint threshold).
    Warn,
    /// A notable normal event (recovery replayed the WAL).
    Info,
    /// Detail for diagnosing a problem.
    Debug,
    /// Step-by-step detail: the most verbose level.
    Trace,
}

impl Level {
    /// The level's name, as its macro is named: `"error"`, `"warn"`,
    /// `"info"`, `"debug"` or `"trace"`. `Display` prints the same.
    pub const fn as_str(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warn => "warn",
            Level::Info => "info",
            Level::Debug => "debug",
            Level::Trace => "trace",
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One record, as the sink sees it.
#[derive(Clone, Copy, Debug)]
pub struct LogRecord<'a> {
    level: Level,
    target: &'static str,
    message: fmt::Arguments<'a>,
}

impl<'a> LogRecord<'a> {
    /// The level: which macro logged it.
    pub fn level(&self) -> Level {
        self.level
    }

    /// The module that logged it, as `module_path!()` names it
    /// (`hardshell_storage::wal`).
    pub fn target(&self) -> &'static str {
        self.target
    }

    /// The message, not yet formatted: write it with `{}`, or take
    /// [`fmt::Arguments::as_str`] when it has no arguments.
    pub fn message(&self) -> fmt::Arguments<'a> {
        self.message
    }
}

/// A sink: called with every record, on the thread that logged it.
///
/// It must not panic, since the panic would unwind into the engine code
/// that logged, and must not log, directly or by calling into the engine,
/// since that would recurse. It should be quick.
pub type Sink = Box<dyn Fn(&LogRecord<'_>) + Send + Sync>;

/// The process-wide sink. Set once, so reading it takes no lock.
static SINK: OnceLock<Sink> = OnceLock::new();

/// Installs the process-wide sink. The first call wins: a later call
/// leaves the installed sink in place and returns its own `sink` unused.
pub fn set_sink(sink: Sink) -> Result<(), Sink> {
    SINK.set(sink)
}

/// The installed sink, if any, in one atomic load. For the macros only.
#[doc(hidden)]
#[inline]
pub fn __sink() -> Option<&'static Sink> {
    SINK.get()
}

/// Hands one record to `sink`. For the macros only.
#[doc(hidden)]
pub fn __emit(sink: &Sink, level: Level, target: &'static str, message: fmt::Arguments<'_>) {
    sink(&LogRecord {
        level,
        target,
        message,
    });
}

/// The body of every logging macro: the arguments are evaluated, inside
/// `format_args!`, only when a sink is installed.
#[doc(hidden)]
#[macro_export]
macro_rules! __log {
    ($level:ident, $($arg:tt)+) => {
        if let ::core::option::Option::Some(sink) = $crate::log::__sink() {
            $crate::log::__emit(
                sink,
                $crate::log::Level::$level,
                ::core::module_path!(),
                ::core::format_args!($($arg)+),
            );
        }
    };
}

/// Logs at [`Level::Error`](crate::log::Level::Error), with `format!`
/// arguments; see [`log`](crate::log).
#[doc(hidden)]
#[macro_export]
macro_rules! __log_error {
    ($($arg:tt)+) => { $crate::__log!(Error, $($arg)+) };
}

/// Logs at [`Level::Warn`](crate::log::Level::Warn), with `format!`
/// arguments; see [`log`](crate::log).
#[doc(hidden)]
#[macro_export]
macro_rules! __log_warn {
    ($($arg:tt)+) => { $crate::__log!(Warn, $($arg)+) };
}

/// Logs at [`Level::Info`](crate::log::Level::Info), with `format!`
/// arguments; see [`log`](crate::log).
#[doc(hidden)]
#[macro_export]
macro_rules! __log_info {
    ($($arg:tt)+) => { $crate::__log!(Info, $($arg)+) };
}

/// Logs at [`Level::Debug`](crate::log::Level::Debug), with `format!`
/// arguments; see [`log`](crate::log).
#[doc(hidden)]
#[macro_export]
macro_rules! __log_debug {
    ($($arg:tt)+) => { $crate::__log!(Debug, $($arg)+) };
}

/// Logs at [`Level::Trace`](crate::log::Level::Trace), with `format!`
/// arguments; see [`log`](crate::log).
#[doc(hidden)]
#[macro_export]
macro_rules! __log_trace {
    ($($arg:tt)+) => { $crate::__log!(Trace, $($arg)+) };
}

#[doc(inline)]
pub use crate::__log_debug as debug;
#[doc(inline)]
pub use crate::__log_error as error;
#[doc(inline)]
pub use crate::__log_info as info;
#[doc(inline)]
pub use crate::__log_trace as trace;
#[doc(inline)]
pub use crate::__log_warn as warn;

#[cfg(test)]
mod tests {
    use super::{Level, LogRecord};

    #[test]
    fn levels_order_from_most_to_least_severe() {
        let levels = [
            Level::Error,
            Level::Warn,
            Level::Info,
            Level::Debug,
            Level::Trace,
        ];
        assert!(levels.windows(2).all(|pair| pair[0] < pair[1]));
        let names = levels.map(|level| (level.as_str(), level.to_string()));
        assert_eq!(
            names.map(|(name, shown)| {
                assert_eq!(name, shown);
                name
            }),
            ["error", "warn", "info", "debug", "trace"]
        );
    }

    #[test]
    fn a_record_gives_back_its_parts() {
        let page = 7;
        let record = LogRecord {
            level: Level::Warn,
            target: "hardshell_storage::wal",
            message: format_args!("page {page} is damaged"),
        };
        assert_eq!(record.level(), Level::Warn);
        assert_eq!(record.target(), "hardshell_storage::wal");
        assert_eq!(record.message().to_string(), "page 7 is damaged");
    }
}
