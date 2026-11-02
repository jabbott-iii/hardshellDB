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

//! SQLSTATE codes: PostgreSQL's five-character error codes (ADR-0004).
//!
//! The table in `docs/subsystems/core.md` ("SQLSTATE codes the engine
//! raises") is the source of truth. Every row has a constant here, with the
//! row's PostgreSQL condition name, and every constant has a row;
//! `tests/sqlstate_docs.rs` checks both directions, and that every SQLSTATE
//! written in `docs/` has a constant.

use std::fmt;

/// A PostgreSQL SQLSTATE: five ASCII digits or upper-case letters, of which
/// the first two are the class.
///
/// Values exist only as the named constants, so every `SqlState` is a code
/// from the table.
///
/// ```
/// use hardshell_core::sqlstate::SqlState;
///
/// let state = SqlState::UNIQUE_VIOLATION;
/// assert_eq!(state.to_string(), "23505");
/// assert_eq!(state.class(), *b"23");
/// assert!(matches!(state, SqlState::UNIQUE_VIOLATION | SqlState::CHECK_VIOLATION));
/// ```
// PartialEq and Eq stay derived: a derived (structural) PartialEq is what
// lets the constants be used as `match` patterns.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct SqlState([u8; 5]);

impl SqlState {
    /// Whether `code` is five characters from `0-9` and `A-Z`.
    const fn is_valid(code: &[u8; 5]) -> bool {
        let mut i = 0;
        while i < code.len() {
            let b = code[i];
            if !(b.is_ascii_digit() || b.is_ascii_uppercase()) {
                return false;
            }
            i += 1;
        }
        true
    }

    /// Private: only the `sqlstates!` table calls it, in const context, so a
    /// bad code fails the build (E0080). A wrong length is a type error.
    const fn from_code(code: &[u8; 5]) -> SqlState {
        assert!(
            SqlState::is_valid(code),
            "a SQLSTATE is five characters from 0-9 and A-Z"
        );
        SqlState(*code)
    }

    /// The class: the first two characters (`*b"42"` for `42601`).
    pub const fn class(self) -> [u8; 2] {
        [self.0[0], self.0[1]]
    }

    /// The five-character code, for example `"42601"`.
    pub fn as_str(&self) -> &str {
        // A true invariant: from_code is the only constructor and admits only ASCII.
        std::str::from_utf8(&self.0)
            .expect("SqlState holds only ASCII digits and upper-case letters")
    }
}

/// The bare five-character code. Honors width and alignment (`{:>7}`).
impl fmt::Display for SqlState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(self.as_str())
    }
}

/// `SqlState("42601")`, not a byte array, so `assert_eq!` failures read well.
impl fmt::Debug for SqlState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SqlState").field(&self.as_str()).finish()
    }
}

