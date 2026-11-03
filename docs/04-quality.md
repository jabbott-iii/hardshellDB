# 04 — Quality: testing, checks, CI

Because we write our own storage engine, parser and types, testing has to
do the job that a mature, battle-tested engine's history would otherwise
do. Every harness below is our own code (dependency policy items 21–24).
The property-test harness is built (M0.4); each other harness names the
roadmap task that builds it.

## Test layers

| Layer | Where | Runs in | What it catches |
| :- | :- | :- | :- |
| Unit tests | `#[cfg(test)] mod tests` beside the code | `cargo test`, all OSes | Logic errors in every crate |
| Property tests | `hardshell_core::testing::check` | `cargo test` | Edge cases: encodings round-trip, key order equals SQL order, arithmetic matches references |
| Model-based tests | `storage` and `catalog` tests | `cargo test` | Pager, WAL, B+tree and catalog diverging from a simple in-memory model |
| VFS contract suite | `crates/os/tests/` | `cargo test`, each OS | Differences between `MemVfs`, `StdVfs` and the OS |
| SQL logic tests | `tests/slt/**/*.slt` via `crates/hardshelldb/tests/slt.rs` | `cargo test -p hardshelldb --test slt` | Wrong query results, wrong error codes |
| Crash campaigns | `crates/storage/tests/crash.rs`, `crates/hardshelldb/tests/crash.rs` via `cargo xtask crash` | CI (short), nightly (long) | Lost commits, visible uncommitted data, corruption after a crash |
| Process-kill test | `crates/storage/tests/kill.rs` | `cargo test --release`, each OS | Real-OS process crashes mid-commit |
| Fuzz smoke | `cargo xtask fuzz-smoke` | CI (time-boxed), nightly | Panics, hangs, huge allocations in parsers and decoders |
| Format fixtures | `tests/fixtures/format-v<N>/` (from M11.1) | `cargo test` | Breaking the on-disk format |
| Differential tests | `cargo xtask diff-pg` (from M9.6) | Manual and nightly, with PostgreSQL | Semantic drift from PostgreSQL |
| Benchmarks | `hardshell-bench` | Manual; nightly report | Performance regressions against the stored baseline |

## Own harnesses

### Property testing — `hardshell_core::testing` (M0.4)

```rust
/// Runs `property` on `cases` inputs generated from seeded `Pcg32`s. On
/// the first failure (an `Err`, or a panic in the generator, the property
/// or the input's drop), panics with a report: the case, the seed that
/// replays it, the input's `Debug` form (built only for a failure) and the
/// error.
pub fn check<T: std::fmt::Debug>(
    name: &str,
    cases: u32,
    generate: impl Fn(&mut Pcg32) -> T,
    property: impl Fn(&T) -> Result<(), String>,
);

/// The same, with a size from 0 to 100 that grows evenly over the run; on
/// failure it rebuilds the failing case at each smaller size and reports
/// the smallest that still fails.
pub fn check_sized<T: std::fmt::Debug>(
    name: &str,
    cases: u32,
    generate: impl Fn(&mut Pcg32, u32) -> T,
    property: impl Fn(&T) -> Result<(), String>,
);
```

- The seed comes from `HARDSHELL_SEED` (decimal, or `0x` and hex digits;
  anything else fails the test), or `DEFAULT_SEED` when it is unset or
  empty, so CI runs are reproducible. The nightly job uses random seeds and
  prints them.
- Case `i` of the property `name` draws from
  `Pcg32::new(seed ^ fnv1a64(name), i)`: cases are independent, properties
  sharing a seed get different inputs, and a reported seed replays the run
  (`HARDSHELL_SEED=<seed> cargo test <test>`).
- A report looks like this (`check_sized` adds the size, and the size it
  shrank from):

  ```
  property "one draw in eight fails" failed at case 4 of 1000; HARDSHELL_SEED=12345 replays it
  input: 2089109368
  error: 2089109368 is a multiple of 8
  ```

- The panic points at the `check` call (`#[track_caller]`). Generators and
  properties should be pure: shrinking calls them again, even after a
  panic.

### Model-based tests

- The B+tree is checked against `BTreeMap<Vec<u8>, Vec<u8>>`; the pager and
  WAL against a map of committed page images per snapshot; the catalog
  against a list of applied DDL.
