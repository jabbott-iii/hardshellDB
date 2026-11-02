# 01 — Architecture

## Principles

1. **Layered crates, one-way dependencies.** Lower layers never know about
   higher ones. `cargo xtask layers` enforces the matrix below.
2. **Storage knows bytes, not SQL.** The pager, WAL and B+trees store and
   compare byte strings. SQL types become bytes through one encoding layer
   in `hardshell-types` (ADR-0006). This keeps the storage engine testable
   against a plain `BTreeMap<Vec<u8>, Vec<u8>>` model.
3. **The OS is quarantined.** Every file, lock and sync goes through the VFS
   trait in `hardshell-os`. Nothing else opens a file, and only
   `hardshell-os` may have a `target_os` cfg. The same trait gives tests an
   in-memory file system and a fault-injecting one.
4. **Every byte from outside is hostile.** SQL text, database files, WAL
   files, CSV input and wire messages are parsed with bounds, checked
   arithmetic and explicit limits. Malformed input is an error with a
   SQLSTATE code, never a panic.
5. **Durability is a design property, not a test result.** Uncommitted data
   never reaches the main file; committed data is always in the WAL or the
   main file, checksummed (ADR-0007, ADR-0009).
6. **Plans never change answers.** The optimizer may only choose between
   equivalent plans. Every rewrite has a test that both plans return the
   same rows.
7. **The caller owns the threads.** The engine is synchronous (ADR-0010),
   starts no thread unless configured to, and never prints. It holds only
   two pieces of process-wide state: the optional log sink, and the
   registry of open database files that stops one process from opening
   the same file twice (ADR-0005).

## Layers

A crate may only depend on crates in lower layers, and only on the specific
crates its row in the dependency matrix allows.

| Layer | Crates |
| :- | :- |
| 7 — applications | `hardshell-cli`, `hardshell-server`, `xtask`, `hardshell-bench` |
| 6 — facade | `hardshelldb` (`Database`, `Connection`, `Statement`, `Rows`, errors) |
| 5 — execution | `hardshell-exec` |
| 4 — planning | `hardshell-plan` |
| 3 — schema | `hardshell-catalog` |
| 2 — storage and language | `hardshell-storage`, `hardshell-sql` |
| 1 — OS, values and protocol | `hardshell-os`, `hardshell-types`, `hardshell-wire` |
| 0 — foundations | `hardshell-core` |

## Dependency matrix

A crate may depend on the workspace crates listed in its row and nothing else.
No crate may depend on a third-party crate unless the dependency policy
approves it (the approved set is empty).

| Crate | Allowed workspace dependencies |
| :- | :- |
| `hardshell-core` | — |
| `hardshell-os` | `core` |
| `hardshell-types` | `core` |
| `hardshell-wire` | `core` |
| `hardshell-storage` | `core`, `os` |
| `hardshell-sql` | `core` |
| `hardshell-catalog` | `core`, `types`, `storage` |
| `hardshell-plan` | `core`, `types`, `sql`, `catalog` |
| `hardshell-exec` | `core`, `os`, `types`, `storage`, `catalog`, `plan` |
| `hardshelldb` (facade) | `core`, `os`, `types`, `storage`, `sql`, `catalog`, `plan`, `exec` |
| `hardshell-cli` | `core`, `hardshelldb` |
| `hardshell-server` | `core`, `wire`, `hardshelldb` |
| `xtask` | `core`, `os`, `types`, `storage`, `sql`, `wire` (the crates whose parsers it fuzzes) |
| `hardshell-bench` | any workspace crate |

Notes:

- `hardshell-catalog` stores column defaults, `CHECK` expressions and view
  bodies as SQL text. `hardshell-plan` parses them when it binds, so the
  catalog never depends on the parser.
- `hardshell-exec` depends on `os` only for temporary files used when
  sorts and hash tables spill (M9), through the VFS.
- The crash campaign has two levels: page-level transactions in
  `crates/storage/tests/crash.rs` (M2.6) and SQL workloads in
  `crates/hardshelldb/tests/crash.rs` (M6.7). `cargo xtask crash` runs
  those test binaries rather than linking the facade.

