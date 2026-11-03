# Subsystem: `hardshell-os`

The only crate that touches the file system, file locks, syncs and the
clock. Everything else does I/O through the `Vfs` trait, so tests can swap
in an in-memory file system or one that injects faults and simulates power
loss. Nothing is built yet; M1 builds `StdVfs`, `MemVfs` and `FaultVfs`.
The per-OS rules this crate implements are in `docs/03-platforms.md`.

`unsafe` is denied here as everywhere; this is the one crate that may opt
in, and only through a new ADR naming the OS call `std` lacks (ADR-0013).
(Outside it, only a test binary's counting allocator may, ADR-0017.)
At M1 everything is plain `std`: positional I/O (`FileExt`), `sync_all` /
`sync_data`, and `File::lock` / `try_lock` (stable since Rust 1.89).

## Contents

| Module | Provides |
| :- | :- |
| `vfs` | The `Vfs` and `VfsFile` traits, `OpenFlags`, `LockKind`, `OsError` |
| `std_vfs` | `StdVfs`: real files through `std::fs`; the only module with `#[cfg(unix)]` / `#[cfg(windows)]` branches |
| `mem_vfs` | `MemVfs`: files in memory, for unit tests and the CLI's in-memory database |
| `fault_vfs` | `FaultVfs` (feature `testing`): wraps `MemVfs`, records every write and sync, injects errors, and produces the file images a power loss could leave |
| `clock` | `Clock` trait (`now_utc_micros`, `monotonic`), `SystemClock`, and `FixedClock` for tests |
| `sys` | Empty. Raw OS declarations go here if an ADR approves one (ADR-0013) |

## Key types

```rust
pub trait Vfs: Send + Sync {
    fn open(&self, path: &Path, flags: OpenFlags) -> Result<Box<dyn VfsFile>, OsError>;
    fn delete(&self, path: &Path) -> Result<(), OsError>;
    fn exists(&self, path: &Path) -> Result<bool, OsError>;
    fn sync_dir(&self, dir: &Path) -> Result<(), OsError>;   // no-op where the OS needs none (03-platforms)
    fn temp_file(&self, dir: &Path) -> Result<Box<dyn VfsFile>, OsError>; // deleted on drop
    fn canonical(&self, path: &Path) -> Result<PathBuf, OsError>;
}

pub trait VfsFile: Send + Sync {
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> Result<(), OsError>; // short read => OsError::Eof
    fn write_all_at(&self, buf: &[u8], offset: u64) -> Result<(), OsError>;
    fn sync_data(&self) -> Result<(), OsError>;   // data + size; WAL commits use this
    fn sync_all(&self) -> Result<(), OsError>;    // data + all metadata; file creation, checkpoints
    fn len(&self) -> Result<u64, OsError>;
    fn set_len(&self, len: u64) -> Result<(), OsError>;
    fn try_lock(&self, kind: LockKind) -> Result<bool, OsError>;  // false = held by someone else
    fn unlock(&self) -> Result<(), OsError>;
}

pub enum LockKind { Shared, Exclusive }

pub struct OpenFlags { pub create: bool, pub read_only: bool, pub truncate: bool }

pub enum OsError {
    Eof { offset: u64 },
    NotFound(PathBuf),
    PermissionDenied(PathBuf),
    DiskFull,                                 // -> 53100 disk_full
    Io { op: &'static str, path: PathBuf, source: std::io::Error },   // -> 58030
}
```

`FaultVfs` (feature `testing`):

```rust
pub struct FaultVfs { inner: MemVfs, log: Mutex<IoLog>, plan: Mutex<FaultPlan> }

pub struct FaultPlan {
    pub crash_after_ops: Option<u64>,        // every op after the Nth fails as if the machine stopped
    pub fail_op: Option<(u64, FaultKind)>,   // make the Nth op fail once
    pub seed: u64,                           // drives which unsynced writes survive
}

pub enum FaultKind { WriteError, DiskFull, SyncError, ShortWrite }

impl FaultVfs {
    pub fn op_count(&self) -> u64;
    /// File images after a power loss now: synced bytes survive; each unsynced
    /// write survives, is lost, or (at 512-byte sector granularity) tears,
    /// chosen by the seed.
    pub fn crash_image(&self, mode: CrashMode) -> MemVfs;
}

pub enum CrashMode { DropAllUnsynced, KeepAllUnsynced, Random, RandomTorn }
```

## Invariants

- No other crate calls `std::fs`, `File::lock` or a sync function. `cargo
  xtask layers` cannot see this, so review does, and a grep check in
  `xtask unsafe-audit`'s scan flags `std::fs::` outside `hardshell-os`,
  `xtask` and the apps.
- `read_exact_at` and `write_all_at` loop over partial reads and writes;
  a caller never sees a short operation as success.
- `StdVfs` never uses the file's cursor, so concurrent positional I/O on one
  handle from several threads is sound on every OS (Windows `seek_read`
  moves the cursor, which we never read).
- A sync error is returned as it happened; `hardshell-os` never retries a
  sync. The storage layer then poisons the `Database` (ADR-0009).
- `FaultVfs` is deterministic: the same plan, seed and sequence of calls
  produce the same images.
- `FaultVfs::crash_image` never invents bytes: every byte of an image was
  either synced, written by some call, or the file's previous content.

## Locking

- The main database file is locked with `try_lock(Exclusive)` on open
  (ADR-0005), or `Shared` when `read_only`. A held lock is `55006`.
- Two `Database::open` calls on the same file in one process must also
  conflict. Whether `std`'s lock gives that depends on the OS lock
  primitive it uses (open-file-description locks conflict within a
  process; POSIX record locks do not): **verify** on each OS in M1, and if
  not, keep a process-wide set of canonical paths of open databases (see
  open questions).

## Limits

| Limit | Default | When exceeded |
| :- | :- | :- |
| Path length | What the OS allows | `OsError::Io` → `58030` |
| Temp files per query | Bounded by `work_mem` spilling (M9) | — |

## Tests

- `StdVfs` and `MemVfs` pass the same conformance suite (one generic test
  module run against both): positional reads and writes, EOF behavior,
  `set_len` growth and shrink, lock conflicts between two handles.
- `FaultVfs`: property tests that crash images contain only legal bytes
  (invariant above), and that `DropAllUnsynced` equals the last synced state.
- The crash campaign (`docs/04-quality.md`, M2) is this crate's main
  consumer: it runs SQL workloads on `FaultVfs`, crashes at every op count
  or a sample of them, and checks recovery.
- On each OS in CI: lock conflict between two processes (a test spawns a
  child process), and directory sync after create.

## Open questions

- **Random bytes.** `gen_random_uuid()` (M10), the WAL's salts and the
  server's cancel keys (M12) want OS randomness. Unix can read
  `/dev/urandom` with `std::fs`; Windows has no `std` API for it
  (`BCryptGenRandom` is FFI, so an ADR under ADR-0013). WAL salts only
  need to differ between WAL generations, so `Pcg32` seeded from the clock
  and process id is enough for them; cancel keys must be unguessable.
- **Same-file guard in one process.** A process-wide registry is global
  state, which the architecture avoids (principle 7). Decide in M1 after
  verifying the lock behavior above.
- **Device atomic write size** (to skip torn-page handling) and `O_DIRECT`
  are backlog items, each needing an ADR.
