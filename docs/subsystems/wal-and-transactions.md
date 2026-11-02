# Subsystem: `hardshell-storage` — WAL, transactions and recovery

How `hardshell-storage` makes writes durable and gives readers consistent
snapshots: the write-ahead log (ADR-0007), the single write lock and
snapshot reads (ADR-0008), checksums and poisoning (ADR-0009), checkpoints
and recovery. The page format and B+trees are in `storage.md`. Nothing is
built yet; M2 builds all of it except savepoints (M10) and group commit
(M9).

## Contents

| Module | Provides |
| :- | :- |
| `wal::file` | The WAL file format: header, frames, the chained checksum |
| `wal::index` | In-memory map from page to its committed frames, per generation; the last commit frame |
| `txn` | `ReadTxn`, `WriteTxn`, `Snapshot`, the write lock, reader marks |
| `checkpoint` | Passive checkpoints, the close-time truncating checkpoint, the auto-checkpoint threshold |
| `recovery` | WAL scan and validation on open |
| `savepoint` | Savepoint stack for a write transaction (M10) |

## WAL file format

`<name>-wal` beside the database. Little-endian.

**Header (32 bytes):**

| Offset | Size | Field |
| :- | :- | :- |
| 0 | 8 | Magic `b"hsWAL\0\0\x01"` |
| 8 | 4 | Database page size |
| 12 | 4 | Generation (`salt1`): +1 at every WAL restart |
| 16 | 4 | `salt2`: differs per restart (from `Pcg32`; see `os-and-vfs.md` open questions) |
| 20 | 4 | Checkpoint sequence |
| 24 | 4 | Reserved |
| 28 | 4 | CRC-32C of bytes 0..28 |

**Frame:** a 24-byte header followed by one page image.

| Offset | Size | Field |
| :- | :- | :- |
| 0 | 4 | Page number |
| 4 | 4 | Commit marker: page count after commit, or 0 for a non-commit frame |
| 8 | 4 | `salt1` (copy of the header's) |
| 12 | 4 | `salt2` (copy of the header's) |
| 16 | 4 | Reserved |
| 20 | 4 | Chained checksum: CRC-32C continued from the previous frame's checksum (the header's CRC for frame 1) over bytes 0..20 of this header and the page image |

A frame is valid when its salts match the header and its chained checksum
matches. Validity ends at the first invalid frame, so a torn tail, a frame
from an older generation, or garbage past the end all stop the scan.

## Key types

```rust
pub struct Snapshot {
    pub generation: u32,
    pub commit_frame: u64,        // last committed frame visible; 0 = WAL ignored
    pub page_count: u32,
    pub schema_version: u64,
}

pub struct ReadTxn<'db> { db: &'db Shared, snap: Snapshot, mark: ReaderMark }   // mark released on drop

pub struct WriteTxn<'db> {
    db: &'db Shared,
    snap: Snapshot,               // always the latest commit when the lock was taken
    lock: WriteLockGuard<'db>,
    dirty: FastHashMap<PageNo, DirtyPage>,
    spilled_upto: u64,            // uncommitted frames already in the WAL
}

pub trait ReadView {               // both txn kinds implement it; B+tree cursors take it
    fn page(&self, no: PageNo) -> Result<PageRef<'_>, StorageError>;
    fn snapshot(&self) -> &Snapshot;
}

pub enum Begin { Deferred, Immediate }   // Immediate takes the write lock at BEGIN
```

## Transactions (ADR-0008)

- **Read:** a transaction starts with a snapshot of the latest commit and a
  reader mark. It reads each page from the newest WAL frame at or before its
  `commit_frame`, or from the main file.
- **Write:** the first write takes the write lock, waiting up to
  `busy_timeout` (`55P03` after). If a commit happened since the
  transaction's snapshot, the upgrade fails with `40001`; `Begin::Immediate`
  avoids this by taking the lock at `BEGIN`.
- **DDL is transactional:** the catalog lives in B+trees, so `CREATE TABLE`
  inside `BEGIN` rolls back with everything else.
- **Errors in an explicit transaction:** the facade puts the transaction in
  the failed state (`25P02` until `ROLLBACK`), as PostgreSQL does, so the
  storage layer needs no statement-level undo.

## Commit, in order

1. Write a frame for each dirty page not yet in the WAL, or changed since it
   was spilled. The last frame carries the commit marker.
   `crash_point!("commit.frames_written")`.
2. With `Durability::Full`, `sync_data` the WAL.
   `crash_point!("commit.synced")`. With `Relaxed`, skip.
3. Publish: under the WAL index's write lock, record the new frames and
   advance the last commit frame. New snapshots now see the commit.
4. Release the write lock.
5. If the WAL passed `wal_autocheckpoint` (default 16 MiB) and no
   background checkpointer is configured, run a passive checkpoint.

A failure in step 1 (for example `53100`, `disk_full`) fails the
transaction and leaves the WAL index unchanged; the next writer overwrites
the invalid tail. A failure in step 2 poisons the `Database` (ADR-0009):
every later write is `58030` until it is reopened.

## Checkpoint

1. Target = the oldest live snapshot's `commit_frame` (or the last commit
   if there are no readers). A reader in an older generation blocks the
   checkpoint entirely.