## Workspace layout

```
.
├── Cargo.toml                # [workspace], [workspace.lints], [workspace.package], profiles
├── rust-toolchain.toml       # pinned stable toolchain + rustfmt, clippy
├── clippy.toml               # allow-unwrap-in-tests = true, allow-expect-in-tests = true
├── .cargo/config.toml        # alias: xtask
├── crates/
│   ├── core/                 # hardshell-core
│   ├── os/                   # hardshell-os
│   ├── types/                # hardshell-types
│   ├── wire/                 # hardshell-wire (M12)
│   ├── storage/              # hardshell-storage
│   ├── sql/                  # hardshell-sql
│   ├── catalog/              # hardshell-catalog
│   ├── plan/                 # hardshell-plan
│   ├── exec/                 # hardshell-exec
│   └── hardshelldb/          # hardshelldb (facade)
├── apps/
│   ├── cli/                  # hardshell-cli, binary `hardshell`
│   └── server/               # hardshell-server, binary `hardshelld` (M12)
├── xtask/
├── bench/                    # hardshell-bench
├── tests/
│   ├── slt/                  # SQL logic tests (*.slt), run by the facade's `slt` test
│   └── fixtures/             # committed database files per format version
├── fuzz/corpus/              # seed inputs per fuzz target
└── docs/
```

No crate is named `debug`: the root `.gitignore` ignores `debug`.

`.cargo/config.toml`:

```toml
[alias]
xtask = "run --profile xtask --package xtask --"
```

Workspace lints (`Cargo.toml`); every member uses `[lints] workspace = true`:

```toml
[workspace.lints.rust]
unsafe_code = "deny"              # only hardshell-os may override, and only by ADR (ADR-0013)
unsafe_op_in_unsafe_fn = "deny"
missing_docs = "warn"             # -D in CI
unreachable_pub = "warn"

[workspace.lints.clippy]
undocumented_unsafe_blocks = "deny"
multiple_unsafe_ops_per_block = "deny"
unwrap_used = "deny"
print_stdout = "deny"             # the CLI and server allow it at their crate roots, with a comment
print_stderr = "deny"
dbg_macro = "deny"
todo = "deny"
exit = "deny"
```

`unsafe_code` is `deny` rather than `forbid` so that `hardshell-os` can add
`#![allow(unsafe_code)]` at its root if an ADR approves it. `cargo xtask
unsafe-audit` checks that the attribute appears nowhere else.

## The life of a statement

Everything runs on the caller's thread. This is the shape the facade
implements in M6; names are the planned ones.

```rust
impl Connection {
    pub fn query(&mut self, text: &str, params: &[Value]) -> Result<Rows<'_>, Error> {
        let stmt = sql::parse_one(text, &self.limits)?;          // AST with spans; 42601 on error
        let txn = self.transaction_for(&stmt)?;                  // autocommit, or the open BEGIN
        let snap = txn.snapshot();                               // main file + WAL up to a commit
        let schema = self.db.schema_at(&snap)?;                  // Arc<Schema>, cached by version
        let bound = plan::bind(&stmt, &schema, param_types(params))?; // names, types, casts; 42xxx
        let logical = plan::optimize(bound, &schema)?;           // rewrites + cost-based choices
        let physical = plan::physical(&logical, &schema)?;
        let ctx = exec::Context::new(&self.settings, self.interrupt.clone()); // work_mem, cancel
        let cursor = exec::open(physical, txn, params, ctx)?;    // pull-based (ADR-0012)
        Ok(Rows::new(cursor))                                    // each next() pulls one row;
    }                                                            // drop or exhaustion ends the
}                                                                // statement (autocommit: commit)
```

Rules:

- **One statement at a time per connection.** A `Rows` borrows its
  `Connection` mutably, so the compiler enforces this.
- **Autocommit:** a statement outside `BEGIN` runs in its own transaction.
  A read-only one commits when its `Rows` is exhausted or dropped; a
  writing one commits when it completes, and rolls back if it fails.
