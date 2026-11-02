# Subsystem: `hardshell-core`

Foundation types used by every other crate. No `unsafe`, no OS calls beyond
`std` time, no dependencies (layer 0). M0.2 built the errors (`sqlstate`,
`error`); the rest of M0 builds the other modules the guardrails need
(`crc32c`, `varint`, `rng`, `testing`, `cli`, `log`), and later milestones
add to it only when two or more crates need the same thing.

## Contents

| Module | Provides |
| :- | :- |
| `sqlstate` | `SqlState` (five ASCII bytes), one named constant per row of the table below plus `SqlState::ALL`, `class()`, `as_str()`, `Display` as the five-character code |
| `error` | `SqlStateError` (the `sqlstate()` every error type implements), `ContextError` and the `ErrorContext` helper for adding "while doing X" notes without changing the SQLSTATE; `CoreError` arrives with core's first decoder (M0.3) |
| `bytes` | `Reader` (bounds-checked little-endian reads with an offset, every read returns `Result`) and `Writer` (appends to a reused `Vec<u8>`) for every decoder and encoder in the workspace |
| `crc32c` | CRC-32C (Castagnoli, reflected polynomial `0x82F63B78`), table-driven slicing-by-8; `crc32c(&[u8])` and `Crc32c::update` for chained checksums (ADR-0009) |
| `varint` | Unsigned LEB128 for `u64`, zigzag for `i64`; decode returns the value and the bytes used, and rejects overlong encodings |
| `hash` | `fnv1a64`; `FastHasher` with `FastHashMap` / `FastHashSet` aliases (deterministic, non-cryptographic) |
| `rng` | `Pcg32` (seedable, `next_u32`, `next_u64`, `range`, `shuffle`, `fill_bytes`); not for secrets |
| `log` | Macros `error!`, `warn!`, `info!`, `debug!`, `trace!` with a module target; one global sink slot set by the facade's `set_log_sink`; with no sink each macro costs one atomic load |
| `testing` | Property-test harness `check` and `check_sized`, `assert_approx!` (see `docs/04-quality.md`) |
| `cli` | Minimal argument parser for `xtask`, `hardshell` and `hardshelld` (`--flag`, `--key value`, `-k value`, positionals, `--`) |
| `csv` | RFC 4180 reader and writer over bytes: configurable delimiter, quote and escape, header row, `\N`-style NULL marker; no types (the facade converts fields with `hardshell-types`) |
| `civil` | UTC civil date and time from `SystemTime` (days-to-civil), for log lines and file names |

## Key types

```rust
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct SqlState([u8; 5]);              // private field: only the constants below exist

impl SqlState {
    // One constant per row of the table below, named after its PostgreSQL
    // condition name; an invalid code fails the build.
    pub const SYNTAX_ERROR: SqlState;      // 42601
    pub const DATA_CORRUPTED: SqlState;    // XX001
    // ...
    pub const ALL: &'static [SqlState];    // every constant, in table order
    pub const fn class(self) -> [u8; 2];   // the first two characters
    pub fn as_str(&self) -> &str;          // "42601"; Display prints the same, Debug SqlState("42601")
}

pub trait SqlStateError: std::error::Error {   // every workspace error type implements it
    fn sqlstate(&self) -> SqlState;
}

pub struct ContextError { /* the boxed error and its note */ }  // Display: "{error}; {note}"

impl ContextError {
    pub fn error(&self) -> &(dyn SqlStateError + Send + Sync + 'static);  // for a downcast
}

pub trait ErrorContext<T> {    // for Result<T, E> where E: SqlStateError + Send + Sync + 'static
    fn with_context<N: Into<Cow<'static, str>>, F: FnOnce() -> N>(self, note: F)
        -> Result<T, ContextError>;
}

pub struct Reader<'a> { buf: &'a [u8], pos: usize }

impl<'a> Reader<'a> {
    pub fn u8(&mut self) -> Result<u8, CoreError>;            // CoreError::Truncated { at, wanted }
    pub fn u32_le(&mut self) -> Result<u32, CoreError>;
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], CoreError>;
    pub fn varint_u64(&mut self) -> Result<u64, CoreError>;
    pub fn position(&self) -> usize;
    pub fn remaining(&self) -> usize;
}
```

## SQLSTATE codes the engine raises

The table is the source of truth for codes; other specs refer to it. Every
code is a real PostgreSQL code with PostgreSQL's meaning (ADR-0004).

