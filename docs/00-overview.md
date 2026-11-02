# 00 — Overview

## Purpose

hardshellDB is an all-in-one SQL database engine and library written in
Rust. It is one engine with three ways in:

1. **Embedded:** link the `hardshelldb` crate, open a file, run SQL. No
   server, no configuration.
2. **Shell:** the `hardshell` CLI opens a database file and runs SQL and
   psql-style backslash commands (M8).
3. **Server:** `hardshelld` serves the same engine over the PostgreSQL wire
   protocol (M12).

The SQL dialect is PostgreSQL-flavored and strictly typed (ADR-0004). The
storage engine is a single file of checksummed pages, B+trees, and a
write-ahead log (ADR-0006, ADR-0007). It is built from first principles with
no third-party crates (ADR-0002).

The name is the promise: a hard shell around the data. Durability and
correctness under crashes, hostile files and hostile SQL come before speed
and before breadth.

## Goals

1. **Never lose a committed transaction, never return a wrong answer
   silently.** Corruption is detected and reported (ADR-0009); crashes at
   any point recover to the last commit (ADR-0007).
2. **Serializable by default.** Every transaction is serializable
   (ADR-0008).
3. **PostgreSQL semantics for what we support.** Where a query runs on both,
   it returns the same rows (in the same order when `ORDER BY` fixes it), or
   `docs/sql-reference.md` documents the difference.
4. **Own the whole stack**, so that any bug can be understood and fixed
   in-house, and the code that holds users' data has no supply chain.
5. **Small enough to hold in one or two heads.** As an estimate, about
   60–80k lines of Rust at 1.0 (M11).
6. **Embeddable:** no global state, no background threads unless asked for,
   no printing; the host application owns logging and threads.

## Non-goals

- WASM, mobile, 32-bit and big-endian targets (ADR-0001).
- Several processes writing one database file without the server
  (ADR-0005). Network filesystems.
- Compatibility with PostgreSQL's file format, catalogs (`pg_catalog`
  beyond what drivers need at M12), extensions, procedural languages, or
  replication protocol.
- Multi-writer MVCC before the backlog trigger in ADR-0008 fires.
- Locale-aware collations (ADR-0014).
- Distributed or sharded operation.
- An async API (ADR-0010).
- A GUI.

## Performance budgets

These are the targets we test against. They are **targets, not
measurements**: nothing is built yet. No reference machine is named yet:
record one here (CPU, RAM, storage device, OS, filesystem) before M9.7
measures every budget on it. Durable-commit numbers depend on the storage
device's flush latency, so they are stated relative to it.

| Budget | Target |
| :- | :- |
| Point lookup by primary key, prepared, warm cache | ≤ 5 µs median, ≤ 20 µs p99, from the embedded API |
| Read scaling | 8 reader threads doing point lookups reach ≥ 6× one thread's throughput |
| Bulk load | 1,000,000 rows of `(BIGINT, TEXT(32), DOUBLE PRECISION)` in one transaction ≤ 3 s |
| Durable commit | A single-row autocommit `INSERT` costs ≤ 1.5× the device's measured `fdatasync` latency |
| Full scan | ≥ 5 million rows/s for `SELECT count(*) FROM t WHERE x > $1` on a warm 3-column table |
| Open | Opening a cleanly closed 10 GiB database ≤ 50 ms (no full-file scan) |
| Recovery | Opening after a crash with a 64 MiB WAL ≤ 1 s |
| Memory | Page cache defaults to 64 MiB and never exceeds its setting; each query's sorts and hash tables stay within `work_mem` (default 64 MiB) |
| Binary size | `hardshell` CLI ≤ 8 MB stripped, per target |
| Build time | Clean `cargo build --workspace --release` ≤ 90 s on the reference machine |

## Assumptions

Revisit these if any turns out to be wrong.

- One or two engineers, with an AI coding agent doing much of the
  implementation under review.
- The storage device honors flush commands (`fdatasync`, `F_FULLFSYNC`,
  `FlushFileBuffers`). We cannot protect against devices that lie about
  flushes, and the docs say so.
