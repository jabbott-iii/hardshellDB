# Roadmap

How to use this file:

- Work top to bottom. A task is **unblocked** when everything in its
  `Depends` line is checked.
- Tick a box only after its acceptance criteria are met **and** its
  verification commands were run and passed (see "Definition of done" in
  `AGENTS.md`).
- If a criterion needs an OS you can't run, leave the box unchecked. Add an
  indented note saying exactly what remains:
  `  - ⏳ needs macOS: run the VFS contract suite`.
- Add indented `Note:` lines under a task for what the work settled:
  pinned versions, measured numbers, conventions chosen, follow-ups.
- Exit criteria for a milestone are checked by a human.
- OS order within every milestone: **Linux → macOS → Windows**.
- A task that depends on a `Proposed` ADR starts by asking the user to
  accept, change or reject it (`docs/decisions.md`).
- Every task that adds SQL surface also updates `docs/sql-reference.md`.

Status: work has started at M0; a ticked box is done.

---

## M0 — Foundation and guardrails

**Goal:** an empty but fully guarded workspace. The constraints are
enforced by tooling from day one.

- [x] **M0.1 — Workspace skeleton**
  - Depends: —
  - Do:
    - Create every crate in the workspace map (`AGENTS.md`) as an empty
      library or binary with a crate-level doc comment, including the ones
      built later (`wire`, `server`), so the layer check covers them.
    - Root `Cargo.toml`: `[workspace]` (resolver 3), `[workspace.package]`
      (edition 2024, version 0.1.0, license Apache-2.0, repository),
      `[workspace.dependencies]` for the members, and `[workspace.lints]`
      exactly as in `docs/01-architecture.md`.
    - `rust-toolchain.toml`: pin the current stable version, with components
      `rustfmt` and `clippy`. Record the version in a task note.
    - `clippy.toml`: `allow-unwrap-in-tests = true`,
      `allow-expect-in-tests = true`.
    - `.cargo/config.toml` with the `xtask` alias and the `xtask` profile
      (`inherits = "dev"`, `opt-level = 3`).
    - Check `.gitignore` still covers `target/`; no crate is named `debug`.
  - Accept:
    - `cargo build --workspace` and
      `cargo clippy --workspace --all-targets -- -D warnings` succeed.
    - `Cargo.lock` lists only workspace packages.
    - No crate contains `#![allow(unsafe_code)]`.
  - Verify: `cargo build --workspace`;
    `cargo clippy --workspace --all-targets -- -D warnings`;
    `grep -c 'source = ' Cargo.lock` (expect 0)
  - Note: toolchain pinned to Rust 1.99.0, the installed stable on
    2026-11-01.
  - Note: both profiles (`release`, `xtask`) live in the root `Cargo.toml`,
    as `docs/01-architecture.md` and `docs/05-release.md` show;
    `.cargo/config.toml` holds only the alias (user decision, 2026-11-01).
  - Note: `[workspace.dependencies]` lists only the library crates, since
    nothing may depend on an application. No crate depends on another yet.
    `xtask` and `hardshell-bench` are `publish = false`.
  - Note: ADR-0013 and ADR-0015 accepted (2026-11-01). Every `.rs` file
    starts with the ADR-0015 header from M0.1 on; TOML files have none.

- [x] **M0.2 — core: errors and SQLSTATE codes**
  - Depends: M0.1
  - Do: `SqlState` (five ASCII characters, `Display`, class accessor) and
    named constants for every code the docs use; `ErrorContext` for adding
    "while doing X" context; the error conventions in
    `docs/subsystems/core.md`.
  - Accept:
    - Every SQLSTATE that appears in `docs/` has a constant (a test greps
      the docs and fails on a missing one).
    - Constants are valid codes (`[0-9A-Z]{5}`), and the class of each is
      its first two characters.
  - Verify: `cargo test -p hardshell-core`
  - Note: 59 constants, one per row of core.md's table (`428C9` was used
    by M7.8 but missing from the table, and is added). Every code and name
    was checked against PostgreSQL's documentation (Appendix A) and its
    `errcodes.txt` on 2026-11-02.
  - Note: the docs test reads a five-character word with a digit as a
    SQLSTATE when it is a whole code span, or anywhere when it is in one of
    PostgreSQL 17.4's 42 classes; `NOT_SQLSTATES` lists the exceptions
    (today only `--iterations 20000`). It also checks that the table, the
    constants' names, their doc comments and `SqlState::ALL`'s order agree.
  - Note: notes added with `ErrorContext` are type-erased (`ContextError`),
    so any error takes one without opting in; the conventions are in
    core.md, "Error conventions". `CoreError` waits for M0.3.

