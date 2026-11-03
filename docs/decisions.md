# Architecture decision records

New ADRs are appended at the end using the template below; `/record-decision`
does this for you. Never edit an accepted ADR's decision. Supersede it with a
new ADR and set the old one's status to `Superseded by ADR-NNNN`.

An ADR is `Accepted` only when the user made or approved the decision.
Everything else is `Proposed` until the user says otherwise. ADR-0001 to
ADR-0004 record the user's answers when the project was planned; the rest are
the planning proposals the other docs build on, and the user should accept,
change or reject each before the roadmap task that depends on it starts.

```markdown
## ADR-NNNN — <title>

- **Status:** Proposed | Accepted | Rejected | Superseded by ADR-NNNN
- **Date:** YYYY-MM-DD
- **Context:** <forces at play, constraints, what prompted this>
- **Decision:** <what we will do>
- **Consequences:** <good and bad results, follow-up work>
- **Alternatives considered:** <option — why not>
- **Revisit when:** <a concrete trigger>
```

---

## ADR-0001 — Targets: 64-bit Linux, macOS and Windows

- **Status:** Accepted
- **Date:** 2026-11-01
- **Context:** An embedded database's hardest code is its file I/O: durable
  writes, locking, and what the OS promises after a crash. Every target adds
  a set of fsync, locking and path rules to get right and test. WASM and
  mobile add more constraints still (no threads or restricted threads, no
  real file locks, browser storage APIs).
- **Decision:**
  - Support Linux (x86_64 and aarch64, glibc), macOS 12+ (Apple silicon and
    Intel) and Windows 10+ (x86_64 and aarch64, MSVC).
  - Nothing else. 32-bit and big-endian targets are not supported.
  - Linux comes first in every milestone (CI and agents run there), then
    macOS, then Windows.
- **Consequences:**
  - Plain `std` threads and positional file I/O work on every target.
  - 64-bit only lets the code use `usize` for in-memory sizes freely; the
    file format still writes fixed-width little-endian integers, so files
    are portable between the targets.
  - A WASM or mobile port later would be a major project (backlog).
- **Alternatives considered:** WASM as a second tier. Rejected for now: it
  forces a storage abstraction over browser APIs and a threadless build
  before the core is stable.
- **Revisit when:** a concrete user needs an in-browser or mobile build.

## ADR-0002 — Build it ourselves; zero third-party crates

- **Status:** Accepted
- **Date:** 2026-11-01
- **Context:** The project follows the same rule as Nadir: own the whole
  stack, and take a dependency only when building it is unreasonable. For a
  database this also means no transitive supply-chain risk in the code that
  holds users' data.
- **Decision:**
  - The approved third-party crate set is empty.
  - Exceptions follow the test and process in
    `docs/02-dependency-policy.md`, and require the user's approval.
  - `cargo xtask deps` enforces this from M0.
- **Consequences:**
  - We write the SQL parser, storage engine, numeric and date/time types,
    CSV codec, test harnesses, fuzzer and benchmark harness ourselves.
  - The testing harnesses are part of the project, not borrowed.
  - The server's TLS and password hashing (M12) will hit the policy's
    "specialist correctness" criterion and need an explicit decision.
- **Alternatives considered:** a curated allowlist from day one (for
  example a crypto crate). Rejected by the user: nothing before M12 needs
  one.
- **Revisit when:** a switch trigger in the dependency register fires.

## ADR-0003 — Embedded library and CLI first; a network server later

- **Status:** Accepted
- **Date:** 2026-11-01
- **Context:** "All in one" means one engine that can be linked into a Rust
  program, driven from a shell, and later served over the network. Building
  the server early would fix wire-protocol, authentication and concurrency
  choices before the engine underneath is stable.
- **Decision:**
  - The engine is a Rust library (`hardshelldb`) first: open a file, run
    SQL, read rows, in-process.
  - A CLI shell (`hardshell`) over the same library comes next (M8).
  - A network server (`hardshelld`) is the last milestone before the
    backlog (M12), built on the same library with no engine forks.
