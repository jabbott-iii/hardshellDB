# 02 — Dependency policy

## The rule

> **Build it ourselves unless building it is unreasonable.** A third-party
> crate is used only when it passes the exception test below, the user has
> approved it, and it is recorded in the register in this document
> (ADR-0002).

The current approved set of third-party crates is **empty**. `Cargo.lock` must
contain only workspace packages. `cargo xtask deps` fails otherwise.

## What counts as a dependency

| Counts (needs approval) | Does not count |
| :- | :- |
| Any crate from crates.io or git in any `[dependencies]`, `[dev-dependencies]` or `[build-dependencies]` table | The Rust toolchain and `std` (including the system libraries `std` itself links) |
| Vendored or copied third-party source code, including "just one file" | Public specifications we implement ourselves (SQL standard texts, PostgreSQL's documentation, RFC 4180, the PostgreSQL protocol, Unicode data file formats) |
| A build script that downloads anything or runs third-party code | GitHub Actions used by CI, pinned by commit SHA (`docs/04-quality.md`) |
| A git submodule containing code | External programs used only by dev tooling and never linked: `psql` and a PostgreSQL server for `cargo xtask diff-pg` (M9.6) |
| A `cargo install`-ed tool that CI depends on | Third-party **data** files committed with their license beside them (see "Third-party data") |

Reading PostgreSQL's documentation or a specification and implementing the
behavior ourselves is **not** a dependency. That is the intended path.
Copying PostgreSQL source code is vendoring, and needs approval (and its
license kept).

### OS libraries

hardshellDB links no OS library of its own. Everything goes through `std`
(ADR-0013). If an ADR ever allows FFI in `hardshell-os`, the libraries it
calls are listed here, per OS, with how they are linked or loaded.

| OS | Libraries | How |
| :- | :- | :- |
| Linux | (none beyond `std`) | — |
| macOS | (none beyond `std`) | — |
| Windows | (none beyond `std`) | — |

### Third-party data

| File | Used for | License | Where |
| :- | :- | :- | :- |
| `UnicodeData.txt` (planned, M7.10) | Generating the simple case-mapping table for `lower`/`upper` (ADR-0014) | Unicode License v3, kept beside the file | `crates/types/data/` |

## The exception test

Before proposing any crate, first try to **change the design so the need
disappears**:

- choose a simpler format or algorithm (binary collation instead of UCA)
- cut scope (fixed UTC offsets instead of the time zone database)
- move the work into dev tooling (a differential oracle run through
  `psql`, not a PostgreSQL client library)

If the need remains, a crate may be proposed only when at least one of these
is true:

1. **Disproportionate cost.** Building it is estimated above 4 engineer-weeks,
   the capability is not central to the database, and no design change
   avoids it.
2. **Specialist correctness with severe failure modes.** The domain demands
   expert knowledge, bugs are hard to detect by testing, and the consequences
   are severe. Examples: TLS, password hashing, encryption at rest. Nothing
   before M12 needs these.
3. **Dev-only tooling with no reasonable equivalent.** It never ships in the
   library or the binaries, and our own version would be far weaker.
   Example: coverage-guided fuzzing.

"It's popular" and "it would be faster to start" are not reasons. The
storage engine, the SQL parser, the planner and the executor are central:
criterion 1 never applies to them.

## Exception process

1. Run `/evaluate-dependency` (or follow
   `.claude/skills/evaluate-dependency/` manually) and write the evaluation
   to `docs/dependency-evaluations/<crate>.md`. It must cover:
   - the need, and the design changes considered
   - an estimate for building it ourselves
   - the candidate crate's license, maintainers and release cadence
   - its transitive dependency count and `unsafe` usage
   - its binary-size impact, and whether it ships in the library or
     binaries
2. The **user** approves or rejects it. Agents never approve dependencies.
3. If approved:
   - Add a row to the register below with status `Approved` and an ADR
     reference.
   - Add the exact pin to `xtask/deps-allowlist.txt` (`name version ADR-NNNN`).
   - Use an exact version requirement (`= x.y.z`).
   - Wrap the crate behind a single module or crate boundary of ours, so it
     can be replaced without touching callers.
4. Review approved dependencies on every upgrade. Upgrades are their own
   change, never bundled with feature work.

## Build-vs-buy register

**Status** is one of:

- **Build:** we write it.
- **Decide at <task>:** the need is known; the decision is scheduled.
- **Proposed:** awaiting the user's decision.
- **Approved:** the user said yes, ADR recorded.
- **Rejected.**

The **fallback** column is a crate we would evaluate first if the **switch
trigger** fires. It is not approved, and nothing about it has been verified.

| # | Capability | Status | Our approach | Fallback (not approved) | Switch trigger |
| :- | :- | :- | :- | :- | :- |
| 1 | SQL lexer and parser | Build | Hand-written recursive descent + Pratt (ADR-0011) | `sqlparser` | Grammar past about 400 productions with recurring precedence bugs (then first consider our own generator) |
| 2 | Storage engine: pager, WAL, B+tree | Build | `hardshell-storage` (ADR-0006, ADR-0007) | — | Never: central |
| 3 | File I/O, locking, sync | Build | `std` through `hardshell-os` (ADR-0013) | `rustix` | `std` lacks a needed call and the FFI surface would pass about 50 declarations |
| 4 | Checksums | Build | Software CRC-32C in `hardshell-core` (ADR-0009) | `crc32c` | Checksum time over 5% of a read benchmark and no ADR allows `unsafe` intrinsics |
| 5 | Exact decimal arithmetic | Build | `NUMERIC` in `hardshell-types` | `bigdecimal` | More than 3 `NUMERIC` correctness bugs found by `diff-pg` in one milestone after M9.6 |
| 6 | Dates, times, intervals | Build | `hardshell-types` | `jiff` | Not expected |
| 7 | Time zone database | Decide at backlog | Fixed offsets only until then | `jiff`'s tzdb support | Users need IANA zones |
| 8 | Unicode case mapping | Build | Table generated from committed `UnicodeData.txt` | — | Not expected |
| 9 | Locale collation (UCA) | Build (backlog) | Binary collation only (ADR-0014) | `icu_collator` | ADR-0014's trigger |
| 10 | CSV | Build | `hardshell-core::csv` | `csv` | Not expected |
| 11 | JSON (tooling and CLI output) | Build | A small reader in `xtask`, a writer in the CLI | `serde_json` | Not expected |
| 12 | Hashing for maps and hash joins | Build | FNV-1a and a deterministic fast hasher; `std`'s `RandomState` where input may be adversarial (server) | `ahash` | Not expected |
| 13 | Pseudo-random numbers | Build | `Pcg32` | `rand` | Not expected |
| 14 | OS entropy (for `gen_random_uuid`, `random()` seeds) | Decide at M10.7 | Options: read `/dev/urandom` through `std` on Unix; `std`'s randomly keyed `RandomState` as a seed source; FFI on Windows (needs an ADR) | `getrandom` | M10.7 |
| 15 | Threads, channels, locks | Build | `std::thread`, `std::sync` | `parking_lot`, `crossbeam` | Measured lock overhead above 10% of a benchmark after sharding |
| 16 | Async runtime | None (ADR-0010) | Synchronous engine; thread pool in the server | `tokio` | ADR-0010's trigger |
| 17 | Error plumbing | Build | Per-crate error enums with SQLSTATE | `thiserror`, `anyhow` | Not expected |
| 18 | Logging | Build | `hardshell-core::log` with a host sink | `log`, `tracing` | Not expected (hosts bridge through the sink) |
| 19 | CLI argument parsing | Build | `hardshell-core::cli` | `clap` | Not expected |
| 20 | CLI line editing and history | Build (backlog) | Plain line input until an ADR allows terminal raw mode | `rustyline` | Users ask for editing before the backlog item lands |
| 21 | Property testing | Build | `hardshell-core::testing` | `proptest` | Not expected |
| 22 | SQL logic test runner | Build | `crates/hardshelldb/tests/slt.rs` | `sqllogictest` | Not expected |
| 23 | Benchmarking | Build | `hardshell-bench` | `criterion` | Not expected |
| 24 | Fuzzing (stable, CI) | Build | `cargo xtask fuzz-smoke` | — | — |
| 25 | PostgreSQL wire protocol | Build | `hardshell-wire` (ADR-0016) | `pgwire` | Not expected |
| 26 | TLS for the server | Decide at M12.1 | Server stays loopback-only until decided (ADR-0016) | `rustls` | M12.1 (criterion 2) |
| 27 | SCRAM-SHA-256 primitives (SHA-256, HMAC, PBKDF2) | Decide at M12.1 | — | `sha2`, `hmac`, `pbkdf2` | M12.1 (criterion 2) |
| 28 | `md5()` SQL function | Build | Own MD5 (not used for security) | `md-5` | Not expected |
| 29 | Hex and base64 | Build | `hardshell-types` | `base64` | Not expected |
| 30 | Coverage-guided fuzzing (optional, nightly) | **Proposed** | `cargo fuzz` with `libfuzzer-sys` in an excluded `fuzz/` workspace, dev-only, never linked into the library or binaries | — | User decision in M11.7 (criterion 3) |
| 31 | Page compression | Build (backlog) | — | `lz4_flex` | Backlog trigger |
| 32 | Encryption at rest | Decide at backlog | — | `chacha20poly1305`, `aes-gcm` | User demand (criterion 2) |

## Enforcement

- **`cargo xtask deps`** reads `cargo metadata --format-version 1` (parsed
  with `xtask`'s own JSON reader). It fails if any package has a non-null
  `source` and does not appear in `xtask/deps-allowlist.txt`, or if a path
  package lies outside the workspace (vendored code).
- **`cargo xtask layers`** checks the workspace dependency graph against the
  matrix in `docs/01-architecture.md`.
- **Path-scoped rule** `.claude/rules/dependencies.md` loads whenever an agent
  opens a Cargo manifest.
- **Claude Code permissions** (`.claude/settings.json`) ask before
  `cargo add`, `cargo remove`, `cargo update` and `cargo install`.