- [x] **M0.3 — core: CRC-32C, varints, hashing, RNG**
  - Depends: M0.1
  - Do: table-driven CRC-32C (Castagnoli) with incremental update; LEB128
    `u64` varints with a checked decoder; FNV-1a 64 and a deterministic fast
    hasher with `FastHashMap`/`FastHashSet` aliases; `Pcg32`.
  - Accept:
    - CRC-32C of `"123456789"` is `0xE3069283`; incremental equals one-shot.
    - Varint decode never panics, rejects overlong and truncated encodings,
      and round-trips every `u64` tested (property test).
    - FNV-1a and `Pcg32` match their reference vectors.
  - Verify: `cargo test -p hardshell-core`
  - Note (from M0.2): the varint decoder's errors start `CoreError`, which
    implements `hardshell_core::error::SqlStateError` (core.md, "Error
    conventions"). Its own code is this task's choice; callers reclassify
    it into their own variants (storage `XX001`, wire `08P01`).
  - Note: ADR-0009 accepted (2026-11-02).
  - Note: `CoreError` reports `XX001`. Its variants `VarintTruncated`,
    `VarintOverlong` and `VarintOverflow` carry no offsets: the decoder
    sees only a slice that starts at the varint, so a caller adds the
    position when it reclassifies. Zigzag `i64` varints were dropped for
    lack of a consumer; `bytes::Reader` and `Writer` wait for their first
    decoder. `crc32c_append(crc, bytes)` replaces the planned
    `Crc32c::update`: a finished checksum is the whole state, so
    continuing one needs no struct.
  - Note: reference vectors, fetched and re-derived on 2026-11-02: the CRC
    catalogue's CRC-32/ISCSI check value and RFC 3720 appendix B.4;
    `test_fnv.c` from the FNV reference suite; round 1 of pcg-c-basic's
    `pcg32-demo` (the same values as round 1 of pcg-c's
    `test-high/expected/check-pcg32.out`, less its `Again:` line).
  - Note: `FastHashMap` iteration order is the same in every run but not
    across targets (hashbrown's probe groups are 16 wide with SSE2 and 8
    wide on aarch64), so AGENTS.md's determinism rule now asks for a sort
    wherever order must match across targets.
  - Note: `FastHasher` folds each 64-bit word into its state (a 128-bit
    product whose halves are XORed) and folds once more in `finish`.
    `Pcg32::range` and `shuffle` match the reference exactly; `next_u64`
    takes its first output as the low half.

- [x] **M0.4 — core: property-test harness and logging**
  - Depends: M0.3
  - Do: `testing::check` and `testing::check_sized` (seeded from
    `HARDSHELL_SEED`, fixed in CI, failure prints seed and case);
    `log` macros with a host-installed sink and a no-op default.
  - Accept: a failing property reports a seed that reproduces it; logging
    with no sink allocates nothing (test with a counting allocator in the
    test binary).
  - Verify: `cargo test -p hardshell-core`
  - Note (from M0.3): seeding case `i` as `Pcg32::new(seed, i)` lets the
    reported seed and case replay that case alone, and lets `check_sized`
    rebuild it to shrink. M0.3's property tests are plain seeded loops;
    moving them onto `check` is optional.
  - Note: ADR-0017 accepted (2026-11-03): the no-sink test counts
    allocations with a counting allocator in `crates/core/tests/log_alloc.rs`,
    the one `unsafe` in the workspace, under an item-level allow. M0.5's
    `unsafe-audit` allows exactly that shape.
  - Note: case `i` of the property `name` draws from
    `Pcg32::new(seed ^ fnv1a64(name), i)`, so properties sharing a seed get
    different inputs. A panic in the generator, the property or the input's
    drop is caught and reported with the seed. `check_sized` shrinks by
    scanning sizes upward on the failing case's stream. An empty
    `HARDSHELL_SEED` means unset; `DEFAULT_SEED` is 42; 0 cases panic.
  - Note: the sink is a `OnceLock`, set once (`set_sink` returns a refused
    sink as `Err`), so reading it takes no lock; there is no level
    threshold (core.md, open questions). `assert_approx!` is not built: no
    test needs it yet. M0.3's property tests stay plain loops.

- [x] **M0.5 — xtask: deps, layers, unsafe-audit, headers, ci**
  - Depends: M0.1
  - Do:
    - `deps`: parse `cargo metadata --format-version 1` with a small JSON
      reader in `xtask`; fail on any package with a non-null `source` not
      in `xtask/deps-allowlist.txt` (empty), and on path packages outside
      the workspace.
    - `layers`: the dependency matrix as a `const` table, compared with
      every member's normal, dev and build dependencies; a unit test
      compares the table with `docs/01-architecture.md`.
    - `unsafe-audit`: no `unsafe` outside `hardshell-os` and ADR-0017's
      counting allocators (an `unsafe impl GlobalAlloc` with an item-level
      `allow(unsafe_code)` in a crate's `tests/`, today
      `crates/core/tests/log_alloc.rs`); no `allow(unsafe_code)` anywhere
      else at M0; every `unsafe` preceded by `// SAFETY:`; no `std::fs`
      outside `hardshell-os`, `xtask`, the apps and tests (all engine file
      access goes through the VFS); prints per-crate counts.
    - `headers`: every source file starts with the Apache-2.0 header from
      `LICENSE`'s appendix (ADR-0015); add it to every existing file.
    - `release-no-testing`: the `testing` feature is not in the non-dev
      tree of `hardshell-cli` or `hardshell-server`.
    - `ci`: fmt check, clippy `-D warnings`, `test`, then each check above;
      stops at the first failure and prints a summary table.
  - Accept: each check fails on a deliberately broken fixture in its own
    unit tests and passes on the workspace; exit codes 0 pass, 1 fail,
    2 usage error.
  - Verify: `cargo xtask ci`
  - Note (from M0.1): every `.rs` file already carries the header. Decide
    which other files `headers` covers (TOML manifests and configs have
    none yet).
  - Note: `headers` covers what ADR-0015 calls source: `.rs` (`//`),
    scripts `.sh`, `.bash`, `.py`, `.ps1` and `.github/workflows/*.yml`
    (`#`). Manifests, configs, Markdown, the allowlist and fixtures carry
    none. Every existing source file already had the header.
  - Note: rustdoc compiles doc examples without `[lints]`, so
    `unsafe_code = "deny"` does not stop an `unsafe` block in one (checked
    with Rust 1.99.0, as is that lint flags in `RUSTDOCFLAGS` do not reach
    them either). Every library root now carries
    `#![doc(test(attr(forbid(unsafe_code))))]`, so the compiler rejects
    one, even with its own `allow` (user decision, 2026-11-04);
    `unsafe-audit` fails a library root without it.
  - Note: `unsafe-audit` allows `unsafe` in `hardshell-os` only once an
    accepted ADR is listed in `OS_UNSAFE_ADRS` (`xtask/src/unsafe_audit.rs`,
    empty). A `// SAFETY:` comment covers one `unsafe`, not the next.
    `std::os::*::fs` counts as `std::fs`; so does what could hide one: a
    glob or rename of `std`, or an import of `std::os` or a whole OS module
    (`use std::os::unix`), though a named item (`OsStrExt`) is fine. The audit judges a file by where
    it sits, so every target's root must lie in its own package (a
    library's, binary's or build script's not in `tests/`), and a `#[path]`
    module or `include!` must name a `.rs` file, by a plain literal, in the
    same top-level directory of its package. `#!` starts a shebang only as
    rustc reads it (`#!/**/[...]` is an attribute). Outside the packages it
    skips Cargo's build directories and other checkouts (a nested Git
    worktree).
  - Note: every Cargo command `xtask` runs that resolves dependencies is
    `--locked --offline`, `ci`'s clippy and test included; the checks find
    the workspace from `xtask`'s manifest directory. `xtask` prints through
    writes that ignore a closed pipe, so it needs no print lint allowance
    and keeps its exit status under `| head`. Its tests run real Cargo on
    throwaway workspaces in the system temporary directory for `deps`,
    `layers` and `release-no-testing`, and `xtask/tests/cli.rs` runs the
    binary for its exit statuses.

- [ ] **M0.6 — xtask: fuzz-smoke harness**
  - Depends: M0.5, M0.3
  - Do: `cargo xtask fuzz-smoke [--secs N] [--target T]`: seeds from
    `fuzz/corpus/<target>/` run unchanged, then mutated (bit flips, byte
    insert/delete, splice, length-field tweaks, interesting integers);
    each input runs under `catch_unwind` with a 2 s hang watchdog; findings
    saved to `fuzz/crashes/<target>/` (git-ignored) and deduplicated by
    panic location; seed printed; `HARDSHELL_SEED` honored. Register a
    first target for `varint` decoding.
  - Accept: a test target that panics on a magic byte is found within the
    unit test's budget; a clean run exits 0, a finding exits 1.
  - Verify: `cargo test -p xtask`; `cargo xtask fuzz-smoke --secs 5`
  - Note (from M0.3): `varint::decode_u64` accepts only the shortest
    encoding, so the `varint` target can also check that a decoded value
    re-encodes to exactly the bytes it used. `Pcg32::range` panics on a
    bound of 0: a mutator picking a position in an input that may be empty
    checks first.
  - Note (from M0.4): `testing` parses `HARDSHELL_SEED` (decimal, or `0x`
    and hex digits; empty means unset) in a private `seed_from`. Make it
    public and share it rather than parse the variable another way; M2.6's
    `cargo xtask crash` likewise.

- [ ] **M0.7 — bench: harness**
  - Depends: M0.4
  - Do: `hardshell-bench`: warm-up, N samples, median and p95, a
    text baseline (`bench/baseline.txt`: `name median_ns p95_ns`), and a
    report of changes beyond a threshold; `cargo xtask bench [filter]`
    runs it in release mode. One bench for CRC-32C throughput.
  - Accept: the report compares against the baseline and flags a
    regression larger than 10%; the bench never fails CI (report only).
  - Verify: `cargo xtask bench crc32c`
  - Note (from M0.5): `unsafe-audit` allows `std::fs` only in
    `hardshell-os`, `xtask`, the apps and `tests/`. If the harness itself
    reads `bench/baseline.txt`, either `cargo xtask bench` reads it and
    passes it on, or adding `hardshell-bench` to `FS_PACKAGES` is a
    decision to record.

- [ ] **M0.8 — CI workflow and repository hygiene**
  - Depends: M0.5, M0.6
  - Do:
    - `.github/workflows/ci.yml`: matrix `ubuntu-latest`, `macos-latest`,
      `windows-latest`; pinned toolchain; `core.autocrlf` off before
      checkout; `cargo xtask ci`; Linux only:
      `cargo xtask fuzz-smoke --secs 30`.
    - Every action pinned by full commit SHA with a `# vX` comment;
      top-level `permissions: contents: read`;
      `persist-credentials: false` on checkout; no `pull_request_target`.
      `cargo xtask workflows` checks these rules and joins `cargo xtask ci`.
    - `.github/dependabot.yml` for GitHub Actions only;
      `.github/pull_request_template.md` with the definition-of-done
      checklist.
  - Accept: the workflow passes YAML review; the first run on a PR is
    green on all three OSes.
  - Verify: `cargo xtask workflows`; YAML review; the CI run on the first
    PR (human)
  - Note (from M0.5): `workflows` joins `CHECKS` in `xtask/src/main.rs`,
    whose order `ci` follows. `headers` already requires the Apache-2.0
    header, as `#` comments, in `.github/workflows/*.yml`; `dependabot.yml`
    and the PR template need none. Run `cargo --locked xtask ci`: the
    `cargo xtask` alias alone would refresh a stale lockfile before the
    checks see it.

