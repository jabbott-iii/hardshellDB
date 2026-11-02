# 03 — Platforms

What the engine needs from each OS, and the rules it follows to get it.
Everything here goes through `std` and `hardshell-os` (ADR-0013). Lines
marked **verify (M1.2)** are what we believe, not yet checked against the
pinned toolchain's `std` source and a test on that OS; M1.2 replaces each
with the checked fact and its source.

## Targets

| OS | Architectures | Minimum | CI runner | Notes |
| :- | :- | :- | :- | :- |
| Linux | x86_64, aarch64 | The glibc of the release build image, recorded at M11.6 (Rust's `std` itself needs 2.17 or newer) | `ubuntu-latest` | aarch64 tested from the release pipeline (M11.6) |
| macOS | aarch64 (Apple silicon), x86_64 | macOS 12 | `macos-latest` | |
| Windows | x86_64, aarch64 (MSVC) | Windows 10 | `windows-latest` | aarch64 tested from the release pipeline (M11.6) |

Only 64-bit little-endian targets (ADR-0001). The file format writes
fixed-width little-endian integers anyway, so files move between targets.

## Development order

Linux first in every milestone: CI's fastest runner and where agents run.
Then macOS, then Windows. A task whose code is done but whose check needs
another OS stays unticked with a `⏳ needs <OS>` note (`docs/roadmap.md`).

## What the engine needs from the OS

| Need | Why | Linux | macOS | Windows |
| :- | :- | :- | :- | :- |
| Positional read and write | Many threads read pages from one handle without sharing a cursor | `std::os::unix::fs::FileExt::read_at`/`write_at` (`pread`/`pwrite`) | Same as Linux | `std::os::windows::fs::FileExt::seek_read`/`seek_write` (they also move the handle's cursor, which we never use) |
| Durable data sync | Commit (WAL) and checkpoint (main file) | `File::sync_data` → `fdatasync`, which also persists a size change | `File::sync_data`/`sync_all`; whether `std` uses `F_FULLFSYNC`: **verify (M1.2)**. Plain `fsync` on macOS does not flush the drive's cache | `File::sync_all`/`sync_data` → `FlushFileBuffers` **verify (M1.2)** |
| Directory sync | A newly created database or WAL file must survive a crash | Open the directory with `File::open` and `sync_all` | As Linux; whether `F_FULLFSYNC` on a directory handle is accepted: **verify (M1.2)** | Opening a directory needs `FILE_FLAG_BACKUP_SEMANTICS` through `OpenOptionsExt::custom_flags`; whether `FlushFileBuffers` on it is supported: **verify (M1.2)**. Until verified, best effort, and the docs say so |
| Exclusive lock on the database file | One process per database (ADR-0005) | `File::try_lock` (stable since Rust 1.89); which syscall `std` uses (`flock` or `fcntl`): **verify (M1.2)** | As Linux | `File::try_lock` → `LockFileEx` **verify (M1.2)**. Windows locks are mandatory: other handles cannot read the locked range, so the engine opens each file once and shares the handle |
| Shared lock | `read_only` opens (several readers, no writer) | `File::try_lock_shared` | As Linux | As Linux |
| Truncate and extend | WAL reset, free-page return (`VACUUM`) | `File::set_len` | As Linux | As Linux |
| Delete | Removing the WAL on clean close | `std::fs::remove_file` | As Linux | May fail while another program (antivirus, indexer) holds a handle: retry with backoff for up to 1 s, then leave it (see below) |
| Temporary files | Spilling sorts and hash tables (M9) | Created in `temp_dir` through the VFS; deleted when done and on drop | As Linux | As Linux |

## Rules that follow

- **Never retry a failed sync.** After a failed `fsync`, Linux may have
  dropped the dirty pages it failed to write and report later syncs as
  successful. Any sync failure poisons the `Database` until it is reopened
  and recovered from the WAL (ADR-0009).
- **Order of operations on create:** write the header page, sync the file,
  sync the directory, then take the lock and report success. Same for the
  WAL file the first time it is created.
- **Clean close:** full checkpoint; sync the main file; truncate the WAL to
  zero length and sync it; then delete it. A leftover empty WAL (a failed
  delete on Windows) is harmless: the next open finds nothing to recover.
- **One handle per file per `Database`,** shared by all connections. This
  avoids Windows mandatory-lock conflicts with ourselves and keeps the lock
  owner obvious.
- **Write errors are not sync errors.** A failed `write_at` to the WAL
  (for example `ENOSPC`) fails the transaction with `53100` (`disk_full`)
  or `58030` (`io_error`) and rolls it back; frames after the last commit
  are ignored by readers and recovery. Only a failed sync poisons.
- **No `mmap`, no `O_DIRECT`.** Reads and writes go through the OS page
  cache; the buffer pool caches on top. Both are backlog items needing an
  ADR (ADR-0013).
- **Torn writes:** we assume 512-byte sector writes are atomic and page
  writes are not. A torn page in the main file is repaired from the WAL
  (ADR-0007); a torn WAL frame is detected by its checksum and ignored.

## Filesystems

| Kind | Support |
| :- | :- |
| ext4, XFS, Btrfs (Linux); APFS (macOS); NTFS, ReFS (Windows) | Supported |
| tmpfs, ramdisks | Works; not durable across reboots, by nature |
| NFS, SMB/CIFS, other network filesystems | **Not supported**: locks and sync semantics are unreliable. `Database::open` warns in the log if it can tell (verify how in M1.2), and the docs say so |
| Folders synced by cloud clients (Dropbox, OneDrive, iCloud Drive) | Not supported: the client may copy a database and its WAL at different moments |
| FAT32/exFAT | Not supported: no reliable directory sync; 4 GiB file limit on FAT32 |

## Linux

- `fdatasync` persists data and the metadata needed to read it back,
  including a changed file size, so it is enough for WAL appends.
- Errors from writeback are reported to each open file description once;
  the poison rule above covers this.
- `std::fs::File::open` on a directory works, and `sync_all` on it syncs
  the directory entry.

## macOS

- `fsync(2)` on macOS flushes to the drive but not out of the drive's
  cache; `fcntl(F_FULLFSYNC)` does. M1.2 records which one `std`'s
  `sync_all` and `sync_data` use for the pinned toolchain. If neither does
  `F_FULLFSYNC`, the durability promise needs an ADR under ADR-0013 to
  call it ourselves.
- APFS is case-insensitive by default. The WAL path is the database path
  plus `-wal`, derived textually, so case does not matter.

## Windows

- Paths longer than 260 characters: `std` adds the `\\?\` prefix for many
  file functions **verify (M1.2)**. Tests include a long path.
- `std`'s default share mode allows other handles to read, write and
  delete **verify (M1.2)**; the exclusive lock (ADR-0005) is what keeps
  other hardshellDB processes out.
- Line endings: committed text fixtures (SLT files, golden outputs) are read
  byte for byte; CI turns `core.autocrlf` off before checkout.

## Cross-platform invariants (tested on every OS)

- The VFS contract suite (M1.1) passes against `StdVfs` on each OS.
- A database file written on one OS opens on the others (fixture tests,
  M11.1).
- The process-kill test (M2.7) passes on each OS.
- `TIMESTAMPTZ` and `now()` read the clock through `std::time::SystemTime`
  only; no local time zone is consulted (session `TimeZone` defaults to
  UTC).
