---
paths:
  - "crates/os/**"
  - "crates/storage/**"
  - "crates/types/src/encoding/**"
  - "tests/fixtures/**"
---

# Rules for storage, durability and on-disk formats

A bug here loses or corrupts users' data, and tests rarely show it. Work
from the invariants, then prove them with the crash campaign.

- **The invariants (ADR-0007, ADR-0008, ADR-0009):**
  - The main file holds only data that is committed in the WAL or was
    checkpointed. Uncommitted pages never reach it.
  - A commit is acknowledged only after the WAL sync that covers its commit
    frame returns success (`durability = Full`).
  - The WAL restarts only after the main file has been synced with
    everything it held.
  - A reader's snapshot never changes while the reader holds it.
  - At most one write transaction at a time.
- **Never retry a failed sync.** Poison the `Database` (`58030`) and require
  a reopen. Never swallow an I/O error; a sync error is never logged and
  ignored.
- **All file access goes through the `Vfs`.** No `std::fs` outside
  `hardshell-os`. Tests use `MemVfs` and `FaultVfs`.
- **Mark durability boundaries** with `crash_point!("area.event")` (feature
  `testing`) at every new write or sync, so the crash campaign reaches them.
- **Read paths verify before use:** checksum first, then every length,
  offset, page number and count against the page size, the page count and
  the remaining bytes. A failure is `XX001` with the page number, never a
  panic (`.claude/rules/untrusted-input.md`).
- **Lock order** is the one in `docs/01-architecture.md` ("Shared state and
  threading"). Taking two locks in another order is a bug even if no test
  deadlocks.
- **Format changes:** any change to the header, a page layout, the WAL
  frame, or the key or row encoding goes through `/change-on-disk-format`.
  It bumps the version (or adds a feature flag after 1.0), updates the spec,
  and adds a fixture test.
- **Encodings are forever:** the key encoding's `memcmp` order must equal
  SQL order for every type (`docs/subsystems/types-and-values.md`). A change
  that alters any existing key's bytes is a format change.
- **Before reporting done:**
  - run `cargo xtask crash` (both levels once they exist) and report the
    iteration count and seed
  - for changes to sync or locking, run the process-kill test
  - walk the "Storage change review checklist" in `docs/04-quality.md` and
    answer each item in the report