- **Consequences:**
  - The library API is the primary contract and gets stabilized at 1.0
    (M11), before the server exists.
  - The engine must be safe for many threads in one process from the start
    (ADR-0005), since the server will need it.
- **Alternatives considered:** embedded and CLI only (no server ever);
  server from the start. The user chose the staged path.
- **Revisit when:** a user needs remote access before M12.

## ADR-0004 — A PostgreSQL-flavored dialect with strict static types

- **Status:** Accepted
- **Date:** 2026-11-01
- **Context:** A SQL engine needs a reference for its syntax and semantics,
  or every edge case becomes a new decision. PostgreSQL is well specified,
  widely known, and strictly typed. Following it also makes the PostgreSQL
  wire protocol a natural fit for the server later (ADR-0016).
- **Decision:**
  - Syntax follows PostgreSQL where we support a feature: `$1` parameters,
    `::` casts, `||`, `RETURNING`, `ON CONFLICT`, `ILIKE`, double-quoted
    identifiers, unquoted identifiers folded to lower case.
  - Every column has a declared type, enforced on write. No type affinity.
  - Literals are untyped until context resolves them, as in PostgreSQL's
    `unknown` type. Other implicit casts follow a short, documented table
    (`docs/subsystems/types-and-values.md`).
  - Errors carry PostgreSQL SQLSTATE codes.
  - Where we differ from PostgreSQL, `docs/sql-reference.md` says so.
- **Consequences:**
  - PostgreSQL is the oracle for semantics tests (differential testing via
    `psql`, `docs/04-quality.md`).
  - We do not promise compatibility with PostgreSQL's catalogs, extensions,
    or file format. Only syntax and semantics of what we support.
  - Unsupported syntax must fail clearly (`0A000 feature_not_supported`),
    never be silently accepted (ADR-0011).
- **Alternatives considered:** SQLite-compatible syntax and dynamic typing;
  an own ANSI-core dialect. The user chose the PostgreSQL flavor.
- **Revisit when:** compatibility with another dialect becomes a product
  goal.

## ADR-0005 — One process owns a database file; many connections and threads within it

- **Status:** Proposed
- **Date:** 2026-11-01
- **Context:** Sharing a file between processes (as SQLite does) needs a
  shared-memory WAL index and cross-process lock protocols that differ per
  OS and fail on network filesystems. Within one process, shared state is
  ordinary Rust memory behind a mutex.
- **Decision:**
  - `Database::open` takes an exclusive OS lock on the database file. A
    second process gets a clear "database is locked by another process"
    error (SQLSTATE `55006`, `object_in_use`).
  - Inside the process, `Database` is `Send + Sync` and hands out any number
    of `Connection`s, each usable from one thread at a time (`Send`, not
    `Sync`).
  - A process-wide registry of open files, keyed by file identity, makes a
    second `Database::open` of the same file in the same process fail with
    `55006` too; the caller clones the existing handle instead. This does
    not depend on whether the OS lock conflicts within one process, which
    differs by lock kind and OS (verify in M1.2).
  - Other processes reach the data through the server (M12).
- **Consequences:**
  - The WAL index, page cache and lock table live in ordinary memory.
  - Network filesystems are unsupported; the docs say so.
  - Tools that inspect a live database (backup, integrity check) must run
    inside the owning process or through the server.
- **Alternatives considered:** SQLite-style multi-process access with a
  shared-memory index. Rejected for v1: large, OS-specific, and hard to
  crash-test.
- **Revisit when:** a user needs two processes on one file without a
  server.

## ADR-0006 — Single-file page store: B+tree tables clustered on a row id, memcmp-ordered keys, page size fixed at creation

- **Status:** Proposed
- **Date:** 2026-11-01
- **Context:** The storage engine needs one layout for tables and indexes
  that is simple to make crash-safe and to check.
