# Subsystem: `hardshell-exec`

Runs physical plans: pull-based operators (ADR-0012), expression
evaluation, the built-in functions and aggregates, DML with constraint
checks and index maintenance, and the per-query memory budget. It depends
on `core`, `os` (temporary files for spilling), `types`, `storage`,
`catalog` and `plan`. Nothing is built yet; M6 builds scans, filter,
project, sort, limit, `VALUES` and single-table DML; M7 joins, aggregates,
set operations, subqueries, CTEs and constraints; M9 spilling and
index-only scans; M10 window functions.

## Contents

| Module | Provides |
| :- | :- |
| `operator` | The `Operator` trait and `open(plan, ...)`, which builds an operator tree from a `PhysicalPlan` |
| `scan` | Sequential scan of a table tree; index range scan (index tree then table lookup); index-only scan (M9) |
| `relational` | Filter, project, limit/offset, `VALUES`, distinct, set operations, CTE materialization |
| `join` | Nested-loop join (any predicate), hash join (equality; spills from M9), and semi/anti variants |
| `aggregate` | Hash aggregation and streaming aggregation over sorted input; the aggregate functions |
| `sort` | In-memory sort with a reused buffer; external merge sort through temporary files (M9) |
| `window` | Window functions over sorted partitions (M10) |
| `dml` | Insert, update, delete: row encoding, row ids, index maintenance, constraint checks, `RETURNING` |
| `eval` | Expression evaluation over a row, three-valued logic, short-circuiting |
| `functions` | Scalar functions: math, string, date/time, conditional, sequence functions |
| `memory` | `MemoryBudget`: reservations against `work_mem` |
| `context` | `Context`: settings, the interrupt flag, the statement deadline, the transaction start time |

## Key types

```rust
pub trait Operator {
    /// The next row, or None when exhausted. Rows are reused buffers: a
    /// RowRef is valid until the next call.
    fn next(&mut self, cx: &mut Context) -> Result<Option<RowRef<'_>>, ExecError>;
    fn output(&self) -> &OutputSchema;
    fn stats(&self) -> OperatorStats;                     // rows out, time; for EXPLAIN ANALYZE (M9)
}

pub struct Context {
    pub work_mem: MemoryBudget,
    pub interrupt: Arc<AtomicBool>,                       // InterruptHandle sets it (ADR-0010)
    pub deadline: Option<Instant>,                        // statement_timeout
    pub txn_start: TimestampTz,                           // now() / CURRENT_TIMESTAMP
    pub vfs: Arc<dyn Vfs>,                                // temporary files (M9)
    pub temp_dir: PathBuf,
}

pub struct MemoryBudget { limit: usize, used: usize }

impl MemoryBudget {
    pub fn reserve(&mut self, bytes: usize) -> Result<Reservation, ExecError>;  // 53200 before M9
}

pub fn open<'t>(plan: PhysicalPlan, txn: TxnRef<'t>, params: &[Value], cx: Context)
    -> Result<Cursor<'t>, ExecError>;
```

## Rules

- **Interruption:** every operator that loops checks `cx.interrupt` and the
  deadline at least every 1,000 rows and in every wait (ADR-0010); either
  ends the statement with `57014`.
- **Evaluation order is not a contract.** `AND`/`OR` short-circuit left to
  right, and `CASE` evaluates only the chosen branch, but as in PostgreSQL
  the docs promise nothing about the order of independent subexpressions.
- **Errors abort the statement.** There is no partial result: a DML
  statement that fails after writing some rows leaves the transaction to
  roll back (autocommit) or in the failed state (explicit transaction,
  `25P02` afterwards).
- **DML reads before it writes when they overlap.** If an `UPDATE` or
  `DELETE` scans the table or an index it modifies, it first collects the
  target row ids (under the memory budget; spilling from M9), so it never
  sees its own changes (the Halloween problem).
- **Constraints**, as PostgreSQL checks them when not deferrable:
  `NOT NULL` (`23502`) and `CHECK` (`23514`) per row before it is written;
  `UNIQUE` and `PRIMARY KEY` (`23505`) per row as each index entry is
  inserted, so `UPDATE t SET id = id + 1` can fail on a transient
  duplicate exactly as it does in PostgreSQL; foreign keys (`23503`) per
  row, after the row's own write. `DEFERRABLE` is `0A000`.
- **Foreign key actions:** `NO ACTION`, `RESTRICT`, `CASCADE`, `SET NULL`
  and `SET DEFAULT`, all in M7.
- **`RETURNING`** yields the rows as written (after defaults and casts),
  through the same cursor as a query.
- **Scalar subqueries** returning more than one row are `21000`.
- **`LIMIT`/`OFFSET`** with a negative count is `2201W` / `2201X`, as in
  PostgreSQL.
- Rows come back in an unspecified order without `ORDER BY`; SQL logic
  tests use `rowsort` for those.

## Built-in functions

`docs/sql-reference.md` ("Functions") is the authoritative list, with the
roadmap task that builds each set: scalar set 1 (M6.3), aggregates (M7.2),
sequences (M7.8), set 2 (M7.10), window functions (M10.1), set 3 (M10.7).
The binder's `functions` table (`planner.md`) holds each function's
signatures and result types; the executor holds its implementation. Every
function:

- handles NULL inputs as PostgreSQL does (most are strict: any NULL
  argument gives NULL; `coalesce`, `concat` and aggregates are not);
- uses checked arithmetic and the documented SQLSTATE codes;
- has unit tests here and an SLT file.

Aggregates accept `DISTINCT` and `FILTER (WHERE ...)`.

## Invariants

- No operator allocates per row in steady state: rows, keys and hash-table
  entries reuse buffers; allocations are reserved against `work_mem`.
- Operators never hold a page pinned across a `next()` that returns to the
  caller, so a slow consumer cannot pin the buffer pool.
- An operator that returned `Ok(None)` keeps returning it; one that
  returned an error is not called again.
- Every arithmetic path uses the checked operations of `hardshell-types`;
  nothing in the executor panics on any data.

## Limits

| Limit | Default | When exceeded |
| :- | :- | :- |
| `work_mem` per query | 64 MiB (`SET work_mem`) | `53200` until M9; then sorts, hash joins, hash aggregates and DML row-id lists spill to temporary files |
| `temp_file_limit` per query (M9) | Default decided in M9 | `53400` (`configuration_limit_exceeded`), as PostgreSQL; a full disk is `53100` |
| `statement_timeout` | 0 (off) | `57014` |
| Recursive CTE iterations (M10) | 1,000,000 | `54000` |

## Tests

- Operator unit tests over in-memory inputs: each operator alone, with
  `NULL`s, empty inputs, duplicates and type edge cases.
- SQL logic tests (`tests/slt/`, run by the facade's `slt` test) are the
  main suite: every feature lands with `.slt` files.
- Model-based DML tests: random `INSERT`/`UPDATE`/`DELETE` against a table
  and an in-memory model, with every index checked against the table after
  each statement.
- Spill tests (M9): the same queries with `work_mem` at its minimum give the
  same results as with plenty.
- Allocation tests: a steady-state scan and a hash join allocate nothing per
  row after warm-up (counting allocator in the test binary).
- Interrupt tests: a long query is cancelled within 50 ms of the interrupt.

## Open questions

- Whether `EXPLAIN ANALYZE` of a DML statement executes it (PostgreSQL
  does); proposed: yes, inside the caller's transaction, as in PostgreSQL.
- Batch-at-a-time execution, behind ADR-0012's trigger.