- Each model test runs random operation sequences, including reopen and
  (with `FaultVfs`) crash steps, and compares state after every step.

### SQL logic tests (M6.6)

- Files in `tests/slt/<area>/<feature>.slt`, in the sqllogictest format:

  ```
  statement ok
  CREATE TABLE t (a INTEGER, b TEXT)

  statement error 23502
  INSERT INTO t (b) VALUES ('x') -- with a NOT NULL a

  query IT rowsort
  SELECT a, b FROM t
  ----
  1 one
  2 NULL
  ```

- Type letters: `I` integer, `R` float or numeric, `T` text, `B` boolean,
  `D` date/time. Sort modes `nosort`, `rowsort`, `valuesort`. Errors are
  matched by SQLSTATE, never by message text.
- Every file starts on a fresh in-memory database. A feature's SLT file is
  part of its definition of done (`/add-sql-feature`).

### Crash campaigns (M2.6, M6.7)

- `FaultVfs` (M1.3) records every write since each file's last sync. A
  crash produces the post-crash image under a seeded policy: drop unsynced
  writes, keep a random subset, tear writes at 512-byte boundaries, or
  reorder them. It can also fail the Nth read, write or sync.
- `crash_point!("area.event")` marks moments in storage code (after a frame
  write, before a checkpoint sync, …); the campaign crashes at each one and
  at random write boundaries.
- After each crash: reopen (recovery), run the tree checker, and compare
  with the model: every acknowledged commit is present, nothing
  uncommitted is visible, every checksum is valid.
- `cargo xtask crash [--iterations N] [--level storage|sql]`; the seed is
  printed and honored from `HARDSHELL_SEED`. CI runs 2,000 iterations per
  level on Linux; nightly runs 1,000,000 (M11.4).
- A campaign must be able to fail: each level has a test that disables a
  sync and asserts the campaign catches it.

### Fuzz smoke (M0.6)

- `cargo xtask fuzz-smoke [--secs N] [--target T]` runs every target (or
  one) for N seconds each (default 60), in the `xtask` profile (optimized,
  with overflow checks and debug assertions on).
- Seeds from `fuzz/corpus/<target>/` run unchanged first; then mutated
  inputs: bit flips, byte insert/delete, splice, length-field tweaks,
  interesting integers (0, 1, 255, 65535, `u32::MAX`, `u64::MAX`).
- Each input runs under `catch_unwind`, with a 2 s hang watchdog. Findings
  go to `fuzz/crashes/<target>/` (git-ignored), deduplicated by panic
  location. Copy one into `fuzz/corpus/<target>/` to keep it as a
  regression seed. Exit status 0 clean, 1 finding.
- Targets (each registered by the task that adds its parser):

  | Target | Input | Crate | Task |
  | :- | :- | :- | :- |
  | `varint` | LEB128 bytes | `core` | M0.6 |
  | `page` | A page image of any type | `storage` | M1.4, M3.1 |
  | `wal` | A WAL file | `storage` | M2.1 |
  | `open` | A whole database file, opened in `MemVfs` and checked | `storage` | M3.5 |
  | `types-text` | Text input for every SQL type | `types` | M4.4 |
  | `key` | Encoded index keys | `types` | M4.5 |
  | `row` | Encoded rows | `types` | M4.6 |
  | `sql` | SQL text | `sql` | M5.5 |
  | `csv` | CSV text | `core` | M8.1 |
  | `wire` | PostgreSQL protocol messages | `wire` | M12.2 |

- A target that can check its own answers does: `key` and `row` re-encode
  what they decode and compare; `sql` prints what it parses and re-parses.
- Coverage-guided fuzzing (`cargo fuzz`) is a separate, **unapproved**
  proposal (dependency policy item 30), decided at M11.7.

### Differential tests against PostgreSQL (M9.6)

- `cargo xtask diff-pg --psql <path> --dsn <dsn>` runs the SLT corpus and
  generated queries on hardshellDB and, through the external `psql`
  program with `--csv`, on a PostgreSQL server; it compares results and
  SQLSTATE codes.
- `psql` and the server are external programs, not dependencies (policy:
  "What counts"). Known differences are listed in `docs/sql-reference.md`
  and skipped by name; anything else is a finding.