- **Decision:**
  - A database is one file of fixed-size pages, plus a WAL file while open
    (ADR-0007). A cleanly closed database is one file.
  - Page size is chosen at creation (4096 default; 4096, 8192, 16384,
    32768 or 65536) and never changes. Page numbers are `u32`; page 0 is
    the file header.
  - Every table is a B+tree keyed by a hidden 64-bit row id. A declared
    primary key is a unique secondary index. Every index is a B+tree whose
    key is the index columns followed by the row id.
  - B+tree keys are byte strings whose `memcmp` order is the SQL order, made
    by an order-preserving key encoding in `hardshell-types`. The B+tree
    compares bytes only and knows nothing about SQL types.
  - Values too large for a page go to chains of overflow pages, up to a
    1 GiB limit per value.
- **Consequences:**
  - Updating a primary key does not move the row; secondary indexes point at
    stable row ids.
  - A lookup by primary key costs two tree searches (index, then table).
    Clustering by primary key is a backlog option.
  - The maximum file size is 2^32 pages (16 TiB at 4 KiB pages).
- **Alternatives considered:** heap files plus indexes (PostgreSQL); an LSM
  tree. Heap files need a free-space map and vacuum for every update; LSM
  trees need compaction and more complex recovery. Both are more code for
  an embedded engine's typical workload.
- **Revisit when:** benchmarks show the double lookup on primary keys costs
  more than the budget in `docs/00-overview.md`, or a workload is
  write-heavy enough that an LSM tree's write amplification matters.

## ADR-0007 — Redo-only WAL of whole page images in a sidecar file; the main file changes only by checkpoint

- **Status:** Proposed
- **Date:** 2026-11-01
- **Context:** Crash safety is the property a database cannot get wrong.
  The simplest scheme to reason about and test is one where committed data
  is always recoverable from one place, and uncommitted data never reaches
  the main file.
- **Decision:**
  - Writes go to `<name>-wal` as frames: a header (page number, commit
    marker with the database size after commit, salts, a cumulative
    checksum) and a whole page image.
  - A transaction commits by appending its last frame with the commit marker
    and syncing the WAL. Nothing else is needed to commit.
  - Uncommitted frames may be written to the WAL when the cache is full;
    readers and recovery ignore frames after the last valid commit marker.
    Rollback forgets them. There is no undo log.
  - A checkpoint copies the newest committed image of each page, up to the
    oldest snapshot any reader still uses, into the main file, syncs it, and
    only then lets the WAL restart from the beginning.
  - Recovery on open scans the WAL, validates frames by salt and checksum,
    and stops at the first invalid one; the last valid commit wins.
- **Consequences:**
  - A torn page in the main file during a checkpoint is repaired from the
    WAL, because the WAL is not reset until the main file is synced.
  - Writing whole pages costs more WAL bytes than logical logging; a
    checkpoint threshold (default 16 MiB of WAL) bounds the file.
  - Point-in-time recovery and replication would need logical records
    (backlog).
