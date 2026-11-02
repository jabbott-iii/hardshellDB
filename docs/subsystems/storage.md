# Subsystem: `hardshell-storage` — file format, pager, B+trees

The storage engine below SQL: the database file's format, the pager and
buffer pool, the free list, B+trees and overflow pages. It stores and
compares byte strings only; SQL types become bytes in `hardshell-types`
(ADR-0006). The WAL, snapshots and transactions, which live in the same
crate, are in `wal-and-transactions.md`. Nothing is built yet; M1 builds
the file format and pager, M2 the WAL, M3 the B+trees.

## Contents

| Module | Provides |
| :- | :- |
| `format` | The file header (page 0), page header and trailer, page types, format version and feature flags |
| `page` | `PageNo`, `PageBuf` (one page, aligned `Box<[u8]>`), checksum compute and verify |
| `pager` | Reads pages through the WAL index or the main file, hands out pinned page guards, tracks the writer's dirty pages |
| `cache` | The buffer pool: fixed frames, clock-sweep eviction, pin counts, keyed by page version |
| `freelist` | Trunk pages listing free page numbers; allocate and free within a write transaction |
| `btree` | B+trees over byte keys: create, seek, cursors, insert, delete, split, merge, drop |
| `overflow` | Chains of overflow pages for values that do not fit in a leaf cell |
| `check` | The integrity checker behind `CHECK DATABASE` (M11) |
| `crash` | `crash_point!("area.event")` (feature `testing`): named points where the crash campaign can stop the engine |

## File format

All integers are little-endian. Page `n` lives at byte offset
`n × page_size`.

**Page 0, the header:**

| Offset | Size | Field |
| :- | :- | :- |
| 0 | 16 | Magic `b"hardshellDB\0fmt\0"` |
| 16 | 4 | Format version: `u16` major, `u16` minor. Major `0` before M11's freeze (each change bumps the minor, old ones are refused); M11 writes major `1` |
| 20 | 4 | Page size in bytes |
| 24 | 4 | Page count (file size in pages, including page 0) |
| 28 | 4 | First free-list trunk page (0 = none) |
| 32 | 4 | Free page count |
| 36 | 8 | Schema version (bumped by every DDL commit) |
| 44 | 8 | Change counter (bumped by every commit) |
| 52 | 4 × 3 | Feature flags: compatible, read-only-compatible, incompatible |
| 64 | 4 × 16 | Catalog roots: root pages of `hs_tables`, `hs_columns`, `hs_indexes`, `hs_constraints`, `hs_views`, `hs_sequences`, `hs_stats`, and spare slots (0 = unused) |
| 128 | … | Reserved, zero |
| `page_size − 4` | 4 | CRC-32C of bytes `0 .. page_size − 4` |

**Every other page:**

| Offset | Size | Field |
| :- | :- | :- |
| 0 | 1 | Page type: `1` table interior, `2` table leaf, `3` index interior, `4` index leaf, `5` overflow, `6` free-list trunk, `7` free (unused) |
| 1 | 1 | Flags |
| 2 | 2 | Cell count |
| 4 | 2 | Start of cell content area |
| 6 | 2 | Fragmented free bytes |
| 8 | 4 | Right-most child (interior) or right sibling (leaf; 0 = none) |
| 12 | 4 | This page's own number (catches misdirected writes) |
| 16 | … | Cell pointer array (2 bytes each), free space, cells growing down from the end |
| `page_size − 4` | 4 | CRC-32C of bytes `0 .. page_size − 4` (ADR-0009) |

- A reader refuses a file whose magic, version or page size it does not
  know, or whose *incompatible* feature flags it does not understand
  (`XX001` with a detail naming the field). Unknown *read-only-compatible*
  flags allow read-only opens only.
- Free pages (type 7) still carry a valid checksum, so a full-file scan
  can verify every page.

## Key types

