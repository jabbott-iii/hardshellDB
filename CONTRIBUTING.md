# Contributing to hardshellDB

hardshellDB is built one roadmap task at a time. It holds other people's
data, so the bar for changes to storage, durability and SQL semantics is
high: every change comes with the tests that prove it, and nothing is
claimed that was not run.

The project is at its first milestone: the workspace holds only empty
crates so far, and the commands below arrive with the roadmap tasks named
beside them.

## Contribution workflow

1. Find the work in [`docs/roadmap.md`](docs/roadmap.md). The next task is
   the first unchecked one whose `Depends` are all checked. For anything
   not on the roadmap, open an issue first: new SQL surface, new
   dependencies and format changes need a decision before code.
2. Read the docs that [`AGENTS.md`](AGENTS.md) → "Read before working"
   lists for the crates you will touch, and any ADR the task relies on in
   [`docs/decisions.md`](docs/decisions.md). A `Proposed` ADR must be
   accepted before code depends on it.
3. Make the smallest complete change that meets the task's acceptance
   criteria.
4. Run the task's `Verify` commands and `cargo xtask ci`.
5. Open a pull request that says which task it completes, what you ran and
   what you could not run (for example a check that needs another OS).

## Ground rules

These are summarized from `AGENTS.md`, which is authoritative.

- **No third-party crates.** The approved set is empty
  ([`docs/02-dependency-policy.md`](docs/02-dependency-policy.md)). If you
  think one is needed, open an issue with the evaluation the policy
  describes.
- **One-way layering.** A crate depends only on the crates its row in the
  dependency matrix allows ([`docs/01-architecture.md`](docs/01-architecture.md)).
- **No `unsafe`.** Only `hardshell-os` may ever contain it, and only after
  an accepted ADR.
- **Durability first.** The main file changes only by checkpoint; commits
  are acknowledged only after their sync; a failed sync is never retried.
  Storage changes run the crash campaign.
- **Hostile input never panics.** SQL text, database files, CSV and wire
  messages are parsed with bounds and limits, and fuzzed.
- **PostgreSQL semantics** for supported SQL, with SQLSTATE codes;
  everything else is rejected with `0A000`
  ([`docs/sql-reference.md`](docs/sql-reference.md)).
- **Honest verification.** Say what you ran, with results, and what you did
  not run.

## Development setup

You need [rustup](https://rustup.rs). From M0.1, `rust-toolchain.toml` pins
the toolchain and rustup installs it the first time you run `cargo` in the
repository. No system packages are needed: the engine uses only Rust's
standard library.

| System | You need |
| :- | :- |
| Linux | A C linker (`cc`), as for any Rust project |
| macOS | The Xcode command-line tools |
| Windows | The MSVC build tools and the Windows SDK |
| Optional, from M9.6 | `psql` and a PostgreSQL server, for `cargo xtask diff-pg` |

## Commands

| Command | What it does | From |
| :- | :- | :- |
| `cargo xtask ci` | The full PR gate: fmt, clippy, tests, and the project checks | M0.5 |
| `cargo xtask fuzz-smoke --secs 60 [--target T]` | Mutation fuzzing of the parsers and decoders | M0.6 |
| `cargo xtask crash --iterations N` | Crash campaigns under the fault-injecting file system | M2.6 |
| `cargo test -p hardshelldb --test slt` | SQL logic tests in `tests/slt/` | M6.6 |
| `cargo xtask bench [filter]` | Benchmarks against the stored baseline | M0.7 |

[`docs/04-quality.md`](docs/04-quality.md) describes each harness and the
CI jobs.

## Coding conventions

- Rust edition 2024, the pinned stable toolchain.
- Each crate has its own error enum; every error maps to a SQLSTATE code.
- No `unwrap()` in library code; `expect("why")` only for true invariants.
- No printing from library crates; use the `hardshell_core::log` macros.
- Every `pub` item has a doc comment.
- Every source file starts with the Apache-2.0 header from `LICENSE`'s
  appendix (ADR-0015).

## Tests

- Unit tests beside the code; integration tests in each crate's `tests/`.
- SQL behavior in `tests/slt/<area>/<feature>.slt`, matched by SQLSTATE
  codes, never message text.
- A new parser or decoder registers a fuzz target with seeds in
  `fuzz/corpus/<target>/`.
- A storage change answers the "Storage change review checklist" in
  `docs/04-quality.md`.

## Documentation

Update the subsystem spec when behavior or design changes,
`docs/sql-reference.md` when SQL changes, and `docs/decisions.md` (a new
ADR) when a decision changes. Tick a roadmap task only when its acceptance
criteria are met and its verification commands passed.

## Commits and pull requests

- One roadmap task (or one issue) per pull request.
- Commit messages: a short imperative summary line naming the task
  (`M1.5: buffer pool with clock-sweep eviction`), a blank line, then what
  and why.
- Dependency upgrades, toolchain bumps and format changes are their own
  pull requests.

## Coding agents

Agents follow [`AGENTS.md`](AGENTS.md). Claude Code also loads
[`CLAUDE.md`](CLAUDE.md), the path-scoped rules in `.claude/rules/`, and the
project skills in `.claude/skills/` (`/next-task`, `/add-sql-feature`,
`/change-on-disk-format`, `/evaluate-dependency`, `/record-decision`).
Agents never commit, push or tag; a human reviews and commits their work.

## Security issues

Do not open a public issue for a vulnerability. Follow
[`SECURITY.md`](SECURITY.md).

## License

By contributing, you agree that your contributions are licensed under the
Apache License 2.0, as stated in section 5 of [`LICENSE`](LICENSE).
