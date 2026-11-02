---
name: change-on-disk-format
description: Use whenever a change would alter bytes hardshellDB writes to disk (file header, page layouts, B+tree cells, free list, WAL header or frames, key or row encoding, system-table rows). Bumps the format version or adds a feature flag, updates the spec, adds fixtures and crash coverage.
argument-hint: "<structure> <change>"
---

# Change an on-disk structure

Change: `$ARGUMENTS`

Users' data outlives our code. A format change that is not versioned,
specified and tested can make existing databases unreadable, or worse,
misread. Follow every step.

## 1. Confirm it is a format change, and that it is needed

- It is a format change if any existing file, WAL or key/row encoding
  would get different bytes, or if new code would read existing bytes
  differently. Changing the key encoding of any value is one: index order
  depends on it.
- Ask whether the need can be met without changing the format (a new
  optional system-table column, a reserved field already present).
- If the change also touches durability (write ordering, syncs, the WAL),
  read `.claude/rules/storage-and-durability.md` and plan the crash points.

## 2. Decide the compatibility path

Read `docs/05-release.md` ("File format version").

- **Before 1.0 (format major 0):** bump the format minor version.
  Opening a file of another version fails with the clear "dump and restore
  with a 0.x CLI" error. No migration.
- **After 1.0:** do not bump the major format. Add a feature flag:
  - **compatible** if older 1.x can ignore the feature entirely and still
    read and write the file correctly;
  - **read-only-compatible** if older 1.x can still read the file correctly
    (it must refuse to write it);
  - **incompatible** if older 1.x would misread it (it must refuse to open
    it, naming the flag).
  New files or an explicit user action turn the flag on; existing files
  are never upgraded silently.
- If you are unsure which applies, stop and ask the user. Record the
  decision with `/record-decision`.

## 3. Specify first

- Update the byte-level description in `docs/subsystems/storage.md`,
  `docs/subsystems/wal-and-transactions.md` or
  `docs/subsystems/types-and-values.md` (from M11.1, `docs/file-format.md`)
  before changing code: offsets, widths, endianness, limits, and what a
  reader must reject.

## 4. Implement

- Writers and readers change together; readers validate every new field
  (`.claude/rules/untrusted-input.md`).
- Old-version and wrong-flag files produce the documented errors, never a
  panic or a misread.
- Add `crash_point!`s at any new durability boundary.

## 5. Test

- **Fixtures:** add a committed file to `tests/fixtures/format-v<N>/` with
  its expected contents, and a test that opens it, checks it with the tree
  checker (later `CHECK DATABASE`) and compares contents. After 1.0, keep
  every older fixture and its test.
- **Rejection tests:** a file with the old version (before 1.0) or an
  unknown incompatible flag (after 1.0) gives the documented error.
- **Fuzz:** update the seeds of the affected targets (`page`, `wal`,
  `open`, `key`, `row`) and run each for at least 60 s.
- **Crash campaign:** run `cargo xtask crash` with at least 100,000
  iterations; report the seed.
- **Encodings:** the property test that `memcmp` order equals SQL order
  must still pass for every type.

## 6. Report

- The structure changed, old and new layouts, and the compatibility path
  (version bump or flag, with the ADR number).
- Fixture files added.
- Commands run with results (tests, fuzz seconds, campaign iterations and
  seed).
- Anything a user must do (dump and restore before 1.0).
