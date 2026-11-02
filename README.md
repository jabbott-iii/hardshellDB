# hardshellDB

hardshellDB is an all-in-one SQL database engine and library, written in
Rust for Linux, macOS and Windows. It is one engine with three ways in:

- **Embedded:** add the `hardshelldb` crate, open a file, run SQL. No
  server, no configuration.
- **Shell:** `hardshell app.hsdb` runs SQL and psql-style backslash
  commands, imports and exports CSV, and dumps databases as SQL.
- **Server:** `hardshelld` serves the same engine over the PostgreSQL wire
  protocol, so `psql` and PostgreSQL drivers can connect.

Its SQL is PostgreSQL-flavored and strictly typed, with PostgreSQL's
SQLSTATE error codes. A database is a single file of checksummed pages with
B+tree tables and indexes and a write-ahead log. One writer and many
readers work at once, and every transaction is serializable.

It is written from the ground up, with no third-party crates: the parser,
storage engine, planner, executor, types and test harnesses are all in this
repository.

## Status

**Planning.** Nothing is built yet. The design, the decisions behind it,
and a task-by-task roadmap are in [`docs/`](docs/). Work starts with
roadmap milestone M0 (the guarded workspace), then builds the storage
engine and its crash tests (M1–M3) before any SQL.

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
