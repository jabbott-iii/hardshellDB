# Subsystem: `hardshell-core`

Foundation types used by every other crate. No `unsafe`, no OS calls beyond
`std` time, no dependencies (layer 0). Nothing is built yet; M0 builds the
modules the guardrails need (errors, `crc32c`, `varint`, `rng`, `testing`,
`cli`, `log`), and later milestones add to it only when two or more crates
need the same thing.

## Contents

| Module | Provides |
| :- | :- |
| `sqlstate` | `SqlState` (five ASCII bytes), named constants for every code the engine raises, `class()`, `Display` as the five-character code |
| `error` | `CoreError` and the `ErrorContext` helper for adding "while doing X" context to errors |
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
pub struct SqlState([u8; 5]);              // always ASCII digits or upper-case letters

impl SqlState {
    pub const SYNTAX_ERROR: SqlState = SqlState(*b"42601");
    pub const FEATURE_NOT_SUPPORTED: SqlState = SqlState(*b"0A000");
    pub const DATA_CORRUPTED: SqlState = SqlState(*b"XX001");
    pub const IO_ERROR: SqlState = SqlState(*b"58030");
    // ... one constant per code in the table below
    pub fn class(self) -> [u8; 2];
    pub fn as_str(&self) -> &str;
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
| `2201W` / `2201X` | invalid row count in `LIMIT` / `OFFSET` | Negative `LIMIT` or `OFFSET` |
| `22021` | `character_not_in_repertoire` | Invalid UTF-8 (ADR-0014) |
| `22023` | `invalid_parameter_value` | Bad type modifier, page size or setting value |
| `22P02` | `invalid_text_representation` | Bad input for a non-date/time type |
| `22P04` | `bad_copy_file_format` | Malformed CSV for `\copy` / `copy_in` |
| `23502` / `23503` / `23505` / `23514` | not-null / foreign-key / unique / check violation | Constraint checks |
| `25P02` | `in_failed_sql_transaction` | Statement after an error inside `BEGIN` |
| `25P03` | `idle_in_transaction_session_timeout` | Server session idle in a transaction too long (M12) |
| `2BP01` | `dependent_objects_still_exist` | `DROP` without `CASCADE` |
| `3B001` | `invalid_savepoint_specification` | Unknown savepoint (M10) |
| `3D000` / `3F000` | `invalid_catalog_name` / `invalid_schema_name` | Wrong database name at server startup; schema other than `public` |
| `40001` | `serialization_failure` | Writing on a stale snapshot (ADR-0008) |
| `42601` | `syntax_error` | Parser |
| `42P01` / `42703` / `42883` / `42704` | undefined table / column / function / object | Binder; unknown setting |
| `42701` / `42710` / `42P07` | duplicate column / object / table | DDL |
| `42702` / `42725` | ambiguous column / function | Binder |
| `42803` | `grouping_error` | Aggregates in `WHERE`, ungrouped columns |
| `42804` | `datatype_mismatch` | Binder, `Row::get` |
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

A spec that needs a new code adds it to this table in the same change.

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

## Limits

`core` has no SQL-visible limits. `csv::Reader` has `max_field_bytes`
(default 64 MiB) and `max_fields` (default 1,600, the column limit); past
either it returns an error the facade reports as `22P04`.

## Tests

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
