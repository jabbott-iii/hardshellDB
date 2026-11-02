---
paths:
  - "crates/sql/**"
  - "crates/plan/**"
  - "crates/exec/**"
  - "crates/types/**"
  - "tests/slt/**"
  - "docs/sql-reference.md"
---

# Rules for SQL behavior

PostgreSQL is the reference (ADR-0004). `docs/sql-reference.md` is the
contract: what it lists is supported and behaves like PostgreSQL unless it
documents a difference; what it does not list is rejected.

- **Unsupported means rejected.** Syntax we parse but do not support fails
  with `0A000` naming the feature. Never ignore a clause, an option or a
  modifier.
- **When unsure what PostgreSQL does, do not guess.** Cite PostgreSQL's
  documentation, or mark the test "verify against psql" and list it in the
  task's report; `cargo xtask diff-pg` (M9.6) settles it. Never write an
  expected result you have not derived from a source.
- **NULL and three-valued logic** in every new expression, operator,
  function and aggregate: NULL inputs, all-NULL groups, empty inputs. Test
  them in SLT.
- **Types are strict.** Implicit casts only as the cast table in
  `docs/subsystems/types-and-values.md` allows. A new implicit cast changes
  the dialect: it needs an ADR.
- **Errors use the right SQLSTATE.** Match PostgreSQL's code for the same
  condition. Tests check codes, never message text.
- **Overflow and precision:** integer overflow is `22003`; division by zero
  `22012`; `NUMERIC` results follow PostgreSQL's scale rules; floats follow
  IEEE 754 with PostgreSQL's NaN ordering.
- **Order:** without `ORDER BY`, row order is unspecified. SLT queries
  without `ORDER BY` use `rowsort`. With `ORDER BY`, NULLs sort last in
  ascending order and first in descending order, unless `NULLS FIRST/LAST`
  says otherwise.
- **Optimizations never change results.** Each rewrite or alternative plan
  has a forced-plan test showing identical rows (`docs/subsystems/planner.md`).
- **Every query keeps its limits:** `work_mem` for sorts and hash tables,
  the interrupt flag checked at least every 1,000 rows, `statement_timeout`.
- **New SQL surface** goes through `/add-sql-feature`, which updates the SQL
  reference and adds SLT files.
