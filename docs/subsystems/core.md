# Subsystem: `hardshell-core`

Foundation types used by every other crate. No `unsafe` (its one test
binary with a counting allocator aside, ADR-0017), no OS calls beyond
`std` time and `testing`'s read of `HARDSHELL_SEED`, no dependencies
(layer 0). M0.2 built the errors (`sqlstate`, `error`), M0.3 the checksums,
varints, hashing and random numbers (`crc32c`, `varint`, `hash`, `rng`),
and M0.4 the property-test harness and logging (`testing`, `log`). M8.1
adds the argument parser and CSV codec (`cli`, `csv`); otherwise later
milestones add to it only when two or more crates need the same thing.

## Contents

| Module | Provides |
| :- | :- |
| `sqlstate` | `SqlState` (five ASCII bytes), one named constant per row of the table below plus `SqlState::ALL`, `class()`, `as_str()`, `Display` as the five-character code |
| `error` | `SqlStateError` (the `sqlstate()` every error type implements), `ContextError` and the `ErrorContext` helper for adding "while doing X" notes without changing the SQLSTATE; `CoreError`, this crate's own error: today the varint decoder's `VarintTruncated`, `VarintOverlong` and `VarintOverflow`, all `XX001` and without offsets (a caller reclassifies it and adds where the varint was) |
| `bytes` | Not built yet: arrives with the first decoder that needs it, bringing `CoreError::Truncated { at, wanted }`. `Reader` (bounds-checked little-endian reads with an offset, every read returns `Result`) and `Writer` (appends to a reused `Vec<u8>`) for every decoder and encoder in the workspace |
| `crc32c` | CRC-32C (Castagnoli, reflected polynomial `0x82F63B78`), table-driven slicing-by-8 over tables built by a `const fn`; `crc32c(&[u8])`, and `crc32c_append(crc, &[u8])`, which continues a finished checksum: the WAL's chained checksum and checksums over data in pieces (ADR-0009) |
| `varint` | Unsigned LEB128 for `u64`: `encode_u64` appends to a `Vec<u8>`; `decode_u64` returns the value and the bytes used, reads at most 10 bytes, and accepts only the shortest encoding (truncated, overlong and over-64-bit forms are errors) |
| `hash` | `fnv1a64` (fixed by its definition, so its values may be stored or printed); `FastHasher` (a folded multiply per 64-bit word, then a finishing fold) with `FastHashMap` / `FastHashSet` aliases: deterministic, non-cryptographic, values never stored. Neither resists chosen keys: use `RandomState` where a remote party picks them |
| `rng` | `Pcg32`: `new(seed, stream)` as the reference's `pcg32_srandom_r`, `next_u32`, `next_u64` (low half first), `range(bound)` (unbiased; the reference's `pcg32_boundedrand_r` up to `u32::MAX`; panics on 0), `shuffle` (as the reference demo deals cards), `fill_bytes`; not for secrets |
| `log` | Macros `error!`, `warn!`, `info!`, `debug!`, `trace!` (`format!` arguments, the calling module's `module_path!()` as the target), reached as `hardshell_core::log::warn!`; `Level`, `LogRecord` (level, target, unformatted `fmt::Arguments`), `Sink`; one process-wide sink slot, set once by `set_sink` (the facade's `set_log_sink`): a second call gets its sink back as `Err`. With no sink a macro costs one atomic load and evaluates none of its arguments |
| `testing` | Property-test harness `check` and `check_sized` (see `docs/04-quality.md`), `DEFAULT_SEED`, `MAX_SIZE`; the seed from `HARDSHELL_SEED`. `assert_approx!` is not built yet: it arrives with the first float test that needs it |
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreError {                       // XX001 for every variant; no offsets
    VarintTruncated,                       // "invalid varint: truncated"
    VarintOverlong,                        // "invalid varint: overlong encoding"
    VarintOverflow,                        // "invalid varint: more than 64 bits"
}

pub fn crc32c(bytes: &[u8]) -> u32;
pub fn crc32c_append(crc: u32, bytes: &[u8]) -> u32;  // from crc32c(a): the CRC of a, then bytes

