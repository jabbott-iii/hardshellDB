---
name: add-sql-feature
description: Use when adding or changing any SQL surface in hardshellDB (a statement, clause, data type, operator, cast, function or aggregate). Carries it through the parser, binder, planner and executor with PostgreSQL semantics, SLT tests and the SQL reference.
argument-hint: "<feature, e.g. 'string_agg aggregate' or 'INSERT ... RETURNING'>"
---

# Add a SQL feature

Feature: `$ARGUMENTS`

PostgreSQL is the reference (ADR-0004) and `docs/sql-reference.md` is the
contract. A feature is done when it parses, binds, plans and executes with
PostgreSQL's semantics, rejects what it does not support, and is covered by
SLT files.

## 1. Pin down the semantics before writing code

- Find the feature in `docs/sql-reference.md` and the roadmap task that
  owns it. If it is not listed, stop and ask the user: new surface needs a
  roadmap entry (and an ADR if it changes the dialect, for example a new
  implicit cast).
- Write down, from PostgreSQL's documentation (cite the page and section):
  - the exact grammar we will accept, and which parts we will reject with
    `0A000`
  - the result type for each input type combination
  - NULL behavior: NULL inputs, empty input, all-NULL groups
  - errors and their SQLSTATE codes (overflow `22003`, division by zero
    `22012`, invalid text `22P02`, cardinality `21000`, …)
  - edge cases: empty strings, zero, negative values, extremes of each
    type, time zone and interval quirks
- Anything you cannot source: mark it "verify against psql", put it in the
  report, and do not invent the expected result.

## 2. Parser (`crates/sql`)

- Add tokens or keywords if needed; check the keyword stays usable as an
  identifier where PostgreSQL allows it.
- Add the AST node with spans; extend the printer; extend the random AST
  generator so the round-trip property test covers it.
- Known-but-unsupported variants fail with `0A000` naming the clause, at
  its span.
- Add seeds for the `sql` fuzz target in `fuzz/corpus/sql/` and run it for
  60 s.

## 3. Binder and planner (`crates/plan`)

- Resolve names and types; apply only the implicit casts the types spec
  allows; give the documented errors (`42703`, `42883`, `42804`, …).
- Add logical and physical plan nodes, or express the feature with
  existing ones.
- Any rewrite or alternative plan gets a forced-plan test showing
  identical rows (`docs/subsystems/planner.md`).
- `EXPLAIN` shows the new node (golden text test), once `EXPLAIN` exists.

## 4. Executor (`crates/exec`)

- Implement the operator or function with three-valued logic and checked
  arithmetic.
- Respect `work_mem` for anything that buffers rows, and check the
  interrupt flag at least every 1,000 rows.
- Unit tests for the operator or function alone, including NULLs and
  errors.

## 5. SQL logic tests

- Add or extend `tests/slt/<area>/<feature>.slt`:
  - the normal cases
  - NULLs, empty inputs, boundaries
  - each error, by SQLSTATE (`statement error 22012`)
  - `rowsort` for queries without `ORDER BY`
- Run `cargo test -p hardshelldb --test slt`.
- Once `cargo xtask diff-pg` exists (M9.6), run it on the new file and list
  any difference; a real difference is either a bug or a documented entry
  in "Differences from PostgreSQL".

## 6. Documentation

- Update the feature's row in `docs/sql-reference.md`: status `Built`,
  notes, and any difference from PostgreSQL in the differences table.
- Update the subsystem specs (`sql-frontend.md`, `planner.md`,
  `executor.md`, `types-and-values.md`) where the design changed.

## 7. Report

- The semantics you implemented, with their sources.
- Files changed in each crate.
- Commands run and results (unit tests, SLT, fuzz seconds, `diff-pg` if
  run).
- "Verify against psql" items still open.
- Unsupported variants that now return `0A000`.
