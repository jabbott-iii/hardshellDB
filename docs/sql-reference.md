# SQL reference

What hardshellDB's SQL supports, what it will support, and where it differs
from PostgreSQL (ADR-0004). This file is the contract for the SQL surface:

- A statement, clause, type, operator or function not listed here is not
  supported, and must fail with `0A000` (`feature_not_supported`) or
  `42601` (`syntax_error`), never be silently accepted (ADR-0011).
- **Status** is `Planned`, `Built` (its roadmap task is ticked), or
  `Backlog`. A task that ticks a feature changes its status here, and
  `/add-sql-feature` does this for you.
- Semantics follow PostgreSQL unless "Differences from PostgreSQL" says
  otherwise. Error codes are PostgreSQL's SQLSTATE codes; message texts are
  our own.

Status today: nothing is built.

## Lexical rules

| Feature | Status | Task | Notes |
| :- | :- | :- | :- |
| Unquoted identifiers folded to lower case; `"quoted"` identifiers kept as written | Planned | M5.1 | Longer than 63 bytes is truncated at a character boundary, with a notice, as in PostgreSQL |
| String constants `'...'` with `''`; `E'...'` escapes; `$$...$$` and `$tag$...$tag$` | Planned | M5.1 | |
| Numeric constants: integers, decimals, exponents | Planned | M5.1 | Integer constants are `integer`, `bigint` or `numeric` by size, as in PostgreSQL |
| Parameters `$1` … `$65535` | Planned | M5.1 | |
| Comments `--` and nested `/* */` | Planned | M5.1 | |
| Several statements separated by `;` | Planned | M5.4 | One `Connection::execute_batch` call or the CLI |

## Data types

| Type | Aliases | Status | Task | Notes |
| :- | :- | :- | :- | :- |
| `BOOLEAN` | `bool` | Planned | M4.1 | |
| `SMALLINT` | `int2` | Planned | M4.1 | |
| `INTEGER` | `int`, `int4` | Planned | M4.1 | |
| `BIGINT` | `int8` | Planned | M4.1 | |
| `REAL` | `float4` | Planned | M4.1 | |
| `DOUBLE PRECISION` | `float8`, `float` | Planned | M4.1 | NaN sorts above all numbers, as in PostgreSQL |
| `NUMERIC(p, s)` | `decimal` | Planned | M4.2 | Precision ≤ 1000; `NaN` and infinities: verify PostgreSQL behavior in M4.2 |
| `TEXT` | | Planned | M4.1 | UTF-8; `"C"` collation only (ADR-0014) |
| `VARCHAR(n)` | `character varying` | Planned | M4.1 | Too long on assignment is `22001` |
| `BYTEA` | | Planned | M4.1 | Hex output format |
| `DATE` | | Planned | M4.3 | |
| `TIME` | `time without time zone` | Planned | M4.3 | |
| `TIMESTAMP` | `timestamp without time zone` | Planned | M4.3 | Microsecond resolution |
| `TIMESTAMPTZ` | `timestamp with time zone` | Planned | M4.3 | Stored as UTC; session `TimeZone` limited to UTC and fixed offsets |
| `INTERVAL` | | Planned | M4.3 | |
| `UUID` | | Planned | M4.4 | |
| `SERIAL`, `BIGSERIAL` | | Planned | M7.8 | Shorthand for an integer column with a sequence default |
| `CHAR(n)` | `character` | Backlog | — | `0A000` until then |
| `JSON`, `JSONB` | | Backlog | — | |
| Arrays, enums, domains, composite and range types | | Backlog | — | |

## Statements

### Data definition

| Statement | Status | Task | Notes |
| :- | :- | :- | :- |
| `CREATE TABLE [IF NOT EXISTS]` with column and table constraints | Planned | M5.3, M6.1 | `TEMPORARY`, `UNLOGGED`, `PARTITION BY`, `INHERITS`: `0A000` |
| `DROP TABLE [IF EXISTS] ... [CASCADE \| RESTRICT]` | Planned | M5.3, M6.1, M7.5 | |
| `CREATE [UNIQUE] INDEX [IF NOT EXISTS] name ON t (cols [ASC\|DESC] [NULLS FIRST\|LAST])` | Planned | M5.3, M6.1 | Expression and partial indexes: Backlog |
| `DROP INDEX [IF EXISTS]` | Planned | M5.3, M6.1 | |
| `CREATE VIEW`, `DROP VIEW` | Planned | M7.5 | Views are read-only |
| `CREATE SEQUENCE`, `DROP SEQUENCE` | Planned | M7.8 | |
| `ALTER TABLE` (`ADD`/`DROP`/`RENAME COLUMN`, `RENAME TO`, `ADD`/`DROP CONSTRAINT`, `ALTER COLUMN ... SET/DROP NOT NULL`, `SET/DROP DEFAULT`, `TYPE`) | Planned | M10.3 | |
| `CREATE SCHEMA` | Backlog | — | Only `public` exists; `public.t` is accepted |