**Exit criteria (M0):** a PR to `main` runs every check on three OSes, and
a deliberately added third-party crate, forbidden layer edge, or `unsafe`
block (outside ADR-0017's test counting allocators) fails CI.

---

## M1 — OS layer, file format and pager

**Goal:** checksummed pages written and read back through a buffer pool,
on every OS, through a VFS that tests can replace. No WAL yet: in this
milestone writes go through a `PageSink` that M1's tests implement as a
direct write; M2 replaces it with the WAL.

- [ ] **M1.1 — os: `Vfs` trait, `MemVfs`, `StdVfs` on Linux**
  - Depends: M0.2
  - Do: the `Vfs` and `VfsFile` traits from `docs/subsystems/os-and-vfs.md`
    (positional read and write, `sync_data`, `sync_all`, `len`, `set_len`,
    shared and exclusive locks, directory sync, delete, temp files);
    `MemVfs`; `StdVfs` using `std` only (positional I/O through
    `std::os::unix::fs::FileExt`, locks through `File::try_lock`).
  - Accept: one contract test suite runs against both `MemVfs` and
    `StdVfs` (short reads, reads past the end, extending writes, lock
    conflicts between two handles, delete while open is an error on
    `MemVfs` as on Windows).
  - Verify: `cargo test -p hardshell-os`

- [ ] **M1.2 — os: `StdVfs` on macOS and Windows; sync semantics recorded**
  - Depends: M1.1
  - Do:
    - Windows positional I/O through `std::os::windows::fs::FileExt`.
    - Read the pinned toolchain's `std` source and record in
      `docs/03-platforms.md` exactly which OS call `sync_all` and
      `sync_data` make on each OS (macOS: whether `F_FULLFSYNC` is used),
      and how a directory is synced on each. If `std` falls short, stop
      and propose an ADR under ADR-0013.
  - Accept: the contract suite passes on all three OSes; the platform doc
    names the call per OS, citing the `std` source file and line for the
    pinned version.
  - Verify: `cargo test -p hardshell-os` on each OS (CI)

- [ ] **M1.3 — os: `FaultVfs`**
  - Depends: M1.1
  - Do: behind feature `testing`, a VFS over `MemVfs` that records writes
    since each file's last sync and, on `crash()`, produces the post-crash
    image under a seeded policy: drop all unsynced writes, keep a random
    subset, tear writes at 512-byte sector boundaries, or apply them out of
    order. It also injects an I/O error on the Nth read, write or sync.
  - Accept: unit tests for each policy; the same seed gives the same image;
    synced data always survives.
  - Verify: `cargo test -p hardshell-os --features testing`

- [ ] **M1.4 — storage: file header and page format**
  - Depends: M1.1, M0.3
  - Do: page 0 header (magic, format version 0, page size, page count,
    free-list head, catalog root, schema version, flags, CRC-32C); the
    common page header and checksum trailer; decoders that never panic.
    Register fuzz target `page`.
  - Accept: header round-trips; every bit flip in a header or page is
    detected (property test); unsupported versions and page sizes give
    clear errors; the fuzz target runs clean for 60 s.
  - Verify: `cargo test -p hardshell-storage`;
    `cargo xtask fuzz-smoke --target page --secs 60`

- [ ] **M1.5 — storage: buffer pool and pager**
  - Depends: M1.4
  - Do: fixed frame array sized by `cache_size`; clock-sweep eviction; pin
    guards (`PageRef`, `PageMut`); CRC check on every read from the file
    (`XX001` on mismatch); hit/miss counters; writes through the
    `PageSink` trait.
  - Accept: model-based property test (random fetch/pin/unpin/dirty
    sequences against a reference model); pinned frames are never evicted;
    with every frame pinned a fetch fails with `53200` instead of
    blocking or panicking.
  - Verify: `cargo test -p hardshell-storage`
  - Note (from M0.2): a review questioned `53200` (`out_of_memory`) for an
    all-pinned pool, since PostgreSQL keeps that code for failed
    allocations; class 53's general code, `insufficient_resources`, may fit
    better. Decide with the user before relying on it.

- [ ] **M1.6 — storage: free list and page allocation**
  - Depends: M1.5
  - Do: free-list trunk and leaf pages; allocate (reuse before growing the
    file) and free; page count in the header.
  - Accept: property test that no page is both free and in use and that
    every page is accounted for after random allocate/free sequences.
  - Verify: `cargo test -p hardshell-storage`

**Exit criteria (M1):** pages survive a write/close/reopen cycle on all
three OSes, and corruption of any byte is detected on read.

---

## M2 — WAL, transactions and crash safety

**Goal:** committed writes survive every crash `FaultVfs` can produce;
readers see stable snapshots while one writer works (ADR-0007, ADR-0008,
ADR-0009). Starts by settling those three ADRs with the user.

- [ ] **M2.1 — storage: WAL frame format and writer**
  - Depends: M1.6, M1.3
  - Do: the WAL header and frame format from
    `docs/subsystems/wal-and-transactions.md` (page number, commit marker,
    salts, cumulative CRC-32C); append; sync at commit per `durability`;
    `crash_point!` macro under `testing`. Register fuzz target `wal`.
  - Accept: frames round-trip; any change to a frame, or a torn last frame,
    stops the scan at that frame (property test); fuzz target clean 60 s.
  - Verify: `cargo test -p hardshell-storage`;
    `cargo xtask fuzz-smoke --target wal --secs 60`

- [ ] **M2.2 — storage: WAL index and snapshot reads**
  - Depends: M2.1
  - Do: the in-memory WAL index (page → frames, per commit); `Snapshot`
    (main file + WAL up to a commit); the read path (WAL first, then main
    file); reader marks.
  - Accept: a reader opened before a commit never sees it; a reader opened
    after sees all of it (model-based test with interleaved readers).
  - Verify: `cargo test -p hardshell-storage`

- [ ] **M2.3 — storage: write transactions**
  - Depends: M2.2
  - Do: the write lock with busy timeout (`55P03`); dirty pages in the
    pool; spilling uncommitted frames when the pool is full; commit;
    rollback; `40001` when a transaction on an old snapshot tries to
    write; the debug-build lock-order assertion from
    `docs/01-architecture.md`.
  - Accept: a transaction larger than the cache commits and rolls back
    correctly; concurrent readers never block; a second writer waits then
    times out with `55P03`; stale upgrade gives `40001`.
  - Verify: `cargo test -p hardshell-storage`
  - Note (from M0.3): the dirty-page map is a `FastHashMap`, whose order
    can differ between targets. Writing a commit's frames in page-number
    order, not map order, keeps WAL bytes and `FaultVfs` call sequences
    the same on every target, so a crash-campaign seed replays anywhere.

- [ ] **M2.4 — storage: checkpoint, WAL reset and clean close**
  - Depends: M2.3
  - Do: passive checkpoint up to the oldest reader mark; sync the main file
    before the WAL restarts with new salts; auto-checkpoint at
    `wal_autocheckpoint`; close does a full checkpoint, removes the WAL and
    syncs the directory; optional background checkpointer thread.
  - Accept: a long reader holds back the checkpoint without losing its
    snapshot; after a clean close the database is one file whose contents
    equal the model.
  - Verify: `cargo test -p hardshell-storage`

- [ ] **M2.5 — storage: recovery and poisoned handles**
  - Depends: M2.4
  - Do: recovery on open (scan, validate, rebuild the index, ignore the
    invalid tail); `read_only` open fails if recovery is needed; any failed
    sync poisons the `Database` (`58030` for every later write) per
    ADR-0009.
  - Accept: injected sync errors poison the handle and reopening recovers
    to the last durable commit.
  - Verify: `cargo test -p hardshell-storage --features testing`

- [ ] **M2.6 — storage: crash campaign**
  - Depends: M2.5
  - Do: `crates/storage/tests/crash.rs`: random page-level transactions
    under `FaultVfs`, a crash at every crash point and at random write
    boundaries, recovery, and comparison with a model of the committed
    state (every acknowledged commit present, nothing uncommitted visible,
    every page checksum valid). `cargo xtask crash [--iterations N]` runs
    it; CI runs a short campaign on Linux.
  - Accept: 100,000 iterations clean, with the seed recorded in a note; a
    deliberately broken commit path (sync skipped) is caught within 1,000
    iterations.
  - Verify: `cargo xtask crash --iterations 100000`

- [ ] **M2.7 — storage: process-kill test**
  - Depends: M2.5
  - Do: a test that runs a child process committing transactions through
    `StdVfs` and reporting each acknowledged commit over a pipe, kills it
    at a random time (`Child::kill`), reopens, and checks every
    acknowledged commit is present. This tests process crashes, not power
    loss: the OS page cache survives a kill. Say so in the test's docs.
  - Accept: 200 kill cycles clean on each OS.
  - Verify: `cargo test -p hardshell-storage --test kill --release` on each
    OS (CI)

**Exit criteria (M2):** the crash campaign and kill test are clean on all
three OSes, and ADR-0007, ADR-0008 and ADR-0009 are accepted.

---

## M3 — B+trees

**Goal:** ordered byte-key trees on top of pages, transactional and
crash-safe, checked against `BTreeMap`.

- [ ] **M3.1 — storage: B+tree node format and search**
  - Depends: M2.3
  - Do: slotted leaf and interior pages (cell pointer array, cells of
    key/value or key/child); binary search by `memcmp`; extend the `page`
    fuzz target to tree pages.
  - Accept: node decode never panics; search finds every key of a node
    built from random sorted keys.
  - Verify: `cargo test -p hardshell-storage`

- [ ] **M3.2 — storage: insert, splits and cursors**
  - Depends: M3.1
  - Do: insert with leaf and interior splits; cursors with `seek` (≥, >,
    ≤, <), `next`, `prev`, `key`, `value`; trees created with a root page id.
  - Accept: model-based property test against `BTreeMap<Vec<u8>, Vec<u8>>`
    for random inserts and range scans in both directions.
  - Verify: `cargo test -p hardshell-storage`

- [ ] **M3.3 — storage: delete, merge and redistribution**
  - Depends: M3.2
  - Do: delete with merge or redistribution; freed pages go back to the
    free list; dropping a whole tree.
  - Accept: the model test with deletes; after deleting every key the tree
    is one empty root and all other pages are free.
  - Verify: `cargo test -p hardshell-storage`

- [ ] **M3.4 — storage: large values and key limits**
  - Depends: M3.3
  - Do: overflow page chains for values larger than a cell, up to 1 GiB
    (`54000` past it); a maximum key size of about a quarter page (`54000`
    past it, with the limit in the message).
  - Accept: values from 0 bytes to 64 MiB round-trip; freeing a value frees
    its whole chain (page accounting test).
  - Verify: `cargo test -p hardshell-storage`

- [ ] **M3.5 — storage: tree checker, crash coverage and `open` fuzzing**
  - Depends: M3.4, M2.6
  - Do: `BTree::check()` (key order, fill bounds, parent/child links,
    overflow chains, page accounting with the free list); extend the crash
    campaign to tree operations; register fuzz target `open` (open an
    arbitrary byte string as a database in `MemVfs`, walk it with the
    checker; errors fine, panics not); benches for insert and lookup.
  - Accept: the checker runs after every crash-campaign recovery; fuzz
    target `open` clean for 60 s; bench baselines recorded.
  - Verify: `cargo xtask crash --iterations 100000`;
    `cargo xtask fuzz-smoke --target open --secs 60`;
    `cargo xtask bench btree`

**Exit criteria (M3):** a million random tree operations with crashes
interleaved match the model, on all three OSes.

---

## M4 — Types, values and encodings

**Goal:** every planned SQL type parses, prints, compares, casts and
encodes exactly as `docs/subsystems/types-and-values.md` specifies.

- [ ] **M4.1 — types: `DataType`, `Value`, booleans, integers, floats, text, bytea**
  - Depends: M0.4
  - Do: the type and value enums; NULL; three-valued logic helpers;
    checked integer arithmetic (`22003`), division by zero (`22012`);
    floats with PostgreSQL's ordering of NaN; UTF-8 validation (`22021`).
  - Accept: property tests for arithmetic against `i128`/`f64` references;
    every overflow path tested.
  - Verify: `cargo test -p hardshell-types`

- [ ] **M4.2 — types: `NUMERIC`**
  - Depends: M4.1
  - Do: arbitrary-precision decimal (precision ≤ 1000); add, subtract,
    multiply, divide (PostgreSQL's result-scale rules), compare, round
    half away from zero; `NUMERIC(p,s)` coercion (`22003` when it does not
    fit).
  - Accept: property tests against `i128` for values that fit; a table of
    PostgreSQL results for division and rounding edge cases, each cited
    (from PostgreSQL's documentation or regression tests, or marked
    "verify against psql in M9.6").
  - Verify: `cargo test -p hardshell-types`

- [ ] **M4.3 — types: dates, times, timestamps, intervals**
  - Depends: M4.1
  - Do: `DATE`, `TIME`, `TIMESTAMP`, `TIMESTAMPTZ` (UTC and fixed offsets
    only), `INTERVAL` (months, days, microseconds); ISO 8601 input and
    output; arithmetic; range checks (`22008`).
  - Accept: civil-date conversion property tests (round-trip over the
    whole range, leap years); interval arithmetic edge cases (month ends).
  - Verify: `cargo test -p hardshell-types`

- [ ] **M4.4 — types: `UUID`, text I/O for every type, casts**
  - Depends: M4.2, M4.3
  - Do: `UUID`; text input and output for every type; the cast table
    (implicit, assignment, explicit) from the types spec; register fuzz
    target `types-text`.
  - Accept: `parse(format(v)) == v` for every type (property test);
    invalid text gives `22P02` or the type's own code; fuzz clean 60 s.
  - Verify: `cargo test -p hardshell-types`;
    `cargo xtask fuzz-smoke --target types-text --secs 60`

- [ ] **M4.5 — types: order-preserving key encoding**
  - Depends: M4.4
  - Do: `encode_key`/`decode_key` for every type and for composite keys,
    with `ASC`/`DESC` and `NULLS FIRST`/`LAST` (ADR-0006); register fuzz
    target `key`.
  - Accept: for random value pairs of every type,
    `memcmp(enc(a), enc(b))` equals SQL comparison (property test); decode
    round-trips; fuzz clean 60 s.
  - Verify: `cargo test -p hardshell-types`;
    `cargo xtask fuzz-smoke --target key --secs 60`

- [ ] **M4.6 — types: row encoding**
  - Depends: M4.4
  - Do: the row format (column count, null bitmap, varint lengths, values),
    readable by a schema with more columns than the row (added columns
    read as their default); register fuzz target `row`.
  - Accept: round-trip property test; a row written with N columns reads
    under a schema of N+k columns; fuzz clean 60 s.
  - Verify: `cargo test -p hardshell-types`;
    `cargo xtask fuzz-smoke --target row --secs 60`

**Exit criteria (M4):** the types spec has no open questions left for the
types M6 and M7 use.

---

## M5 — SQL front end

**Goal:** parse the statements M6 and M7 need, with exact error positions,
and reject everything else with `0A000` or `42601` (ADR-0011).

- [ ] **M5.1 — sql: lexer**
  - Depends: M0.2
  - Do: tokens with byte spans: identifiers (unquoted folded to lower
    case; quoted with `""` escapes), keywords, string constants (`''`,
    `E''` escapes, dollar quoting), numbers, operators, `$n` parameters,
    `--` and nested `/* */` comments.
  - Accept: unit tests per token kind; unterminated strings and comments
    report the start position.
  - Verify: `cargo test -p hardshell-sql`
  - Note (from M0.2): a review questioned `42601` for `$0` and parameter
    numbers above 65535 (`docs/subsystems/sql-frontend.md`): PostgreSQL may
    report a missing parameter as `undefined_parameter`. Cite PostgreSQL's
    behavior or mark the test "verify against psql in M9.6".

- [ ] **M5.2 — sql: expressions**
  - Depends: M5.1
  - Do: Pratt parser with PostgreSQL's precedence: literals, columns,
    parameters, unary and binary operators, `IS [NOT] NULL`,
    `IS [NOT] DISTINCT FROM`, `BETWEEN`, `IN (...)`, `LIKE`/`ILIKE`,
    `CASE`, `CAST` and `::`, function calls (with `DISTINCT` and `*` for
    aggregates), scalar subqueries, `EXISTS`.
  - Accept: a precedence table test (each pair of operators parsed and
    printed with explicit parentheses).
  - Verify: `cargo test -p hardshell-sql`

- [ ] **M5.3 — sql: DDL statements**
  - Depends: M5.2
  - Do: `CREATE TABLE` (types, `NOT NULL`, `DEFAULT`, `CHECK`,
    `PRIMARY KEY`, `UNIQUE`, `REFERENCES`, table constraints,
    `IF NOT EXISTS`), `DROP TABLE`, `CREATE [UNIQUE] INDEX`, `DROP INDEX`,
    `CREATE VIEW`, `DROP VIEW`, `CREATE SEQUENCE`, `DROP SEQUENCE`,
    `CASCADE`/`RESTRICT`.
  - Accept: every form in `docs/sql-reference.md` marked for M5–M7 parses;
    known-but-unsupported clauses give `0A000` naming the clause.
  - Verify: `cargo test -p hardshell-sql`

- [ ] **M5.4 — sql: queries, DML and utility statements**
  - Depends: M5.2
  - Do: `SELECT` (`DISTINCT`, joins, `WHERE`, `GROUP BY`, `HAVING`,
    `ORDER BY ... NULLS FIRST/LAST`, `LIMIT`/`OFFSET`/`FETCH FIRST`, set
    operations, `WITH`), `VALUES`, `INSERT` (`VALUES`, `SELECT`,
    `DEFAULT`, `RETURNING`), `UPDATE`, `DELETE`, `BEGIN`/`START
    TRANSACTION`, `COMMIT`, `ROLLBACK`, `SET`/`SHOW`, `EXPLAIN`.
  - Accept: as M5.3.
  - Verify: `cargo test -p hardshell-sql`

- [ ] **M5.5 — sql: printer, round-trip and fuzzing**
  - Depends: M5.3, M5.4
  - Do: `Display` for the AST printing valid SQL; a random AST generator;
    depth and size limits (`54001`); register fuzz target `sql` seeded from
    the test statements.
  - Accept: `parse(print(ast)) == ast` for generated ASTs (property test);
    a 10,000-deep expression is `54001`, not a stack overflow; fuzz clean
    for 60 s.
  - Verify: `cargo test -p hardshell-sql`;
    `cargo xtask fuzz-smoke --target sql --secs 60`

**Exit criteria (M5):** the parser accepts every statement in the SQL
reference marked for M6 and M7, and nothing it does not mark.

---

## M6 — First queries end to end

**Goal:** `CREATE TABLE`, `INSERT`, `SELECT ... WHERE ... ORDER BY ...
LIMIT`, `UPDATE` and `DELETE` through the embedded API, durable, and
tested by SQL logic tests.

- [ ] **M6.1 — catalog: system tables and schema loading**
  - Depends: M3.5, M4.6
  - Do: bootstrap the system tables (`hs_tables`, `hs_columns`,
    `hs_indexes`, `hs_constraints`, `hs_views`, `hs_sequences`, `hs_stats`)
    on create; load a `Schema` at a snapshot, cached by schema version;
    create and drop tables and indexes as catalog operations.
  - Accept: schema survives close and reopen; a reader on an old snapshot
    keeps the old schema; the crash campaign covers DDL.
  - Verify: `cargo test -p hardshell-catalog`

- [ ] **M6.2 — plan: binder**
  - Depends: M6.1, M5.5
  - Do: name resolution, column types, untyped literal and parameter type
    inference, implicit casts per the types spec; errors `42P01`, `42703`,
    `42702`, `42804`, `42883`, `42P07`.
  - Accept: unit tests per error; every bound expression has a type.
  - Verify: `cargo test -p hardshell-plan`

- [ ] **M6.3 — plan and exec: single-table queries**
  - Depends: M6.2
  - Do: logical and physical plans for scan, filter, project, sort
    (in memory, within `work_mem`, else `53200`), limit/offset; the
    expression evaluator with three-valued logic; scalar functions set 1
    from the SQL reference.
  - Accept: unit tests per operator; a sort larger than `work_mem` fails
    with `53200`.
  - Verify: `cargo test -p hardshell-plan -p hardshell-exec`
  - Note (from M0.2): a review questioned `53200` (`out_of_memory`) for a
    `work_mem` breach, which is a configured budget rather than a failed
    allocation; PostgreSQL reports `temp_file_limit` as `53400`. Raise it
    with the user together with ADR-0012, which is still Proposed.

- [ ] **M6.4 — exec: DML and constraints**
  - Depends: M6.3
  - Do: `INSERT` (`VALUES`, `SELECT`, defaults), `UPDATE`, `DELETE`;
    `NOT NULL` (`23502`), `CHECK` (`23514`), `PRIMARY KEY` and `UNIQUE`
    through unique indexes (`23505`); secondary index maintenance; a
    failing statement leaves the table unchanged.
  - Accept: every table's indexes match its rows after random DML
    (checker test).
  - Verify: `cargo test -p hardshell-exec`

- [ ] **M6.5 — facade: embedded API v0**
  - Depends: M6.4
  - Do: `Database`, `OpenOptions`, `Connection`, `Statement`, `Rows`,
    `Row`, `Transaction`, `Value`, `ToValue`/`FromValue`, `params!`,
    `Error` with `sqlstate()`, `InterruptHandle`, `set_log_sink`;
    autocommit; `BEGIN`/`COMMIT`/`ROLLBACK`; the failed-transaction state
    (`25P02`); `SET`/`SHOW` for the settings in `docs/01-architecture.md`;
    `execute_batch`; notices (`take_notices`); deferred and immediate
    transactions, deciding with the user the SQL spelling of the immediate
    form (`docs/subsystems/api.md`, open questions); the process-wide
    registry of open files (ADR-0005).
  - Accept: the API sketch in `docs/01-architecture.md` compiles and runs
    as a doc test; interrupt cancels a long scan with `57014`;
    `Database: Send + Sync`, `Connection: Send` (compile-time assertions).
  - Verify: `cargo test -p hardshelldb`
  - Note (from M0.2): a review questioned `22004` for a `NULL` read into a
    non-`Option` type: the SQL standard's `null_value_no_indicator_parameter`
    names exactly that case (a `NULL` fetched into a host variable with no
    indicator). Decide with the user before M6.5 fixes it in the API.
  - Note (from M0.4): `hardshell_core::log::set_sink` fills a set-once slot
    and returns a refused sink as `Err`, so `set_log_sink` must report a
    second call somehow (api.md). Decide there whether the sink carries a
    maximum level (core.md, open questions).

- [ ] **M6.6 — facade: SQL logic test runner**
  - Depends: M6.5
  - Do: `crates/hardshelldb/tests/slt.rs` runs `tests/slt/**/*.slt` in the
    sqllogictest format (`statement ok`, `statement error <SQLSTATE>`,
    `query <types> [nosort|rowsort|valuesort]`, `----` results, `#`
    comments, `halt`); each file gets a fresh `MemVfs` database; failures
    print file, line, expected and actual. Seed corpus for M4–M6 features.
  - Accept: a deliberately wrong expectation fails with its file and line;
    the corpus passes.
  - Verify: `cargo test -p hardshelldb --test slt`

- [ ] **M6.7 — facade: SQL-level crash campaign**
  - Depends: M6.6, M2.6
  - Do: `crates/hardshelldb/tests/crash.rs`: random SQL workloads (DDL and
    DML, explicit and autocommit transactions) under `FaultVfs`, crashes,
    recovery, then comparison with the same committed statements replayed
    on an uncrashed `MemVfs` database; `cargo xtask crash` runs both the
    storage and the SQL campaigns.
  - Accept: 20,000 iterations clean; seed recorded.
  - Verify: `cargo xtask crash --iterations 20000`

**Exit criteria (M6):** a small program using the embedded API creates,
fills, queries and updates a table on all three OSes, and both crash
campaigns are clean.

---

## M7 — Relational core

**Goal:** the SQL most applications use day to day.

- [ ] **M7.1 — joins**
  - Depends: M6.6
  - Do: `INNER`, `LEFT`, `RIGHT`, `FULL`, `CROSS` joins with `ON`,
    `USING`, `NATURAL`; nested-loop and in-memory hash join (`53200` past
    `work_mem` until M9.4).
  - Accept: SLT files per join type, including NULL join keys; each join
    type gives the same rows with either algorithm (forced-plan test).
  - Verify: `cargo test -p hardshelldb --test slt`;
    `cargo test -p hardshell-exec`

- [ ] **M7.2 — aggregates, `GROUP BY`, `HAVING`, `DISTINCT`**
  - Depends: M6.6
  - Do: `count`, `sum`, `avg`, `min`, `max`, `bool_and`, `bool_or`,
    `string_agg`; `DISTINCT` and `FILTER (WHERE ...)`; hash and sort
    aggregation;
    PostgreSQL's result types (`sum(integer)` is `bigint`, `sum(bigint)` and
    `avg` of integers are `numeric`).
  - Accept: SLT coverage of empty groups, NULL inputs and overflow; both
    algorithms agree.
  - Verify: `cargo test -p hardshelldb --test slt`

- [ ] **M7.3 — subqueries**
  - Depends: M7.1
  - Do: scalar subqueries (`21000` for more than one row), `IN`/`NOT IN`
    with PostgreSQL's NULL semantics, `EXISTS`, correlated subqueries
    (evaluated per row; decorrelation is M9).
  - Accept: SLT coverage including `NOT IN` with a NULL in the subquery.
  - Verify: `cargo test -p hardshelldb --test slt`

- [ ] **M7.4 — set operations and non-recursive `WITH`**
  - Depends: M7.2
  - Do: `UNION`, `INTERSECT`, `EXCEPT`, each with `ALL`; `WITH` queries
    (inlined or materialized).
  - Accept: SLT coverage, including duplicates and NULLs.
  - Verify: `cargo test -p hardshelldb --test slt`

- [ ] **M7.5 — views**
  - Depends: M7.4
  - Do: `CREATE VIEW` stores the query text; binding expands it; `DROP
    VIEW`; dependency tracking (dropping a table a view uses is `2BP01`
    unless `CASCADE`).
  - Accept: SLT coverage; views survive reopen.
  - Verify: `cargo test -p hardshelldb --test slt`

- [ ] **M7.6 — foreign keys**
  - Depends: M6.4
  - Do: `REFERENCES` with `NO ACTION`, `RESTRICT`, `CASCADE`,
    `SET NULL`, `SET DEFAULT`; checked per statement (`DEFERRABLE` is
    `0A000`); `23503`.
  - Accept: SLT coverage of every action, self-references, and multi-column
    keys with NULLs (`MATCH SIMPLE`).
  - Verify: `cargo test -p hardshelldb --test slt`

- [ ] **M7.7 — indexes in the planner**
  - Depends: M7.1
  - Do: index scans for equality and ranges on leading index columns; an
    index providing `ORDER BY`; rule-based choice (cost-based in M9.2);
    `EXPLAIN` shows the choice.
  - Accept: forced-plan tests show identical rows with and without the
    index; a point query by primary key reads O(log n) pages (page-read
    counter test).
  - Verify: `cargo test -p hardshell-plan`;
    `cargo test -p hardshelldb --test slt`

- [ ] **M7.8 — sequences and identity columns**
  - Depends: M6.4
  - Do: `CREATE SEQUENCE`, `nextval`, `currval`, `setval`, `SERIAL`,
    `BIGSERIAL`, `GENERATED {ALWAYS | BY DEFAULT} AS IDENTITY` (`428C9`
    when inserting into an `ALWAYS` column without `OVERRIDING SYSTEM
    VALUE`).
  - Accept: `nextval` never returns a value twice within a process,
    including across rollbacks; behavior after a crash is documented and
    tested against what `docs/subsystems/catalog.md` specifies.
  - Verify: `cargo test -p hardshelldb --test slt`;
    `cargo xtask crash --iterations 20000`

- [ ] **M7.9 — `RETURNING`, prepared statements, `EXPLAIN`**
  - Depends: M7.7
  - Do: `RETURNING` for `INSERT`, `UPDATE`, `DELETE`; the prepared-plan
    cache keyed by schema version; `EXPLAIN` printing the physical plan.
  - Accept: a prepared statement re-plans after a DDL change and gives
    correct results; `EXPLAIN` output is stable (golden text tests).
  - Verify: `cargo test -p hardshelldb`

- [ ] **M7.10 — scalar functions set 2**
  - Depends: M6.3, M4.3
  - Do: the string, math and date/time functions the SQL reference marks
    for M7.10 (`now()` and `current_timestamp` fixed per transaction, as
    in PostgreSQL).
  - Accept: SLT coverage of each function, including NULL inputs.
  - Verify: `cargo test -p hardshelldb --test slt`

**Exit criteria (M7):** every statement the SQL reference marks for M7
runs and is covered by SLT files; both crash campaigns are clean.

---

## M8 — CLI shell

**Goal:** `hardshell app.hsdb` is a useful interactive shell and scripting
tool.

- [ ] **M8.1 — core: argument parser and CSV codec**
  - Depends: M0.4
  - Do: `core::cli` (flags, `--key value`, positionals, generated help);
    `core::csv` (RFC 4180 reader and writer, configurable delimiter and
    quote, line numbers in errors, size limits); register fuzz target
    `csv`.
  - Accept: CSV round-trip property test; fuzz clean 60 s.
  - Verify: `cargo test -p hardshell-core`;
    `cargo xtask fuzz-smoke --target csv --secs 60`

- [ ] **M8.2 — cli: REPL and scripting basics**
  - Depends: M8.1, M6.5
  - Do: `hardshell [FILE]`, `-c SQL`, `-f FILE`, `--read-only`; statement
    splitting through the facade's `split_statements` (the SQL lexer:
    quotes, dollar quotes, comments; the CLI may not depend on `sql`); the
    aligned table output; `\q`, `\?`, `\timing`, `\x`; exit codes (0 ok,
    1 SQL error with `-c`/`-f`, 2 usage). Plain line input; line editing
    is backlog (needs an ADR under ADR-0013).
  - Accept: CLI integration tests run the binary with stdin and files and
    check output and exit codes.
  - Verify: `cargo test -p hardshell-cli`