| Code | Name | Raised for |
| :- | :- | :- |
| `08P01` | `protocol_violation` | Malformed wire message (M12) |
| `0A000` | `feature_not_supported` | Known but unsupported syntax or option (ADR-0011) |
| `21000` | `cardinality_violation` | Scalar subquery returned more than one row |
| `22001` | `string_data_right_truncation` | Value too long for `VARCHAR(n)` on assignment |
| `22003` | `numeric_value_out_of_range` | Integer, float or `NUMERIC` overflow |
| `22004` | `null_value_not_allowed` | `NULL` read into a non-`Option` Rust type |
| `22007` / `22008` | `invalid_datetime_format` / `datetime_field_overflow` | Bad or out-of-range date/time input |
| `22012` | `division_by_zero` | Integer, float or `NUMERIC` division or modulo by zero |
| `2201W` / `2201X` | `invalid_row_count_in_limit_clause` / `invalid_row_count_in_result_offset_clause` | Negative `LIMIT` or `OFFSET` |
| `22021` | `character_not_in_repertoire` | Invalid UTF-8 (ADR-0014) |
| `22023` | `invalid_parameter_value` | Bad type modifier, page size or setting value |
| `22P02` | `invalid_text_representation` | Bad input for a non-date/time type |
| `22P04` | `bad_copy_file_format` | Malformed CSV for `\copy` / `copy_in` |
| `23502` / `23503` / `23505` / `23514` | `not_null_violation` / `foreign_key_violation` / `unique_violation` / `check_violation` | Constraint checks |
| `25P02` | `in_failed_sql_transaction` | Statement after an error inside `BEGIN` |
| `25P03` | `idle_in_transaction_session_timeout` | Server session idle in a transaction too long (M12) |
| `2BP01` | `dependent_objects_still_exist` | `DROP` without `CASCADE` |
| `3B001` | `invalid_savepoint_specification` | Unknown savepoint (M10) |
| `3D000` / `3F000` | `invalid_catalog_name` / `invalid_schema_name` | Wrong database name at server startup; schema other than `public` |
| `40001` | `serialization_failure` | Writing on a stale snapshot (ADR-0008) |
| `42601` | `syntax_error` | Parser |
| `42P01` / `42703` / `42883` / `42704` | `undefined_table` / `undefined_column` / `undefined_function` / `undefined_object` | Binder; unknown setting |
| `42701` / `42710` / `42P07` | `duplicate_column` / `duplicate_object` / `duplicate_table` | DDL |
| `42702` / `42725` | `ambiguous_column` / `ambiguous_function` | Binder |
| `42803` | `grouping_error` | Aggregates in `WHERE`, ungrouped columns |
| `42804` | `datatype_mismatch` | Binder, `Row::get` |
| `428C9` | `generated_always` | Inserting into a `GENERATED ALWAYS` identity column without `OVERRIDING SYSTEM VALUE` (M7.8) |
| `42939` | `reserved_name` | Names starting with `hs_` |
| `42P10` | `invalid_column_reference` | Out-of-range `ORDER BY` / `GROUP BY` ordinal |
| `42P18` | `indeterminate_datatype` | Parameter whose type nothing determines |
| `53100` | `disk_full` | No space for the WAL, the file, or temporary files |
| `53200` | `out_of_memory` | `work_mem` exceeded before spilling, or every cache frame pinned (ADR-0012) |
| `53300` | `too_many_connections` | Server connection limit (M12) |
| `53400` | `configuration_limit_exceeded` | `temp_file_limit` (M9) |
| `54000` / `54001` | `program_limit_exceeded` / `statement_too_complex` | Size and depth limits |
| `54011` / `54023` | `too_many_columns` / `too_many_arguments` | Column and argument limits |
| `55000` | `object_not_in_prerequisite_state` | Read-only open of a database that needs recovery |
| `55006` | `object_in_use` | Database locked by another process (ADR-0005) |
| `55P03` | `lock_not_available` | Busy timeout on the write lock (ADR-0008) |
| `57014` | `query_canceled` | Interrupt or `statement_timeout` (ADR-0010) |
| `57P01` | `admin_shutdown` | Server shutting down (M12) |
| `58030` | `io_error` | OS I/O failure; poisoned handle (ADR-0009) |
| `XX000` | `internal_error` | A broken invariant that is not corruption (a bug) |
| `XX001` / `XX002` | `data_corrupted` / `index_corrupted` | Checksum or structure failure |

A spec that needs a new code adds it to this table, with its PostgreSQL
condition name, and a constant to `sqlstates!` in
`crates/core/src/sqlstate.rs`, in the same change;
`crates/core/tests/sqlstate_docs.rs` fails otherwise.

## Error conventions

- **One error type per crate.** It implements `Debug`, `Display`,
  `std::error::Error` and `hardshell_core::error::SqlStateError`, and is
  `Send + Sync + 'static`. An enum's `sqlstate()` is an exhaustive `match`
  with no `_ =>` arm; a struct that stores its code (`ParseError`) returns
  it. Every code is a `SqlState` constant.