### Constraints

| Constraint | Status | Task | Error |
| :- | :- | :- | :- |
| `NOT NULL` | Planned | M6.4 | `23502` |
| `CHECK (expr)` | Planned | M6.4 | `23514` |
| `PRIMARY KEY`, `UNIQUE` | Planned | M6.4 | `23505` |
| `DEFAULT expr` | Planned | M6.4 | |
| `REFERENCES` (`NO ACTION`, `RESTRICT`, `CASCADE`, `SET NULL`, `SET DEFAULT`; `MATCH SIMPLE`) | Planned | M7.6 | `23503`; `DEFERRABLE` is `0A000` |
| `GENERATED {ALWAYS \| BY DEFAULT} AS IDENTITY` | Planned | M7.8 | `428C9` |

### Queries

| Feature | Status | Task |
| :- | :- | :- |
| `SELECT` list, `*`, `t.*`, aliases | Planned | M6.3 |
| `FROM` a table or subquery; `WHERE`; `ORDER BY ... [ASC\|DESC] [NULLS FIRST\|LAST]` | Planned | M6.3 |
| `LIMIT`, `OFFSET`, `FETCH FIRST n ROWS ONLY` | Planned | M6.3 |
| `VALUES` lists | Planned | M6.3 |
| `SELECT DISTINCT`; `DISTINCT ON (...)` | Planned | M7.2 (`DISTINCT ON`: Backlog) |
| Joins: `INNER`, `LEFT`, `RIGHT`, `FULL`, `CROSS`, `NATURAL`, `ON`, `USING` | Planned | M7.1 |
| `GROUP BY`, `HAVING`, aggregates | Planned | M7.2 |
| Scalar subqueries, `IN`, `NOT IN`, `EXISTS`, correlated subqueries | Planned | M7.3 |
| `UNION`, `INTERSECT`, `EXCEPT` (each `[ALL]`) | Planned | M7.4 |
| `WITH` (non-recursive) | Planned | M7.4 |
| `WITH RECURSIVE` | Planned | M10.2 |
| Window functions, `OVER`, `WINDOW` | Planned | M10.1 |
| `GROUPING SETS`, `ROLLUP`, `CUBE` | Backlog | — |
| `LATERAL` | Backlog | — |
| `SELECT ... FOR UPDATE / FOR SHARE` | Backlog | — |

### Data manipulation