- [ ] **M8.3 — cli: catalog commands and output formats**
  - Depends: M8.2
  - Do: the facade's schema introspection API (tables, columns, indexes,
    views); `\dt`, `\d [name]`, `\di`, `\dv` through it; output formats
    aligned (default), `--csv` and `--json`, per `docs/subsystems/cli.md`.
  - Accept: golden-text tests for each command and format.
  - Verify: `cargo test -p hardshell-cli`

- [ ] **M8.4 — cli: `\copy`, `\i`, `\o`**
  - Depends: M8.3
  - Do: `\copy table [(cols)] FROM|TO 'file' [CSV] [HEADER]` streaming
    through the facade in one transaction; errors name the CSV line;
    `\i file`; `\o file`.
  - Accept: a 1,000,000-row CSV imports and exports byte-identically
    (round-trip test with generated data).
  - Verify: `cargo test -p hardshell-cli --release`

- [ ] **M8.5 — cli: `hardshell dump` and restore**
  - Depends: M8.4, M7.8
  - Do: `hardshell dump FILE` writes schema (tables, indexes, constraints,
    views, sequences with their current values) and data as SQL; restore
    with `hardshell NEW -f dump.sql`.
  - Accept: dump → restore → dump gives identical text on generated
    databases.
  - Verify: `cargo test -p hardshell-cli`