### Benchmarks — `hardshell-bench` (M0.7)

- Warm-up, then N samples; reports median and p95; compares with
  `bench/baseline.txt` (`name median_ns p95_ns`) and flags changes beyond
  10%. Report only: benches never fail CI.
- Budgets in `docs/00-overview.md` are measured on the recorded reference
  machine (M9.7), not on CI runners.

## `xtask` checks

| Check | Implementation |
| :- | :- |
| `fmt` | `cargo fmt --all -- --check` |
| `clippy` | `cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| `test` | `cargo test --workspace --all-features` (includes the SLT runner) |
| `deps` | `cargo metadata --format-version 1`; every package with a non-null `source` must appear in `xtask/deps-allowlist.txt` with its exact version; a path package outside the workspace fails |
| `layers` | Same metadata; each crate's normal, dev and build dependencies must be within its row of the dependency matrix; a crate with no row fails. The matrix is a `const` table in `xtask`; a unit test compares it with `docs/01-architecture.md` |
| `unsafe-audit` | Scans `*.rs`, ignoring comments and strings: no `unsafe` outside `hardshell-os`, except in the `unsafe impl GlobalAlloc` of a counting allocator in a crate's `tests/` (ADR-0017); `allow(unsafe_code)` only at that crate's root and only if an ADR allows it, or on that impl; every `unsafe` block or impl preceded by `// SAFETY:` within 3 lines; no `std::fs` outside `hardshell-os`, `xtask`, the apps and tests; prints per-crate counts |
| `headers` | Every source file starts with the Apache-2.0 header of `LICENSE`'s appendix in its comment syntax (ADR-0015) |
| `release-no-testing` | The `testing` feature is absent from the non-dev dependency trees of `hardshell-cli` and `hardshell-server` |
| `workflows` (from M0.8) | Every GitHub Action pinned by full commit SHA with a version comment; top-level permissions `contents: read` or `{}`; `persist-credentials: false` on checkout; no `pull_request_target` |
| `ci` | All of the above in order; stops at the first failure and prints a summary table |

Each check is also a subcommand (`cargo xtask deps`). Exit status: 0 pass,
1 fail, 2 command line not understood.

## CI (GitHub Actions)

- **Every PR and push** (`.github/workflows/ci.yml`, M0.8), matrix
  `ubuntu-latest`, `macos-latest`, `windows-latest`:
  1. Turn `core.autocrlf` off, then check out.
  2. Install the pinned toolchain from `rust-toolchain.toml`.
  3. `cargo xtask ci`.
  4. Linux only: `cargo xtask fuzz-smoke --secs 30`.
  5. Linux only (from M2.6): `cargo xtask crash --iterations 2000`.

  A newer push to a pull request cancels its older run.
- **Nightly** (M11.4): fuzz smoke 30 minutes per target (one job per
  target), crash campaigns of 1,000,000 iterations, property tests with
  random seeds, benchmarks (report only), and, from M9.6, `diff-pg` with a
  PostgreSQL service container. Artifacts (`fuzz/crashes/`, failing seeds)
  are uploaded on failure.
- **Release** (tag `v*`, M11.6): see `docs/05-release.md`.

## Storage change review checklist

Use this for any change to `hardshell-os`, `hardshell-storage`, or the
encodings in `hardshell-types` (`.claude/rules/storage-and-durability.md`
loads it for agents).

- [ ] Can any crash point between two of the changed writes leave the main
      file holding data that is not committed in the WAL? (Must be no.)
- [ ] Is every new write followed, before the operation is acknowledged, by
      the sync that makes it durable, in the order `docs/03-platforms.md`
      gives?
- [ ] Does every new read path verify checksums, bounds and lengths before
      use, and return `XX001` instead of panicking?
- [ ] Is a sync failure still fatal to the handle (ADR-0009)?
- [ ] Did the change add a `crash_point!` at each new durability boundary,
      and does the crash campaign reach it?
- [ ] Did an on-disk structure change? Then: the format version is bumped,
      the change is in `docs/subsystems/storage.md` (from M11.1,
      `docs/file-format.md`), and a fixture test covers it
      (`/change-on-disk-format`).
- [ ] Were the crash campaign and kill test run, with their iteration
      counts and seeds reported?
