# Subsystem: `hardshell-cli` (the `hardshell` shell)

An interactive shell and script runner over the embedded library, in the
style of `psql`. It depends only on `core` and the `hardshelldb` facade's
public API (dependency matrix), so anything it can do, a library user can
do too. Nothing is built yet; M8 builds it, except `hardshell check`, which
needs M11's `CHECK DATABASE`.

## Contents

| Module | Provides |
| :- | :- |
| `main` | Argument parsing (`hardshell_core::cli`), exit codes; the only place `print!` is allowed, with a comment at the crate root |
| `repl` | The read-eval-print loop: prompts, continuation lines, statement splitting via `hardshelldb::split_statements` / `is_complete` |
| `meta` | Backslash commands |
| `format` | Output formats: aligned (psql-like), expanded (`\x`), CSV, JSON lines |
| `copy` | `\copy` through `Connection::copy_in` / `copy_out` |
| `dump` | `hardshell dump`: schema and data as a SQL script |

## Invocation

```
hardshell [OPTIONS] [FILE]          # FILE omitted: an in-memory database
hardshell check FILE                # storage checks (M8), full CHECK DATABASE (M11); exit 0 if clean
hardshell dump FILE [-o OUT.sql]    # schema + data as SQL that recreates the database

Options:
  -c, --command SQL       run SQL (and meta-commands) and exit
  -f, --file SCRIPT       run a script and exit
      --read-only         open with OpenOptions::read_only
      --csv | --json      output format for -c / -f
      --continue-on-error keep going after a failing statement in -c / -f
  -q, --quiet             no banner, no row-count footers
  -h, --help | -V, --version
```

Exit codes: `0` success; `1` a statement failed (non-interactive);
`2` usage error; `3` the database could not be opened (locked, corrupt, not
found with `--read-only`). In `-c`/`-f` mode the first error stops the run
unless `--continue-on-error`.

## Meta-commands

| Command | Does |
| :- | :- |
| `\q` | Quit |
| `\?` | List meta-commands |
| `\dt [pattern]` | List tables |
| `\d [name]` | Describe a table, view, index or sequence (columns, types, constraints, indexes) |
| `\di [pattern]`, `\dv [pattern]` | List indexes, views |
| `\timing [on\|off]` | Show each statement's elapsed time |
| `\x [on\|off]` | Expanded (one column per line) output |
| `\i FILE` | Run statements from a file |
| `\o [FILE]` | Send query output to a file, or back to standard output |
| `\copy TABLE [(cols)] FROM\|TO 'file' [WITH (FORMAT csv, HEADER, DELIMITER ',', NULL '')]` | Client-side CSV import and export |
| `\pset null STRING` | How `NULL` is displayed (default: empty) |

Patterns use `*` and `?` wildcards, as in psql. `\d` commands read the
catalog through SQL (`information_schema` from M10; before that through
a facade introspection call, see open questions).

## Rules

- **Statement boundaries** come from the SQL lexer (`is_complete`), never
  from a naive `;` search: a `;` inside a string, a quoted identifier or a
  comment does not end a statement. The prompt changes to show a pending
  quote or statement (`hardshell=>` versus `hardshell->`).
- **Errors** print as `ERROR:  <message>` with `DETAIL`, `HINT`, the SQLSTATE
  (always, unlike psql's default), and a caret under the error position.
- **Output** in aligned mode matches psql's layout closely enough to diff
  by eye; CSV output is RFC 4180 through `hardshell-core::csv`; JSON output
  writes one object per row with typed values (numbers stay numbers;
  `NUMERIC` is a string to keep precision).
- **No line editing at first:** history and arrow keys need terminal raw
  mode, which needs OS calls `std` does not have, so FFI in `hardshell-os`
  and an ADR under ADR-0013. Until then input is plain buffered lines;
  `rlwrap hardshell` gives history.
- **`\copy` is client-side:** files are read and written by the shell
  process, streamed through the facade in batches, so it works the same in
  M12 against a server.
- **`dump`** writes `CREATE TABLE`, `CREATE INDEX`, `CREATE VIEW` (in
  dependency order), sequences with their current values, and data as
  multi-row `INSERT`s in primary-key order, inside one transaction; loading
  the dump into an empty database gives a database that `dump` writes
  byte-identically again.

## Invariants

- The CLI uses only the facade's public API.
- Nothing it prints depends on hash iteration order: listings are sorted by
  name.
- Once Ctrl-C can be caught (open questions), it cancels the running
  statement through `InterruptHandle` and returns to the prompt, and a
  second Ctrl-C at the prompt exits.

## Limits

| Limit | Default | When exceeded |
| :- | :- | :- |
| Input line | 16 MiB (the parser's statement limit) | `54001` from the parser |
| `\i` nesting depth | 16 | Error, run stops |

## Tests

- Integration tests run the built binary (`CARGO_BIN_EXE_hardshell`) with
  `-c` and `-f`, checking standard output, standard error and exit codes.
- Golden output files for each format (aligned, expanded, CSV, JSON) over a
  fixed dataset.
- Round-trip tests: `dump` then load then `dump` is byte-identical;
  `\copy TO` then `\copy FROM` into an empty table reproduces it.
- The CLI's own fuzz coverage comes from the `sql` and `csv` targets.

## Open questions

- Ctrl-C handling needs a signal handler (Unix) or console control handler
  (Windows): FFI, so an ADR under ADR-0013, the same one as raw mode for
  line editing. Without it, Ctrl-C ends the process; the WAL keeps the
  database safe.
- How `\d` reads the catalog before M10's `information_schema`: a
  documented introspection API on `Connection` (`tables()`,
  `describe(name)`), which the server would also want.
- Binary size budget (`docs/00-overview.md`): measure at M8.
