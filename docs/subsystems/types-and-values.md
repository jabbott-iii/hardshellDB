# Subsystem: `hardshell-types`

SQL types and values: parsing and formatting text, comparison, casts,
arithmetic, and the two byte encodings the storage engine sees (the
order-preserving key encoding and the row encoding, ADR-0006). Strict
typing follows PostgreSQL (ADR-0004). Nothing is built yet; M4 builds it,
except `NUMERIC`'s full arithmetic and the date/time functions, which grow
with the SQL features that need them (M6, M7, M10).

## Contents

| Module | Provides |
| :- | :- |
| `ty` | `SqlType` (the declared type, with modifiers such as `NUMERIC(p,s)` and `VARCHAR(n)`), `TypeId`, PostgreSQL OIDs for the wire protocol |
| `value` | `Value` (owned) and `ValueRef<'a>` (borrowed from a row buffer) |
| `numeric` | `Numeric`: arbitrary-precision decimal, base 10,000 digits, precision ≤ 1000 |
| `datetime` | `Date`, `Time`, `Timestamp`, `TimestampTz`, `Interval`; ISO 8601 parsing and formatting; civil-date arithmetic |
| `text` | Parse and format every type's text form (PostgreSQL's input/output functions) |
| `cast` | The cast table: implicit, assignment and explicit casts, and their conversions |
| `ops` | Comparison (three-valued), arithmetic with overflow checks, `LIKE`/`ILIKE` matching, simple Unicode case mapping (ADR-0014) |
| `encoding::key` | Order-preserving key encoding: `memcmp` of encodings equals SQL order |
| `encoding::row` | Row encoding: compact, versioned, decodable column by column |
| `uuid` | 16-byte UUIDs, text forms |

## Types

| SQL type (aliases) | `Value` variant | Storage | PostgreSQL OID |
| :- | :- | :- | :- |
| `BOOLEAN` (`BOOL`) | `Bool(bool)` | 1 byte | 16 |
| `SMALLINT` (`INT2`) | `Int16(i16)` | 2 | 21 |
| `INTEGER` (`INT`, `INT4`) | `Int32(i32)` | 4 | 23 |
| `BIGINT` (`INT8`) | `Int64(i64)` | 8 | 20 |
| `REAL` (`FLOAT4`) | `Float32(f32)` | 4 | 700 |
| `DOUBLE PRECISION` (`FLOAT8`, `FLOAT`) | `Float64(f64)` | 8 | 701 |
| `NUMERIC(p,s)` (`DECIMAL`) | `Numeric(Numeric)` | variable | 1700 |
| `TEXT` | `Text(String)` | variable | 25 |
| `VARCHAR(n)` (`CHARACTER VARYING`) | `Text(String)` | variable | 1043 |
| `BYTEA` | `Bytea(Vec<u8>)` | variable | 17 |
| `DATE` | `Date(Date)` | `i32` days since 2000-01-01 | 1082 |
| `TIME` (without time zone) | `Time(Time)` | `i64` µs since midnight | 1083 |
| `TIMESTAMP` (without time zone) | `Timestamp(Timestamp)` | `i64` µs since 2000-01-01 00:00 | 1114 |
| `TIMESTAMPTZ` | `TimestampTz(TimestampTz)` | `i64` µs since 2000-01-01 00:00 UTC | 1184 |
| `INTERVAL` | `Interval(Interval)` | months `i32`, days `i32`, µs `i64` | 1186 |
| `UUID` | `Uuid([u8; 16])` | 16 | 2950 |
| (untyped literal) | — | — | 705 (`unknown`) |

Not supported until the backlog: `CHAR(n)`, `TIMETZ`, `JSON`/`JSONB`,
arrays, composite types, enums, ranges, `MONEY`, geometric types. Naming
one is `0A000`.