- [ ] **M8.6 — cli: `hardshell check`**
  - Depends: M8.2, M3.5
  - Do: `hardshell check FILE` runs the storage checks that exist (page
    checksums, tree checker, page accounting) and exits non-zero on any
    problem. M11.2 extends it.
  - Accept: corrupt fixture files are reported with the page number.
  - Verify: `cargo test -p hardshell-cli`

**Exit criteria (M8):** the CLI imports a CSV, queries it, and dumps it on
all three OSes.

---

## M9 — Optimizer, statistics and memory

**Goal:** good plans for multi-join queries, bounded memory for any query,
and every performance budget measured.

- [ ] **M9.1 — plan: statistics and `ANALYZE`**
  - Depends: M7.7
  - Do: `ANALYZE [table]` collects row counts, null fractions, distinct
    estimates, most-common values and histograms from a sample into
    `hs_stats`.
  - Accept: estimates within documented error bounds on generated
    distributions (uniform, skewed, correlated).
  - Verify: `cargo test -p hardshell-plan`

- [ ] **M9.2 — plan: cost model and access paths**
  - Depends: M9.1
  - Do: selectivity estimation; costs for scans, index scans, sorts and
    joins; choice of access path by cost.
  - Accept: forced-plan equivalence tests for every rule; plan-choice tests
    on generated data.
  - Verify: `cargo test -p hardshell-plan`

