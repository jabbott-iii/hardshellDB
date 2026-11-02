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

//! The error convention from `docs/subsystems/core.md`, exercised from
//! outside the crate, as a dependent crate's errors would use it.

use hardshell_core::error::{ContextError, ErrorContext, SqlStateError};
use hardshell_core::sqlstate::SqlState;
use std::error::Error;
use std::fmt;
use std::io;

/// Like `hardshell_os::OsError`: a leaf whose Display already holds its cause.
#[derive(Debug)]
enum OsError {
    DiskFull,
    Io {
        op: &'static str,
        path: &'static str,
        error: io::Error,
    },
}

impl fmt::Display for OsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OsError::DiskFull => f.write_str("could not extend file: no space left on device"),
            OsError::Io { op, path, error } => write!(f, "could not {op} file \"{path}\": {error}"),
        }
    }
}

impl Error for OsError {}

impl SqlStateError for OsError {
    fn sqlstate(&self) -> SqlState {
        match self {
            OsError::DiskFull => SqlState::DISK_FULL,
            OsError::Io { .. } => SqlState::IO_ERROR,
        }
    }
}

/// Like `hardshell_storage::StorageError`: wraps OsError, has typed facts,
/// and receives notes.
#[derive(Debug)]
enum StorageError {
    Os(OsError),
    Corrupt { page: u64 },
    Context(ContextError),
}

impl From<OsError> for StorageError {
    fn from(e: OsError) -> Self {
        StorageError::Os(e)
    }
}

impl From<ContextError> for StorageError {
    fn from(e: ContextError) -> Self {
        StorageError::Context(e)
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::Os(e) => fmt::Display::fmt(e, f),
            StorageError::Corrupt { page } => write!(f, "invalid page {page}: checksum mismatch"),
            StorageError::Context(e) => fmt::Display::fmt(e, f),
        }
    }
}

impl Error for StorageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            StorageError::Os(e) => e.source(),
            StorageError::Corrupt { .. } => None,
            StorageError::Context(e) => e.source(),
        }
    }
}

impl SqlStateError for StorageError {
    fn sqlstate(&self) -> SqlState {
        match self {
            StorageError::Os(e) => e.sqlstate(),
            StorageError::Corrupt { .. } => SqlState::DATA_CORRUPTED,
            StorageError::Context(e) => e.sqlstate(),
        }
    }
}

/// Like `hardshell_exec::ExecError`: one layer further out.
#[derive(Debug)]
enum ExecError {
    Context(ContextError),
}

impl From<ContextError> for ExecError {
    fn from(e: ContextError) -> Self {
        ExecError::Context(e)
    }
}

impl fmt::Display for ExecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecError::Context(e) => fmt::Display::fmt(e, f),
        }
    }
}

impl Error for ExecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ExecError::Context(e) => e.source(),
        }
    }
}

impl SqlStateError for ExecError {
    fn sqlstate(&self) -> SqlState {
        match self {
            ExecError::Context(e) => e.sqlstate(),
        }
    }
}

fn read_page(page: u64) -> Result<(), StorageError> {
    Err(StorageError::Corrupt { page })
}

fn read_free_list() -> Result<(), StorageError> {
    read_page(7).with_context(|| "while reading the free list")?;
    Ok(())
}

fn scan(table: &str) -> Result<(), ExecError> {
    read_free_list().with_context(|| format!("while scanning table \"{table}\""))?;
    Ok(())
}

fn sync_wal(fail: OsError) -> Result<(), StorageError> {
    let synced: Result<(), OsError> = Err(fail);
    synced.with_context(|| "while syncing the WAL")?;
    Ok(())
}

#[test]
fn code_survives_notes_and_question_mark_across_crates() {
    let e = scan("users").expect_err("scan fails");
    assert_eq!(e.sqlstate(), SqlState::DATA_CORRUPTED);
    assert_eq!(
        e.to_string(),
        "invalid page 7: checksum mismatch; while reading the free list; while scanning table \"users\""
    );
}

#[test]
fn a_leaf_error_takes_a_note_without_opting_in() {
    let e = sync_wal(OsError::DiskFull).expect_err("sync fails");
    assert_eq!(e.sqlstate(), SqlState::DISK_FULL);
    let e = sync_wal(OsError::Io {
        op: "fsync",
        path: "app.hsdb-wal",
        error: io::Error::other("EIO"),
    })
    .expect_err("sync fails");
    assert_eq!(e.sqlstate(), SqlState::IO_ERROR);
    assert_eq!(
        e.to_string(),
        "could not fsync file \"app.hsdb-wal\": EIO; while syncing the WAL"
    );
    assert!(e.source().is_none(), "the cause is already in the message");
}

#[test]
fn typed_facts_are_reachable_under_notes() {
    let ExecError::Context(outer) = scan("users").expect_err("scan fails");
    let outer: &(dyn Error + 'static) = outer.error();
    let Some(StorageError::Context(inner)) = outer.downcast_ref::<StorageError>() else {
        panic!("expected StorageError::Context, got {outer:?}");
    };
    let inner: &(dyn Error + 'static) = inner.error();
    assert!(matches!(
        inner.downcast_ref::<StorageError>(),
        Some(StorageError::Corrupt { page: 7 })
    ));
}

#[test]
fn facade_style_conversion_keeps_state_and_message() {
    fn convert<E: SqlStateError + Send + Sync + 'static>(
        e: E,
    ) -> (SqlState, String, Box<dyn Error + Send + Sync>) {
        (e.sqlstate(), e.to_string(), Box::new(e))
    }
    let (state, message, _boxed) = convert(scan("users").expect_err("scan fails"));
    assert_eq!(state, SqlState::DATA_CORRUPTED);
    assert!(message.starts_with("invalid page 7"));
}