```rust
#[non_exhaustive]
pub enum Value {
    Null,
    Bool(bool), Int16(i16), Int32(i32), Int64(i64), Float32(f32), Float64(f64),
    Numeric(Numeric), Text(String), Bytea(Vec<u8>),
    Date(Date), Time(Time), Timestamp(Timestamp), TimestampTz(TimestampTz),
    Interval(Interval), Uuid([u8; 16]),
}

pub enum SqlType {
    Bool, Int16, Int32, Int64, Float32, Float64,
    Numeric { precision_scale: Option<(u16, u16)> },  // None = unconstrained
    Text, Varchar { max_chars: Option<u32> },
    Bytea, Date, Time, Timestamp, TimestampTz, Interval, Uuid,
}
```

## Semantics (PostgreSQL's, for what we support)

- **NULL:** comparisons with `NULL` are `NULL`; `AND`/`OR` use three-valued
  logic; `IS [NOT] DISTINCT FROM` treats `NULL`s as equal. In sorting and
  indexes, `NULL` is larger than every value: last in ascending order,
  first in descending, unless `NULLS FIRST`/`NULLS LAST` says otherwise.
- **Literals:** `'...'` is untyped (`unknown`) until context fixes its type;
  then its text is parsed by that type's input function (`22P02`, `22007`
  on failure). Integer literals are `INTEGER` if they fit, else `BIGINT`,
  else `NUMERIC`; literals with a point or exponent are `NUMERIC`.
- **Integers:** overflow is `22003`; division truncates toward zero; `%`
  takes the dividend's sign; dividing by zero is `22012`.
- **Floats:** IEEE 754. Overflow to infinity from finite operands is
  `22003`; division by zero is `22012`, as in PostgreSQL. `'NaN'`,
  `'Infinity'` and `'-Infinity'` are accepted as input; `NaN` equals `NaN`
  and sorts above every other value, and `-0.0 = 0.0`.
- **NUMERIC:** exact. `NUMERIC(p,s)` rounds to `s` places on assignment,
  half away from zero, and a value needing more than `p` digits is
  `22003`. Unconstrained `NUMERIC` keeps the scale it computes, with
  division producing at least 16 significant digits (PostgreSQL's rule:
  **verify** the exact display scale against PostgreSQL in differential
  tests).
- **Text:** valid UTF-8 only (`22021`); compared by code point (ADR-0014).
  `VARCHAR(n)` counts characters. Assigning a longer value is `22001`; an
  explicit cast `::VARCHAR(n)` truncates silently, as in PostgreSQL.