- **Alternatives considered:** a rollback journal (SQLite's older mode):
  readers block writers. ARIES-style logical redo and undo: much more code
  and harder to test.
- **Revisit when:** WAL volume or checkpoint stalls exceed the budgets, or
  replication becomes a goal.

## ADR-0008 — One writer, many snapshot readers; every transaction is serializable

- **Status:** Proposed
- **Date:** 2026-11-01
- **Context:** Multi-writer MVCC needs row versions, conflict detection and
  garbage collection. An embedded engine's common case is many readers and
  few writes.
- **Decision:**
  - At most one write transaction runs at a time, holding the database's
    write lock. Others wait up to a busy timeout (default 5 s), then fail
    with `55P03` (`lock_not_available`).
  - A read transaction reads a fixed snapshot: the main file plus the WAL up
    to the last commit when it started. Readers never block writers or each
    other.
  - A transaction that read on an older snapshot and then tries to write
    fails with `40001` (`serialization_failure`); the caller retries.
  - Because writes are serial and each reader sees one committed state,
    every transaction is serializable. Weaker isolation levels requested
    with `SET TRANSACTION` are accepted and run as serializable, which the
    SQL standard allows.
- **Consequences:**
  - No row-level locks, no deadlocks, no vacuum of row versions.
  - Write throughput is one transaction at a time, bounded by WAL sync
    latency; group commit is a later optimization.
  - Long-running readers hold back checkpoints (ADR-0007), so the WAL can
    grow; the engine logs a warning past a threshold.
- **Alternatives considered:** row-level MVCC with snapshot isolation (as
  PostgreSQL); two-phase locking. Both are backlog options behind this
  ADR's trigger.
- **Revisit when:** a benchmark or user workload is limited by
  single-writer throughput after group commit.

## ADR-0009 — Every page and WAL frame carries a CRC-32C; an fsync failure poisons the handle

- **Status:** Accepted
- **Date:** 2026-11-01
- **Context:** Disks, filesystems and OSes lose and corrupt data. Since
  2018 it is well known that on Linux a failed `fsync` may drop the dirty
  pages it failed to write, so retrying it can report success for data that
  is gone (the "fsyncgate" discussion around PostgreSQL).
- **Decision:**
  - Each page stores a CRC-32C of its contents, checked on every read from
    the main file. A mismatch is `XX001` (`data_corrupted`), never a panic
    or a silent wrong answer.
  - WAL frames use a cumulative checksum, so a torn tail is detected.
  - If any sync of the WAL or main file fails, the `Database` is poisoned:
    every later write fails with `58030` (`io_error`), and only closing and
    reopening (which runs recovery) clears it.
  - CRC-32C is table-driven software code in `hardshell-core` at first;
    hardware acceleration needs `unsafe` intrinsics and so a new ADR
    (ADR-0013).
- **Consequences:** a small CPU cost per page read and write; corruption is
  reported instead of propagated.
- **Alternatives considered:** checksums off by default (SQLite's default);
  retrying failed syncs. Both trade correctness for speed or convenience.
- **Revisit when:** checksum time exceeds 5% of a read-heavy benchmark.

## ADR-0010 — Synchronous, thread-based execution; no async runtime

- **Status:** Proposed
- **Date:** 2026-11-01
- **Context:** An async runtime is a large dependency (ruled out by
  ADR-0002) or a large thing to write. File I/O on our targets is blocking
  anyway, and an embedded caller runs queries on its own threads.
- **Decision:**
  - The engine is synchronous. A query runs on the caller's thread.
  - Shared state uses `std::sync` (`Mutex`, `RwLock`, `Condvar`, atomics).
  - The server (M12) runs one `std` thread per open connection, capped by
    `max_connections`. Serving many idle connections on fewer threads
    would need OS readiness polling, which `std` lacks (FFI, ADR-0013).
  - Cancellation is cooperative: `Connection::interrupt_handle()` sets a
    flag the executor checks at least every 1,000 rows and in every loop
    that waits; the query fails with `57014` (`query_canceled`).
- **Consequences:** simple stacks and backtraces; no async API. An async
  caller wraps calls in its runtime's blocking-task facility.
- **Alternatives considered:** an own async executor with non-blocking
  sockets for the server. Kept as backlog behind the trigger.
- **Revisit when:** the server must hold more idle connections than the
  thread budget allows.

## ADR-0011 — Hand-written lexer and recursive-descent parser with Pratt expressions; unsupported syntax fails with 0A000

- **Status:** Proposed
- **Date:** 2026-11-01
- **Context:** Parser generators are dependencies (ADR-0002). A hand-written
  parser gives precise error positions and messages, and grows one
  statement at a time.
- **Decision:**
  - `hardshell-sql` holds a lexer, a recursive-descent statement parser and
    a Pratt (precedence-climbing) expression parser, with PostgreSQL's
    operator precedence.
  - Every AST node carries a byte span into the source text.
  - Syntax we know but do not support yet produces `0A000` with the
    feature's name and the span. It is never ignored.
  - The parser bounds recursion depth (default 256) and input size (default
    16 MiB), returning `54001` (`statement_too_complex`) past them.
  - The AST pretty-prints back to SQL; `parse(print(ast)) == ast` is a
    property test.
- **Consequences:** more code than a grammar file, but no generator, and
  error messages we control.
- **Alternatives considered:** a PEG or LALR generator written by us.
  Rejected: a second language to maintain.
- **Revisit when:** the grammar passes about 400 productions and
  precedence bugs recur.

## ADR-0012 — Pull-based row-at-a-time executor with a per-query memory limit

- **Status:** Proposed
- **Date:** 2026-11-01
- **Context:** Vectorized and compiled executors are faster but much larger.
  Correctness first, then measured speed.
- **Decision:**
  - Physical operators implement a `next() -> Result<Option<Row>>` pull
    interface (the Volcano model). Expressions are evaluated from a tree.
  - Each query has a memory budget (`work_mem`, default 64 MiB) shared by
    its sorts and hash tables. Until spilling lands (M9), exceeding it fails
    with `53200` (`out_of_memory`); after M9, sorts and hash joins spill to
    temporary files.
- **Consequences:** per-row overhead; easy to test each operator alone.
- **Alternatives considered:** batch-at-a-time vectors; bytecode (as
  SQLite). Both are backlog options.
- **Revisit when:** profiling shows interpretation overhead above 40% of a
  benchmark query's time.

## ADR-0013 — `unsafe` is denied everywhere; only `hardshell-os` may opt in, and only by a new ADR

- **Status:** Accepted; amended by ADR-0017 (a test binary's counting allocator)
- **Date:** 2026-11-01
- **Context:** Nadir allows `unsafe` in three FFI crates because it binds
  windowing and GPU APIs. A database needs much less: `std` already offers
  positional reads and writes, `sync_all`/`sync_data`, and file locks
  (`File::lock`, stable since Rust 1.89).
- **Decision:**
  - `unsafe_code = "deny"` for the whole workspace. At M0 no crate allows it.
  - `hardshell-os` is the only crate that may ever add
    `#![allow(unsafe_code)]`, and only after an ADR names the OS call that
    `std` lacks (for example querying the device's atomic write size,
    `O_DIRECT`, or terminal raw mode for the CLI's line editor).
  - Raw OS declarations, when they come, live in `hardshell-os::sys` with
    the same rules as Nadir's FFI code (`.claude/rules/os-and-unsafe.md`).
- **Consequences:** most of the code is plain safe Rust. Hardware CRC-32C,
  `mmap` and direct I/O wait for a decision.
- **Alternatives considered:** a separate `-sys` crate as in Nadir. Not
  needed while the FFI surface is empty.
- **Revisit when:** a second crate needs `unsafe`.

## ADR-0014 — Text is UTF-8 with binary (code point) collation only

- **Status:** Proposed
- **Date:** 2026-11-01
- **Context:** Locale-aware collation needs the Unicode Collation Algorithm
  and locale data (ICU-sized). UTF-8 byte order equals code point order, so
  binary collation is both simple and well defined.
- **Decision:**
  - `TEXT` and `VARCHAR(n)` hold valid UTF-8; invalid input is `22021`
    (`character_not_in_repertoire`).
  - Comparison, `ORDER BY`, indexes and `DISTINCT` use code point order
    (PostgreSQL's `"C"` collation).
  - `lower`/`upper`/`ILIKE` use the simple case mappings of the Unicode
    Character Database, generated into a table by `xtask` from a committed
    copy of `UnicodeData.txt` (Unicode license, notice kept beside it).
  - `COLLATE` with anything other than `"C"` is `0A000`.
- **Consequences:** sorting differs from PostgreSQL databases created with
  a locale; `docs/sql-reference.md` says so.
- **Alternatives considered:** own UCA implementation. Backlog.
- **Revisit when:** a user needs locale-correct ordering.

## ADR-0015 — The Apache-2.0 header on every source file

- **Status:** Accepted
- **Date:** 2026-11-01
- **Context:** The repository is Apache-2.0 (`LICENSE`). Nadir puts the
  license's appendix header on every source file and checks it in CI
  (Nadir's own decision record 37). Doing the same here keeps the two
  projects alike.
- **Decision:** every source file (Rust, scripts, workflows, the CLI's
  help text sources) starts with the Apache-2.0 header from `LICENSE`'s
  appendix in its comment syntax, after a shebang if there is one.
  `cargo xtask headers` checks it from M0.
- **Consequences:** a few lines per file; a check that new files have it.
- **Alternatives considered:** SPDX one-line identifiers. Shorter, but not
  what the sibling project does.
- **Revisit when:** the license changes.

## ADR-0016 — The server speaks the PostgreSQL wire protocol v3.0; TLS and authentication are decided at M12

- **Status:** Proposed
- **Date:** 2026-11-01
- **Context:** A PostgreSQL-flavored engine (ADR-0004) served over the
  PostgreSQL protocol works with `psql` and existing drivers. TLS and
  SCRAM-SHA-256 need SHA-256, HMAC, PBKDF2 and a TLS stack: the
  dependency policy's criterion 2 (specialist correctness).
- **Decision:**
  - `hardshell-wire` implements protocol 3.0: startup, simple query, the
    extended query protocol (Parse, Bind, Describe, Execute, Sync),
    `ErrorResponse` with SQLSTATE, and cancel requests.
  - Before TLS exists, `hardshelld` listens on loopback TCP and, on Linux
    and macOS, a Unix-domain socket (`std` has none on Windows), and
    refuses to bind a non-loopback address.
  - M12.1 runs `/evaluate-dependency` for TLS and for SCRAM primitives and
    the user decides: build, adopt, or keep the server local-only.
- **Consequences:** clients get PostgreSQL error codes and type OIDs for the
  types we support; we must map our types to PostgreSQL OIDs.
- **Alternatives considered:** an own protocol. Rejected: no existing
  clients.
- **Revisit when:** M12 starts, or a user needs remote access sooner.

## ADR-0017 — A test binary may hold a counting allocator, the one `unsafe` outside `hardshell-os` (amends ADR-0013)

- **Status:** Accepted
- **Date:** 2026-11-03
- **Context:** M0.4 must show that logging with no sink allocates nothing,
  and the executor's allocation tests (`docs/subsystems/executor.md`) must
  show that a steady-state scan allocates nothing per row. Counting heap
  allocations needs a `#[global_allocator]`, and `GlobalAlloc` is an
  `unsafe` trait: `unsafe impl GlobalAlloc` and its `unsafe fn` methods
  fail `unsafe_code = "deny"` (checked with Rust 1.99.0). ADR-0013 allows
  no `unsafe` at M0, and none outside `hardshell-os` ever. Without `unsafe`,
  a test can only argue that nothing allocates (no argument is evaluated,
  nothing is formatted), not measure it.
- **Decision:**
  - An integration-test file (a crate's `tests/*.rs`, which builds its own
    test binary) may define one `#[global_allocator]` that counts
    allocations and forwards every call unchanged to `std::alloc::System`.
  - The `allow` is on that item only: `#[allow(unsafe_code)] // ADR-0017`
    on the `unsafe impl GlobalAlloc`, never `#![allow(unsafe_code)]` on the
    file. The impl carries `// SAFETY:`, and so does each `unsafe` block in
    it, one operation per block (the workspace's clippy lints).
  - Counting never allocates, panics or unwinds: an atomic, or a
    `const`-initialized thread-local `Cell`.
  - Nothing else in the file is `unsafe`. Never in a crate's `src/`
    (library and binary code and their unit tests), so never in shipped
    code.
  - `cargo xtask unsafe-audit` (M0.5) allows exactly this shape and still
    fails on any other `unsafe` outside `hardshell-os`.
- **Consequences:** allocation claims are measured, not argued. Each test
  file that counts holds its own copy of the forwarder (about 40 lines); the
  first is `crates/core/tests/log_alloc.rs`. The audit recognises a second
  allowed shape. The rest of ADR-0013 stands.
- **Alternatives considered:** the argued check above (weaker: it shows
  what the code does not do, not what the allocator saw); one shared
  counting allocator in a library crate (puts `unsafe` in library code, and
  `core`'s tests cannot depend on any other crate); the allocator in
  `hardshell-os` behind `testing` (the dependency matrix forbids `core`
  depending on `os`, dev-dependencies included).
- **Revisit when:** a test needs any other `unsafe`, or `std` gains a safe
  way to count allocations.
