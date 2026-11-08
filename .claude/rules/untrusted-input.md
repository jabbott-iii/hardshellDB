---
paths:
  - "crates/sql/**"
  - "crates/storage/**"
  - "crates/types/**"
  - "crates/wire/**"
  - "crates/core/src/csv*"
  - "crates/core/src/varint*"
  - "apps/server/**"
  - "fuzz/**"
---

# Rules for parsers and decoders

Every byte we read may be malformed or hostile: SQL from an application's
users, a database file someone sends you, a damaged WAL, a CSV export, a
client on the wire.

- **Never panic on input.**
  - No indexing or slicing that can go out of bounds on input-derived
    offsets; use `get`, or check first.
  - No `unwrap`/`expect` on input-derived values.
  - No unchecked arithmetic on input lengths, counts or offsets: use
    `checked_add`/`checked_mul` and return an error on overflow.
- **Check before allocating.** Compare every claimed size against a limit
  and against the remaining input before `Vec::with_capacity`, `resize` or
  a loop that allocates per item.
- **Bound recursion** (expressions, subqueries, B+tree depth, nested
  comments) by an explicit depth limit. Exceeding it is an error
  (`54001` for SQL), never a stack overflow.
- **Bound loops over on-disk links** (overflow chains, free-list chains,
  tree descents) by the page count, so a cycle in a damaged file is an
  error, not a hang.
- **Errors carry context and a SQLSTATE:**
  - SQL: the byte offset of the problem (`position`), `42601` for syntax,
    `0A000` for known-but-unsupported features, `54001` for limits.
  - Files: the page number and structure, `XX001` (`data_corrupted`) or
    `XX002` (`index_corrupted`).
  - Text input for types: `22P02` or the type's own code, with the input.
  - Wire: `08P01` (`protocol_violation`), then close the connection.
- **Every new parser or decoder:**
  1. registers a `cargo xtask fuzz-smoke` target in `TARGETS`
     (`xtask/src/fuzz_targets.rs`) with a seed corpus in
     `fuzz/corpus/<target>/` (target names in `docs/04-quality.md`)
  2. has hand-made test vectors beside its tests, including truncated and
     oversized inputs
  3. documents its limits and their defaults in its subsystem spec
- **Limits are configuration, not constants buried in code.** Each has a
  named default in its crate's limits struct, and a test that exceeding it
  gives the documented error.
- **Fuzz before reporting done:** run the target for at least 60 s and
  report the result.
