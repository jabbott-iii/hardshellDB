# Subsystem: `hardshell-wire` and `hardshell-server` (`hardshelld`)

The network face of the engine (ADR-0003, ADR-0016). `hardshell-wire` is a
byte-level codec for the PostgreSQL frontend/backend protocol 3.0 (layer 1,
`core` only). `hardshell-server` is the `hardshelld` binary: listeners, a
bounded thread pool, sessions, and the mapping between protocol messages
and the facade's public API. Nothing is built yet; M12 builds both, and its
first task decides TLS and authentication with the user.

## Contents

| Crate / module | Provides |
| :- | :- |
| `wire::frame` | Length-prefixed message framing with a size limit; the untyped startup packet |
| `wire::frontend` | Decoding client messages: `StartupMessage`, `SSLRequest`, `CancelRequest`, `Query`, `Parse`, `Bind`, `Describe`, `Execute`, `Sync`, `Close`, `Flush`, `Terminate`, `PasswordMessage` |
| `wire::backend` | Encoding server messages: `AuthenticationOk` (and later SASL), `ParameterStatus`, `BackendKeyData`, `ReadyForQuery`, `RowDescription`, `DataRow`, `CommandComplete`, `EmptyQueryResponse`, `ParseComplete`, `BindComplete`, `ParameterDescription`, `NoData`, `ErrorResponse`, `NoticeResponse` |
| `server::listen` | TCP on loopback and Unix-domain sockets (Unix only; `std` has no Unix sockets on Windows) |
| `server::pool` | Reusable connection threads, capped at `max_connections` |
| `server::session` | One client's state: its `Connection`, named statements and portals, transaction status |
| `server::types` | Value ↔ text format; value ↔ binary format for the supported types; PostgreSQL type OIDs |
| `server::config` | Command-line and file configuration: database path, listen addresses, port, `max_connections` |

## Key types

```rust
// hardshell-wire
pub enum Frontend<'a> {
    Startup { protocol: u32, params: Vec<(&'a str, &'a str)> },  // 196608 = 3.0
    SslRequest,                                                  // code 80877103
    CancelRequest { pid: i32, key: i32 },                        // code 80877102
    Query(&'a str),
    Parse { name: &'a str, sql: &'a str, param_oids: Vec<u32> },
    Bind { portal: &'a str, stmt: &'a str, formats: Vec<Format>, params: Vec<Option<&'a [u8]>>, result_formats: Vec<Format> },
    Describe { kind: DescribeKind, name: &'a str },
    Execute { portal: &'a str, max_rows: i32 },
    Sync, Flush, Close { kind: DescribeKind, name: &'a str }, Terminate,
}

pub fn decode_frontend(buf: &[u8], phase: Phase, limits: &Limits) -> Result<(Frontend<'_>, usize), WireError>;
pub fn encode_backend(msg: &Backend<'_>, out: &mut Vec<u8>);

// hardshell-server
pub struct Session {
    conn: hardshelldb::Connection,
    statements: FastHashMap<String, PreparedStmt>,
    portals: FastHashMap<String, Portal>,
    status: TxnStatus,                                 // ReadyForQuery: 'I', 'T' or 'E'
    cancel: (i32, i32, InterruptHandle),
}
```

## Protocol behavior

- **Startup:** protocol 3.0 only; other versions get `ErrorResponse`
  `08P01` (`protocol_violation`) and the connection closes. `SSLRequest` is
  answered `N` until TLS exists. The `database` parameter must match the
  served database's configured name (`3D000`, `invalid_catalog_name`,
  otherwise); `user` is recorded for logs.
- **Authentication:** until M12's first task decides it, the server sends
  `AuthenticationOk` without a password on loopback and Unix sockets only,
  refuses to start unless given `--insecure-local-trust`, logs a warning
  at start, and refuses to bind any other address (ADR-0016).
- **Simple query** runs each statement of the text in turn, as `psql`
  expects; an error skips the rest and still ends with `ReadyForQuery`.
- **Extended query:** named and unnamed statements and portals; `Execute`
  with `max_rows` suspends a portal (`PortalSuspended`); after an error the
  server discards messages until `Sync`, as the protocol requires.
- **Errors** carry severity, the SQLSTATE, message, detail, hint and
  position from the facade's `Error`.
- **Cancel:** `BackendKeyData` gives each session a process id and secret
  key; a `CancelRequest` on a new connection with a matching pair calls that
  session's `InterruptHandle` (`57014` for the client). Keys must be
  unguessable (`os-and-vfs.md`, open questions on randomness).
- **Values:** text format for every supported type; binary format for
  parameters and results of the fixed-width types and `TEXT`/`BYTEA`, others
  text-only until needed. Type OIDs come from `types-and-values.md`.

## Server rules

- One `hardshelld` process serves one database file and holds its lock
  (ADR-0005); that is how several processes share a database.
- One thread per open connection (ADR-0010), from a pool capped at
  `max_connections` (default 64); threads are reused after a connection
  closes. Connections past the cap are refused with `53300`
  (`too_many_connections`). Blocking `std` sockets cannot multiplex idle
  connections onto fewer threads without OS polling calls (FFI), which is
  ADR-0010's revisit trigger.
- Idle sessions in a transaction are ended after
  `idle_in_transaction_session_timeout` (default off; when set, `25P03`).
- Shutdown on SIGTERM/Ctrl-C (needs the same signal-handling ADR as the
  CLI): stop accepting, send `57P01` (`admin_shutdown`) to idle sessions,
  wait for running statements up to a timeout, close the database cleanly.
- Every byte from a socket is hostile: frame lengths are checked against the
  limit before any allocation, and a malformed message is `08P01` and closes
  the connection, never a panic.

## Invariants

- The server uses only the facade's public API and `hardshell-wire`; it
  never reaches into the engine.
- The protocol state machine never sends `ReadyForQuery` with a status that
  disagrees with the session's transaction state.
- `hardshell-wire` decoding never panics and never allocates more than the
  declared message length, which is checked first.

## Limits

| Limit | Default | When exceeded |
| :- | :- | :- |
| Message length | 64 MiB (startup packet: 10,000 bytes) | `08P01`, connection closed |
| `max_connections` | 64 | `53300` |
| Named statements per session | 1,000 | `54000` |
| Port | Open question (must not collide silently with PostgreSQL's 5432) | — |

## Tests

- Codec round-trip and property tests for every message; known-good byte
  sequences captured from the protocol documentation's examples.
- Fuzz target `wire`: arbitrary bytes through `decode_frontend` in every
  phase.
- Integration tests: a test client written against `hardshell-wire` drives
  simple and extended queries, errors, cancel, and limits.
- Compatibility smoke test (manual or a dev-only CI job): `psql` connects,
  runs the SQL logic corpus's statements, and `\d` works.

## Open questions

- TLS and SCRAM-SHA-256: build, adopt behind the dependency policy's
  criterion 2, or keep the server local-only (ADR-0016; M12's first task).
- Which `pg_catalog` views and functions drivers need at connect time
  (`catalog.md`, open questions), and which drivers to test.
- Protocol 3.2 (PostgreSQL 18's longer cancel keys): out of scope until a
  driver needs it.
- Windows has no Unix-socket listener through `std`; loopback TCP only.