- **`Display` is the whole message,** in PostgreSQL's style: one line, a
  lower-case first word, no final period, names in double quotes. It
  includes a foreign cause's text (`could not fsync file "app.hsdb-wal":
  Input/output error`), and `source()` never returns an error whose text
  `Display` already printed. The facade's `Error` follows the same rule.
- **Facts are fields; notes say what was in progress.** A corrupt page's
  number, `OsError::Io`'s operation and path, and `ParseError`'s span are
  typed fields. "while replaying the WAL" is a note added with
  `ErrorContext::with_context`. Notes follow the message after `"; "`,
  innermost first, and never change the SQLSTATE.
- **A crate that receives notes** adds a `Context(ContextError)` variant and
  `From<ContextError>`. That variant, like any variant that wraps another
  error, forwards `Display`, `source()` and `sqlstate()`.
  `ContextError::error()` reaches the wrapped error for a downcast.
- **Reclassifying is a variant, never a note.** A truncated read is `XX001`
  in a page and `08P01` in a wire message: each crate maps the inner error
  into its own variant with its own code.
- **Notes are bounded by code, not by input.** No note per level of a
  recursion or per item of a loop over input. A dynamic note is built in the
  closure, so the success path never formats.
- **No note on a `ParseError` the facade reports,** since its span becomes
  `position`. A `ParseError` from re-parsing stored text (a view body) may
  take one.
- **`XX000` is for bugs only.** Malformed input gets its input's code
  (`42601`, `22P02`, `08P01`, `XX001`), never a panic.
- **Codes in docs.** Write a SQLSTATE as its own code span. The docs test
  reads a five-character word with a digit as a SQLSTATE when it is a whole
  code span, or anywhere when it starts with a PostgreSQL class, and such a
  word needs a constant. Write other numbers like that with a separator
  (16,384) or list them in the test's `NOT_SQLSTATES`. To discuss a
  PostgreSQL code the engine does not raise, use its condition name.

## Invariants

- Nothing in `core` panics on input. `Reader` never indexes past its buffer;
  `varint` rejects encodings longer than 10 bytes and non-minimal ones.
- `crc32c(b"123456789") == 0xE306_9283` (the standard check value), and
  `Crc32c` chained over split input equals `crc32c` over the whole.
- `Pcg32` output is identical across targets for the same seed (unit-tested
  against fixed vectors).
- `FastHashMap` iteration order depends only on insertion history, never
  on per-process random state. Code whose output order matters (catalog
  listings, `EXPLAIN`, dumps) sorts explicitly anyway.
- `log` never allocates when no sink is installed, and never prints.
- Every `SqlState` is one of the named constants; an invalid code fails the
  build.
- A note never changes an error's SQLSTATE.

## Limits

`core` has no SQL-visible limits. `csv::Reader` has `max_field_bytes`
(default 64 MiB) and `max_fields` (default 1,600, the column limit); past
either it returns an error the facade reports as `22P04`.

## Tests

- `sqlstate`: every constant is five characters from `[0-9A-Z]` whose first
  two are its class; codes are unique; the validator rejects anything else;
  the `Display` and `Debug` forms; constants as `match` patterns.
- `tests/sqlstate_docs.rs`: every SQLSTATE in `docs/` has a constant; the
  table above lists exactly the constants, in `SqlState::ALL`'s order, each
  name matching its constant and the constant's doc comment; look-alike
  cases pin the scanner's rule.
- `error`: a note keeps the SQLSTATE; notes read innermost first; `source()`
  is the wrapped error's; the note's closure runs only on `Err`; the wrapped
  error is reachable by downcast. `tests/error_context.rs` runs the
  conventions across crate-like layers, from outside the crate.
- `crc32c` against published test vectors (RFC 3720 appendix B.4) and a
  bit-at-a-time reference implementation in the test module.
- Property tests: `varint` round-trips every `u64`/`i64` sample and rejects
  overlong forms; `csv` write-then-read round-trips arbitrary fields
  (including quotes, delimiters, CR, LF and empty strings); `Reader` never
  panics on random buffers and offsets.
- `Pcg32` against fixed reference outputs.
- `civil` against known timestamps, including leap years and 2038.
- Fuzz target `csv` (`docs/04-quality.md`).

## Open questions

- Hardware CRC-32C (SSE4.2 / ARMv8 CRC) needs `unsafe` intrinsics, which
  ADR-0013 forbids outside `hardshell-os`. Decide at M9 only if checksums
  exceed ADR-0009's 5% trigger.