- **Errors inside an explicit transaction** put it in the failed state, as
  in PostgreSQL: every later statement fails with `25P02`
  (`in_failed_sql_transaction`) until `ROLLBACK` (or `ROLLBACK TO
  SAVEPOINT`, M10).
- **Prepared statements** cache the physical plan keyed by the schema
  version. DDL bumps the version, and the next execution re-plans.
- **Parameters** are typed by the binder from context (`$1` compared with a
  `BIGINT` column is `BIGINT`), or by the caller's value when context gives
  nothing.
- **Cancellation** is cooperative (ADR-0010): `InterruptHandle::interrupt()`
  from any thread makes the running statement fail with `57014`.

## Data flow

```
SQL text ─► sql (lex, parse) ─► AST ─► plan::bind ◄── catalog (Schema at the snapshot)
                                          │
                                          ▼
                          logical plan ─► optimize ─► physical plan
                                                          │
                                                          ▼
       caller ◄── Rows ◄── exec operators (scan, filter, join, aggregate, sort, DML)
                                   │ keys and rows as bytes (types::encoding)
                                   ▼
                     storage: B+tree cursors ─► pager / buffer pool
                                   │                      │ misses: read page, check CRC
                                   │ commit: frames        ▼
                                   ▼               main file (pages)
                               WAL file  ──── checkpoint ───┘
                                   │
                                   ▼
                           os::Vfs (std file I/O, locks, sync) ─► OS
```

## Shared state and threading

`Database` is an `Arc` around shared state. `Connection`s hold a clone.

| State | Guarded by | Notes |
| :- | :- | :- |
| Buffer pool frames | A mutex per shard (start with one shard; shard when contention shows in benchmarks) | Pin counts keep a frame resident while a cursor uses it |
| WAL index (page → newest frame per commit) | `RwLock` | Readers look up by snapshot; the writer appends |
| Write lock | `Mutex` + `Condvar` | At most one write transaction (ADR-0008); waits up to the busy timeout |
| Reader marks (oldest snapshot in use) | `Mutex` | Bounds how far a checkpoint may copy (ADR-0007) |
| Schema cache | `RwLock<Arc<Schema>>` | Replaced on DDL commit |
| Poisoned flag | `AtomicBool` | Set on any sync failure (ADR-0009) |

| Thread | Owns | Talks to others via |
| :- | :- | :- |
| Caller threads (any number) | Their `Connection`, its open transaction, its running statement | The shared state above |
| Checkpointer (optional, off by default) | Nothing; runs checkpoints when the WAL passes its threshold | The shared state; without it, the committing thread checkpoints |
| Server connection threads (M12) | One thread per open connection, up to `max_connections` (ADR-0010) | The same `Database` handle |

- No thread is started by `Database::open` unless
  `OpenOptions::background_checkpoint(true)` is set.
- Lock order, to avoid deadlocks: write lock → schema cache → WAL index →
  reader marks → buffer pool shard. Code that takes two of them takes them
  in this order; a debug-build lock-order checker asserts it (M2).

## Feature flags

| Feature | Crates | Enables |
| :- | :- | :- |
| `testing` | `os`, `storage`, `hardshelldb` | The fault-injecting VFS, crash points, test-only hooks |
| (none) | — | Release behavior: no fault injection compiled in |

`cargo xtask release-no-testing` fails if `testing` is enabled in the
non-dev dependency tree of `hardshell-cli` or `hardshell-server`. A member
turns `testing` on only from a dev-dependency, never from a normal
dependency.

## Files and configuration

- A database is `<name>` (convention: `.hsdb`) plus `<name>-wal` while
  open. `Database::open` creates the file if `OpenOptions::create(true)`
  (the default), syncs it and its directory, then takes the exclusive lock
  (ADR-0005).
- Temporary files for spilling (M9) live in `OpenOptions::temp_dir`
  (default: the OS temp directory), are created through the VFS, and are
  deleted when their operator ends.