- [ ] **M9.3 — plan: join ordering and decorrelation**
  - Depends: M9.2, M7.3
  - Do: dynamic programming for up to 8 relations, greedy beyond; merge
    join; decorrelation of `EXISTS`/`IN` subqueries into semi- and
    anti-joins (respecting `NOT IN`'s NULL semantics).
  - Accept: each rewrite has an equivalence test; planning a 12-way join
    takes under 50 ms.
  - Verify: `cargo test -p hardshell-plan`

- [ ] **M9.4 — exec: spilling sorts, hash joins and aggregates**
  - Depends: M7.1, M7.2
  - Do: external merge sort and partitioned hash join and aggregation using
    temporary files through the VFS; `work_mem` respected.
  - Accept: queries with `work_mem` at 64 KiB give the same rows as with
    1 GiB; temp files are removed after success, error and cancellation.
  - Verify: `cargo test -p hardshell-exec`

- [ ] **M9.5 — storage: pipelined group commit**
  - Depends: M2.6
  - Do: propose an ADR first; then let the next writer start once the
    commit frame is written, and acknowledge and publish a commit only
    after the sync covering it completes.
  - Accept: the crash campaigns stay clean; concurrent single-row commits
    from 8 threads reach at least 3× the throughput of serial commits
    (bench).
  - Verify: `cargo xtask crash --iterations 100000`;
    `cargo xtask bench commit`

- [ ] **M9.6 — quality: differential tests against PostgreSQL**
  - Depends: M7.10
  - Do: `cargo xtask diff-pg --psql <path> --dsn <dsn>` runs the SLT corpus
    and generated queries on hardshellDB and, through the `psql` program,
    on a PostgreSQL server, and reports differences; known differences are
    listed in `docs/sql-reference.md` and skipped by name. A manual or
    nightly CI job with a PostgreSQL service container runs it.
  - Accept: the corpus runs with zero unexplained differences; every
    "verify against psql" item from earlier tasks is resolved.
  - Verify: `cargo xtask diff-pg` against a local PostgreSQL (manual;
    record the PostgreSQL version in a note)

- [ ] **M9.7 — performance pass on the reference machine**
  - Depends: M9.3, M9.4, M9.5
  - Do: record the reference machine in `docs/00-overview.md`; measure
    every budget there with benches; fix violations.
  - Accept: every budget is met, or its exception is recorded in an ADR.
  - Verify: `cargo xtask bench` with notes of the numbers per OS

**Exit criteria (M9):** budgets measured and met (or excepted by ADR), and
`diff-pg` clean.

---

## M10 — SQL breadth

**Goal:** the rest of the SQL that PostgreSQL users expect from an
application database.

- [ ] **M10.1 — window functions**
  - Depends: M9.4
  - Do: `OVER (PARTITION BY ... ORDER BY ... frame)` with `ROWS` and
    `RANGE` frames; `row_number`, `rank`, `dense_rank`, `lag`, `lead`,
    `first_value`, `last_value`, `nth_value`, and aggregates as windows;
    `WINDOW` clauses.
  - Accept: SLT coverage, checked with `diff-pg`.
  - Verify: `cargo test -p hardshelldb --test slt`; `cargo xtask diff-pg`

- [ ] **M10.2 — recursive `WITH`**
  - Depends: M7.4
  - Do: `WITH RECURSIVE` with `UNION` and `UNION ALL`, bounded by
    `work_mem` and `statement_timeout`.
  - Accept: SLT coverage (trees, graphs with cycles under `UNION`).
  - Verify: `cargo test -p hardshelldb --test slt`

- [ ] **M10.3 — `ALTER TABLE`**
  - Depends: M7.6
  - Do: `ADD COLUMN` (with defaults, no table rewrite, thanks to the row
    format), `DROP COLUMN`, `RENAME COLUMN`, `RENAME TO`, `ADD`/`DROP
    CONSTRAINT`, `ALTER COLUMN SET/DROP NOT NULL`, `SET/DROP DEFAULT`,
    `ALTER COLUMN TYPE` (rewrite).
  - Accept: SLT coverage; crash campaign covers each form.
  - Verify: `cargo test -p hardshelldb --test slt`;
    `cargo xtask crash --iterations 20000`

- [ ] **M10.4 — `INSERT ... ON CONFLICT`**
  - Depends: M7.9
  - Do: `ON CONFLICT [(cols) | ON CONSTRAINT name] DO NOTHING | DO UPDATE
    SET ... [WHERE ...]` with `EXCLUDED`; `21000` when a row would be
    updated twice in one statement.
  - Accept: SLT coverage, checked with `diff-pg`.
  - Verify: `cargo test -p hardshelldb --test slt`

- [ ] **M10.5 — savepoints**
  - Depends: M6.5
  - Do: `SAVEPOINT`, `RELEASE`, `ROLLBACK TO` per
    `docs/subsystems/wal-and-transactions.md`; `ROLLBACK TO` leaves the
    failed state (`25P02`).
  - Accept: model-based test of nested savepoints with random DML; crash
    campaign covers them.
  - Verify: `cargo test -p hardshell-storage -p hardshelldb`;
    `cargo xtask crash --iterations 20000`

- [ ] **M10.6 — `information_schema` subset**
  - Depends: M7.5
  - Do: `information_schema.tables`, `columns`, `table_constraints`,
    `key_column_usage`, `views`; `version()`, `current_schema()`.
  - Accept: SLT coverage; the CLI's `\d` commands can switch to them.
  - Verify: `cargo test -p hardshelldb --test slt`

- [ ] **M10.7 — functions set 3 and set-returning functions**
  - Depends: M7.10
  - Do: the functions the SQL reference marks for M10.7, including
    `generate_series` in `FROM`.
  - Accept: SLT coverage, checked with `diff-pg`.
  - Verify: `cargo test -p hardshelldb --test slt`

**Exit criteria (M10):** every statement and function in the SQL reference
is `Built` or explicitly moved to the backlog.

---

## M11 — Embedded 1.0: hardening and release

**Goal:** a 1.0 library and CLI with a frozen file format, a stable API and
a release pipeline.

- [ ] **M11.1 — file format v1 freeze**
  - Depends: M10.3
  - Do: write `docs/file-format.md` (every structure, byte by byte); set
    format version 1; commit fixture databases in `tests/fixtures/format-v1/`
    with their expected contents; opening a version 0 file gives a clear
    error telling the user to dump and restore with a 0.x CLI.
  - Accept: fixture tests pass on all three OSes; the doc matches the
    decoders (reviewed by a human).
  - Verify: `cargo test -p hardshelldb --test fixtures`

- [ ] **M11.2 — `CHECK DATABASE`**
  - Depends: M11.1, M8.6
  - Do: the SQL statement and `hardshell check`: checksums, tree checker,
    page accounting, every index entry matches its row and vice versa,
    constraints hold, the catalog is consistent; results as rows.
  - Accept: each class of corruption in generated fixtures is reported.
  - Verify: `cargo test -p hardshelldb -p hardshell-cli`

- [ ] **M11.3 — backup and `VACUUM`**
  - Depends: M11.1
  - Do: `VACUUM` (return free pages at the end of the file to the OS);
    `VACUUM INTO 'path'` and `Database::backup_to(path)` (a consistent copy
    from a read snapshot while writers continue).
  - Accept: a backup taken during a write workload opens and passes
    `CHECK DATABASE` and equals the database at its snapshot.
  - Verify: `cargo test -p hardshelldb`

- [ ] **M11.4 — long fuzz and crash campaigns**
  - Depends: M11.1
  - Do: a nightly workflow: `fuzz-smoke` 30 minutes per target (one job per
    target), crash campaigns of 1,000,000 iterations, property tests with
    random seeds; artifacts uploaded on failure.
  - Accept: a week of clean nightly runs, recorded in a note.
  - Verify: nightly CI logs

- [ ] **M11.5 — API review and stabilization**
  - Depends: M10.7
  - Do: review every public item of `hardshelldb` (naming, errors,
    `#[non_exhaustive]` where growth is expected); MSRV policy; examples;
    `cargo doc --workspace --no-deps` with no warnings.
  - Accept: the user signs off the API in an ADR.
  - Verify: `cargo doc --workspace --no-deps`; the ADR

- [ ] **M11.6 — release pipeline**
  - Depends: M11.5
  - Do: `.github/workflows/release.yml` on tag `v*`, per
    `docs/05-release.md`: version check, CLI archives for six targets with
    SHA-256 checksums, crates to crates.io from a protected environment.
    Document secret **names** only; a human creates the secrets.
  - Accept: the workflow passes YAML review and `cargo xtask workflows`;
    no secret values anywhere in the repository.
  - Verify: YAML review; `git diff` reviewed for secret material
  - Note (from M0.5): `release-no-testing` checks what `cargo build
    --release -p <binary>` turns on. Build each binary with `-p`: a
    `--workspace` build unifies features across members, so a member that
    turned `testing` on through a normal dependency would reach the
    binaries.

- [ ] **M11.7 — security pass**
  - Depends: M11.2
  - Do: the threat model in `SECURITY.md` (hostile files, hostile SQL,
    resource exhaustion); review every limit and its default;
    `statement_timeout`; decide dependency-policy item 30 (coverage-guided
    fuzzing) with the user.
  - Accept: the threat model is reviewed; each limit has a test.
  - Verify: `cargo test --workspace`; human review

- [ ] **M11.8 — 1.0 release-candidate dry run**
  - Depends: M11.3, M11.4, M11.6, M11.7
  - Do: work through the release checklist in `docs/05-release.md`.
  - Accept: every item checked, with evidence linked.
  - Verify: the completed checklist (human sign-off)

**Exit criteria (M11):** a tag produces the 1.0 crates and CLI archives,
and the release checklist is complete.

---

## M12 — Server

**Goal:** `hardshelld` serves the engine over the PostgreSQL wire protocol
v3.0, and `psql` works against it (ADR-0016).

- [ ] **M12.1 — decision: TLS and password authentication**
  - Depends: M11.8
  - Do: run `/evaluate-dependency` for a TLS stack and for SCRAM-SHA-256's
    primitives (SHA-256, HMAC, PBKDF2), including the build-it-ourselves
    estimate; the user decides; record an ADR.
  - Accept: the ADR is `Accepted`.
  - Verify: the ADR

- [ ] **M12.2 — wire: message codec**
  - Depends: M0.6
  - Do: encode and decode the frontend and backend messages listed in
    `docs/subsystems/server-and-wire.md`; message length limit (default
    64 MiB); register fuzz target `wire`.
  - Accept: round-trip tests per message; fuzz clean 60 s.
  - Verify: `cargo test -p hardshell-wire`;
    `cargo xtask fuzz-smoke --target wire --secs 60`

- [ ] **M12.3 — server: listener, pool and startup**
  - Depends: M12.2, M11.5
  - Do: `hardshelld --data FILE` listening on loopback TCP and (Linux,
    macOS) a Unix-domain socket only; a bounded thread pool;
    `max_connections` (`53300`); startup and parameter status; until
    M12.7, `trust` authentication only, and only with
    `--insecure-local-trust`.
  - Accept: refuses non-loopback addresses; refuses to start without the
    flag; connection limit enforced (integration tests with an in-repo test
    client).
  - Verify: `cargo test -p hardshell-server`

- [ ] **M12.4 — server: simple query protocol**
  - Depends: M12.3
  - Do: `Query` → `RowDescription` (PostgreSQL type OIDs) / `DataRow` (text
    format) / `CommandComplete` / `ErrorResponse` / `ReadyForQuery` with
    transaction status.
  - Accept: the SLT corpus run through the server gives the same results as
    embedded; `psql -h 127.0.0.1` can create, insert and select (manual,
    noted with the psql version).
  - Verify: `cargo test -p hardshell-server`; manual `psql` session

- [ ] **M12.5 — server: extended query protocol**
  - Depends: M12.4
  - Do: `Parse`, `Bind`, `Describe`, `Execute`, `Sync`, `Close`, `Flush`;
    named and unnamed statements and portals; binary format for integers,
    floats, `bool` and `bytea`.
  - Accept: the SLT corpus with parameters through the extended protocol
    matches embedded results.
  - Verify: `cargo test -p hardshell-server`
  - Note (from M0.3): `server-and-wire.md` keys a session's statements and
    portals by client-chosen names in `FastHashMap`s, which do not resist
    chosen keys; `docs/02-dependency-policy.md` (item 12) wants
    `RandomState` where input may be adversarial. Each map belongs to one
    connection, so a client could only slow its own session. Decide here.

- [ ] **M12.6 — server: cancellation, timeouts and shutdown**
  - Depends: M12.5
  - Do: `CancelRequest` with unguessable per-connection secret keys (from
    the entropy source decided in M10.7); `statement_timeout`
    and idle timeouts; shutdown on a `hardshelld stop` command over the
    local socket, or decide signal handling in an ADR under ADR-0013
    (`std` has no signal API). A killed server is crash-safe either way.
  - Accept: cancel interrupts a long query with `57014`; stop drains
    connections and closes the database cleanly.
  - Verify: `cargo test -p hardshell-server`

- [ ] **M12.7 — server: TLS and authentication per M12.1**
  - Depends: M12.1, M12.6
  - Do: what the M12.1 ADR decided; then allow non-loopback listening only
    with TLS and password authentication on.
  - Accept: per the ADR.
  - Verify: per the ADR

- [ ] **M12.8 — server: packaging and docs**
  - Depends: M12.7
  - Do: add `hardshelld` to the release workflow and archives; operator
    docs (configuration, files, logs, backup through `VACUUM INTO`).
  - Accept: release dry run includes the server.
  - Verify: YAML review; dry run

**Exit criteria (M12):** `psql` and the extended-protocol test client run
the SLT corpus against `hardshelld` with results identical to embedded.

---

## Backlog (M13+, each needs a trigger or a decision)

| Item | Trigger / reason |
| :- | :- |
| Multi-writer MVCC (row versions, snapshot isolation or SSI) | ADR-0008's trigger: single-writer throughput limits a workload after group commit |
| Multi-process access without the server | ADR-0005's trigger |
| Primary-key-clustered tables (`WITHOUT ROWID`-style) | ADR-0006's trigger: the double lookup costs more than the point-lookup budget |
| `JSON`/`JSONB` types and operators | User demand; needs a JSON codec and an ADR on storage |
| Regular expressions (`~`, `SIMILAR TO`, `regexp_*`) | User demand; an own regex engine with linear-time matching |
| Locale collations | ADR-0014's trigger |
| Schemas beyond `public` (`CREATE SCHEMA`, `search_path`) | Applications that need namespaces |
| Time zone database for `TIMESTAMPTZ` beyond fixed offsets | User demand; needs a decision on shipping tzdata |
| Line editing and history in the CLI; Ctrl-C cancelling the running query instead of exiting | Needs terminal raw mode and signal handling: one ADR under ADR-0013 can cover terminal, signals and OS randomness together |
| C API and bindings for other languages | A non-Rust user; needs `unsafe` FFI and an ADR |
| WASM build | ADR-0001's trigger |
| Encryption at rest | User demand; criterion 2 of the dependency policy |
| Page compression | Database size becomes a user problem |
| Hardware CRC-32C | ADR-0009's trigger (checksum time > 5% of a read benchmark); needs `unsafe` intrinsics |
| Logical WAL records, point-in-time recovery, replication | ADR-0007's trigger |
| Vectorized or compiled execution | ADR-0012's trigger |
| Parallel query | A profiled analytic workload on idle cores |
| Async API | ADR-0010's trigger |
| Triggers, stored procedures | User demand; large semantic surface |
| `MERGE` | User demand after `ON CONFLICT` |
| `COPY FROM STDIN`/`TO STDOUT` over the wire | A driver or tool that needs it (M12 follow-up) |
| Import of the public-domain sqllogictest corpus | After M9.6, to widen coverage; needs a license note and skip lists |
