# hardshellDB — an all-in-one SQL database engine and library (Rust)

**Status:** the workspace exists (M0.1), but every crate is still empty.
`docs/roadmap.md` ticks each task once it
is done and verified. A task whose code is written but still needs a check
on another OS stays unticked, with a `⏳ needs <OS>` note. The next task is
the first unticked one whose dependencies are all ticked, skipping one
whose remaining work needs an OS you cannot run. This document describes
code, commands and checks that do not exist yet: check before assuming.

## What we are building

- One SQL engine with three ways in: an embedded Rust library
  (`hardshelldb`), a CLI shell (`hardshell`, M8), and a server speaking the
  PostgreSQL wire protocol (`hardshelld`, M12) (ADR-0003).
- **Dialect:** PostgreSQL-flavored, strictly typed; errors carry PostgreSQL
  SQLSTATE codes (ADR-0004). `docs/sql-reference.md` is the contract.
- **Storage:** one file of checksummed pages, B+tree tables and indexes, a
  redo-only WAL; one writer and many snapshot readers; every transaction
  serializable (ADR-0006 to ADR-0009).
- **Targets (only these):** Linux (x86_64, aarch64), macOS 12+ (Apple
  silicon, Intel), Windows 10+ (x86_64, aarch64, MSVC) (ADR-0001).
- **Out of scope:** WASM, mobile, 32-bit, network filesystems, several
  processes on one file without the server, locale collations, an async API.
- **Built from the ground up:** zero third-party crates (ADR-0002).

## Read before working

| If you are touching… | Read first |
| :- | :- |
| Anything | `docs/00-overview.md`, `docs/01-architecture.md` |
| `Cargo.toml`, `Cargo.lock`, a new crate | `docs/02-dependency-policy.md` |
| `crates/core` | `docs/subsystems/core.md` |
| `crates/os` | `docs/subsystems/os-and-vfs.md`, `docs/03-platforms.md` |
| `crates/types` | `docs/subsystems/types-and-values.md` |
| `crates/storage` | `docs/subsystems/storage.md`, `docs/subsystems/wal-and-transactions.md`, `docs/04-quality.md` ("Storage change review checklist") |
| `crates/sql` | `docs/subsystems/sql-frontend.md`, `docs/sql-reference.md` |
| `crates/catalog` | `docs/subsystems/catalog.md` |
| `crates/plan` | `docs/subsystems/planner.md` |
| `crates/exec` | `docs/subsystems/executor.md` |
| `crates/hardshelldb` | `docs/subsystems/api.md` |
| `apps/cli` | `docs/subsystems/cli.md` |
| `crates/wire`, `apps/server` | `docs/subsystems/server-and-wire.md` |
| Tests, CI, `xtask`, `tests/slt`, fuzzing, crash campaigns | `docs/04-quality.md` |
| Packaging, versions, the file format promise | `docs/05-release.md` |
| Why something is the way it is | `docs/decisions.md` |

## Non-negotiable rules

1. **No third-party dependencies.** Do not add any crate outside this
   workspace to any `[dependencies]`, `[dev-dependencies]` or
   `[build-dependencies]` table, unless `docs/02-dependency-policy.md` lists
   it as `Approved` (none is). If you believe one is needed, stop, write the
   evaluation the policy describes, and ask the user. `cargo xtask deps`
   enforces this.
2. **Layering is one-way.** A crate may depend only on crates listed for it
   in `docs/01-architecture.md` → "Dependency matrix". Do not edit the
   matrix to make code compile; ask. `cargo xtask layers` enforces this.
3. **No `unsafe`.** `unsafe_code = "deny"` everywhere. Only `hardshell-os`
   may ever allow it, and only after an accepted ADR names the OS call
   `std` lacks (ADR-0013). Every `unsafe` block then carries `// SAFETY:`.
4. **Durability is not negotiable.**
   - Never acknowledge a commit before the sync that makes it durable.
   - The main database file changes only by checkpoint, with data already
     committed in the WAL (ADR-0007).
   - Never retry a failed sync; it poisons the handle (ADR-0009).
   - Any change to a write path runs the crash campaign and reports the
     iterations and seed.
5. **Every byte from outside is hostile.** SQL text, database and WAL
   files, CSV and wire messages never cause a panic, an unbounded
   allocation, or unbounded recursion. Malformed input is an error with a
   SQLSTATE code.
6. **Plans never change answers.** An optimization must have a test that
   the plans with and without it return the same rows.
7. **Honest verification.**
   - Never state that a build, test, benchmark, campaign or check passed
     unless you ran it in this session and saw it pass.
   - If a command does not exist yet, say so instead of guessing.
   - Report what you did not run.
8. **Smallest complete change.**
   - No unrelated refactors, renames, reformatting or "while I'm here" fixes.
   - Preserve public APIs and the on-disk format unless the task authorizes
     a break; record breaks in `docs/decisions.md`.
9. **Git is read-only for agents.** Do not commit, amend, rebase, merge, tag,
   push, force-push or rewrite history. Leave all changes uncommitted for
   human review.
10. **No secrets.** Never write tokens, keys, certificates or passwords into
    the repository, test fixtures included. Release secrets exist only in
    CI secret storage (`docs/05-release.md`).

## Workspace map (crates created in M0.1, filled in by the milestones shown)