- `OpenOptions` (all optional):

  | Option | Default | Notes |
  | :- | :- | :- |
  | `create` | `true` | Create the file if missing |
  | `read_only` | `false` | Shared lock; no WAL writes; fails with `55000` if recovery is needed |
  | `page_size` | 4096 | Creation only (ADR-0006) |
  | `cache_size` | 64 MiB | Buffer pool size |
  | `busy_timeout` | 5 s | Wait for the write lock (ADR-0008) |
  | `wal_autocheckpoint` | 16 MiB | Checkpoint when the WAL passes this |
  | `durability` | `Full` | `Full` syncs the WAL at every commit. `Relaxed` syncs only at checkpoints: a crash can lose recent commits but never corrupts |
  | `work_mem` | 64 MiB | Per-query memory budget (ADR-0012); also `SET work_mem` |
  | `max_statement_bytes` | 16 MiB | Parser input limit (ADR-0011) |
  | `temp_file_limit` | Decided in M9.4 | Spill bytes per query; past it `53400` |

- Session settings use `SET name = value` / `SHOW name`: `work_mem`,
  `statement_timeout`, `busy_timeout`, `application_name`. Unknown names
  are `42704` (`undefined_object`).

## Errors and logging

- Each crate defines its own error enum implementing `Display` and
  `std::error::Error`. The facade's `hardshelldb::Error` wraps them and
  exposes `sqlstate() -> SqlState`, `message()`, and optional `detail()`,
  `hint()` and `position()` (a 1-based character position in the
  statement, as PostgreSQL counts it), mirroring PostgreSQL's
  `ErrorResponse` fields.
- `SqlState` and the code table live in `hardshell-core::sqlstate`, so every
  crate can name a code.
- The library logs through `hardshell_core::log` macros to a sink the host
  installs with `hardshelldb::set_log_sink`. With no sink, logging costs one
  atomic load and does nothing.

## Public API sketch (embedded)

```rust
use hardshelldb::{Database, OpenOptions, params};

fn main() -> Result<(), hardshelldb::Error> {
    let db = Database::open_with("app.hsdb", OpenOptions::new().cache_size(128 << 20))?;
    let mut conn = db.connect()?;

    conn.execute(
        "CREATE TABLE users (id BIGINT PRIMARY KEY, name TEXT NOT NULL, born DATE)",
        params![],
    )?;
    conn.execute("INSERT INTO users VALUES ($1, $2, $3)", params![1_i64, "Ada", None::<&str>])?;

    let mut stmt = conn.prepare("SELECT name FROM users WHERE id = $1")?;
    let mut rows = stmt.query(params![1_i64])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(0)?;          // 42804 if the column is not text
        let _ = name;
    }
    drop(rows);

    let tx = conn.transaction()?;                // BEGIN; dropped without commit => ROLLBACK
    tx.execute("UPDATE users SET name = $1 WHERE id = $2", params!["Ada L.", 1_i64])?;
    tx.commit()?;
    Ok(())
}
```

- `params!` is a `macro_rules!` macro building a `&[Value]` from anything
  implementing `ToValue`; `row.get::<T>` uses `FromValue`, with `Option<T>`
  for nullable columns (a `NULL` read as non-`Option` is `22004`,
  `null_value_not_allowed`).
- `Database: Send + Sync + Clone`; `Connection: Send`; `Rows` borrows its
  connection.
- `Database::close(self)` checkpoints, removes the WAL and releases the
  lock, reporting any error; dropping does the same but can only log errors.
- `conn.transaction()` begins a deferred transaction: it takes the write
  lock at its first write, which fails with `40001` if another commit
  happened since its snapshot (ADR-0008). `conn.transaction_immediate()`
  takes the write lock at `BEGIN`. The SQL spelling of the immediate form
  (for example `BEGIN READ WRITE`) is decided in M6.5.
- `conn.execute_batch(text)` runs several statements without parameters;
  `conn.take_notices()` returns notices (for example `DROP ... IF EXISTS`
  of a missing object, or a truncated identifier), as PostgreSQL's
  `NoticeResponse` would.
- `hardshelldb` also re-exports what the CLI needs from layers it may not
  depend on: statement splitting (`split_statements`) and schema
  introspection for `\d` before `information_schema` exists (M10.6).
