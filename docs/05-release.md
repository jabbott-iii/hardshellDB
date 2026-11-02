# 05 — Build, release, operate

Nothing here exists yet. This is the plan the roadmap builds toward
(M0.1 for profiles, M11.1 for the format promise, M11.6 for the release
workflow).

## Cargo profiles

```toml
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
debug = "line-tables-only"     # symbolicated backtraces in bug reports
panic = "unwind"               # the fuzz harness relies on catch_unwind;
                               # the engine never relies on unwinding (see "Panics")

[profile.xtask]                # what `cargo xtask` builds with
inherits = "dev"
opt-level = 3                  # faster fuzzing and crash campaigns; overflow checks
                               # and debug assertions stay on
```

These profiles apply to our binaries (`hardshell`, `hardshelld`, `xtask`,
benches). An application embedding `hardshelldb` uses its own profiles.

## Versioning

There are two version numbers, and they move independently.

### Crate versions

- Every workspace crate shares the workspace version (`[workspace.package]
  version`). A release raises it for all of them.
- `hardshelldb` follows SemVer. Before 1.0, a minor bump (0.x → 0.x+1) may
  break the API; the release notes say how.
- The other library crates are published because the facade depends on
  them, and their docs say: internal, no stability promise, use
  `hardshelldb`.
- The minimum supported Rust version is the pinned toolchain at release
  time, declared as `rust-version` in every manifest. Raising it is a minor
  release.

### File format version

- The header (page 0) holds a format version (`u16` major, `u16` minor) and
  three sets of feature flags (`docs/subsystems/storage.md`), used from
  1.0 on:
  - **compatible** flags: an older 1.x may ignore them;
  - **read-only-compatible** flags: an older 1.x can read the file but must
    not write it;
  - **incompatible** flags: an older 1.x refuses to open it, naming the
    flag.
- **Before 1.0 (major 0):** any 0.x release may change the format, and each
  change bumps the minor. Opening a file of another version fails with a
  clear error naming the version and the 0.x CLI that can `hardshell dump`
  it. No migrations.
- **From 1.0 (major 1, M11.1):** every 1.x release opens and writes every
  major-1 file. New on-disk features use feature flags and are off for
  existing files until the user enables them. Fixture files from every
  1.x release stay in `tests/fixtures/` and are tested forever.
- The WAL format is part of the file format version.

## What a release publishes (M11.6)

| Where | What |
| :- | :- |
| crates.io | `hardshelldb` and the library crates it depends on; `hardshell-cli` (`cargo install hardshell-cli` installs `hardshell`) |
| GitHub Releases | `hardshell` archives for six targets (`.tar.gz` for Linux and macOS, `.zip` for Windows), a `SHA256SUMS` file, and from M12.8 the same for `hardshelld` |

Crate names on crates.io are not checked yet: verify that `hardshelldb` and
the `hardshell-*` names are free before the first publish, and record an ADR
if any must change.

## The release workflow (`.github/workflows/release.yml`, M11.6)

On a tag `v<version>`:

1. **Gate:** the tag equals the workspace version; the commit is on
   `main`; CI passed on it.
2. **Build and test,** one job per target, on GitHub-hosted runners
   (Linux x86_64 and aarch64, macOS aarch64 and x86_64, Windows x86_64 and
   aarch64): `cargo --locked xtask ci`, then a release build of the CLI,
   packed into its archive. Runner labels are chosen and recorded in M11.6.
3. **Publish crates** from a protected environment (`crates-io`, requiring
   a maintainer's approval): crates in dependency order. Prefer crates.io's
   trusted publishing (OIDC) if it is available for the repository
   (verify in M11.6); otherwise the secret `CARGO_REGISTRY_TOKEN`.
4. **GitHub release:** archives and `SHA256SUMS`. Only this job has
   `contents: write`.

Rules:

- Secrets exist only in GitHub's secret storage. The repository documents
  secret **names**, never values. Agents never create, read or print them.
- No caches in the release workflow: a release builds from its commit
  alone.
- Actions are pinned by commit SHA (`cargo xtask workflows`).
- Agents never tag or publish (`AGENTS.md`, rule 7).

## Operating the engine

### Files

| File | When | Safe to delete? |
| :- | :- | :- |
| `<name>` (e.g. `app.hsdb`) | Always | No: it is the database |
| `<name>-wal` | While the database is open, and after a crash | **No**, unless it is empty: after a crash it holds committed transactions not yet in the main file |
| Temporary spill files | During large sorts and hash joins (M9) | Yes, when no process has the database open |

Copying a database: close it first and copy the one file, or use
`VACUUM INTO` / `Database::backup_to` while it is open (M11.3). Copying the
main file of an open database without its WAL loses committed data.

### Panics

- The engine must not panic on any input: SQL text, database files, CSV,
  wire messages, or API misuse. A panic is a bug.
- If one happens anyway (a bug), it may poison a `std::sync::Mutex` the
  engine holds. The engine treats any poisoned internal lock as fatal for
  that `Database`: every later call fails with `XX000` (`internal_error`)
  until the database is reopened, which runs recovery. Committed data is
  safe, because commits are durable only through the WAL (ADR-0007).

### Logging

- The library never prints. It logs through the sink the host installs
  (`hardshelldb::set_log_sink`). The CLI installs a sink that writes
  warnings and errors to stderr; `hardshelld` logs to stderr or a file
  (M12).
- Warnings worth surfacing: a checkpoint held back by a long reader, a WAL
  past twice its checkpoint threshold, a database on a filesystem we know
  is unsupported.

## Release checklist (manual items)

- [ ] Crash campaigns (storage and SQL) of 1,000,000 iterations each clean
      on every OS, seeds recorded.
- [ ] Process-kill test clean on every OS.
- [ ] A week of clean nightly fuzzing for every target.
- [ ] `cargo xtask diff-pg` clean, with the PostgreSQL version recorded.
- [ ] Fixture files from every earlier 1.x release open and pass
      `CHECK DATABASE` (from 1.1 on).
- [ ] Budgets in `docs/00-overview.md` measured on the reference machine,
      or their ADR exceptions current.
- [ ] `cargo doc --workspace --no-deps` with no warnings.
- [ ] `docs/sql-reference.md` statuses match the roadmap's ticks.
- [ ] `SECURITY.md`'s supported-versions table updated.
- [ ] Release notes list API changes, format changes, and new SQL.
- [ ] Downloaded archives match `SHA256SUMS`, and `hardshell --version`
      prints the release version on each OS.