| Path | Package | Purpose | Built in |
| :- | :- | :- | :- |
| `crates/core` | `hardshell-core` | SQLSTATE codes and errors, CRC-32C, varints, hashing, `Pcg32`, logging, property-test harness, CLI argument parser, CSV codec | M0, M8 |
| `crates/os` | `hardshell-os` | `Vfs` trait, `StdVfs`, `MemVfs`, `FaultVfs` (feature `testing`), locks, sync rules | M1 |
| `crates/types` | `hardshell-types` | SQL types and values, casts, comparison, `NUMERIC`, date/time, key and row encodings | M4 |
| `crates/storage` | `hardshell-storage` | File header, pager, buffer pool, free list, WAL, checkpoint, recovery, snapshots, B+trees | M1–M3 |
| `crates/sql` | `hardshell-sql` | Lexer, parser, AST, printer | M5 |
| `crates/catalog` | `hardshell-catalog` | System tables, schema objects, schema versions | M6 |
| `crates/plan` | `hardshell-plan` | Binder, logical plans, optimizer, statistics, physical plans | M6, M7, M9 |
| `crates/exec` | `hardshell-exec` | Operators, expression evaluation, functions, DML, constraints, spilling | M6, M7, M9 |
| `crates/hardshelldb` | `hardshelldb` | Facade: `Database`, `Connection`, `Statement`, `Rows`, errors; SLT runner and SQL crash campaign tests | M6 |
| `crates/wire` | `hardshell-wire` | PostgreSQL protocol v3.0 messages | M12 |
| `apps/cli` | `hardshell-cli` | The `hardshell` shell | M8 |
| `apps/server` | `hardshell-server` | The `hardshelld` server | M12 |
| `xtask` | `xtask` | Checks, fuzz smoke, crash campaign runner, benches runner, `diff-pg` | M0 onward |
| `bench` | `hardshell-bench` | Own benchmark harness and benches | M0.7 |

## Commands (none exist yet; verify one exists before relying on it)

| Command | What it does | From |
| :- | :- | :- |
| `cargo xtask ci` | fmt check, clippy (`-D warnings`), tests (with SLT), `deps`, `layers`, `unsafe-audit`, `headers`, `release-no-testing`, `workflows`: the full PR gate | M0.5 |
| `cargo xtask deps` | Fails if `Cargo.lock` has a non-workspace package not approved in the policy | M0.5 |
| `cargo xtask layers` | Fails if a crate depends on a crate the matrix forbids | M0.5 |
| `cargo xtask unsafe-audit` | Fails on `unsafe` outside `hardshell-os`, or without `// SAFETY:` | M0.5 |
| `cargo xtask headers` | Fails if a source file lacks the Apache-2.0 header (ADR-0015) | M0.5 |
| `cargo xtask fuzz-smoke [--secs N] [--target T]` | Mutation fuzzer over every parser, or one | M0.6 |
| `cargo xtask bench [filter]` | Benchmarks in release mode, compared with `bench/baseline.txt` | M0.7 |
| `cargo xtask crash [--iterations N] [--level storage\|sql]` | Crash campaigns under `FaultVfs` | M2.6 |
| `cargo test -p hardshelldb --test slt` | SQL logic tests in `tests/slt/` | M6.6 |
| `cargo xtask diff-pg --psql <path> --dsn <dsn>` | Differential tests against PostgreSQL through `psql` | M9.6 |
| `cargo run -p hardshell-cli -- app.hsdb` | The shell | M8.2 |

`xtask` never uploads, publishes, signs, reads secrets, or calls the
network (except `diff-pg`, which talks only to the PostgreSQL server it is
given).

## Conventions

- **Toolchain and edition:** Rust edition 2024, stable toolchain pinned in
  `rust-toolchain.toml` (M0.1 records the version).
- **Errors:**
  - Each crate defines its own error enum implementing `Display` and
    `std::error::Error`, and maps every variant to a `SqlState`.
  - No `unwrap()` in library code (`clippy::unwrap_used` is denied outside
    tests). `expect("why this cannot fail")` only for true invariants,
    never on input-derived values.
- **Logging:** the `hardshell_core::log` macros only. `print!`/`eprint!`
  are denied in library crates; the CLI and server allow them at their
  roots with a comment.
- **Integers on disk:** fixed-width little-endian or LEB128 varints; every
  length read from disk is checked against the remaining input and a limit
  before use.
- **Determinism:** no `std::collections::HashMap` with `RandomState` where
  iteration order can reach results, files or tests; use `FastHashMap` or
  sort. Results without `ORDER BY` may come in any order, so SLT files use
  `rowsort` for them.
- **Docs:** every `pub` item has a doc comment (`missing_docs` is `-D` in
  CI).
- **License headers:** every source file starts with the Apache-2.0 header
  from `LICENSE`'s appendix, in its comment syntax (ADR-0015).
- **Tests:**
  - Unit tests in `#[cfg(test)] mod tests` beside the code.
  - Integration tests in each crate's `tests/`.
  - SQL behavior in `tests/slt/<area>/<feature>.slt`.
  - Fuzz seeds in `fuzz/corpus/<target>/`; format fixtures in
    `tests/fixtures/`.

## Definition of done for a roadmap task

1. Every acceptance criterion for the task in `docs/roadmap.md` is met.
2. The task's verification commands were run. Report each command and its
   real result.
3. The checkbox is ticked only after steps 1 and 2.
4. Docs are updated if behavior or design changed: the subsystem spec, and
   `docs/sql-reference.md` for any SQL surface. If a decision changed, add
   an ADR in `docs/decisions.md`.
5. The final message summarizes:
   - changes and files affected
   - validation performed (commands and real results; campaign iterations
     and seeds)
   - checks not run, and why
   - assumptions
   - unresolved issues and residual risks