/// Defines one associated constant per row, plus `SqlState::ALL`.
macro_rules! sqlstates {
    ($( $(#[$attr:meta])* $name:ident = $code:literal; )+) => {
        impl SqlState {
            $( $(#[$attr])* pub const $name: SqlState = SqlState::from_code($code); )+

            /// Every named code, in the order of the table in `docs/subsystems/core.md`.
            pub const ALL: &'static [SqlState] = &[$( SqlState::$name ),+];
        }

        // Required. The Rust Reference guarantees that free constants are
        // evaluated; associated constants are evaluated only where used. This
        // one uses every constant through ALL, so a bad code fails every build.
        const _: &[SqlState] = SqlState::ALL;
    };
}

sqlstates! {
    /// `08P01` `protocol_violation`: a malformed wire message (M12).
    PROTOCOL_VIOLATION = b"08P01";
    /// `0A000` `feature_not_supported`: known but unsupported syntax or option (ADR-0011).
    FEATURE_NOT_SUPPORTED = b"0A000";
    /// `21000` `cardinality_violation`: a scalar subquery returned more than one row.
    CARDINALITY_VIOLATION = b"21000";
    /// `22001` `string_data_right_truncation`: a value too long for `VARCHAR(n)` on
    /// assignment.
    STRING_DATA_RIGHT_TRUNCATION = b"22001";
    /// `22003` `numeric_value_out_of_range`: integer, float or `NUMERIC` overflow.
    NUMERIC_VALUE_OUT_OF_RANGE = b"22003";
    /// `22004` `null_value_not_allowed`: `NULL` read into a non-`Option` Rust type.
    NULL_VALUE_NOT_ALLOWED = b"22004";
    /// `22007` `invalid_datetime_format`: bad date/time input.
    INVALID_DATETIME_FORMAT = b"22007";
    /// `22008` `datetime_field_overflow`: out-of-range date/time input.
    DATETIME_FIELD_OVERFLOW = b"22008";
    /// `22012` `division_by_zero`: integer, float or `NUMERIC` division or modulo by zero.
    DIVISION_BY_ZERO = b"22012";
    /// `2201W` `invalid_row_count_in_limit_clause`: a negative `LIMIT`.
    INVALID_ROW_COUNT_IN_LIMIT_CLAUSE = b"2201W";
    /// `2201X` `invalid_row_count_in_result_offset_clause`: a negative `OFFSET`.
    INVALID_ROW_COUNT_IN_RESULT_OFFSET_CLAUSE = b"2201X";
    /// `22021` `character_not_in_repertoire`: invalid UTF-8 (ADR-0014).
    CHARACTER_NOT_IN_REPERTOIRE = b"22021";
    /// `22023` `invalid_parameter_value`: a bad type modifier, page size or setting value.
    INVALID_PARAMETER_VALUE = b"22023";
    /// `22P02` `invalid_text_representation`: bad input for a non-date/time type.
    INVALID_TEXT_REPRESENTATION = b"22P02";
    /// `22P04` `bad_copy_file_format`: malformed CSV for `\copy` or `copy_in`.
    BAD_COPY_FILE_FORMAT = b"22P04";
    /// `23502` `not_null_violation`: a `NOT NULL` constraint failed.
    NOT_NULL_VIOLATION = b"23502";
    /// `23503` `foreign_key_violation`: a foreign key constraint failed.
    FOREIGN_KEY_VIOLATION = b"23503";
    /// `23505` `unique_violation`: a `PRIMARY KEY` or `UNIQUE` constraint failed.
    UNIQUE_VIOLATION = b"23505";
    /// `23514` `check_violation`: a `CHECK` constraint failed.
    CHECK_VIOLATION = b"23514";
    /// `25P02` `in_failed_sql_transaction`: a statement after an error inside `BEGIN`.
    IN_FAILED_SQL_TRANSACTION = b"25P02";
    /// `25P03` `idle_in_transaction_session_timeout`: a server session idle in a
    /// transaction too long (M12).
    IDLE_IN_TRANSACTION_SESSION_TIMEOUT = b"25P03";
    /// `2BP01` `dependent_objects_still_exist`: `DROP` without `CASCADE` of an object
    /// that others depend on.
    DEPENDENT_OBJECTS_STILL_EXIST = b"2BP01";
    /// `3B001` `invalid_savepoint_specification`: an unknown savepoint (M10).
    INVALID_SAVEPOINT_SPECIFICATION = b"3B001";
    /// `3D000` `invalid_catalog_name`: a wrong database name at server startup (M12).
    INVALID_CATALOG_NAME = b"3D000";
    /// `3F000` `invalid_schema_name`: a schema other than `public`.
    INVALID_SCHEMA_NAME = b"3F000";
    /// `40001` `serialization_failure`: writing on a stale snapshot (ADR-0008).
    SERIALIZATION_FAILURE = b"40001";
    /// `42601` `syntax_error`: a syntax error found by the parser.
    SYNTAX_ERROR = b"42601";
    /// `42P01` `undefined_table`: an unknown table.
    UNDEFINED_TABLE = b"42P01";
    /// `42703` `undefined_column`: an unknown column.
    UNDEFINED_COLUMN = b"42703";
    /// `42883` `undefined_function`: an unknown function.
    UNDEFINED_FUNCTION = b"42883";
    /// `42704` `undefined_object`: an unknown object, such as a setting name.
    UNDEFINED_OBJECT = b"42704";
    /// `42701` `duplicate_column`: a duplicate column name.
    DUPLICATE_COLUMN = b"42701";
    /// `42710` `duplicate_object`: a duplicate name of an object that is not a relation,
    /// such as a constraint.
    DUPLICATE_OBJECT = b"42710";
    /// `42P07` `duplicate_table`: a duplicate relation name (table, index, view or
    /// sequence), as in PostgreSQL.
    DUPLICATE_TABLE = b"42P07";
    /// `42702` `ambiguous_column`: an ambiguous column reference.
    AMBIGUOUS_COLUMN = b"42702";
    /// `42725` `ambiguous_function`: an ambiguous function call.
    AMBIGUOUS_FUNCTION = b"42725";
    /// `42803` `grouping_error`: an aggregate in `WHERE`, or an ungrouped column.
    GROUPING_ERROR = b"42803";
    /// `42804` `datatype_mismatch`: a type mismatch in the binder or in `Row::get`.
    DATATYPE_MISMATCH = b"42804";
    /// `428C9` `generated_always`: inserting into a `GENERATED ALWAYS` identity column
    /// without `OVERRIDING SYSTEM VALUE` (M7.8).
    GENERATED_ALWAYS = b"428C9";
    /// `42939` `reserved_name`: a name starting with `hs_`.
    RESERVED_NAME = b"42939";
    /// `42P10` `invalid_column_reference`: an out-of-range `ORDER BY` or `GROUP BY`
    /// ordinal.
    INVALID_COLUMN_REFERENCE = b"42P10";
    /// `42P18` `indeterminate_datatype`: a parameter whose type nothing determines.
    INDETERMINATE_DATATYPE = b"42P18";
    /// `53100` `disk_full`: no space for the WAL, the database file, or temporary files.
    DISK_FULL = b"53100";
    /// `53200` `out_of_memory`: `work_mem` exceeded before spilling, or every cache
    /// frame pinned (ADR-0012).
    OUT_OF_MEMORY = b"53200";
    /// `53300` `too_many_connections`: the server's connection limit (M12).
    TOO_MANY_CONNECTIONS = b"53300";
    /// `53400` `configuration_limit_exceeded`: `temp_file_limit` exceeded (M9).
    CONFIGURATION_LIMIT_EXCEEDED = b"53400";
    /// `54000` `program_limit_exceeded`: a size limit, such as the largest value or key.
    PROGRAM_LIMIT_EXCEEDED = b"54000";
    /// `54001` `statement_too_complex`: a statement past a depth or size limit, in the
    /// parser (ADR-0011) or the planner.
    STATEMENT_TOO_COMPLEX = b"54001";
    /// `54011` `too_many_columns`: the column limit.
    TOO_MANY_COLUMNS = b"54011";
    /// `54023` `too_many_arguments`: the function argument limit.
    TOO_MANY_ARGUMENTS = b"54023";
    /// `55000` `object_not_in_prerequisite_state`: a read-only open of a database that
    /// needs recovery.
    OBJECT_NOT_IN_PREREQUISITE_STATE = b"55000";
    /// `55006` `object_in_use`: a database locked by another process (ADR-0005).
    OBJECT_IN_USE = b"55006";
    /// `55P03` `lock_not_available`: the busy timeout on the write lock (ADR-0008).
    LOCK_NOT_AVAILABLE = b"55P03";
    /// `57014` `query_canceled`: an interrupt or `statement_timeout` (ADR-0010).
    QUERY_CANCELED = b"57014";
    /// `57P01` `admin_shutdown`: the server is shutting down (M12).
    ADMIN_SHUTDOWN = b"57P01";
    /// `58030` `io_error`: an OS I/O failure, or a poisoned handle (ADR-0009).
    IO_ERROR = b"58030";
    /// `XX000` `internal_error`: a broken invariant that is not corruption (a bug).
    INTERNAL_ERROR = b"XX000";
    /// `XX001` `data_corrupted`: a page checksum or on-disk structure failure, in any
    /// page, index pages included (ADR-0009).
    DATA_CORRUPTED = b"XX001";
    /// `XX002` `index_corrupted`: a damaged index. Page checksum and tree failures are
    /// `XX001`, even in an index (ADR-0009).
    INDEX_CORRUPTED = b"XX002";
}

#[cfg(test)]
mod tests {
    use super::SqlState;
    use std::collections::BTreeSet;

    #[test]
    fn every_constant_is_a_valid_code() {
        assert!(!SqlState::ALL.is_empty());
        for s in SqlState::ALL {
            let b = s.as_str().as_bytes();
            assert_eq!(b.len(), 5, "{s:?}");
            assert!(
                b.iter()
                    .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase()),
                "{s:?}"
            );
        }
    }

    #[test]
    fn class_is_the_first_two_characters() {
        for s in SqlState::ALL {
            let b = s.as_str().as_bytes();
            assert_eq!(s.class(), [b[0], b[1]], "{s:?}");
        }
    }

    #[test]
    fn codes_are_unique() {
        let set: BTreeSet<&str> = SqlState::ALL.iter().map(SqlState::as_str).collect();
        assert_eq!(set.len(), SqlState::ALL.len());
    }

    #[test]
    fn validator_accepts_digits_and_upper_case_only() {
        assert!(SqlState::is_valid(b"428C9"));
        assert!(SqlState::is_valid(b"XX000"));
        for bad in [b"4260a", b"42 01", b"-4260", b"4260\xff", b"4260\0"] {
            assert!(!SqlState::is_valid(bad), "{bad:?}");
        }
    }

    #[test]
    #[should_panic(expected = "a SQLSTATE is five characters")]
    fn from_code_rejects_a_bad_code_at_run_time() {
        let _ = SqlState::from_code(b"4260a");
    }

    #[test]
    fn display_debug_and_as_str() {
        let s = SqlState::SYNTAX_ERROR;
        assert_eq!(s.to_string(), "42601");
        assert_eq!(s.as_str(), "42601");
        assert_eq!(format!("[{s:>7}]"), "[  42601]");
        assert_eq!(format!("[{s:<6}]"), "[42601 ]");
        assert_eq!(format!("{s:?}"), "SqlState(\"42601\")");
    }

    #[test]
    fn constants_work_as_match_patterns() {
        let retryable = |s: SqlState| {
            matches!(
                s,
                SqlState::SERIALIZATION_FAILURE | SqlState::LOCK_NOT_AVAILABLE
            )
        };
        assert!(retryable(SqlState::SERIALIZATION_FAILURE));
        assert!(retryable(SqlState::LOCK_NOT_AVAILABLE));
        assert!(!retryable(SqlState::SYNTAX_ERROR));
    }

    #[test]
    fn spot_values() {
        assert_eq!(SqlState::GENERATED_ALWAYS.as_str(), "428C9");
        assert_eq!(
            SqlState::INVALID_ROW_COUNT_IN_LIMIT_CLAUSE.as_str(),
            "2201W"
        );
        assert_eq!(SqlState::INVALID_ROW_COUNT_IN_LIMIT_CLAUSE.class(), *b"22");
        assert_eq!(SqlState::DATA_CORRUPTED.class(), *b"XX");
        assert_eq!(SqlState::IO_ERROR.as_str(), "58030");
    }
}
