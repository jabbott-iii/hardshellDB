# Subsystem: `hardshell-catalog`

The schema: tables, columns, indexes, constraints, views, sequences and
statistics, stored in system B+trees inside the database file and loaded
into an immutable, versioned `Schema` per snapshot. It depends on `core`,
`types` and `storage` only; column defaults, `CHECK` expressions and view
bodies are stored as canonical SQL text and parsed by the binder
(`docs/01-architecture.md`, dependency matrix notes). Nothing is built yet;
M6 builds tables, columns and indexes, M7 adds constraints, views and
sequences, M9 statistics.

## Contents

| Module | Provides |
| :- | :- |
| `system` | The fixed schemas of the system tables, hard-coded (not self-describing), with their own catalog format version |
| `schema` | `Schema`: an immutable snapshot of every object, indexed by id and by name |
| `ddl` | Create, drop and (M10) alter objects inside a `WriteTxn`; dependency checks |
| `ids` | `ObjectId` allocation (a counter in `hs_tables`' first row) |
| `names` | Identifier rules: folding has happened in the parser; reserved prefixes; uniqueness |
| `stats` | `TableStats`, `ColumnStats` read from `hs_stats` (M9) |

## System tables

Each is a B+tree rooted from a slot of the file header's catalog roots
(`storage.md`). Rows use the ordinary row encoding (`types-and-values.md`).

| Table | One row per | Main columns |
| :- | :- | :- |
| `hs_tables` | table | id, name, root page, kind (table, sequence storage), created schema version |
| `hs_columns` | column | table id, ordinal, name, type, `NOT NULL`, default SQL text, identity sequence id, default for rows written before the column existed (M10) |
| `hs_indexes` | index | id, name, table id, root page, unique, columns with direction and nulls order, owning constraint |
| `hs_constraints` | constraint | id, name, table id, kind (`PRIMARY KEY`, `UNIQUE`, `CHECK`, `FOREIGN KEY`), columns, `CHECK` SQL text, referenced table and columns, `ON DELETE`/`ON UPDATE` actions |
| `hs_views` | view | id, name, canonical SQL text of the query, output column names and types |
| `hs_sequences` | sequence | id, name, start, increment, min, max, cycle, last value, owner column |
| `hs_stats` | analyzed column | table id, column, null fraction, distinct estimate, most common values, histogram bounds, row count at analysis (M9) |

## Key types

```rust
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct ObjectId(u32);

pub struct Schema {
    pub version: u64,                                 // the header's schema version
    tables: Vec<Arc<TableDef>>,
    by_name: FastHashMap<String, ObjectRef>,          // one namespace: `public`
    // indexes, constraints, views, sequences ...
}

pub struct TableDef {
    pub id: ObjectId,
    pub name: String,
    pub root: PageNo,
    pub columns: Vec<ColumnDef>,
    pub primary_key: Option<ObjectId>,                // a unique index (ADR-0006)
    pub indexes: Vec<ObjectId>,
    pub checks: Vec<CheckDef>,
    pub foreign_keys: Vec<ForeignKeyDef>,
}

pub struct ColumnDef {
    pub name: String,
    pub ty: SqlType,
    pub not_null: bool,
    pub default_sql: Option<String>,                  // parsed and bound by hardshell-plan
    pub identity: Option<ObjectId>,                   // GENERATED ... AS IDENTITY / SERIAL
}

pub struct IndexDef {
    pub id: ObjectId, pub name: String, pub table: ObjectId, pub root: PageNo,
    pub unique: bool, pub columns: Vec<IndexColumn>,  // column ordinal, ASC/DESC, NULLS FIRST/LAST
}

pub fn load(view: &dyn ReadView) -> Result<Schema, CatalogError>;
pub fn create_table(tx: &mut WriteTxn, def: NewTable) -> Result<ObjectId, CatalogError>;
pub fn drop_table(tx: &mut WriteTxn, schema: &Schema, name: &str, behavior: DropBehavior) -> Result<(), CatalogError>;
```

## Rules

- **One namespace, `public`.** `public.t` is accepted and means `t`. Other
  schema names are `3F000` (`invalid_schema_name`); `CREATE SCHEMA` is
  `0A000` until the backlog item. Names beginning with `hs_` are reserved
  for system objects (`42939`, `reserved_name`).
- **Tables, indexes, views and sequences share one name space**, as in
  PostgreSQL's relations: a duplicate is `42P07` (`duplicate_table`).
  Duplicate columns are `42701`; duplicate constraint names `42710`.
- **Caching:** the facade keeps one `Arc<Schema>` per schema version. A
  snapshot loads the schema of its own version, so a reader on an old
  snapshot keeps its old schema while DDL commits.
- **DDL bumps the schema version** in the file header inside the writing
  transaction; prepared plans for older versions re-plan
  (`docs/01-architecture.md`).
- **Row ids** for a table's next insert come from its B+tree's last key + 1;
  a table whose last row id is `i64::MAX` is `54000`.
- **`DROP`** frees the table's tree and every index tree to the free list.
  Dependent views or foreign keys make it `2BP01`
  (`dependent_objects_still_exist`) unless `CASCADE` (M7). `IF EXISTS`
  turns a missing object into a notice.
- **Sequences** are rows in `hs_sequences`. As in PostgreSQL, `nextval()`
  is not undone by a rollback:
  - The `Database` keeps an in-memory high-water mark per sequence, loaded
    from `hs_sequences` on first use. `nextval` advances it under a mutex
    and never moves it back, so a process never returns a value twice.
  - `nextval` also writes the new `last value` into `hs_sequences` in the
    calling transaction, so it needs the write lock (in a read
    transaction it upgrades, which can fail with `40001`, ADR-0008). If
    that transaction rolls back, the row reverts but the high-water mark
    does not.
  - After a crash, the high-water mark reloads from the last committed
    row, so values handed only to transactions that never committed can
    be returned again. Whether PostgreSQL behaves the same after a crash
    is to verify in M7; `docs/sql-reference.md` records the outcome.
- System tables are not exposed to SQL directly; M10's
  `information_schema` subset reads them.

## Invariants

- Every object id in any catalog row refers to an existing object of the
  right kind; every root page is a tree of the right kind.
- Every constraint that needs an index (`PRIMARY KEY`, `UNIQUE`) owns
  exactly one unique index with matching columns.
- A `Schema` never changes after it is built; DDL builds a new one.
- Loading the catalog never panics on a damaged file: unknown kinds, bad
  ids and dangling references are `XX001` naming the system table and row.

## Limits

| Limit | Value | When exceeded |
| :- | :- | :- |
| Columns per table | 1,600 | `54011` |
| Columns per index | 32 (PostgreSQL's default) | `54011` |
| Objects per database | 2^32 − 1 ids | `54000` |
| Identifier length | 63 bytes (truncated by the parser) | — |

## Tests

- DDL unit tests: create, drop, re-create; name collisions and their codes;
  rollback of DDL restores the old schema.
- Snapshot tests: a reader keeps its schema while DDL commits.
- Model-based tests: random DDL sequences against an in-memory model of the
  schema, compared after each commit and after reopening the file.
- `CHECK DATABASE` (M11) cross-checks catalog rows against the trees they
  name.

## Open questions

- Whether to expose read-only `hs_*` tables to SQL before
  `information_schema` exists (useful for the CLI's `\d` commands in M8).
- PostgreSQL's `pg_catalog` views that drivers query at connect time (M12):
  which, and generated from this catalog how.