2. For each page with frames at or before the target, copy its newest such
   frame into the main file. `crash_point!("checkpoint.page_copied")`.
3. Grow or shrink the file to the target's page count; `sync_all` it.
   `crash_point!("checkpoint.synced")`.
4. Drop cached `Main` versions of the copied pages.
5. If the target was the last commit and no reader uses WAL frames, the next
   writer restarts the WAL: generation + 1, new `salt2`, frames from the
   start. The file is reused, not truncated, until close.

`Database::close` runs a full checkpoint, truncates and deletes the WAL,
and syncs the directory, so a cleanly closed database is one file.

## Recovery on open

1. No WAL, or a WAL shorter than its header, or a header whose checksum
   fails: the main file is complete (a WAL restart only happens after a
   synced checkpoint). Start a new WAL.
2. Otherwise scan frames from the start, validating salts and the chained
   checksum; the last valid commit marker ends the recovered log.
3. Rebuild the WAL index from the valid frames, then (unless `read_only`)
   checkpoint, sync, and continue.
4. `read_only` with a non-empty WAL fails with `55000`
   (`object_not_in_prerequisite_state`) and a hint to open read-write once.

Recovery is idempotent: a crash during recovery's checkpoint is recovered
by running it again.

## Savepoints (M10)

`SAVEPOINT s` records the dirty-page map's state; the first change to a page
after a savepoint keeps that page's prior version (from the cache, or a
reference to its spilled frame). `ROLLBACK TO s` restores them;
`RELEASE s` discards them. Unknown names are `3B001`
(`invalid_savepoint_specification`). Spilled frames that a rollback
abandons stay in the WAL as uncommitted frames and are overwritten.

## Invariants

- **Committed data is always recoverable:** a committed page version is in
  a valid WAL frame, or in the synced main file, or both.
- **Uncommitted data never reaches the main file:** checkpoints copy only
  frames at or before a commit marker.
- **The main file never holds a page version newer than the oldest live
  snapshot.** This is why a checkpoint stops at the oldest reader, and why
  cached `Main` versions it overwrites are never needed again.
- The WAL is restarted only after the main file holds every committed
  frame and is synced.
- At most one `WriteTxn` exists per `Database` at any time.
- Lock order (from `docs/01-architecture.md`): write lock → schema cache →
  WAL index → reader marks → buffer pool shard. A debug-build checker
  asserts it.

## Limits

| Limit | Default | When exceeded |
| :- | :- | :- |
| `busy_timeout` | 5 s | `55P03` |
| `wal_autocheckpoint` | 16 MiB | Checkpoint runs; if blocked by a reader for more than 4× this size, a warning is logged per checkpoint attempt |
| Frames per WAL generation | 2^63 | Cannot be reached in practice |

## Tests

- **The crash campaigns** (`cargo xtask crash`): page-level transactions
  in `crates/storage/tests/crash.rs` (M2) and random SQL workloads in the
  facade's `crash` test (M6), both on `FaultVfs`; crash after op *N* for
  every *N* in small workloads and a sample in large ones; take each
  `CrashMode` image; reopen; check that the database equals the state of
  the last transaction whose commit returned, or of the in-flight one if
  its sync completed, and that the tree checker (from M11,
  `CHECK DATABASE`) passes.
- **Process-kill test:** a child process commits a numbered sequence;
  the parent kills it at random times, reopens, and checks a prefix of the
  sequence is present with no gaps.
- **Fault tests:** injected write errors, `ENOSPC` and sync failures give
  the documented errors, and a sync failure poisons the handle.
- **Concurrency tests:** readers on old snapshots see exactly their
  snapshot while a writer commits and checkpoints run.
- Fuzz target `wal`: recovery over arbitrary WAL bytes never panics and
  never accepts a frame with a bad checksum.

## Open questions

- **Group commit (M9):** releasing the write lock before the sync lets the
  next writer start, but readers could then observe a commit that a crash
  later loses. Options: publish only after the sync (simpler), or allow it
  with `Durability::Relaxed` only.
- SQL syntax for `Begin::Immediate`: PostgreSQL has no `BEGIN IMMEDIATE`.
  Candidate: `BEGIN READ WRITE` takes the lock at once. Decide in M6.
- A background checkpointer's wake-up policy (time, size, or both).
- Recovery step 1 cannot tell a torn restart header from a corrupted header
  in front of committed frames. Proposal: sync the new header before the
  first frame of a generation, and record "WAL generation G may hold
  commits" in a main-file header field written at checkpoint, so a bad
  header over a generation the main file has not absorbed is `XX001`
  instead of an empty WAL.
