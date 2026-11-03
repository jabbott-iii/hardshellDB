# Subsystem: `hardshelldb` (the facade)

The crate users depend on: `Database`, `Connection`, `Statement`, `Rows`,
`Transaction`, values and errors. It ties the parser, binder, planner,
executor, catalog and storage together along the statement lifecycle in
`docs/01-architecture.md`, and re-exports what users need so they never name
the inner crates. Nothing is built yet; M6 builds API v0 (open, execute,
query, prepare, transactions), M7 to M10 extend it, and M11 stabilizes it
for 1.0. The CLI (`cli.md`) and the server (`server-and-wire.md`) use only
this crate's public API.

## Contents

| Module | Provides |
| :- | :- |
| `database` | `Database`, `OpenOptions`, `close`, `checkpoint`, `backup_to` (M11) |
| `connection` | `Connection`: `execute`, `execute_batch`, `query`, `prepare`, `transaction`, settings, notices |
| `statement` | `Statement`: a prepared, re-plannable statement with parameter and column metadata |
| `rows` | `Rows`, `Row`, `Column`, `RowIndex` |
| `value` | Re-exports `Value` and its date/time/numeric types; `ToValue`, `FromValue`, `params!` |
| `transaction` | `Transaction` guard: commit, rollback on drop; savepoints (M10) |
| `copy` | `copy_in` / `copy_out` of CSV through `hardshell-core::csv` (M8, used by `\copy`) |
| `error` | `Error`, `SqlState` re-export, conversions from every inner error |
| `log` | `set_log_sink`, `LogRecord`, `Level` |
| `interrupt` | `InterruptHandle` |
| `script` | `split_statements` and `is_complete`, re-exported from `hardshell-sql::split` for the CLI |

## Key types

```rust
#[derive(Clone)]
pub struct Database { shared: Arc<Shared> }            // Send + Sync

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Database, Error>;
    pub fn open_with(path: impl AsRef<Path>, opts: OpenOptions) -> Result<Database, Error>;
    pub fn open_in_memory() -> Result<Database, Error>;    // MemVfs; gone when dropped
    pub fn connect(&self) -> Result<Connection, Error>;
    pub fn checkpoint(&self) -> Result<CheckpointReport, Error>;
    pub fn close(self) -> Result<(), Error>;            // checkpoint, delete the WAL, unlock
}

pub struct Connection { /* Send, not Sync */ }

impl Connection {
    pub fn execute(&mut self, sql: &str, params: &[Value]) -> Result<u64, Error>;   // rows affected
    pub fn execute_batch(&mut self, sql: &str) -> Result<(), Error>;               // many statements, no params
    pub fn query(&mut self, sql: &str, params: &[Value]) -> Result<Rows<'_>, Error>;
    pub fn prepare(&mut self, sql: &str) -> Result<Statement<'_>, Error>;
    pub fn transaction(&mut self) -> Result<Transaction<'_>, Error>;               // BEGIN (deferred)
    pub fn transaction_immediate(&mut self) -> Result<Transaction<'_>, Error>;     // takes the write lock now
    pub fn interrupt_handle(&self) -> InterruptHandle;                            // Send + Sync + Clone
    pub fn take_notices(&mut self) -> Vec<Notice>;                                // e.g. identifier truncated
    pub fn in_transaction(&self) -> TxnStatus;                                    // Idle, InTransaction, Failed
}

pub struct Rows<'c> { /* borrows its Connection mutably */ }

impl<'c> Rows<'c> {
    pub fn columns(&self) -> &[Column];                  // name, SqlType
    pub fn next(&mut self) -> Result<Option<Row<'_>>, Error>;
}

impl Row<'_> {
    pub fn get<T: FromValue>(&self, idx: impl RowIndex) -> Result<T, Error>;   // by ordinal or name
    pub fn get_ref(&self, idx: impl RowIndex) -> Result<ValueRef<'_>, Error>;  // zero-copy
}

pub struct Error {
    state: SqlState,
    message: String,
    detail: Option<String>,
    hint: Option<String>,
    position: Option<u32>,                               // 1-based character position, as PostgreSQL
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl Error {
    pub fn sqlstate(&self) -> SqlState;
    pub fn is_retryable(&self) -> bool;                  // 40001 and 55P03
}
```

## Rules

- **Strict conversions:** `FromValue` accepts only lossless conversions by
  type, never by value: `i64` reads `SMALLINT`, `INTEGER` and `BIGINT`;
  `i32` reads `SMALLINT` and `INTEGER` but not `BIGINT`, even when the
  value fits. A mismatch is `42804`. `NULL` into a non-`Option` type is
  `22004`.
- **`ToValue`** is implemented for `bool`, the integer and float types,
  `&str`, `String`, `&[u8]`, `Vec<u8>`, the date/time and `Numeric` types,
  `[u8; 16]` as `UUID`, and `Option<T>` (`None` is `NULL`).
- **`params!`** is a `macro_rules!` macro producing `&[Value]`; `params![]`
  is empty.
- **Autocommit and transactions** behave as `docs/01-architecture.md`
  describes. Dropping a `Transaction` without `commit` rolls back; a
  rollback error on drop is logged, never panicked.
- **`Rows` borrows its connection**, so one statement runs at a time per
  connection, enforced by the compiler.
- **Statement cache:** `prepare` caches by SQL text per connection (default
  64 entries, LRU); `Statement` re-plans if the schema version changed.
- **Errors map to PostgreSQL fields:** the parser's byte span becomes
  `position`, counted in characters from 1, as PostgreSQL's
  `ErrorResponse` does.
- **Logging:** `set_log_sink(Box<dyn Fn(&LogRecord) + Send + Sync>)`
  installs one process-wide sink, once: it fills the slot in
  `hardshell_core::log`, which is never replaced, so a second call fails
  (M6.5 picks how it reports that). The library never prints.
- **No panics across the API:** every failure is an `Error`. A panic inside
  the engine is a bug; it is not caught, but the poisoned-handle rule
  (ADR-0009) and the WAL keep the file safe.

## Stability

- Before M11 the API may change at any minor version.
- At 1.0 (M11) it follows SemVer. `Value`, `SqlType`, `Error` kinds and
  option enums are `#[non_exhaustive]`, so adding a type (for example
  `JSONB` from the backlog) is not a breaking change.
- Every public item has a doc comment with an example where useful
  (`missing_docs` is denied in CI).

## Limits

| Limit | Default | When exceeded |
| :- | :- | :- |
| Connections per `Database` | unlimited (each holds a small struct) | — |
| Statement cache per connection | 64 | Least recently used is evicted |
| Parameters per statement | 65,535 | `54000` |

## Tests

- Doc tests for every public example (they run in `cargo test`).
- The SQL logic test runner lives here (`tests/slt.rs`, over
  `tests/slt/**/*.slt`), and so does the crash campaign (`tests/crash.rs`).
- Threading tests: many connections reading while one writes; `Database`
  shared across threads; interrupts from another thread.
- Compile-fail tests (as doc tests with `compile_fail`): using a
  `Connection` while a `Rows` borrows it.
- API snapshot: from M11, `cargo xtask api-diff` (or a committed public-API
  listing) catches accidental breaking changes. The tool is an open
  question.

## Open questions

- Notices: `take_notices` polling, or a callback per connection?
- An async-friendly wrapper (ADR-0010 says no async API; a thin
  `spawn_blocking`-style example in the docs may be enough).
- How to produce the public-API listing for the M11 stability check without
  third-party tools (rustdoc's JSON output is unstable).
