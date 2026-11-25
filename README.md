# hardshellDB

hardshellDB is an all-in-one SQL database engine and library for Linux,
macOS, and Windows, written in Rust. It doesn't run SQL yet ([Status](#status)
says what exists), so this overview describes the design. It will be one
engine with three ways in:

- **Embedded (M6):** an application will add the `hardshelldb` crate, open
  a file, and run SQL, with no server and no configuration.
- **Shell (M8):** `hardshell app.hsdb` will run SQL and psql-style
  backslash commands, import and export CSV, and dump databases as SQL.
- **Server (M12):** `hardshelld` will serve the same engine over the
  PostgreSQL wire protocol, so `psql` and PostgreSQL drivers can connect.

Its SQL will be PostgreSQL-flavored and strictly typed, with PostgreSQL's
SQLSTATE error codes. A database will be a single file of checksummed
pages, with B+tree tables and indexes and a write-ahead log. One writer and
many readers will work at once, and every transaction will be serializable.

It has no third-party crates: the parser, storage engine, planner,
executor, types, and test harnesses are each written here, from the ground
up, as the roadmap builds them.

## Status

**Groundwork.** No SQL runs yet, and nothing can open a database. What
exists is most of milestone M0, the guarded workspace (tasks M0.1–M0.6 in
the [roadmap](docs/roadmap.md)):

- The workspace: every crate is in place, and all but two are still empty.
- `hardshell-core`: SQLSTATE error codes, CRC-32C checksums,
  variable-length integers (varints), hashing, a seeded random number
  generator, a property-test harness (it runs a test on many generated
  inputs), and logging.
- `xtask`: the checks that fail on a third-party crate, a dependency
  between crates that the architecture forbids, `unsafe` code outside
  the places the rules allow, file access that bypasses the OS layer, a
  missing license header, or test-only code in a release build. It also
  holds a mutation fuzzer, whose one target so far is the varint decoder.

Next come the benchmark harness and the CI workflow (M0.7 and M0.8), then
the storage engine and its crash tests (M1–M3), all before any SQL. The
design, the decisions behind it, and the whole roadmap are in
[`docs/`](docs/).

| Milestone | What it delivers |
| :- | :- |
| M0 | Workspace, checks, fuzzing and benchmark harnesses, CI |
| M1–M3 | Pages, buffer pool, WAL, crash safety, B+trees |
| M4–M6 | Types, SQL parser, first queries end to end through the embedded API |
| M7 | Joins, aggregates, subqueries, views, constraints, indexes |
| M8 | The `hardshell` CLI |
| M9 | Optimizer, statistics, spilling, measured performance |
| M10 | Window functions, recursive CTEs, `ALTER TABLE`, `ON CONFLICT`, savepoints |
| M11 | 1.0: frozen file format, `CHECK DATABASE`, backups, release pipeline |
| M12 | The `hardshelld` server |

## What it will look like

```rust
use hardshelldb::{Database, params};

fn main() -> Result<(), hardshelldb::Error> {
    let db = Database::open("app.hsdb")?;
    let mut conn = db.connect()?;
    conn.execute("CREATE TABLE users (id BIGINT PRIMARY KEY, name TEXT NOT NULL)", params![])?;
    conn.execute("INSERT INTO users VALUES ($1, $2)", params![1_i64, "Ada"])?;
    let mut rows = conn.query("SELECT name FROM users WHERE id = $1", params![1_i64])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(0)?;
        let _ = name;
    }
    Ok(())
}
```

This is the planned API (`docs/01-architecture.md`); it does not compile
yet.

## Documentation

| Document | Covers |
| :- | :- |
| [`docs/00-overview.md`](docs/00-overview.md) | Goals, non-goals, performance budgets, risks |
| [`docs/01-architecture.md`](docs/01-architecture.md) | Crates, layers, the life of a statement, threading, configuration |
| [`docs/02-dependency-policy.md`](docs/02-dependency-policy.md) | The zero-dependency rule and its exception process |
| [`docs/03-platforms.md`](docs/03-platforms.md) | File I/O, sync and locking rules per OS |
| [`docs/04-quality.md`](docs/04-quality.md) | Tests, crash campaigns, fuzzing, CI |
| [`docs/05-release.md`](docs/05-release.md) | Versioning, the file format promise, releases |
| [`docs/sql-reference.md`](docs/sql-reference.md) | The SQL surface and differences from PostgreSQL |
| [`docs/roadmap.md`](docs/roadmap.md) | Every task, with acceptance criteria |
| [`docs/decisions.md`](docs/decisions.md) | Architecture decision records |
| [`docs/subsystems/`](docs/subsystems/) | One design spec per crate |

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md). Coding agents read
[`AGENTS.md`](AGENTS.md) (Claude Code also reads [`CLAUDE.md`](CLAUDE.md)).
Report security issues privately, as [`SECURITY.md`](SECURITY.md)
describes.

## License

Apache License 2.0. See [`LICENSE`](LICENSE).