- Sector writes of 512 bytes are atomic; page writes are not (pages can
  tear, and the design tolerates it).
- Databases up to about 1 TiB and tables up to about 1 billion rows on
  commodity SSDs. The format allows more (16 TiB at 4 KiB pages).
- Typical embedded workloads are read-heavy, with occasional write bursts.
- The machine has at least 1 GiB of RAM for the engine and a 64-bit CPU.

## Scope estimate

Sizes are rough line-count estimates for planning, not commitments.
S ≈ <1k lines, M ≈ 1–3k, L ≈ 3–6k, XL ≈ 6k+.

| Area | Size | Why |
| :- | :- | :- |
| `core` | M | Errors and SQLSTATE codes, CRC-32C, varints, logging, property-test harness, CLI argument parser, CSV codec |
| `os` | M | File abstraction (VFS), positional I/O, locking, sync rules per OS, the fault-injecting VFS for crash tests |
| `types` | XL | Every SQL type's parse/format/compare/cast, `NUMERIC`, dates, times, intervals, the order-preserving key encoding, the row encoding |
| `storage` | XL | File header, pager, buffer pool, free list, WAL, checkpoint, recovery, snapshots, B+tree, overflow pages, integrity check |
| `sql` | L | Lexer, parser for a PostgreSQL-flavored grammar, AST, printer |
| `catalog` | M | Schema objects, system tables, DDL bookkeeping, schema versioning |
| `plan` | XL | Binder, type checking, logical plan, rewrites, cost model, statistics, physical planning |
| `exec` | XL | Operators, expression evaluation, functions, DML, constraint checks, sorting, hashing, spilling |
| `hardshelldb` (facade) | M | `Database`, `Connection`, `Statement`, `Rows`, parameters, transactions, errors |
| `cli` | M | REPL, meta-commands, output formats, CSV import and export, dump |
| `wire` + `server` | L | PostgreSQL protocol v3.0, connection management, authentication |
| `xtask` + harnesses | L | Checks, fuzz smoke, crash campaign, SQL logic test runner, benchmarks, differential tests |

The biggest risk is not any one area. It is **the breadth of SQL
semantics** multiplied by **the depth of crash safety**: every feature must
be right for NULLs, types, errors and transactions, and every write path
must be right after a crash at any byte. The roadmap therefore builds the
storage engine and its crash tests before any SQL, and grows SQL one
feature at a time behind SQL logic tests.

## Top risks and mitigations

| Risk | Mitigation |
| :- | :- |
| A crash or I/O error loses or corrupts committed data | Redo-only WAL with whole pages (ADR-0007); checksums everywhere (ADR-0009); the fault-injecting VFS and `cargo xtask crash` campaign (`docs/04-quality.md`); a process-kill test; poisoned handles on sync failure |
| Hostile or damaged database files crash or exploit the engine | Every on-disk structure is parsed as untrusted input: bounded, checked, fuzzed (`.claude/rules/untrusted-input.md`); `CHECK DATABASE` verifies the whole file (M11) |
| SQL semantics drift from PostgreSQL (NULLs, casts, rounding, ordering) | `docs/sql-reference.md` as the contract; SQL logic tests; differential tests against a real PostgreSQL through `psql` (M9) |
| Scope creep in the SQL surface | The SQL reference lists what is supported; everything else is `0A000`; new features enter through the roadmap and `/add-sql-feature` |
| The optimizer makes wrong plans or wrong results | Plans never change results: every rewrite has an equivalence test, and plan choice is tested with forced alternatives giving identical rows |
| Zero dependencies blocks TLS and password hashing for the server | Decided explicitly at M12.1 (ADR-0016); the server stays local-only until then |
| Single-writer throughput is too low | Group commit (M9); ADR-0008's trigger for MVCC |
| Performance regressions go unnoticed | `hardshell-bench` with a stored baseline; budgets checked at M9.7 and M11 |
