# Subsystem: `hardshell-plan`

From AST to physical plan: the binder (names, types, casts, semantic
checks), the logical plan, rewrites, the cost model and physical planning.
It depends on `core`, `types`, `sql` and `catalog`; it reads no pages and
runs nothing. Nothing is built yet; M6 builds the binder and a rule-based
planner for single-table queries and DML, M7 adds joins, aggregates,
subqueries, set operations, CTEs, views and index selection, M9 statistics,
the cost model and join ordering, M10 window functions and recursive CTEs.

The one rule above all others: **plans never change answers**
(`docs/01-architecture.md`, principle 6).

## Contents

| Module | Provides |
| :- | :- |
| `bind` | Resolves names against a `Schema`, types every expression, inserts casts, resolves functions and operators, checks semantic rules |
| `functions` | The function and operator catalog: signatures, result types, volatility, and the executor entry each binds to |
| `logical` | `LogicalPlan` nodes and their output schemas |
| `rewrite` | Equivalence-preserving rewrites: constant folding, predicate pushdown, projection pruning, subquery decorrelation, outer-to-inner join simplification, view expansion |
| `cost` | Row-count and cost estimates from `hs_stats` (M9); fixed default selectivities before that |
| `join_order` | Dynamic programming over join orders up to 8 relations, greedy beyond (M9) |
| `physical` | `PhysicalPlan`: chooses access paths (sequential, index range, index-only), join algorithms, sort and aggregate strategies |
| `explain` | `EXPLAIN` text, PostgreSQL-like tree format; `EXPLAIN ANALYZE` adds actual rows and times from the executor (M9) |

## Binding rules (PostgreSQL's, for what we support)

- Unqualified column names resolve through the `FROM` clause; a name in two
  relations is `42702` (`ambiguous_column`); unknown is `42703`; unknown
  relation `42P01`.
- `ORDER BY` may name output columns (aliases or ordinals) or input
  expressions; `GROUP BY` may use input expressions, output aliases or
  ordinals. Out-of-range ordinals are `42P10`.
- Aggregates in `WHERE` or `GROUP BY`, or ungrouped columns in the select
  list of a grouped query, are `42803` (`grouping_error`).
- **Operator and function resolution:** exact signature match first; then
  candidates reachable by implicit casts (`types-and-values.md`, "Casts"),
  preferring the fewest casts; untyped literals take the type of the other
  operand. No candidate is `42883`; more than one equally good is `42725`
  (`ambiguous_function`).
- **Parameters** get their type from context (`$1 = bigint_col` makes `$1`
  `BIGINT`). A parameter with no context and no caller-supplied type is
  `42P18` (`indeterminate_datatype`).
- **Assignment** (`INSERT`, `UPDATE SET`) uses assignment casts to the
  column's type; a type with no assignment cast is `42804`.
- `CASE`, `COALESCE`, `UNION` and `VALUES` branches unify to one type by
  PostgreSQL's rule: the first non-unknown type's category, widened by
  implicit casts; no common type is `42804`.
- `now()` / `CURRENT_TIMESTAMP` are the transaction's start time, as in
  PostgreSQL; functions are classified immutable, stable or volatile, and
  only immutable ones are constant-folded.

## Key types

```rust
pub enum LogicalPlan {
    Scan { table: ObjectId, columns: Vec<usize> },
    Values { rows: Vec<Vec<Expr>>, schema: OutputSchema },
    Filter { input: Box<LogicalPlan>, predicate: Expr },
    Project { input: Box<LogicalPlan>, exprs: Vec<(Expr, String)> },
    Join { kind: JoinKind, left: Box<LogicalPlan>, right: Box<LogicalPlan>, on: Expr },  // Inner, Left, Right, Full, Semi, Anti
    Aggregate { input: Box<LogicalPlan>, group_by: Vec<Expr>, aggregates: Vec<AggCall> },
    Window { input: Box<LogicalPlan>, functions: Vec<WindowCall> },                      // M10
    Sort { input: Box<LogicalPlan>, keys: Vec<SortKey> },
    Limit { input: Box<LogicalPlan>, limit: Option<Expr>, offset: Option<Expr> },
    Distinct { input: Box<LogicalPlan> },
    SetOp { op: SetOp, all: bool, left: Box<LogicalPlan>, right: Box<LogicalPlan> },
    CteRef { id: CteId }, RecursiveCte { .. },                                            // recursive: M10
    Insert { table: ObjectId, input: Box<LogicalPlan>, on_conflict: Option<OnConflict>, returning: Vec<Expr> },
    Update { table: ObjectId, input: Box<LogicalPlan>, assignments: Vec<(usize, Expr)>, returning: Vec<Expr> },
    Delete { table: ObjectId, input: Box<LogicalPlan>, returning: Vec<Expr> },
    Ddl(DdlPlan), Utility(UtilityPlan),        // CREATE/DROP, SET/SHOW, BEGIN/COMMIT, CHECKPOINT ...
}

pub enum Expr {                                 // bound: every node has a type
    Column { input: usize, ty: SqlType },
    Literal(Value),
    Param { index: u16, ty: SqlType },
    Cast { expr: Box<Expr>, to: SqlType },
    Call { func: FuncId, args: Vec<Expr>, ty: SqlType },
    And(Vec<Expr>), Or(Vec<Expr>), Not(Box<Expr>),
    Case { .. }, InList { .. }, IsNull { .. },
    Subquery { plan: Box<LogicalPlan>, kind: SubqueryKind, correlated: bool },
}

pub enum AccessPath {
    SeqScan,
    IndexRange { index: ObjectId, lower: Bound<KeyPrefix>, upper: Bound<KeyPrefix> },
    IndexOnly { index: ObjectId, lower: Bound<KeyPrefix>, upper: Bound<KeyPrefix> },  // M9
}
```