- **Date/time:** ISO 8601 input and output (PostgreSQL's `DateStyle = ISO`).
  `TIMESTAMPTZ` is stored as UTC and shown in the session `TimeZone`, which
  is limited to `UTC` and fixed offsets (`+05:30`) until a time zone
  database decision. Ranges follow PostgreSQL: `DATE` from 4713 BC to
  5874897 AD, timestamps from 4713 BC to 294276 AD; past them is `22008`.
- **INTERVAL:** comparison treats a month as 30 days and a day as 24 hours,
  as PostgreSQL does, so `'1 month' = '30 days'` is true.
- **Aggregate result types** follow PostgreSQL: `sum(INTEGER)` is `BIGINT`,
  `sum(BIGINT)` and `avg` of any integer type are `NUMERIC`, `avg(REAL)` is
  `DOUBLE PRECISION`, `count` is `BIGINT`.

## Casts

The binder chooses casts from one table in `cast`, with three contexts as in
PostgreSQL:

| From → to | Context |
| :- | :- |
| `SMALLINT` → `INTEGER` → `BIGINT` → `NUMERIC` → `REAL` → `DOUBLE PRECISION` (each step and its compositions) | implicit |
| `VARCHAR` → `TEXT` | implicit |
| `DATE` → `TIMESTAMP` → `TIMESTAMPTZ` | implicit |
| Any narrowing numeric cast (`BIGINT` → `INTEGER`, `NUMERIC` → `BIGINT`, `DOUBLE PRECISION` → `INTEGER`, ...) | assignment |
| `TEXT` → `VARCHAR(n)` (checks length), `TIMESTAMPTZ` → `TIMESTAMP`, `TIMESTAMP` → `DATE` | assignment |
| Any type ↔ `TEXT` (through the type's input/output functions) | explicit, plus assignment *to* `TEXT` |
| `BOOLEAN` ↔ `INTEGER` | explicit |

Rounding when casting to an integer: from `NUMERIC`, half away from zero;
from `REAL`/`DOUBLE PRECISION`, half to even (PostgreSQL uses `rint()`;
**verify** both in differential tests).

## Key encoding (ADR-0006)

Each column is encoded so that comparing whole keys with `memcmp` gives the
SQL order, column by column:

- A marker byte: `0x01` for a value, `0x02` for `NULL` (so `NULL` sorts
  last).
- `BOOLEAN`: `0x00`/`0x01`. Integers, `DATE`, `TIME`, timestamps:
  big-endian with the sign bit flipped.
- Floats: `-0.0` becomes `0.0` and every `NaN` the canonical `NaN`; then the
  IEEE bits big-endian, with all bits flipped for negatives and the sign
  bit flipped for positives (so `NaN` sorts above `+Infinity`).
- `TEXT`/`BYTEA`: the bytes with `0x00` escaped as `0x00 0xFF`, terminated
  by `0x00 0x00`.
- `NUMERIC`: sign, then a biased exponent, then base-100 digit pairs and a
  terminator, all inverted for negatives. The exact layout is fixed in M4.
- `INTERVAL`: the comparison value (total µs with 30-day months, as `i128`).
- `UUID`: the 16 bytes.
- A `DESC` column inverts every byte of its encoding (marker included), so
  `NULL` comes first, matching PostgreSQL's `DESC` default.

The key encoding is **not always invertible**: equal values that display
differently (`1.0` and `1.00`, `'1 month'` and `'30 days'`) encode the
same. Index-only scans therefore read such columns from the table row.

## Row encoding

```
row     := version:u8  ncols:varint  null_bitmap:[u8; ceil(ncols/8)]  value*
value   := fixed-width little-endian bytes            (BOOLEAN .. UUID fixed types)
         | len:varint bytes                           (NUMERIC, TEXT, VARCHAR, BYTEA)
```

- Values appear in column order, `NULL`s omitted (the bitmap says which).
- A row with fewer columns than its table reads the missing trailing
  columns as the default recorded when the column was added (M10's `ALTER
  TABLE ADD COLUMN`), so adding a column never rewrites the table.
- Decoding is lazy: `RowRef::get(i)` walks to column `i` without allocating.

## Invariants

- For any two values `a`, `b` of one type: `cmp(a, b) ==
  memcmp(key(a), key(b))`, with ties exactly when SQL says the values are
  equal.
- `decode_row(encode_row(r)) == r` for every row of a table's types.
- Text formatting then parsing round-trips every value of every type
  (`Float` uses the shortest round-trip representation).
- No arithmetic in this crate can panic: every operation on integers uses
  `checked_*` and maps failure to `22003` or `22012`.

## Limits

| Limit | Default | When exceeded |
| :- | :- | :- |
| `NUMERIC` precision | 1000 digits | `22003` (declared `p` above 1000: `22023`) |
| `VARCHAR(n)` | `n` ≤ 10,485,760 | `22023` at `CREATE TABLE` |
| One value | 1 GiB (ADR-0006) | `54000` |
| Columns per row | 1,600 | `54011` (`too_many_columns`) |

## Tests

- Property tests: key order equals value order for every type (including
  `NULL`, `NaN`, `-0.0`, empty strings, embedded `0x00`); row round-trips;
  text round-trips; casts that should be lossless are.
- Unit tests from PostgreSQL's documented examples for date/time input and
  interval arithmetic.
- Differential tests (M9): the same casts, arithmetic and formatting
  through `psql` against a real PostgreSQL.
- Fuzz targets: `types-text` (every input function on arbitrary text),
  `key` (decode arbitrary keys, then re-encode), `row` (decode arbitrary
  bytes against a random schema).

## Open questions

- `NUMERIC` `NaN` and `±Infinity` (PostgreSQL supports both) and date/time
  `±infinity`: supported or `0A000`? Default until decided: `0A000`.
- BC dates in input and output (`'0044-03-15 BC'`).
- A time zone database for named zones (`America/Phoenix`): the IANA data
  is public domain, but compiling it is a project of its own.
