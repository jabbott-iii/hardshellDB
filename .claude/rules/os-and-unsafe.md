---
paths:
  - "crates/os/**"
---

# Rules for the OS layer and `unsafe`

`hardshell-os` is the only crate that may ever contain `unsafe`, and today
it contains none (ADR-0013). The one exception outside it is a counting
allocator in a crate's `tests/` (ADR-0017).

The `unsafe` section of `rust.md` in this folder has the general rules:
one operation per block with its `// SAFETY:` comment, a `# Safety`
section on every `pub unsafe fn`, `unsafe extern` blocks that name their
header, layout asserts for `#[repr(C)]` types, and RAII wrappers that
document who frees each handle. Its "Errors and panics" section covers
adding the path and operation to errors, and commenting a `Result` that is
ignored on purpose. This file adds the OS layer's own rules.

- **`std` first.** Positional I/O, `sync_data`/`sync_all`, `set_len`,
  `File::lock`/`try_lock`/`lock_shared`, directory opening and temp
  directories all exist in `std`. Use them. `OpenOptionsExt::custom_flags`
  and `share_mode` are safe ways to pass OS flags.
- **`cfg(target_os)` lives only here.** Other crates see the same `Vfs`
  API on every OS.
- **Before writing any `unsafe` or `extern` block, stop.** Write an ADR
  (`/record-decision`, status `Proposed`) naming the OS call, why `std`
  cannot do it, the safe wrapper's API, and how it is tested on each OS.
  Wait for the user to accept it.
- **Once an ADR allows it:**
  - Raw declarations live in `hardshell-os::sys::<os>` (`unsafe extern "C"`,
    or `unsafe extern "system"` on Windows), each with a `// source:`
    comment naming the header or documentation.
  - Add the ADR's number to `OS_UNSAFE_ADRS` in `xtask/src/unsafe_audit.rs`.
    Until then `cargo xtask unsafe-audit` fails on any `unsafe` here.
  - Doc examples stay free of `unsafe`: the crate root's
    `#![doc(test(attr(forbid(unsafe_code))))]` stays, and the ADR says so
    if an example ever needs it.
  - Run `cargo xtask unsafe-audit` before reporting done.
- **Sync semantics are part of the contract.** Any change to how a file or
  directory is synced, locked or opened updates `docs/03-platforms.md` with
  the OS call made and its source, and runs the VFS contract suite and the
  process-kill test on every OS you can; name the ones you could not.
- **The `FaultVfs`** must model at least what the real OS can do to unsynced
  data (`docs/04-quality.md`). When you learn of a new failure mode, add it
  there first.