## Rewrites

- **Constant folding** evaluates immutable expressions of constants at plan
  time. If evaluation fails (for example `1/0`), the expression is left
  unfolded so the error happens only if the row is actually evaluated.
- **Predicate pushdown** moves filters below joins and into scans; never
  below an outer join's null-producing side unless the predicate rejects
  `NULL`s (which also turns the outer join into an inner join).
- **Decorrelation:** `EXISTS`, `NOT EXISTS`, `IN (subquery)` become semi and
  anti joins when the subquery's correlation is a conjunction of
  equalities; `NOT IN` becomes an anti join only when both sides are
  provably non-null (its `NULL` semantics differ otherwise). Other
  correlated subqueries run as nested evaluation.
- **Views** expand to their stored query, bound in the view's schema
  version; a view whose dependencies changed incompatibly was already
  blocked by `DROP` (`catalog.md`).
- **Index selection:** a sargable predicate (`=`, `<`, `<=`, `>`, `>=`,
  `BETWEEN`, `IS NULL`, `IN` lists up to 64 items, `LIKE 'prefix%'`) on a
  prefix of an index's columns becomes a key range in the index's
  encoding.

## Cost model (M9)

- Inputs: table row counts and page counts, per-column null fraction,
  distinct count, most common values and an equi-depth histogram from
  `ANALYZE` (sample of up to 30,000 rows per table).
- Costs: pages read (sequential and random, with separate weights) plus
  per-row CPU for each operator, plus memory pressure (a sort or hash table
  expected to exceed `work_mem` is costed with its spill).
- The constants are calibrated on the reference machine in M9 and recorded
  in this spec.
- Before M9, without statistics: an index range on equality beats a
  sequential scan; otherwise sequential scan; joins in `FROM`-clause order
  with hash joins on equality predicates.

## Invariants

- Every rewrite is equivalence-preserving for every input, including
  `NULL`s, empty inputs and duplicate rows.
- Planning is deterministic: the same SQL, schema and statistics give the
  same plan (ties broken by a stable order, never by hash iteration).
- The binder never panics on any AST the parser produces; every bound
  expression has a concrete type (no `unknown` survives binding: an
  unresolved literal becomes `TEXT`, as in PostgreSQL).

## Limits

| Limit | Default | When exceeded |
| :- | :- | :- |
| Relations in one `FROM` (after view expansion) | 64 | `54001` |
| Exhaustive join ordering | 8 relations | Greedy ordering beyond |
| Function arguments | 100 (PostgreSQL's `FUNC_MAX_ARGS`) | `54023` (`too_many_arguments`) |
| View expansion depth | 32 | `54001` |

## Tests

- Binder unit tests: every rule above, with its error code and span.
- Equivalence tests: each rewrite applied and not applied gives the same
  rows on generated data (run through the executor in the facade's tests).
- Forced-plan tests: every access path and join algorithm forced in turn
  gives identical results for the SQL logic test corpus.
- `EXPLAIN` snapshot tests for representative queries.
- Differential tests against PostgreSQL (M9) cover type resolution
  through the answers they produce.

## Open questions

- A plan hint or setting to force access paths and join algorithms for
  tests (`SET enable_hashjoin = off`, as PostgreSQL does): proposed, as
  session settings that only affect plan choice.
- Whether prepared statements re-plan per parameter value (PostgreSQL's
  custom versus generic plans) or always use one generic plan.