```rust
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PageNo(u32);                    // 0 is the header; never a tree page

pub enum PageVersion {
    Main,                                  // the main file's copy
    Frame { generation: u32, frame: u64 }, // a committed WAL frame
    Dirty,                                 // the writer's private copy
}

pub struct Pager {
    file: Box<dyn VfsFile>,
    cache: Cache,
    wal: Wal,                              // wal-and-transactions.md
    page_size: u32,
}

pub struct BTree { root: PageNo, kind: TreeKind }
pub enum TreeKind { Table, Index }         // table keys: i64 row ids; index keys: bytes, empty values

impl BTree {
    pub fn create(tx: &mut WriteTxn, kind: TreeKind) -> Result<BTree, StorageError>;
    pub fn open(root: PageNo, kind: TreeKind) -> BTree;
    pub fn cursor<'t>(&self, view: &'t dyn ReadView) -> Cursor<'t>;
    pub fn insert(&self, tx: &mut WriteTxn, key: &[u8], value: &[u8]) -> Result<Option<Replaced>, StorageError>;
    pub fn delete(&self, tx: &mut WriteTxn, key: &[u8]) -> Result<bool, StorageError>;
    pub fn last_key(&self, view: &dyn ReadView) -> Result<Option<Vec<u8>>, StorageError>;
    pub fn drop_tree(self, tx: &mut WriteTxn) -> Result<(), StorageError>;  // frees every page
}

pub struct Cursor<'t> { /* stack of (PageNo, cell index), pinned leaf */ }

impl Cursor<'_> {
    pub fn seek(&mut self, key: &[u8], op: SeekOp) -> Result<bool, StorageError>; // Eq, Ge, Gt, Le, Lt
    pub fn first(&mut self) -> Result<bool, StorageError>;
    pub fn next(&mut self) -> Result<bool, StorageError>;
    pub fn prev(&mut self) -> Result<bool, StorageError>;
    pub fn key(&self) -> &[u8];
    pub fn value(&mut self) -> Result<ValueBytes<'_>, StorageError>;     // reads overflow lazily
}
```

## Buffer pool

- A fixed array of frames sized by `cache_size` (default 64 MiB), never
  exceeded. Each frame holds one page version, keyed by `(PageNo,
  PageVersion)`: snapshot readers can need different versions of one page
  at once.
- Eviction is clock-sweep over unpinned frames. A pinned frame (used by a
  cursor or the writer) is never evicted.
- The writer's dirty pages can only leave the cache by being written to the
  WAL as uncommitted frames (ADR-0007); after that they are re-read from the
  WAL if needed. If every frame is pinned, the operation fails with
  `53200` and a hint to raise `cache_size`.
- When a checkpoint copies a page into the main file, cached `Main` frames
  of that page are dropped (`wal-and-transactions.md` explains why no live
  reader needs them).
- Every page read from the main file or the WAL has its checksum and its
  own page number verified before it enters the cache.

## B+tree rules

- A tree's root page number never changes: a root split moves the root's
  contents into a new child. The catalog stores roots once.
- Table leaves hold `(row id, row bytes)`; a row larger than the inline
  limit (about a quarter of a page) keeps a prefix inline and the rest in
  an overflow chain.
- Index keys must fit inline: a key larger than the index key limit (about
  a quarter of a page minus cell overhead, about 1,000 bytes at 4 KiB) is
  `54000`, as PostgreSQL does for oversized index rows.
- Interior pages hold separator keys, truncated to the shortest prefix
  that separates their children (suffix truncation).
- Deletion merges a page that falls below a quarter full with a sibling, or
  borrows from one; empty pages go to the free list.
- Every structural change (split, merge, root move) happens inside the write
  transaction, so a crash mid-change leaves the old tree (ADR-0007).

## Invariants

- Keys within a page, and across a level from left to right, are strictly
  increasing by `memcmp`; every key in a subtree lies within its parent's
  separators.
- Every page in `1 .. page_count` is reachable exactly once: from one tree
  (as a tree or overflow page), from the free list, or as a free-list
  trunk. `CHECK DATABASE` verifies this (M11).
- Nothing read from disk is trusted: cell offsets, lengths, child pointers
  and overflow lengths are bounds-checked against the page and the file;
  a cycle in an overflow chain or a tree deeper than 64 levels is
  `XX001`.
- No B+tree operation panics on any page contents.

## Limits

| Limit | Value | When exceeded |
| :- | :- | :- |
| Page size | 4096–65536, power of two, fixed at creation | `22023` at creation |
| Database size | 2^32 − 1 pages (16 TiB at 4 KiB) | `54000` |
| Value size | 1 GiB (ADR-0006) | `54000` |
| Index key | ≈ page size / 4 | `54000` |
| Tree depth | 64 | `XX001` (cannot happen in a valid file) |

## Tests

- Model-based tests: random sequences of insert, delete, seek and scan on
  a `BTree` and a `std::collections::BTreeMap`, compared after every step,
  at every page size; seeded through `testing::check` (`HARDSHELL_SEED`).
- Property tests: tree invariants hold after every operation; overflow
  values of every length around the inline limit round-trip.
- Fuzz targets `page` (decode and walk arbitrary page bytes) and `open`
  (open an arbitrary file on `MemVfs`, then scan every tree it claims).
- Crash tests: tree operations under `FaultVfs` with crashes at every I/O
  op, through the M2 crash campaign.
- Format fixtures: `tests/fixtures/` holds files written by each format
  version; a test opens each and checks its contents.

## Open questions

- Prefix compression of keys within a page: backlog, behind a benchmark.
- Clustering a table by its primary key: ADR-0006's revisit trigger.