pub fn encode_u64(value: u64, out: &mut Vec<u8>);
pub fn decode_u64(input: &[u8]) -> Result<(u64, usize), CoreError>;  // value, bytes used

pub fn fnv1a64(bytes: &[u8]) -> u64;
pub struct FastHasher { state: u64 }       // Default, Debug, Hasher
pub type FastHashMap<K, V> = HashMap<K, V, BuildHasherDefault<FastHasher>>;
pub type FastHashSet<T> = HashSet<T, BuildHasherDefault<FastHasher>>;

#[derive(Clone, Debug)]                    // not Copy: a copy repeats its sequence
pub struct Pcg32 { state: u64, increment: u64 }

impl Pcg32 {
    pub fn new(seed: u64, stream: u64) -> Pcg32;      // 2^63 streams
    pub fn next_u32(&mut self) -> u32;
    pub fn next_u64(&mut self) -> u64;
    pub fn range(&mut self, bound: usize) -> usize;   // 0..bound; panics if bound is 0
    pub fn shuffle<T>(&mut self, items: &mut [T]);
    pub fn fill_bytes(&mut self, bytes: &mut [u8]);
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level { Error, Warn, Info, Debug, Trace }   // most to least severe; as_str() and Display: "warn"

#[derive(Clone, Copy, Debug)]
pub struct LogRecord<'a> { level: Level, target: &'static str, message: fmt::Arguments<'a> }
// level(), target() (`module_path!()` of the call), message() (unformatted)

pub type Sink = Box<dyn Fn(&LogRecord<'_>) + Send + Sync>;  // runs on the logging thread
pub fn set_sink(sink: Sink) -> Result<(), Sink>;          // the first call wins
// error!, warn!, info!, debug!, trace!: `format!` arguments, evaluated only with a sink

pub const DEFAULT_SEED: u64;               // 42: the seed when HARDSHELL_SEED is unset or empty
pub const MAX_SIZE: u32;                   // 100
pub fn check<T: Debug>(name: &str, cases: u32,
    generate: impl Fn(&mut Pcg32) -> T, property: impl Fn(&T) -> Result<(), String>);
pub fn check_sized<T: Debug>(name: &str, cases: u32,
    generate: impl Fn(&mut Pcg32, u32) -> T, property: impl Fn(&T) -> Result<(), String>);

// Planned, not built (see the `bytes` row).
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

- Nothing in `core` panics on input. `varint::decode_u64` reads at most 10
  bytes and never past the varint, and accepts only the shortest encoding,
  so a decoded value re-encodes to exactly the bytes it used. `Reader`,
  when it lands, never indexes past its buffer. The documented panics are
  a caller's bug or a test failing, never engine input: `Pcg32::range(0)`,
  and `testing::check` and `check_sized`, which fail a test by panicking
  (a failing case, no cases, or a malformed `HARDSHELL_SEED`).
- `crc32c(b"123456789") == 0xE306_9283` (the standard check value), and
  `crc32c_append(crc32c(a), b)` is the CRC-32C of `a` then `b`, so a chain
  over split input equals `crc32c` over the whole.
- `Pcg32` gives the reference C library's outputs for the same seed and
  stream on every target, `range` and `shuffle` included (unit-tested
  against the reference demo's output).
- A value's `FastHasher` hash is the same in every run and on every target
  (pinned in tests). `FastHashMap` iteration order therefore depends only on
  the operations applied, never on per-process random state; it can still
  differ between targets and `std` releases (the table's probe groups are
  not the same width everywhere). Code whose output order matters (catalog
  listings, `EXPLAIN`, dumps) sorts.
- `log` never prints. With no sink installed a macro is one atomic load
  (the sink slot's `OnceLock::get`): it evaluates no argument, formats
  nothing and allocates nothing. With one, `core` still allocates nothing:
  the record carries the unformatted arguments. Both are measured with a
  counting allocator (`tests/log_alloc.rs`, ADR-0017).
- The sink is set once per process and never replaced, so reading it takes
  no lock. It runs on the thread that logged; it must not panic or log.
- A property run is a function of its seed, its name and its case count:
  case `i` of the property `name` draws from
  `Pcg32::new(seed ^ fnv1a64(name), i)`, so the seed a failure reports
  replays it, and one case can be rebuilt alone.
- Every `SqlState` is one of the named constants; an invalid code fails the
  build.
- A note never changes an error's SQLSTATE.

## Limits

`core` has no SQL-visible limits. `csv::Reader` has `max_field_bytes`
(default 64 MiB) and `max_fields` (default 1,600, the column limit); past
either it returns an error the facade reports as `22P04`.

A varint is at most 10 bytes (64 bits in groups of seven): a property of
the format, not a setting.

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
- `crc32c` against published test vectors (the CRC catalogue's check value
  and RFC 3720 appendix B.4) and a bit-at-a-time reference implementation
  in the test module, at every length up to 80 bytes and every alignment;
  `crc32c_append` at every split and pair of splits, and over the WAL's
  chain.
- `varint` hand-made vectors, and a differently shaped model decoder that
  `decode_u64` must agree with on every input of up to two bytes, every
  path through the decoder, every tenth byte (with an eleventh it must not
  read) and 200,000 random inputs.
- Property tests: `varint` round-trips every `u64` sample (boundaries and
  100,000 values of uniform bit length) and rejects each one's truncated
  and padded forms; `csv` write-then-read round-trips arbitrary fields
  (including quotes, delimiters, CR, LF and empty strings); `Reader` never
  panics on random buffers and offsets. M0.3's are plain seeded `Pcg32`
  loops, written before `testing::check` existed.
- `testing`: each case draws once from its own stream; sizes run evenly
  from 0 to 100 (pinned at 1, 2, 3, 101 and 1,000 cases and at the largest
  run); the report of the first failing case is pinned word for word, and
  later cases do not run; only a failing case is formatted; panics in the
  generator, the property, or the input's `Debug` form or drop (even with
  a payload whose own drop panics) are reported with their message and
  the seed; shrinking finds the smallest failing size of the same case, at
  both ends of its scan (0 and one below the first failing size) and when
  failure does not grow with size; seeds parse as decimal or `0x` hex
  digits only. `tests/testing_seed.rs` runs a failing `check` and a
  failing `check_sized` in a child process with `HARDSHELL_SEED` set,
  unset, empty and malformed, and checks that the seed each reports
  replays the same case and input, and that the panic points at the
  caller.
- `log`: levels order and print by name; `tests/log_sink.rs` checks each
  macro's level, target (the calling module) and message, that arguments
  are evaluated once, that the sink runs on the logging thread, and that a
  second sink is refused; `tests/log_alloc.rs` counts allocations with and
  without a sink.
- `Pcg32` against the reference demo's first round for seed 42 and stream
  54 (six outputs, 65 coin flips, 33 dice rolls, a 52-card shuffle);
  `range` rejects the biased outputs on both its paths, and matches the
  reference at the edge of its rejection threshold on both.
- `fnv1a64` against the vectors of `test_fnv.c`, the FNV reference test
  suite. `FastHasher`: pinned values; inputs of different lengths differ,
  every tail length of 0 to 7 bytes included;
  page numbers, strided and high-bit integers, pairs and names spread over
  buckets and tags as random hashes would.
- `civil` against known timestamps, including leap years and 2038.
- Fuzz targets `varint` (M0.6) and `csv` (`docs/04-quality.md`).

## Open questions

- Level filtering: once a sink is installed, every macro at every level
  evaluates its arguments and calls the sink, which filters. A maximum
  level stored with the sink would keep a filtered `trace!` at one load and
  a compare. Decide with `set_log_sink`'s signature at M6.5, or earlier if
  logging shows up in a profile.
- Hardware CRC-32C (SSE4.2 / ARMv8 CRC) needs `unsafe` intrinsics, which
  ADR-0013 forbids outside `hardshell-os`. Decide at M9 only if checksums
  exceed ADR-0009's 5% trigger.
