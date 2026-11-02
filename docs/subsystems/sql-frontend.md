# Subsystem: `hardshell-sql`

The lexer, parser and abstract syntax tree for hardshellDB's
PostgreSQL-flavored SQL (ADR-0004, ADR-0011). It turns text into an AST
with byte spans, and prints an AST back to SQL. It knows nothing about
tables, types or storage (dependency matrix: `core` only). Nothing is built
yet; M5 builds the lexer, parser and printer for the grammar M6 and M7 need,
and M10 extends it. `docs/sql-reference.md` lists what is supported and is
the contract this crate implements.

## Contents

| Module | Provides |
| :- | :- |
| `lexer` | Tokens with spans: keywords, identifiers, literals, parameters, operators, punctuation; skips comments |
| `keywords` | The keyword table (reserved, non-reserved, type-name), one `const` array, sorted, with PostgreSQL's reserved status |
| `parser` | Recursive descent for statements; Pratt parsing for expressions with PostgreSQL's precedence |
| `ast` | Statement, query and expression nodes; every node carries a `Span` |
| `print` | Canonical SQL from an AST (`Display`), used by `EXPLAIN`, dumps, catalog storage of view bodies and defaults, and round-trip tests |
| `split` | Splits a script into statements and reports whether text ends inside a quote, comment or unfinished statement (the CLI's continuation prompt) |
| `error` | `ParseError { state: SqlState, message, span, expected: Vec<&'static str> }` |

## Lexical rules (PostgreSQL's)

- Unquoted identifiers and keywords are case-insensitive; identifiers fold
  to lower case. `"Quoted"` identifiers keep their case; `""` escapes a
  quote. Identifiers longer than 63 bytes are truncated at a character
  boundary, with a notice, as PostgreSQL does.
- String constants: `'...'` with `''` escaping a quote. `E'...'` escape
  strings and `$tag$...$tag$` dollar quoting are M10 (until then `0A000`).
- Numbers: `42`, `4.2`, `.5`, `4e2`, `4.2E-3`; the AST keeps the literal's
  text, and the binder types it (`types-and-values.md`, "Literals").
- Parameters: `$1` .. `$65535`; `$0` and numbers above 65535 are `42601`.
- Comments: `-- to end of line` and `/* ... */`, which nest as in
  PostgreSQL.
- Operators: `+ - * / % ^ = <> != < > <= >= || :: ~~ ~~*` and the
  punctuation `( ) [ ] , ; .`. Any other run of operator characters lexes
  as one operator token, as in PostgreSQL; the binder reports an operator
  it does not know as `42883` (`undefined_function`).

## Expression precedence (highest first, PostgreSQL's table)

| Level | Operators | Associativity |
| :- | :- | :- |
| 1 | `.` | left |
| 2 | `::` | left |
| 3 | `[ ]` (reserved for arrays: `0A000`) | left |
| 4 | unary `+ -` | right |
| 5 | `^` | left |
| 6 | `* / %` | left |
| 7 | `+ -` | left |
| 8 | any other operator (`||`, ...) | left |
| 9 | `BETWEEN IN LIKE ILIKE SIMILAR` | — |
| 10 | `< > = <= >= <>` | — |
| 11 | `IS ISNULL NOTNULL` | — |
| 12 | `NOT` | right |
| 13 | `AND` | left |
| 14 | `OR` | left |

## Key types

```rust
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Span { pub start: u32, pub end: u32 }   // byte offsets into the statement text

pub struct Spanned<T> { pub node: T, pub span: Span }

pub enum Statement {
    Query(Box<Query>),
    Insert(Insert), Update(Update), Delete(Delete),
    CreateTable(CreateTable), CreateIndex(CreateIndex), CreateView(CreateView),
    CreateSequence(CreateSequence), Drop(Drop), AlterTable(AlterTable),       // AlterTable: M10
    Begin(TxnOptions), Commit, Rollback { to_savepoint: Option<Ident> },
    Savepoint(Ident), ReleaseSavepoint(Ident),                                // M10
    Set(SetVar), Show(Ident), Explain { analyze: bool, stmt: Box<Statement> },
    Checkpoint, CheckDatabase, VacuumInto(StringLit),                         // CHECK/VACUUM: M11
}

pub struct Query {
    pub with: Option<With>,
    pub body: SetExpr,
    pub order_by: Vec<OrderItem>,            // expr, ASC/DESC, NULLS FIRST/LAST
    pub limit: Option<Spanned<Expr>>,        // LIMIT or FETCH FIRST n ROWS ONLY
    pub offset: Option<Spanned<Expr>>,
}

pub enum SetExpr {
    Select(Box<Select>),
    Values(Vec<Vec<Spanned<Expr>>>),
    SetOp { op: SetOp, all: bool, left: Box<SetExpr>, right: Box<SetExpr> },
    Nested(Box<Query>),
}

pub enum Expr {
    Literal(Literal),                         // Number(text), String(text), Bool, Null
    Column(Vec<Ident>),                       // a, t.a
    Param(u16),
    Unary { op: UnaryOp, expr: Box<Spanned<Expr>> },
    Binary { op: BinaryOp, left: Box<Spanned<Expr>>, right: Box<Spanned<Expr>> },
    Cast { expr: Box<Spanned<Expr>>, ty: TypeName },        // CAST(x AS t) and x::t
    Case { operand: Option<Box<Spanned<Expr>>>, arms: Vec<(Spanned<Expr>, Spanned<Expr>)>, default: Option<Box<Spanned<Expr>>> },
    Between { expr: Box<Spanned<Expr>>, negated: bool, low: Box<Spanned<Expr>>, high: Box<Spanned<Expr>> },
    InList { expr: Box<Spanned<Expr>>, negated: bool, list: Vec<Spanned<Expr>> },
    InSubquery { expr: Box<Spanned<Expr>>, negated: bool, query: Box<Query> },
    Exists { negated: bool, query: Box<Query> },
    Subquery(Box<Query>),
    Like { expr: Box<Spanned<Expr>>, negated: bool, case_insensitive: bool, pattern: Box<Spanned<Expr>>, escape: Option<Box<Spanned<Expr>>> },
    IsNull { expr: Box<Spanned<Expr>>, negated: bool },
    IsDistinctFrom { left: Box<Spanned<Expr>>, right: Box<Spanned<Expr>>, negated: bool },
    Function(FunctionCall),                   // name, args, DISTINCT, FILTER (WHERE ...), OVER (M10)
}

pub fn parse(text: &str, limits: &Limits) -> Result<Vec<Spanned<Statement>>, ParseError>;
pub fn parse_one(text: &str, limits: &Limits) -> Result<Spanned<Statement>, ParseError>;
```

## Rules

- **Known but unsupported is `0A000`** (ADR-0011): the parser recognizes
  the start of every PostgreSQL statement and clause we do not support yet
  (for example `CREATE FUNCTION`, `LATERAL`, `GROUPING SETS`, `FOR
  UPDATE`), and reports the feature by name with its span. Text that is
  not SQL at all is `42601`. Nothing is skipped or ignored.
- **Errors point at the problem:** `42601` carries the span of the
  offending token and the list of what was expected ("expected `FROM`, `,`
  or end of statement"), and the facade turns the span into PostgreSQL's
  `position` field.
- The parser builds the AST only; name resolution, typing and semantic
  checks (aggregates in `WHERE`, `GROUP BY` rules) are the binder's
  (`planner.md`).
- The printer emits canonical SQL: keywords upper case, identifiers quoted
  only when needed, every expression parenthesized where precedence would
  otherwise change it.

## Invariants

- `parse(print(ast)) == ast`, ignoring spans, for every AST the parser can
  produce.
- The parser never panics, never recurses deeper than the depth limit, and
  never allocates more than a small multiple of its input.
- Spans always lie within the input and on character boundaries.

## Limits

| Limit | Default | When exceeded |
| :- | :- | :- |
| Statement text | 16 MiB (`max_statement_bytes`, ADR-0011) | `54001` |
| Expression and query nesting depth | 256 | `54001` |
| Parameters | `$65535` | `42601` |
| Identifier length | 63 bytes | Truncated with a notice |
| Items in an `IN` list or `VALUES` | 100,000 | `54001` |

## Tests

- Unit tests per statement and clause: accepted text and its AST, rejected
  text and its error code, message and span.
- Property tests: print/parse round-trip over generated ASTs; the lexer
  re-lexes the concatenation of printed tokens to the same tokens.
- A corpus of PostgreSQL syntax we do not support, each expected to give
  `0A000` with the right feature name.
- Fuzz target `sql`: arbitrary bytes (as UTF-8 where valid) through `parse`;
  any successful parse must round-trip through `print`.

## Open questions

- Keyword status: copy PostgreSQL's reserved/non-reserved split exactly
  (so the same identifiers need quoting), or reserve fewer? Default: copy
  it for the keywords we know.
- Whether `split` should live here (it does) and be re-exported by the
  facade for the CLI, which may not depend on `hardshell-sql` (dependency
  matrix).