| Statement | Status | Task |
| :- | :- | :- |
| `INSERT INTO t [(cols)] VALUES ... \| SELECT ... \| DEFAULT VALUES` | Planned | M6.4 |
| `UPDATE t SET ... [FROM ...] [WHERE ...]` | Planned | M6.4 (`FROM`: M7.1) |
| `DELETE FROM t [USING ...] [WHERE ...]` | Planned | M6.4 (`USING`: M7.1) |
| `RETURNING` | Planned | M7.9 |
| `INSERT ... ON CONFLICT ... DO NOTHING \| DO UPDATE` | Planned | M10.4 |
| `MERGE` | Backlog | — |
| `COPY` | Backlog | — (the CLI's `\copy`, M8.4, covers files) |

### Transactions and sessions

| Statement | Status | Task | Notes |
| :- | :- | :- | :- |
| `BEGIN`, `START TRANSACTION`, `COMMIT`, `END`, `ROLLBACK` | Planned | M6.5 | |
| `SET TRANSACTION ISOLATION LEVEL ...`, `READ ONLY`/`READ WRITE` | Planned | M6.5 | Every level runs as serializable (ADR-0008) |
| `SAVEPOINT`, `RELEASE [SAVEPOINT]`, `ROLLBACK TO [SAVEPOINT]` | Planned | M10.5 | |
| `SET name = value`, `SET name TO value`, `RESET`, `SHOW name` | Planned | M6.5 | Settings in `docs/01-architecture.md` |
| `PREPARE`, `EXECUTE`, `DEALLOCATE` (SQL-level) | Backlog | — | Use the API's `prepare` |

### Utility

| Statement | Status | Task | Notes |
| :- | :- | :- | :- |
| `EXPLAIN` | Planned | M7.9 | Our own plan format |
| `ANALYZE [table]` | Planned | M9.1 | |
| `VACUUM`, `VACUUM INTO 'path'` | Planned | M11.3 | `VACUUM INTO` is not PostgreSQL |
| `CHECK DATABASE` | Planned | M11.2 | Not PostgreSQL |

## Operators

| Operators | Status | Task | Notes |
| :- | :- | :- | :- |
| `+ - * / %` on numbers | Planned | M6.3 | Integer overflow `22003`; division by zero `22012`; integer division truncates |
| `^` (power) | Planned | M7.10 | Left-associative, as in PostgreSQL |
| `= <> != < <= > >=` | Planned | M6.3 | |
| `AND OR NOT` with three-valued logic | Planned | M6.3 | |
| `IS [NOT] NULL`, `IS [NOT] DISTINCT FROM`, `IS [NOT] TRUE/FALSE/UNKNOWN` | Planned | M6.3 | |
| `BETWEEN [SYMMETRIC]`, `IN (list)` | Planned | M6.3 | |
| `\|\|` (text and bytea concatenation) | Planned | M6.3 | |
| `LIKE`, `NOT LIKE`, `ILIKE`, `NOT ILIKE`, `ESCAPE` | Planned | M6.3 (`ILIKE`: M7.10) | |
| `CASE`, `CAST(x AS t)`, `x::t` | Planned | M6.3 | |
| Date/time arithmetic (`date + integer`, `timestamp ± interval`, `timestamp - timestamp`) | Planned | M7.10 | |
| Bitwise `& \| # ~ << >>` on integers | Planned | M7.10 | |
| `~`, `~*`, `SIMILAR TO` | Backlog | — | Needs a regex engine |

## Functions

### Set 1 (M6.3)

`abs`, `ceil`/`ceiling`, `floor`, `round`, `trunc`, `mod`, `length`/
`char_length`, `octet_length`, `substring`/`substr`, `position`,
`coalesce`, `nullif`, `greatest`, `least`.

### Aggregates (M7.2)

`count(*)`, `count(x)`, `count(DISTINCT x)`, `sum`, `avg`, `min`, `max`,
`bool_and`, `bool_or`, `every`, `string_agg`; each with `DISTINCT` and
`FILTER (WHERE ...)`.

### Set 2 (M7.10)

- Text: `lower`, `upper` (Unicode simple case mapping, ADR-0014), `trim`,
  `ltrim`, `rtrim`, `btrim`, `replace`, `left`, `right`, `lpad`, `rpad`,
  `concat`, `concat_ws`, `split_part`, `strpos`, `repeat`, `reverse`.
- Math: `power`, `sqrt`, `exp`, `ln`, `log`, `sign`, `pi`.
- Date/time: `now()`, `current_date`, `current_time`, `current_timestamp`,
  `localtimestamp` (fixed per transaction); `extract`, `date_part`,
  `date_trunc`, `age`, `make_date`, `make_timestamp`, `to_timestamp(double
  precision)`.

### Window functions (M10.1)

`row_number`, `rank`, `dense_rank`, `lag`, `lead`, `first_value`,
`last_value`, `nth_value`, and every aggregate with `OVER`.

### Set 3 (M10.7)

- `generate_series` (integers, numerics and timestamps) in `FROM`.
- Text: `initcap`, `translate`, `format`, `encode`/`decode` (`hex`,
  `base64`), `md5`.
- Math: `log10`, `cbrt`, `degrees`, `radians`, `sin`, `cos`, `tan`, `asin`,
  `acos`, `atan`, `atan2`, `width_bucket`.
- Random: `random()`, `setseed()`, `gen_random_uuid()`. These need an
  entropy source; `std` has no stable API for OS randomness, so M10.7
  starts with an ADR on where the seed comes from.

### Backlog

`to_char` and the other formatting functions, regular-expression
functions, JSON functions, array functions, full-text search.

## Differences from PostgreSQL

Each entry names the behavior, why, and its ADR. `cargo xtask diff-pg`
(M9.6) skips these by name.

| Area | hardshellDB | PostgreSQL | Why |
| :- | :- | :- | :- |
| Collation | `"C"` (code point order) only | Locale collations | ADR-0014 |
| Isolation | Every transaction serializable; one writer at a time; waiting writers fail with `55P03` after the busy timeout | Read committed by default; row-level locks | ADR-0008 |
| Time zones | UTC and fixed offsets only | IANA time zone database | No tz data yet (backlog) |
| Schemas | `public` only | Any number | Backlog |
| `VACUUM INTO`, `CHECK DATABASE` | Supported | Not present | Embedded maintenance (M11) |
| Server-side file access (`COPY ... FROM 'file'`, `pg_read_file`) | Not supported | Superuser only | Embedded SQL must not read arbitrary files |
| Catalogs | `hs_*` system tables and an `information_schema` subset | `pg_catalog` | ADR-0004 |
| Sequences after a crash | A value handed only to a transaction that never committed may be returned again after recovery (`docs/subsystems/catalog.md`); within a process, never, even after rollback | Values are not reused after a rollback; crash behavior to verify in M7.8 | Single writer and redo-only WAL (ADR-0007, ADR-0008) |
| `nextval` in a read transaction | Takes the write lock, so it can fail with `40001` on an old snapshot | No such conflict | ADR-0008 |
| Error messages | Our own text | PostgreSQL's text | Only SQLSTATE codes are part of the contract |
